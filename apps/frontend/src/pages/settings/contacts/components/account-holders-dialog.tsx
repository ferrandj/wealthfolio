import {
  addContactIdentifier,
  createContact,
  linkContactAccount,
  listAccountIdentities,
  listContacts,
  saveAccountIdentity,
  setContactAccountShare,
} from "@/adapters";
import { useAccounts } from "@/hooks/use-accounts";
import { shouldInvalidateAfterPortfolioUpdate } from "@/lib/query-invalidation";
import { QueryKeys } from "@/lib/query-keys";
import type { ContactSummary } from "@/lib/types/contacts";
import type { AccountIdentity } from "@/lib/types/statements";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
  Icons,
  Input,
  Label,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@wealthfolio/ui";
import { toast } from "@wealthfolio/ui/components/ui/use-toast";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import {
  emptyForm,
  formatIban,
  formToInput,
  parsePrintedName,
  suggestContact,
} from "../contacts-utils";

const NEW = "__new__";

/** One holder of the account: a contact that exists, or a new one named as the bank printed it. */
export interface HolderRow {
  key: string;
  choice: string; // a contact id, or NEW
  name: string; // the name of a new contact
}

/** The holders of an account, each matched to a known contact when one bears the printed name. */
export function holderRows(names: string[], contacts: ContactSummary[]): HolderRow[] {
  const taken = new Set<string>();
  // nothing printed (a life insurance, a PEA…): the person themselves, when known
  if (names.length === 0) {
    const self = contacts.find((c) => c.contact.kind === "self");
    return [{ key: "h0", choice: self?.contact.id ?? NEW, name: "" }];
  }
  const rows = names.map((name, i) => {
    const found = name
      ? suggestContact(
          contacts.filter((c) => !taken.has(c.contact.id)),
          name,
        )
      : null;
    if (found) taken.add(found);
    return { key: `h${i}`, choice: found ?? NEW, name };
  });
  return rows;
}

/** "M FERRAND JEREMIE et MME CHACON KARLA": the name of a joint account is the names of its holders. */
export function jointName(names: string[]): string {
  const kept = names.map((n) => n.trim()).filter(Boolean);
  if (kept.length <= 1) return kept[0] ?? "";
  return `${kept.slice(0, -1).join(", ")} et ${kept[kept.length - 1]}`;
}

/**
 * Who holds an account of Wealthfolio, on one screen: every holder the documents print (two or more for a joint
 * account), each one a contact that exists or a new one, and more can be added. Only a current account asks for its
 * IBAN and BIC. Each holder gets an equal share of the account.
 */
export function AccountHoldersDialog({
  accountIds,
  onClose,
}: {
  accountIds: string[];
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const [index, setIndex] = useState(0);
  const contacts = useQuery({ queryKey: [QueryKeys.CONTACTS], queryFn: listContacts });
  const identities = useQuery({
    queryKey: [QueryKeys.ACCOUNT_IDENTITIES],
    queryFn: listAccountIdentities,
  });
  const { accounts, isLoading } = useAccounts({ filterActive: false, includeArchived: true });
  const accountId = accountIds[index];
  if (!accountId) return null;
  const ready = !!contacts.data && !!identities.data && !isLoading;
  const next = () => (index + 1 >= accountIds.length ? onClose() : setIndex(index + 1));
  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="max-h-[90vh] overflow-y-auto sm:max-w-2xl">
        <DialogTitle>{t("settings:contacts.holders_title")}</DialogTitle>
        <DialogDescription>
          {t("settings:contacts.wizard_progress", { current: index + 1, total: accountIds.length })}
        </DialogDescription>
        {ready && (
          <HoldersForm
            key={accountId}
            accountId={accountId}
            accountName={accounts.find((a) => a.id === accountId)?.name ?? ""}
            accountType={accounts.find((a) => a.id === accountId)?.accountType}
            identity={identities.data?.find((i) => i.accountId === accountId)}
            contacts={contacts.data ?? []}
            onDone={next}
          />
        )}
      </DialogContent>
    </Dialog>
  );
}

function HoldersForm({
  accountId,
  accountName,
  accountType,
  identity,
  contacts,
  onDone,
}: {
  accountId: string;
  accountName: string;
  accountType?: string;
  identity?: AccountIdentity;
  contacts: ContactSummary[];
  onDone: () => void;
}) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const printed = identity?.holders?.length
    ? identity.holders
    : identity?.holderName
      ? [identity.holderName]
      : [];
  const [rows, setRows] = useState<HolderRow[]>(() => holderRows(printed, contacts));
  const bank = accountType === "CASH";
  const [iban, setIban] = useState(formatIban(identity?.iban ?? ""));
  const [bic, setBic] = useState(identity?.bic ?? "");
  const [institution, setInstitution] = useState(identity?.institution ?? "");
  const nameOf = (row: HolderRow) =>
    row.choice === NEW
      ? row.name.trim()
      : (contacts.find((c) => c.contact.id === row.choice)?.contact.displayName ?? "");
  const [label, setLabel] = useState<string | null>(null);
  const shownLabel =
    label ?? (jointName(printed.length > 1 ? printed : rows.map(nameOf)) || accountName);
  const hasSelf = contacts.some((c) => c.contact.kind === "self");

  const save = useMutation({
    mutationFn: async () => {
      const compactIban = bank ? iban.replace(/\s+/g, "").toUpperCase() : "";
      const ids: string[] = [];
      for (const [i, row] of rows.entries()) {
        let id = row.choice;
        if (id === NEW) {
          const parsed = parsePrintedName(row.name);
          // the first holder is the person themselves when nobody is yet
          const kind = !hasSelf && i === 0 ? "self" : "person";
          const form = {
            ...emptyForm(kind),
            civility: parsed.civility,
            firstName: parsed.firstName,
            lastName: parsed.lastName,
          };
          const created = await createContact({
            ...formToInput(form),
            identifiers: row.name.trim() ? [{ kind: "name", value: row.name.trim() }] : [],
            linkedAccountIds: [],
          });
          id = created.contact.id;
        }
        if (ids.includes(id)) continue;
        ids.push(id);
        await linkContactAccount(id, accountId);
        if (compactIban) {
          await addContactIdentifier(id, {
            kind: "iban",
            value: compactIban,
            label: shownLabel || null,
            bic: bic.trim() || null,
            institution: institution.trim() || null,
          });
        }
      }
      // a joint account: each holder gets an equal part (it can be changed on the contact's card)
      if (ids.length > 1) {
        for (const id of ids) {
          await setContactAccountShare(id, accountId, Math.round(10000 / ids.length) / 100);
        }
      }
      await saveAccountIdentity(accountId, {
        iban: compactIban || identity?.iban || null,
        bic: bank ? bic.trim() || null : (identity?.bic ?? null),
        institution: institution.trim() || null,
        holderName: shownLabel || null,
      });
    },
    onSuccess: () => {
      for (const key of [
        QueryKeys.CONTACTS,
        QueryKeys.CONTACT,
        QueryKeys.ACCOUNT_IDENTITIES,
        QueryKeys.PERSON_OVERVIEW,
        QueryKeys.UNRESOLVED_COUNTERPARTIES,
        QueryKeys.MISSING_OWN_ACCOUNTS,
      ]) {
        queryClient.invalidateQueries({ queryKey: [key] });
      }
      queryClient.invalidateQueries({
        predicate: (query) => shouldInvalidateAfterPortfolioUpdate(query.queryKey),
      });
      onDone();
    },
    onError: (e) =>
      toast({
        title: t("settings:contacts.error"),
        description: String(e),
        variant: "destructive",
      }),
  });

  const items = [
    ...contacts.map((c) => ({
      value: c.contact.id,
      label: `${c.contact.displayName} · ${t(`settings:contacts.kind_${c.contact.kind}`)}`,
    })),
    { value: NEW, label: t("settings:contacts.holders_new") },
  ];
  const patch = (key: string, p: Partial<HolderRow>) =>
    setRows((rs) => rs.map((r) => (r.key === key ? { ...r, ...p } : r)));
  const complete = rows.length > 0 && rows.every((r) => r.choice !== NEW || r.name.trim());

  return (
    <div className="grid gap-5 text-sm">
      <div className="grid gap-1 rounded-md border p-3">
        <span className="font-medium">{accountName}</span>
        {identity?.iban && (
          <code className="text-muted-foreground text-xs">{formatIban(identity.iban)}</code>
        )}
      </div>

      <section className="grid gap-3">
        <div>
          <h3 className="text-sm font-semibold">
            {t("settings:contacts.holders_section", { count: rows.length })}
          </h3>
          <p className="text-muted-foreground text-xs">
            {printed.length > 1
              ? t("settings:contacts.holders_joint_hint", { count: printed.length })
              : t("settings:contacts.holders_hint")}
          </p>
        </div>
        {rows.map((row, i) => (
          <div key={row.key} className="grid gap-2 rounded-md border p-3" data-testid="holder-row">
            <div className="flex items-center justify-between gap-2">
              <span className="text-muted-foreground text-xs">
                {t("settings:contacts.holders_row", { n: i + 1 })}
                {printed[i]
                  ? ` · ${t("settings:contacts.holders_printed", { name: printed[i] })}`
                  : ""}
              </span>
              {rows.length > 1 && (
                <Button
                  size="sm"
                  variant="ghost"
                  aria-label={t("settings:contacts.holders_remove")}
                  onClick={() => setRows((rs) => rs.filter((r) => r.key !== row.key))}
                >
                  <Icons.Close className="size-4" />
                </Button>
              )}
            </div>
            <Select
              items={items}
              value={row.choice}
              onValueChange={(v) => v && patch(row.key, { choice: v })}
            >
              <SelectTrigger aria-label={t("settings:contacts.holders_row", { n: i + 1 })}>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {items.map((item) => (
                  <SelectItem key={item.value} value={item.value}>
                    {item.label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            {row.choice === NEW && (
              <Input
                value={row.name}
                placeholder={t("settings:contacts.holders_new_name")}
                onChange={(e) => patch(row.key, { name: e.target.value })}
              />
            )}
          </div>
        ))}
        <Button
          size="sm"
          variant="outline"
          className="justify-self-start"
          onClick={() => setRows((rs) => [...rs, { key: `h${Date.now()}`, choice: NEW, name: "" }])}
        >
          <Icons.Plus className="me-1 size-4" />
          {t("settings:contacts.holders_add")}
        </Button>
      </section>

      <section className="grid gap-3">
        <h3 className="text-sm font-semibold">{t("settings:contacts.wizard_step_account")}</h3>
        <div className="grid gap-3 sm:grid-cols-2">
          {bank && (
            <>
              <div className="grid gap-1.5 sm:col-span-2">
                <Label>{t("settings:contacts.wizard_field_iban")}</Label>
                <Input
                  className="font-mono"
                  value={iban}
                  onChange={(e) => setIban(e.target.value)}
                />
              </div>
              <div className="grid gap-1.5">
                <Label>{t("settings:contacts.wizard_field_bic")}</Label>
                <Input className="font-mono" value={bic} onChange={(e) => setBic(e.target.value)} />
              </div>
            </>
          )}
          <div className="grid gap-1.5">
            <Label>{t("settings:contacts.wizard_field_institution")}</Label>
            <Input value={institution} onChange={(e) => setInstitution(e.target.value)} />
          </div>
          <div className="grid gap-1.5 sm:col-span-2">
            <Label>{t("settings:contacts.wizard_field_label")}</Label>
            <Input value={shownLabel} onChange={(e) => setLabel(e.target.value)} />
          </div>
        </div>
      </section>

      <div className="flex justify-end gap-2">
        <Button variant="ghost" onClick={onDone}>
          {t("settings:contacts.holders_later")}
        </Button>
        <Button disabled={!complete || save.isPending} onClick={() => save.mutate()}>
          {t("settings:contacts.holders_save", { count: rows.length })}
        </Button>
      </div>
    </div>
  );
}

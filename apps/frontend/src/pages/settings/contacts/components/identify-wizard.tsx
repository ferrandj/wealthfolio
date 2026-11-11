import {
  addContactIdentifier,
  createAccount,
  createContact,
  linkContactAccount,
  listAccountIdentities,
  listContacts,
  saveAccountIdentity,
  setContactAccountShare,
  updateContact,
} from "@/adapters";
import { useAccounts } from "@/hooks/use-accounts";
import { useBalancePrivacy } from "@/hooks/use-balance-privacy";
import { shouldInvalidateAfterPortfolioUpdate } from "@/lib/query-invalidation";
import { maskIban, maskText } from "@/lib/privacy-mask";
import { QueryKeys } from "@/lib/query-keys";
import type {
  ContactKind,
  NewContactIdentifier,
  UnresolvedCounterparty,
} from "@/lib/types/contacts";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Badge,
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
  Switch,
} from "@wealthfolio/ui";
import { toast } from "@wealthfolio/ui/components/ui/use-toast";
import { useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  contactToForm,
  emptyForm,
  formatIban,
  formToInput,
  isFormComplete,
  parsePrintedName,
  sameName,
  suggestContact,
  toNumber,
  type ContactForm as ContactFormValue,
} from "../contacts-utils";
import { ContactForm } from "./contact-form";

type Step = "owner" | "contact" | "account";
type Owner = { mode: "existing"; contactId: string } | { mode: "new"; kind: ContactKind };

/** The kinds a new owner can be: the "other" one asks for no details. */
const NEW_KINDS: ContactKind[] = ["person", "organization", "self", "other"];

const message = (e: unknown) =>
  e && typeof e === "object" && "message" in e
    ? String((e as { message: unknown }).message)
    : String(e);

interface AccountFields {
  iban: string;
  bic: string;
  institution: string;
  label: string;
}

/** What is identified: a counterparty, or an account of Wealthfolio that one of its holders is asked for. */
export type IdentifyItemData = UnresolvedCounterparty & {
  /** Holders of the account when it is joint: each one gets an equal share. */
  holderCount?: number;
};

interface WizardProps {
  /** The accounts to identify, one after the other. */
  items: IdentifyItemData[];
  onClose: () => void;
}

/**
 * Identifying accounts that no contact recognises yet, one at a time: who owns it (an existing contact or a new
 * one), the card of that contact, then the account itself (IBAN, BIC...). An account of mine can be created in
 * Wealthfolio on the way; an account of someone else is only remembered, not tracked.
 */
export function IdentifyWizard({ items, onClose }: WizardProps) {
  const { t } = useTranslation();
  const [index, setIndex] = useState(0);
  const [done, setDone] = useState(0);
  const item = items[index];

  const next = (identified: boolean) => {
    const count = done + (identified ? 1 : 0);
    setDone(count);
    if (index + 1 >= items.length) {
      if (count > 0) toast({ title: t("settings:contacts.wizard_done", { count }) });
      onClose();
    } else {
      setIndex(index + 1);
    }
  };

  if (!item) return null;
  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="max-h-[90vh] overflow-y-auto sm:max-w-2xl">
        <DialogTitle>{t("settings:contacts.wizard_title")}</DialogTitle>
        <DialogDescription>
          {t("settings:contacts.wizard_progress", { current: index + 1, total: items.length })}
        </DialogDescription>
        <IdentifyItem
          key={item.key}
          item={item}
          onIdentified={() => next(true)}
          onSkip={() => next(false)}
        />
      </DialogContent>
    </Dialog>
  );
}

function IdentifyItem({
  item,
  onIdentified,
  onSkip,
}: {
  item: IdentifyItemData;
  onIdentified: () => void;
  onSkip: () => void;
}) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const { isBalanceHidden: hidden } = useBalancePrivacy();
  const { accounts } = useAccounts({ filterActive: false, includeArchived: true });
  const contacts = useQuery({ queryKey: [QueryKeys.CONTACTS], queryFn: listContacts });
  const identities = useQuery({
    queryKey: [QueryKeys.ACCOUNT_IDENTITIES],
    queryFn: listAccountIdentities,
  });

  const printed = item.name ?? "";
  // the holder of one of my accounts: the account is probably mine too
  const looksMine =
    !!item.ownAccountId ||
    (identities.data ?? []).some((i) => i.holderName && sameName(i.holderName, printed));
  const list = contacts.data ?? [];
  const suggestedId = suggestContact(list, printed);

  const [step, setStep] = useState<Step>("owner");
  const [choice, setChoice] = useState<Owner | null>(null);
  const owner: Owner =
    choice ??
    (suggestedId
      ? { mode: "existing", contactId: suggestedId }
      : { mode: "new", kind: looksMine ? "self" : "person" });
  const [form, setForm] = useState<ContactFormValue>(emptyForm());
  const [fields, setFields] = useState<AccountFields>({
    iban: formatIban(item.iban ?? ""),
    bic: "",
    institution: "",
    label: printed,
  });
  const [createInWealthfolio, setCreateInWealthfolio] = useState(true);
  const [accountName, setAccountName] = useState("");

  const existing =
    owner.mode === "existing" ? list.find((c) => c.contact.id === owner.contactId) : null;
  const ownerKind: ContactKind | null =
    owner.mode === "new" ? owner.kind : (existing?.contact.kind ?? null);
  const ownerName =
    owner.mode === "existing"
      ? (existing?.contact.displayName ?? "")
      : form.displayName || [form.firstName, form.lastName].filter(Boolean).join(" ") || printed;
  // an account of mine that is not in Wealthfolio yet can be created there
  const canCreate = ownerKind === "self" && !item.ownAccountId;
  const ownAccount = item.ownAccountId
    ? accounts.find((a) => a.id === item.ownAccountId)
    : undefined;
  const currency = accounts.find((a) => item.accountIds.includes(a.id))?.currency ?? "EUR";

  /** The card of a new contact, prefilled with what the bank printed (or what an account of mine already says). */
  const prefill = (kind: ContactKind): ContactFormValue => {
    const base = emptyForm(kind);
    if (kind === "person" || kind === "self") {
      const own =
        identities.data?.find((i) => i.accountId === item.ownAccountId) ??
        (kind === "self" ? identities.data?.find((i) => i.holderName) : undefined);
      const parsed = parsePrintedName(
        kind === "self" && own?.holderName ? own.holderName : printed,
      );
      return {
        ...base,
        civility: parsed.civility,
        lastName: parsed.lastName,
        firstName: parsed.firstName,
        street: kind === "self" ? (own?.holderStreet ?? "") : "",
        postalCode: kind === "self" ? (own?.holderPostalCode ?? "") : "",
        city: kind === "self" ? (own?.holderCity ?? "") : "",
      };
    }
    return { ...base, displayName: printed };
  };

  const enterAccount = () => {
    setAccountName((n) => n || ownerName || printed);
    setStep("account");
  };

  const goFromOwner = () => {
    const own = identities.data?.find((i) => i.accountId === item.ownAccountId);
    setFields((f) => ({
      ...f,
      bic: f.bic || own?.bic || "",
      institution: f.institution || own?.institution || "",
    }));
    if (owner.mode === "existing") {
      // the card of a contact that exists is shown filled in: validated as it is, or modified
      if (existing) setForm(contactToForm(existing.contact));
      setStep("contact");
    } else if (owner.kind === "other") {
      enterAccount();
    } else {
      setForm(prefill(owner.kind));
      setStep("contact");
    }
  };

  const refresh = () => {
    for (const key of [
      QueryKeys.CONTACTS,
      QueryKeys.CONTACT,
      QueryKeys.UNRESOLVED_COUNTERPARTIES,
      QueryKeys.MISSING_OWN_ACCOUNTS,
      QueryKeys.ACCOUNT_IDENTITIES,
      QueryKeys.PERSON_OVERVIEW,
    ]) {
      queryClient.invalidateQueries({ queryKey: [key] });
    }
    queryClient.invalidateQueries({
      predicate: (query) => shouldInvalidateAfterPortfolioUpdate(query.queryKey),
    });
  };

  const done = useRef<{
    contactId: string | null;
    accountId: string | null;
    linked: boolean;
    shared: boolean;
  }>({ contactId: null, accountId: null, linked: false, shared: false });
  const validate = useMutation({
    mutationFn: async () => {
      const identifiers: NewContactIdentifier[] = [];
      const iban = fields.iban.replace(/\s+/g, "").toUpperCase();
      if (iban) {
        identifiers.push({
          kind: "iban",
          value: iban,
          label: fields.label.trim() || printed || null,
          bic: fields.bic.trim() || null,
          institution: fields.institution.trim() || null,
        });
      }
      if (printed && (iban || item.kind === "name")) {
        identifiers.push({ kind: "name", value: printed });
      }
      // a step that already went through is not done twice when the person retries after an error
      let contactId = done.current.contactId;
      if (!contactId) {
        if (owner.mode === "existing") {
          contactId = owner.contactId;
          const changed =
            !!existing &&
            JSON.stringify(formToInput(form)) !==
              JSON.stringify(formToInput(contactToForm(existing.contact)));
          if (changed) await updateContact(contactId, formToInput(form));
          for (const identifier of identifiers) await addContactIdentifier(contactId, identifier);
        } else {
          const input =
            owner.kind === "other" ? { kind: owner.kind, displayName: printed } : formToInput(form);
          const created = await createContact({ ...input, identifiers, linkedAccountIds: [] });
          contactId = created.contact.id;
        }
        done.current.contactId = contactId;
      }
      // an account that is already in Wealthfolio is attached to its owner
      if (item.ownAccountId && !done.current.linked) {
        await linkContactAccount(contactId, item.ownAccountId);
        done.current.linked = true;
      }
      // a joint account: each holder gets an equal part (it can be changed on the contact's card)
      if (item.ownAccountId && item.holderCount && item.holderCount > 1 && !done.current.shared) {
        await setContactAccountShare(
          contactId,
          item.ownAccountId,
          Math.round(10000 / item.holderCount) / 100,
        );
        done.current.shared = true;
      }
      if (canCreate && createInWealthfolio && !done.current.accountId) {
        const account = await createAccount({
          name: accountName.trim() || ownerName || printed,
          accountType: "CASH",
          group: "Cash",
          currency,
          isDefault: false,
          isActive: true,
          isArchived: false,
          trackingMode: "TRANSACTIONS",
          accountNumber: iban || undefined,
        });
        done.current.accountId = account.id;
        await saveAccountIdentity(account.id, {
          iban: iban || null,
          bic: fields.bic.trim() || null,
          institution: fields.institution.trim() || null,
          holderName: ownerName || null,
        });
        await linkContactAccount(contactId, account.id);
      }
    },
    onSuccess: () => {
      refresh();
      onIdentified();
    },
    onError: (e) =>
      toast({
        title: t("settings:contacts.error"),
        description: message(e),
        variant: "destructive",
      }),
  });

  const setField = (key: keyof AccountFields, value: string) =>
    setFields((f) => ({ ...f, [key]: value }));
  const kindLabel = (kind: ContactKind) => t(`settings:contacts.kind_${kind}`);

  return (
    <div className="grid gap-5">
      <div className="grid gap-1 rounded-md border p-3">
        <span className="font-medium break-words">
          {printed || ownAccount?.name
            ? hidden
              ? maskText(printed || ownAccount?.name || "")
              : printed || ownAccount?.name
            : hidden
              ? maskIban(item.iban ?? "")
              : formatIban(item.iban ?? "")}
        </span>
        {item.iban && printed ? (
          <code className="text-muted-foreground text-xs">
            {hidden ? maskIban(item.iban) : formatIban(item.iban)}
          </code>
        ) : null}
        <div className="flex flex-wrap items-center gap-2 text-xs">
          {item.count > 0 && (
            <Badge variant="secondary">
              {t("settings:contacts.operations_label", { count: item.count })}
            </Badge>
          )}
          {toNumber(item.totalOut) > 0 && (
            <span className="text-muted-foreground">
              {t("settings:contacts.total_out")} {toNumber(item.totalOut).toFixed(2)}
            </span>
          )}
          {toNumber(item.totalIn) > 0 && (
            <span className="text-success">
              {t("settings:contacts.total_in")} {toNumber(item.totalIn).toFixed(2)}
            </span>
          )}
        </div>
        {ownAccount && (
          <span className="text-muted-foreground text-xs">
            {t("settings:contacts.wizard_own_known", { name: ownAccount.name })}
          </span>
        )}
      </div>

      {step === "owner" && (
        <section className="grid gap-4">
          <h3 className="text-sm font-semibold">{t("settings:contacts.wizard_step_owner")}</h3>
          {list.length > 0 && (
            <div className="grid gap-1.5">
              <Label>{t("settings:contacts.wizard_existing")}</Label>
              <Select
                items={list.map((c) => ({ value: c.contact.id, label: c.contact.displayName }))}
                value={owner.mode === "existing" ? owner.contactId : ""}
                onValueChange={(v) => v && setChoice({ mode: "existing", contactId: v })}
              >
                <SelectTrigger aria-label={t("settings:contacts.wizard_existing")}>
                  <SelectValue placeholder={t("settings:contacts.wizard_existing_placeholder")} />
                </SelectTrigger>
                <SelectContent>
                  {list.map((c) => (
                    <SelectItem key={c.contact.id} value={c.contact.id}>
                      {c.contact.displayName} · {kindLabel(c.contact.kind)}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
              {owner.mode === "existing" && owner.contactId === suggestedId && !choice && (
                <p className="text-muted-foreground text-xs">
                  {t("settings:contacts.wizard_suggested")}
                </p>
              )}
            </div>
          )}
          <div className="grid gap-1.5">
            <Label>
              {t(
                list.length > 0
                  ? "settings:contacts.wizard_new"
                  : "settings:contacts.wizard_new_first",
              )}
            </Label>
            <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
              {NEW_KINDS.map((kind) => (
                <Button
                  key={kind}
                  type="button"
                  variant={owner.mode === "new" && owner.kind === kind ? "default" : "outline"}
                  onClick={() => setChoice({ mode: "new", kind })}
                >
                  {kindLabel(kind)}
                </Button>
              ))}
            </div>
            {owner.mode === "new" && owner.kind === "self" && (
              <p className="text-muted-foreground text-xs">
                {t("settings:contacts.wizard_kind_self_hint")}
              </p>
            )}
            {owner.mode === "new" && owner.kind === "other" && (
              <p className="text-muted-foreground text-xs">
                {t("settings:contacts.wizard_kind_other_hint")}
              </p>
            )}
          </div>
          <div className="flex flex-wrap justify-between gap-2">
            <Button type="button" variant="ghost" onClick={onSkip}>
              {t("settings:contacts.wizard_skip")}
            </Button>
            <Button type="button" onClick={goFromOwner}>
              {t("settings:contacts.wizard_next")}
              <Icons.ChevronRight className="ms-1 size-4 rtl:rotate-180" />
            </Button>
          </div>
        </section>
      )}

      {step === "contact" && ownerKind && (
        <section className="grid gap-4">
          <div>
            <h3 className="text-sm font-semibold">
              {t("settings:contacts.wizard_step_contact", { kind: kindLabel(ownerKind) })}
            </h3>
            <p className="text-muted-foreground text-xs">
              {t("settings:contacts.wizard_step_contact_hint")}
            </p>
          </div>
          <ContactForm value={form} onChange={setForm} hideKind />
          <div className="flex flex-wrap justify-between gap-2">
            <Button type="button" variant="ghost" onClick={() => setStep("owner")}>
              <Icons.ChevronLeft className="me-1 size-4 rtl:rotate-180" />
              {t("settings:contacts.wizard_back")}
            </Button>
            <Button type="button" disabled={!isFormComplete(form)} onClick={enterAccount}>
              {t("settings:contacts.wizard_next")}
              <Icons.ChevronRight className="ms-1 size-4 rtl:rotate-180" />
            </Button>
          </div>
        </section>
      )}

      {step === "account" && (
        <section className="grid gap-4">
          <div>
            <h3 className="text-sm font-semibold">{t("settings:contacts.wizard_step_account")}</h3>
            <p className="text-muted-foreground text-xs">
              {t("settings:contacts.wizard_step_account_hint")}
            </p>
          </div>
          <div className="grid gap-4 sm:grid-cols-2">
            <div className="grid gap-1.5 sm:col-span-2">
              <Label htmlFor="wizard-iban">{t("settings:contacts.wizard_field_iban")}</Label>
              <Input
                id="wizard-iban"
                className="font-mono"
                value={fields.iban}
                onChange={(e) => setField("iban", e.target.value)}
              />
            </div>
            <div className="grid gap-1.5">
              <Label htmlFor="wizard-bic">{t("settings:contacts.wizard_field_bic")}</Label>
              <Input
                id="wizard-bic"
                className="font-mono"
                value={fields.bic}
                onChange={(e) => setField("bic", e.target.value)}
              />
            </div>
            <div className="grid gap-1.5">
              <Label htmlFor="wizard-institution">
                {t("settings:contacts.wizard_field_institution")}
              </Label>
              <Input
                id="wizard-institution"
                value={fields.institution}
                onChange={(e) => setField("institution", e.target.value)}
              />
            </div>
            <div className="grid gap-1.5 sm:col-span-2">
              <Label htmlFor="wizard-label">{t("settings:contacts.wizard_field_label")}</Label>
              <Input
                id="wizard-label"
                value={fields.label}
                onChange={(e) => setField("label", e.target.value)}
              />
            </div>
          </div>

          {canCreate ? (
            <div className="grid gap-3 rounded-md border p-3">
              <label className="flex items-start gap-3 text-sm">
                <Switch checked={createInWealthfolio} onCheckedChange={setCreateInWealthfolio} />
                <span className="grid gap-0.5">
                  <span className="font-medium">
                    {t("settings:contacts.wizard_create_account")}
                  </span>
                  <span className="text-muted-foreground text-xs">
                    {t("settings:contacts.wizard_create_account_hint")}
                  </span>
                </span>
              </label>
              {createInWealthfolio && (
                <div className="grid gap-1.5">
                  <Label htmlFor="wizard-account-name">
                    {t("settings:contacts.wizard_account_name", { currency })}
                  </Label>
                  <Input
                    id="wizard-account-name"
                    value={accountName}
                    onChange={(e) => setAccountName(e.target.value)}
                  />
                </div>
              )}
            </div>
          ) : !item.ownAccountId && ownerKind !== "self" ? (
            <p className="text-muted-foreground rounded-md border border-dashed p-3 text-xs">
              {t("settings:contacts.wizard_not_tracked", { name: ownerName || printed })}
            </p>
          ) : null}

          <div className="flex flex-wrap justify-between gap-2">
            <Button
              type="button"
              variant="ghost"
              onClick={() =>
                setStep(owner.mode === "new" && owner.kind === "other" ? "owner" : "contact")
              }
            >
              <Icons.ChevronLeft className="me-1 size-4 rtl:rotate-180" />
              {t("settings:contacts.wizard_back")}
            </Button>
            <Button
              type="button"
              disabled={
                validate.isPending ||
                (canCreate && createInWealthfolio && accountName.trim().length < 2)
              }
              onClick={() => validate.mutate()}
            >
              {t("settings:contacts.wizard_validate")}
            </Button>
          </div>
        </section>
      )}
    </div>
  );
}

import {
  addContactIdentifier,
  createContact,
  deleteContact,
  getContact,
  linkContactAccount,
  listAccountIdentities,
  listContacts,
  mergeContacts,
  removeContactIdentifier,
  setContactAccountShare,
  unlinkContactAccount,
  updateContact,
} from "@/adapters";
import { useAccounts } from "@/hooks/use-accounts";
import { QueryKeys } from "@/lib/query-keys";
import type { AccountIdentity } from "@/lib/types/statements";
import type {
  ContactDetail,
  ContactIdentifierKind,
  NewContact,
  NewContactIdentifier,
} from "@/lib/types/contacts";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
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
  Skeleton,
} from "@wealthfolio/ui";
import { toast } from "@wealthfolio/ui/components/ui/use-toast";
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  contactToForm,
  emptyForm,
  formatIban,
  formToInput,
  isFormComplete,
  toNumber,
  type ContactForm as ContactFormValue,
} from "../contacts-utils";
import { ContactForm } from "./contact-form";
import { PersonOverviewSection } from "./person-overview";

const message = (e: unknown) =>
  e && typeof e === "object" && "message" in e
    ? String((e as { message: unknown }).message)
    : String(e);

function money(value: string | number, currency: string): string {
  const n = toNumber(value);
  try {
    return new Intl.NumberFormat(undefined, { style: "currency", currency }).format(n);
  } catch {
    return n.toFixed(2);
  }
}

/** The share (percent) a person holds of an account: 100 unless it is a joint account. */
function ShareField({
  value,
  onSave,
}: {
  value: number | string;
  onSave: (share: number) => void;
}) {
  const { t } = useTranslation();
  const [text, setText] = useState(String(toNumber(value)));
  useEffect(() => setText(String(toNumber(value))), [value]);
  const commit = () => {
    const share = Number(text.replace(",", "."));
    if (Number.isFinite(share) && share >= 0 && share <= 100 && share !== toNumber(value)) {
      onSave(share);
    } else {
      setText(String(toNumber(value)));
    }
  };
  return (
    <span className="ml-auto flex items-center gap-1">
      <Input
        className="w-20 text-right"
        inputMode="decimal"
        value={text}
        aria-label={t("settings:contacts.share_label")}
        onChange={(e) => setText(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
      />
      <span className="text-muted-foreground text-sm">%</span>
    </span>
  );
}

export interface ContactDialogProps {
  /** Open on this contact; undefined opens an empty card to create. */
  contactId?: string;
  /** Prefilled when creating (from the counterparties to identify). */
  initial?: { form?: Partial<ContactFormValue>; identifiers?: NewContactIdentifier[] };
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

function useInvalidateContacts() {
  const queryClient = useQueryClient();
  return () => {
    for (const key of [
      QueryKeys.CONTACTS,
      QueryKeys.CONTACT,
      QueryKeys.UNRESOLVED_COUNTERPARTIES,
    ]) {
      queryClient.invalidateQueries({ queryKey: [key] });
    }
  };
}

export function ContactDialog({ contactId, initial, open, onOpenChange }: ContactDialogProps) {
  const { t } = useTranslation();
  const invalidate = useInvalidateContacts();
  const queryClient = useQueryClient();
  const { accounts } = useAccounts({ filterActive: false, includeArchived: true });

  const detail = useQuery({
    queryKey: [QueryKeys.CONTACT, contactId],
    queryFn: () => getContact(contactId!),
    enabled: open && !!contactId,
  });
  const all = useQuery({ queryKey: [QueryKeys.CONTACTS], queryFn: listContacts, enabled: open });
  const identities = useQuery({
    queryKey: [QueryKeys.ACCOUNT_IDENTITIES],
    queryFn: listAccountIdentities,
    enabled: open,
  });

  const [form, setForm] = useState<ContactFormValue>(emptyForm());
  const [draftIdentifiers, setDraftIdentifiers] = useState<NewContactIdentifier[]>([]);
  const [draftAccounts, setDraftAccounts] = useState<string[]>([]);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [mergeTarget, setMergeTarget] = useState<string | null>(null);

  // load the card when the dialog opens on a contact, or the prefilled values when it opens to create
  const loaded = detail.data?.contact;
  const loadedStamp = loaded ? `${loaded.id}:${loaded.updatedAt}` : null;
  useEffect(() => {
    if (!open) return;
    if (contactId) {
      if (loaded) setForm(contactToForm(loaded));
    } else {
      setForm({ ...emptyForm(), ...initial?.form });
      setDraftIdentifiers(initial?.identifiers ?? []);
      setDraftAccounts([]);
    }
  }, [open, contactId, loadedStamp]);

  const accountName = useMemo(() => {
    const names = new Map<string, string>();
    for (const a of accounts) names.set(a.id, a.name);
    return (id: string) => names.get(id) ?? id;
  }, [accounts]);

  const afterChange = (next: ContactDetail) => {
    queryClient.setQueryData([QueryKeys.CONTACT, next.contact.id], next);
    invalidate();
  };
  const fail = (e: unknown) =>
    toast({ title: t("settings:contacts.error"), description: message(e), variant: "destructive" });

  const create = useMutation({
    mutationFn: () => {
      const payload: NewContact = {
        ...formToInput(form),
        identifiers: draftIdentifiers,
        linkedAccountIds: draftAccounts,
      };
      return createContact(payload);
    },
    onSuccess: () => {
      invalidate();
      toast({ title: t("settings:contacts.created") });
      onOpenChange(false);
    },
    onError: fail,
  });

  const save = useMutation({
    mutationFn: () => updateContact(contactId!, formToInput(form)),
    onSuccess: (next) => {
      afterChange(next);
      toast({ title: t("settings:contacts.saved") });
    },
    onError: fail,
  });

  const remove = useMutation({
    mutationFn: () => deleteContact(contactId!),
    onSuccess: () => {
      invalidate();
      toast({ title: t("settings:contacts.deleted") });
      onOpenChange(false);
    },
    onError: fail,
  });

  const merge = useMutation({
    mutationFn: (target: string) => mergeContacts(contactId!, target),
    onSuccess: () => {
      invalidate();
      toast({ title: t("settings:contacts.merged") });
      onOpenChange(false);
    },
    onError: fail,
  });

  const addIdentifier = useMutation({
    mutationFn: (identifier: NewContactIdentifier) => addContactIdentifier(contactId!, identifier),
    onSuccess: afterChange,
    onError: fail,
  });
  const dropIdentifier = useMutation({
    mutationFn: (id: string) => removeContactIdentifier(id),
    onSuccess: afterChange,
    onError: fail,
  });
  const link = useMutation({
    mutationFn: (accountId: string) => linkContactAccount(contactId!, accountId),
    onSuccess: afterChange,
    onError: fail,
  });
  const setShare = useMutation({
    mutationFn: (v: { accountId: string; share: number }) =>
      setContactAccountShare(contactId!, v.accountId, v.share),
    onSuccess: (next) => {
      afterChange(next);
      queryClient.invalidateQueries({ queryKey: [QueryKeys.PERSON_OVERVIEW] });
    },
    onError: fail,
  });
  const unlink = useMutation({
    mutationFn: (accountId: string) => unlinkContactAccount(contactId!, accountId),
    onSuccess: afterChange,
    onError: fail,
  });

  const linked = contactId ? (detail.data?.linkedAccountIds ?? []) : draftAccounts;
  // the IBAN and BIC of an account in Wealthfolio are shown under that account, for each of its holders
  const identityOf = (accountId: string) => identities.data?.find((i) => i.accountId === accountId);
  const linkedIbans = new Set(
    linked.map((a) => identityOf(a)?.iban?.replace(/\s+/g, "").toUpperCase()).filter(Boolean),
  );
  const identifiers = contactId
    ? (detail.data?.identifiers ?? []).filter(
        (i) => !(i.kind === "iban" && linkedIbans.has(i.value.toUpperCase())),
      )
    : null;
  const complete = isFormComplete(form);
  const others = (all.data ?? []).filter((c) => c.contact.id !== contactId);
  const busy = create.isPending || save.isPending;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-h-[90vh] overflow-y-auto sm:max-w-3xl">
        <DialogTitle>
          {contactId
            ? (detail.data?.contact.displayName ?? t("settings:contacts.edit_title"))
            : t("settings:contacts.new_title")}
        </DialogTitle>
        <DialogDescription>{t("settings:contacts.dialog_hint")}</DialogDescription>

        {contactId && detail.isLoading ? (
          <Skeleton className="h-64 w-full" />
        ) : (
          <div className="grid gap-6">
            <section className="grid gap-3">
              <h3 className="text-sm font-semibold">{t("settings:contacts.section_card")}</h3>
              <ContactForm value={form} onChange={setForm} />
              <div className="flex justify-end">
                <Button
                  disabled={!complete || busy}
                  onClick={() => (contactId ? save.mutate() : create.mutate())}
                >
                  {contactId ? t("settings:contacts.save") : t("settings:contacts.create")}
                </Button>
              </div>
            </section>

            <section className="grid gap-3">
              <div>
                <h3 className="text-sm font-semibold">{t("settings:contacts.section_accounts")}</h3>
                <p className="text-muted-foreground text-xs">
                  {t("settings:contacts.identifiers_hint")}
                </p>
              </div>
              <IdentifierList
                identifiers={
                  identifiers ??
                  draftIdentifiers.map((d, i) => ({
                    id: String(i),
                    contactId: "",
                    kind: d.kind,
                    value: d.value,
                    label: d.label ?? null,
                    bic: d.bic ?? null,
                    institution: d.institution ?? null,
                    createdAt: "",
                  }))
                }
                onRemove={(id) =>
                  contactId
                    ? dropIdentifier.mutate(id)
                    : setDraftIdentifiers((list) => list.filter((_, i) => String(i) !== id))
                }
              />
              <IdentifierAdd
                onAdd={(identifier) =>
                  contactId
                    ? addIdentifier.mutateAsync(identifier)
                    : Promise.resolve(setDraftIdentifiers((list) => [...list, identifier]))
                }
              />
            </section>

            <section className="grid gap-3">
              <div>
                <h3 className="text-sm font-semibold">{t("settings:contacts.section_linked")}</h3>
                <p className="text-muted-foreground text-xs">
                  {t("settings:contacts.linked_hint")}
                </p>
              </div>
              {linked.length === 0 ? (
                <p className="text-muted-foreground text-sm">
                  {t("settings:contacts.linked_none")}
                </p>
              ) : (
                <ul className="grid gap-2">
                  {linked.map((accountId) => (
                    <li
                      key={accountId}
                      className="flex items-center justify-between gap-2 rounded-md border px-3 py-2"
                    >
                      <div className="grid min-w-0 flex-1 gap-0.5">
                        <span className="flex items-center gap-2 text-sm">
                          <Icons.CreditCard className="text-muted-foreground size-4" />
                          {accountName(accountId)}
                        </span>
                        <AccountBankLine identity={identityOf(accountId)} />
                      </div>
                      {contactId && (
                        <ShareField
                          value={
                            detail.data?.accountShares.find((s) => s.accountId === accountId)
                              ?.sharePercent ?? 100
                          }
                          onSave={(share) => setShare.mutate({ accountId, share })}
                        />
                      )}
                      <Button
                        variant="ghost"
                        size="sm"
                        aria-label={t("settings:contacts.linked_remove")}
                        onClick={() =>
                          contactId
                            ? unlink.mutate(accountId)
                            : setDraftAccounts((list) => list.filter((a) => a !== accountId))
                        }
                      >
                        <Icons.Close className="size-4" />
                      </Button>
                    </li>
                  ))}
                </ul>
              )}
              <Select
                value=""
                onValueChange={(accountId) => {
                  if (!accountId) return;
                  if (contactId) link.mutate(accountId);
                  else
                    setDraftAccounts((list) =>
                      list.includes(accountId) ? list : [...list, accountId],
                    );
                }}
              >
                <SelectTrigger aria-label={t("settings:contacts.linked_add")}>
                  <SelectValue placeholder={t("settings:contacts.linked_add")} />
                </SelectTrigger>
                <SelectContent>
                  {accounts
                    .filter((a) => !linked.includes(a.id))
                    .map((a) => (
                      <SelectItem key={a.id} value={a.id}>
                        {a.name}
                      </SelectItem>
                    ))}
                </SelectContent>
              </Select>
            </section>

            {contactId && detail.data ? (
              <>
                {linked.length > 0 && <PersonOverviewSection contactId={contactId} />}
                <section className="grid gap-3">
                  <div className="flex items-center gap-2">
                    <h3 className="text-sm font-semibold">
                      {t("settings:contacts.section_operations")}
                    </h3>
                    <Badge variant="secondary">{detail.data.operationCount}</Badge>
                  </div>
                  {detail.data.operations.length === 0 ? (
                    <p className="text-muted-foreground text-sm">
                      {t("settings:contacts.operations_none")}
                    </p>
                  ) : (
                    <div className="overflow-x-auto rounded-md border">
                      <table className="w-full text-sm">
                        <tbody>
                          {detail.data.operations.map((op) => (
                            <tr key={op.activityId} className="border-b last:border-0">
                              <td className="px-3 py-2 whitespace-nowrap">{op.date}</td>
                              <td className="text-muted-foreground px-3 py-2">
                                {accountName(op.accountId)}
                              </td>
                              <td className="px-3 py-2">{op.label}</td>
                              <td
                                className={`px-3 py-2 text-right whitespace-nowrap ${
                                  toNumber(op.amount) > 0 ? "text-success" : ""
                                }`}
                              >
                                {money(op.amount, op.currency)}
                              </td>
                            </tr>
                          ))}
                        </tbody>
                      </table>
                    </div>
                  )}
                </section>

                <section className="grid gap-3 rounded-md border border-dashed p-3">
                  {others.length > 0 ? (
                    <div className="flex flex-wrap items-center gap-2">
                      <Select
                        items={others.map((c) => ({
                          value: c.contact.id,
                          label: c.contact.displayName,
                        }))}
                        value={mergeTarget ?? ""}
                        onValueChange={setMergeTarget}
                      >
                        <SelectTrigger
                          className="w-64"
                          aria-label={t("settings:contacts.merge_into")}
                        >
                          <SelectValue placeholder={t("settings:contacts.merge_into")} />
                        </SelectTrigger>
                        <SelectContent>
                          {others.map((c) => (
                            <SelectItem key={c.contact.id} value={c.contact.id}>
                              {c.contact.displayName}
                            </SelectItem>
                          ))}
                        </SelectContent>
                      </Select>
                      <Button
                        variant="outline"
                        disabled={!mergeTarget || merge.isPending}
                        onClick={() => mergeTarget && merge.mutate(mergeTarget)}
                      >
                        {t("settings:contacts.merge_action")}
                      </Button>
                      <p className="text-muted-foreground w-full text-xs">
                        {t("settings:contacts.merge_hint")}
                      </p>
                    </div>
                  ) : null}
                  <div>
                    <Button variant="destructive" onClick={() => setConfirmDelete(true)}>
                      {t("settings:contacts.delete_action")}
                    </Button>
                  </div>
                </section>
              </>
            ) : null}
          </div>
        )}

        <AlertDialog open={confirmDelete} onOpenChange={setConfirmDelete}>
          <AlertDialogContent>
            <AlertDialogHeader>
              <AlertDialogTitle>{t("settings:contacts.delete_confirm_title")}</AlertDialogTitle>
              <AlertDialogDescription>
                {t("settings:contacts.delete_confirm_description")}
              </AlertDialogDescription>
            </AlertDialogHeader>
            <AlertDialogFooter>
              <AlertDialogCancel>{t("settings:contacts.cancel")}</AlertDialogCancel>
              <AlertDialogAction onClick={() => remove.mutate()}>
                {t("settings:contacts.delete_action")}
              </AlertDialogAction>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialog>
      </DialogContent>
    </Dialog>
  );
}

/** The IBAN, BIC and bank of an account, under its name, with a button to copy the IBAN. */
function AccountBankLine({ identity }: { identity?: AccountIdentity }) {
  const { t } = useTranslation();
  if (!identity?.iban && !identity?.bic) return null;
  return (
    <span className="text-muted-foreground flex flex-wrap items-center gap-x-2 text-xs">
      {identity.iban ? <code>{formatIban(identity.iban)}</code> : null}
      {[identity.bic, identity.institution].filter(Boolean).join(" · ")}
      {identity.iban ? (
        <button
          type="button"
          className="hover:text-foreground"
          aria-label={t("settings:contacts.copy_iban")}
          onClick={() => {
            navigator.clipboard
              ?.writeText(identity.iban ?? "")
              .then(() => toast({ title: t("settings:contacts.copied") }))
              .catch(() => undefined);
          }}
        >
          <Icons.Copy className="size-3" />
        </button>
      ) : null}
    </span>
  );
}

function IdentifierList({
  identifiers,
  onRemove,
}: {
  identifiers: ContactDetail["identifiers"];
  onRemove: (id: string) => void;
}) {
  const { t } = useTranslation();
  if (identifiers.length === 0) {
    return (
      <p className="text-muted-foreground text-sm">{t("settings:contacts.identifier_none")}</p>
    );
  }
  return (
    <ul className="grid gap-2">
      {identifiers.map((i) => (
        <li
          key={i.id}
          className="flex items-center justify-between gap-2 rounded-md border px-3 py-2"
        >
          <div className="grid min-w-0 gap-0.5 text-sm">
            <span className="flex flex-wrap items-center gap-2">
              <Badge variant="outline">
                {i.kind === "iban"
                  ? t("settings:contacts.identifier_iban")
                  : t("settings:contacts.identifier_name")}
              </Badge>
              {i.kind === "iban" ? (
                <code className="text-xs">{formatIban(i.value)}</code>
              ) : (
                <span>{i.label ?? i.value}</span>
              )}
            </span>
            {i.kind === "iban" && (i.label || i.institution || i.bic) ? (
              <span className="text-muted-foreground text-xs">
                {[i.label, i.institution, i.bic].filter(Boolean).join(" · ")}
              </span>
            ) : null}
          </div>
          <div className="flex shrink-0 items-center">
            {i.kind === "iban" ? (
              <Button
                variant="ghost"
                size="sm"
                aria-label={t("settings:contacts.copy_iban")}
                onClick={() => {
                  navigator.clipboard
                    ?.writeText(i.value)
                    .then(() => toast({ title: t("settings:contacts.copied") }))
                    .catch(() => undefined);
                }}
              >
                <Icons.Copy className="size-4" />
              </Button>
            ) : null}
            <Button
              variant="ghost"
              size="sm"
              aria-label={t("settings:contacts.identifier_remove")}
              onClick={() => onRemove(i.id)}
            >
              <Icons.Close className="size-4" />
            </Button>
          </div>
        </li>
      ))}
    </ul>
  );
}

function IdentifierAdd({
  onAdd,
}: {
  onAdd: (identifier: NewContactIdentifier) => Promise<unknown>;
}) {
  const { t } = useTranslation();
  const [kind, setKind] = useState<ContactIdentifierKind>("iban");
  const [value, setValue] = useState("");
  const [label, setLabel] = useState("");
  const [pending, setPending] = useState(false);

  const submit = async () => {
    if (!value.trim()) return;
    setPending(true);
    try {
      await onAdd({
        kind,
        value: value.trim(),
        label: label.trim() || null,
      });
      setValue("");
      setLabel("");
    } catch {
      // the mutation reports the error
    } finally {
      setPending(false);
    }
  };

  return (
    <div className="grid gap-2 sm:grid-cols-[10rem_1fr_1fr_auto] sm:items-end">
      <div className="grid gap-1.5">
        <Label>{t("settings:contacts.identifier_type")}</Label>
        <Select
          items={[
            { value: "iban", label: t("settings:contacts.identifier_iban") },
            { value: "name", label: t("settings:contacts.identifier_name") },
          ]}
          value={kind}
          onValueChange={(v) => v && setKind(v as ContactIdentifierKind)}
        >
          <SelectTrigger aria-label={t("settings:contacts.identifier_type")}>
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="iban">{t("settings:contacts.identifier_iban")}</SelectItem>
            <SelectItem value="name">{t("settings:contacts.identifier_name")}</SelectItem>
          </SelectContent>
        </Select>
      </div>
      <div className="grid gap-1.5">
        <Label htmlFor="identifier-value">
          {kind === "iban"
            ? t("settings:contacts.identifier_value_iban")
            : t("settings:contacts.identifier_value_name")}
        </Label>
        <Input
          id="identifier-value"
          autoComplete="off"
          value={value}
          onChange={(e) => setValue(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              void submit();
            }
          }}
        />
      </div>
      <div className="grid gap-1.5">
        <Label htmlFor="identifier-label">{t("settings:contacts.identifier_label")}</Label>
        <Input
          id="identifier-label"
          autoComplete="off"
          value={label}
          onChange={(e) => setLabel(e.target.value)}
        />
      </div>
      <Button variant="outline" disabled={!value.trim() || pending} onClick={() => void submit()}>
        {t("settings:contacts.identifier_add")}
      </Button>
    </div>
  );
}

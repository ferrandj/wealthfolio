import { listAccountIdentities, listContacts } from "@/adapters";
import { useBalancePrivacy } from "@/hooks/use-balance-privacy";
import { maskText } from "@/lib/privacy-mask";
import { QueryKeys } from "@/lib/query-keys";
import { useQuery } from "@tanstack/react-query";
import type { AccountIdentity } from "@/lib/types/statements";
import { useAccounts } from "@/hooks/use-accounts";
import { Button, Icons } from "@wealthfolio/ui";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toNumber } from "../contacts-utils";
import { ContactDialog } from "./contact-dialog";
import { IdentifyWizard, type IdentifyItemData } from "./identify-wizard";

/**
 * What the wizard needs to ask for the owners of an account that Wealthfolio already has: one entry per holder
 * the documents name (several for a joint account), or one blank entry when no document names anyone.
 */
function ownerItems(accountId: string, identity?: AccountIdentity): IdentifyItemData[] {
  const names = identity?.holders?.length
    ? identity.holders
    : identity?.holderName
      ? [identity.holderName]
      : [null];
  return names.map((name, index) => ({
    kind: "name",
    key: `own:${accountId}:${index}`,
    name,
    iban: identity?.iban ?? null,
    count: 0,
    totalIn: 0,
    totalOut: 0,
    lastDate: "",
    accountIds: [accountId],
    ownAccountId: accountId,
    holderCount: names.length,
  }));
}

/**
 * Who owns an account of Wealthfolio, wherever the account is shown. The owner is a contact like any other
 * (the person themselves included). It can be changed at any time: a click opens the contact's card (shares of a
 * joint account, detaching the account), the button adds another owner or sets the first one.
 */
export function AccountOwners({ accountId }: { accountId: string }) {
  const { t } = useTranslation();
  const { isBalanceHidden: hidden } = useBalancePrivacy();
  const contacts = useQuery({ queryKey: [QueryKeys.CONTACTS], queryFn: listContacts });
  const identities = useQuery({
    queryKey: [QueryKeys.ACCOUNT_IDENTITIES],
    queryFn: listAccountIdentities,
  });
  const [wizard, setWizard] = useState<IdentifyItemData[] | null>(null);
  const [openContact, setOpenContact] = useState<string | null>(null);

  const owners = (contacts.data ?? []).filter((c) => c.linkedAccountIds.includes(accountId));
  const identity = identities.data?.find((i) => i.accountId === accountId);

  return (
    <span className="inline-flex flex-wrap items-center gap-x-1.5 gap-y-1">
      {owners.length === 0 ? (
        <span className="text-warning">{t("settings:contacts.owner_none")}</span>
      ) : (
        owners.map((c) => {
          const share = toNumber(
            c.accountShares.find((s) => s.accountId === accountId)?.sharePercent ?? 100,
          );
          return (
            <button
              key={c.contact.id}
              type="button"
              className="hover:underline"
              onClick={() => setOpenContact(c.contact.id)}
            >
              <Icons.User className="me-0.5 inline size-3" />
              {hidden ? maskText(c.contact.displayName) : c.contact.displayName}
              {owners.length > 1 || share < 100 ? ` · ${share} %` : ""}
            </button>
          );
        })
      )}
      <button
        type="button"
        className="text-primary hover:underline"
        onClick={() => setWizard(ownerItems(accountId, identity))}
      >
        {owners.length === 0
          ? t("settings:contacts.owner_define")
          : t("settings:contacts.owner_add")}
      </button>
      {wizard ? <IdentifyWizard items={wizard} onClose={() => setWizard(null)} /> : null}
      {openContact ? (
        <ContactDialog
          open
          contactId={openContact}
          onOpenChange={(o) => !o && setOpenContact(null)}
        />
      ) : null}
    </span>
  );
}

/**
 * Asks for the owner of accounts one after the other (those that have none yet): right after an import that created
 * accounts, or from the banner of accounts without an owner.
 */
export function AccountOwnerPrompt({
  accountIds,
  onClose,
}: {
  accountIds: string[];
  onClose: () => void;
}) {
  const contacts = useQuery({ queryKey: [QueryKeys.CONTACTS], queryFn: listContacts });
  const identities = useQuery({
    queryKey: [QueryKeys.ACCOUNT_IDENTITIES],
    queryFn: listAccountIdentities,
  });
  if (!contacts.data || !identities.data) return null;
  return (
    <PromptReady
      accountIds={accountIds.filter(
        (id) => !contacts.data.some((c) => c.linkedAccountIds.includes(id)),
      )}
      identities={identities.data}
      onClose={onClose}
    />
  );
}

function PromptReady({
  accountIds,
  identities,
  onClose,
}: {
  accountIds: string[];
  identities: AccountIdentity[];
  onClose: () => void;
}) {
  // frozen when it opens: the list must not move while the owners are being set
  const [items] = useState(() =>
    accountIds.flatMap((id) =>
      ownerItems(
        id,
        identities.find((i) => i.accountId === id),
      ),
    ),
  );
  if (items.length === 0) {
    onClose();
    return null;
  }
  return <IdentifyWizard items={items} onClose={onClose} />;
}

/** A reminder on the documents page: accounts that nobody owns yet. */
export function OwnerlessAccountsBanner() {
  const { t } = useTranslation();
  const { accounts } = useAccounts({ filterActive: false });
  const contacts = useQuery({ queryKey: [QueryKeys.CONTACTS], queryFn: listContacts });
  const [open, setOpen] = useState(false);
  if (!contacts.data) return null;
  const ownerless = accounts.filter(
    (a) => !contacts.data.some((c) => c.linkedAccountIds.includes(a.id)),
  );
  if (ownerless.length === 0) return null;
  return (
    <div className="border-warning/60 flex flex-wrap items-center justify-between gap-3 rounded-lg border p-3 text-sm">
      <span className="flex items-center gap-2">
        <Icons.AlertTriangle className="text-warning size-4" />
        {t("settings:contacts.owner_banner", { count: ownerless.length })}
      </span>
      <Button size="sm" variant="outline" onClick={() => setOpen(true)}>
        {t("settings:contacts.owner_define")}
      </Button>
      {open ? (
        <AccountOwnerPrompt
          accountIds={ownerless.map((a) => a.id)}
          onClose={() => setOpen(false)}
        />
      ) : null}
    </div>
  );
}

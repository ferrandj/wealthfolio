import type { ContactSummary } from "@/lib/types/contacts";
import type { AccountIdentity, StatementDocument } from "@/lib/types/statements";
import { sameName } from "../contacts-utils";

const SAVINGS_NAME = /\b(livret|ldds|lep|pel|cel|[ée]pargne|compte [àa] terme)\b/i;

/**
 * A current account (personal or joint) is the only one whose IBAN and BIC are asked: money is paid to and from it.
 * The kind read on its documents decides; without one, a cash account whose name is not a savings product.
 */
export function isCurrentAccount(
  accountId: string,
  accountType: string | undefined,
  accountName: string,
  documents: Pick<StatementDocument, "accountId" | "accountType">[],
): boolean {
  const kinds = documents
    .filter((d) => d.accountId === accountId && d.accountType)
    .map((d) => d.accountType);
  if (kinds.length > 0) return kinds.includes("checking");
  return accountType === "CASH" && !SAVINGS_NAME.test(accountName);
}

/** The name identifier of a contact that is this printed name (same words, any order, civility aside). */
export function printedNameOwner(
  contacts: ContactSummary[],
  printed: string,
): { id: string; contactId: string } | null {
  for (const c of contacts) {
    for (const i of c.identifiers) {
      if (i.kind === "name" && (sameName(i.value, printed) || sameName(i.label ?? "", printed))) {
        return { id: i.id, contactId: c.contact.id };
      }
    }
  }
  return null;
}

/** Whether a contact already carries this IBAN (a second contact cannot take it). */
export function ibanTaken(contacts: ContactSummary[], iban: string): boolean {
  return contacts.some((c) => c.identifiers.some((i) => i.kind === "iban" && i.value === iban));
}

/**
 * The bank of an account, as well as it can be known: what its documents recorded, else the institution of one of its
 * documents, else the first part of the name Wealthfolio gave it at import ("Fortuneo - Assurance-vie ••7101").
 */
export function guessInstitution(
  identity: AccountIdentity | undefined,
  accountId: string,
  accountName: string,
  documents: Pick<StatementDocument, "accountId" | "institution">[],
): string {
  const known = identity?.institution?.trim();
  if (known) return known;
  const fromDocument = documents.find((d) => d.accountId === accountId && d.institution?.trim());
  if (fromDocument?.institution) return fromDocument.institution.trim();
  const head = accountName.split(" - ");
  return head.length > 1 ? head[0].trim() : "";
}

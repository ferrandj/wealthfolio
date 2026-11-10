// Contacts: beneficiaries, debtors and people.

export type ContactKind = "person" | "organization" | "self" | "other";
export type ContactRole = "none" | "beneficiary" | "debtor" | "both";
export type ContactIdentifierKind = "iban" | "name";

export interface Contact {
  id: string;
  kind: ContactKind;
  role: ContactRole;
  displayName: string;
  civility: string | null;
  lastName: string | null;
  firstName: string | null;
  birthName: string | null;
  address: string | null;
  phone: string | null;
  email: string | null;
  notes: string | null;
  createdAt: string;
  updatedAt: string;
}

/** One bank account (IBAN) or printed name that recognises a contact in imported operations. */
export interface ContactIdentifier {
  id: string;
  contactId: string;
  kind: ContactIdentifierKind;
  /** The IBAN without spaces, or the normalised name key. */
  value: string;
  /** What the user typed, or a label such as "Compte courant". */
  label: string | null;
  bic: string | null;
  institution: string | null;
  createdAt: string;
}

/** The fields filled in on a contact's card. */
export interface ContactInput {
  kind: ContactKind;
  role?: ContactRole;
  displayName?: string | null;
  civility?: string | null;
  lastName?: string | null;
  firstName?: string | null;
  birthName?: string | null;
  address?: string | null;
  phone?: string | null;
  email?: string | null;
  notes?: string | null;
}

export interface NewContactIdentifier {
  kind: ContactIdentifierKind;
  value: string;
  label?: string | null;
  bic?: string | null;
  institution?: string | null;
}

export interface NewContact extends ContactInput {
  identifiers?: NewContactIdentifier[];
  /** Wealthfolio accounts (imported ones included) that belong to this contact. */
  linkedAccountIds?: string[];
}

export interface ContactSummary {
  contact: Contact;
  identifiers: ContactIdentifier[];
  linkedAccountIds: string[];
  /** The part of each linked account the person holds (100 unless the account is joint). */
  accountShares: AccountShare[];
  operationCount: number;
  totalIn: string | number;
  totalOut: string | number;
  lastDate: string | null;
}

export interface ContactOperation {
  activityId: string;
  accountId: string;
  date: string;
  /** Signed: negative when money went out. */
  amount: string | number;
  currency: string;
  label: string;
  counterpartyName: string | null;
  counterpartyIban: string | null;
}

export interface ContactDetail extends ContactSummary {
  operations: ContactOperation[];
}

/** A counterparty seen in imported operations that no contact recognises yet. */
export interface UnresolvedCounterparty {
  kind: ContactIdentifierKind;
  key: string;
  name: string | null;
  iban: string | null;
  count: number;
  totalIn: string | number;
  totalOut: string | number;
  lastDate: string;
  accountIds: string[];
  /** Set when the IBAN is that of an account already in Wealthfolio. */
  ownAccountId: string | null;
}

export interface BeneficiarySpending {
  contactId: string | null;
  name: string;
  currency: string;
  count: number;
  totalIn: number | string;
  totalOut: number | string;
  lastDate: string;
}

export interface AccountShare {
  accountId: string;
  /** Percent, 0 to 100. */
  sharePercent: number | string;
}

export interface CoHolder {
  contactId: string;
  name: string;
  sharePercent: number | string;
}

export interface PersonAccount {
  accountId: string;
  name: string;
  currency: string;
  sharePercent: number | string;
  joint: boolean;
  coHolders: CoHolder[];
  /** The person's part of the balance at the latest statement. */
  balance: number | string | null;
  balanceDate: string | null;
  income: number | string;
  spending: number | string;
  operationCount: number;
}

export interface CurrencyTotal {
  currency: string;
  balance: number | string;
  income: number | string;
  spending: number | string;
}

export interface PersonOverview {
  contactId: string;
  name: string;
  accounts: PersonAccount[];
  totals: CurrencyTotal[];
  /** Joint accounts whose shares do not add up to 100 %. */
  unbalancedAccounts: string[];
}

/** An account of the person that Wealthfolio does not have: it must be created or imported. */
export interface MissingOwnAccount {
  contactId: string;
  contactName: string;
  iban: string | null;
  label: string | null;
  count: number;
  totalIn: number | string;
  totalOut: number | string;
  lastDate: string;
  accountIds: string[];
}

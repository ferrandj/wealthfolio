import type { Account } from "@/lib/types";
import type {
  ContactSummary,
  MissingOwnAccount,
  UnresolvedCounterparty,
} from "@/lib/types/contacts";
import type {
  AccountCoverage,
  LoanGap,
  LoanView,
  OperationToReview,
  StatementDocument,
} from "@/lib/types/statements";

/** Which kind of review an operation of the statements needs. */
export const OPERATION_KINDS = [
  "to_categorize",
  "doubt",
  "loan_to_attach",
  "loan_proposed",
] as const;
export type OperationKind = (typeof OPERATION_KINDS)[number];

export interface TodoLoan {
  loan: LoanView;
  /** What is missing, the blocking points first. */
  gaps: LoanGap[];
}

export interface TodoAccountStatements {
  accountId: string;
  /** Months without a statement that are not marked as normal (an account closed for a while…). */
  missingMonths: string[];
  /** Two statements in a row whose balances do not follow. */
  balanceBreaks: number;
}

export interface TodoSummary {
  documents: StatementDocument[];
  operations: Record<OperationKind, number>;
  loans: TodoLoan[];
  ownerlessAccounts: Account[];
  statements: TodoAccountStatements[];
  missingOwnAccounts: MissingOwnAccount[];
  counterparties: number;
  pendingActivities: number;
  /** Everything to do, all sections together. */
  total: number;
}

export interface TodoSources {
  documents?: StatementDocument[];
  operations?: OperationToReview[];
  loans?: LoanView[];
  accounts?: Account[];
  contacts?: ContactSummary[];
  coverage?: AccountCoverage[];
  missingOwnAccounts?: MissingOwnAccount[];
  counterparties?: UnresolvedCounterparty[];
  pendingActivities?: number;
}

/**
 * Everything that is missing or waits for the person, from what the app already knows: documents to validate,
 * operations to categorize or attach to a loan, loans whose papers are incomplete, accounts nobody holds, months
 * without a statement, own accounts and counterparties not identified, activities flagged for review.
 */
export function buildTodo(sources: TodoSources): TodoSummary {
  const documents = (sources.documents ?? []).filter((d) => d.status === "PENDING");
  const items = sources.operations ?? [];
  const operations = Object.fromEntries(
    OPERATION_KINDS.map((k) => [k, items.filter((i) => i.why.includes(k)).length]),
  ) as Record<OperationKind, number>;
  const loans = (sources.loans ?? [])
    .map((loan) => ({
      loan,
      gaps: [...(loan.gaps ?? [])].sort((a, b) => Number(b.blocking) - Number(a.blocking)),
    }))
    .filter(({ loan, gaps }) => gaps.length > 0 || loan.liabilityPending);
  const contacts = sources.contacts;
  const ownerlessAccounts = contacts
    ? (sources.accounts ?? []).filter(
        (a) =>
          a.isActive && !a.isArchived && !contacts.some((c) => c.linkedAccountIds.includes(a.id)),
      )
    : [];
  const statements = (sources.coverage ?? [])
    .map((c) => ({
      accountId: c.accountId,
      missingMonths: c.missingMonths.filter((m) => !m.normal).map((m) => m.month),
      balanceBreaks: c.balanceBreaks.length,
    }))
    .filter((s) => s.missingMonths.length > 0 || s.balanceBreaks > 0);
  const missingOwnAccounts = sources.missingOwnAccounts ?? [];
  const counterparties = sources.counterparties?.length ?? 0;
  const pendingActivities = sources.pendingActivities ?? 0;
  const total =
    documents.length +
    items.length +
    loans.length +
    ownerlessAccounts.length +
    statements.length +
    missingOwnAccounts.length +
    counterparties +
    pendingActivities;
  return {
    documents,
    operations,
    loans,
    ownerlessAccounts,
    statements,
    missingOwnAccounts,
    counterparties,
    pendingActivities,
    total,
  };
}

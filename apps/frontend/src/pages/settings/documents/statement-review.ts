import type {
  ReviewedOperation,
  StatementDocument,
  StatementReview,
  StatementReviewReason,
} from "@/lib/types/statements";

/** A category picked for an operation during the review. */
export interface ReviewChoice {
  taxonomyId: string;
  categoryId: string;
}

/**
 * The logical order to look at documents: account by account, oldest period first, so that each statement's
 * opening balance is the previous one's closing balance. Mirrors `logical_order` in the core crate.
 */
export function logicalOrder(a: StatementDocument, b: StatementDocument): number {
  const key = (d: StatementDocument): [number, string, number, string, string, string] => [
    d.accountId ? 0 : 1,
    d.accountId ?? "",
    d.periodStart ? 0 : 1,
    d.periodStart ?? "",
    d.periodEnd ?? "",
    d.createdAt,
  ];
  const ka = key(a);
  const kb = key(b);
  for (let i = 0; i < ka.length; i += 1) {
    if (ka[i] < kb[i]) return -1;
    if (ka[i] > kb[i]) return 1;
  }
  return 0;
}

export function parseReviewReason(value: string | null | undefined): StatementReviewReason | null {
  if (!value) return null;
  try {
    const parsed = JSON.parse(value) as Partial<StatementReviewReason>;
    return {
      doubts: Number(parsed.doubts ?? 0),
      balanceGap: parsed.balanceGap ?? null,
      warnings: Number(parsed.warnings ?? 0),
    };
  } catch {
    return null;
  }
}

/** "Account to account": money moved between the person's own accounts, neither spent nor earned. */
export function ownTransferChoice(amount: string): ReviewChoice {
  return Number(amount) < 0
    ? { taxonomyId: "spending_categories", categoryId: "cat_transfers_own" }
    : { taxonomyId: "income_sources", categoryId: "cat_income_transfers_own" };
}

/** The category the reader suggests for an operation, when it has one (else the stored one). */
export function suggestedChoice(op: ReviewedOperation): ReviewChoice | null {
  const taxonomyId = op.suggestedTaxonomyId ?? op.taxonomyId;
  const categoryId = op.suggestedCategoryId ?? op.categoryId;
  return taxonomyId && categoryId ? { taxonomyId, categoryId } : null;
}

/** The doubtful operations of a review that nobody has decided yet (those that can be changed: they have an activity). */
export function doubtfulOperations(
  statements: StatementReview[],
  choices: Record<string, ReviewChoice>,
): ReviewedOperation[] {
  return statements.flatMap((s) =>
    s.operations.filter((o) => o.needsReview && o.activityId && !choices[o.activityId]),
  );
}

/** Confirm every doubtful operation as the reader suggests it. */
export function confirmDoubts(
  statements: StatementReview[],
  choices: Record<string, ReviewChoice>,
): Record<string, ReviewChoice> {
  const next = { ...choices };
  for (const op of doubtfulOperations(statements, choices)) {
    const choice = suggestedChoice(op);
    if (choice && op.activityId) next[op.activityId] = choice;
  }
  return next;
}

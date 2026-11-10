import type { StatementDocument, StatementReviewReason } from "@/lib/types/statements";

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

import type { ActivityDetails } from "@/lib/types";
import type { LoanAnswers, LoanPayment, LoanView } from "@/lib/types/statements";
import { answersWith } from "./loan-answers";

/** Activity kinds an instalment can be debited as. */
export const DEBIT_TYPES = ["WITHDRAWAL", "TRANSFER_OUT"];

/** How many days around the due date a debit is looked for. */
export const MATCH_WINDOW = { before: 15, after: 20 };

const days = (a: string, b: string) =>
  Math.round((new Date(a).getTime() - new Date(b).getTime()) / 86_400_000);

const amountOf = (a: Pick<ActivityDetails, "amount">) => Math.abs(Number(a.amount ?? 0));

/** The window to search debits in for an instalment due on `date` (YYYY-MM-DD). */
export function matchWindow(date: string): { dateFrom: string; dateTo: string } {
  const at = new Date(`${date}T00:00:00Z`).getTime();
  const iso = (t: number) => new Date(t).toISOString().slice(0, 10);
  return {
    dateFrom: iso(at - MATCH_WINDOW.before * 86_400_000),
    dateTo: iso(at + MATCH_WINDOW.after * 86_400_000),
  };
}

export interface DebitCandidate {
  activity: ActivityDetails;
  /** The debit is the instalment's amount to the cent. */
  sameAmount: boolean;
  /** Days between the debit and the due date. */
  gap: number;
  /** Already paying another instalment of this loan. */
  usedFor?: string;
}

/**
 * The debits that may pay an instalment: those of the window, the same amount first, then the closest amounts and
 * days. A debit that already pays another instalment is kept but said so (it would move).
 */
export function debitCandidates(
  activities: ActivityDetails[],
  line: Pick<LoanPayment, "date" | "expected" | "activityId">,
  lines: Pick<LoanPayment, "date" | "activityId">[],
): DebitCandidate[] {
  const expected = Math.abs(Number(line.expected));
  return activities
    .filter((a) => DEBIT_TYPES.includes(a.activityType) && amountOf(a) > 0)
    .map((activity) => {
      const date = new Date(activity.date).toISOString().slice(0, 10);
      const other = lines.find((l) => l.activityId === activity.id && l.date !== line.date);
      return {
        activity,
        sameAmount: Math.abs(amountOf(activity) - expected) < 0.005,
        gap: days(date, line.date),
        usedFor: other?.date,
      };
    })
    .sort(
      (x, y) =>
        Number(y.sameAmount) - Number(x.sameAmount) ||
        Math.abs(amountOf(x.activity) - expected) - Math.abs(amountOf(y.activity) - expected) ||
        Math.abs(x.gap) - Math.abs(y.gap),
    );
}

/** The answers once `activityId` pays the instalment due on `date`, and only that one. */
export function matchedAnswers(loan: LoanView, date: string, activityId: string): LoanAnswers {
  const confirmed = (loan.answers?.confirmedPayments ?? []).filter(
    (c) => c.date !== date && c.activityId !== activityId,
  );
  return answersWith(loan, {
    confirmedPayments: [...confirmed, { date, activityId }],
    detachedActivities: (loan.answers?.detachedActivities ?? []).filter((a) => a !== activityId),
    otherActivities: (loan.answers?.otherActivities ?? []).filter((a) => a !== activityId),
  });
}

/** The answers once `activityId` is one of the loan's other operations (a fee…): attached, not an instalment. */
export function costAnswers(loan: LoanView, activityId: string): LoanAnswers {
  const others = loan.answers?.otherActivities ?? [];
  return answersWith(loan, {
    confirmedPayments: (loan.answers?.confirmedPayments ?? []).filter(
      (c) => c.activityId !== activityId,
    ),
    detachedActivities: (loan.answers?.detachedActivities ?? []).filter((a) => a !== activityId),
    otherActivities: others.includes(activityId) ? others : [...others, activityId],
  });
}

/** The answers once `activityId` is said not to pay this loan: it is never matched to it again. */
export function detachedAnswers(loan: LoanView, activityId: string): LoanAnswers {
  const detached = loan.answers?.detachedActivities ?? [];
  return answersWith(loan, {
    confirmedPayments: (loan.answers?.confirmedPayments ?? []).filter(
      (c) => c.activityId !== activityId,
    ),
    detachedActivities: detached.includes(activityId) ? detached : [...detached, activityId],
    otherActivities: (loan.answers?.otherActivities ?? []).filter((a) => a !== activityId),
  });
}

/** The loan an operation is attached to as a fee or another non-instalment operation. */
export function loanCostOf(loans: LoanView[], activityId: string): LoanView | null {
  return (
    loans.find(
      (l) =>
        l.answers?.otherActivities?.includes(activityId) ||
        l.payments?.costs?.some((c) => c.activityId === activityId),
    ) ?? null
  );
}

/** Which instalment of which loan a debit pays, as the reconciliation found it (matched, or proposed in doubt). */
export function loanLineOf(
  loans: LoanView[],
  activityId: string,
): { loan: LoanView; line: LoanPayment } | null {
  for (const loan of loans) {
    const line = loan.payments?.lines.find((l) => l.activityId === activityId);
    if (line) return { loan, line };
  }
  return null;
}

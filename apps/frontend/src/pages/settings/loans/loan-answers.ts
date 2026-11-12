import { setStatementLoanAnswers, setStatementLoanFirstDue } from "@/adapters";
import { QueryKeys } from "@/lib/query-keys";
import type { LoanAnswers, LoanView } from "@/lib/types/statements";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "@wealthfolio/ui/components/ui/use-toast";
import { useTranslation } from "react-i18next";
import { invalidateLoanCaches } from "./loan-caches";

/** The answer a gap accepts, merged with what the person already told. */
export function answersWith(loan: LoanView, patch: Partial<LoanAnswers>): LoanAnswers {
  return { ...(loan.answers ?? {}), ...patch };
}

/** The answers with these proposed debits confirmed for their instalments. */
export function withConfirmed(
  loan: LoanView,
  lines: { date: string; activityId?: string | null }[],
): LoanAnswers {
  const confirmed = [...(loan.answers?.confirmedPayments ?? [])];
  for (const l of lines) {
    if (!l.activityId) continue;
    const rest = confirmed.filter((c) => c.date !== l.date);
    confirmed.splice(0, confirmed.length, ...rest, { date: l.date, activityId: l.activityId });
  }
  return answersWith(loan, { confirmedPayments: confirmed });
}

export function useLoanMutations(loan: LoanView) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const done = (loans: LoanView[]) => {
    queryClient.setQueryData([QueryKeys.STATEMENT_LOANS], loans);
    queryClient.invalidateQueries({ queryKey: [QueryKeys.STATEMENT_DOCUMENTS] });
    queryClient.invalidateQueries({ queryKey: [QueryKeys.STATEMENT_REVIEW] });
    queryClient.invalidateQueries({ queryKey: [QueryKeys.STATEMENT_REVIEW_QUEUE] });
    invalidateLoanCaches(queryClient);
  };
  const onError = (e: unknown) =>
    toast({ title: t("settings:loans.error"), description: String(e), variant: "destructive" });
  const answer = useMutation({
    mutationFn: (answers: LoanAnswers) => setStatementLoanAnswers(loan.key, answers),
    onSuccess: done,
    onError,
  });
  const firstDue = useMutation({
    mutationFn: (value: string | null) => setStatementLoanFirstDue(loan.key, value),
    onSuccess: done,
    onError,
  });
  return { answer, firstDue };
}

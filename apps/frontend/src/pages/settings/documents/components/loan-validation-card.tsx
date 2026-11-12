import type { LoanView } from "@/lib/types/statements";
import { Icons, Separator, Skeleton } from "@wealthfolio/ui";
import { useTranslation } from "react-i18next";
import { LoanChecks, LoanDocuments, LoanFacts, LoanPlan } from "../../loans/loan-details";

export const LOAN_KINDS = ["loan_offer", "loan_schedule", "loan_event"];

/** The loan a paper belongs to: nothing is kept (no liability) until the person validates it. */
export function loanOfDocument(loans: LoanView[] | undefined, documentId: string) {
  return loans?.find((l) => l.documents.some((d) => d.documentId === documentId));
}

/** No paper dates the schedule: the instalments really debited, read in the bank statements, will. */
export const needsDate = (loan: LoanView | undefined): boolean =>
  !!loan && !loan.firstDue && !loan.inferredFirstDue;

/** What the person sees before validating a loan paper: everything the papers say, nothing about operations. */
export function LoanValidationCard({
  loan,
  loading,
}: {
  loan: LoanView | undefined;
  loading?: boolean;
}) {
  const { t } = useTranslation();
  if (!loan) {
    return loading ? (
      <Skeleton className="h-48" />
    ) : (
      <p className="text-muted-foreground text-sm">{t("settings:documents.loan_unknown")}</p>
    );
  }
  return (
    <div className="space-y-4 text-sm">
      <p className="text-muted-foreground">{t("settings:documents.loan_intro")}</p>
      <div className="font-medium">{loan.label}</div>
      <LoanFacts loan={loan} />
      {needsDate(loan) && (
        <p className="text-muted-foreground flex items-start gap-2">
          <Icons.Info className="mt-0.5 size-4 shrink-0" />
          <span>{t("settings:loans.dates_from_statements")}</span>
        </p>
      )}
      <LoanChecks loan={loan} />
      <Separator />
      <LoanDocuments loan={loan} />
      <LoanPlan loan={loan} defaultOpen />
      {loan.plan.checks.some((c) => c.status === "warn") && (
        <div className="flex items-start gap-2 text-amber-600">
          <Icons.AlertTriangle className="mt-0.5 size-4 shrink-0" />
          <span>{t("settings:documents.loan_check_first")}</span>
        </div>
      )}
    </div>
  );
}

import type { LoanView } from "@/lib/types/statements";
import { Icons, Separator, Skeleton } from "@wealthfolio/ui";
import { useTranslation } from "react-i18next";
import { LoanGaps } from "../../loans/loan-gaps";
import {
  LoanChecks,
  LoanDocuments,
  LoanFacts,
  LoanFollow,
  LoanPlan,
} from "../../loans/loan-details";

export const LOAN_KINDS = ["loan_offer", "loan_schedule", "loan_event"];

/** The loan a paper belongs to: nothing is kept (no liability) until the person validates it. */
export function loanOfDocument(loans: LoanView[] | undefined, documentId: string) {
  return loans?.find((l) => l.documents.some((d) => d.documentId === documentId));
}

/** What keeps a loan from being validated: its blocking gaps. */
export const blockingGaps = (loan: LoanView | undefined) =>
  (loan?.gaps ?? []).filter((g) => g.blocking);

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
      <LoanGaps loan={loan} />
      <LoanFollow loan={loan} />
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

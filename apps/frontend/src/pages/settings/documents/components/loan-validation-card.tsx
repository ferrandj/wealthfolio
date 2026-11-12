import type { LoanView } from "@/lib/types/statements";
import { Icons, Input } from "@wealthfolio/ui";
import { useTranslation } from "react-i18next";
import { day, euros, percent } from "../../loans/loans-utils";

export const LOAN_KINDS = ["loan_offer", "loan_schedule", "loan_event"];

/** The loan a paper belongs to: nothing is kept (no liability) until the person validates it. */
export function loanOfDocument(loans: LoanView[] | undefined, documentId: string) {
  return loans?.find((l) => l.documents.some((d) => d.documentId === documentId));
}

/** The first instalment is needed to date the schedule, unless a paper already dates it. */
export const needsDate = (loan: LoanView | undefined): boolean =>
  !!loan && !loan.firstDue && !loan.inferredFirstDue;

export function LoanValidationCard({
  loan,
  firstDue,
  onFirstDue,
}: {
  loan: LoanView | undefined;
  firstDue: string;
  onFirstDue: (value: string) => void;
}) {
  const { t } = useTranslation();
  if (!loan) {
    return <p className="text-muted-foreground text-sm">{t("settings:documents.loan_unknown")}</p>;
  }
  const problems = loan.plan.checks.filter((c) => c.status === "warn");
  return (
    <div className="space-y-3 text-sm">
      <p className="text-muted-foreground">{t("settings:documents.loan_intro")}</p>
      <div className="grid gap-3 sm:grid-cols-3">
        <div>
          <div className="text-muted-foreground text-xs">{loan.label}</div>
          <div className="font-medium">{euros(loan.principal)}</div>
        </div>
        <div>
          <div className="text-muted-foreground text-xs">{t("settings:loans.rate")}</div>
          <div className="font-medium">{percent(loan.rate)}</div>
        </div>
        <div>
          <div className="text-muted-foreground text-xs">{t("settings:loans.duration")}</div>
          <div className="font-medium">
            {loan.duration ? t("settings:loans.months", { count: loan.duration }) : "—"}
          </div>
        </div>
      </div>
      {loan.plan.summary?.endDate && (
        <div>
          {t("settings:loans.end")} : {day(loan.plan.summary?.endDate)}
        </div>
      )}
      {needsDate(loan) && (
        <div className="space-y-1">
          <div className="font-medium">{t("settings:documents.loan_first_due_ask")}</div>
          <Input
            type="date"
            className="w-44"
            value={firstDue}
            onChange={(e) => onFirstDue(e.target.value)}
          />
        </div>
      )}
      {problems.map((c) => (
        <div key={c.text} className="flex items-start gap-2 text-amber-600">
          <Icons.AlertTriangle className="mt-0.5 size-4 shrink-0" />
          <span>{c.text}</span>
        </div>
      ))}
    </div>
  );
}

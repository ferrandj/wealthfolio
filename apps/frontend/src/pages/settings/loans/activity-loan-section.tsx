import type { LoanPayment, LoanView } from "@/lib/types/statements";
import {
  Badge,
  Button,
  Label,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@wealthfolio/ui";
import { useTranslation } from "react-i18next";
import { useLoanMutations } from "./loan-answers";
import { DEBIT_TYPES, detachedAnswers, loanCostOf, loanLineOf, matchedAnswers } from "./loan-match";
import { loanLinkValue, useLoanLink } from "./loan-payment-picker";
import { day, euros } from "./loans-utils";

/** The instalment a debit pays, as found: matched, or proposed in doubt with what to confirm. */
function MatchedLine({ loan, line }: { loan: LoanView; line: LoanPayment }) {
  const { t } = useTranslation();
  const { answer } = useLoanMutations(loan);
  const sure = line.state === "found";
  return (
    <div className="grid gap-1.5 rounded-md border p-2 text-sm" data-testid="activity-loan-line">
      <div className="flex flex-wrap items-center gap-2">
        <Badge variant="outline" className={sure ? "" : "border-amber-500 text-amber-700"}>
          {sure
            ? t("settings:loans.payment_link.matched")
            : t("settings:loans.payment_link.proposed")}
        </Badge>
        <span>
          {t("settings:loans.activity.instalment", {
            number: line.number ?? "",
            date: day(line.date),
            amount: euros(line.expected),
          })}
        </span>
      </div>
      {line.note ? <p className="text-muted-foreground text-xs">{line.note}</p> : null}
      <div className="flex flex-wrap gap-2">
        {!sure || !line.confirmed ? (
          <Button
            type="button"
            size="sm"
            variant={sure ? "ghost" : "outline"}
            disabled={answer.isPending}
            onClick={() => answer.mutate(matchedAnswers(loan, line.date, line.activityId ?? ""))}
          >
            {t("settings:loans.payment_confirm")}
          </Button>
        ) : null}
        <Button
          type="button"
          size="sm"
          variant="ghost"
          disabled={answer.isPending}
          onClick={() => answer.mutate(detachedAnswers(loan, line.activityId ?? ""))}
        >
          {t("settings:loans.match.detach")}
        </Button>
      </div>
    </div>
  );
}

/**
 * In a transaction: which loan this debit belongs to. The instalment found by the app is shown (a doubt says so and
 * waits for a confirmation); the debit can be attached to another loan, as an instalment or as another operation of
 * the loan (a fee…), or said not to belong to any.
 */
export function ActivityLoanSection({
  activityId,
  activityType,
}: {
  activityId: string;
  activityType: string;
}) {
  const { t } = useTranslation();
  const { loans, items, change, hasLoans } = useLoanLink(activityId);
  if (![...DEBIT_TYPES, "FEE"].includes(activityType) || !hasLoans) return null;
  const hit = loanLineOf(loans, activityId);
  const cost = hit ? null : loanCostOf(loans, activityId);
  const value = hit ? hit.loan.key : cost ? loanLinkValue(cost.key, "cost") : "";
  return (
    <div className="grid gap-2" data-testid="activity-loan-section">
      <Label>{t("settings:loans.activity.title")}</Label>
      {hit ? <MatchedLine loan={hit.loan} line={hit.line} /> : null}
      <Select items={items} value={value} onValueChange={(v) => v && change.mutate(v)}>
        <SelectTrigger aria-label={t("settings:loans.payment_loan")} disabled={change.isPending}>
          <SelectValue placeholder={t("settings:loans.payment_choose_loan")} />
        </SelectTrigger>
        <SelectContent>
          {items.map((item) => (
            <SelectItem key={item.value} value={item.value}>
              {item.label}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </div>
  );
}

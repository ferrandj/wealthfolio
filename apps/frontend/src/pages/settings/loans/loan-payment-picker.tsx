import { attachStatementLoanPayment, getStatementLoans, setStatementLoanAnswers } from "@/adapters";
import { QueryKeys } from "@/lib/query-keys";
import type { LoanView, ReviewedOperation } from "@/lib/types/statements";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Badge,
  Button,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@wealthfolio/ui";
import { toast } from "@wealthfolio/ui/components/ui/use-toast";
import { useTranslation } from "react-i18next";
import { invalidateLoanCaches } from "./loan-caches";
import { costAnswers } from "./loan-match";

const NONE = "__none__";
const COST = "cost:";

/** The loans a payment can be attached to: every loan account, validated or not. */
export const loanChoices = (
  loans: {
    key: string;
    label: string;
    liabilityName?: string | null;
    liabilityPending?: boolean;
  }[],
  pendingWord: string,
) =>
  loans
    .filter((l) => l.key !== "?")
    .map((l) => ({
      value: l.key,
      label: `${l.liabilityName ?? l.label}${l.liabilityPending ? ` (${pendingWord})` : ""}`,
    }));

/** The value of the choice an operation has: an instalment of a loan, another operation of a loan, or nothing. */
export const loanLinkValue = (
  loanKey: string | null | undefined,
  state: string | null | undefined,
) => (!loanKey || state === "to_attach" ? "" : state === "cost" ? `${COST}${loanKey}` : loanKey);

/**
 * What a debit is to the loans: an instalment of one (matched to the instalment due nearest to it), another
 * operation of one — a fee, a guarantee, interim interest, attached but never matched to an instalment — or nothing.
 */
export function useLoanLink(activityId: string) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const { data: loans = [] } = useQuery({
    queryKey: [QueryKeys.STATEMENT_LOANS],
    queryFn: getStatementLoans,
  });
  const done = (result: LoanView[]) => {
    queryClient.setQueryData([QueryKeys.STATEMENT_LOANS], result);
    queryClient.invalidateQueries({ queryKey: [QueryKeys.STATEMENT_REVIEW] });
    queryClient.invalidateQueries({ queryKey: [QueryKeys.STATEMENT_OPERATIONS_REVIEW] });
    invalidateLoanCaches(queryClient);
  };
  const change = useMutation({
    mutationFn: async (value: string) => {
      if (value === NONE) return attachStatementLoanPayment(activityId, null);
      if (!value.startsWith(COST)) return attachStatementLoanPayment(activityId, value);
      // a fee of a loan: off every instalment first, then among the loan's other operations
      const key = value.slice(COST.length);
      const after = await attachStatementLoanPayment(activityId, null);
      const loan = after.find((l) => l.key === key);
      if (!loan) return after;
      return setStatementLoanAnswers(key, costAnswers(loan, activityId));
    },
    onSuccess: done,
    onError: (e) =>
      toast({ title: t("settings:loans.error"), description: String(e), variant: "destructive" }),
  });
  const pending = t("settings:loans.account_pending");
  const items = [
    ...loanChoices(loans, pending).flatMap((c) => [
      { value: c.value, label: t("settings:loans.link.instalment", { loan: c.label }) },
      { value: `${COST}${c.value}`, label: t("settings:loans.link.cost", { loan: c.label }) },
    ]),
    { value: NONE, label: t("settings:loans.payment_no_loan") },
  ];
  return { loans, items, change, hasLoans: items.length > 1 };
}

/**
 * Which loan an instalment debit pays: found by itself when a loan (even not validated yet) has an instalment of its
 * amount near its day, proposed when only the amount differs, and otherwise left to attach. A fee of the loan can be
 * attached to it without being an instalment.
 */
export function LoanPaymentPicker({ op }: { op: ReviewedOperation }) {
  const { t } = useTranslation();
  const { items, change } = useLoanLink(op.activityId ?? "");
  if (!op.loanState || !op.activityId) return null;
  const value = loanLinkValue(op.loanKey, op.loanState);
  return (
    <div className="flex w-full flex-wrap items-center gap-2" data-testid="loan-payment-picker">
      <Badge
        variant="outline"
        className={
          op.loanState === "matched" || op.loanState === "cost"
            ? ""
            : "border-amber-500 text-amber-700"
        }
      >
        {t(`settings:loans.payment_link.${op.loanState}`)}
      </Badge>
      <div className="min-w-0 flex-1">
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
      {op.loanState === "proposed" && op.loanKey && (
        <Button
          size="sm"
          variant="outline"
          disabled={change.isPending}
          onClick={() => change.mutate(op.loanKey ?? NONE)}
        >
          {t("settings:loans.payment_confirm")}
        </Button>
      )}
    </div>
  );
}

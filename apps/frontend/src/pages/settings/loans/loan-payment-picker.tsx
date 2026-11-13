import { attachStatementLoanPayment, getStatementLoans } from "@/adapters";
import { QueryKeys } from "@/lib/query-keys";
import type { ReviewedOperation } from "@/lib/types/statements";
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

const NONE = "__none__";

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

/**
 * Which loan an instalment debit pays: found by itself when a loan (even not validated yet) has an instalment of its
 * amount near its day, proposed when only the amount differs, and otherwise left to attach.
 */
export function LoanPaymentPicker({ op }: { op: ReviewedOperation }) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const { data: loans = [] } = useQuery({
    queryKey: [QueryKeys.STATEMENT_LOANS],
    queryFn: getStatementLoans,
  });
  const attach = useMutation({
    mutationFn: (key: string | null) => attachStatementLoanPayment(op.activityId ?? "", key),
    onSuccess: (result) => {
      queryClient.setQueryData([QueryKeys.STATEMENT_LOANS], result);
      queryClient.invalidateQueries({ queryKey: [QueryKeys.STATEMENT_REVIEW] });
      queryClient.invalidateQueries({ queryKey: [QueryKeys.STATEMENT_OPERATIONS_REVIEW] });
      invalidateLoanCaches(queryClient);
    },
    onError: (e) =>
      toast({ title: t("settings:loans.error"), description: String(e), variant: "destructive" }),
  });
  if (!op.loanState || !op.activityId) return null;
  const items = [
    ...loanChoices(loans, t("settings:loans.account_pending")),
    { value: NONE, label: t("settings:loans.payment_no_loan") },
  ];
  const value = op.loanState === "to_attach" ? "" : (op.loanKey ?? "");
  return (
    <div className="flex w-full flex-wrap items-center gap-2" data-testid="loan-payment-picker">
      <Badge
        variant="outline"
        className={op.loanState === "matched" ? "" : "border-amber-500 text-amber-700"}
      >
        {t(`settings:loans.payment_link.${op.loanState}`)}
      </Badge>
      <div className="min-w-0 flex-1">
        <Select
          items={items}
          value={value}
          onValueChange={(v) => v && attach.mutate(v === NONE ? null : v)}
        >
          <SelectTrigger aria-label={t("settings:loans.payment_loan")} disabled={attach.isPending}>
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
          disabled={attach.isPending}
          onClick={() => attach.mutate(op.loanKey ?? null)}
        >
          {t("settings:loans.payment_confirm")}
        </Button>
      )}
    </div>
  );
}

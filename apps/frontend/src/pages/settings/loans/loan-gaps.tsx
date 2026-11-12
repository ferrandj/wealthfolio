import {
  getStatementLoans,
  linkStatementLoanDocument,
  setStatementLoanAnswers,
  setStatementLoanFirstDue,
} from "@/adapters";
import { useAccounts } from "@/hooks/use-accounts";
import { QueryKeys } from "@/lib/query-keys";
import type { LoanAnswers, LoanGap, LoanView } from "@/lib/types/statements";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Badge,
  Button,
  Icons,
  Input,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@wealthfolio/ui";
import { toast } from "@wealthfolio/ui/components/ui/use-toast";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { invalidateLoanCaches } from "./loan-caches";
import { accountChoices } from "./loans-utils";

/** The answer a gap accepts, merged with what the person already told. */
export function answersWith(loan: LoanView, patch: Partial<LoanAnswers>): LoanAnswers {
  return { ...(loan.answers ?? {}), ...patch };
}

function useLoanMutations(loan: LoanView) {
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
  const attach = useMutation({
    mutationFn: async (loanKey: string) => {
      let loans: LoanView[] = [];
      for (const d of loan.documents.filter((d) => d.kind === "loan_event")) {
        loans = await linkStatementLoanDocument(d.documentId, loanKey);
      }
      return loans;
    },
    onSuccess: done,
    onError,
  });
  return { answer, firstDue, attach };
}

function FoundIn({ gap }: { gap: LoanGap }) {
  const { t } = useTranslation();
  if (gap.foundIn.length === 0) return null;
  return (
    <div className="flex flex-wrap items-center gap-1.5 text-xs">
      <span className="text-muted-foreground">{t("settings:loans.found_in")}</span>
      {gap.foundIn.map((kind) => (
        <Badge key={kind} variant="outline">
          {t(`settings:loans.found_in_kind.${kind}`, kind)}
        </Badge>
      ))}
    </div>
  );
}

function GapAnswer({ loan, gap }: { loan: LoanView; gap: LoanGap }) {
  const { t } = useTranslation();
  const { data: accounts = [] } = useAccountsList();
  const { answer, firstDue, attach } = useLoanMutations(loan);
  const { data: loans = [] } = useQuery({
    queryKey: [QueryKeys.STATEMENT_LOANS],
    queryFn: getStatementLoans,
    enabled: gap.answer === "link_loan",
  });
  const [date, setDate] = useState(loan.inferredFirstDue ?? "");
  const [account, setAccount] = useState("");
  const pending = answer.isPending || firstDue.isPending || attach.isPending;

  switch (gap.answer) {
    case "first_due":
      return (
        <div className="flex flex-wrap items-end gap-2">
          <div className="space-y-1">
            <div className="text-muted-foreground text-xs">{t("settings:loans.first_due")}</div>
            <Input
              type="date"
              className="w-44"
              value={date}
              onChange={(e) => setDate(e.target.value)}
            />
          </div>
          <Button size="sm" disabled={!date || pending} onClick={() => firstDue.mutate(date)}>
            {t("settings:loans.first_due_save")}
          </Button>
        </div>
      );
    case "debit_account": {
      const items = accountChoices(accounts, loan.payments?.lines ?? []);
      return (
        <div className="flex flex-wrap items-end gap-2">
          <div className="w-64">
            <Select items={items} value={account} onValueChange={(v) => setAccount(v ?? "")}>
              <SelectTrigger aria-label={t("settings:loans.debit_account")}>
                <SelectValue placeholder={t("settings:loans.debit_account_choose")} />
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
          <Button
            size="sm"
            disabled={!account || pending}
            onClick={() => answer.mutate(answersWith(loan, { debitAccountId: account }))}
          >
            {t("settings:loans.first_due_save")}
          </Button>
        </div>
      );
    }
    case "link_loan": {
      const items = loans
        .filter((l) => l.key !== "?")
        .map((l) => ({ value: l.key, label: l.label }));
      return (
        <div className="flex flex-wrap items-end gap-2">
          <div className="w-64">
            <Select items={items} value={account} onValueChange={(v) => setAccount(v ?? "")}>
              <SelectTrigger aria-label={t("settings:loans.link_loan_choose")}>
                <SelectValue placeholder={t("settings:loans.link_loan_choose")} />
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
          <Button size="sm" disabled={!account || pending} onClick={() => attach.mutate(account)}>
            {t("settings:loans.link_loan_save")}
          </Button>
        </div>
      );
    }
    case "no_early_repayment":
      return (
        <Button
          size="sm"
          variant="outline"
          disabled={pending}
          onClick={() => answer.mutate(answersWith(loan, { noEarlyRepayment: true }))}
        >
          {t("settings:loans.answer_no_early_repayment")}
        </Button>
      );
    case "payments_unchanged":
      return (
        <Button
          size="sm"
          variant="outline"
          disabled={pending}
          onClick={() => answer.mutate(answersWith(loan, { paymentsUnchanged: true }))}
        >
          {t("settings:loans.answer_payments_unchanged")}
        </Button>
      );
    default:
      return null;
  }
}

function useAccountsList() {
  const { accounts, isLoading } = useAccounts();
  return { data: accounts, isLoading };
}

/**
 * What the loan still lacks to be rebuilt, said plainly: each gap names the papers where the missing fact can be
 * found and, when the person can simply answer, offers the answer. A blocking gap keeps the loan from being validated.
 */
export function LoanGaps({ loan }: { loan: LoanView }) {
  const { t } = useTranslation();
  const gaps = loan.gaps ?? [];
  if (gaps.length === 0) return null;
  return (
    <div className="space-y-3" data-testid="loan-gaps">
      <div className="font-medium">{t("settings:loans.gaps_title")}</div>
      <ul className="space-y-3">
        {gaps.map((gap) => (
          <li key={gap.code} className="space-y-1.5 text-sm">
            <div className="flex items-start gap-2">
              {gap.blocking ? (
                <Icons.AlertTriangle className="text-destructive mt-0.5 size-4 shrink-0" />
              ) : (
                <Icons.Info className="mt-0.5 size-4 shrink-0 text-amber-600" />
              )}
              <div className="space-y-1.5">
                <span>{gap.text}</span>
                {gap.blocking && (
                  <div className="text-destructive text-xs">{t("settings:loans.gap_blocking")}</div>
                )}
                <FoundIn gap={gap} />
                <GapAnswer loan={loan} gap={gap} />
              </div>
            </div>
          </li>
        ))}
      </ul>
    </div>
  );
}

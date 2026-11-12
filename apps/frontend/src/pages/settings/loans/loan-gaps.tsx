import { useAccounts } from "@/hooks/use-accounts";
import type { LoanGap, LoanView } from "@/lib/types/statements";
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
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { LoanAccountDialog } from "./loan-account-dialog";
import { useLoanMutations, withConfirmed, answersWith } from "./loan-answers";
import { accountChoices } from "./loans-utils";

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
  const { answer, firstDue } = useLoanMutations(loan);
  const [attaching, setAttaching] = useState<string | null>(null);
  const [date, setDate] = useState(loan.inferredFirstDue ?? "");
  const [account, setAccount] = useState("");
  const pending = answer.isPending || firstDue.isPending;

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
    case "attach_account":
      return (
        <>
          <Button
            size="sm"
            disabled={loan.documents.length === 0}
            onClick={() => setAttaching(loan.documents[0]?.documentId ?? null)}
          >
            {t("settings:loans.account_open")}
          </Button>
          <LoanAccountDialog documentId={attaching} onClose={() => setAttaching(null)} />
        </>
      );
    case "confirm_payments": {
      const proposed = (loan.payments?.lines ?? []).filter(
        (l) => l.state === "proposed" && l.activityId,
      );
      return (
        <Button
          size="sm"
          variant="outline"
          disabled={pending || proposed.length === 0}
          onClick={() => answer.mutate(withConfirmed(loan, proposed))}
        >
          {t("settings:loans.answer_confirm_payments", { count: proposed.length })}
        </Button>
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

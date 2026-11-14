import { searchActivities } from "@/adapters";
import type { LoanPayment, LoanView } from "@/lib/types/statements";
import { useQuery } from "@tanstack/react-query";
import {
  Badge,
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
  Skeleton,
} from "@wealthfolio/ui";
import { useTranslation } from "react-i18next";
import { useLoanMutations } from "./loan-answers";
import {
  DEBIT_TYPES,
  debitCandidates,
  detachedAnswers,
  matchedAnswers,
  matchWindow,
} from "./loan-match";
import { day, euros } from "./loans-utils";

/**
 * Choose by hand the debit that pays one instalment: the debits around its due date, the same amount first. The one
 * chosen pays this instalment only (it leaves another one it paid); the current one can be said not to be this loan.
 */
export function LoanMatchDialog({
  loan,
  line,
  onClose,
}: {
  loan: LoanView;
  line: LoanPayment;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const { answer } = useLoanMutations(loan);
  const window = matchWindow(line.date);
  const { data, isLoading } = useQuery({
    queryKey: ["loan-match-candidates", loan.key, line.date],
    queryFn: () => searchActivities(0, 200, { activityTypes: DEBIT_TYPES, ...window }, ""),
  });
  const candidates = debitCandidates(data?.data ?? [], line, loan.payments?.lines ?? []);
  const choose = (activityId: string) =>
    answer.mutate(matchedAnswers(loan, line.date, activityId), { onSuccess: onClose });

  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="max-h-[90vh] overflow-y-auto sm:max-w-2xl">
        <DialogTitle>
          {t("settings:loans.match.title", { date: day(line.date), amount: euros(line.expected) })}
        </DialogTitle>
        <DialogDescription>{t("settings:loans.match.description")}</DialogDescription>
        {line.activityId ? (
          <div className="flex flex-wrap items-center justify-between gap-2 rounded-md border p-2 text-sm">
            <span>
              {t("settings:loans.match.current", {
                date: day(line.paidOn),
                amount: euros(line.amount),
              })}
            </span>
            <Button
              size="sm"
              variant="outline"
              disabled={answer.isPending}
              onClick={() =>
                answer.mutate(detachedAnswers(loan, line.activityId ?? ""), { onSuccess: onClose })
              }
            >
              {t("settings:loans.match.detach")}
            </Button>
          </div>
        ) : null}
        {isLoading ? (
          <Skeleton className="h-32" />
        ) : candidates.length === 0 ? (
          <p className="text-muted-foreground text-sm">{t("settings:loans.match.none")}</p>
        ) : (
          <ul className="divide-y rounded-md border">
            {candidates.map(({ activity, sameAmount, gap, usedFor }) => (
              <li
                key={activity.id}
                className="flex flex-wrap items-center justify-between gap-2 px-3 py-2 text-sm"
              >
                <div className="min-w-0">
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="font-medium">
                      {day(new Date(activity.date).toISOString().slice(0, 10))} ·{" "}
                      {euros(Math.abs(Number(activity.amount ?? 0)))}
                    </span>
                    {sameAmount ? (
                      <Badge variant="outline">{t("settings:loans.match.same_amount")}</Badge>
                    ) : (
                      <Badge variant="outline" className="border-amber-500 text-amber-700">
                        {t("settings:loans.match.other_amount")}
                      </Badge>
                    )}
                    {usedFor ? (
                      <Badge variant="outline" className="border-amber-500 text-amber-700">
                        {t("settings:loans.match.used_for", { date: day(usedFor) })}
                      </Badge>
                    ) : null}
                  </div>
                  <div className="text-muted-foreground truncate text-xs">
                    {[
                      activity.accountName,
                      activity.comment,
                      t("settings:loans.match.gap", { count: gap }),
                    ]
                      .filter(Boolean)
                      .join(" · ")}
                  </div>
                </div>
                <Button
                  size="sm"
                  variant={activity.id === line.activityId ? "default" : "outline"}
                  disabled={answer.isPending || activity.id === line.activityId}
                  onClick={() => choose(activity.id)}
                >
                  {activity.id === line.activityId
                    ? t("settings:loans.match.chosen")
                    : t("settings:loans.match.choose")}
                </Button>
              </li>
            ))}
          </ul>
        )}
      </DialogContent>
    </Dialog>
  );
}

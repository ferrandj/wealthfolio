import { getStatementCoverage, setStatementMonthNormal } from "@/adapters";
import { useAccounts } from "@/hooks/use-accounts";
import { QueryKeys } from "@/lib/query-keys";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Badge, Button, Card, CardContent, Icons } from "@wealthfolio/ui";
import { useTranslation } from "react-i18next";

/** Months without a statement and statements whose balances do not follow each other, account by account. */
export function CoverageCard() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const { accounts } = useAccounts({ filterActive: false, includeArchived: true });
  const { data } = useQuery({
    queryKey: [QueryKeys.STATEMENT_COVERAGE],
    queryFn: getStatementCoverage,
  });

  const mark = useMutation({
    mutationFn: (v: { accountId: string; month: string; normal: boolean }) =>
      setStatementMonthNormal(v.accountId, v.month, v.normal),
    onSuccess: (next) => queryClient.setQueryData([QueryKeys.STATEMENT_COVERAGE], next),
  });

  const open = (data ?? []).filter((c) => c.balanceBreaks.length > 0 || c.missingMonths.length > 0);
  if (open.length === 0) return null;
  const nameOf = (id: string) => accounts?.find((a) => a.id === id)?.name ?? id;

  return (
    <div className="space-y-3">
      <h2 className="text-lg font-semibold">{t("settings:documents.coverage_title")}</h2>
      {open.map((c) => (
        <Card key={c.accountId}>
          <CardContent className="space-y-3 p-4">
            <div className="flex items-center gap-2">
              <span className="font-medium">{nameOf(c.accountId)}</span>
              <span className="text-muted-foreground text-xs">
                {c.firstMonth} → {c.lastMonth}
              </span>
            </div>
            {c.missingMonths.length > 0 && (
              <div className="flex flex-wrap items-center gap-2">
                <span className="text-muted-foreground text-sm">
                  {t("settings:documents.coverage_missing")}
                </span>
                {c.missingMonths.map((m) => (
                  <span key={m.month} className="inline-flex items-center gap-1">
                    <Badge variant={m.normal ? "secondary" : "destructive"}>{m.month}</Badge>
                    <Button
                      type="button"
                      variant="ghost"
                      size="sm"
                      className="h-6 px-2 text-xs"
                      disabled={mark.isPending}
                      onClick={() =>
                        mark.mutate({
                          accountId: c.accountId,
                          month: m.month,
                          normal: !m.normal,
                        })
                      }
                    >
                      {m.normal
                        ? t("settings:documents.coverage_undo_normal")
                        : t("settings:documents.coverage_normal")}
                    </Button>
                  </span>
                ))}
              </div>
            )}
            {c.balanceBreaks.map((b) => (
              <div
                key={`${b.previousDocumentId}-${b.nextDocumentId}`}
                className="flex items-center gap-2 text-sm"
              >
                <Icons.AlertTriangle className="text-destructive size-4" />
                {t("settings:documents.coverage_break", {
                  expected: b.expected,
                  found: b.found,
                })}
              </div>
            ))}
          </CardContent>
        </Card>
      ))}
    </div>
  );
}

import { getStatementLoans, setStatementLoanFirstDue } from "@/adapters";
import { QueryKeys } from "@/lib/query-keys";
import type { LoanView } from "@/lib/types/statements";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Badge,
  Button,
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  Input,
  Separator,
  Skeleton,
} from "@wealthfolio/ui";
import { toast } from "@wealthfolio/ui/components/ui/use-toast";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { SettingsHeader } from "../settings-header";
import { LoanAssetLinkSection } from "./loan-asset-link";
import { invalidateLoanCaches } from "./loan-caches";
import { LoanChecks, LoanDocuments, LoanFacts, LoanFollow, LoanPlan } from "./loan-details";
import { LoanGaps } from "./loan-gaps";
import { problemCount } from "./loans-utils";

function FirstDue({ loan, onSave }: { loan: LoanView; onSave: (value: string | null) => void }) {
  const { t } = useTranslation();
  const [value, setValue] = useState(loan.firstDue ?? loan.inferredFirstDue ?? "");
  return (
    <div className="flex flex-wrap items-end gap-2">
      <div className="space-y-1">
        <div className="text-muted-foreground text-xs">{t("settings:loans.first_due")}</div>
        <Input
          type="date"
          className="w-44"
          value={value}
          onChange={(e) => setValue(e.target.value)}
        />
      </div>
      <Button size="sm" disabled={!value} onClick={() => onSave(value)}>
        {t("settings:loans.first_due_save")}
      </Button>
      {loan.firstDue && (
        <Button size="sm" variant="ghost" onClick={() => onSave(null)}>
          {t("settings:loans.first_due_forget")}
        </Button>
      )}
    </div>
  );
}

function LoanCard({
  loan,
  onFirstDue,
}: {
  loan: LoanView;
  onFirstDue: (v: string | null) => void;
}) {
  const { t } = useTranslation();
  const problems = problemCount(loan.plan.checks);
  return (
    <Card>
      <CardHeader className="flex flex-row flex-wrap items-center justify-between gap-2">
        <div>
          <CardTitle>{loan.label}</CardTitle>
        </div>
        <Badge variant={problems === 0 ? "secondary" : "destructive"}>
          {problems === 0
            ? t("settings:loans.consistent")
            : t("settings:loans.problems", { count: problems })}
        </Badge>
      </CardHeader>
      <CardContent className="space-y-4">
        <LoanFacts loan={loan} />

        <LoanGaps loan={loan} />
        {loan.firstDue && <FirstDue loan={loan} onSave={onFirstDue} />}
        <LoanFollow loan={loan} />

        <LoanChecks loan={loan} />

        {loan.liabilityId && (
          <>
            <Separator />
            <LoanAssetLinkSection loan={loan} />
          </>
        )}

        <Separator />
        <LoanDocuments loan={loan} />
        <LoanPlan loan={loan} />
      </CardContent>
    </Card>
  );
}

export default function LoansPage() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const { data, isLoading } = useQuery({
    queryKey: [QueryKeys.STATEMENT_LOANS],
    queryFn: getStatementLoans,
  });
  const firstDue = useMutation({
    mutationFn: (v: { key: string; firstDue: string | null }) =>
      setStatementLoanFirstDue(v.key, v.firstDue),
    onSuccess: (loans) => {
      queryClient.setQueryData([QueryKeys.STATEMENT_LOANS], loans);
      queryClient.invalidateQueries({
        queryKey: [QueryKeys.STATEMENT_DOCUMENTS],
      });
      invalidateLoanCaches(queryClient);
    },
    onError: (e) =>
      toast({
        title: t("settings:loans.error"),
        description: String(e),
        variant: "destructive",
      }),
  });
  const loans = data ?? [];
  return (
    <div className="space-y-6">
      <SettingsHeader heading={t("settings:loans.title")} text={t("settings:loans.subtitle")} />
      <Separator />
      {isLoading ? (
        <Skeleton className="h-40 w-full" />
      ) : loans.length === 0 ? (
        <p className="text-muted-foreground text-sm">{t("settings:loans.empty")}</p>
      ) : (
        loans.map((loan) => (
          <LoanCard
            key={loan.key}
            loan={loan}
            onFirstDue={(v) => firstDue.mutate({ key: loan.key, firstDue: v })}
          />
        ))
      )}
    </div>
  );
}

import { getStatementLoans, setStatementLoanFirstDue } from "@/adapters";
import { QueryKeys } from "@/lib/query-keys";
import type { LoanPlanCheck, LoanView } from "@/lib/types/statements";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Badge,
  Button,
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  Icons,
  Input,
  Separator,
  Skeleton,
} from "@wealthfolio/ui";
import { toast } from "@wealthfolio/ui/components/ui/use-toast";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { SettingsHeader } from "../settings-header";
import { day, euros, needsFirstDue, percent, problemCount } from "./loans-utils";

function Fact({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <div className="text-muted-foreground text-xs">{label}</div>
      <div className="font-medium">{value}</div>
    </div>
  );
}

function CheckLine({ check }: { check: LoanPlanCheck }) {
  const icon =
    check.status === "ok" ? (
      <Icons.CheckCircle className="mt-0.5 size-4 shrink-0 text-green-600" />
    ) : check.status === "warn" ? (
      <Icons.AlertTriangle className="mt-0.5 size-4 shrink-0 text-amber-600" />
    ) : (
      <Icons.Info className="text-muted-foreground mt-0.5 size-4 shrink-0" />
    );
  return (
    <li className="flex items-start gap-2 text-sm">
      {icon}
      <span>{check.text}</span>
    </li>
  );
}

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
  const [open, setOpen] = useState(false);
  const s = loan.plan.summary;
  const problems = problemCount(loan.plan.checks);
  return (
    <Card>
      <CardHeader className="flex flex-row flex-wrap items-center justify-between gap-2">
        <div>
          <CardTitle>{loan.label}</CardTitle>
          {loan.borrower && <div className="text-muted-foreground text-sm">{loan.borrower}</div>}
        </div>
        <Badge variant={problems === 0 ? "secondary" : "destructive"}>
          {problems === 0
            ? t("settings:loans.consistent")
            : t("settings:loans.problems", { count: problems })}
        </Badge>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="grid grid-cols-2 gap-4 md:grid-cols-4">
          <Fact label={t("settings:loans.borrowed")} value={euros(loan.principal)} />
          <Fact label={t("settings:loans.rate")} value={percent(loan.rate)} />
          <Fact label={t("settings:loans.taeg")} value={percent(loan.taeg)} />
          <Fact
            label={t("settings:loans.duration")}
            value={loan.duration ? t("settings:loans.months", { count: loan.duration }) : "—"}
          />
        </div>

        {s && (
          <>
            <Separator />
            <div className="grid grid-cols-2 gap-4 md:grid-cols-4">
              <Fact label={t("settings:loans.owed")} value={euros(s.owed)} />
              <Fact
                label={t("settings:loans.next_payment")}
                value={s.next ? `${euros(s.next.total)} · ${day(s.next.date)}` : "—"}
              />
              <Fact label={t("settings:loans.end")} value={day(s.endDate)} />
              <Fact
                label={t("settings:loans.installments")}
                value={t("settings:loans.installments_value", {
                  paid: s.paidCount,
                  remaining: s.remainingCount,
                })}
              />
              <Fact label={t("settings:loans.future_interest")} value={euros(s.futureInterest)} />
              <Fact label={t("settings:loans.future_insurance")} value={euros(s.futureInsurance)} />
              <Fact label={t("settings:loans.future_total")} value={euros(s.futureTotal)} />
              <Fact
                label={t("settings:loans.lifetime_interest")}
                value={euros(s.lifetimeInterest)}
              />
              {Number(s.repaidEarly) > 0 && (
                <Fact label={t("settings:loans.repaid_early")} value={euros(s.repaidEarly)} />
              )}
              {s.saved && (
                <Fact
                  label={t("settings:loans.saved")}
                  value={t("settings:loans.saved_value", {
                    interest: euros(s.saved.interest),
                    insurance: euros(s.saved.insurance),
                    months: s.saved.months,
                  })}
                />
              )}
            </div>
          </>
        )}

        {needsFirstDue(loan) && <FirstDue loan={loan} onSave={onFirstDue} />}

        {loan.plan.checks.length > 0 && (
          <>
            <Separator />
            <ul className="space-y-1.5">
              {loan.plan.checks.map((c, i) => (
                <CheckLine key={i} check={c} />
              ))}
            </ul>
          </>
        )}

        <Separator />
        <div className="space-y-1">
          <div className="text-muted-foreground text-xs">{t("settings:loans.documents")}</div>
          {loan.documents.map((d) => (
            <div key={d.documentId} className="flex flex-wrap items-center gap-2 text-sm">
              <Icons.FileText className="text-muted-foreground size-4" />
              <span>{d.fileName}</span>
              <Badge variant="outline">{t(`settings:loans.kind.${d.kind}`, d.kind)}</Badge>
              <Badge variant={d.status === "validated" ? "secondary" : "outline"}>
                {t(`settings:loans.status.${d.status}`, d.status)}
              </Badge>
            </div>
          ))}
        </div>

        <Button variant="outline" size="sm" onClick={() => setOpen(!open)}>
          {open ? t("settings:loans.hide_plan") : t("settings:loans.show_plan")}
        </Button>
        {open && (
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead className="text-muted-foreground text-left text-xs">
                <tr>
                  <th className="py-1 pr-3">{t("settings:loans.col_date")}</th>
                  <th className="pr-3 text-right">{t("settings:loans.col_capital")}</th>
                  <th className="pr-3 text-right">{t("settings:loans.col_interest")}</th>
                  <th className="pr-3 text-right">{t("settings:loans.col_insurance")}</th>
                  <th className="pr-3 text-right">{t("settings:loans.col_total")}</th>
                  <th className="pr-3 text-right">{t("settings:loans.col_remaining")}</th>
                  <th>{t("settings:loans.col_source")}</th>
                </tr>
              </thead>
              <tbody>
                {loan.plan.rows.map((r, i) => (
                  <tr
                    key={i}
                    className={
                      r.kind === "prepayment"
                        ? "bg-muted/60 font-medium"
                        : r.status === "next"
                          ? "bg-primary/5 font-medium"
                          : r.status === "paid"
                            ? "text-muted-foreground"
                            : ""
                    }
                    title={r.note || undefined}
                  >
                    <td className="py-1 pr-3">{r.date ? day(r.date) : `n° ${r.number ?? ""}`}</td>
                    <td className="pr-3 text-right">{euros(r.capital)}</td>
                    <td className="pr-3 text-right">{euros(r.interest)}</td>
                    <td className="pr-3 text-right">{euros(r.insurance)}</td>
                    <td className="pr-3 text-right">{euros(r.total)}</td>
                    <td className="pr-3 text-right">{euros(r.remaining)}</td>
                    <td>{r.kind === "prepayment" ? t("settings:loans.prepayment") : r.source}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
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
      queryClient.invalidateQueries({ queryKey: [QueryKeys.STATEMENT_DOCUMENTS] });
    },
    onError: (e) =>
      toast({ title: t("settings:loans.error"), description: String(e), variant: "destructive" }),
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

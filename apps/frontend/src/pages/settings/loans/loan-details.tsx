import type { LoanPlanCheck, LoanView } from "@/lib/types/statements";
import { Badge, Icons, Separator } from "@wealthfolio/ui";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@wealthfolio/ui";
import { day, euros, percent } from "./loans-utils";

export function Fact({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <div className="text-muted-foreground text-xs">{label}</div>
      <div className="font-medium">{value}</div>
    </div>
  );
}

export function CheckLine({ check }: { check: LoanPlanCheck }) {
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

/** Everything the papers say about the loan: terms, then where the plan stands. */
export function LoanFacts({ loan }: { loan: LoanView }) {
  const { t } = useTranslation();
  const s = loan.plan.summary;
  return (
    <>
      <div className="grid grid-cols-2 gap-4 md:grid-cols-4">
        {loan.product && (
          <Fact label={t("settings:loans.product")} value={loan.product} />
        )}
        {loan.borrower && (
          <Fact label={t("settings:loans.borrower")} value={loan.borrower} />
        )}
        <Fact
          label={t("settings:loans.borrowed")}
          value={euros(loan.principal)}
        />
        <Fact label={t("settings:loans.rate")} value={percent(loan.rate)} />
        <Fact label={t("settings:loans.taeg")} value={percent(loan.taeg)} />
        <Fact
          label={t("settings:loans.duration")}
          value={
            loan.duration
              ? t("settings:loans.months", { count: loan.duration })
              : "—"
          }
        />
      </div>

      {s && (
        <>
          <Separator />
          <div className="grid grid-cols-2 gap-4 md:grid-cols-4">
            <Fact label={t("settings:loans.owed")} value={euros(s.owed)} />
            <Fact
              label={t("settings:loans.next_payment")}
              value={
                s.next ? `${euros(s.next.total)} · ${day(s.next.date)}` : "—"
              }
            />
            <Fact label={t("settings:loans.end")} value={day(s.endDate)} />
            <Fact
              label={t("settings:loans.installments")}
              value={t("settings:loans.installments_value", {
                paid: s.paidCount,
                remaining: s.remainingCount,
              })}
            />
            <Fact
              label={t("settings:loans.future_interest")}
              value={euros(s.futureInterest)}
            />
            <Fact
              label={t("settings:loans.future_insurance")}
              value={euros(s.futureInsurance)}
            />
            <Fact
              label={t("settings:loans.future_total")}
              value={euros(s.futureTotal)}
            />
            <Fact
              label={t("settings:loans.lifetime_interest")}
              value={euros(s.lifetimeInterest)}
            />
            {Number(s.repaidEarly) > 0 && (
              <Fact
                label={t("settings:loans.repaid_early")}
                value={euros(s.repaidEarly)}
              />
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
    </>
  );
}

export function LoanChecks({ loan }: { loan: LoanView }) {
  if (loan.plan.checks.length === 0) return null;
  return (
    <>
      <Separator />
      <ul className="space-y-1.5">
        {loan.plan.checks.map((c, i) => (
          <CheckLine key={i} check={c} />
        ))}
      </ul>
    </>
  );
}

export function LoanDocuments({ loan }: { loan: LoanView }) {
  const { t } = useTranslation();
  return (
    <div className="space-y-1">
      <div className="text-muted-foreground text-xs">
        {t("settings:loans.documents")}
      </div>
      {loan.documents.map((d) => (
        <div
          key={d.documentId}
          className="flex flex-wrap items-center gap-2 text-sm"
        >
          <Icons.FileText className="text-muted-foreground size-4" />
          <span>{d.fileName}</span>
          <Badge variant="outline">
            {t(`settings:loans.kind.${d.kind}`, d.kind)}
          </Badge>
          <Badge variant={d.status === "validated" ? "secondary" : "outline"}>
            {t(`settings:loans.status.${d.status}`, d.status)}
          </Badge>
        </div>
      ))}
    </div>
  );
}

/** The full schedule, row by row; `defaultOpen` shows it without a click. */
export function LoanPlan({
  loan,
  defaultOpen = false,
}: {
  loan: LoanView;
  defaultOpen?: boolean;
}) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(defaultOpen);
  return (
    <>
      <Button variant="outline" size="sm" onClick={() => setOpen(!open)}>
        {open ? t("settings:loans.hide_plan") : t("settings:loans.show_plan")}
      </Button>
      {open && (
        <div className="max-h-96 overflow-auto">
          <table className="w-full text-sm">
            <thead className="bg-background text-muted-foreground sticky top-0 text-left text-xs">
              <tr>
                <th className="py-1 pr-3">{t("settings:loans.col_date")}</th>
                <th className="pr-3 text-right">
                  {t("settings:loans.col_capital")}
                </th>
                <th className="pr-3 text-right">
                  {t("settings:loans.col_interest")}
                </th>
                <th className="pr-3 text-right">
                  {t("settings:loans.col_insurance")}
                </th>
                <th className="pr-3 text-right">
                  {t("settings:loans.col_total")}
                </th>
                <th className="pr-3 text-right">
                  {t("settings:loans.col_remaining")}
                </th>
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
                  <td className="py-1 pr-3">
                    {r.date ? day(r.date) : `n° ${r.number ?? ""}`}
                  </td>
                  <td className="pr-3 text-right">{euros(r.capital)}</td>
                  <td className="pr-3 text-right">{euros(r.interest)}</td>
                  <td className="pr-3 text-right">{euros(r.insurance)}</td>
                  <td className="pr-3 text-right">{euros(r.total)}</td>
                  <td className="pr-3 text-right">{euros(r.remaining)}</td>
                  <td>
                    {r.kind === "prepayment"
                      ? t("settings:loans.prepayment")
                      : r.source}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </>
  );
}

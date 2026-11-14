import type { LoanPayment, LoanPlanCheck, LoanView } from "@/lib/types/statements";
import { Badge, Icons, Separator } from "@wealthfolio/ui";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@wealthfolio/ui";
import { useAccounts } from "@/hooks/use-accounts";
import { useLoanMutations, withConfirmed } from "./loan-answers";
import { day, euros, percent } from "./loans-utils";
import { LoanMatchDialog } from "./loan-match-dialog";
import { detachedAnswers } from "./loan-match";

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
        {loan.product && <Fact label={t("settings:loans.product")} value={loan.product} />}
        {loan.borrower && <Fact label={t("settings:loans.borrower")} value={loan.borrower} />}
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
            <Fact label={t("settings:loans.lifetime_interest")} value={euros(s.lifetimeInterest)} />
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
    </>
  );
}

/** How the loan is followed in the bank accounts: the date of instalment n°1, the account debited, what was found. */
export function LoanFollow({ loan }: { loan: LoanView }) {
  const { t } = useTranslation();
  const { accounts } = useAccounts({ filterActive: false });
  const payments = loan.payments;
  const account = accounts.find((a) => a.id === payments?.accountId);
  const source = loan.firstDueSource;
  if (!payments && !source) return null;
  return (
    <>
      <Separator />
      <div className="grid grid-cols-2 gap-4 md:grid-cols-4" data-testid="loan-follow">
        {source && (
          <Fact
            label={t("settings:loans.first_due_origin")}
            value={t(`settings:loans.first_due_source.${source}`)}
          />
        )}
        <Fact
          label={t("settings:loans.debit_account")}
          value={
            account
              ? `${account.name} (${t(`settings:loans.debit_account_source.${payments?.accountSource ?? "statements"}`)})`
              : t("settings:loans.debit_account_unknown")
          }
        />
        {payments && payments.expected > 0 && (
          <Fact
            label={t("settings:loans.payments_found_label")}
            value={t("settings:loans.payments_found", {
              found: payments.found,
              expected: payments.expected,
            })}
          />
        )}
      </div>
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
      <div className="text-muted-foreground text-xs">{t("settings:loans.documents")}</div>
      {loan.documents.map((d) => (
        <div key={d.documentId} className="flex flex-wrap items-center gap-2 text-sm">
          <Icons.FileText className="text-muted-foreground size-4" />
          <span>{d.fileName}</span>
          <Badge variant="outline">{t(`settings:loans.kind.${d.kind}`, d.kind)}</Badge>
          <Badge variant={d.status.toLowerCase() === "validated" ? "secondary" : "outline"}>
            {t(`settings:loans.status.${d.status.toLowerCase()}`, d.status)}
          </Badge>
        </div>
      ))}
    </div>
  );
}

function PaymentCell({
  line,
  account,
  onConfirm,
  confirming,
}: {
  line?: LoanPayment;
  account?: string;
  onConfirm?: () => void;
  confirming?: boolean;
}) {
  const { t } = useTranslation();
  if (!line) return null;
  if (line.state === "proposed") {
    return (
      <span className="inline-flex flex-wrap items-center gap-1.5 text-amber-600">
        <Icons.AlertTriangle className="size-3.5 shrink-0" />
        <span>
          {t("settings:loans.payment_proposed", {
            date: day(line.paidOn),
            amount: euros(line.amount),
          })}
          {account ? ` · ${account}` : ""}
        </span>
        {onConfirm && (
          <Button size="sm" variant="outline" disabled={confirming} onClick={onConfirm}>
            {t("settings:loans.payment_confirm")}
          </Button>
        )}
      </span>
    );
  }
  if (line.state === "found") {
    return (
      <span className="inline-flex items-center gap-1 text-green-600">
        <Icons.CheckCircle className="size-3.5 shrink-0" />
        <span className="text-foreground">
          {day(line.paidOn)} · {euros(line.amount)}
          {account ? ` · ${account}` : ""}
          {line.note && <span className="text-muted-foreground block text-xs">{line.note}</span>}
        </span>
      </span>
    );
  }
  const tone = line.state === "missing" ? "text-destructive" : "text-muted-foreground";
  return <span className={tone}>{t(`settings:loans.payment_state.${line.state}`)}</span>;
}

/** The full schedule, row by row; `defaultOpen` shows it without a click. */
export function LoanPlan({ loan, defaultOpen = false }: { loan: LoanView; defaultOpen?: boolean }) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(defaultOpen);
  const { accounts } = useAccounts({ filterActive: false });
  const payments = new Map((loan.payments?.lines ?? []).map((l) => [l.row, l]));
  const { answer } = useLoanMutations(loan);
  const [matching, setMatching] = useState<LoanPayment | null>(null);
  return (
    <>
      {matching ? (
        <LoanMatchDialog loan={loan} line={matching} onClose={() => setMatching(null)} />
      ) : null}
      {loan.payments?.costs?.length ? (
        <div className="grid gap-1 text-sm" data-testid="loan-costs">
          <div className="text-muted-foreground text-xs font-medium">
            {t("settings:loans.costs_title")}
          </div>
          {loan.payments.costs.map((c) => (
            <div key={c.activityId} className="flex flex-wrap items-center gap-2">
              <span>
                {day(c.date)} · {euros(c.amount)}
              </span>
              <span className="text-muted-foreground truncate text-xs">
                {[c.label, accounts.find((a) => a.id === c.accountId)?.name]
                  .filter(Boolean)
                  .join(" · ")}
              </span>
              <Button
                size="sm"
                variant="ghost"
                className="h-6 px-2 text-xs"
                disabled={answer.isPending}
                onClick={() => answer.mutate(detachedAnswers(loan, c.activityId))}
              >
                {t("settings:loans.match.detach")}
              </Button>
            </div>
          ))}
        </div>
      ) : null}
      <Button variant="outline" size="sm" onClick={() => setOpen(!open)}>
        {open ? t("settings:loans.hide_plan") : t("settings:loans.show_plan")}
      </Button>
      {open && (
        <div className="max-h-96 overflow-auto">
          <table className="w-full text-sm">
            <thead className="bg-background text-muted-foreground sticky top-0 text-left text-xs">
              <tr>
                <th className="py-1 pr-3">{t("settings:loans.col_date")}</th>
                <th className="pr-3 text-right">{t("settings:loans.col_capital")}</th>
                <th className="pr-3 text-right">{t("settings:loans.col_interest")}</th>
                <th className="pr-3 text-right">{t("settings:loans.col_insurance")}</th>
                <th className="pr-3 text-right">{t("settings:loans.col_total")}</th>
                <th className="pr-3 text-right">{t("settings:loans.col_remaining")}</th>
                <th className="pr-3">{t("settings:loans.col_source")}</th>
                <th>{t("settings:loans.col_payment")}</th>
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
                  <td className="pr-3">
                    {r.kind === "prepayment" ? t("settings:loans.prepayment") : r.source}
                  </td>
                  <td>
                    <PaymentCell
                      confirming={answer.isPending}
                      onConfirm={() => {
                        const line = payments.get(i);
                        if (line) answer.mutate(withConfirmed(loan, [line]));
                      }}
                      line={payments.get(i)}
                      account={accounts.find((a) => a.id === payments.get(i)?.accountId)?.name}
                    />
                    {payments.get(i) ? (
                      <Button
                        size="sm"
                        variant="ghost"
                        className="ms-1 h-6 px-2 text-xs"
                        onClick={() => setMatching(payments.get(i) ?? null)}
                      >
                        {payments.get(i)?.activityId
                          ? t("settings:loans.match.change")
                          : t("settings:loans.match.open")}
                      </Button>
                    ) : null}
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

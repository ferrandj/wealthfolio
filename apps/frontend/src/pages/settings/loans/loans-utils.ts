import type { LoanPlanCheck, LoanView } from "@/lib/types/statements";

const money = new Intl.NumberFormat("fr-FR", { style: "currency", currency: "EUR" });

export const euros = (v: number | string | null | undefined): string =>
  v === null || v === undefined || v === "" ? "—" : money.format(Number(v));

/** 2026-06-15 → 15/06/2026, without going through a time zone. */
export const day = (iso: string | null | undefined): string => {
  if (!iso) return "—";
  const [y, m, d] = iso.split("-");
  return y && m && d ? `${d}/${m}/${y}` : iso;
};

export const percent = (v: number | string | null | undefined): string =>
  v === null || v === undefined ? "—" : `${Number(v).toLocaleString("fr-FR")} %`;

export const problemCount = (checks: LoanPlanCheck[]): number =>
  checks.filter((c) => c.status === "warn").length;

/** The loan has papers but not enough to date its instalments: the person can type the first due date. */
export const needsFirstDue = (loan: LoanView): boolean =>
  loan.plan.rows.some((r) => r.date === null);

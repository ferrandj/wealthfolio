import type { AlternativeAssetHolding } from "@/lib/types";
import type { LoanPlanCheck, LoanView, NewLoanAsset } from "@/lib/types/statements";

const money = new Intl.NumberFormat("fr-FR", {
  style: "currency",
  currency: "EUR",
});

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

/** The assets a loan can finance: a property, a vehicle or another asset (never another liability). */
export const attachableAssets = (holdings: AlternativeAssetHolding[]): AlternativeAssetHolding[] =>
  holdings.filter((h) => ["PROPERTY", "VEHICLE", "OTHER"].includes(h.kind.toUpperCase()));

/** The lines of a stored address ("street\npostal code city"). */
export const addressLines = (address: unknown): string[] =>
  typeof address === "string"
    ? address
        .split("\n")
        .map((l) => l.trim())
        .filter(Boolean)
    : [];

export interface NewAssetForm {
  kind: NewLoanAsset["kind"];
  name: string;
  street: string;
  postalCode: string;
  city: string;
  value: string;
}

/** The asset to create from the form; `null` while the name or the value is missing or not a number. */
export function newAssetOf(form: NewAssetForm): NewLoanAsset | null {
  const name = form.name.trim();
  const value = Number(form.value.replace(/\s/g, "").replace(",", "."));
  if (!name || form.value.trim() === "" || !Number.isFinite(value) || value < 0) return null;
  const text = (v: string) => (v.trim() === "" ? null : v.trim());
  return {
    kind: form.kind,
    name,
    street: text(form.street),
    postalCode: text(form.postalCode),
    city: text(form.city),
    value,
  };
}

/**
 * The accounts the instalments can be debited from, those where a matching debit was found first. Each choice is an
 * account id and its name.
 */
export const accountChoices = (
  accounts: { id: string; name: string }[],
  lines: { accountId?: string | null }[],
): { value: string; label: string }[] => {
  const seen = new Set(lines.map((l) => l.accountId).filter((id): id is string => !!id));
  const ranked = [...accounts].sort((a, b) => Number(seen.has(b.id)) - Number(seen.has(a.id)));
  return ranked.map((a) => ({ value: a.id, label: a.name }));
};

/** The loan a paper belongs to. */
export function loanOfDocument(loans: LoanView[] | undefined, documentId: string) {
  return loans?.find((l) => l.documents.some((d) => d.documentId === documentId));
}

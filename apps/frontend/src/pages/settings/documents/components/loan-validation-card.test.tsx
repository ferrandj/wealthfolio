import type { LoanView } from "@/lib/types/statements";
import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { LoanValidationCard, loanOfDocument, needsDate } from "./loan-validation-card";

vi.mock("react-i18next", () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}));

const loan = (over: Partial<LoanView> = {}): LoanView =>
  ({
    key: "FT1",
    label: "Prêt FT1",
    borrower: "M. Test",
    product: "Prêt Habitat",
    principal: 122432,
    duration: 243,
    rate: 3.1,
    taeg: 3.5,
    documents: [
      {
        documentId: "d1",
        kind: "loan_offer",
        fileName: "offre.pdf",
        status: "pending",
      },
    ],
    firstDue: null,
    inferredFirstDue: null,
    plan: {
      rows: [],
      checks: [{ status: "ok", text: "Contrat cohérent" }],
      summary: null,
    },
    ...over,
  }) as unknown as LoanView;

describe("loan validation card", () => {
  it("shows the terms and checks of the paper, not an error", () => {
    render(<LoanValidationCard loan={loan()} />);
    expect(screen.getByText("Prêt Habitat")).toBeTruthy();
    expect(screen.getByText("M. Test")).toBeTruthy();
    expect(screen.getByText("Contrat cohérent")).toBeTruthy();
    expect(screen.getByText("settings:loans.rate")).toBeTruthy();
    expect(screen.getByText("settings:loans.taeg")).toBeTruthy();
    expect(screen.queryByText("settings:documents.loan_unknown")).toBeNull();
  });

  it("tells the bank statements will date the instalments instead of asking for a date", () => {
    render(<LoanValidationCard loan={loan()} />);
    expect(screen.getByText("settings:loans.dates_from_statements")).toBeTruthy();
    expect(document.querySelector('input[type="date"]')).toBeNull();
  });

  it("knows when no paper dates the schedule", () => {
    expect(needsDate(loan())).toBe(true);
    expect(needsDate(loan({ inferredFirstDue: "2020-01-05" }))).toBe(false);
    expect(needsDate(undefined)).toBe(false);
  });

  it("waits while loading and says unknown only once loaded", () => {
    const { rerender } = render(<LoanValidationCard loan={undefined} loading />);
    expect(screen.queryByText("settings:documents.loan_unknown")).toBeNull();
    rerender(<LoanValidationCard loan={undefined} />);
    expect(screen.getByText("settings:documents.loan_unknown")).toBeTruthy();
  });

  it("finds the loan of a document", () => {
    expect(loanOfDocument([loan()], "d1")?.key).toBe("FT1");
    expect(loanOfDocument([loan()], "other")).toBeUndefined();
  });
});

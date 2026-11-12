import type { LoanView } from "@/lib/types/statements";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render as plainRender, screen } from "@testing-library/react";
import type { ReactElement } from "react";
import { describe, expect, it, vi } from "vitest";
import { LoanValidationCard, blockingGaps, loanOfDocument } from "./loan-validation-card";

vi.mock("@/adapters", () => ({
  getStatementLoans: vi.fn(),
  attachStatementLoanDocument: vi.fn(),
  getAlternativeHoldings: vi.fn(),
  setStatementLoanAnswers: vi.fn(),
  setStatementLoanFirstDue: vi.fn(),
}));
vi.mock("@/hooks/use-accounts", () => ({
  useAccounts: () => ({ accounts: [], isLoading: false }),
}));
vi.mock("react-i18next", () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}));

const render = (ui: ReactElement) =>
  plainRender(<QueryClientProvider client={new QueryClient()}>{ui}</QueryClientProvider>);

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

  it("lists what is missing with the papers where to find it, and blocks while it is blocking", () => {
    const missing = loan({
      gaps: [
        {
          code: "first_due",
          text: "La date de la première échéance est inconnue",
          foundIn: ["loan_event", "statement"],
          blocking: true,
          answer: "first_due",
        },
      ],
    });
    render(<LoanValidationCard loan={missing} />);
    expect(screen.getByText("La date de la première échéance est inconnue")).toBeTruthy();
    expect(screen.getByText("settings:loans.found_in_kind.loan_event")).toBeTruthy();
    expect(screen.getByText("settings:loans.found_in_kind.statement")).toBeTruthy();
    expect(blockingGaps(missing)).toHaveLength(1);
    expect(blockingGaps(loan())).toHaveLength(0);
    expect(blockingGaps(undefined)).toHaveLength(0);
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

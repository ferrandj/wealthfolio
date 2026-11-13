import type { OperationToReview } from "@/lib/types/statements";
import { describe, expect, it } from "vitest";
import { loanChoices } from "@/pages/settings/loans/loan-payment-picker";
import { countByKind } from "./operations-review-dialog";

describe("operations review", () => {
  it("counts what each operation needs", () => {
    const item = (why: string[]) => ({ why }) as unknown as OperationToReview;
    expect(
      countByKind([item(["doubt"]), item(["doubt", "loan_to_attach"]), item(["to_categorize"])]),
    ).toEqual({ doubt: 2, to_categorize: 1, loan_to_attach: 1, loan_proposed: 0 });
  });

  it("offers every loan account, the pending ones said so", () => {
    expect(
      loanChoices(
        [
          { key: "?", label: "Sans référence" },
          {
            key: "FT1",
            label: "Fiximmo FT1",
            liabilityName: "Prêt maison",
            liabilityPending: true,
          },
          { key: "AUTO", label: "Crédit auto" },
        ],
        "non validé",
      ),
    ).toEqual([
      { value: "FT1", label: "Prêt maison (non validé)" },
      { value: "AUTO", label: "Crédit auto" },
    ]);
  });
});

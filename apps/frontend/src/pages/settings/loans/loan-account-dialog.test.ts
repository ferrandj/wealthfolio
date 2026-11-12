import type { AlternativeAssetHolding } from "@/lib/types";
import { describe, expect, it } from "vitest";
import { loanAccounts } from "./loan-account-dialog";

describe("loan accounts", () => {
  it("offers the liabilities only", () => {
    const holdings = [
      { id: "house", kind: "property", name: "Maison" },
      {
        id: "loan",
        kind: "liability",
        name: "Prêt maison",
        metadata: { statement_loan_status: "pending" },
      },
      { id: "car", kind: "LIABILITY", name: "Crédit auto" },
    ] as unknown as AlternativeAssetHolding[];
    expect(loanAccounts(holdings).map((h) => h.id)).toEqual(["loan", "car"]);
  });
});

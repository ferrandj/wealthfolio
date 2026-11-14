import { describe, expect, it } from "vitest";
import { isInvestmentCategory, isOwnTransferCategory } from "../statement-review";
import { accountNamedIn, bestCandidate } from "./own-transfer-account";

describe("own transfer account", () => {
  it("knows the categories that send money to another own account", () => {
    expect(isOwnTransferCategory("income_sources", "cat_income_transfers_own")).toBe(true);
    expect(isOwnTransferCategory("spending_categories", "cat_transfers_own")).toBe(true);
    expect(isInvestmentCategory("spending_categories", "cat_investments_life")).toBe(true);
    expect(isInvestmentCategory("spending_categories", "cat_housing_notary")).toBe(false);
  });

  it("finds the account a label names by its number", () => {
    const ids = [
      { accountId: "av", accountNumber: "734639427101", iban: null },
      { accountId: "cc", accountNumber: null, iban: "FR76 1451 8292 6707 3463 9424 025" },
    ];
    expect(accountNamedIn("VRST FORTUNEO VIE 734639427101", ids)).toBe("av");
    expect(accountNamedIn("VIR VERS 67073463942", ids)).toBeNull();
    expect(accountNamedIn("VIR 07346394240 25", ids)).toBe("cc");
  });

  it("proposes in one click only the single certain candidate", () => {
    const c = (confidence: string) => ({ confidence }) as never;
    expect(bestCandidate([c("high"), c("low")])).toEqual({ confidence: "high" });
    expect(bestCandidate([c("high"), c("high")])).toBeNull();
    expect(bestCandidate([c("medium")])).toBeNull();
  });
});

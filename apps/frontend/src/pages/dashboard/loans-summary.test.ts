import { describe, expect, it } from "vitest";
import type { AlternativeAssetHolding } from "@/lib/types";
import { loansOf } from "./loans-summary";

const holding = (id: string, kind: string) =>
  ({ id, kind, name: id, currency: "EUR", marketValue: "-1485.75" }) as AlternativeAssetHolding;

describe("loansOf", () => {
  it("keeps the liabilities only", () => {
    const loans = loansOf([holding("house", "property"), holding("loan", "LIABILITY")]);
    expect(loans.map((l) => l.id)).toEqual(["loan"]);
  });
});

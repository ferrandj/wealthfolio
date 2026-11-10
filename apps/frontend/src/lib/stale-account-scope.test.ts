import { describe, expect, it } from "vitest";
import { dropStaleScope } from "./stale-account-scope";

const accounts = new Set(["a", "b"]);
const portfolios = new Set(["p"]);

describe("dropStaleScope", () => {
  it("keeps a scope that still exists", () => {
    const scope = { type: "account", accountId: "a" } as const;
    expect(dropStaleScope(scope, accounts, portfolios)).toBe(scope);
    expect(dropStaleScope({ type: "all" }, accounts, portfolios)).toEqual({ type: "all" });
  });

  it("falls back to all accounts when the account or portfolio is gone", () => {
    expect(dropStaleScope({ type: "account", accountId: "x" }, accounts, portfolios)).toEqual({
      type: "all",
    });
    expect(dropStaleScope({ type: "portfolio", portfolioId: "x" }, accounts, portfolios)).toEqual({
      type: "all",
    });
  });

  it("drops only the missing accounts of a multi-account scope", () => {
    expect(
      dropStaleScope({ type: "accounts", accountIds: ["a", "x"] }, accounts, portfolios),
    ).toEqual({ type: "accounts", accountIds: ["a"] });
    expect(dropStaleScope({ type: "accounts", accountIds: ["x"] }, accounts, portfolios)).toEqual({
      type: "all",
    });
  });
});

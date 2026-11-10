import type { AccountScope } from "@/lib/types";

/**
 * The scope with everything that no longer exists dropped. Selections live for the whole session, so an account
 * deleted in the meantime (or a portfolio) must not keep failing every page that reads the selection.
 * Returns the same object when nothing is stale.
 */
export function dropStaleScope(
  scope: AccountScope,
  accountIds: ReadonlySet<string>,
  portfolioIds: ReadonlySet<string>,
): AccountScope {
  switch (scope.type) {
    case "all":
      return scope;
    case "account":
      return accountIds.has(scope.accountId) ? scope : { type: "all" };
    case "portfolio":
      return portfolioIds.has(scope.portfolioId) ? scope : { type: "all" };
    case "accounts": {
      const kept = scope.accountIds.filter((id) => accountIds.has(id));
      if (kept.length === scope.accountIds.length) return scope;
      return kept.length === 0 ? { type: "all" } : { type: "accounts", accountIds: kept };
    }
  }
}

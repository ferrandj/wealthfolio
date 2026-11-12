import { QueryKeys } from "@/lib/query-keys";
import type { QueryClient } from "@tanstack/react-query";

/**
 * Everything a loan touches: the loans themselves, their liability (Passifs on the Holdings page, the net worth)
 * and the account held for them. Whatever changes the papers or the answers about a loan must refresh all of it,
 * or the liability stays invisible until the cache expires.
 */
export function invalidateLoanCaches(queryClient: QueryClient) {
  return Promise.all(
    [
      QueryKeys.STATEMENT_LOANS,
      QueryKeys.ALTERNATIVE_HOLDINGS,
      QueryKeys.NET_WORTH,
      QueryKeys.ACCOUNTS,
    ].map((key) => queryClient.invalidateQueries({ queryKey: [key] })),
  );
}

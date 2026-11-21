import { getAlternativeHoldings } from "@/adapters";
import { DashboardCard } from "@/components/dashboard-card";
import { useBalancePrivacy } from "@/hooks/use-balance-privacy";
import { QueryKeys } from "@/lib/query-keys";
import type { AlternativeAssetHolding } from "@/lib/types";
import { useQuery } from "@tanstack/react-query";
import { AmountDisplay } from "@wealthfolio/ui";
import {
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemTitle,
} from "@wealthfolio/ui/components/ui/item";
import { useTranslation } from "react-i18next";
import { Link } from "react-router-dom";

/** The loans (liabilities), what is still owed on each: shown beside the accounts. */
export const loansOf = (holdings: AlternativeAssetHolding[]) =>
  holdings.filter((h) => h.kind.toLowerCase() === "liability");

/**
 * The credits beside the accounts: each loan account (a liability, rebuilt from the bank's papers or typed by the
 * person) with the capital still owed. Opens the loan's page.
 */
export function LoansSummary() {
  const { t } = useTranslation();
  const { isBalanceHidden } = useBalancePrivacy();
  const { data } = useQuery({
    queryKey: [QueryKeys.ALTERNATIVE_HOLDINGS],
    queryFn: getAlternativeHoldings,
  });
  const loans = loansOf(data ?? []);
  if (loans.length === 0) return null;
  return (
    <DashboardCard title={t("dashboard:loans.title")}>
      {loans.map((loan) => (
        <Item
          key={loan.id}
          size="sm"
          render={<Link to={`/holdings/${encodeURIComponent(loan.id)}`} />}
          className="-mx-2 flex-nowrap gap-3 border-0 px-2"
        >
          <ItemContent className="min-w-0 gap-0">
            <ItemTitle className="block w-full truncate text-base font-semibold">
              {loan.name}
            </ItemTitle>
            <ItemDescription className="truncate text-sm">
              {t("dashboard:loans.still_owed")}
            </ItemDescription>
          </ItemContent>
          <ItemActions className="shrink-0">
            <AmountDisplay
              value={Math.abs(Number(loan.marketValue) || 0)}
              currency={loan.currency}
              isHidden={isBalanceHidden}
              className="text-base font-semibold"
            />
          </ItemActions>
        </Item>
      ))}
    </DashboardCard>
  );
}

import { useBalancePrivacy } from "@/hooks/use-balance-privacy";
import { useIsMobileViewport } from "@/hooks/use-platform";
import { useAmountFormatting } from "@wealthfolio/ui";

/**
 * The value axis of a money chart: compact amounts in the chosen currency ("12 k€", "$1.2M"), masked when balances are
 * hidden. Spread into a Recharts `YAxis`.
 */
export function useCurrencyAxis(currency: string | undefined) {
  const { formatCompactAmount } = useAmountFormatting();
  const { isBalanceHidden } = useBalancePrivacy();
  const isMobile = useIsMobileViewport();
  return {
    tickLine: false,
    axisLine: false,
    tick: { fontSize: isMobile ? "0.6875rem" : "0.75rem", fill: "var(--muted-foreground)" },
    width: isMobile ? 48 : 64,
    tickCount: 4,
    tickFormatter: (value: number) =>
      isBalanceHidden || !currency ? "••" : formatCompactAmount(value, currency, false),
  };
}

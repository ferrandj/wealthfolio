import type { Asset } from "@/lib/types";

/** Assets with no market price of their own: a euro fund of a life insurance, priced by the insurer's documents. */
const NO_MARKET_PRICE = /\b(fds|fonds)\s+euros?\b|\bfonds?\s+en\s+euros?\b|\beuro fund\b/i;

/** The assets whose prices are entered by hand (or by documents) and could follow the market instead. */
export function manualAssets(assets: Asset[]): Asset[] {
  return assets.filter((a) => a.quoteMode === "MANUAL" && a.kind === "INVESTMENT");
}

/** Left out by default: the assets that have no market price (a euro fund…). */
export function defaultExclusions(assets: Asset[]): string[] {
  return assets
    .filter((a) => NO_MARKET_PRICE.test(`${a.name ?? ""} ${a.displayCode ?? ""}`))
    .map((a) => a.id);
}

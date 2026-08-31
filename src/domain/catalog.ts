import type { Contract, Market } from "./types";

interface RawSymbol {
  symbol?: string;
  pair?: string;
  contractType?: string;
  status?: string;
  contractStatus?: string;
  baseAsset?: string;
  quoteAsset?: string;
  marginAsset?: string;
  pricePrecision?: number;
  filters?: Array<{ filterType?: string; tickSize?: string }>;
}

export function parseExchangeInfo(market: Market, payload: unknown): Contract[] {
  if (!payload || typeof payload !== "object" || !("symbols" in payload)) return [];
  const symbols = (payload as { symbols?: unknown }).symbols;
  if (!Array.isArray(symbols)) return [];

  return symbols
    .filter((item): item is RawSymbol => Boolean(item && typeof item === "object"))
    .filter((item) => item.contractType === "PERPETUAL")
    .filter((item) => (market === "coinm" ? item.contractStatus : item.status) === "TRADING")
    .filter((item) => Boolean(item.symbol && item.baseAsset && item.quoteAsset && item.marginAsset))
    .map((item) => {
      const tickSize = item.filters?.find((filter) => filter.filterType === "PRICE_FILTER")?.tickSize ?? "1";
      const fraction = tickSize.split(".")[1]?.replace(/0+$/, "") ?? "";
      return {
        market,
        symbol: item.symbol!,
        baseAsset: item.baseAsset!,
        quoteAsset: item.quoteAsset!,
        marginAsset: item.marginAsset!,
        contractType: "PERPETUAL",
        status: "TRADING",
        tickSize,
        priceDecimals: fraction.length || item.pricePrecision || 0,
      };
    });
}

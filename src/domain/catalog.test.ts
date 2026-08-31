import { describe, expect, it } from "vitest";
import { parseExchangeInfo } from "./catalog";

const common = {
  baseAsset: "BTC",
  quoteAsset: "USDT",
  marginAsset: "USDT",
  filters: [{ filterType: "PRICE_FILTER", tickSize: "0.10" }],
};

describe("parseExchangeInfo", () => {
  it("accepts only trading USD-M perpetuals", () => {
    const result = parseExchangeInfo("usdm", {
      symbols: [
        { ...common, symbol: "BTCUSDT", contractType: "PERPETUAL", status: "TRADING" },
        { ...common, symbol: "BTCUSDT_2609", contractType: "CURRENT_QUARTER", status: "TRADING" },
        { ...common, symbol: "OLDUSDT", contractType: "PERPETUAL", status: "SETTLING" },
      ],
    });
    expect(result.map((item) => item.symbol)).toEqual(["BTCUSDT"]);
    expect(result[0]?.priceDecimals).toBe(1);
  });

  it("uses contractStatus for COIN-M", () => {
    const result = parseExchangeInfo("coinm", {
      symbols: [
        {
          ...common,
          symbol: "BTCUSD_PERP",
          contractType: "PERPETUAL",
          contractStatus: "TRADING",
          quoteAsset: "USD",
          marginAsset: "BTC",
        },
      ],
    });
    expect(result[0]).toMatchObject({ market: "coinm", symbol: "BTCUSD_PERP" });
  });
});

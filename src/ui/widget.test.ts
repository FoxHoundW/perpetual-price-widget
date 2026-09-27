import { describe, expect, it } from "vitest";
import type { TickerSnapshot } from "../domain/types";
import { renderWidget } from "./widget";

function ticker(change24h: number): TickerSnapshot {
  return {
    market: "usdm",
    symbol: "BTCUSDT",
    price: 112_842.6,
    priceText: "112,842.6",
    change24h,
    eventTime: 1,
    status: "live",
  };
}

describe("renderWidget", () => {
  it("shows the balance in the existing drag region, and uses dashes for errors", () => {
    expect(renderWidget({ tickers: [], showBalance: true, balance: "123456.78" }))
      .toMatch(/data-tauri-drag-region>\s*<button[^>]+class="wallet-balance symbol-name"[^>]*>账号余额：123456.78/);
    expect(renderWidget({ tickers: [], showBalance: true })).toContain("账号余额：---");
    expect(renderWidget({ tickers: [], showBalance: true })).not.toContain("仓位浮盈");
    expect(renderWidget({ tickers: [], showBalance: true })).not.toContain("position-profit");
    expect(renderWidget({ tickers: [], showBalance: true, balance: "0.00" })).toContain("账号余额：0.00");
    expect(renderWidget({ tickers: [], balance: "123" })).not.toContain("wallet-balance");
    expect(renderWidget({ tickers: [], showBalance: true, lockWindow: true })).not.toContain("data-tauri-drag-region");
  });
  it("renders no visual header or price flash classes", () => {
    const html = renderWidget({ tickers: [ticker(2.84)] });
    expect(html).not.toContain("ticker-head");
    expect(html).not.toContain("price-flash");
    expect(html).not.toContain("resize-corner");
    expect(html).toContain('aria-label="BTCUSDT 最新价 112,842.6，24小时 +2.84%"');
  });

  it.each([
    [2.84, "up", "+2.84%"],
    [-1.27, "down", "-1.27%"],
    [0, "flat", "0.00%"],
  ] as const)("uses the 24h direction for price color", (change24h, direction, label) => {
    const html = renderWidget({ tickers: [ticker(change24h)] });
    expect(html).toContain(`class="ticker-price ${direction}"`);
    expect(html).toContain(`class="ticker-change ${direction}"`);
    expect(html).toContain(label);
  });

  it("dims rows only when their market connection is reconnecting", () => {
    const reconnecting = { ...ticker(1), status: "reconnecting" as const };
    const quietButConnected = { ...ticker(1), symbol: "QUIETUSDT", eventTime: 0 };

    expect(renderWidget({ tickers: [reconnecting] })).toContain("ticker-row is-stale");
    expect(renderWidget({ tickers: [quietButConnected] })).not.toContain("ticker-row is-stale");
  });
});

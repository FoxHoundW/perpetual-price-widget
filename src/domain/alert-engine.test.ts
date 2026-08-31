import { describe, expect, it } from "vitest";
import { AlertEngine } from "./alert-engine";
import type { PriceSample, WatchItem } from "./types";

const minute = 60_000;
const watch: WatchItem = {
  market: "usdm",
  symbol: "BTCUSDT",
  alertEnabled: true,
  riseThreshold: 5,
  fallThreshold: 5,
};
const samples: PriceSample[] = Array.from({ length: 66 }, (_, index) => ({
  timestamp: index * minute,
  price: 100,
}));

describe("AlertEngine", () => {
  it("triggers all eligible periods independently", () => {
    const engine = new AlertEngine();
    const alerts = engine.evaluate({
      now: 65 * minute,
      market: "usdm",
      symbol: "BTCUSDT",
      price: 106,
      samples,
      watch,
      periods: [5, 10, 60],
    });
    expect(alerts.map((alert) => alert.period)).toEqual([5, 10, 60]);
    expect(alerts.every((alert) => alert.direction === "up")).toBe(true);
  });

  it("suppresses same-direction alerts during cooldown", () => {
    const engine = new AlertEngine();
    const first = engine.evaluate({
      now: 65 * minute,
      market: "usdm",
      symbol: "BTCUSDT",
      price: 106,
      samples,
      watch,
      periods: [5],
    });
    const second = engine.evaluate({
      now: 67 * minute,
      market: "usdm",
      symbol: "BTCUSDT",
      price: 112,
      samples,
      watch,
      periods: [5],
    });
    expect(first).toHaveLength(1);
    expect(second).toHaveLength(0);
  });

  it("uses the previous trigger price after cooldown", () => {
    const engine = new AlertEngine();
    engine.evaluate({
      now: 65 * minute,
      market: "usdm",
      symbol: "BTCUSDT",
      price: 106,
      samples,
      watch,
      periods: [5],
    });
    expect(
      engine.evaluate({
        now: 71 * minute,
        market: "usdm",
        symbol: "BTCUSDT",
        price: 110,
        samples,
        watch,
        periods: [5],
      }),
    ).toHaveLength(0);
    const next = engine.evaluate({
      now: 72 * minute,
      market: "usdm",
      symbol: "BTCUSDT",
      price: 112,
      samples,
      watch,
      periods: [5],
    });
    expect(next).toHaveLength(1);
    expect(next[0]?.changePercent).toBeCloseTo(5.66, 1);
  });

  it("keeps the opposite direction independent", () => {
    const engine = new AlertEngine();
    engine.evaluate({
      now: 65 * minute,
      market: "usdm",
      symbol: "BTCUSDT",
      price: 106,
      samples,
      watch,
      periods: [5],
    });
    const down = engine.evaluate({
      now: 66 * minute,
      market: "usdm",
      symbol: "BTCUSDT",
      price: 94,
      samples,
      watch,
      periods: [5],
    });
    expect(down).toHaveLength(1);
    expect(down[0]?.direction).toBe("down");
  });

  it("uses the rolling 24h change when kline history is unavailable", () => {
    const engine = new AlertEngine();
    const alerts = engine.evaluate({
      now: Date.now(),
      market: "usdm",
      symbol: "SOLUSDT",
      price: 101,
      samples: [],
      watch: { ...watch, symbol: "SOLUSDT", riseThreshold: 1, fallThreshold: 1 },
      periods: [1440],
      rolling24hChange: -3,
    });
    expect(alerts).toHaveLength(1);
    expect(alerts[0]).toMatchObject({ period: 1440, direction: "down", changePercent: -3 });
  });

  it("reuses the 24h trigger price after cooldown", () => {
    const engine = new AlertEngine();
    const input = {
      market: "usdm" as const,
      symbol: "SOLUSDT",
      samples: [],
      watch: { ...watch, symbol: "SOLUSDT", riseThreshold: 1, fallThreshold: 1 },
      periods: [1440] as const,
      rolling24hChange: -3,
    };
    expect(engine.evaluate({ ...input, periods: [...input.periods], now: 0, price: 100 })).toHaveLength(1);
    expect(engine.evaluate({ ...input, periods: [...input.periods], now: 6 * minute, price: 99.5 })).toHaveLength(0);
    expect(engine.evaluate({ ...input, periods: [...input.periods], now: 7 * minute, price: 98.9 })).toHaveLength(1);
  });
});

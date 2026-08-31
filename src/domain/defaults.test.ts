import { describe, expect, it } from "vitest";
import { normalizeSettings } from "./defaults";

describe("normalizeSettings", () => {
  it("keeps the requested arbitrary refresh interval", () => {
    expect(normalizeSettings({ refreshIntervalMs: 137 }).refreshIntervalMs).toBe(137);
    expect(normalizeSettings({ refreshIntervalMs: 1999 }).refreshIntervalMs).toBe(1999);
  });

  it("clamps visible rows and refresh bounds", () => {
    expect(normalizeSettings({ visibleRows: 0, refreshIntervalMs: 50 })).toMatchObject({
      visibleRows: 1,
      refreshIntervalMs: 100,
    });
    expect(normalizeSettings({ visibleRows: 99, refreshIntervalMs: 4000 })).toMatchObject({
      visibleRows: 20,
      refreshIntervalMs: 2000,
    });
  });

  it("clamps widget width without changing valid compact widths", () => {
    expect(normalizeSettings({ windowGeometry: { x: null, y: null, width: 299 } }).windowGeometry.width).toBe(300);
    expect(normalizeSettings({ windowGeometry: { x: null, y: null, width: 300 } }).windowGeometry.width).toBe(300);
    expect(normalizeSettings({ windowGeometry: { x: null, y: null, width: 392 } }).windowGeometry.width).toBe(392);
  });

  it("normalizes and de-duplicates watchlist keys", () => {
    const settings = normalizeSettings({
      watchlist: [
        { market: "usdm", symbol: " btcusdt ", alertEnabled: true, riseThreshold: 5, fallThreshold: 5 },
        { market: "usdm", symbol: "BTCUSDT", alertEnabled: false, riseThreshold: 2, fallThreshold: 2 },
      ],
    });
    expect(settings.watchlist).toHaveLength(1);
    expect(settings.watchlist[0]?.symbol).toBe("BTCUSDT");
  });

  it("keeps supported long alert periods and removes invalid values", () => {
    const settings = normalizeSettings({
      alerts: {
        ...normalizeSettings({}).alerts,
        periods: [5, 360, 720, 1440, 999 as 1440],
      },
    });
    expect(settings.alerts.periods).toEqual([5, 360, 720, 1440]);
  });

  it("normalizes personalization ranges and colors", () => {
    const settings = normalizeSettings({
      appearance: {
        ...normalizeSettings({}).appearance,
        backgroundColor: "invalid",
        backgroundOpacity: -1,
        fontOpacity: 1,
        fontShadowSize: 99,
        alertUpColor: "#abcdef",
        alertAccentOpacity: 101,
        alertCardBackgroundOpacity: 1,
      },
    });
    expect(settings.appearance).toMatchObject({
      backgroundColor: "#0D131C",
      backgroundOpacity: 0,
      fontOpacity: 30,
      fontShadowSize: 6,
      alertUpColor: "#ABCDEF",
      alertAccentOpacity: 100,
      alertCardBackgroundOpacity: 20,
    });
  });
});

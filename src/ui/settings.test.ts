import { describe, expect, it } from "vitest";
import { DEMO_CONTRACTS, DEFAULT_SETTINGS } from "../domain/defaults";
import {
  mergeWatchlistInPlace,
  parseAlertThreshold,
  renderAbout,
  renderAvailablePairRows,
  renderSettingsWindow,
} from "./settings";
import { periodLabel } from "./shared";

describe("renderSettingsWindow", () => {
  it("includes the personalization secondary page entry", () => {
    const html = renderSettingsWindow();
    expect(html).toContain('data-page="personalization"');
    expect(html).toContain("个性化");
    expect(renderAbout()).toContain("版本 1.1.0");
    expect(renderAbout()).not.toContain("版本 0.1.0");
  });

  it("filters catalog rows without rebuilding the search input", () => {
    const html = renderAvailablePairRows({
      contracts: DEMO_CONTRACTS,
      watchlist: DEFAULT_SETTINGS.watchlist,
      marketFilter: "all",
      search: "doge",
    });
    expect(html).toContain("DOGEUSDT");
    expect(html).not.toContain("BNBUSDT");
    expect(html).not.toContain("pair-search");
  });

  it("accepts only valid alert thresholds for autosave", () => {
    expect(parseAlertThreshold("5.5")).toBe(5.5);
    expect(parseAlertThreshold("0.1")).toBe(0.1);
    expect(parseAlertThreshold("0")).toBeNull();
    expect(parseAlertThreshold("")).toBeNull();
  });

  it("preserves field bindings when saved watchlist values are merged", () => {
    const watchlist = DEFAULT_SETTINGS.watchlist.map((item) => ({ ...item }));
    const ethBinding = watchlist[1]!;
    const saved = watchlist.map((item) => ({ ...item, riseThreshold: item.symbol === "BTCUSDT" ? 3 : 5 }));
    mergeWatchlistInPlace(watchlist, saved);
    expect(watchlist[1]).toBe(ethBinding);
    ethBinding.fallThreshold = 2.5;
    expect(watchlist[1]?.fallThreshold).toBe(2.5);
  });

  it("formats long alert periods in hours", () => {
    expect(periodLabel(5)).toBe("5min");
    expect(periodLabel(360)).toBe("6h");
    expect(periodLabel(1440)).toBe("24h");
  });
});

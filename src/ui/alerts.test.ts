import { describe, expect, it } from "vitest";
import type { AlertRecord } from "../domain/types";
import { alertPanelSignature, groupAlerts, renderAlertPanel } from "./alerts";

function alert(
  id: string,
  market: "usdm" | "coinm",
  symbol: string,
  triggerTime: number,
): AlertRecord {
  return {
    id,
    market,
    symbol,
    period: 5,
    direction: "up",
    changePercent: 5,
    triggerPrice: 100,
    triggerTime,
    expiresAt: triggerTime + 6 * 60 * 60_000,
    dismissed: false,
  };
}

describe("groupAlerts", () => {
  it("stacks repeated alerts for the same market and symbol newest first", () => {
    const stacks = groupAlerts([
      alert("old", "usdm", "BTCUSDT", 1),
      alert("new", "usdm", "BTCUSDT", 2),
    ]);
    expect(stacks).toHaveLength(1);
    expect(stacks[0]?.cards.map((card) => card.id)).toEqual(["new", "old"]);
  });

  it("keeps equal symbol names from different markets in separate stacks", () => {
    const stacks = groupAlerts([
      alert("u", "usdm", "TESTPERP", 1),
      alert("coin", "coinm", "TESTPERP", 2),
    ]);
    expect(stacks).toHaveLength(2);
  });

  it("clamps a preserved carousel index after cards expire", () => {
    const previous = [{
      symbol: "BTCUSDT",
      market: "usdm" as const,
      activeIndex: 4,
      cards: [alert("old", "usdm", "BTCUSDT", 1)],
    }];
    expect(groupAlerts([alert("new", "usdm", "BTCUSDT", 2)], previous)[0]?.activeIndex).toBe(0);
  });

  it("keeps a stable signature for unchanged alert data", () => {
    const first = groupAlerts([alert("one", "usdm", "SOLUSDT", 1)]);
    const second = groupAlerts([alert("one", "usdm", "SOLUSDT", 1)], first);
    expect(alertPanelSignature(second)).toBe(alertPanelSignature(first));
  });

  it("adds the entry animation only to explicitly new stacks", () => {
    const stacks = groupAlerts([
      alert("btc", "usdm", "BTCUSDT", 1),
      alert("sol", "usdm", "SOLUSDT", 2),
    ]);
    const html = renderAlertPanel(stacks, new Set(["usdm:SOLUSDT"]));
    expect(html.match(/is-entering/g)).toHaveLength(1);
    expect(html).toContain('data-alert-stack="usdm:SOLUSDT"');
  });
});

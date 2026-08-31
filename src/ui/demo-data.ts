import type { AlertRecord } from "../domain/types";
import type { AlertStack } from "./alerts";

const now = Date.now();

function alert(
  id: string,
  symbol: string,
  period: 5 | 10 | 60,
  direction: "up" | "down",
  changePercent: number,
  triggerPrice: number,
  minutesAgo: number,
): AlertRecord {
  return {
    id,
    market: "usdm",
    symbol,
    period,
    direction,
    changePercent,
    triggerPrice,
    triggerTime: now - minutesAgo * 60_000,
    expiresAt: now + (360 - minutesAgo) * 60_000,
    dismissed: false,
  };
}

export function createDemoAlertStacks(): AlertStack[] {
  return [
    {
      symbol: "SOLUSDT",
      market: "usdm",
      activeIndex: 0,
      cards: [
        alert("sol-1", "SOLUSDT", 5, "up", 5.38, 218.762, 1),
        alert("sol-2", "SOLUSDT", 10, "up", 7.14, 216.831, 3),
        alert("sol-3", "SOLUSDT", 60, "up", 11.62, 211.4, 18),
      ],
    },
    {
      symbol: "ETHUSDT",
      market: "usdm",
      activeIndex: 0,
      cards: [alert("eth-1", "ETHUSDT", 10, "down", -5.21, 4_612.34, 7)],
    },
  ];
}

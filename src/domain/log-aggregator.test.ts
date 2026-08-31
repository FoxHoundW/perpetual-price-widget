import { describe, expect, it } from "vitest";
import { LogAggregator, sanitizeLogMessage } from "./log-aggregator";

describe("LogAggregator", () => {
  it("groups consecutive identical failures and ends on recovery", () => {
    const logs = new LogAggregator();
    for (let index = 0; index < 6; index += 1) {
      logs.append({
        at: index * 5_000,
        level: "error",
        source: "COIN-M WebSocket",
        code: "TIMEOUT",
        message: "连接超时",
      });
    }
    logs.recover("COIN-M WebSocket", "已恢复，正在恢复订阅", 30_000);
    expect(logs.list()).toHaveLength(2);
    expect(logs.list()[0]).toMatchObject({ count: 6, firstAt: 0, lastAt: 25_000 });
    expect(logs.list()[1]?.recovered).toBe(true);
  });

  it("sanitizes credentials", () => {
    expect(sanitizeLogMessage("https://alice:secret@proxy.test?token=abc")).toBe(
      "https://***:***@proxy.test?token=***",
    );
  });
});

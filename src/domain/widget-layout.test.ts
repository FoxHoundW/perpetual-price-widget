import { describe, expect, it } from "vitest";
import { widgetHeightForRows } from "./widget-layout";

describe("widgetHeightForRows", () => {
  it.each([
    [1, 66],
    [3, 154],
    [20, 902],
  ])("maps %i visible rows to %ipx", (rows, height) => {
    expect(widgetHeightForRows(rows)).toBe(height);
  });
});

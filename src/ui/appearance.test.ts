import { describe, expect, it } from "vitest";
import { DEFAULT_SETTINGS } from "../domain/defaults";
import { applyAppearanceVariables } from "./appearance";

describe("applyAppearanceVariables", () => {
  it("maps colors and opacity to isolated CSS variables", () => {
    const values = new Map<string, string>();
    const target = {
      style: { setProperty: (name: string, value: string) => values.set(name, value) },
    } as unknown as HTMLElement;

    applyAppearanceVariables({
      ...DEFAULT_SETTINGS.appearance,
      backgroundColor: "#123456",
      backgroundOpacity: 0,
      fontOpacity: 80,
      fontShadowSize: 3,
      alertAccentOpacity: 50,
    }, target);

    expect(values.get("--widget-background")).toBe("rgba(18, 52, 86, 0)");
    expect(values.get("--widget-font-opacity")).toBe("0.8");
    expect(values.get("--widget-font-shadow")).toBe("0 1px 2px rgba(0, 0, 0, 0.92), 0 0 3px rgba(0, 0, 0, 0.58)");
    expect(values.get("--alert-up-accent")).toBe("rgba(73, 225, 154, 0.5)");
    expect(values.get("--widget-glow")).toBe("rgba(101, 217, 255, 0)");
  });

  it("turns the font shadow off at zero size", () => {
    const values = new Map<string, string>();
    const target = {
      style: { setProperty: (name: string, value: string) => values.set(name, value) },
    } as unknown as HTMLElement;
    applyAppearanceVariables({ ...DEFAULT_SETTINGS.appearance, fontShadowSize: 0 }, target);
    expect(values.get("--widget-font-shadow")).toBe("none");
  });
});

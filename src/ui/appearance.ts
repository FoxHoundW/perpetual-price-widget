import type { AppearanceSettings } from "../domain/types";

function colorChannels(hex: string): [number, number, number] {
  return [
    Number.parseInt(hex.slice(1, 3), 16),
    Number.parseInt(hex.slice(3, 5), 16),
    Number.parseInt(hex.slice(5, 7), 16),
  ];
}

function rgba(hex: string, opacityPercent: number): string {
  const [red, green, blue] = colorChannels(hex);
  return `rgba(${red}, ${green}, ${blue}, ${opacityPercent / 100})`;
}

export function applyAppearanceVariables(
  appearance: AppearanceSettings,
  target: HTMLElement = document.documentElement,
): void {
  target.style.setProperty("--widget-background", rgba(appearance.backgroundColor, appearance.backgroundOpacity));
  target.style.setProperty("--widget-highlight", rgba("#FFFFFF", appearance.backgroundOpacity * 0.035));
  target.style.setProperty("--widget-glow", rgba("#65D9FF", appearance.backgroundOpacity * 0.08));
  target.style.setProperty("--widget-font-opacity", String(appearance.fontOpacity / 100));
  const shadowSize = Math.min(6, Math.max(0, appearance.fontShadowSize));
  const shadowValue = shadowSize === 0
    ? "none"
    : `0 ${Math.min(1, shadowSize / 3)}px ${shadowSize * 2 / 3}px rgba(0, 0, 0, 0.92), 0 0 ${shadowSize}px rgba(0, 0, 0, 0.58)`;
  target.style.setProperty("--widget-font-shadow", shadowValue);
  target.style.setProperty("--appearance-up", appearance.alertUpColor);
  target.style.setProperty("--appearance-down", appearance.alertDownColor);
  target.style.setProperty("--alert-up-accent", rgba(appearance.alertUpColor, appearance.alertAccentOpacity));
  target.style.setProperty("--alert-down-accent", rgba(appearance.alertDownColor, appearance.alertAccentOpacity));
  target.style.setProperty("--alert-up-soft", rgba(appearance.alertUpColor, appearance.alertAccentOpacity * 0.1));
  target.style.setProperty("--alert-down-soft", rgba(appearance.alertDownColor, appearance.alertAccentOpacity * 0.1));
  target.style.setProperty(
    "--alert-card-background",
    rgba(appearance.alertCardBackgroundColor, appearance.alertCardBackgroundOpacity),
  );
}

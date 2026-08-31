export function escapeHtml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#039;");
}

export function formatClock(timestamp: number): string {
  return new Intl.DateTimeFormat("zh-CN", {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false,
  }).format(timestamp);
}

export function marketLabel(market: "usdm" | "coinm"): string {
  return market === "usdm" ? "U 本位" : "币本位";
}

export function directionLabel(direction: "up" | "down"): string {
  return direction === "up" ? "上涨" : "下跌";
}

export function periodLabel(period: number): string {
  return period >= 60 && period % 60 === 0 ? `${period / 60}h` : `${period}min`;
}

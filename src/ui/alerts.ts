import type { AlertRecord } from "../domain/types";
import { directionLabel, escapeHtml, formatClock, marketLabel, periodLabel } from "./shared";

export interface AlertStack {
  symbol: string;
  market: "usdm" | "coinm";
  activeIndex: number;
  cards: AlertRecord[];
}

export function alertStackKey(stack: Pick<AlertStack, "market" | "symbol">): string {
  return `${stack.market}:${stack.symbol}`;
}

export function alertPanelSignature(stacks: AlertStack[]): string {
  return stacks
    .map((stack) => `${alertStackKey(stack)}@${stack.activeIndex}:${stack.cards.map((card) => card.id).join(",")}`)
    .join("|");
}

export function renderAlertPanel(
  stacks: AlertStack[],
  enteringKeys: ReadonlySet<string> = new Set(),
): string {
  return `
    <aside class="alert-panel scroll-surface" aria-label="异动提醒">
      ${stacks.map((stack) => renderStack(stack, enteringKeys.has(alertStackKey(stack)))).join("")}
    </aside>`;
}

function renderStack(stack: AlertStack, entering: boolean): string {
  const active = stack.cards[stack.activeIndex] ?? stack.cards[0];
  if (!active) return "";
  const tone = active.direction === "up" ? "up" : "down";
  const change = `${active.changePercent >= 0 ? "+" : ""}${active.changePercent.toFixed(2)}%`;
  const visibleLayers = Math.min(3, stack.cards.length);

  return `
    <div class="alert-stack tone-${tone}${entering ? " is-entering" : ""}" data-alert-stack="${stack.market}:${escapeHtml(stack.symbol)}" style="--layers:${visibleLayers}">
      ${Array.from({ length: Math.max(0, visibleLayers - 1) }, (_, index) => `<span class="stack-layer layer-${index + 1}" aria-hidden="true"></span>`).join("")}
      <article class="alert-card" tabindex="0" role="button" aria-label="${escapeHtml(stack.symbol)} ${periodLabel(active.period)} ${directionLabel(active.direction)}，点击查看下一条" data-alert-card>
        <header class="alert-card-head">
          <div>
            <div class="alert-title-line">
              <strong>${escapeHtml(stack.symbol)}</strong>
              <span class="market-badge">${marketLabel(stack.market)}</span>
            </div>
            <span class="alert-kind">${periodLabel(active.period)} · ${directionLabel(active.direction)}</span>
          </div>
          <div class="alert-card-actions">
            ${stack.cards.length > 1 ? `<span class="stack-count">${stack.activeIndex + 1} / ${stack.cards.length}</span>` : ""}
            <button class="icon-button close-alert" type="button" aria-label="关闭当前警示" title="关闭">×</button>
          </div>
        </header>
        <div class="alert-metric ${tone}">${change}</div>
        <dl class="alert-details">
          <div><dt>触发价格</dt><dd>${active.triggerPrice.toLocaleString("en-US", { maximumFractionDigits: 8 })}</dd></div>
          <div><dt>触发时间</dt><dd>${formatClock(active.triggerTime)}</dd></div>
        </dl>
      </article>
    </div>`;
}

export function bindAlertPanel(
  root: HTMLElement,
  stacks: AlertStack[],
  onDismiss?: (id: string) => Promise<void>,
  onLocalRender?: () => void,
): void {
  const rerender = (): void => {
    root.innerHTML = renderAlertPanel(stacks);
    attach();
    onLocalRender?.();
  };

  const cycle = (key: string): void => {
    const stack = stacks.find((item) => `${item.market}:${item.symbol}` === key);
    if (!stack || stack.cards.length < 2) return;
    stack.activeIndex = (stack.activeIndex + 1) % stack.cards.length;
    rerender();
  };

  const dismiss = (key: string): void => {
    const stackIndex = stacks.findIndex((item) => `${item.market}:${item.symbol}` === key);
    const stack = stacks[stackIndex];
    if (!stack) return;
    const dismissed = stack.cards[stack.activeIndex];
    if (dismissed && onDismiss) void onDismiss(dismissed.id);
    stack.cards.splice(stack.activeIndex, 1);
    if (stack.cards.length === 0) stacks.splice(stackIndex, 1);
    else stack.activeIndex %= stack.cards.length;
    rerender();
  };

  const attach = (): void => {
    root.querySelectorAll<HTMLElement>("[data-alert-stack]").forEach((element) => {
      const key = element.dataset.alertStack;
      const card = element.querySelector<HTMLElement>("[data-alert-card]");
      const close = element.querySelector<HTMLButtonElement>(".close-alert");
      if (!key || !card) return;
      card.addEventListener("click", (event) => {
        if ((event.target as HTMLElement).closest(".close-alert")) return;
        cycle(key);
      });
      card.addEventListener("keydown", (event) => {
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          cycle(key);
        }
      });
      close?.addEventListener("click", (event) => {
        event.stopPropagation();
        dismiss(key);
      });
    });
  };

  attach();
}

export function groupAlerts(records: AlertRecord[], previous: AlertStack[] = []): AlertStack[] {
  const bySymbol = new Map<string, AlertStack>();
  for (const record of [...records].sort((a, b) => b.triggerTime - a.triggerTime)) {
    const key = `${record.market}:${record.symbol}`;
    const existing = bySymbol.get(key);
    if (existing) {
      existing.cards.push(record);
    } else {
      const old = previous.find(
        (stack) => stack.market === record.market && stack.symbol === record.symbol,
      );
      bySymbol.set(key, {
        symbol: record.symbol,
        market: record.market,
        activeIndex: old ? Math.min(old.activeIndex, records.length - 1) : 0,
        cards: [record],
      });
    }
  }
  return [...bySymbol.values()]
    .map((stack) => ({
      ...stack,
      activeIndex: Math.min(stack.activeIndex, Math.max(0, stack.cards.length - 1)),
    }))
    .sort((a, b) => (b.cards[0]?.triggerTime ?? 0) - (a.cards[0]?.triggerTime ?? 0));
}

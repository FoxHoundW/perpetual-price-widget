import type { Direction, TickerSnapshot } from "../domain/types";
import { symbolKey } from "../domain/types";
import { escapeHtml, marketLabel } from "./shared";

export interface WidgetOptions {
  tickers: TickerSnapshot[];
  stale?: boolean;
  lockWindow?: boolean;
  className?: string;
  alertSymbols?: Map<string, Direction>;
}

export function renderWidget({
  tickers,
  stale = false,
  lockWindow = false,
  className = "",
  alertSymbols = new Map(),
}: WidgetOptions): string {
  const rows = tickers
    .map((ticker) => {
      const direction = ticker.change24h > 0 ? "up" : ticker.change24h < 0 ? "down" : "flat";
      const signedChange = `${ticker.change24h > 0 ? "+" : ""}${ticker.change24h.toFixed(2)}%`;
      const alertDirection = alertSymbols.get(symbolKey(ticker.market, ticker.symbol));
      const alerting = alertDirection !== undefined;
      const rowState = [
        "ticker-row",
        alerting ? `is-alerting is-${alertDirection}` : "",
        ticker.status === "stale" || ticker.status === "reconnecting" || stale ? "is-stale" : "",
      ]
        .filter(Boolean)
        .join(" ");
      return `
        <div class="${rowState}" role="row" aria-label="${escapeHtml(ticker.symbol)} 最新价 ${escapeHtml(ticker.priceText)}，24小时 ${signedChange}">
          <div class="ticker-symbol" role="cell">
            <span class="alert-rail" aria-hidden="true"></span>
            <span class="symbol-name">${escapeHtml(ticker.symbol)}</span>
            <span class="market-badge">${marketLabel(ticker.market)}</span>
          </div>
          <div class="ticker-price ${direction}" role="cell">${ticker.status === "delisted" ? "已下架" : escapeHtml(ticker.priceText)}</div>
          <div class="ticker-change ${direction}" role="cell">
            <span class="direction-glyph" aria-hidden="true">${direction === "up" ? "↗" : direction === "down" ? "↘" : "–"}</span>${signedChange}
          </div>
        </div>`;
    })
    .join("");
  const body = rows || '<div class="widget-empty"><strong>尚未添加交易对</strong><span>右键打开设置并添加永续合约</span></div>';

  return `
    <section class="ticker-widget ${className}" aria-label="永续合约行情">
      <div class="widget-drag-region" ${lockWindow ? "" : "data-tauri-drag-region"}>
        <div class="connection-dot ${stale ? "warning" : "live"}" title="${stale ? "行情暂时中断，正在重新连接" : "行情连接正常"}">
          <span></span><span class="sr-only">${stale ? "行情暂时中断" : "行情连接正常"}</span>
        </div>
      </div>
      <div class="ticker-table" role="table" aria-label="自选行情">
        <div class="ticker-scroll scroll-surface">${body}</div>
      </div>
    </section>`;
}

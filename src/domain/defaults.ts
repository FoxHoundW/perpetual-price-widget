import type { AlertPeriod, AppSettings, Contract, TickerSnapshot } from "./types";
import { WIDGET_DEFAULT_WIDTH, WIDGET_MAX_WIDTH, WIDGET_MIN_WIDTH } from "./widget-layout";

export const DEFAULT_SETTINGS: AppSettings = {
  autostart: true,
  alwaysOnTop: true,
  visibleRows: 3,
  refreshIntervalMs: 500,
  lockSize: false,
  lockWindow: false,
  windowGeometry: { x: null, y: null, width: WIDGET_DEFAULT_WIDTH },
  watchlist: ["BTCUSDT", "ETHUSDT", "SOLUSDT"].map((symbol) => ({
    market: "usdm" as const,
    symbol,
    alertEnabled: true,
    riseThreshold: 5,
    fallThreshold: 5,
  })),
  alerts: {
    periods: [5, 10, 60],
    cooldownMinutes: 5,
    retentionHours: 6,
    hiddenBehavior: "side-card",
    defaultEnabled: true,
    defaultRiseThreshold: 5,
    defaultFallThreshold: 5,
  },
  appearance: {
    backgroundColor: "#0D131C",
    backgroundOpacity: 99,
    fontOpacity: 100,
    fontShadowSize: 3,
    alertUpColor: "#49E19A",
    alertDownColor: "#FF7181",
    alertAccentOpacity: 100,
    alertCardBackgroundColor: "#111923",
    alertCardBackgroundOpacity: 94,
  },
  proxy: {
    mode: "system",
    host: "",
    port: null,
    username: "",
  },
};

export const ALERT_PERIODS: AlertPeriod[] = [5, 10, 60, 360, 720, 1440];

export const DEMO_TICKERS: TickerSnapshot[] = [
  {
    market: "usdm",
    symbol: "BTCUSDT",
    price: 112_842.6,
    priceText: "112,842.6",
    change24h: 2.84,
    eventTime: Date.now(),
    status: "live",
    lastMove: "up",
  },
  {
    market: "usdm",
    symbol: "ETHUSDT",
    price: 4_612.34,
    priceText: "4,612.34",
    change24h: -1.27,
    eventTime: Date.now(),
    status: "live",
    lastMove: "down",
  },
  {
    market: "usdm",
    symbol: "SOLUSDT",
    price: 218.762,
    priceText: "218.762",
    change24h: 6.41,
    eventTime: Date.now(),
    status: "live",
    lastMove: "up",
  },
];

export const DEMO_CONTRACTS: Contract[] = [
  ["BTCUSDT", "BTC", "USDT", "0.10", 1],
  ["ETHUSDT", "ETH", "USDT", "0.01", 2],
  ["SOLUSDT", "SOL", "USDT", "0.001", 3],
  ["BNBUSDT", "BNB", "USDT", "0.001", 3],
  ["XRPUSDT", "XRP", "USDT", "0.0001", 4],
  ["DOGEUSDT", "DOGE", "USDT", "0.00001", 5],
].map(([symbol, baseAsset, quoteAsset, tickSize, priceDecimals]) => ({
  market: "usdm",
  symbol: String(symbol),
  baseAsset: String(baseAsset),
  quoteAsset: String(quoteAsset),
  marginAsset: String(quoteAsset),
  contractType: "PERPETUAL",
  status: "TRADING",
  tickSize: String(tickSize),
  priceDecimals: Number(priceDecimals),
}));

export function normalizeSettings(input: Partial<AppSettings>): AppSettings {
  const visibleRows = Math.round(Number(input.visibleRows ?? DEFAULT_SETTINGS.visibleRows));
  const refreshIntervalMs = Math.round(
    Number(input.refreshIntervalMs ?? DEFAULT_SETTINGS.refreshIntervalMs),
  );

  const watchlist = (input.watchlist ?? DEFAULT_SETTINGS.watchlist).map((item) => ({ ...item }));
  const seen = new Set<string>();
  const normalizedWatchlist = watchlist.filter((item) => {
    const key = `${item.market}:${item.symbol.trim().toUpperCase()}`;
    if (seen.has(key)) return false;
    seen.add(key);
    item.symbol = item.symbol.trim().toUpperCase();
    item.riseThreshold = Math.max(0.01, Number(item.riseThreshold) || 0.01);
    item.fallThreshold = Math.max(0.01, Number(item.fallThreshold) || 0.01);
    return true;
  });
  const appearance = { ...DEFAULT_SETTINGS.appearance, ...input.appearance };
  const normalizeColor = (value: string, fallback: string): string =>
    /^#[0-9a-f]{6}$/i.test(value) ? value.toUpperCase() : fallback;
  const clampPercent = (value: number, minimum: number): number =>
    Math.min(100, Math.max(minimum, Math.round(Number(value))));
  const shadowSize = Number(appearance.fontShadowSize);
  const periods = (input.alerts?.periods ?? DEFAULT_SETTINGS.alerts.periods)
    .filter((period): period is AlertPeriod => ALERT_PERIODS.includes(period as AlertPeriod))
    .filter((period, index, values) => values.indexOf(period) === index)
    .sort((left, right) => left - right);

  return {
    ...DEFAULT_SETTINGS,
    ...input,
    visibleRows: Math.min(20, Math.max(1, visibleRows)),
    refreshIntervalMs: Math.min(2000, Math.max(100, refreshIntervalMs)),
    watchlist: normalizedWatchlist,
    alerts: { ...DEFAULT_SETTINGS.alerts, ...input.alerts, periods },
    appearance: {
      backgroundColor: normalizeColor(appearance.backgroundColor, DEFAULT_SETTINGS.appearance.backgroundColor),
      backgroundOpacity: clampPercent(appearance.backgroundOpacity, 0),
      fontOpacity: clampPercent(appearance.fontOpacity, 30),
      fontShadowSize: Number.isFinite(shadowSize)
        ? Math.min(6, Math.max(0, Math.round(shadowSize * 2) / 2))
        : DEFAULT_SETTINGS.appearance.fontShadowSize,
      alertUpColor: normalizeColor(appearance.alertUpColor, DEFAULT_SETTINGS.appearance.alertUpColor),
      alertDownColor: normalizeColor(appearance.alertDownColor, DEFAULT_SETTINGS.appearance.alertDownColor),
      alertAccentOpacity: clampPercent(appearance.alertAccentOpacity, 20),
      alertCardBackgroundColor: normalizeColor(
        appearance.alertCardBackgroundColor,
        DEFAULT_SETTINGS.appearance.alertCardBackgroundColor,
      ),
      alertCardBackgroundOpacity: clampPercent(appearance.alertCardBackgroundOpacity, 20),
    },
    proxy: { ...DEFAULT_SETTINGS.proxy, ...input.proxy },
    windowGeometry: {
      ...DEFAULT_SETTINGS.windowGeometry,
      ...input.windowGeometry,
      width: Math.min(
        WIDGET_MAX_WIDTH,
        Math.max(
          WIDGET_MIN_WIDTH,
          Number(input.windowGeometry?.width ?? WIDGET_DEFAULT_WIDTH),
        ),
      ),
    },
  };
}

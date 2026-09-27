export type Market = "usdm" | "coinm";
export type Direction = "up" | "down";
export type AlertPeriod = 5 | 10 | 60 | 360 | 720 | 1440;
export type ConnectionStatus = "connecting" | "live" | "stale" | "reconnecting";

export interface Contract {
  market: Market;
  symbol: string;
  baseAsset: string;
  quoteAsset: string;
  marginAsset: string;
  contractType: "PERPETUAL";
  status: "TRADING" | "DELISTED";
  tickSize: string;
  priceDecimals: number;
}

export interface WatchItem {
  market: Market;
  symbol: string;
  alertEnabled: boolean;
  riseThreshold: number;
  fallThreshold: number;
}

export interface TickerSnapshot {
  market: Market;
  symbol: string;
  price: number;
  priceText: string;
  change24h: number;
  eventTime: number;
  status: ConnectionStatus | "delisted";
  lastMove?: Direction;
}

export interface AlertRecord {
  id: string;
  market: Market;
  symbol: string;
  period: AlertPeriod;
  direction: Direction;
  changePercent: number;
  triggerPrice: number;
  triggerTime: number;
  expiresAt: number;
  dismissed: boolean;
}

export interface AlertGlobalSettings {
  periods: AlertPeriod[];
  cooldownMinutes: 5;
  retentionHours: 6;
  hiddenBehavior: "side-card" | "record-only";
  defaultEnabled: boolean;
  defaultRiseThreshold: number;
  defaultFallThreshold: number;
}

export interface AppearanceSettings {
  backgroundColor: string;
  backgroundOpacity: number;
  fontOpacity: number;
  fontShadowSize: number;
  alertUpColor: string;
  alertDownColor: string;
  alertAccentOpacity: number;
  alertCardBackgroundColor: string;
  alertCardBackgroundOpacity: number;
}

export interface AppSettings {
  showBalance: boolean;
  autostart: boolean;
  alwaysOnTop: boolean;
  visibleRows: number;
  refreshIntervalMs: number;
  lockSize: boolean;
  lockWindow: boolean;
  windowGeometry: {
    x: number | null;
    y: number | null;
    width: number;
  };
  watchlist: WatchItem[];
  alerts: AlertGlobalSettings;
  appearance: AppearanceSettings;
  proxy: {
    mode: "system" | "http" | "socks5";
    host: string;
    port: number | null;
    username: string;
  };
}

export interface PriceSample {
  timestamp: number;
  price: number;
}

export interface LogEvent {
  id: string;
  level: "info" | "warning" | "error";
  source: string;
  code: string;
  message: string;
  firstAt: number;
  lastAt: number;
  count: number;
  recovered: boolean;
}

export const symbolKey = (market: Market, symbol: string): string => `${market}:${symbol}`;

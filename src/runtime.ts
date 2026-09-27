import { invoke } from "@tauri-apps/api/core";
import type { AlertRecord, AppSettings, Contract, TickerSnapshot } from "./domain/types";

export interface BootstrapPayload {
  settings: AppSettings;
  contracts: Contract[];
  tickers: TickerSnapshot[];
  catalogSyncedAt: number | null;
}

export interface RuntimeLogRow {
  id: number;
  level: "info" | "warning" | "error";
  source: string;
  code: string;
  message: string;
  firstAt: number;
  lastAt: number;
  count: number;
  recovered: boolean;
}

export const isTauriRuntime = "__TAURI_INTERNALS__" in window;

export interface WalletSnapshot {
  details: Array<{ name: string; balance: string }>;
  balance: string | null;
  wallets: string[];
  error: string | null;
  updatedAt: number | null;
}

export const getWalletBalance = (): Promise<WalletSnapshot> => invoke("get_wallet_balance");
export const getWalletStatus = (): Promise<WalletSnapshot> => invoke("get_wallet_status");
export const toggleWalletDetails = (): Promise<void> => invoke("toggle_wallet_details");
export const hideWalletDetails = (): Promise<void> => invoke("hide_wallet_details");
export const saveWalletCredentials = (apiKey: string, secret: string): Promise<void> =>
  invoke("save_wallet_credentials", { apiKey, secret });

export async function getBootstrap(): Promise<BootstrapPayload> {
  return invoke<BootstrapPayload>("get_bootstrap");
}

export async function getTickers(): Promise<TickerSnapshot[]> {
  return invoke<TickerSnapshot[]>("get_tickers");
}

export async function saveSettings(
  settings: AppSettings,
  forceReconnect = false,
): Promise<AppSettings> {
  return invoke<AppSettings>("save_settings", { settings, forceReconnect });
}

export async function syncContracts(): Promise<{ usdmCount: number; coinmCount: number; syncedAt: number }> {
  return invoke("sync_contracts");
}

export async function openSettings(): Promise<void> {
  await invoke("open_settings");
}

export async function closeSettings(): Promise<void> {
  await invoke("close_settings");
}

export async function hideMain(): Promise<void> {
  await invoke("hide_main");
}

export async function setMousePassthrough(enabled: boolean): Promise<void> {
  await invoke("set_mouse_passthrough", { enabled });
}

export async function exitApp(): Promise<void> {
  await invoke("exit_app");
}

export async function getAlerts(): Promise<AlertRecord[]> {
  return invoke<AlertRecord[]>("get_alerts");
}

export async function getAlertStates(): Promise<AlertRecord[]> {
  return invoke<AlertRecord[]>("get_alert_states");
}

export async function dismissAlert(id: string): Promise<void> {
  await invoke("dismiss_alert", { id });
}

export async function getLogs(offset = 0, limit = 50): Promise<RuntimeLogRow[]> {
  return invoke<RuntimeLogRow[]>("get_logs", { offset, limit });
}

export async function clearLogs(): Promise<void> {
  await invoke("clear_logs");
}

export async function saveProxyPassword(username: string, password: string): Promise<void> {
  await invoke("save_proxy_password", { username, password });
}

export async function testProxy(
  proxy: AppSettings["proxy"],
  password: string,
): Promise<string> {
  return invoke<string>("test_proxy", { proxy, password });
}

export async function saveWindowGeometry(
  visibleRows: number,
  width: number,
  x: number | null,
  y: number | null,
): Promise<void> {
  await invoke("save_window_geometry", { visibleRows, width, x, y });
}

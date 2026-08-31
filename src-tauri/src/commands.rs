use std::{collections::HashSet, sync::Arc};

use chrono::Utc;
use serde::Serialize;
use tauri::{
    AppHandle, Emitter, LogicalSize, Manager, Size, State, WebviewUrl, WebviewWindowBuilder,
};
use tauri_plugin_autostart::ManagerExt;
use tokio::sync::{Mutex, RwLock};

use crate::{
    alert_service::AlertService,
    app_logging::{LogInput, LogRow},
    contract_catalog::{build_http_client, fetch_contracts},
    credentials,
    market_stream::MarketStreamManager,
    models::{
        widget_height, AlertRecord, AppSettings, Contract, Market, ProxySettings, TickerSnapshot,
    },
    persistence::Persistence,
};

pub struct AppState {
    pub persistence: Arc<Persistence>,
    pub settings: Arc<RwLock<AppSettings>>,
    pub settings_write_lock: Mutex<()>,
    pub catalog: Arc<RwLock<Vec<Contract>>>,
    pub market_streams: Arc<MarketStreamManager>,
    pub alerts: Arc<AlertService>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BootstrapPayload {
    pub settings: AppSettings,
    pub contracts: Vec<Contract>,
    pub tickers: Vec<TickerSnapshot>,
    pub catalog_synced_at: Option<i64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncResult {
    pub usdm_count: usize,
    pub coinm_count: usize,
    pub synced_at: i64,
}

#[tauri::command]
pub async fn get_bootstrap(state: State<'_, AppState>) -> Result<BootstrapPayload, String> {
    let settings = state.settings.read().await.clone();
    let contracts = state.catalog.read().await.clone();
    let tickers = state
        .market_streams
        .snapshots(Utc::now().timestamp_millis())
        .await;
    let catalog_synced_at = [Market::Usdm, Market::Coinm]
        .into_iter()
        .filter_map(|market| state.persistence.last_contract_sync(market).ok().flatten())
        .max();
    Ok(BootstrapPayload {
        settings,
        contracts,
        tickers,
        catalog_synced_at,
    })
}

#[tauri::command]
pub async fn get_tickers(state: State<'_, AppState>) -> Result<Vec<TickerSnapshot>, String> {
    Ok(state
        .market_streams
        .snapshots(Utc::now().timestamp_millis())
        .await)
}

#[tauri::command]
pub async fn save_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: AppSettings,
    force_reconnect: bool,
) -> Result<AppSettings, String> {
    let _write_guard = state.settings_write_lock.lock().await;
    let settings = settings.normalize();
    let previous = state.settings.read().await.clone();
    state
        .persistence
        .save_settings(&settings)
        .map_err(error_message)?;
    let autostart = app.autolaunch();
    if settings.autostart {
        autostart.enable().map_err(error_message)?;
    } else {
        autostart.disable().map_err(error_message)?;
    }
    *state.settings.write().await = settings.clone();
    if previous.alerts.periods != settings.alerts.periods {
        for item in previous.watchlist.iter().chain(settings.watchlist.iter()) {
            state.alerts.reset_symbol(item.market, &item.symbol);
        }
    }
    for item in &previous.watchlist {
        let changed = settings
            .watchlist
            .iter()
            .find(|candidate| candidate.market == item.market && candidate.symbol == item.symbol)
            .map_or(true, |candidate| {
                candidate.alert_enabled != item.alert_enabled
                    || candidate.rise_threshold != item.rise_threshold
                    || candidate.fall_threshold != item.fall_threshold
            });
        if changed {
            state.alerts.reset_symbol(item.market, &item.symbol);
        }
    }
    if stream_reconnect_required(&previous, &settings, force_reconnect) {
        let catalog = state.catalog.read().await.clone();
        state
            .market_streams
            .replace_watchlist(&settings.watchlist, &settings.proxy, &catalog)
            .await;
    }
    if let Some(window) = app.get_webview_window("main") {
        apply_main_topmost(&app, settings.always_on_top)?;
        window
            .set_resizable(!(settings.lock_size || settings.lock_window))
            .map_err(error_message)?;
        let height = widget_height(settings.visible_rows);
        window
            .set_size(Size::Logical(LogicalSize::new(
                settings.window_geometry.width,
                height,
            )))
            .map_err(error_message)?;
    }
    let _ = app.emit("settings-updated", &settings);
    Ok(settings)
}

#[tauri::command]
pub async fn save_window_geometry(
    state: State<'_, AppState>,
    visible_rows: u8,
    width: f64,
    x: Option<i32>,
    y: Option<i32>,
) -> Result<(), String> {
    let _write_guard = state.settings_write_lock.lock().await;
    let mut settings = state.settings.read().await.clone();
    settings.visible_rows = visible_rows;
    settings.window_geometry.width = width;
    settings.window_geometry.x = x;
    settings.window_geometry.y = y;
    let settings = settings.normalize();
    state
        .persistence
        .save_settings(&settings)
        .map_err(error_message)?;
    *state.settings.write().await = settings;
    Ok(())
}

#[tauri::command]
pub async fn sync_contracts(state: State<'_, AppState>) -> Result<SyncResult, String> {
    let settings = state.settings.read().await.clone();
    sync_catalog(
        &state.persistence,
        &state.catalog,
        &state.market_streams,
        &settings,
    )
    .await
    .map_err(error_message)
}

pub(crate) async fn sync_catalog(
    persistence: &Arc<Persistence>,
    catalog_state: &Arc<RwLock<Vec<Contract>>>,
    market_streams: &Arc<MarketStreamManager>,
    settings: &AppSettings,
) -> anyhow::Result<SyncResult> {
    let password = credentials::get_proxy_password(&settings.proxy.username);
    let client = build_http_client(&settings.proxy, password.as_deref())?;
    let (usdm, coinm) = tokio::join!(
        fetch_contracts(&client, Market::Usdm),
        fetch_contracts(&client, Market::Coinm)
    );
    let synced_at = Utc::now().timestamp_millis();

    let mut catalog = catalog_state.write().await;
    let mut usdm_count = 0;
    let mut coinm_count = 0;
    match usdm {
        Ok(contracts) => {
            usdm_count = contracts.len();
            persistence.replace_contracts(Market::Usdm, &contracts, synced_at)?;
            catalog.retain(|contract| contract.market != Market::Usdm);
            catalog.extend(contracts);
            let _ =
                persistence.append_recovery("USDⓈ-M 合约目录", "交易对列表同步已恢复", synced_at);
        }
        Err(error) => {
            let _ = persistence.append_log(LogInput {
                level: "error",
                source: "USDⓈ-M 合约目录",
                code: "SYNC_FAILED",
                message: &error.to_string(),
                at: synced_at,
            });
        }
    }
    match coinm {
        Ok(contracts) => {
            coinm_count = contracts.len();
            persistence.replace_contracts(Market::Coinm, &contracts, synced_at)?;
            catalog.retain(|contract| contract.market != Market::Coinm);
            catalog.extend(contracts);
            let _ =
                persistence.append_recovery("COIN-M 合约目录", "交易对列表同步已恢复", synced_at);
        }
        Err(error) => {
            let _ = persistence.append_log(LogInput {
                level: "error",
                source: "COIN-M 合约目录",
                code: "SYNC_FAILED",
                message: &error.to_string(),
                at: synced_at,
            });
        }
    }
    catalog.sort_by(|a, b| a.symbol.cmp(&b.symbol));
    let catalog_snapshot = catalog.clone();
    drop(catalog);

    market_streams
        .replace_watchlist(&settings.watchlist, &settings.proxy, &catalog_snapshot)
        .await;

    if usdm_count == 0 && coinm_count == 0 {
        anyhow::bail!("两个市场的交易对同步都失败，正在继续使用上次有效列表");
    }
    Ok(SyncResult {
        usdm_count,
        coinm_count,
        synced_at,
    })
}

#[tauri::command]
pub fn save_proxy_password(username: String, password: String) -> Result<(), String> {
    credentials::save_proxy_password(&username, &password).map_err(error_message)
}

#[tauri::command]
pub async fn test_proxy(proxy: ProxySettings, password: String) -> Result<String, String> {
    let client = build_http_client(&proxy, (!password.is_empty()).then_some(password.as_str()))
        .map_err(error_message)?;
    let (usdm, coinm) = tokio::join!(
        client.get("https://fapi.binance.com/fapi/v1/time").send(),
        client.get("https://dapi.binance.com/dapi/v1/time").send(),
    );
    usdm.map_err(error_message)?
        .error_for_status()
        .map_err(error_message)?;
    coinm
        .map_err(error_message)?
        .error_for_status()
        .map_err(error_message)?;
    Ok("USDⓈ-M 与 COIN-M 连接正常".into())
}

#[tauri::command]
pub fn get_logs(
    state: State<'_, AppState>,
    offset: u32,
    limit: u32,
) -> Result<Vec<LogRow>, String> {
    state
        .persistence
        .query_logs(offset, limit)
        .map_err(error_message)
}

#[tauri::command]
pub fn clear_logs(state: State<'_, AppState>) -> Result<(), String> {
    state.persistence.clear_logs().map_err(error_message)
}

#[tauri::command]
pub async fn get_alerts(state: State<'_, AppState>) -> Result<Vec<AlertRecord>, String> {
    Ok(state.alerts.active().await)
}

#[tauri::command]
pub async fn get_alert_states(state: State<'_, AppState>) -> Result<Vec<AlertRecord>, String> {
    Ok(state.alerts.all_current().await)
}

#[tauri::command]
pub async fn dismiss_alert(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    state.alerts.dismiss(&id).await.map_err(error_message)?;
    if state.alerts.active().await.is_empty() {
        if let Some(window) = app.get_webview_window("alerts") {
            window.hide().map_err(error_message)?;
        }
    }
    Ok(())
}

#[tauri::command]
pub fn open_settings(app: AppHandle) -> Result<(), String> {
    if let Some(main) = app.get_webview_window("main") {
        main.set_always_on_top(false).map_err(error_message)?;
    }
    if let Some(window) = app.get_webview_window("settings") {
        window.show().map_err(error_message)?;
        window.set_focus().map_err(error_message)?;
        return Ok(());
    }
    WebviewWindowBuilder::new(
        &app,
        "settings",
        WebviewUrl::App("index.html?view=settings".into()),
    )
    .title("永续价格悬浮窗 · 设置")
    .inner_size(920.0, 650.0)
    .min_inner_size(720.0, 540.0)
    .decorations(false)
    .center()
    .build()
    .map_err(error_message)?;
    Ok(())
}

#[tauri::command]
pub async fn close_settings(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("settings") {
        window.hide().map_err(error_message)?;
    }
    let always_on_top = state.settings.read().await.always_on_top;
    apply_main_topmost(&app, always_on_top)
}

#[tauri::command]
pub async fn show_main(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let always_on_top = state.settings.read().await.always_on_top;
    show_main_window(&app, always_on_top)
}

#[tauri::command]
pub fn hide_main(app: AppHandle) -> Result<(), String> {
    app.get_webview_window("main")
        .ok_or_else(|| "主体窗口不存在".to_string())?
        .hide()
        .map_err(error_message)
}

#[tauri::command]
pub fn set_mouse_passthrough(app: AppHandle, enabled: bool) -> Result<(), String> {
    apply_mouse_passthrough(&app, enabled)
}

#[tauri::command]
pub fn exit_app(app: AppHandle) {
    app.exit(0);
}

pub(crate) fn apply_mouse_passthrough(app: &AppHandle, enabled: bool) -> Result<(), String> {
    let window = app.get_webview_window("main").ok_or("主体窗口不存在")?;
    window
        .set_ignore_cursor_events(enabled)
        .map_err(error_message)?;
    let _ = app.emit("mouse-passthrough-changed", enabled);
    Ok(())
}

pub(crate) fn settings_window_visible(app: &AppHandle) -> bool {
    app.get_webview_window("settings")
        .and_then(|window| window.is_visible().ok())
        .unwrap_or(false)
}

pub(crate) fn apply_main_topmost(app: &AppHandle, requested: bool) -> Result<(), String> {
    let window = app.get_webview_window("main").ok_or("主体窗口不存在")?;
    let should_be_topmost = requested && !settings_window_visible(app);
    window
        .set_always_on_top(should_be_topmost)
        .map_err(error_message)?;
    #[cfg(windows)]
    if should_be_topmost {
        use windows::Win32::UI::WindowsAndMessaging::{
            SetWindowPos, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
        };
        let hwnd = window.hwnd().map_err(error_message)?;
        unsafe {
            SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            )
        }
        .map_err(error_message)?;
    }
    Ok(())
}

pub(crate) fn show_main_window(app: &AppHandle, always_on_top: bool) -> Result<(), String> {
    let window = app.get_webview_window("main").ok_or("主体窗口不存在")?;
    window.show().map_err(error_message)?;
    apply_main_topmost(app, always_on_top)?;
    window.set_focus().map_err(error_message)
}

fn error_message(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn stream_reconnect_required(
    previous: &AppSettings,
    next: &AppSettings,
    force_reconnect: bool,
) -> bool {
    force_reconnect
        || previous.proxy != next.proxy
        || stream_subscriptions(&previous.watchlist) != stream_subscriptions(&next.watchlist)
}

fn stream_subscriptions(watchlist: &[crate::models::WatchItem]) -> HashSet<(Market, String)> {
    watchlist
        .iter()
        .map(|item| (item.market, item.symbol.to_uppercase()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alert_and_appearance_changes_do_not_restart_market_streams() {
        let previous = AppSettings::default();
        let mut next = previous.clone();
        next.watchlist[0].rise_threshold += 1.0;
        next.appearance.background_opacity = 40;

        assert!(!stream_reconnect_required(&previous, &next, false));
    }

    #[test]
    fn subscription_proxy_and_explicit_requests_restart_market_streams() {
        let previous = AppSettings::default();
        let mut added_symbol = previous.clone();
        let mut item = added_symbol.watchlist[0].clone();
        item.symbol = "BNBUSDT".into();
        added_symbol.watchlist.push(item);
        assert!(stream_reconnect_required(&previous, &added_symbol, false));

        let mut changed_proxy = previous.clone();
        changed_proxy.proxy.host = "127.0.0.1".into();
        assert!(stream_reconnect_required(&previous, &changed_proxy, false));

        assert!(stream_reconnect_required(&previous, &previous, true));
    }

    #[test]
    fn watchlist_reordering_does_not_restart_market_streams() {
        let previous = AppSettings::default();
        let mut reordered = previous.clone();
        reordered.watchlist.reverse();

        assert!(!stream_reconnect_required(&previous, &reordered, false));
    }
}

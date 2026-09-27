mod alert_engine;
mod alert_service;
mod app_logging;
mod commands;
mod contract_catalog;
mod credentials;
mod market_stream;
mod models;
mod persistence;
mod wallet;

use std::sync::Arc;

use alert_service::AlertService;
use chrono::Utc;
use commands::AppState;
use market_stream::MarketStreamManager;
use models::widget_height;
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    LogicalSize, Manager, PhysicalPosition, Position, Size,
};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_autostart::ManagerExt;
use tokio::sync::{Mutex, RwLock};

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--autostart"]),
        ))
        .on_window_event(|window, event| {
            if window.label() == "main"
                && matches!(
                    event,
                    tauri::WindowEvent::Moved(_) | tauri::WindowEvent::Resized(_)
                )
            {
                let _ = commands::hide_wallet_details(window.app_handle().clone());
            }
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    let _ = commands::hide_wallet_details(window.app_handle().clone());
                }
                api.prevent_close();
                let _ = window.hide();
                if window.label() == "settings" {
                    let app = window.app_handle().clone();
                    tauri::async_runtime::spawn(async move {
                        let always_on_top =
                            app.state::<AppState>().settings.read().await.always_on_top;
                        let _ = commands::apply_main_topmost(&app, always_on_top);
                    });
                }
            }
        })
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            let persistence = Arc::new(persistence::Persistence::open(data_dir)?);
            let settings = persistence.load_settings().unwrap_or_else(|error| {
                let _ = persistence.append_log(crate::app_logging::LogInput {
                    level: "error",
                    source: "本地设置",
                    code: "SETTINGS_LOAD_FAILED",
                    message: &error.to_string(),
                    at: Utc::now().timestamp_millis(),
                });
                crate::models::AppSettings::default()
            });
            let catalog = persistence.load_contracts().unwrap_or_else(|error| {
                let _ = persistence.append_log(crate::app_logging::LogInput {
                    level: "error",
                    source: "合约目录缓存",
                    code: "CATALOG_LOAD_FAILED",
                    message: &error.to_string(),
                    at: Utc::now().timestamp_millis(),
                });
                Vec::new()
            });
            let initial_catalog = catalog.clone();
            let market_streams = MarketStreamManager::new(Arc::clone(&persistence));
            let settings_state = Arc::new(RwLock::new(settings.clone()));
            let catalog_state = Arc::new(RwLock::new(catalog));
            let alerts = AlertService::new(
                Arc::clone(&settings_state),
                Arc::clone(&market_streams),
                Arc::clone(&persistence),
            );
            app.manage(AppState {
                wallet: Mutex::new(wallet::WalletState::default()),
                wallet_snapshot: RwLock::new(wallet::WalletSnapshot::default()),
                persistence: Arc::clone(&persistence),
                settings: settings_state,
                settings_write_lock: Mutex::new(()),
                catalog: Arc::clone(&catalog_state),
                market_streams: Arc::clone(&market_streams),
                alerts: Arc::clone(&alerts),
            });

            if let Some(window) = app.get_webview_window("main") {
                let height = widget_height(settings.visible_rows);
                let _ = window.set_size(Size::Logical(LogicalSize::new(
                    settings.window_geometry.width,
                    height,
                )));
                let _ = window.set_always_on_top(settings.always_on_top);
                if let (Some(x), Some(y)) = (settings.window_geometry.x, settings.window_geometry.y)
                {
                    let saved_position = PhysicalPosition::new(x, y);
                    let intersects_monitor = window
                        .available_monitors()
                        .map(|monitors| {
                            monitors.into_iter().any(|monitor| {
                                let position = monitor.position();
                                let size = monitor.size();
                                x >= position.x - 100
                                    && y >= position.y - 100
                                    && x < position.x + size.width as i32
                                    && y < position.y + size.height as i32
                            })
                        })
                        .unwrap_or(false);
                    if intersects_monitor {
                        let _ = window.set_position(Position::Physical(saved_position));
                    } else {
                        let _ = window.center();
                    }
                }
            }

            let wallet_app = app.handle().clone();
            tauri::async_runtime::spawn(wallet::run(wallet_app));
            let stream_settings = settings.clone();
            tauri::async_runtime::spawn(async move {
                market_streams
                    .replace_watchlist(
                        &stream_settings.watchlist,
                        &stream_settings.proxy,
                        &initial_catalog,
                    )
                    .await;
            });

            let autostart = app.autolaunch();
            if settings.autostart {
                let _ = autostart.enable();
            } else {
                let _ = autostart.disable();
            }
            let alert_app = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                alerts.run(alert_app).await;
            });

            let show = MenuItem::with_id(app, "show", "显示悬浮窗", true, None::<&str>)?;
            let hide = MenuItem::with_id(app, "hide", "隐藏悬浮窗", true, None::<&str>)?;
            let settings_item = MenuItem::with_id(app, "settings", "设置", true, None::<&str>)?;
            let disable_mouse_passthrough = MenuItem::with_id(
                app,
                "disable-mouse-passthrough",
                "关闭鼠标穿透",
                true,
                None::<&str>,
            )?;
            let exit = MenuItem::with_id(app, "exit", "退出", true, None::<&str>)?;
            let menu = Menu::with_items(
                app,
                &[
                    &show,
                    &hide,
                    &settings_item,
                    &disable_mouse_passthrough,
                    &exit,
                ],
            )?;
            TrayIconBuilder::new()
                .icon(
                    app.default_window_icon()
                        .expect("application icon missing")
                        .clone(),
                )
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        let app = app.clone();
                        tauri::async_runtime::spawn(async move {
                            let always_on_top =
                                app.state::<AppState>().settings.read().await.always_on_top;
                            let _ = commands::show_main_window(&app, always_on_top);
                        });
                    }
                    "hide" => {
                        let _ = commands::hide_main(app.clone());
                    }
                    "settings" => {
                        let _ = commands::open_settings(app.clone());
                    }
                    "disable-mouse-passthrough" => {
                        let _ = commands::apply_mouse_passthrough(app, false);
                    }
                    "exit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;

            let topmost_app = app.handle().clone();
            let topmost_settings = app.state::<AppState>().settings.clone();
            tauri::async_runtime::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
                loop {
                    interval.tick().await;
                    if !topmost_settings.read().await.always_on_top
                        || commands::settings_window_visible(&topmost_app)
                    {
                        continue;
                    }
                    let main_visible = topmost_app
                        .get_webview_window("main")
                        .and_then(|window| window.is_visible().ok())
                        .unwrap_or(false);
                    if main_visible {
                        let _ = commands::apply_main_topmost(&topmost_app, true);
                    }
                }
            });

            let cleanup_store = Arc::clone(&persistence);
            tauri::async_runtime::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
                loop {
                    interval.tick().await;
                    let _ = cleanup_store.cleanup(Utc::now().timestamp_millis());
                }
            });

            let sync_store = Arc::clone(&persistence);
            let sync_settings = app.state::<AppState>().settings.clone();
            let sync_catalog_state = Arc::clone(&catalog_state);
            let sync_market_streams = app.state::<AppState>().market_streams.clone();
            tauri::async_runtime::spawn(async move {
                const DAY_MS: i64 = 24 * 60 * 60 * 1_000;
                loop {
                    let now = Utc::now().timestamp_millis();
                    let stale = [crate::models::Market::Usdm, crate::models::Market::Coinm]
                        .into_iter()
                        .any(|market| {
                            sync_store
                                .last_contract_sync(market)
                                .ok()
                                .flatten()
                                .map_or(true, |last| now - last >= DAY_MS)
                        });
                    if stale {
                        let settings = sync_settings.read().await.clone();
                        let _ = commands::sync_catalog(
                            &sync_store,
                            &sync_catalog_state,
                            &sync_market_streams,
                            &settings,
                        )
                        .await;
                    }
                    let delay = if stale {
                        std::time::Duration::from_secs(5)
                    } else {
                        std::time::Duration::from_secs(60 * 60)
                    };
                    tokio::time::sleep(delay).await;
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::toggle_wallet_details,
            commands::hide_wallet_details,
            wallet::get_wallet_balance,
            wallet::get_wallet_status,
            wallet::save_wallet_credentials,
            commands::get_bootstrap,
            commands::get_tickers,
            commands::save_settings,
            commands::save_window_geometry,
            commands::sync_contracts,
            commands::save_proxy_password,
            commands::test_proxy,
            commands::get_logs,
            commands::clear_logs,
            commands::get_alerts,
            commands::get_alert_states,
            commands::dismiss_alert,
            commands::open_settings,
            commands::close_settings,
            commands::show_main,
            commands::hide_main,
            commands::set_mouse_passthrough,
            commands::exit_app,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run perpetual price widget");
}

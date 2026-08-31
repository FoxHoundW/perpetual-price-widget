use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    sync::{Arc, Mutex},
    time::Duration,
};

use chrono::Utc;
use tauri::{
    Emitter, LogicalPosition, LogicalSize, Manager, Position, Size, WebviewUrl,
    WebviewWindowBuilder,
};
use tokio::sync::RwLock;

use crate::{
    alert_engine::{AlertEngine, Evaluation},
    contract_catalog::{build_http_client, fetch_minute_klines},
    credentials,
    market_stream::MarketStreamManager,
    models::{
        AlertRecord, AppSettings, ConnectionStatus, HiddenAlertBehavior, Market, PriceSample,
        ALERT_SAMPLE_RETENTION_MINUTES, MAX_ALERT_PERIOD_MINUTES,
    },
    persistence::Persistence,
};

pub struct AlertService {
    engine: Mutex<AlertEngine>,
    samples: Mutex<HashMap<(Market, String), VecDeque<PriceSample>>>,
    last_persisted: Mutex<HashMap<(Market, String), i64>>,
    history_attempts: Mutex<HashMap<(Market, String), HistoryAttempt>>,
    last_history_request: Mutex<i64>,
    history_failures: Mutex<HashSet<(Market, String)>>,
    active: RwLock<Vec<AlertRecord>>,
    settings: Arc<RwLock<AppSettings>>,
    market_streams: Arc<MarketStreamManager>,
    persistence: Arc<Persistence>,
}

impl AlertService {
    pub fn new(
        settings: Arc<RwLock<AppSettings>>,
        market_streams: Arc<MarketStreamManager>,
        persistence: Arc<Persistence>,
    ) -> Arc<Self> {
        let now = Utc::now().timestamp_millis();
        let active = persistence.load_active_alerts(now).unwrap_or_default();
        let mut engine = AlertEngine::default();
        engine.restore(persistence.load_trigger_states().unwrap_or_default());
        Arc::new(Self {
            engine: Mutex::new(engine),
            samples: Mutex::new(HashMap::new()),
            last_persisted: Mutex::new(HashMap::new()),
            history_attempts: Mutex::new(HashMap::new()),
            last_history_request: Mutex::new(0),
            history_failures: Mutex::new(HashSet::new()),
            active: RwLock::new(active),
            settings,
            market_streams,
            persistence,
        })
    }

    pub async fn run(self: Arc<Self>, app: tauri::AppHandle) {
        let mut interval = tokio::time::interval(Duration::from_millis(250));
        loop {
            interval.tick().await;
            let now = Utc::now().timestamp_millis();
            self.expire(now).await;
            let settings = self.settings.read().await.clone();
            self.ensure_history(&settings, now).await;
            let snapshots = self.market_streams.snapshots(now).await;
            for snapshot in snapshots {
                if snapshot.status != ConnectionStatus::Live || snapshot.price <= 0.0 {
                    continue;
                }
                let Some(watch) = settings
                    .watchlist
                    .iter()
                    .find(|item| item.market == snapshot.market && item.symbol == snapshot.symbol)
                else {
                    continue;
                };
                if !watch.alert_enabled {
                    continue;
                }
                let samples =
                    self.update_samples(snapshot.market, &snapshot.symbol, now, snapshot.price);
                let (alerts, trigger_states) = {
                    let mut engine = self.engine.lock().expect("alert engine mutex poisoned");
                    let alerts = engine.evaluate(Evaluation {
                        now,
                        market: snapshot.market,
                        symbol: &snapshot.symbol,
                        price: snapshot.price,
                        samples: &samples,
                        watch,
                        periods: &settings.alerts.periods,
                        rolling_24h_change: Some(snapshot.change_24h),
                    });
                    let states = (!alerts.is_empty()).then(|| engine.snapshot().clone());
                    (alerts, states)
                };
                if alerts.is_empty() {
                    continue;
                }
                if let Some(states) = trigger_states {
                    let _ = self.persistence.replace_trigger_states(&states);
                }
                for alert in &alerts {
                    let _ = self.persistence.save_alert(alert);
                }
                self.active.write().await.extend(alerts);
                let current = self.active().await;
                let _ = app.emit("alerts-updated", &current);
                if settings.alerts.hidden_behavior == HiddenAlertBehavior::SideCard
                    || app
                        .get_webview_window("main")
                        .and_then(|window| window.is_visible().ok())
                        .unwrap_or(false)
                {
                    let _ = show_alert_window(&app);
                }
            }
        }
    }

    pub async fn active(&self) -> Vec<AlertRecord> {
        self.active
            .read()
            .await
            .iter()
            .filter(|alert| !alert.dismissed)
            .cloned()
            .collect()
    }

    pub async fn all_current(&self) -> Vec<AlertRecord> {
        self.active.read().await.clone()
    }

    pub async fn dismiss(&self, id: &str) -> anyhow::Result<()> {
        self.persistence.dismiss_alert(id)?;
        if let Some(alert) = self
            .active
            .write()
            .await
            .iter_mut()
            .find(|alert| alert.id == id)
        {
            alert.dismissed = true;
        }
        Ok(())
    }

    pub fn reset_symbol(&self, market: Market, symbol: &str) {
        self.engine
            .lock()
            .expect("alert engine mutex poisoned")
            .reset_symbol(market, symbol);
        self.samples
            .lock()
            .expect("samples mutex poisoned")
            .remove(&(market, symbol.to_owned()));
        let _ = self
            .persistence
            .delete_trigger_states_for_symbol(market, symbol);
    }

    fn update_samples(
        &self,
        market: Market,
        symbol: &str,
        now: i64,
        price: f64,
    ) -> Vec<PriceSample> {
        let minute = now / 60_000 * 60_000;
        let key = (market, symbol.to_owned());
        let result = {
            let mut all = self.samples.lock().expect("samples mutex poisoned");
            let samples = all.entry(key.clone()).or_insert_with(|| {
                self.persistence
                    .load_samples(
                        market,
                        symbol,
                        now - ALERT_SAMPLE_RETENTION_MINUTES * 60_000,
                    )
                    .unwrap_or_default()
                    .into()
            });
            if samples
                .back()
                .is_some_and(|sample| sample.timestamp == minute)
            {
                if let Some(last) = samples.back_mut() {
                    last.price = price;
                }
            } else {
                samples.push_back(PriceSample {
                    timestamp: minute,
                    price,
                });
            }
            while samples.len() > ALERT_SAMPLE_RETENTION_MINUTES as usize {
                samples.pop_front();
            }
            samples.iter().cloned().collect::<Vec<_>>()
        };

        let mut persisted = self
            .last_persisted
            .lock()
            .expect("persist timestamp mutex poisoned");
        if now - persisted.get(&key).copied().unwrap_or_default() >= 5_000 {
            let _ = self.persistence.upsert_sample(
                market,
                symbol,
                &PriceSample {
                    timestamp: minute,
                    price,
                },
            );
            persisted.insert(key, now);
        }
        result
    }

    async fn expire(&self, now: i64) {
        self.active
            .write()
            .await
            .retain(|alert| alert.expires_at > now);
    }

    async fn ensure_history(&self, settings: &AppSettings, now: i64) {
        let Some(required_period) = settings.alerts.periods.iter().copied().max() else {
            return;
        };
        let required_period = required_period.min(MAX_ALERT_PERIOD_MINUTES);
        {
            let last_request = self
                .last_history_request
                .lock()
                .expect("history request timestamp mutex poisoned");
            if now - *last_request < 1_000 {
                return;
            }
        }
        let candidates = {
            let samples = self.samples.lock().expect("samples mutex poisoned");
            let attempts = self
                .history_attempts
                .lock()
                .expect("history attempts mutex poisoned");
            settings
                .watchlist
                .iter()
                .filter(|item| item.alert_enabled)
                .filter(|item| {
                    !history_is_ready(
                        samples.get(&(item.market, item.symbol.clone())),
                        now,
                        required_period,
                    )
                })
                .filter(|item| {
                    let attempt = attempts
                        .get(&(item.market, item.symbol.clone()))
                        .copied()
                        .unwrap_or_default();
                    now - attempt.at >= history_retry_delay(attempt.failures)
                })
                .cloned()
                .collect::<Vec<_>>()
        };
        if candidates.is_empty() {
            return;
        }
        let password = credentials::get_proxy_password(&settings.proxy.username);
        let Ok(client) = build_http_client(&settings.proxy, password.as_deref()) else {
            return;
        };
        if let Some(item) = candidates.into_iter().next() {
            let key = (item.market, item.symbol.clone());
            {
                let mut attempts = self
                    .history_attempts
                    .lock()
                    .expect("history attempts mutex poisoned");
                attempts.entry(key.clone()).or_default().at = now;
            }
            *self
                .last_history_request
                .lock()
                .expect("history request timestamp mutex poisoned") = now;
            let source = format!("{} {} 异动历史", market_name(item.market), item.symbol);
            let history_limit = required_period.saturating_add(6).clamp(66, 1500);
            match fetch_minute_klines(&client, item.market, &item.symbol, history_limit).await {
                Ok(history) => {
                    let mut samples = self.samples.lock().expect("samples mutex poisoned");
                    merge_history(samples.entry(key.clone()).or_default(), history);
                    self.history_attempts
                        .lock()
                        .expect("history attempts mutex poisoned")
                        .remove(&key);
                    let recovered = self
                        .history_failures
                        .lock()
                        .expect("history failures mutex poisoned")
                        .remove(&key);
                    if recovered {
                        let _ = self.persistence.append_recovery(
                            &source,
                            "分钟基线已补齐，异动监测恢复",
                            now,
                        );
                    }
                }
                Err(error) => {
                    let retry_at = retry_at_from_error(&error.to_string(), now);
                    {
                        let mut attempts = self
                            .history_attempts
                            .lock()
                            .expect("history attempts mutex poisoned");
                        let attempt = attempts.entry(key.clone()).or_default();
                        attempt.at = retry_at;
                        attempt.failures = attempt.failures.saturating_add(1);
                    }
                    self.history_failures
                        .lock()
                        .expect("history failures mutex poisoned")
                        .insert(key);
                    let _ = self.persistence.append_log(crate::app_logging::LogInput {
                        level: "error",
                        source: &source,
                        code: "KLINE_BACKFILL_FAILED",
                        message: &error.to_string(),
                        at: now,
                    });
                }
            }
        }
    }
}

#[derive(Clone, Copy, Default)]
struct HistoryAttempt {
    at: i64,
    failures: u8,
}

fn history_retry_delay(failures: u8) -> i64 {
    let exponent = failures.saturating_sub(1).min(6);
    (5_000_i64 * (1_i64 << exponent)).min(300_000)
}

fn retry_at_from_error(message: &str, now: i64) -> i64 {
    let Some(value) = message.split("banned until ").nth(1) else {
        return now;
    };
    value
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>()
        .parse::<i64>()
        .ok()
        .filter(|timestamp| *timestamp > now)
        .unwrap_or(now)
}

fn history_is_ready(
    samples: Option<&VecDeque<PriceSample>>,
    now: i64,
    required_period: u16,
) -> bool {
    let target = now - i64::from(required_period) * 60_000;
    samples
        .and_then(|items| items.front())
        .is_some_and(|sample| sample.timestamp <= target)
}

fn merge_history(samples: &mut VecDeque<PriceSample>, history: Vec<PriceSample>) {
    let mut by_minute = BTreeMap::new();
    for sample in history.into_iter().chain(samples.iter().cloned()) {
        by_minute.insert(sample.timestamp, sample);
    }
    *samples = by_minute.into_values().collect();
    while samples.len() > ALERT_SAMPLE_RETENTION_MINUTES as usize {
        samples.pop_front();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_history_keeps_requesting_until_period_is_covered() {
        let now = 1_500 * 60_000;
        let partial = VecDeque::from([PriceSample {
            timestamp: now - 60 * 60_000,
            price: 100.0,
        }]);
        assert!(!history_is_ready(Some(&partial), now, 1440));
        assert!(history_is_ready(Some(&partial), now, 60));
    }

    #[test]
    fn history_merge_preserves_newer_live_price() {
        let mut live = VecDeque::from([PriceSample {
            timestamp: 120_000,
            price: 102.0,
        }]);
        merge_history(
            &mut live,
            vec![
                PriceSample {
                    timestamp: 60_000,
                    price: 99.0,
                },
                PriceSample {
                    timestamp: 120_000,
                    price: 100.0,
                },
            ],
        );
        assert_eq!(live.len(), 2);
        assert_eq!(live.back().unwrap().price, 102.0);
    }

    #[test]
    fn history_retry_uses_bounded_backoff() {
        assert_eq!(history_retry_delay(0), 5_000);
        assert_eq!(history_retry_delay(1), 5_000);
        assert_eq!(history_retry_delay(2), 10_000);
        assert_eq!(history_retry_delay(20), 300_000);
    }

    #[test]
    fn rate_limit_ban_pauses_until_server_timestamp() {
        assert_eq!(
            retry_at_from_error(
                "分钟行情返回错误状态 418: IP banned until 1788151859911. Please retry",
                1788141544298,
            ),
            1788151859911
        );
        assert_eq!(retry_at_from_error("普通网络错误", 42), 42);
    }
}

fn market_name(market: Market) -> &'static str {
    match market {
        Market::Usdm => "USDⓈ-M",
        Market::Coinm => "COIN-M",
    }
}

fn show_alert_window(app: &tauri::AppHandle) -> anyhow::Result<()> {
    let main = app
        .get_webview_window("main")
        .ok_or_else(|| anyhow::anyhow!("主体窗口不存在"))?;
    let main_position = main.outer_position()?;
    let main_size = main.outer_size()?;
    let scale = main.scale_factor()?;
    let monitor = main
        .current_monitor()?
        .ok_or_else(|| anyhow::anyhow!("无法获取当前显示器"))?;
    let monitor_position = monitor.position();
    let monitor_size = monitor.size();
    let alert_width = (264.0 * scale) as i32;
    let monitor_mid = monitor_position.x + monitor_size.width as i32 / 2;
    let main_mid = main_position.x + main_size.width as i32 / 2;
    let x = if main_mid <= monitor_mid {
        main_position.x + main_size.width as i32 + (8.0 * scale) as i32
    } else {
        main_position.x - alert_width - (8.0 * scale) as i32
    };
    let x = x.clamp(
        monitor_position.x,
        monitor_position.x + monitor_size.width as i32 - alert_width,
    );
    let logical_height = f64::from(main_size.height) / scale;

    let window = if let Some(window) = app.get_webview_window("alerts") {
        window
    } else {
        WebviewWindowBuilder::new(
            app,
            "alerts",
            WebviewUrl::App("index.html?view=alerts".into()),
        )
        .title("异动提醒")
        .inner_size(264.0, logical_height)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .resizable(false)
        .skip_taskbar(true)
        .build()?
    };
    window.set_size(Size::Logical(LogicalSize::new(264.0, logical_height)))?;
    window.set_position(Position::Logical(LogicalPosition::new(
        f64::from(x) / scale,
        f64::from(main_position.y) / scale,
    )))?;
    window.show()?;
    Ok(())
}

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const WIDGET_MIN_WIDTH: f64 = 300.0;
pub const WIDGET_FRAME_HEIGHT: f64 = 22.0;
pub const WIDGET_ROW_HEIGHT: f64 = 44.0;
pub const ALERT_PERIODS: [u16; 6] = [5, 10, 60, 360, 720, 1440];
pub const MAX_ALERT_PERIOD_MINUTES: u16 = 1440;
pub const ALERT_SAMPLE_RETENTION_MINUTES: i64 = 1500;

pub fn widget_height(visible_rows: u8) -> f64 {
    WIDGET_FRAME_HEIGHT + f64::from(visible_rows) * WIDGET_ROW_HEIGHT
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Market {
    Usdm,
    Coinm,
}

impl Market {
    pub fn rest_base(self) -> &'static str {
        match self {
            Self::Usdm => "https://fapi.binance.com",
            Self::Coinm => "https://dapi.binance.com",
        }
    }

    pub fn exchange_info_path(self) -> &'static str {
        match self {
            Self::Usdm => "/fapi/v1/exchangeInfo",
            Self::Coinm => "/dapi/v1/exchangeInfo",
        }
    }

    pub fn websocket_base(self) -> &'static str {
        match self {
            Self::Usdm => "wss://fstream.binance.com/market/stream",
            Self::Coinm => "wss://dstream.binance.com/market/stream",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Contract {
    pub market: Market,
    pub symbol: String,
    pub base_asset: String,
    pub quote_asset: String,
    pub margin_asset: String,
    pub contract_type: String,
    pub tick_size: String,
    pub price_decimals: u8,
    pub status: ContractStatus,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ContractStatus {
    Trading,
    Delisted,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WatchItem {
    pub market: Market,
    pub symbol: String,
    pub alert_enabled: bool,
    pub rise_threshold: f64,
    pub fall_threshold: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum HiddenAlertBehavior {
    SideCard,
    RecordOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AlertSettings {
    pub periods: Vec<u16>,
    pub cooldown_minutes: u8,
    pub retention_hours: u8,
    pub hidden_behavior: HiddenAlertBehavior,
    pub default_enabled: bool,
    pub default_rise_threshold: f64,
    pub default_fall_threshold: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppearanceSettings {
    pub background_color: String,
    pub background_opacity: u8,
    pub font_opacity: u8,
    #[serde(default = "default_font_shadow_size")]
    pub font_shadow_size: f64,
    pub alert_up_color: String,
    pub alert_down_color: String,
    pub alert_accent_opacity: u8,
    pub alert_card_background_color: String,
    pub alert_card_background_opacity: u8,
}

fn default_font_shadow_size() -> f64 {
    3.0
}

impl Default for AppearanceSettings {
    fn default() -> Self {
        Self {
            background_color: "#0D131C".into(),
            background_opacity: 99,
            font_opacity: 100,
            font_shadow_size: default_font_shadow_size(),
            alert_up_color: "#49E19A".into(),
            alert_down_color: "#FF7181".into(),
            alert_accent_opacity: 100,
            alert_card_background_color: "#111923".into(),
            alert_card_background_opacity: 94,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ProxyMode {
    System,
    Http,
    Socks5,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProxySettings {
    pub mode: ProxyMode,
    pub host: String,
    pub port: Option<u16>,
    pub username: String,
}

impl Default for ProxySettings {
    fn default() -> Self {
        Self {
            mode: ProxyMode::System,
            host: String::new(),
            port: None,
            username: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[serde(default)]
pub struct AppSettings {
    pub autostart: bool,
    pub always_on_top: bool,
    pub visible_rows: u8,
    pub refresh_interval_ms: u16,
    pub lock_size: bool,
    pub lock_window: bool,
    pub window_geometry: WindowGeometry,
    pub watchlist: Vec<WatchItem>,
    pub alerts: AlertSettings,
    pub appearance: AppearanceSettings,
    pub proxy: ProxySettings,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WindowGeometry {
    pub x: Option<i32>,
    pub y: Option<i32>,
    pub width: f64,
}

impl Default for WindowGeometry {
    fn default() -> Self {
        Self {
            x: None,
            y: None,
            width: 392.0,
        }
    }
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            autostart: true,
            always_on_top: true,
            visible_rows: 3,
            refresh_interval_ms: 500,
            lock_size: false,
            lock_window: false,
            window_geometry: WindowGeometry::default(),
            watchlist: ["BTCUSDT", "ETHUSDT", "SOLUSDT"]
                .into_iter()
                .map(|symbol| WatchItem {
                    market: Market::Usdm,
                    symbol: symbol.to_string(),
                    alert_enabled: true,
                    rise_threshold: 5.0,
                    fall_threshold: 5.0,
                })
                .collect(),
            alerts: AlertSettings {
                periods: vec![5, 10, 60],
                cooldown_minutes: 5,
                retention_hours: 6,
                hidden_behavior: HiddenAlertBehavior::SideCard,
                default_enabled: true,
                default_rise_threshold: 5.0,
                default_fall_threshold: 5.0,
            },
            appearance: AppearanceSettings::default(),
            proxy: ProxySettings {
                mode: ProxyMode::System,
                host: String::new(),
                port: None,
                username: String::new(),
            },
        }
    }
}

impl AppSettings {
    pub fn normalize(mut self) -> Self {
        self.visible_rows = self.visible_rows.clamp(1, 20);
        self.refresh_interval_ms = self.refresh_interval_ms.clamp(100, 2000);
        self.window_geometry.width = self.window_geometry.width.clamp(WIDGET_MIN_WIDTH, 2000.0);
        self.alerts
            .periods
            .retain(|period| ALERT_PERIODS.contains(period));
        self.alerts.periods.sort_unstable();
        self.alerts.periods.dedup();
        let defaults = AppearanceSettings::default();
        self.appearance.background_color = normalize_hex_color(
            &self.appearance.background_color,
            &defaults.background_color,
        );
        self.appearance.background_opacity = self.appearance.background_opacity.min(100);
        self.appearance.font_opacity = self.appearance.font_opacity.clamp(30, 100);
        self.appearance.font_shadow_size = self.appearance.font_shadow_size.clamp(0.0, 6.0);
        self.appearance.alert_up_color =
            normalize_hex_color(&self.appearance.alert_up_color, &defaults.alert_up_color);
        self.appearance.alert_down_color = normalize_hex_color(
            &self.appearance.alert_down_color,
            &defaults.alert_down_color,
        );
        self.appearance.alert_accent_opacity = self.appearance.alert_accent_opacity.clamp(20, 100);
        self.appearance.alert_card_background_color = normalize_hex_color(
            &self.appearance.alert_card_background_color,
            &defaults.alert_card_background_color,
        );
        self.appearance.alert_card_background_opacity =
            self.appearance.alert_card_background_opacity.clamp(20, 100);
        for item in &mut self.watchlist {
            item.symbol = item.symbol.trim().to_uppercase();
            item.rise_threshold = item.rise_threshold.max(0.01);
            item.fall_threshold = item.fall_threshold.max(0.01);
        }
        let mut seen = HashSet::new();
        self.watchlist
            .retain(|item| seen.insert((item.market, item.symbol.clone())));
        self
    }
}

fn normalize_hex_color(value: &str, fallback: &str) -> String {
    let is_valid = value.len() == 7
        && value.starts_with('#')
        && value[1..]
            .chars()
            .all(|character| character.is_ascii_hexdigit());
    if is_valid {
        value.to_ascii_uppercase()
    } else {
        fallback.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{widget_height, AppSettings};

    #[test]
    fn widget_height_matches_visible_rows() {
        assert_eq!(widget_height(1), 66.0);
        assert_eq!(widget_height(3), 154.0);
        assert_eq!(widget_height(20), 902.0);
    }

    #[test]
    fn settings_enforce_compact_minimum_width() {
        let mut settings = AppSettings::default();
        settings.window_geometry.width = 299.0;
        assert_eq!(settings.normalize().window_geometry.width, 300.0);
    }

    #[test]
    fn settings_normalize_personalization() {
        let mut settings = AppSettings::default();
        settings.appearance.background_color = "invalid".into();
        settings.appearance.font_opacity = 1;
        settings.appearance.font_shadow_size = 99.0;
        settings.appearance.alert_up_color = "#abcdef".into();
        settings.appearance.alert_accent_opacity = 101;
        settings.appearance.alert_card_background_opacity = 1;
        let normalized = settings.normalize();
        assert_eq!(normalized.appearance.background_color, "#0D131C");
        assert_eq!(normalized.appearance.font_opacity, 30);
        assert_eq!(normalized.appearance.font_shadow_size, 6.0);
        assert_eq!(normalized.appearance.alert_up_color, "#ABCDEF");
        assert_eq!(normalized.appearance.alert_accent_opacity, 100);
        assert_eq!(normalized.appearance.alert_card_background_opacity, 20);
    }

    #[test]
    fn missing_personalization_uses_defaults() {
        let mut value = serde_json::to_value(AppSettings::default()).unwrap();
        value.as_object_mut().unwrap().remove("appearance");
        let decoded: AppSettings = serde_json::from_value(value).unwrap();
        assert_eq!(decoded.appearance, super::AppearanceSettings::default());
    }

    #[test]
    fn missing_font_shadow_size_uses_default() {
        let mut value = serde_json::to_value(AppSettings::default()).unwrap();
        value["appearance"]
            .as_object_mut()
            .unwrap()
            .remove("fontShadowSize");
        let decoded: AppSettings = serde_json::from_value(value).unwrap();
        assert_eq!(decoded.appearance.font_shadow_size, 3.0);
    }

    #[test]
    fn settings_accept_long_alert_periods() {
        let mut settings = AppSettings::default();
        settings.alerts.periods = vec![5, 360, 720, 1440, 999];
        assert_eq!(settings.normalize().alerts.periods, vec![5, 360, 720, 1440]);
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Up,
    Down,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AlertRecord {
    pub id: String,
    pub market: Market,
    pub symbol: String,
    pub period: u16,
    pub direction: Direction,
    pub change_percent: f64,
    pub trigger_price: f64,
    pub trigger_time: i64,
    pub expires_at: i64,
    pub dismissed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PriceSample {
    pub timestamp: i64,
    pub price: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TickerSnapshot {
    pub market: Market,
    pub symbol: String,
    pub price: f64,
    pub price_text: String,
    pub change_24h: f64,
    pub event_time: i64,
    pub status: ConnectionStatus,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionStatus {
    Connecting,
    Live,
    Stale,
    Reconnecting,
}

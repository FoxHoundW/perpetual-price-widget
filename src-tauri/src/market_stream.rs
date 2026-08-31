use std::{
    collections::{HashMap, HashSet},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use async_http_proxy::{http_connect_tokio, http_connect_tokio_with_basic_auth};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::net::TcpStream;
use tokio::sync::RwLock;
use tokio_socks::tcp::Socks5Stream;
use tokio_tungstenite::{
    client_async_tls, connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream,
};

use crate::{
    app_logging::LogInput,
    credentials,
    models::{
        ConnectionStatus, Contract, Market, ProxyMode, ProxySettings, TickerSnapshot, WatchItem,
    },
    persistence::Persistence,
};

const RECONNECT_DELAY: Duration = Duration::from_secs(5);
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);
const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(90);
const GENERATION_POLL_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, Default)]
struct LiveTicker {
    price: f64,
    price_text: String,
    change_24h: f64,
    event_time: i64,
}

pub struct MarketStreamManager {
    latest: RwLock<HashMap<(Market, String), LiveTicker>>,
    subscriptions: RwLock<HashMap<Market, HashSet<String>>>,
    market_status: RwLock<HashMap<Market, ConnectionStatus>>,
    proxy: RwLock<ProxySettings>,
    generation: AtomicU64,
    persistence: Arc<Persistence>,
}

impl MarketStreamManager {
    pub fn new(persistence: Arc<Persistence>) -> Arc<Self> {
        Arc::new(Self {
            latest: RwLock::new(HashMap::new()),
            subscriptions: RwLock::new(HashMap::new()),
            market_status: RwLock::new(HashMap::new()),
            proxy: RwLock::new(ProxySettings::default()),
            generation: AtomicU64::new(0),
            persistence,
        })
    }

    pub async fn replace_watchlist(
        self: &Arc<Self>,
        watchlist: &[WatchItem],
        proxy: &ProxySettings,
        catalog: &[Contract],
    ) {
        let mut subscriptions = HashMap::<Market, HashSet<String>>::new();
        for item in watchlist {
            if !catalog.is_empty()
                && !catalog.iter().any(|contract| {
                    contract.market == item.market && contract.symbol == item.symbol
                })
            {
                continue;
            }
            subscriptions
                .entry(item.market)
                .or_default()
                .insert(item.symbol.to_uppercase());
        }
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        *self.subscriptions.write().await = subscriptions.clone();
        *self.proxy.write().await = proxy.clone();
        {
            let mut statuses = self.market_status.write().await;
            for market in [Market::Usdm, Market::Coinm] {
                if subscriptions
                    .get(&market)
                    .is_some_and(|symbols| !symbols.is_empty())
                {
                    statuses.insert(market, ConnectionStatus::Connecting);
                } else {
                    statuses.remove(&market);
                }
            }
        }
        for market in [Market::Usdm, Market::Coinm] {
            let manager = Arc::clone(self);
            tauri::async_runtime::spawn(async move {
                manager.run_market(market, generation).await;
            });
        }
    }

    pub async fn snapshots(&self, _now: i64) -> Vec<TickerSnapshot> {
        let subscriptions = self.subscriptions.read().await;
        let latest = self.latest.read().await;
        let statuses = self.market_status.read().await;
        let mut snapshots = Vec::new();
        for (&market, symbols) in subscriptions.iter() {
            let status = statuses
                .get(&market)
                .copied()
                .unwrap_or(ConnectionStatus::Connecting);
            for symbol in symbols {
                if let Some(ticker) = latest.get(&(market, symbol.clone())) {
                    snapshots.push(TickerSnapshot {
                        market,
                        symbol: symbol.clone(),
                        price: ticker.price,
                        price_text: ticker.price_text.clone(),
                        change_24h: ticker.change_24h,
                        event_time: ticker.event_time,
                        status,
                    });
                } else {
                    snapshots.push(TickerSnapshot {
                        market,
                        symbol: symbol.clone(),
                        price: 0.0,
                        price_text: "—".into(),
                        change_24h: 0.0,
                        event_time: 0,
                        status,
                    });
                }
            }
        }
        snapshots.sort_by(|a, b| a.symbol.cmp(&b.symbol));
        snapshots
    }

    async fn run_market(self: Arc<Self>, market: Market, generation: u64) {
        let mut had_error = false;
        loop {
            if self.generation.load(Ordering::SeqCst) != generation {
                return;
            }
            let symbols = self
                .subscriptions
                .read()
                .await
                .get(&market)
                .cloned()
                .unwrap_or_default();
            if symbols.is_empty() {
                return;
            }
            self.set_market_status(market, generation, ConnectionStatus::Connecting)
                .await;
            let url = combined_stream_url(market, &symbols);
            let proxy = self.proxy.read().await.clone();
            let password = credentials::get_proxy_password(&proxy.username);
            match connect_market_stream(&url, market, &proxy, password.as_deref()).await {
                Ok(mut socket) => {
                    if self.generation.load(Ordering::SeqCst) != generation {
                        let _ = socket.close(None).await;
                        return;
                    }
                    self.set_market_status(market, generation, ConnectionStatus::Live)
                        .await;
                    if had_error {
                        let _ = self.persistence.append_recovery(
                            source_name(market),
                            "行情连接已恢复，正在恢复订阅",
                            chrono::Utc::now().timestamp_millis(),
                        );
                    }
                    let mut heartbeat = tokio::time::interval(HEARTBEAT_INTERVAL);
                    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
                    heartbeat.tick().await;
                    let mut generation_poll = tokio::time::interval(GENERATION_POLL_INTERVAL);
                    generation_poll
                        .set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
                    generation_poll.tick().await;
                    let mut last_frame_at = Instant::now();
                    let disconnect_message = loop {
                        if self.generation.load(Ordering::SeqCst) != generation {
                            let _ = socket.close(None).await;
                            return;
                        }
                        tokio::select! {
                            message = socket.next() => {
                                match message {
                                    Some(Ok(message)) => {
                                        last_frame_at = Instant::now();
                                        match message {
                                            Message::Text(text) => {
                                                if let Some(event) = parse_stream_message(text.as_ref()) {
                                                    self.apply_event(market, event).await;
                                                }
                                            }
                                            Message::Ping(payload) => {
                                                if let Err(error) = socket.send(Message::Pong(payload)).await {
                                                    break format!("行情心跳响应失败：{error}");
                                                }
                                            }
                                            Message::Close(frame) => {
                                                let detail = frame.map_or_else(
                                                    || "未提供关闭原因".to_owned(),
                                                    |frame| format!("code={} reason={}", frame.code, frame.reason),
                                                );
                                                break format!("行情服务端关闭连接：{detail}");
                                            }
                                            _ => {}
                                        }
                                    }
                                    Some(Err(error)) => break format!("行情连接读取失败：{error}"),
                                    None => break "行情连接已由远端结束".to_owned(),
                                }
                            }
                            _ = heartbeat.tick() => {
                                if last_frame_at.elapsed() >= HEARTBEAT_TIMEOUT {
                                    break format!(
                                        "行情心跳超时：{} 秒未收到任何 WebSocket 帧",
                                        HEARTBEAT_TIMEOUT.as_secs(),
                                    );
                                }
                                if let Err(error) = socket.send(Message::Ping(Vec::new().into())).await {
                                    break format!("行情心跳发送失败：{error}");
                                }
                            }
                            _ = generation_poll.tick() => {}
                        }
                    };
                    had_error = true;
                    self.set_market_status(market, generation, ConnectionStatus::Reconnecting)
                        .await;
                    self.log_connection_error(
                        market,
                        &format!("{disconnect_message}，正在每 5 秒重新连接"),
                    );
                }
                Err(error) => {
                    had_error = true;
                    self.set_market_status(market, generation, ConnectionStatus::Reconnecting)
                        .await;
                    self.log_connection_error(market, &format!("行情连接失败：{error:#}"));
                }
            }
            tokio::time::sleep(RECONNECT_DELAY).await;
        }
    }

    async fn set_market_status(&self, market: Market, generation: u64, status: ConnectionStatus) {
        let mut statuses = self.market_status.write().await;
        if self.generation.load(Ordering::SeqCst) == generation {
            statuses.insert(market, status);
        }
    }

    fn log_connection_error(&self, market: Market, message: &str) {
        let _ = self.persistence.append_log(LogInput {
            level: "error",
            source: source_name(market),
            code: "WS_DISCONNECTED",
            message,
            at: chrono::Utc::now().timestamp_millis(),
        });
    }

    async fn apply_event(&self, market: Market, event: StreamEvent) {
        let mut latest = self.latest.write().await;
        let ticker = latest
            .entry((market, event.symbol().to_owned()))
            .or_default();
        apply_stream_event(ticker, event);
    }
}

fn apply_stream_event(ticker: &mut LiveTicker, event: StreamEvent) {
    match event {
        StreamEvent::Trade {
            price,
            price_text,
            event_time,
            ..
        } => {
            ticker.price = price;
            ticker.price_text = price_text;
            ticker.event_time = event_time;
        }
        StreamEvent::Ticker {
            last_price,
            last_price_text,
            change_24h,
            event_time,
            ..
        } => {
            ticker.price = last_price;
            ticker.price_text = last_price_text;
            ticker.change_24h = change_24h;
            ticker.event_time = ticker.event_time.max(event_time);
        }
    }
}

type BinanceSocket = WebSocketStream<MaybeTlsStream<TcpStream>>;

async fn connect_market_stream(
    url: &str,
    market: Market,
    configured: &ProxySettings,
    password: Option<&str>,
) -> Result<BinanceSocket> {
    let effective = resolve_proxy(configured)?;
    let Some(proxy) = effective else {
        return connect_async(url)
            .await
            .map(|(socket, _)| socket)
            .context("WebSocket 直连失败");
    };
    let target = websocket_host(market);
    let proxy_host = proxy
        .host
        .trim()
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .trim_start_matches("socks5://")
        .trim_start_matches("socks5h://")
        .trim_end_matches('/');
    anyhow::ensure!(!proxy_host.is_empty(), "手动代理缺少地址");
    let proxy_address = format!("{proxy_host}:{}", proxy.port.context("代理缺少端口")?);
    let stream = match proxy.mode {
        ProxyMode::Http | ProxyMode::System => {
            let mut stream = TcpStream::connect(&proxy_address)
                .await
                .with_context(|| format!("无法连接 HTTP 代理 {proxy_address}"))?;
            if proxy.username.is_empty() {
                http_connect_tokio(&mut stream, target, 443)
                    .await
                    .context("HTTP 代理 CONNECT 失败")?;
            } else {
                http_connect_tokio_with_basic_auth(
                    &mut stream,
                    target,
                    443,
                    &proxy.username,
                    password.unwrap_or_default(),
                )
                .await
                .context("HTTP 代理认证或 CONNECT 失败")?;
            }
            stream
        }
        ProxyMode::Socks5 => {
            let target_address = (target, 443);
            if proxy.username.is_empty() {
                Socks5Stream::connect(proxy_address.as_str(), target_address)
                    .await
                    .context("SOCKS5 代理连接失败")?
                    .into_inner()
            } else {
                Socks5Stream::connect_with_password(
                    proxy_address.as_str(),
                    target_address,
                    &proxy.username,
                    password.unwrap_or_default(),
                )
                .await
                .context("SOCKS5 代理认证或连接失败")?
                .into_inner()
            }
        }
    };
    client_async_tls(url, stream)
        .await
        .map(|(socket, _)| socket)
        .context("代理隧道 TLS/WebSocket 握手失败")
}

pub(crate) fn resolve_proxy(configured: &ProxySettings) -> Result<Option<ProxySettings>> {
    if configured.mode != ProxyMode::System {
        validate_manual_proxy(configured)?;
        return Ok(Some(configured.clone()));
    }
    let system = sysproxy::Sysproxy::get_system_proxy().context("无法读取 Windows 系统代理")?;
    if !system.enable {
        return Ok(None);
    }
    anyhow::ensure!(
        !system.host.trim().is_empty() && system.port > 0,
        "Windows 系统代理已启用，但无法解析静态 HTTPS 代理地址"
    );
    Ok(Some(ProxySettings {
        mode: ProxyMode::Http,
        host: system.host,
        port: Some(system.port),
        username: String::new(),
    }))
}

fn validate_manual_proxy(proxy: &ProxySettings) -> Result<()> {
    anyhow::ensure!(!proxy.host.trim().is_empty(), "手动代理缺少地址");
    anyhow::ensure!(proxy.port.is_some_and(|port| port > 0), "手动代理缺少端口");
    Ok(())
}

fn websocket_host(market: Market) -> &'static str {
    match market {
        Market::Usdm => "fstream.binance.com",
        Market::Coinm => "dstream.binance.com",
    }
}

fn source_name(market: Market) -> &'static str {
    match market {
        Market::Usdm => "USDⓈ-M WebSocket",
        Market::Coinm => "COIN-M WebSocket",
    }
}

fn combined_stream_url(market: Market, symbols: &HashSet<String>) -> String {
    let mut symbols: Vec<_> = symbols.iter().map(|symbol| symbol.to_lowercase()).collect();
    symbols.sort();
    let streams = symbols
        .iter()
        .flat_map(|symbol| [format!("{symbol}@aggTrade"), format!("{symbol}@ticker")])
        .collect::<Vec<_>>()
        .join("/");
    format!("{}?streams={streams}", market.websocket_base())
}

#[derive(Debug, Deserialize)]
struct CombinedMessage {
    data: RawEvent,
}

#[derive(Debug, Deserialize)]
struct RawEvent {
    e: String,
    #[serde(rename = "E")]
    event_time: i64,
    s: String,
    p: Option<String>,
    c: Option<String>,
    #[serde(rename = "P")]
    change_percent: Option<String>,
}

#[derive(Debug, PartialEq)]
enum StreamEvent {
    Trade {
        symbol: String,
        price: f64,
        price_text: String,
        event_time: i64,
    },
    Ticker {
        symbol: String,
        last_price: f64,
        last_price_text: String,
        change_24h: f64,
        event_time: i64,
    },
}

impl StreamEvent {
    fn symbol(&self) -> &str {
        match self {
            Self::Trade { symbol, .. } | Self::Ticker { symbol, .. } => symbol,
        }
    }
}

fn parse_stream_message(input: &str) -> Option<StreamEvent> {
    let event = serde_json::from_str::<CombinedMessage>(input).ok()?.data;
    match event.e.as_str() {
        "aggTrade" => {
            let price_text = event.p?;
            Some(StreamEvent::Trade {
                symbol: event.s,
                price: price_text.parse().ok()?,
                price_text,
                event_time: event.event_time,
            })
        }
        "24hrTicker" => {
            let last_price_text = event.c?;
            Some(StreamEvent::Ticker {
                symbol: event.s,
                last_price: last_price_text.parse().ok()?,
                last_price_text,
                change_24h: event.change_percent?.parse().ok()?,
                event_time: event.event_time,
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_sorted_combined_stream_url() {
        let symbols = HashSet::from(["ETHUSDT".into(), "BTCUSDT".into()]);
        let url = combined_stream_url(Market::Usdm, &symbols);
        assert_eq!(url, "wss://fstream.binance.com/market/stream?streams=btcusdt@aggTrade/btcusdt@ticker/ethusdt@aggTrade/ethusdt@ticker");
    }

    #[test]
    fn parses_aggregate_trade() {
        let event = parse_stream_message(r#"{"stream":"btcusdt@aggTrade","data":{"e":"aggTrade","E":1499405254326,"s":"BTCUSDT","p":"112842.60"}}"#).unwrap();
        assert_eq!(
            event,
            StreamEvent::Trade {
                symbol: "BTCUSDT".into(),
                price: 112842.6,
                price_text: "112842.60".into(),
                event_time: 1499405254326
            }
        );
    }

    #[test]
    fn parses_24h_ticker() {
        let event = parse_stream_message(r#"{"stream":"btcusdt@ticker","data":{"e":"24hrTicker","E":1499405254326,"s":"BTCUSDT","c":"112842.60","P":"2.84"}}"#).unwrap();
        assert_eq!(
            event,
            StreamEvent::Ticker {
                symbol: "BTCUSDT".into(),
                last_price: 112842.6,
                last_price_text: "112842.60".into(),
                change_24h: 2.84,
                event_time: 1499405254326
            }
        );
    }

    #[test]
    fn ticker_stream_refreshes_last_trade_price() {
        let mut ticker = LiveTicker {
            price: 100.0,
            price_text: "100.0".into(),
            ..LiveTicker::default()
        };
        apply_stream_event(
            &mut ticker,
            StreamEvent::Ticker {
                symbol: "BNBUSDT".into(),
                last_price: 101.25,
                last_price_text: "101.25".into(),
                change_24h: 1.5,
                event_time: 2000,
            },
        );
        assert_eq!(ticker.price, 101.25);
        assert_eq!(ticker.price_text, "101.25");
    }

    #[tokio::test]
    async fn quiet_symbol_stays_live_while_market_connection_is_live() {
        let directory = tempfile::tempdir().unwrap();
        let persistence = Arc::new(Persistence::open(directory.path().to_path_buf()).unwrap());
        let manager = MarketStreamManager::new(persistence);
        manager
            .subscriptions
            .write()
            .await
            .insert(Market::Usdm, HashSet::from(["QUIETUSDT".into()]));
        manager
            .market_status
            .write()
            .await
            .insert(Market::Usdm, ConnectionStatus::Live);
        manager.latest.write().await.insert(
            (Market::Usdm, "QUIETUSDT".into()),
            LiveTicker {
                price: 12.5,
                price_text: "12.5".into(),
                change_24h: 1.0,
                event_time: 1,
            },
        );

        let snapshots = manager.snapshots(i64::MAX).await;

        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].status, ConnectionStatus::Live);
        assert_eq!(snapshots[0].price_text, "12.5");
    }

    #[tokio::test]
    async fn subscribed_symbol_without_cache_inherits_market_status() {
        let directory = tempfile::tempdir().unwrap();
        let persistence = Arc::new(Persistence::open(directory.path().to_path_buf()).unwrap());
        let manager = MarketStreamManager::new(persistence);
        manager
            .subscriptions
            .write()
            .await
            .insert(Market::Usdm, HashSet::from(["NEWUSDT".into()]));
        manager
            .market_status
            .write()
            .await
            .insert(Market::Usdm, ConnectionStatus::Reconnecting);

        let snapshots = manager.snapshots(0).await;

        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].status, ConnectionStatus::Reconnecting);
        assert_eq!(snapshots[0].price_text, "—");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore = "requires live Binance network and runs for roughly two minutes"]
    async fn live_market_stream_stays_live_across_heartbeat_window() {
        let directory = tempfile::tempdir().unwrap();
        let persistence = Arc::new(Persistence::open(directory.path().to_path_buf()).unwrap());
        let manager = MarketStreamManager::new(persistence);
        let watchlist = [
            "BTCUSDT", "ETHUSDT", "SOLUSDT", "BNBUSDT", "SNDKUSDT", "MUUSDT",
        ]
        .into_iter()
        .map(|symbol| WatchItem {
            market: Market::Usdm,
            symbol: symbol.into(),
            alert_enabled: false,
            rise_threshold: 5.0,
            fall_threshold: 5.0,
        })
        .collect::<Vec<_>>();
        manager
            .replace_watchlist(&watchlist, &ProxySettings::default(), &[])
            .await;

        let mut initial_live = false;
        for _ in 0..15 {
            let snapshots = manager.snapshots(0).await;
            initial_live = snapshots.len() == watchlist.len()
                && snapshots.iter().all(|ticker| {
                    ticker.status == ConnectionStatus::Live && ticker.price_text != "—"
                });
            if initial_live {
                break;
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        assert!(
            initial_live,
            "all live symbols should receive an initial price"
        );

        for _ in 0..105 {
            tokio::time::sleep(Duration::from_secs(1)).await;
            let snapshots = manager.snapshots(0).await;
            assert!(
                snapshots
                    .iter()
                    .all(|ticker| ticker.status == ConnectionStatus::Live),
                "market connection entered a non-live state during the heartbeat window",
            );
        }
    }
}

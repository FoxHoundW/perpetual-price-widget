use anyhow::{Context, Result};
use reqwest::{Client, Proxy};
use serde::Deserialize;

use crate::models::{Contract, ContractStatus, Market, PriceSample, ProxyMode, ProxySettings};

#[derive(Debug, Deserialize)]
struct ExchangeInfo {
    symbols: Vec<RawSymbol>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawSymbol {
    symbol: String,
    contract_type: String,
    status: Option<String>,
    contract_status: Option<String>,
    base_asset: String,
    quote_asset: String,
    margin_asset: String,
    price_precision: Option<u8>,
    filters: Vec<RawFilter>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawFilter {
    filter_type: String,
    tick_size: Option<String>,
}

pub fn build_http_client(proxy: &ProxySettings, password: Option<&str>) -> Result<Client> {
    let mut builder = Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("perpetual-price-widget/1.0.0");

    if let Some(proxy) = crate::market_stream::resolve_proxy(proxy)? {
        let port = proxy.port.context("代理缺少端口")?;
        let scheme = match proxy.mode {
            ProxyMode::Http => "http",
            ProxyMode::Socks5 => "socks5h",
            ProxyMode::System => "http",
        };
        let host = proxy
            .host
            .trim()
            .trim_start_matches("http://")
            .trim_start_matches("https://")
            .trim_start_matches("socks5://")
            .trim_start_matches("socks5h://")
            .trim_end_matches('/');
        anyhow::ensure!(!host.is_empty(), "手动代理缺少地址");
        let url = format!("{scheme}://{host}:{port}");
        let mut configured = Proxy::all(&url).context("代理地址无效")?;
        if !proxy.username.is_empty() {
            configured = configured.basic_auth(&proxy.username, password.unwrap_or_default());
        }
        builder = builder.proxy(configured);
    }

    builder.build().context("无法创建网络客户端")
}

pub async fn fetch_contracts(client: &Client, market: Market) -> Result<Vec<Contract>> {
    let url = format!("{}{}", market.rest_base(), market.exchange_info_path());
    let response = client
        .get(url)
        .send()
        .await
        .context("交易对列表请求失败")?
        .error_for_status()
        .context("交易对列表返回错误状态")?;
    let exchange = response
        .json::<ExchangeInfo>()
        .await
        .context("交易对列表格式无效")?;
    Ok(parse_contracts(market, exchange.symbols))
}

pub async fn fetch_minute_klines(
    client: &Client,
    market: Market,
    symbol: &str,
    limit: u16,
) -> Result<Vec<PriceSample>> {
    let path = match market {
        Market::Usdm => "/fapi/v1/klines",
        Market::Coinm => "/dapi/v1/klines",
    };
    let limit_text = limit.min(1500).to_string();
    let response = client
        .get(format!("{}{path}", market.rest_base()))
        .query(&[
            ("symbol", symbol),
            ("interval", "1m"),
            ("limit", limit_text.as_str()),
        ])
        .send()
        .await
        .context("分钟行情请求失败")?;
    let status = response.status();
    if !status.is_success() {
        let detail = response.text().await.unwrap_or_default();
        anyhow::bail!(
            "分钟行情返回错误状态 {status}: {}",
            detail.chars().take(180).collect::<String>()
        );
    }
    let rows = response
        .json::<Vec<Vec<serde_json::Value>>>()
        .await
        .context("分钟行情格式无效")?;
    Ok(rows
        .into_iter()
        .filter_map(|row| {
            let timestamp = row.first()?.as_i64()?;
            let price = row.get(4)?.as_str()?.parse::<f64>().ok()?;
            Some(PriceSample { timestamp, price })
        })
        .collect())
}

fn parse_contracts(market: Market, symbols: Vec<RawSymbol>) -> Vec<Contract> {
    symbols
        .into_iter()
        .filter(|symbol| {
            matches!(
                symbol.contract_type.as_str(),
                "PERPETUAL" | "TRADIFI_PERPETUAL"
            )
        })
        .filter(|symbol| {
            let status = match market {
                Market::Usdm => symbol.status.as_deref(),
                Market::Coinm => symbol.contract_status.as_deref(),
            };
            status == Some("TRADING")
        })
        .map(|symbol| {
            let tick_size = symbol
                .filters
                .iter()
                .find(|filter| filter.filter_type == "PRICE_FILTER")
                .and_then(|filter| filter.tick_size.clone())
                .unwrap_or_else(|| "1".into());
            Contract {
                market,
                symbol: symbol.symbol,
                base_asset: symbol.base_asset,
                quote_asset: symbol.quote_asset,
                margin_asset: symbol.margin_asset,
                contract_type: "PERPETUAL".into(),
                price_decimals: decimals_from_tick(&tick_size)
                    .unwrap_or(symbol.price_precision.unwrap_or(0)),
                tick_size,
                status: ContractStatus::Trading,
            }
        })
        .collect()
}

fn decimals_from_tick(tick_size: &str) -> Option<u8> {
    let fractional = tick_size.split_once('.')?.1.trim_end_matches('0');
    Some(fractional.len().min(u8::MAX as usize) as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn symbol(market: Market, contract_type: &str, status: &str) -> RawSymbol {
        RawSymbol {
            symbol: "BTCUSDT".into(),
            contract_type: contract_type.into(),
            status: (market == Market::Usdm).then(|| status.into()),
            contract_status: (market == Market::Coinm).then(|| status.into()),
            base_asset: "BTC".into(),
            quote_asset: "USDT".into(),
            margin_asset: "USDT".into(),
            price_precision: Some(2),
            filters: vec![RawFilter {
                filter_type: "PRICE_FILTER".into(),
                tick_size: Some("0.10".into()),
            }],
        }
    }

    #[test]
    fn keeps_only_trading_perpetuals() {
        let parsed = parse_contracts(
            Market::Usdm,
            vec![
                symbol(Market::Usdm, "PERPETUAL", "TRADING"),
                symbol(Market::Usdm, "CURRENT_QUARTER", "TRADING"),
                symbol(Market::Usdm, "PERPETUAL", "SETTLING"),
            ],
        );
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].price_decimals, 1);
    }

    #[test]
    fn reads_coin_m_contract_status() {
        let parsed = parse_contracts(
            Market::Coinm,
            vec![symbol(Market::Coinm, "PERPETUAL", "TRADING")],
        );
        assert_eq!(parsed.len(), 1);
    }

    #[test]
    fn keeps_tradfi_perpetuals_in_usdm_catalog() {
        let mut tradfi = symbol(Market::Usdm, "TRADIFI_PERPETUAL", "TRADING");
        tradfi.symbol = "SNDKUSDT".into();
        tradfi.base_asset = "SNDK".into();
        let parsed = parse_contracts(Market::Usdm, vec![tradfi]);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].symbol, "SNDKUSDT");
        assert_eq!(parsed[0].contract_type, "PERPETUAL");
    }
}

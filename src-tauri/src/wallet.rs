use crate::{
    app_logging::LogInput, commands::AppState, contract_catalog::build_http_client, credentials,
};
use hmac::{Hmac, Mac};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::{
    collections::HashSet,
    str::FromStr,
    time::{Duration, Instant},
};
use tauri::{Emitter, Manager, State};

const SERVICE: &str = "io.github.foxhoundw.perpetual-price-widget.wallet";

#[derive(Default, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletSnapshot {
    pub balance: Option<String>,
    pub wallets: Vec<String>,
    pub details: Vec<WalletDetail>,
    pub error: Option<String>,
    pub updated_at: Option<i64>,
}

#[derive(Clone, Serialize)]
pub struct WalletDetail {
    pub name: String,
    pub balance: String,
}

#[derive(Default)]
pub struct WalletState {
    snapshot: WalletSnapshot,
    pub(crate) next_request: Option<Instant>,
    client: Option<(crate::models::ProxySettings, reqwest::Client)>,
    clock_offset: i64,
    clock_synced: Option<Instant>,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct Keys {
    api_key: String,
    secret: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Wallet {
    wallet_name: String,
    balance: String,
}

fn summarize(body: &str) -> Result<(String, Vec<WalletDetail>), String> {
    let wallets: Vec<Wallet> =
        serde_json::from_str(body).map_err(|_| "余额数据格式无效".to_string())?;
    if wallets.is_empty() {
        return Err("接口未返回任何钱包，无法确认总余额".into());
    }
    let mut names = HashSet::new();
    let mut total = Decimal::ZERO;
    let mut ordered = Vec::new();
    for wallet in wallets {
        if wallet.wallet_name.trim().is_empty() || !names.insert(wallet.wallet_name.clone()) {
            return Err("钱包名称缺失或重复，已停止汇总".into());
        }
        let value = Decimal::from_str(&wallet.balance).map_err(|_| "钱包余额无效".to_string())?;
        total = total.checked_add(value).ok_or("余额超出可计算范围")?;
        ordered.push(WalletDetail {
            name: wallet.wallet_name,
            balance: format!("{:.2}", value.round_dp(2)),
        });
    }
    Ok((format!("{:.2}", total.round_dp(2)), ordered))
}

fn sign(query: &str, secret: &str) -> String {
    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key length");
    mac.update(query.as_bytes());
    mac.finalize()
        .into_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[tauri::command]
pub async fn save_wallet_credentials(
    state: State<'_, AppState>,
    api_key: String,
    secret: String,
) -> Result<(), String> {
    if api_key.trim().is_empty() || secret.trim().is_empty() {
        return Err("请同时填写 API Key 和 Secret Key".into());
    }
    if !api_key.trim().bytes().all(|c| c.is_ascii_alphanumeric())
        || !secret.trim().bytes().all(|c| c.is_ascii_alphanumeric())
    {
        return Err("请使用币安系统生成的 HMAC API Key 和 Secret Key".into());
    }
    let mut wallet = state.wallet.lock().await;
    let keys = Keys {
        api_key: api_key.trim().into(),
        secret: secret.trim().into(),
    };
    let entry =
        keyring::Entry::new(SERVICE, "default").map_err(|_| "无法打开 Windows 凭据管理器")?;
    entry
        .set_password(&serde_json::to_string(&keys).map_err(|_| "无法编码凭据")?)
        .map_err(|_| "无法保存账户凭据")?;
    *wallet = WalletState::default();
    Ok(())
}

#[tauri::command]
pub async fn get_wallet_status(state: State<'_, AppState>) -> Result<WalletSnapshot, String> {
    Ok(state.wallet_snapshot.read().await.clone())
}

#[tauri::command]
pub async fn get_wallet_balance(state: State<'_, AppState>) -> Result<WalletSnapshot, String> {
    get_wallet_status(state).await
}

// Only the native service performs requests; webview focus and timer throttling cannot
// stop polling, and opening another view never doubles the request rate.
pub async fn run(app: tauri::AppHandle) {
    let mut interval = tokio::time::interval(Duration::from_secs(1));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        interval.tick().await;
        let state = app.state::<AppState>();
        let snapshot = refresh_wallet(&state).await;
        *state.wallet_snapshot.write().await = snapshot.clone();
        let _ = app.emit("wallet-updated", snapshot);
    }
}

async fn refresh_wallet(state: &AppState) -> WalletSnapshot {
    let mut wallet = state.wallet.lock().await;
    let settings = state.settings.read().await.clone();
    if !settings.show_balance {
        wallet.snapshot = WalletSnapshot::default();
        return wallet.snapshot.clone();
    }
    if wallet
        .next_request
        .is_some_and(|next| Instant::now() < next)
    {
        return wallet.snapshot.clone();
    }
    // The service serializes requests. Only server backoff is cached.
    wallet.next_request = None;
    let result = fetch(&mut wallet, &settings.proxy).await;
    let current_settings = state.settings.read().await;
    if !current_settings.show_balance || current_settings.proxy != settings.proxy {
        wallet.snapshot = WalletSnapshot::default();
        return wallet.snapshot.clone();
    }
    drop(current_settings);
    match result {
        Ok((balance, details)) => {
            wallet.snapshot = WalletSnapshot {
                balance: Some(balance),
                wallets: details.iter().map(|item| item.name.clone()).collect(),
                details,
                error: None,
                updated_at: Some(chrono::Utc::now().timestamp_millis()),
            };
        }
        Err(message) => {
            // Never log signed URLs, raw responses, keys or balances.
            let _ = state.persistence.append_log(LogInput {
                level: "error",
                source: "账号余额",
                code: "WALLET_READ_FAILED",
                message: &message,
                at: chrono::Utc::now().timestamp_millis(),
            });
            wallet.snapshot.balance = None;
            wallet.snapshot.details.clear();
            wallet.snapshot.wallets.clear();
            wallet.snapshot.error = Some(message);
        }
    }
    wallet.snapshot.clone()
}

async fn fetch(
    wallet: &mut WalletState,
    proxy: &crate::models::ProxySettings,
) -> Result<(String, Vec<WalletDetail>), String> {
    let (client, keys) = prepare(wallet, proxy).await?;
    let body = signed_get(
        wallet,
        &client,
        &keys,
        "https://api.binance.com/sapi/v1/asset/wallet/balance",
        "quoteAsset=USDT&",
    )
    .await?;
    summarize(&body)
}

pub(crate) async fn prepare(
    wallet: &mut WalletState,
    proxy: &crate::models::ProxySettings,
) -> Result<(reqwest::Client, Keys), String> {
    let entry =
        keyring::Entry::new(SERVICE, "default").map_err(|_| "无法打开 Windows 凭据管理器")?;
    let stored = entry
        .get_password()
        .map_err(|_| "尚未配置账户凭据，请在基础设置中保存 API Key 与 Secret Key")?;
    let keys: Keys = serde_json::from_str(&stored).map_err(|_| "账户凭据无法读取，请重新保存")?;
    if wallet
        .client
        .as_ref()
        .map_or(true, |(previous, _)| previous != proxy)
    {
        let password = credentials::get_proxy_password(&proxy.username);
        let client = build_http_client(proxy, password.as_deref())
            .map_err(|_| "无法建立余额连接，请检查网络代理")?;
        wallet.client = Some((proxy.clone(), client));
        wallet.clock_synced = None;
    }
    let client = wallet.client.as_ref().unwrap().1.clone();
    if wallet
        .clock_synced
        .map_or(true, |at| at.elapsed() >= Duration::from_secs(600))
    {
        let before = chrono::Utc::now().timestamp_millis();
        let response = client
            .get("https://api.binance.com/api/v3/time")
            .send()
            .await
            .map_err(|_| "币安时间同步失败，请检查网络")?;
        check_rate_limit(wallet, &response)?;
        let time: serde_json::Value = response
            .error_for_status()
            .map_err(|_| "币安时间接口暂不可用")?
            .json()
            .await
            .map_err(|_| "币安时间数据无效")?;
        let server = time["serverTime"].as_i64().ok_or("币安时间数据无效")?;
        wallet.clock_offset = server - (before + chrono::Utc::now().timestamp_millis()) / 2;
        wallet.clock_synced = Some(Instant::now());
    }
    Ok((client, keys))
}

pub(crate) async fn signed_get(
    wallet: &mut WalletState,
    client: &reqwest::Client,
    keys: &Keys,
    endpoint: &str,
    params: &str,
) -> Result<String, String> {
    let query = format!(
        "{params}recvWindow=5000&timestamp={}",
        chrono::Utc::now().timestamp_millis() + wallet.clock_offset
    );
    let response = client
        .get(format!(
            "{endpoint}?{query}&signature={}",
            sign(&query, &keys.secret)
        ))
        .header("X-MBX-APIKEY", &keys.api_key)
        .send()
        .await
        .map_err(|_| "余额请求失败，请检查网络和代理")?;
    let status = response.status();
    check_rate_limit(wallet, &response)?;
    let body = response.text().await.map_err(|_| "余额响应读取失败")?;
    if !status.is_success() {
        let code = serde_json::from_str::<serde_json::Value>(&body)
            .ok()
            .and_then(|value| value["code"].as_i64());
        return Err(match code {
            Some(-2008) => "API Key 无效（-2008），请检查是否为币安正式账户的系统生成密钥".into(),
            Some(-1021) => {
                wallet.clock_synced = None;
                "账户请求时间偏差，下一次重新同步时间".into()
            }
            Some(-2014 | -2015 | -1022) => {
                format!(
                    "账户验证失败（{}），请检查 API 密钥、接口权限和 IP 白名单",
                    code.unwrap()
                )
            }
            _ => format!(
                "余额接口请求失败（HTTP {}，错误代码 {}）",
                status.as_u16(),
                code.map_or("未知".into(), |c| c.to_string())
            ),
        });
    }
    Ok(body)
}

pub(crate) fn check_rate_limit(
    wallet: &mut WalletState,
    response: &reqwest::Response,
) -> Result<(), String> {
    let status = response.status().as_u16();
    if status != 429 && status != 418 {
        return Ok(());
    }
    let seconds = response
        .headers()
        .get("retry-after")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(if status == 418 { 120 } else { 60 })
        .clamp(1, 604800);
    wallet.next_request = Some(Instant::now() + Duration::from_secs(seconds));
    Err(format!("币安限制请求频率，等待 {seconds} 秒后重试"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sums_all_wallets_with_decimal_precision() {
        let (total, names) = summarize(r#"[{"walletName":"Spot","balance":"0.1"},{"walletName":"USD-M Futures","balance":"0.2"},{"walletName":"Copy Trading","balance":"123456.78"}]"#).unwrap();
        assert_eq!(total, "123457.08");
        assert_eq!(names.len(), 3);
    }
    #[test]
    fn refuses_partial_or_duplicate_results() {
        for body in [
            "[]",
            r#"[{"walletName":"Spot","balance":"NaN"}]"#,
            r#"[{"walletName":"Spot","balance":"1"},{"walletName":"Spot","balance":"2"}]"#,
            r#"[{"walletName":"Spot"}]"#,
        ] {
            assert!(summarize(body).is_err());
        }
        assert_eq!(
            summarize(r#"[{"walletName":"Spot","balance":"0"}]"#)
                .unwrap()
                .0,
            "0.00"
        );
    }
    #[test]
    fn hmac_matches_known_vector() {
        assert_eq!(
            sign("The quick brown fox jumps over the lazy dog", "key"),
            "f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8"
        );
    }
}

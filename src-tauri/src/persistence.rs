use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

use anyhow::{Context, Result};
use rusqlite::{params, Connection};

use crate::{
    alert_engine::TriggerState,
    models::{
        AlertRecord, AppSettings, Contract, ContractStatus, Direction, Market, PriceSample,
        ALERT_SAMPLE_RETENTION_MINUTES,
    },
};

pub struct Persistence {
    root: PathBuf,
    state: Mutex<Connection>,
    logs: Mutex<Connection>,
}

impl Persistence {
    pub fn open(root: PathBuf) -> Result<Self> {
        fs::create_dir_all(&root).context("无法创建应用数据目录")?;
        let state = Connection::open(root.join("state.db")).context("无法打开状态数据库")?;
        let logs = Connection::open(root.join("logs.db")).context("无法打开日志数据库")?;
        let persistence = Self {
            root,
            state: Mutex::new(state),
            logs: Mutex::new(logs),
        };
        persistence.migrate()?;
        Ok(persistence)
    }

    fn migrate(&self) -> Result<()> {
        let state = self.state.lock().expect("state database mutex poisoned");
        state.execute_batch(
            r#"
            PRAGMA journal_mode = WAL;
            PRAGMA foreign_keys = ON;
            CREATE TABLE IF NOT EXISTS metadata (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS contracts (
                market TEXT NOT NULL,
                symbol TEXT NOT NULL,
                base_asset TEXT NOT NULL,
                quote_asset TEXT NOT NULL,
                margin_asset TEXT NOT NULL,
                tick_size TEXT NOT NULL,
                price_decimals INTEGER NOT NULL,
                status TEXT NOT NULL,
                synced_at INTEGER NOT NULL,
                PRIMARY KEY (market, symbol)
            );
            CREATE TABLE IF NOT EXISTS minute_samples (
                market TEXT NOT NULL,
                symbol TEXT NOT NULL,
                minute_at INTEGER NOT NULL,
                price REAL NOT NULL,
                PRIMARY KEY (market, symbol, minute_at)
            );
            CREATE INDEX IF NOT EXISTS idx_samples_time ON minute_samples(minute_at);
            CREATE TABLE IF NOT EXISTS alerts (
                id TEXT PRIMARY KEY,
                market TEXT NOT NULL,
                symbol TEXT NOT NULL,
                period INTEGER NOT NULL,
                direction TEXT NOT NULL,
                change_percent REAL NOT NULL,
                trigger_price REAL NOT NULL,
                trigger_time INTEGER NOT NULL,
                expires_at INTEGER NOT NULL,
                dismissed INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_alerts_expiry ON alerts(expires_at);
            CREATE TABLE IF NOT EXISTS trigger_states (
                state_key TEXT PRIMARY KEY,
                trigger_price REAL NOT NULL,
                cooldown_until INTEGER NOT NULL
            );
            "#,
        )?;
        drop(state);

        let logs = self.logs.lock().expect("logs database mutex poisoned");
        logs.execute_batch(
            r#"
            PRAGMA journal_mode = WAL;
            CREATE TABLE IF NOT EXISTS log_groups (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                fingerprint TEXT NOT NULL,
                level TEXT NOT NULL,
                source TEXT NOT NULL,
                code TEXT NOT NULL,
                message TEXT NOT NULL,
                first_at INTEGER NOT NULL,
                last_at INTEGER NOT NULL,
                count INTEGER NOT NULL,
                recovered INTEGER NOT NULL DEFAULT 0,
                active INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_logs_last_at ON log_groups(last_at DESC);
            "#,
        )?;
        Ok(())
    }

    pub fn load_settings(&self) -> Result<AppSettings> {
        let path = self.root.join("settings.json");
        if !path.exists() {
            let settings = AppSettings::default();
            self.save_settings(&settings)?;
            return Ok(settings);
        }
        let bytes = fs::read(&path).context("无法读取设置")?;
        serde_json::from_slice::<AppSettings>(&bytes)
            .map(AppSettings::normalize)
            .context("设置文件格式无效")
    }

    pub fn save_settings(&self, settings: &AppSettings) -> Result<()> {
        let path = self.root.join("settings.json");
        let temporary = self.root.join("settings.json.tmp");
        let bytes = serde_json::to_vec_pretty(&settings.clone().normalize())?;
        fs::write(&temporary, bytes).context("无法写入临时设置")?;
        replace_file(&temporary, &path).context("无法保存设置")
    }

    pub fn replace_contracts(
        &self,
        market: Market,
        contracts: &[Contract],
        synced_at: i64,
    ) -> Result<()> {
        let mut connection = self.state.lock().expect("state database mutex poisoned");
        let transaction = connection.transaction()?;
        transaction.execute(
            "DELETE FROM contracts WHERE market = ?1",
            [market_name(market)],
        )?;
        {
            let mut statement = transaction.prepare(
                "INSERT INTO contracts (market,symbol,base_asset,quote_asset,margin_asset,tick_size,price_decimals,status,synced_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            )?;
            for contract in contracts {
                statement.execute(params![
                    market_name(contract.market),
                    contract.symbol,
                    contract.base_asset,
                    contract.quote_asset,
                    contract.margin_asset,
                    contract.tick_size,
                    contract.price_decimals,
                    status_name(contract.status),
                    synced_at
                ])?;
            }
        }
        transaction.execute(
            "INSERT INTO metadata(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![format!("last_sync_{}", market_name(market)), synced_at.to_string()],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn load_contracts(&self) -> Result<Vec<Contract>> {
        let connection = self.state.lock().expect("state database mutex poisoned");
        let mut statement = connection.prepare(
            "SELECT market,symbol,base_asset,quote_asset,margin_asset,tick_size,price_decimals,status FROM contracts ORDER BY market,symbol",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(Contract {
                market: parse_market(row.get::<_, String>(0)?.as_str()),
                symbol: row.get(1)?,
                base_asset: row.get(2)?,
                quote_asset: row.get(3)?,
                margin_asset: row.get(4)?,
                contract_type: "PERPETUAL".into(),
                tick_size: row.get(5)?,
                price_decimals: row.get(6)?,
                status: if row.get::<_, String>(7)? == "TRADING" {
                    ContractStatus::Trading
                } else {
                    ContractStatus::Delisted
                },
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    pub fn last_contract_sync(&self, market: Market) -> Result<Option<i64>> {
        let connection = self.state.lock().expect("state database mutex poisoned");
        let key = format!("last_sync_{}", market_name(market));
        let mut statement = connection.prepare("SELECT value FROM metadata WHERE key=?1")?;
        let mut rows = statement.query([key])?;
        let Some(row) = rows.next()? else {
            return Ok(None);
        };
        Ok(row.get::<_, String>(0)?.parse::<i64>().ok())
    }

    pub fn upsert_sample(&self, market: Market, symbol: &str, sample: &PriceSample) -> Result<()> {
        let connection = self.state.lock().expect("state database mutex poisoned");
        let minute_at = sample.timestamp / 60_000 * 60_000;
        connection.execute(
            "INSERT INTO minute_samples(market,symbol,minute_at,price) VALUES(?1,?2,?3,?4) ON CONFLICT(market,symbol,minute_at) DO UPDATE SET price=excluded.price",
            params![market_name(market), symbol, minute_at, sample.price],
        )?;
        Ok(())
    }

    pub fn save_alert(&self, alert: &AlertRecord) -> Result<()> {
        let connection = self.state.lock().expect("state database mutex poisoned");
        connection.execute(
            "INSERT OR REPLACE INTO alerts(id,market,symbol,period,direction,change_percent,trigger_price,trigger_time,expires_at,dismissed) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            params![alert.id, market_name(alert.market), alert.symbol, alert.period, direction_name(alert.direction), alert.change_percent, alert.trigger_price, alert.trigger_time, alert.expires_at, alert.dismissed],
        )?;
        Ok(())
    }

    pub fn load_active_alerts(&self, now: i64) -> Result<Vec<AlertRecord>> {
        let connection = self.state.lock().expect("state database mutex poisoned");
        let mut statement = connection.prepare(
            "SELECT id,market,symbol,period,direction,change_percent,trigger_price,trigger_time,expires_at,dismissed FROM alerts WHERE expires_at>?1 ORDER BY trigger_time DESC",
        )?;
        let rows = statement.query_map([now], |row| {
            Ok(AlertRecord {
                id: row.get(0)?,
                market: parse_market(row.get::<_, String>(1)?.as_str()),
                symbol: row.get(2)?,
                period: row.get(3)?,
                direction: if row.get::<_, String>(4)? == "down" {
                    Direction::Down
                } else {
                    Direction::Up
                },
                change_percent: row.get(5)?,
                trigger_price: row.get(6)?,
                trigger_time: row.get(7)?,
                expires_at: row.get(8)?,
                dismissed: row.get::<_, i64>(9)? != 0,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    pub fn dismiss_alert(&self, id: &str) -> Result<()> {
        let connection = self.state.lock().expect("state database mutex poisoned");
        connection.execute("UPDATE alerts SET dismissed=1 WHERE id=?1", [id])?;
        Ok(())
    }

    pub fn load_trigger_states(&self) -> Result<HashMap<String, TriggerState>> {
        let connection = self.state.lock().expect("state database mutex poisoned");
        let mut statement = connection
            .prepare("SELECT state_key,trigger_price,cooldown_until FROM trigger_states")?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                TriggerState {
                    trigger_price: row.get(1)?,
                    cooldown_until: row.get(2)?,
                },
            ))
        })?;
        rows.collect::<rusqlite::Result<HashMap<_, _>>>()
            .map_err(Into::into)
    }

    pub fn replace_trigger_states(&self, states: &HashMap<String, TriggerState>) -> Result<()> {
        let mut connection = self.state.lock().expect("state database mutex poisoned");
        let transaction = connection.transaction()?;
        transaction.execute("DELETE FROM trigger_states", [])?;
        {
            let mut statement = transaction.prepare(
                "INSERT INTO trigger_states(state_key,trigger_price,cooldown_until) VALUES(?1,?2,?3)",
            )?;
            for (key, state) in states {
                statement.execute(params![key, state.trigger_price, state.cooldown_until])?;
            }
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn delete_trigger_states_for_symbol(&self, market: Market, symbol: &str) -> Result<()> {
        let prefix = format!("{}:{symbol}:%", market_name(market));
        let connection = self.state.lock().expect("state database mutex poisoned");
        connection.execute(
            "DELETE FROM trigger_states WHERE state_key LIKE ?1",
            [prefix],
        )?;
        Ok(())
    }

    pub fn load_samples(
        &self,
        market: Market,
        symbol: &str,
        since: i64,
    ) -> Result<Vec<PriceSample>> {
        let connection = self.state.lock().expect("state database mutex poisoned");
        let mut statement = connection.prepare(
            "SELECT minute_at,price FROM minute_samples WHERE market=?1 AND symbol=?2 AND minute_at>=?3 ORDER BY minute_at",
        )?;
        let rows = statement.query_map(params![market_name(market), symbol, since], |row| {
            Ok(PriceSample {
                timestamp: row.get(0)?,
                price: row.get(1)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    pub fn cleanup(&self, now: i64) -> Result<()> {
        let connection = self.state.lock().expect("state database mutex poisoned");
        connection.execute(
            "DELETE FROM minute_samples WHERE minute_at < ?1",
            [now - ALERT_SAMPLE_RETENTION_MINUTES * 60_000],
        )?;
        connection.execute("DELETE FROM alerts WHERE expires_at <= ?1", [now])?;
        drop(connection);
        self.cleanup_logs(now)
    }

    pub fn logs_connection(&self) -> &Mutex<Connection> {
        &self.logs
    }

    pub fn clear_logs(&self) -> Result<()> {
        let connection = self.logs.lock().expect("logs database mutex poisoned");
        connection.execute("DELETE FROM log_groups", [])?;
        connection.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
        Ok(())
    }

    fn cleanup_logs(&self, now: i64) -> Result<()> {
        let connection = self.logs.lock().expect("logs database mutex poisoned");
        connection.execute(
            "DELETE FROM log_groups WHERE last_at < ?1",
            [now - 7 * 24 * 60 * 60_000],
        )?;
        loop {
            let logical_bytes: i64 = connection.query_row(
                "SELECT COALESCE(SUM(length(fingerprint)+length(level)+length(source)+length(code)+length(message)+128),0) FROM log_groups",
                [],
                |row| row.get(0),
            )?;
            if logical_bytes <= 10 * 1024 * 1024 {
                break;
            }
            let removed = connection.execute(
                "DELETE FROM log_groups WHERE id IN (SELECT id FROM log_groups ORDER BY last_at ASC LIMIT 50)",
                [],
            )?;
            if removed == 0 {
                break;
            }
        }
        connection.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
        Ok(())
    }
}

fn replace_file(from: &Path, to: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    if to.exists() {
        let backup = to.with_extension("json.bak");
        let _ = fs::remove_file(&backup);
        fs::rename(to, &backup)?;
        match fs::rename(from, to) {
            Ok(()) => {
                let _ = fs::remove_file(backup);
                return Ok(());
            }
            Err(error) => {
                let _ = fs::rename(backup, to);
                return Err(error);
            }
        }
    }
    fs::rename(from, to)
}

fn market_name(market: Market) -> &'static str {
    match market {
        Market::Usdm => "usdm",
        Market::Coinm => "coinm",
    }
}

fn parse_market(value: &str) -> Market {
    if value == "coinm" {
        Market::Coinm
    } else {
        Market::Usdm
    }
}

fn status_name(status: ContractStatus) -> &'static str {
    match status {
        ContractStatus::Trading => "TRADING",
        ContractStatus::Delisted => "DELISTED",
    }
}

fn direction_name(direction: Direction) -> &'static str {
    match direction {
        Direction::Up => "up",
        Direction::Down => "down",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persists_normalized_settings() {
        let temp = tempfile::tempdir().unwrap();
        let persistence = Persistence::open(temp.path().to_path_buf()).unwrap();
        let mut settings = AppSettings::default();
        settings.visible_rows = 99;
        settings.refresh_interval_ms = 42;
        persistence.save_settings(&settings).unwrap();
        let loaded = persistence.load_settings().unwrap();
        assert_eq!(loaded.visible_rows, 20);
        assert_eq!(loaded.refresh_interval_ms, 100);
    }
}

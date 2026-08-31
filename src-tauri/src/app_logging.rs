use anyhow::Result;
use regex::Regex;
use rusqlite::{params, OptionalExtension};
use serde::Serialize;

use crate::persistence::Persistence;
use std::sync::OnceLock;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogRow {
    pub id: i64,
    pub level: String,
    pub source: String,
    pub code: String,
    pub message: String,
    pub first_at: i64,
    pub last_at: i64,
    pub count: u32,
    pub recovered: bool,
}

pub struct LogInput<'a> {
    pub level: &'a str,
    pub source: &'a str,
    pub code: &'a str,
    pub message: &'a str,
    pub at: i64,
}

impl Persistence {
    pub fn append_log(&self, input: LogInput<'_>) -> Result<()> {
        let message = sanitize_message(input.message);
        let fingerprint = format!("{}\0{}\0{}", input.source, input.code, message);
        let connection = self
            .logs_connection()
            .lock()
            .expect("logs database mutex poisoned");
        let active = connection
            .query_row(
                "SELECT id,fingerprint FROM log_groups WHERE active=1 AND source=?1 ORDER BY id DESC LIMIT 1",
                [input.source],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;
        if let Some((id, active_fingerprint)) = active {
            if active_fingerprint == fingerprint {
                connection.execute(
                    "UPDATE log_groups SET last_at=?1,count=count+1 WHERE id=?2",
                    params![input.at, id],
                )?;
                return Ok(());
            }
            connection.execute("UPDATE log_groups SET active=0 WHERE id=?1", [id])?;
        }
        connection.execute(
            "INSERT INTO log_groups(fingerprint,level,source,code,message,first_at,last_at,count,recovered,active) VALUES(?1,?2,?3,?4,?5,?6,?6,1,0,1)",
            params![fingerprint, input.level, input.source, input.code, message, input.at],
        )?;
        Ok(())
    }

    pub fn append_recovery(&self, source: &str, message: &str, at: i64) -> Result<()> {
        let connection = self
            .logs_connection()
            .lock()
            .expect("logs database mutex poisoned");
        let updated = connection.execute(
            "UPDATE log_groups SET active=0 WHERE active=1 AND source=?1",
            [source],
        )?;
        if updated == 0 {
            return Ok(());
        }
        connection.execute(
            "INSERT INTO log_groups(fingerprint,level,source,code,message,first_at,last_at,count,recovered,active) VALUES(?1,'info',?2,'RECOVERED',?3,?4,?4,1,1,0)",
            params![format!("recovery:{source}:{at}"), source, sanitize_message(message), at],
        )?;
        Ok(())
    }

    pub fn query_logs(&self, offset: u32, limit: u32) -> Result<Vec<LogRow>> {
        let connection = self
            .logs_connection()
            .lock()
            .expect("logs database mutex poisoned");
        let mut statement = connection.prepare(
            "SELECT id,level,source,code,message,first_at,last_at,count,recovered FROM log_groups ORDER BY last_at DESC LIMIT ?1 OFFSET ?2",
        )?;
        let rows = statement.query_map(params![limit.clamp(1, 100), offset], |row| {
            Ok(LogRow {
                id: row.get(0)?,
                level: row.get(1)?,
                source: row.get(2)?,
                code: row.get(3)?,
                message: row.get(4)?,
                first_at: row.get(5)?,
                last_at: row.get(6)?,
                count: row.get(7)?,
                recovered: row.get::<_, i64>(8)? != 0,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }
}

pub fn sanitize_message(message: &str) -> String {
    static AUTH: OnceLock<Regex> = OnceLock::new();
    static QUERY: OnceLock<Regex> = OnceLock::new();
    static CREDENTIALS: OnceLock<Regex> = OnceLock::new();
    let auth = AUTH.get_or_init(|| {
        Regex::new(r"(?i)(authorization\s*:\s*)[^\s,]+").expect("static auth regex")
    });
    let query = QUERY.get_or_init(|| {
        Regex::new(r"(?i)([?&](?:token|signature|key|password)=)[^&\s]+")
            .expect("static query regex")
    });
    let credentials = CREDENTIALS.get_or_init(|| {
        Regex::new(r"(https?://)[^:@\s/]+:[^@\s/]+@").expect("static credentials regex")
    });
    let value = auth.replace_all(message, "$1***");
    let value = query.replace_all(&value, "$1***");
    credentials.replace_all(&value, "$1***:***@").into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_repeated_errors_and_recovery() {
        let temp = tempfile::tempdir().unwrap();
        let persistence = Persistence::open(temp.path().to_path_buf()).unwrap();
        for at in (0..=25_000).step_by(5_000) {
            persistence
                .append_log(LogInput {
                    level: "error",
                    source: "COIN-M WebSocket",
                    code: "TIMEOUT",
                    message: "连接超时",
                    at,
                })
                .unwrap();
        }
        persistence
            .append_recovery("COIN-M WebSocket", "已恢复", 30_000)
            .unwrap();
        let rows = persistence.query_logs(0, 20).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1].count, 6);
        assert!(rows[0].recovered);
    }

    #[test]
    fn redacts_proxy_credentials() {
        assert_eq!(
            sanitize_message("https://alice:secret@host?token=abc"),
            "https://***:***@host?token=***"
        );
    }
}

//! SMSClient — 设置键值存取 (PRD §5.1 settings 表 / F8)
//!
//! 简单 KV：theme / preferred_channel / delivery_report / broadcast_interval 等，
//! 值统一存 TEXT，类型转换由前端负责。

use rusqlite::{params, Connection, Result as SqlResult};
use tauri::State;

use crate::db::DbState;

// ============================================================
// 核心存取（可单测）
// ============================================================

/// 读取设置值；key 不存在返回 Ok(None)。
pub fn get(conn: &Connection, key: &str) -> SqlResult<Option<String>> {
    let mut stmt = conn.prepare("SELECT value FROM settings WHERE key = ?1")?;
    let mut rows = stmt.query(params![key])?;
    match rows.next()? {
        Some(row) => Ok(Some(row.get(0)?)),
        None => Ok(None),
    }
}

/// 写入/更新设置值（INSERT OR REPLACE）。
pub fn set(conn: &Connection, key: &str, value: &str) -> SqlResult<()> {
    if key.trim().is_empty() {
        return Err(rusqlite::Error::InvalidParameterName(
            "设置键不能为空".into(),
        ));
    }
    conn.execute(
        "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
        params![key, value],
    )?;
    Ok(())
}

// ============================================================
// Tauri Commands（薄封装）
// ============================================================

fn map_err(e: rusqlite::Error) -> String {
    match e {
        rusqlite::Error::InvalidParameterName(msg) => msg,
        other => format!("数据库错误: {other}"),
    }
}

#[tauri::command]
pub fn get_setting(db: State<'_, DbState>, key: String) -> Result<Option<String>, String> {
    db.with(|c| get(c, &key)).map_err(map_err)
}

#[tauri::command]
pub fn set_setting(db: State<'_, DbState>, key: String, value: String) -> Result<(), String> {
    db.with(|c| set(c, &key, &value)).map_err(map_err)
}

// ============================================================
// 单元测试
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;

    #[test]
    fn get_missing_key_returns_none() {
        let conn = open_in_memory().unwrap();
        assert_eq!(get(&conn, "nope").unwrap(), None);
    }

    #[test]
    fn set_then_get_roundtrip() {
        let conn = open_in_memory().unwrap();
        set(&conn, "theme", "dark").unwrap();
        assert_eq!(get(&conn, "theme").unwrap().as_deref(), Some("dark"));
    }

    #[test]
    fn set_overwrites_existing() {
        let conn = open_in_memory().unwrap();
        set(&conn, "theme", "dark").unwrap();
        set(&conn, "theme", "light").unwrap();
        assert_eq!(get(&conn, "theme").unwrap().as_deref(), Some("light"));
    }

    #[test]
    fn empty_key_rejected() {
        let conn = open_in_memory().unwrap();
        assert!(set(&conn, "  ", "x").is_err());
    }

    #[test]
    fn chinese_value_roundtrip() {
        let conn = open_in_memory().unwrap();
        set(&conn, "signature", "来自 ASTeam 的问候").unwrap();
        assert_eq!(
            get(&conn, "signature").unwrap().as_deref(),
            Some("来自 ASTeam 的问候")
        );
    }
}

//! SMSClient — SQLite 数据层 (PRD §5)
//!
//! 表结构严格按 PRD §5.1 SQL 定义。数据库文件位于应用数据目录 smsclient.db。
//! Tauri 托管状态 DbState 包裹 Mutex<Connection>，供各模块 command 使用。

use rusqlite::{Connection, Result};
use std::path::Path;
use std::sync::Mutex;
use tauri::Manager;

// ============================================================
// Schema (PRD §5.1 — 四张表)
// ============================================================

/// PRD §5.1 冻结表结构，幂等创建（IF NOT EXISTS）。
pub const SCHEMA_SQL: &str = r#"
-- 联系人表
CREATE TABLE IF NOT EXISTS contacts (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    name        TEXT NOT NULL,
    phone       TEXT NOT NULL UNIQUE,
    note        TEXT,
    created_at  DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- 短信表
CREATE TABLE IF NOT EXISTS messages (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    direction   TEXT CHECK(direction IN ('in', 'out')) NOT NULL,
    phone       TEXT NOT NULL,
    content     TEXT NOT NULL,
    encoding    TEXT DEFAULT 'UCS2',
    is_long     BOOLEAN DEFAULT 0,
    part_index  INTEGER DEFAULT 1,
    part_total  INTEGER DEFAULT 1,
    group_id    TEXT,
    status      TEXT DEFAULT 'pending',
    error       TEXT,
    timestamp   DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- 群发任务表
CREATE TABLE IF NOT EXISTS broadcast_tasks (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    content     TEXT NOT NULL,
    total       INTEGER NOT NULL,
    sent        INTEGER DEFAULT 0,
    failed      INTEGER DEFAULT 0,
    status      TEXT DEFAULT 'pending',
    created_at  DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- 设置表
CREATE TABLE IF NOT EXISTS settings (
    key         TEXT PRIMARY KEY,
    value       TEXT NOT NULL
);

-- 检索索引（F1 按号码/时间查询，F7 按号码查联系人）
CREATE INDEX IF NOT EXISTS idx_messages_phone    ON messages(phone);
CREATE INDEX IF NOT EXISTS idx_messages_ts       ON messages(timestamp);
CREATE INDEX IF NOT EXISTS idx_messages_dir_ts   ON messages(direction, timestamp);
"#;

/// 打开数据库并执行幂等迁移（建表 + 索引）。
pub fn open(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)?;
    configure(&conn)?;
    conn.execute_batch(SCHEMA_SQL)?;
    Ok(conn)
}

/// 打开内存数据库（单元测试用），同样执行建表迁移。
pub fn open_in_memory() -> Result<Connection> {
    let conn = Connection::open_in_memory()?;
    configure(&conn)?;
    conn.execute_batch(SCHEMA_SQL)?;
    Ok(conn)
}

fn configure(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "PRAGMA foreign_keys = ON;
         PRAGMA journal_mode = WAL;
         PRAGMA busy_timeout = 5000;",
    )?;
    Ok(())
}

// ============================================================
// Tauri 托管状态
// ============================================================

/// Tauri managed state：全局唯一数据库连接（Mutex 保证线程安全）。
pub struct DbState(pub Mutex<Connection>);

impl DbState {
    /// 加锁获取连接引用并执行 f；锁中毒时返回 Err。
    /// f 返回 rusqlite::Result<T>，with 直接透传不包裹。
    pub fn with<R>(&self, f: impl FnOnce(&Connection) -> Result<R>) -> Result<R> {
        let guard = self
            .0
            .lock()
            .map_err(|_| rusqlite::Error::InvalidParameterName("db mutex poisoned".into()))?;
        f(&guard)
    }
}

/// 应用启动时调用：初始化数据目录、建库、托管状态。
/// 支持绿色便携模式：若 exe 所在目录存在 data/ 文件夹，则优先使用本地数据库。
pub fn init(app: &mut tauri::App) -> Result<()> {
    let portable_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("data")))
        .filter(|d| d.exists() && d.is_dir());

    let dir = if let Some(p) = portable_dir {
        p
    } else {
        let app_dir = app
            .path()
            .app_data_dir()
            .map_err(|e| rusqlite::Error::InvalidParameterName(format!("app_data_dir: {e}")))?;
        std::fs::create_dir_all(&app_dir)
            .map_err(|e| rusqlite::Error::InvalidParameterName(format!("create_dir_all: {e}")))?;
        app_dir
    };

    let path = dir.join("smsclient.db");
    let conn = open(&path)?;
    app.manage(DbState(Mutex::new(conn)));
    Ok(())
}

// ============================================================
// 单元测试
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_creates_all_four_tables() {
        let conn = open_in_memory().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT name FROM sqlite_master WHERE type='table' AND name IN \
                 ('contacts','messages','broadcast_tasks','settings') ORDER BY name",
            )
            .unwrap();
        let names: Vec<String> = stmt
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<Vec<_>>>()
            .unwrap();
        assert_eq!(
            names,
            vec!["broadcast_tasks", "contacts", "messages", "settings"]
        );
    }

    #[test]
    fn schema_is_idempotent() {
        // 重复执行不报错（IF NOT EXISTS）
        let conn = open_in_memory().unwrap();
        conn.execute_batch(SCHEMA_SQL).unwrap();
        conn.execute_batch(SCHEMA_SQL).unwrap();
    }

    #[test]
    fn messages_direction_check_enforced() {
        let conn = open_in_memory().unwrap();
        let err = conn.execute(
            "INSERT INTO messages (direction, phone, content) VALUES ('sideways', '10086', 'x')",
            [],
        );
        assert!(err.is_err(), "direction CHECK 约束应拒绝非法值");
    }

    #[test]
    fn contacts_phone_unique_enforced() {
        let conn = open_in_memory().unwrap();
        conn.execute(
            "INSERT INTO contacts (name, phone) VALUES ('A', '13800138000')",
            [],
        )
        .unwrap();
        let err = conn.execute(
            "INSERT INTO contacts (name, phone) VALUES ('B', '13800138000')",
            [],
        );
        assert!(err.is_err(), "phone UNIQUE 约束应拒绝重复号码");
    }

    #[test]
    fn messages_defaults_apply() {
        let conn = open_in_memory().unwrap();
        conn.execute(
            "INSERT INTO messages (direction, phone, content) VALUES ('in', '10086', 'hi')",
            [],
        )
        .unwrap();
        let (enc, is_long, part_index, part_total, status): (String, i64, i64, i64, String) = conn
            .query_row(
                "SELECT encoding, is_long, part_index, part_total, status FROM messages",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .unwrap();
        assert_eq!(enc, "UCS2");
        assert_eq!(is_long, 0);
        assert_eq!(part_index, 1);
        assert_eq!(part_total, 1);
        assert_eq!(status, "pending");
    }

    #[test]
    fn settings_key_value_roundtrip() {
        let conn = open_in_memory().unwrap();
        conn.execute(
            "INSERT INTO settings (key, value) VALUES ('theme', 'dark')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES ('theme', 'light')",
            [],
        )
        .unwrap();
        let v: String = conn
            .query_row("SELECT value FROM settings WHERE key='theme'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(v, "light");
    }
}

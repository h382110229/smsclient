//! SMSClient — 短信消息存取 (PRD §5.1 messages 表 / F1 F5)
//!
//! M2 提供持久化读写；M3 WinRT 通道将复用 insert/list，M5 回执更新 status。

use rusqlite::{params, Connection, Result as SqlResult};
use tauri::State;

use crate::contacts::normalize_phone;
use crate::db::DbState;
use crate::models::{NewMessage, SmsMessage};

// ============================================================
// 核心存取（可单测）
// ============================================================

/// 插入一条消息。若 msg.timestamp 存在则写入指定时间，否则由数据库生成 UTC 时间戳。
/// 返回自增主键 id。
pub fn insert(conn: &Connection, msg: &NewMessage) -> SqlResult<i64> {
    let phone = normalize_phone(&msg.phone);
    if phone.is_empty() {
        return Err(rusqlite::Error::InvalidParameterName(
            "手机号码不能为空".into(),
        ));
    }
    if msg.content.trim().is_empty() {
        return Err(rusqlite::Error::InvalidParameterName(
            "短信内容不能为空".into(),
        ));
    }
    if let Some(ts) = &msg.timestamp {
        conn.execute(
            "INSERT INTO messages \
             (direction, phone, content, encoding, is_long, part_index, part_total, group_id, status, timestamp) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                msg.direction,
                phone,
                msg.content,
                msg.encoding,
                msg.is_long,
                msg.part_index,
                msg.part_total,
                msg.group_id,
                msg.status,
                ts,
            ],
        )?;
    } else {
        conn.execute(
            "INSERT INTO messages \
             (direction, phone, content, encoding, is_long, part_index, part_total, group_id, status, timestamp) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))",
            params![
                msg.direction,
                phone,
                msg.content,
                msg.encoding,
                msg.is_long,
                msg.part_index,
                msg.part_total,
                msg.group_id,
                msg.status,
            ],
        )?;
    }
    Ok(conn.last_insert_rowid())
}

/// 按方向列出消息，最新在前。direction 为 None 时返回全部。
pub fn list(conn: &Connection, direction: Option<&str>, limit: i64) -> SqlResult<Vec<SmsMessage>> {
    let mut out = Vec::new();
    match direction {
        Some(d) => {
            let mut stmt = conn.prepare(
                "SELECT id, direction, phone, content, encoding, is_long, part_index, \
                 part_total, group_id, status, error, timestamp \
                 FROM messages WHERE direction = ?1 ORDER BY timestamp DESC, id DESC LIMIT ?2",
            )?;
            let rows = stmt.query_map(params![d, limit], row_to_message)?;
            for r in rows {
                out.push(r?);
            }
        }
        None => {
            let mut stmt = conn.prepare(
                "SELECT id, direction, phone, content, encoding, is_long, part_index, \
                 part_total, group_id, status, error, timestamp \
                 FROM messages ORDER BY timestamp DESC, id DESC LIMIT ?1",
            )?;
            let rows = stmt.query_map(params![limit], row_to_message)?;
            for r in rows {
                out.push(r?);
            }
        }
    }
    Ok(out)
}

/// 按 id 查询单条消息。
pub fn get(conn: &Connection, id: i64) -> SqlResult<SmsMessage> {
    conn.query_row(
        "SELECT id, direction, phone, content, encoding, is_long, part_index, \
         part_total, group_id, status, error, timestamp \
         FROM messages WHERE id = ?1",
        params![id],
        row_to_message,
    )
}

/// 更新消息状态（pending/sent/delivered/read/failed）。error 可选。
pub fn set_status(conn: &Connection, id: i64, status: &str, error: Option<&str>) -> SqlResult<()> {
    let affected = conn.execute(
        "UPDATE messages SET status = ?1, error = ?2 WHERE id = ?3",
        params![status, error, id],
    )?;
    if affected == 0 {
        return Err(rusqlite::Error::QueryReturnedNoRows);
    }
    Ok(())
}

/// 标记收件箱消息为已读（status → 'read'）。
pub fn mark_read(conn: &Connection, id: i64) -> SqlResult<()> {
    set_status(conn, id, "read", None)
}

fn row_to_message(row: &rusqlite::Row<'_>) -> SqlResult<SmsMessage> {
    Ok(SmsMessage {
        id: row.get(0)?,
        direction: row.get(1)?,
        phone: row.get(2)?,
        content: row.get(3)?,
        encoding: row.get(4)?,
        is_long: row.get(5)?,
        part_index: row.get(6)?,
        part_total: row.get(7)?,
        group_id: row.get(8)?,
        status: row.get(9)?,
        error: row.get(10)?,
        timestamp: row.get(11)?,
    })
}

// ============================================================
// Tauri Commands（薄封装）
// ============================================================

fn map_err(e: rusqlite::Error) -> String {
    match e {
        rusqlite::Error::QueryReturnedNoRows => "消息不存在".to_string(),
        rusqlite::Error::InvalidParameterName(msg) => msg,
        other => format!("数据库错误: {other}"),
    }
}

/// 获取收件箱（纯 DB 极速读取，彻底剥离硬件同步，绝不阻塞 UI 或争抢 SIM 卡总线）
#[tauri::command]
pub async fn get_inbox(db: State<'_, DbState>) -> Result<Vec<SmsMessage>, String> {
    db.with(|c| {
        // 消费事件驱动缓冲区（SmsDevice SmsMessageReceived 兜底推入）
        for msg in crate::sms::winrt::drain_pending_sms() {
            if !crate::sms::inbox_exists_recent(c, &msg.phone, &msg.content, msg.part_index)
                .unwrap_or(false)
            {
                let _ = insert(c, &msg);
            }
        }
        list(c, Some("in"), 200)
    })
    .map_err(map_err)
}

/// 获取已发送（direction='out'，最新在前，上限 200 条）
#[tauri::command]
pub fn get_sent(db: State<'_, DbState>) -> Result<Vec<SmsMessage>, String> {
    db.with(|c| list(c, Some("out"), 200)).map_err(map_err)
}

/// 标记消息已读
#[tauri::command]
pub fn mark_message_read(db: State<'_, DbState>, id: i64) -> Result<(), String> {
    db.with(|c| mark_read(c, id)).map_err(map_err)
}

// ============================================================
// 单元测试
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;

    #[test]
    fn insert_and_get_message() {
        let conn = open_in_memory().unwrap();
        let msg = NewMessage::outgoing("138 0013 8000".into(), "你好世界".into());
        let id = insert(&conn, &msg).unwrap();
        assert!(id >= 1);
        let loaded = get(&conn, id).unwrap();
        assert_eq!(loaded.direction, "out");
        assert_eq!(loaded.phone, "13800138000"); // 号码规范化
        assert_eq!(loaded.content, "你好世界");
        assert_eq!(loaded.encoding, "UCS2");
        assert_eq!(loaded.status, "pending");
        assert!(!loaded.timestamp.is_empty(), "timestamp 应由 DEFAULT 填充");
    }

    #[test]
    fn insert_rejects_empty_phone_or_content() {
        let conn = open_in_memory().unwrap();
        assert!(insert(&conn, &NewMessage::outgoing("  ".into(), "hi".into())).is_err());
        assert!(insert(
            &conn,
            &NewMessage::outgoing("13800138000".into(), "  ".into())
        )
        .is_err());
    }

    #[test]
    fn list_filters_by_direction() {
        let conn = open_in_memory().unwrap();
        insert(
            &conn,
            &NewMessage::outgoing("13800138000".into(), "发1".into()),
        )
        .unwrap();
        insert(&conn, &NewMessage::incoming("10086".into(), "收1".into())).unwrap();
        insert(&conn, &NewMessage::incoming("10086".into(), "收2".into())).unwrap();

        assert_eq!(list(&conn, Some("in"), 100).unwrap().len(), 2);
        assert_eq!(list(&conn, Some("out"), 100).unwrap().len(), 1);
        assert_eq!(list(&conn, None, 100).unwrap().len(), 3);
    }

    #[test]
    fn list_respects_limit() {
        let conn = open_in_memory().unwrap();
        for i in 0..5 {
            insert(
                &conn,
                &NewMessage::incoming("10086".into(), format!("第{i}条")),
            )
            .unwrap();
        }
        assert_eq!(list(&conn, Some("in"), 3).unwrap().len(), 3);
    }

    #[test]
    fn status_update_flow() {
        let conn = open_in_memory().unwrap();
        let id = insert(
            &conn,
            &NewMessage::outgoing("13800138000".into(), "测试".into()),
        )
        .unwrap();
        set_status(&conn, id, "sent", None).unwrap();
        assert_eq!(get(&conn, id).unwrap().status, "sent");
        set_status(&conn, id, "failed", Some("网络超时")).unwrap();
        let m = get(&conn, id).unwrap();
        assert_eq!(m.status, "failed");
        assert_eq!(m.error.as_deref(), Some("网络超时"));
        assert!(set_status(&conn, 999, "sent", None).is_err());
    }

    #[test]
    fn mark_read_only_changes_status() {
        let conn = open_in_memory().unwrap();
        let id = insert(&conn, &NewMessage::incoming("10086".into(), "通知".into())).unwrap();
        mark_read(&conn, id).unwrap();
        let m = get(&conn, id).unwrap();
        assert_eq!(m.status, "read");
        assert_eq!(m.direction, "in");
        assert_eq!(m.content, "通知");
    }

    #[test]
    fn long_sms_fields_persist() {
        let conn = open_in_memory().unwrap();
        let long_content = "长短信测试".repeat(20); // 100 chars
        let mut msg = NewMessage::outgoing("13900139000".into(), long_content);
        msg.group_id = Some("grp-001".into());
        let id = insert(&conn, &msg).unwrap();
        let m = get(&conn, id).unwrap();
        assert!(m.is_long);
        assert_eq!(m.part_total, 2); // 100/67 → 2 段
        assert_eq!(m.part_index, 1);
        assert_eq!(m.group_id.as_deref(), Some("grp-001"));
    }
}

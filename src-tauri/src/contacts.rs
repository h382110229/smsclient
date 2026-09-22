//! SMSClient — 联系人 CRUD (PRD §5.1 contacts 表 / F7)
//!
//! 核心函数以 &Connection 为参数（可单测），Tauri command 薄封装托管状态。
//! 号码规范化：写入前去除空格/短横线/括号，并剥离中国大陆 +86/86 前缀。

use rusqlite::{params, Connection, Result as SqlResult};
use tauri::State;

use crate::db::DbState;
use crate::models::Contact;

// ============================================================
// 号码规范化
// ============================================================

/// 规范化手机号：trim → 去分隔符 → 剥离 +86 / 86 前缀（仅当剩余长度 ≥ 11）。
///
/// 例："+86 138-0013-8000" → "13800138000"；"10086" → "10086"。
pub fn normalize_phone(raw: &str) -> String {
    let mut s: String = raw
        .trim()
        .chars()
        .filter(|c| !matches!(c, ' ' | '-' | '(' | ')' | '\u{200b}'))
        .collect();
    if let Some(rest) = s.strip_prefix("+86") {
        if rest.len() >= 11 {
            s = rest.to_string();
        }
    } else if let Some(rest) = s.strip_prefix("86") {
        if rest.len() >= 11 {
            s = rest.to_string();
        }
    }
    s
}

/// 校验非空并返回规范化结果；空值返回 Err。
fn validated(name: &str, phone: &str) -> SqlResult<(String, String)> {
    let name = name.trim();
    let phone = normalize_phone(phone);
    if name.is_empty() {
        return Err(rusqlite::Error::InvalidParameterName("姓名不能为空".into()));
    }
    if phone.is_empty() {
        return Err(rusqlite::Error::InvalidParameterName(
            "手机号码不能为空".into(),
        ));
    }
    Ok((name.to_string(), phone))
}

// ============================================================
// 核心 CRUD（可单测）
// ============================================================

/// 查询单个联系人，不存在返回 Err。
pub fn get(conn: &Connection, id: i64) -> SqlResult<Contact> {
    conn.query_row(
        "SELECT id, name, phone, note, created_at FROM contacts WHERE id = ?1",
        params![id],
        row_to_contact,
    )
}

/// 列出联系人；search 非空时按姓名/号码模糊匹配。按姓名升序。
pub fn list(conn: &Connection, search: Option<&str>) -> SqlResult<Vec<Contact>> {
    let mut out = Vec::new();
    match search {
        Some(q) if !q.trim().is_empty() => {
            let like = format!("%{}%", q.trim());
            let mut stmt = conn.prepare(
                "SELECT id, name, phone, note, created_at FROM contacts \
                 WHERE name LIKE ?1 OR phone LIKE ?1 ORDER BY name ASC",
            )?;
            let rows = stmt.query_map(params![like], row_to_contact)?;
            for r in rows {
                out.push(r?);
            }
        }
        _ => {
            let mut stmt = conn.prepare(
                "SELECT id, name, phone, note, created_at FROM contacts ORDER BY name ASC",
            )?;
            let rows = stmt.query_map([], row_to_contact)?;
            for r in rows {
                out.push(r?);
            }
        }
    }
    Ok(out)
}

/// 新增联系人；号码重复返回 Err。
pub fn insert(
    conn: &Connection,
    name: &str,
    phone: &str,
    note: Option<&str>,
) -> SqlResult<Contact> {
    let (name, phone) = validated(name, phone)?;
    if find_id_by_phone(conn, &phone)?.is_some() {
        return Err(rusqlite::Error::InvalidParameterName(format!(
            "该号码已存在联系人: {phone}"
        )));
    }
    let note = note.map(str::trim).filter(|n| !n.is_empty());
    conn.execute(
        "INSERT INTO contacts (name, phone, note) VALUES (?1, ?2, ?3)",
        params![name, phone, note],
    )?;
    get(conn, conn.last_insert_rowid())
}

/// 更新联系人；不存在返回 Err，号码冲突返回 Err。
pub fn update(
    conn: &Connection,
    id: i64,
    name: &str,
    phone: &str,
    note: Option<&str>,
) -> SqlResult<Contact> {
    let (name, phone) = validated(name, phone)?;
    if let Some(existing) = find_id_by_phone(conn, &phone)? {
        if existing != id {
            return Err(rusqlite::Error::InvalidParameterName(format!(
                "该号码已存在联系人: {phone}"
            )));
        }
    }
    let note = note.map(str::trim).filter(|n| !n.is_empty());
    let affected = conn.execute(
        "UPDATE contacts SET name = ?1, phone = ?2, note = ?3 WHERE id = ?4",
        params![name, phone, note, id],
    )?;
    if affected == 0 {
        return Err(rusqlite::Error::QueryReturnedNoRows);
    }
    get(conn, id)
}

/// 删除联系人；不存在返回 Err。
pub fn delete(conn: &Connection, id: i64) -> SqlResult<()> {
    let affected = conn.execute("DELETE FROM contacts WHERE id = ?1", params![id])?;
    if affected == 0 {
        return Err(rusqlite::Error::QueryReturnedNoRows);
    }
    Ok(())
}

fn find_id_by_phone(conn: &Connection, phone: &str) -> SqlResult<Option<i64>> {
    let mut stmt = conn.prepare("SELECT id FROM contacts WHERE phone = ?1 LIMIT 1")?;
    let mut rows = stmt.query(params![phone])?;
    match rows.next()? {
        Some(row) => Ok(Some(row.get(0)?)),
        None => Ok(None),
    }
}

fn row_to_contact(row: &rusqlite::Row<'_>) -> SqlResult<Contact> {
    Ok(Contact {
        id: row.get(0)?,
        name: row.get(1)?,
        phone: row.get(2)?,
        note: row.get(3)?,
        created_at: row.get(4)?,
    })
}

// ============================================================
// Tauri Commands（薄封装）
// ============================================================

fn map_err(e: rusqlite::Error) -> String {
    match e {
        rusqlite::Error::QueryReturnedNoRows => "联系人不存在".to_string(),
        rusqlite::Error::InvalidParameterName(msg) => msg,
        other => format!("数据库错误: {other}"),
    }
}

#[tauri::command]
pub fn list_contacts(
    db: State<'_, DbState>,
    search: Option<String>,
) -> Result<Vec<Contact>, String> {
    db.with(|c| list(c, search.as_deref())).map_err(map_err)
}

#[tauri::command]
pub fn add_contact(
    db: State<'_, DbState>,
    name: String,
    phone: String,
    note: Option<String>,
) -> Result<Contact, String> {
    db.with(|c| insert(c, &name, &phone, note.as_deref()))
        .map_err(map_err)
}

#[tauri::command]
pub fn update_contact(
    db: State<'_, DbState>,
    id: i64,
    name: String,
    phone: String,
    note: Option<String>,
) -> Result<Contact, String> {
    db.with(|c| update(c, id, &name, &phone, note.as_deref()))
        .map_err(map_err)
}

#[tauri::command]
pub fn delete_contact(db: State<'_, DbState>, id: i64) -> Result<(), String> {
    db.with(|c| delete(c, id)).map_err(map_err)
}

// ============================================================
// 单元测试
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;

    #[test]
    fn normalize_phone_variants() {
        assert_eq!(normalize_phone("+86 138-0013-8000"), "13800138000");
        assert_eq!(normalize_phone("8613800138000"), "13800138000");
        assert_eq!(normalize_phone(" 138 0013 8000 "), "13800138000");
        assert_eq!(normalize_phone("10086"), "10086");
        assert_eq!(normalize_phone("+8610086"), "+8610086"); // 剩余 <11 不剥离
        assert_eq!(normalize_phone(""), "");
    }

    #[test]
    fn insert_and_get_contact() {
        let conn = open_in_memory().unwrap();
        let c = insert(&conn, "张三", "138 0013 8000", Some(" 同事 ")).unwrap();
        assert_eq!(c.id, 1);
        assert_eq!(c.name, "张三");
        assert_eq!(c.phone, "13800138000");
        assert_eq!(c.note.as_deref(), Some("同事"));
        assert!(c.created_at.is_some(), "created_at 应由 DEFAULT 填充");
        let again = get(&conn, 1).unwrap();
        assert_eq!(again, c);
    }

    #[test]
    fn insert_duplicate_phone_rejected() {
        let conn = open_in_memory().unwrap();
        insert(&conn, "A", "13800138000", None).unwrap();
        // 同一号码不同格式（+86/空格）应识别为重复
        let err = insert(&conn, "B", "+86 138-0013-8000", None);
        assert!(err.is_err());
        assert!(
            err.unwrap_err().to_string().contains("已存在"),
            "错误信息应为中文提示"
        );
    }

    #[test]
    fn insert_empty_name_or_phone_rejected() {
        let conn = open_in_memory().unwrap();
        assert!(insert(&conn, "  ", "13800138000", None).is_err());
        assert!(insert(&conn, "A", "   ", None).is_err());
    }

    #[test]
    fn list_all_and_search() {
        let conn = open_in_memory().unwrap();
        insert(&conn, "张三", "13800138000", None).unwrap();
        insert(&conn, "李四", "13900139000", None).unwrap();
        insert(&conn, "王五", "13700137000", None).unwrap();

        assert_eq!(list(&conn, None).unwrap().len(), 3);
        let hit = list(&conn, Some("张")).unwrap();
        assert_eq!(hit.len(), 1);
        assert_eq!(hit[0].name, "张三");
        let by_phone = list(&conn, Some("13900139")).unwrap();
        assert_eq!(by_phone.len(), 1);
        assert_eq!(by_phone[0].name, "李四");
        assert_eq!(list(&conn, Some("  ")).unwrap().len(), 3); // 空白 = 全量
    }

    #[test]
    fn update_contact_fields() {
        let conn = open_in_memory().unwrap();
        let c = insert(&conn, "张三", "13800138000", None).unwrap();
        let updated = update(&conn, c.id, "张三丰", "13900139000", Some("改了")).unwrap();
        assert_eq!(updated.name, "张三丰");
        assert_eq!(updated.phone, "13900139000");
        assert_eq!(updated.note.as_deref(), Some("改了"));
    }

    #[test]
    fn update_missing_id_rejected() {
        let conn = open_in_memory().unwrap();
        let err = update(&conn, 999, "X", "13800138000", None);
        assert!(matches!(err, Err(rusqlite::Error::QueryReturnedNoRows)));
    }

    #[test]
    fn update_to_duplicate_phone_rejected() {
        let conn = open_in_memory().unwrap();
        insert(&conn, "A", "13800138000", None).unwrap();
        let b = insert(&conn, "B", "13900139000", None).unwrap();
        // B 改成 A 的号码 → 拒绝
        assert!(update(&conn, b.id, "B", "13800138000", None).is_err());
        // B 改成自己的号码（格式不同但规范化后相同）→ 允许
        assert!(update(&conn, b.id, "B", "+86 139-0013-9000", None).is_ok());
    }

    #[test]
    fn delete_contact_removes_row() {
        let conn = open_in_memory().unwrap();
        let c = insert(&conn, "张三", "13800138000", None).unwrap();
        delete(&conn, c.id).unwrap();
        assert!(get(&conn, c.id).is_err());
        assert!(delete(&conn, c.id).is_err(), "重复删除应报不存在");
    }

    #[test]
    fn blank_note_stored_as_null() {
        let conn = open_in_memory().unwrap();
        let c = insert(&conn, "A", "13800138000", Some("   ")).unwrap();
        assert_eq!(c.note, None);
    }
}

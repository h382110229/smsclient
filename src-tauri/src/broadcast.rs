//! SMSClient — 群发任务管理 (PRD F3)
//!
//! 群发流程:
//! 1. start_broadcast: 校验 -> 创建任务 -> 后台线程逐条发送
//! 2. 后台线程: 每条短信发送后 emit broadcast-progress 事件，失败自动重试 2 次(间隔 5s)
//! 3. 限速: broadcast_interval 设置(默认 6000ms = 10条/分钟)
//! 4. cancel_broadcast: 设置原子标志，当前条发送完后停止
//!
//! 每条群发消息在 messages 表中记录(group_id = 'bc-{task_id}')，
//! broadcast_tasks 表汇总进度。

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread;
use std::time::Duration;

use rusqlite::{params, Connection};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::contacts::normalize_phone;
use crate::db::DbState;
use crate::models::BroadcastTask;
use crate::sms::SmsChannel;

// ============================================================
// 全局运行状态（同时只允许一个群发任务）
// ============================================================

struct RunningBroadcast {
    #[allow(dead_code)]
    task_id: i64,
    cancel: Arc<AtomicBool>,
}

static RUNNING: Mutex<Option<RunningBroadcast>> = Mutex::new(None);

// ============================================================
// broadcast_tasks 表 CRUD
// ============================================================

fn insert_task(conn: &Connection, content: &str, total: i64) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT INTO broadcast_tasks (content, total, sent, failed, status) \
         VALUES (?1, ?2, 0, 0, 'running')",
        params![content, total],
    )?;
    Ok(conn.last_insert_rowid())
}

fn update_progress(conn: &Connection, id: i64, sent: i64, failed: i64) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE broadcast_tasks SET sent = ?1, failed = ?2 WHERE id = ?3",
        params![sent, failed, id],
    )?;
    Ok(())
}

fn complete_task(conn: &Connection, id: i64, status: &str) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE broadcast_tasks SET status = ?1 WHERE id = ?2",
        params![status, id],
    )?;
    Ok(())
}

pub fn list_tasks(conn: &Connection, limit: i64) -> rusqlite::Result<Vec<BroadcastTask>> {
    let mut stmt = conn.prepare(
        "SELECT id, content, total, sent, failed, status, created_at \
         FROM broadcast_tasks ORDER BY created_at DESC, id DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit], |row| {
        Ok(BroadcastTask {
            id: row.get(0)?,
            content: row.get(1)?,
            total: row.get(2)?,
            sent: row.get(3)?,
            failed: row.get(4)?,
            status: row.get(5)?,
            created_at: row.get(6)?,
        })
    })?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

#[allow(dead_code)]
pub fn get_task(conn: &Connection, id: i64) -> rusqlite::Result<BroadcastTask> {
    conn.query_row(
        "SELECT id, content, total, sent, failed, status, created_at \
         FROM broadcast_tasks WHERE id = ?1",
        params![id],
        |row| {
            Ok(BroadcastTask {
                id: row.get(0)?,
                content: row.get(1)?,
                total: row.get(2)?,
                sent: row.get(3)?,
                failed: row.get(4)?,
                status: row.get(5)?,
                created_at: row.get(6)?,
            })
        },
    )
}

// ============================================================
// 发送单条（含重试）
// ============================================================

/// 发送一条短信并入库。失败时自动重试 max_retries 次，间隔 retry_interval_ms。
/// 返回 Ok(id) 或 Err(最后一条错误)。
fn send_one(
    conn: &Connection,
    phone: &str,
    content: &str,
    group_id: &str,
    max_retries: u32,
    retry_interval_ms: u64,
) -> Result<i64, String> {
    use crate::messages;

    let ch = crate::sms::active_channel(conn);

    // 入库 (pending)
    let mut msg = crate::models::NewMessage::outgoing(phone.to_string(), content.to_string());
    msg.group_id = Some(group_id.to_string());
    let id = messages::insert(conn, &msg).map_err(|e| e.to_string())?;

    // 重试循环
    let mut last_err = String::new();
    for attempt in 0..=max_retries {
        if attempt > 0 {
            thread::sleep(Duration::from_millis(retry_interval_ms));
        }
        match crate::sms::block_on(ch.send_sms(phone, content)) {
            Ok(_) => {
                messages::set_status(conn, id, "sent", None).map_err(|e| e.to_string())?;
                return Ok(id);
            }
            Err(e) => {
                eprintln!("[broadcast] {phone} attempt {attempt} failed: {e}");
                last_err = e;
            }
        }
    }

    messages::set_status(conn, id, "failed", Some(&last_err)).map_err(|e| e.to_string())?;
    Err(last_err)
}

// ============================================================
// 后台群发线程
// ============================================================

fn run_broadcast(
    task_id: i64,
    content: String,
    phones: Vec<String>,
    cancel: Arc<AtomicBool>,
    interval_ms: u64,
    db_path: PathBuf,
    app: AppHandle,
) {
    // 线程独立打开 DB (WAL + busy_timeout 允许并发)
    let conn = match crate::db::open(&db_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[broadcast] 打开 DB 失败: {e}");
            let _ = complete_task_via_path(&db_path, task_id, "failed");
            return;
        }
    };

    let total = phones.len() as i64;
    let mut sent = 0i64;
    let mut failed = 0i64;
    let group_id = format!("bc-{task_id}");

    for (i, phone) in phones.iter().enumerate() {
        // 检查取消标志
        if cancel.load(Ordering::Relaxed) {
            let _ = complete_task_via_path(&db_path, task_id, "cancelled");
            emit_broadcast_progress(&app, task_id, total, sent, failed, "cancelled", true);
            // 清理运行状态
            clear_running(task_id);
            return;
        }

        match send_one(&conn, phone, &content, &group_id, 2, 5000) {
            Ok(_) => sent += 1,
            Err(_) => failed += 1,
        }

        let _ = update_progress(&conn, task_id, sent, failed);

        // 通知前端进度
        emit_progress_detail(
            &app,
            task_id,
            total,
            sent,
            failed,
            phone,
            i + 1,
            (i + 1) >= phones.len(),
        );

        // 限速间隔（最后一条不 sleep）
        if i + 1 < phones.len() && !cancel.load(Ordering::Relaxed) {
            thread::sleep(Duration::from_millis(interval_ms));
        }
    }

    // 完成
    let _ = complete_task_via_path(&db_path, task_id, "completed");
    emit_broadcast_progress(&app, task_id, total, sent, failed, "completed", true);
    clear_running(task_id);
}

fn emit_broadcast_progress(
    app: &AppHandle,
    task_id: i64,
    total: i64,
    sent: i64,
    failed: i64,
    status: &str,
    done: bool,
) {
    let _ = app.emit(
        "broadcast-progress",
        serde_json::json!({
            "taskId": task_id,
            "total": total,
            "sent": sent,
            "failed": failed,
            "status": status,
            "done": done,
        }),
    );
}

fn emit_progress_detail(
    app: &AppHandle,
    task_id: i64,
    total: i64,
    sent: i64,
    failed: i64,
    current_phone: &str,
    index: usize,
    done: bool,
) {
    let _ = app.emit(
        "broadcast-progress",
        serde_json::json!({
            "taskId": task_id,
            "total": total,
            "sent": sent,
            "failed": failed,
            "currentPhone": current_phone,
            "index": index,
            "status": "running",
            "done": done,
        }),
    );
}

fn complete_task_via_path(db_path: &PathBuf, task_id: i64, status: &str) -> rusqlite::Result<()> {
    let conn = crate::db::open(db_path)?;
    complete_task(&conn, task_id, status)
}

fn clear_running(_task_id: i64) {
    if let Ok(mut running) = RUNNING.lock() {
        *running = None;
    }
}

// ============================================================
// Tauri Commands
// ============================================================

#[tauri::command]
pub fn start_broadcast(
    db: State<'_, DbState>,
    app: AppHandle,
    content: String,
    phones: Vec<String>,
) -> Result<i64, String> {
    // 校验
    if content.trim().is_empty() {
        return Err("短信内容不能为空".into());
    }
    if phones.is_empty() {
        return Err("至少需要一个收件人".into());
    }

    // 规范化 + 去重
    let mut unique: Vec<String> = phones
        .into_iter()
        .map(|p| normalize_phone(&p))
        .filter(|p| !p.is_empty())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    unique.sort();
    if unique.is_empty() {
        return Err("所有收件人号码无效".into());
    }

    // 检查是否已有任务运行
    {
        let mut running = RUNNING.lock().map_err(|_| "内部锁错误".to_string())?;
        if running.is_some() {
            return Err("已有群发任务在运行中，请等待完成或取消".into());
        }

        let task_id = db
            .with(|conn| insert_task(conn, &content, unique.len() as i64))
            .map_err(|e| e.to_string())?;

        let cancel = Arc::new(AtomicBool::new(false));
        *running = Some(RunningBroadcast {
            task_id,
            cancel: cancel.clone(),
        });

        // 读取限速间隔
        let interval_ms = db
            .with(|conn| {
                Ok(crate::settings::get(conn, "broadcast_interval")
                    .unwrap_or(None)
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(6000))
            })
            .unwrap_or(6000);

        // 获取 DB 路径（线程独立打开）
        let db_path = app
            .path()
            .app_data_dir()
            .map_err(|e| e.to_string())?
            .join("smsclient.db");

        thread::spawn(move || {
            run_broadcast(task_id, content, unique, cancel, interval_ms, db_path, app);
        });

        Ok(task_id)
    }
}

#[tauri::command]
pub fn cancel_broadcast() -> Result<(), String> {
    let running = RUNNING.lock().map_err(|_| "内部锁错误".to_string())?;
    if let Some(r) = running.as_ref() {
        r.cancel.store(true, Ordering::Relaxed);
        Ok(())
    } else {
        Err("当前没有运行中的群发任务".into())
    }
}

#[tauri::command]
pub fn list_broadcast_tasks(db: State<'_, DbState>) -> Result<Vec<BroadcastTask>, String> {
    db.with(|conn| list_tasks(conn, 20))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_broadcast_status(
    db: State<'_, DbState>,
    task_id: i64,
) -> Result<BroadcastTask, String> {
    db.with(|conn| get_task(conn, task_id))
        .map_err(|e| e.to_string())
}

// ============================================================
// 单元测试
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;

    #[test]
    fn insert_and_list_tasks() {
        let conn = open_in_memory().unwrap();
        let id1 = insert_task(&conn, "你好", 5).unwrap();
        let id2 = insert_task(&conn, "通知", 3).unwrap();
        assert!(id2 > id1);
        let tasks = list_tasks(&conn, 10).unwrap();
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].content, "通知");
        assert_eq!(tasks[0].total, 3);
        assert_eq!(tasks[0].status, "running");
    }

    #[test]
    fn update_progress_and_complete() {
        let conn = open_in_memory().unwrap();
        let id = insert_task(&conn, "test", 4).unwrap();
        update_progress(&conn, id, 3, 1).unwrap();
        let t = get_task(&conn, id).unwrap();
        assert_eq!(t.sent, 3);
        assert_eq!(t.failed, 1);
        complete_task(&conn, id, "completed").unwrap();
        let t = get_task(&conn, id).unwrap();
        assert_eq!(t.status, "completed");
    }

    #[test]
    fn list_tasks_respects_limit() {
        let conn = open_in_memory().unwrap();
        for i in 0..5 {
            insert_task(&conn, &format!("msg{i}"), i).unwrap();
        }
        assert_eq!(list_tasks(&conn, 3).unwrap().len(), 3);
    }
}

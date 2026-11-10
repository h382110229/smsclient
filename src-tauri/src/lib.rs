//! SMSClient — Tauri 2 Backend (非阻塞异步架构重构版)

pub mod broadcast;
pub mod contacts;
pub mod db;
pub mod messages;
pub mod models;
pub mod settings;
pub mod sms;

use tauri::{AppHandle, Emitter, Manager, State};

use crate::db::DbState;
use crate::models::{ChannelStatus, SimInfo, SmsMessage};

// ============================================================
// M3 命令：全部改为 async fn + spawn_blocking，且绝不在持有 DB 锁时执行硬件 I/O
// ============================================================

#[tauri::command]
async fn get_sim_info(db: State<'_, DbState>) -> Result<SimInfo, String> {
    let preferred = db.with(|c| Ok(sms::preferred_channel(c))).unwrap_or_else(|_| "winrt".into());
    tauri::async_runtime::spawn_blocking(move || sms::fetch_sim_info_by_pref(&preferred))
        .await
        .map_err(|e| format!("任务执行异常: {e}"))
}

#[tauri::command]
async fn get_signal_strength(db: State<'_, DbState>) -> Result<i32, String> {
    let preferred = db.with(|c| Ok(sms::preferred_channel(c))).unwrap_or_else(|_| "winrt".into());
    tauri::async_runtime::spawn_blocking(move || sms::fetch_signal_by_pref(&preferred))
        .await
        .map_err(|e| format!("任务执行异常: {e}"))
}

#[tauri::command]
async fn send_sms(
    app: AppHandle,
    phone: String,
    message: String,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let db = app.state::<DbState>();
        sms::send_non_blocking(&db, &phone, &message).map(|id| id.to_string())
    })
    .await
    .map_err(|e| format!("发送任务异常: {e}"))?
}

#[tauri::command]
async fn get_channel_status(db: State<'_, DbState>) -> Result<ChannelStatus, String> {
    let preferred = db.with(|c| Ok(sms::preferred_channel(c))).unwrap_or_else(|_| "winrt".into());
    tauri::async_runtime::spawn_blocking(move || sms::channel_status_by_pref(&preferred))
        .await
        .map_err(|e| format!("状态查询异常: {e}"))
}

#[tauri::command]
async fn sync_device_inbox(app: AppHandle) -> Result<Vec<SmsMessage>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let db = app.state::<DbState>();
        sms::sync_inbox_non_blocking(&db)
    })
    .await
    .map_err(|e| format!("同步任务异常: {e}"))?
}

// ============================================================
// Tauri App Entry
// ============================================================

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            // 1. 仅执行纯本地 SQLite 初始化（毫秒级完成，绝不阻塞窗口创建）
            db::init(app).map_err(|e| format!("SQLite 初始化失败: {e}"))?;

            // 2. 将所有 WinRT 硬件探测、事件订阅、SIM 卡同步全部移交后台守护线程
            let bg_app = app.handle().clone();
            std::thread::Builder::new()
                .name("sms-winrt-worker".into())
                .spawn(move || {
                    // 稍微延后 300ms 等待主窗口完成首帧渲染
                    std::thread::sleep(std::time::Duration::from_millis(300));

                    // 挂载 WinRT 实时短信事件监听（内部带超时保护，无硬件时快速返回）
                    if let Err(e) = sms::winrt::subscribe_inbox_events(bg_app.clone()) {
                        eprintln!("[smsclient] 短信事件订阅跳过 (无硬件或未就绪): {e}");
                    }

                    // 首次后台同步 SIM 卡存量短信
                    let db = bg_app.state::<DbState>();
                    if let Ok(new_msgs) = sms::sync_inbox_newly_inserted(&db) {
                        for m in new_msgs {
                            let _ = bg_app.emit(
                                "sms-received",
                                serde_json::json!({
                                    "id": m.id,
                                    "phone": m.phone,
                                    "content": m.content,
                                    "timestamp": m.timestamp,
                                    "partIndex": m.part_index,
                                    "partTotal": m.part_total,
                                    "groupId": m.group_id,
                                }),
                            );
                        }
                    }

                    let mut last_tick = std::time::Instant::now();
                    // 15 秒心跳轮询（兼顾热插拔重连、Modern Standby 盒盖休眠自愈与 SIM 卡槽位清理）
                    loop {
                        std::thread::sleep(std::time::Duration::from_secs(15));
                        let elapsed = last_tick.elapsed();
                        last_tick = std::time::Instant::now();

                        // 时钟跳变检测（正常 sleep 15s，若 elapsed > 25s 说明经历了系统挂起/盒盖休眠唤醒）
                        if elapsed > std::time::Duration::from_secs(25) {
                            eprintln!(
                                "[smsclient] 检测到系统休眠唤醒 (时钟跳变: {:.1}s)，重置设备缓存并重连",
                                elapsed.as_secs_f32()
                            );
                            sms::winrt::invalidate_device_cache();
                        }

                        // 若之前事件未订阅成功（例如刚插入 SIM 卡或刚从睡眠唤醒），自动尝试重连
                        sms::winrt::ensure_event_subscribed(&bg_app);

                        let db = bg_app.state::<DbState>();
                        // 关键修复（Gemini 4 Bug 1）：仅在真正从 SIM 卡拉取到新入库短信时才 emit，绝不在每 15 秒发射空事件
                        if let Ok(new_msgs) = sms::sync_inbox_newly_inserted(&db) {
                            for m in new_msgs {
                                let _ = bg_app.emit(
                                    "sms-received",
                                    serde_json::json!({
                                        "id": m.id,
                                        "phone": m.phone,
                                        "content": m.content,
                                        "timestamp": m.timestamp,
                                        "partIndex": m.part_index,
                                        "partTotal": m.part_total,
                                        "groupId": m.group_id,
                                    }),
                                );
                            }
                        }
                    }
                })
                .expect("failed to spawn sms-winrt-worker thread");

            Ok(())
        })
        .plugin(tauri_plugin_notification::init())
        .invoke_handler(tauri::generate_handler![
            get_sim_info,
            get_signal_strength,
            send_sms,
            get_channel_status,
            sync_device_inbox,
            messages::get_inbox,
            contacts::list_contacts,
            contacts::add_contact,
            contacts::update_contact,
            contacts::delete_contact,
            messages::get_sent,
            messages::mark_message_read,
            messages::mark_conversation_read,
            settings::get_setting,
            settings::set_setting,
            broadcast::start_broadcast,
            broadcast::cancel_broadcast,
            broadcast::list_broadcast_tasks,
            broadcast::get_broadcast_status,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

// ============================================================
// 单元测试
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sms::{active_channel, Channel, MockChannel};

    #[test]
    fn fetch_sim_info_returns_or_falls_back() {
        let conn = db::open_in_memory().unwrap();
        let info = sms::fetch_sim_info(&conn);
        assert!(!info.carrier.is_empty());
    }

    #[test]
    fn fetch_signal_returns_real_or_mock_value() {
        let conn = db::open_in_memory().unwrap();
        let sig = sms::fetch_signal(&conn);
        assert!((-110..=-60).contains(&sig), "signal out of range: {sig}");
    }

    #[test]
    fn send_via_channel_persists_to_db() {
        let conn = db::open_in_memory().unwrap();
        settings::set(&conn, "preferred_channel", "mock").unwrap();
        let id = sms::send(&conn, "13800138000", "你好").unwrap();
        assert!(id >= 1);
        let m = messages::get(&conn, id).unwrap();
        assert_eq!(m.direction, "out");
        assert_eq!(m.phone, "13800138000");
        assert_eq!(m.content, "你好");
        assert_eq!(m.status, "sent");
    }

    #[test]
    fn send_empty_phone_returns_error() {
        let conn = db::open_in_memory().unwrap();
        settings::set(&conn, "preferred_channel", "mock").unwrap();
        let err = sms::send(&conn, "", "hi").unwrap_err();
        assert!(err.contains("不能为空"));
        assert!(messages::list(&conn, None, 100).unwrap().is_empty());
    }

    #[test]
    fn send_normalizes_phone() {
        let conn = db::open_in_memory().unwrap();
        settings::set(&conn, "preferred_channel", "mock").unwrap();
        let id = sms::send(&conn, "+86 138-0013-8000", "hello").unwrap();
        let m = messages::get(&conn, id).unwrap();
        assert_eq!(m.phone, "13800138000");
    }

    #[test]
    fn sync_inbox_with_dedupes() {
        let conn = db::open_in_memory().unwrap();
        let mock = MockChannel::new();
        mock.push_inbox(sms::ReceivedSms {
            from: "10086".into(),
            body: "流量提醒".into(),
            timestamp: "".into(),
            device_id: 1,
            part_index: 1,
            part_total: 1,
            ref_id: 0,
        });
        let ch = Channel::Mock(mock);
        let list1 = sms::sync_inbox_with(&conn, &ch).unwrap();
        assert_eq!(list1.len(), 1);
        assert_eq!(list1[0].content, "流量提醒");
        assert_eq!(list1[0].direction, "in");
        assert_eq!(list1[0].status, "delivered");
        // 第二次不重复入库
        let list2 = sms::sync_inbox_with(&conn, &ch).unwrap();
        assert_eq!(list2.len(), 1);
    }

    #[test]
    fn sync_inbox_without_device_reads_db() {
        let conn = db::open_in_memory().unwrap();
        let msg = models::NewMessage::incoming("10086".into(), "余额提醒".into());
        messages::insert(&conn, &msg).unwrap();
        let ch = Channel::Mock(MockChannel::new());
        let list = sms::sync_inbox_with(&conn, &ch).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].content, "余额提醒");
    }

    #[test]
    fn active_channel_handles_device_or_mock() {
        let conn = db::open_in_memory().unwrap();
        let ch = active_channel(&conn);
        assert!(matches!(ch, Channel::Winrt(_) | Channel::Mock(_)));
    }

    #[test]
    fn channel_status_reports_winrt_consistently() {
        let conn = db::open_in_memory().unwrap();
        let st = sms::channel_status(&conn);
        assert_eq!(st.channel_type, "winrt");
        if !st.available {
            assert!(st.error.is_some());
        }
    }

    #[test]
    fn channel_status_mock_when_preferred() {
        let conn = db::open_in_memory().unwrap();
        settings::set(&conn, "preferred_channel", "mock").unwrap();
        let st = sms::channel_status(&conn);
        assert_eq!(st.channel_type, "mock");
        assert!(st.available);
    }
}

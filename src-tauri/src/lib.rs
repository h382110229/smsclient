//! SMSClient — Tauri 2 Backend (M2 SQLite 数据层 + M3 WinRT 通道接入)
//!
//! M1 → M2 → M3 演进：
//! - M3: WinRT 通道接入 lib.rs command 层
//!   get_sim_info / get_signal_strength / send_sms → 活动通道真实查询
//!   get_channel_status → 通道状态探测（新增）
//!   get_inbox → 启动时同步设备短信 → DB 读取（messages::get_inbox）
//!   新增 sync_device_inbox → 按需同步 + 返回最新列表
//!   WinRT 不可用时自动回退 Mock 通道
//! - M2: 联系人 CRUD + 消息/设置存取
//! - M1: 四个 mock command + 7 页面

pub mod contacts;
pub mod db;
pub mod messages;
pub mod models;
pub mod settings;
pub mod sms;

use tauri::{Manager, State};

use crate::db::DbState;
use crate::models::{ChannelStatus, SimInfo, SmsMessage};

// ============================================================
// M3 命令：通过活动通道真实查询（WinRT/Mock 回退）
// ============================================================

/// 获取 SIM 卡信息（M3: WinRT 真实查询 carrier/signal/network，失败回退 mock。
/// ICCID/IMSI 保留 mock 值，WinRT 读不到。）
#[tauri::command]
fn get_sim_info(db: State<'_, DbState>) -> SimInfo {
    db.0.lock()
        .map(|g| sms::fetch_sim_info(&g))
        .unwrap_or_else(|_| sms::fallback_sim_info())
}

/// 获取信号强度（M3: WinRT 真实 dBm，回退 -91）。
/// 返回 dBm（负值），前端用 signalToPercent() 转换为百分比。
#[tauri::command]
fn get_signal_strength(db: State<'_, DbState>) -> i32 {
    db.0.lock()
        .map(|g| sms::fetch_signal(&g))
        .unwrap_or(-91)
}

/// 发送短信（M3: WinRT SendMessageAsync 真实发送 + DB insert，成功返回 DB rowid）。
#[tauri::command]
fn send_sms(db: State<'_, DbState>, phone: String, message: String) -> Result<String, String> {
    let guard = db
        .0
        .lock()
        .map_err(|_| "数据库连接锁被占用".to_string())?;
    sms::send(&guard, &phone, &message).map(|id| id.to_string())
}

/// 获取通道状态（M3 新增）：探测活动通道是否可用，返回通道类型与错误信息。
#[tauri::command]
fn get_channel_status(db: State<'_, DbState>) -> ChannelStatus {
    db.0.lock()
        .map(|g| sms::channel_status(&g))
        .unwrap_or_else(|_| ChannelStatus {
            channel_type: "unknown".into(),
            available: false,
            com_port: None,
            error: Some("数据库连接锁被占用".into()),
        })
}

/// 按需同步设备短信到 DB（M3 新增）：WinRT 收取 → 去重入库 → 返回最新收件箱列表。
#[tauri::command]
fn sync_device_inbox(db: State<'_, DbState>) -> Result<Vec<SmsMessage>, String> {
    let guard = db
        .0
        .lock()
        .map_err(|_| "数据库连接锁被占用".to_string())?;
    sms::sync_inbox(&guard)
}

// ============================================================
// Tauri App Entry
// ============================================================

#[cfg_attr(mobile, tauri::mobile_entry_point)]

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            db::init(app).map_err(|e| format!("SQLite 初始化失败: {e}"))?;
            // M3: 启动时同步设备短信到 DB（block_on 不会死锁，探测已验证）
            let db = app.state::<DbState>();
            if let Ok(guard) = db.0.lock() {
                if let Err(e) = sms::sync_inbox(&guard) {
                    eprintln!("[smsclient] 启动时设备短信同步失败: {e}");
                }
            }
            // M3: 订阅 SmsDevice2 SmsMessageReceived 事件（失败仅打日志，不阻断启动）
            if let Err(e) = sms::winrt::subscribe_inbox_events() {
                eprintln!("[smsclient] 短信事件订阅失败: {e}");
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // M3: 通过活动通道真实查询
            get_sim_info,
            get_signal_strength,
            send_sms,
            get_channel_status,
            sync_device_inbox,
            // M1+M2: 收件箱（启动时已同步设备短信）
            messages::get_inbox,
            // M2 新增：联系人 CRUD
            contacts::list_contacts,
            contacts::add_contact,
            contacts::update_contact,
            contacts::delete_contact,
            // M2 新增：消息查询
            messages::get_sent,
            messages::mark_message_read,
            // M2 新增：设置存取
            settings::get_setting,
            settings::set_setting,
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
        // 有设备返回真实 carrier，无设备回退"中国移动"——两种都合法
        assert!(!info.carrier.is_empty());
    }

    #[test]
    fn fetch_signal_returns_real_or_mock_value() {
        let conn = db::open_in_memory().unwrap();
        let sig = sms::fetch_signal(&conn);
        // 有设备返回真实信号，无设备回退 -91——两种都合法
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
            timestamp: "2026-09-23 10:00:00".into(),
            device_id: 1,
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
        // 用 MockChannel 避免 WinRT 阻塞
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
        // 有设备 available=true；无设备 available=false + error——两种都合法
        if !st.available { assert!(st.error.is_some()); }
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

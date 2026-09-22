//! SMSClient — Tauri 2 Backend (M2 SQLite 数据层 + 联系人 CRUD)
//!
//! M1 → M2 演进：
//! - 前四个 mock command 保留（get_sim_info / get_signal_strength / send_sms / get_inbox）
//! - send_sms 改为写入 SQLite messages 表（持久化验证 AC-7）
//! - get_inbox 改为读取 messages 表（direction='in'）
//! - 新增联系人 CRUD 命令：list_contacts / add_contact / update_contact / delete_contact
//! - 新增消息命令：get_sent / mark_message_read
//! - 新增设置命令：get_setting / set_setting
//! - 数据模型对齐 PRD §5（SmsMessage: phone/content/direction/status/timestamp as DATETIME）

pub mod contacts;
pub mod db;
pub mod messages;
pub mod models;
pub mod settings;
pub mod sms;

use tauri::State;

use crate::db::DbState;
use crate::models::{NewMessage, SimInfo};

// ============================================================
// Mock / Partial Real 命令（M1 保留，M2 DB 化）
// ============================================================

/// 获取 SIM 卡信息（Mock: PRD §2.2 实测数据，M3 将部分改为 WinRT 查询）。
#[tauri::command]
fn get_sim_info() -> SimInfo {
    SimInfo {
        carrier: "中国移动".to_string(),
        iccid: "89860012345678901234".to_string(),
        imsi: "460001234567890".to_string(),
        phone_number: None,
        signal_strength: 35, // PRD §2.1 实测百分比
        signal_dbm: -91,     // PRD §2.1 实测 dBm
        network_type: "4G/LTE".to_string(),
    }
}

/// 获取信号强度（Mock: PRD §2.1 实测值，M3 将改为 WinRT 查询）。
/// 返回 dBm（负值），前端用 signalToPercent() 转换为百分比。
#[tauri::command]
fn get_signal_strength() -> i32 {
    -91
}

/// 发送短信（M2: 持久化到 SQLite messages 表，status='sent' 模拟成功）。
/// M3 将替换为 WinRT SmsDevice2.SendMessageAsync 真实发送。
#[tauri::command]
fn send_sms(db: State<'_, DbState>, phone: String, message: String) -> Result<String, String> {
    let mut msg = NewMessage::outgoing(phone, message);
    // Mock: 发送成功 → 直接标记 sent
    msg.status = "sent".to_string();
    db.with(|c| messages::insert(c, &msg))
        .map_err(|e| format!("数据库错误: {e}"))
        .map(|id| id.to_string())
}

// ============================================================
// Tauri App Entry
// ============================================================

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            db::init(app).map_err(|e| format!("SQLite 初始化失败: {e}"))?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // M1 原有命令
            get_sim_info,
            get_signal_strength,
            send_sms,
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

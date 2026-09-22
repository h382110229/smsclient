//! SMSClient — Tauri 2 Backend (M1 Mock Commands)
//!
//! Worker A「脚手架」— 2026-09-22
//! 本模块提供四个 mock command，供前端 invoke 调通验证。
//! M2 将替换为 WinRT SmsDevice2 真实实现（Q7 决策：WinRT SMS API 先行，AT 通道预留 trait 接口）。

use serde::{Deserialize, Serialize};

// ============================================================
// Data Transfer Objects (DTO)
// ============================================================

/// SIM 卡信息
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimInfo {
    pub carrier: String,
    pub iccid: String,
    pub imsi: String,
    pub signal_strength: i32,
    pub network_type: String,
}

/// 短信消息
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SmsMessage {
    pub id: String,
    pub address: String,
    pub body: String,
    pub timestamp: i64,
    pub read: bool,
    pub status: String,
    pub msg_type: String,
    pub parts: Option<u32>,
}

/// 通道状态（WinRT / AT 预留）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelStatus {
    pub channel: String,
    pub ready: bool,
    pub error: Option<String>,
}

// ============================================================
// Mock Tauri Commands (M1 — 前端 invoke 调通验证)
// ============================================================

/// 获取 SIM 卡信息（Mock: 中国移动卡）
#[tauri::command]
fn get_sim_info() -> SimInfo {
    SimInfo {
        carrier: "中国移动".to_string(),
        iccid: "89860012345678901234".to_string(),
        imsi: "460001234567890".to_string(),
        signal_strength: -75,
        network_type: "4G/LTE".to_string(),
    }
}

/// 获取信号强度 (dBm)
#[tauri::command]
fn get_signal_strength() -> i32 {
    -75
}

/// 发送短信（Mock: 模拟成功返回消息 ID）
#[tauri::command]
fn send_sms(phone: String, message: String) -> Result<String, String> {
    if phone.trim().is_empty() {
        return Err("手机号码不能为空".to_string());
    }
    if message.trim().is_empty() {
        return Err("短信内容不能为空".to_string());
    }
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let msg_id = format!("mock-sent-{ts}");
    println!("[Mock send_sms] to={phone} body={message} -> id={msg_id}");
    Ok(msg_id)
}

/// 获取收件箱（Mock: 两条示例短信）
#[tauri::command]
fn get_inbox() -> Vec<SmsMessage> {
    vec![
        SmsMessage {
            id: "msg-001".to_string(),
            address: "+8613800138000".to_string(),
            body: "【验证码】您的验证码是 8848，请在 5 分钟内使用。".to_string(),
            timestamp: 1758525558,
            read: false,
            status: "received".to_string(),
            msg_type: "inbox".to_string(),
            parts: Some(1),
        },
        SmsMessage {
            id: "msg-002".to_string(),
            address: "+8613900139000".to_string(),
            body: "Hello from ASTeam! 这是一条中英混合的测试短信，用于验证长短信拼接功能。".to_string(),
            timestamp: 1758518400,
            read: true,
            status: "received".to_string(),
            msg_type: "inbox".to_string(),
            parts: Some(1),
        },
        SmsMessage {
            id: "msg-003".to_string(),
            address: "10086".to_string(),
            body: "【中国移动】尊敬的客户，您本月流量已使用 2.3GB，剩余 7.7GB。".to_string(),
            timestamp: 1758510000,
            read: true,
            status: "received".to_string(),
            msg_type: "inbox".to_string(),
            parts: Some(1),
        },
    ]
}

// ============================================================
// Tauri App Entry
// ============================================================

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            get_sim_info,
            get_signal_strength,
            send_sms,
            get_inbox,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

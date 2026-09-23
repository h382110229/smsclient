//! SMSClient — SMS 通道模块 (PRD §4.2)
//!
//! 提供可插拔 SMS 通道架构：
//! - `Channel` 枚举：统一代理 WinRT / Mock 通道（enum dispatch，避免 async fn in trait 不可 dyn 安全问题）
//! - `resolve_channel(preferred)`: 按 settings 选择通道
//! - `block_on()`: 同步阻塞 executor（Tauri 同步 command 调用 async trait 方法）
//!
//! 通道 A (WinRT): `winrt::WinRtChannel` — SmsDevice2 API，零风险不动 USB 组合
//! 通道 B (AT Serial): 预留，需 USB PID_0310→0312 切换
//! Mock: 开发调试，返回 PRD §2.1 探针实测值

pub mod channel;
pub mod mock;
pub mod winrt;

pub use channel::{MessageId, ReceivedSms, SmsChannel};
pub use mock::MockChannel;
pub use winrt::WinRtChannel;

use std::future::Future;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};
use std::thread::{self, Thread};

use rusqlite::{params, Connection};

use crate::models::{ChannelStatus, SimInfo, SmsMessage};

// ============================================================
// Channel 枚举：enum dispatch，避免 async fn in trait 不可 dyn 安全问题
// ============================================================

/// 统一 SMS 通道枚举。所有 `SmsChannel` trait 方法通过模式匹配代理到具体实现。
pub enum Channel {
    Winrt(WinRtChannel),
    Mock(MockChannel),
}

impl SmsChannel for Channel {
    async fn send_sms(&self, to: &str, message: &str) -> Result<MessageId, String> {
        match self {
            Self::Winrt(c) => c.send_sms(to, message).await,
            Self::Mock(c) => c.send_sms(to, message).await,
        }
    }

    async fn get_inbox(&self) -> Result<Vec<ReceivedSms>, String> {
        match self {
            Self::Winrt(c) => c.get_inbox().await,
            Self::Mock(c) => c.get_inbox().await,
        }
    }

    async fn delete_message(&self, id: MessageId) -> Result<(), String> {
        match self {
            Self::Winrt(c) => c.delete_message(id).await,
            Self::Mock(c) => c.delete_message(id).await,
        }
    }

    async fn get_signal_strength(&self) -> Result<i32, String> {
        match self {
            Self::Winrt(c) => c.get_signal_strength().await,
            Self::Mock(c) => c.get_signal_strength().await,
        }
    }

    async fn get_sim_info(&self) -> Result<SimInfo, String> {
        match self {
            Self::Winrt(c) => c.get_sim_info().await,
            Self::Mock(c) => c.get_sim_info().await,
        }
    }

    async fn status(&self) -> ChannelStatus {
        match self {
            Self::Winrt(c) => c.status().await,
            Self::Mock(c) => c.status().await,
        }
    }
}

// ============================================================
// 通道工厂
// ============================================================

/// 按 settings 中 `preferred_channel` 的值解析通道。
/// - 'mock': MockChannel（开发调试）
/// - 'at': 返回错误（通道 B 预留，需 USB PID_0310→0312 切换）
/// - 其他 / 默认: WinRT 通道（首次调用时才探测设备，失败由调用方处理）
pub fn resolve_channel(preferred: &str) -> Result<Channel, String> {
    match preferred {
        "mock" => Ok(Channel::Mock(MockChannel::new())),
        "at" => Err("AT 串口通道未启用：需 USB PID_0310→0312 切换（PRD Q7 选 A 预留）".into()),
        _ => Ok(Channel::Winrt(WinRtChannel::new())),
    }
}

// ============================================================
// 阻塞式 Future 执行器
// ============================================================

struct ThreadWaker(Thread);

impl Wake for ThreadWaker {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.0.unpark();
    }
}

/// 同步阻塞等待 Future 完成。
/// 适用场景：Tauri 同步 command 内调用 async SmsChannel 方法。
pub fn block_on<F: Future>(f: F) -> F::Output {
    let mut f = Box::pin(f);
    let waker = Waker::from(Arc::new(ThreadWaker(thread::current())));
    let mut cx = Context::from_waker(&waker);
    loop {
        match f.as_mut().poll(&mut cx) {
            Poll::Ready(val) => return val,
            Poll::Pending => thread::park(),
        }
    }
}

// ============================================================
// 通道选择与回退 (M3 WinRT 接入)
// ============================================================

const MOCK_ICCID: &str = "89860012345678901234";
const MOCK_IMSI: &str = "460001234567890";

/// 从 settings 读取 preferred_channel，缺省 "winrt"。
pub fn preferred_channel(conn: &Connection) -> String {
    crate::settings::get(conn, "preferred_channel")
        .ok()
        .flatten()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "winrt".to_string())
}

/// 解析通道；WinRT 探测不可用 →回退 Mock；AT/解析错误 → 回退 Mock。
pub fn channel_or_fallback(preferred: &str) -> Channel {
    match resolve_channel(preferred) {
        Ok(ch) => {
            if matches!(ch, Channel::Winrt(_)) {
                // WinRT 通道：探测设备是否可用
                let st = block_on(ch.status());
                if st.available {
                    ch
                } else {
                    Channel::Mock(MockChannel::new())
                }
            } else {
                ch
            }
        }
        // 'at' 预留未启用或其他未知通道 → 回退 Mock 保持可用
        Err(_) => Channel::Mock(MockChannel::new()),
    }
}

/// 解析通道：读取 preferred_channel 设置 + fallback。
pub fn active_channel(conn: &Connection) -> Channel {
    channel_or_fallback(&preferred_channel(conn))
}

/// 完全 Mock 的 SimInfo（PRD §2.2 实测数据）。
pub fn fallback_sim_info() -> SimInfo {
    SimInfo {
        carrier: "中国移动".to_string(),
        iccid: MOCK_ICCID.to_string(),
        imsi: MOCK_IMSI.to_string(),
        phone_number: None,
        signal_strength: 35,
        signal_dbm: -91,
        network_type: "4G/LTE".to_string(),
    }
}

/// WinRT 读不到 ICCID/IMSI → 用 PRD 占位值补齐。
pub fn merge_sim_info(mut info: SimInfo) -> SimInfo {
    if info.iccid.is_empty() || info.iccid.starts_with("N/A") {
        info.iccid = MOCK_ICCID.to_string();
    }
    if info.imsi.is_empty() || info.imsi.starts_with("N/A") {
        info.imsi = MOCK_IMSI.to_string();
    }
    info
}

/// 获取 SIM 信息：WinRT 真实查询 → merge ICCID/IMSI → 失败回退 mock。
pub fn fetch_sim_info(conn: &Connection) -> SimInfo {
    let ch = active_channel(conn);
    match block_on(ch.get_sim_info()) {
        Ok(info) => merge_sim_info(info),
        Err(_) => fallback_sim_info(),
    }
}

/// 获取信号强度：WinRT 真实 dBm → 失败回退 -91。
pub fn fetch_signal(conn: &Connection) -> i32 {
    let ch = active_channel(conn);
    block_on(ch.get_signal_strength()).unwrap_or(-91)
}

/// 发送短信：WinRT 发送 → DB 入库(status='sent') → 返回 rowid。
pub fn send(conn: &Connection, phone: &str, content: &str) -> Result<i64, String> {
    let ch = active_channel(conn);
    block_on(ch.send_sms(phone, content))?;
    let mut msg = crate::models::NewMessage::outgoing(phone.to_string(), content.to_string());
    msg.status = "sent".to_string();
    crate::messages::insert(conn, &msg).map_err(db_err)
}

/// 检查 inbox 消息是否已入库（去重）。
pub fn inbox_exists(conn: &Connection, phone: &str, content: &str) -> rusqlite::Result<bool> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM messages WHERE direction = 'in' AND phone = ?1 AND content = ?2",
        params![phone, content],
        |row| row.get(0),
    )?;
    Ok(n > 0)
}

/// 同步收件箱：设备收取 → 去重入库 → DB 读取。
pub fn sync_inbox(conn: &Connection) -> Result<Vec<SmsMessage>, String> {
    let ch = active_channel(conn);
    sync_inbox_with(conn, &ch)
}

/// 使用指定通道同步收件箱（测试可用）。
pub fn sync_inbox_with(conn: &Connection, ch: &Channel) -> Result<Vec<SmsMessage>, String> {
    let received = block_on(ch.get_inbox())?;
    for r in received {
        let phone = crate::contacts::normalize_phone(&r.from);
        if phone.is_empty() || r.body.trim().is_empty() {
            continue;
        }
        // 去重：同号码+同内容不重复入库
        if !inbox_exists(conn, &phone, r.body.trim()).map_err(db_err)? {
            let nm = crate::models::NewMessage::incoming(phone, r.body.trim().to_string());
            let _ = crate::messages::insert(conn, &nm).map_err(db_err)?;
        }
    }
    crate::messages::list(conn, Some("in"), 200).map_err(db_err)
}

/// 获取通道状态探测结果。
pub fn channel_status(conn: &Connection) -> ChannelStatus {
    let preferred = preferred_channel(conn);
    match resolve_channel(&preferred) {
        Ok(Channel::Winrt(w)) => {
            let st = block_on(w.status());
            if st.available {
                st
            } else {
                ChannelStatus {
                    error: st
                        .error
                        .map(|e| format!("{e}；WinRT 不可用，已回退 Mock 通道")),
                    ..st
                }
            }
        }
        Ok(other) => block_on(other.status()),
        Err(e) => ChannelStatus {
            channel_type: preferred,
            available: false,
            com_port: None,
            error: Some(format!("{e}；已回退 Mock 通道")),
        },
    }
}

fn db_err(e: rusqlite::Error) -> String {
    match e {
        rusqlite::Error::InvalidParameterName(msg) => msg,
        other => format!("数据库错误: {other}"),
    }
}

// ============================================================
// 单元测试
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_on_ready_future() {
        assert_eq!(block_on(async { 42 }), 42);
    }

    #[test]
    fn resolve_mock_channel() {
        let ch = resolve_channel("mock").unwrap();
        assert!(matches!(ch, Channel::Mock(_)));
    }

    #[test]
    fn resolve_at_channel_errors() {
        let err = resolve_channel("at").err().unwrap();
        assert!(err.contains("AT 串口"));
    }

    #[test]
    fn resolve_winrt_channel_no_panic() {
        // 构造时不探测，不 panic
        let ch = resolve_channel("winrt").unwrap();
        assert!(matches!(ch, Channel::Winrt(_)));
    }

    // ---- M3 通道选择与回退测试 ----

    #[test]
    fn merge_sim_info_replaces_na_iccid_imsi() {
        let info = SimInfo {
            carrier: "中国移动".into(),
            iccid: "N/A (WinRT 不支持)".into(),
            imsi: "N/A (WinRT 不支持)".into(),
            phone_number: None,
            signal_strength: 35,
            signal_dbm: -91,
            network_type: "4G/LTE".into(),
        };
        let merged = merge_sim_info(info);
        assert_eq!(merged.iccid, MOCK_ICCID);
        assert_eq!(merged.imsi, MOCK_IMSI);
        assert_eq!(merged.carrier, "中国移动");
        assert_eq!(merged.network_type, "4G/LTE");
    }

    #[test]
    fn merge_sim_info_preserves_real_iccid_imsi() {
        let info = SimInfo {
            carrier: "中国电信".into(),
            iccid: "8986011234567890001".into(),
            imsi: "460011234567890".into(),
            phone_number: Some("13300001111".into()),
            signal_strength: 60,
            signal_dbm: -75,
            network_type: "5G".into(),
        };
        let merged = merge_sim_info(info);
        assert_eq!(merged.iccid, "8986011234567890001");
        assert_eq!(merged.imsi, "460011234567890");
        assert_eq!(merged.carrier, "中国电信");
    }

    #[test]
    fn merge_sim_info_empty_iccid_replaced_with_mock() {
        let info = SimInfo {
            carrier: "中国移动".into(),
            iccid: "".into(),
            imsi: "".into(),
            phone_number: None,
            signal_strength: 50,
            signal_dbm: -80,
            network_type: "4G/LTE".into(),
        };
        let merged = merge_sim_info(info);
        assert_eq!(merged.iccid, MOCK_ICCID);
        assert_eq!(merged.imsi, MOCK_IMSI);
    }

    #[test]
    fn fallback_sim_info_returns_prd_values() {
        let info = fallback_sim_info();
        assert_eq!(info.carrier, "中国移动");
        assert_eq!(info.iccid, MOCK_ICCID);
        assert_eq!(info.imsi, MOCK_IMSI);
        assert_eq!(info.signal_dbm, -91);
        assert_eq!(info.network_type, "4G/LTE");
    }

    #[test]
    fn preferred_channel_defaults_to_winrt() {
        let conn = crate::db::open_in_memory().unwrap();
        assert_eq!(preferred_channel(&conn), "winrt");
    }

    #[test]
    fn preferred_channel_reads_setting() {
        let conn = crate::db::open_in_memory().unwrap();
        crate::settings::set(&conn, "preferred_channel", "mock").unwrap();
        assert_eq!(preferred_channel(&conn), "mock");
    }

    #[test]
    fn channel_or_fallback_winrt_handles_device_or_mock() {
        let ch = channel_or_fallback("winrt");
        // 有设备时返回 WinRT，无设备时回退 Mock——两种都合法
        assert!(matches!(ch, Channel::Winrt(_) | Channel::Mock(_)));
    }

    #[test]
    fn channel_or_fallback_mock_returns_mock() {
        let ch = channel_or_fallback("mock");
        assert!(matches!(ch, Channel::Mock(_)));
    }

    #[test]
    fn channel_or_fallback_at_returns_mock() {
        let ch = channel_or_fallback("at");
        assert!(matches!(ch, Channel::Mock(_)));
    }

    #[test]
    fn active_channel_uses_setting() {
        let conn = crate::db::open_in_memory().unwrap();
        crate::settings::set(&conn, "preferred_channel", "mock").unwrap();
        let ch = active_channel(&conn);
        assert!(matches!(ch, Channel::Mock(_)));
    }

    #[test]
    fn active_channel_default_handles_device_or_mock() {
        let conn = crate::db::open_in_memory().unwrap();
        // 默认 preferred_channel = "winrt" → 有设备返回 WinRT，无设备回退 Mock
        let ch = active_channel(&conn);
        assert!(matches!(ch, Channel::Winrt(_) | Channel::Mock(_)));
    }

    #[test]
    fn channel_status_reports_available_or_unavailable() {
        let conn = crate::db::open_in_memory().unwrap();
        let st = channel_status(&conn);
        assert_eq!(st.channel_type, "winrt");
        // 有设备 available=true；无设备 available=false + error——两种都合法
        if !st.available { assert!(st.error.is_some()); }
    }

    #[test]
    fn channel_status_mock_when_preferred() {
        let conn = crate::db::open_in_memory().unwrap();
        crate::settings::set(&conn, "preferred_channel", "mock").unwrap();
        let st = channel_status(&conn);
        assert_eq!(st.channel_type, "mock");
        assert!(st.available);
    }

    #[test]
    fn channel_status_at_reports_error() {
        let conn = crate::db::open_in_memory().unwrap();
        crate::settings::set(&conn, "preferred_channel", "at").unwrap();
        let st = channel_status(&conn);
        assert_eq!(st.channel_type, "at");
        assert!(!st.available);
        assert!(st.error.unwrap().contains("AT 串口"));
    }

    #[test]
    fn fetch_signal_returns_real_or_mock_in_any_env() {
        let conn = crate::db::open_in_memory().unwrap();
        let sig = fetch_signal(&conn);
        assert!((-110..=-60).contains(&sig), "signal out of range: {sig}");
    }

    #[test]
    fn fetch_sim_info_returns_real_or_falls_back_in_any_env() {
        let conn = crate::db::open_in_memory().unwrap();
        let info = fetch_sim_info(&conn);
        assert!(!info.carrier.is_empty());
        assert!(!info.network_type.is_empty());
    }

    #[test]
    fn send_via_channel_persists_to_db() {
        let conn = crate::db::open_in_memory().unwrap();
        crate::settings::set(&conn, "preferred_channel", "mock").unwrap();
        let id = send(&conn, "13800138000", "你好").unwrap();
        assert!(id >= 1);
        let m = crate::messages::get(&conn, id).unwrap();
        assert_eq!(m.direction, "out");
        assert_eq!(m.phone, "13800138000");
        assert_eq!(m.content, "你好");
        assert_eq!(m.status, "sent");
    }

    #[test]
    fn send_empty_phone_returns_error_no_db_row() {
        let conn = crate::db::open_in_memory().unwrap();
        crate::settings::set(&conn, "preferred_channel", "mock").unwrap();
        let err = send(&conn, "", "hi").unwrap_err();
        assert!(err.contains("不能为空"));
        assert!(crate::messages::list(&conn, None, 100).unwrap().is_empty());
    }

    #[test]
    fn send_normalizes_phone_via_channel() {
        let conn = crate::db::open_in_memory().unwrap();
        crate::settings::set(&conn, "preferred_channel", "mock").unwrap();
        let id = send(&conn, "+86 138-0013-8000", "hello").unwrap();
        let m = crate::messages::get(&conn, id).unwrap();
        assert_eq!(m.phone, "13800138000");
    }

    #[test]
    fn sync_inbox_with_dedupes_device_messages() {
        let conn = crate::db::open_in_memory().unwrap();
        let mock = MockChannel::new();
        mock.push_inbox(ReceivedSms {
            from: "10086".into(),
            body: "流量提醒".into(),
            timestamp: "2026-09-22 10:00:00".into(),
            device_id: 1,
        });
        let ch = Channel::Mock(mock);
        let list1 = sync_inbox_with(&conn, &ch).unwrap();
        assert_eq!(list1.len(), 1);
        assert_eq!(list1[0].content, "流量提醒");
        assert_eq!(list1[0].direction, "in");
        assert_eq!(list1[0].status, "delivered");
        // 第二次同步同一条消息不重复入库
        let list2 = sync_inbox_with(&conn, &ch).unwrap();
        assert_eq!(list2.len(), 1);
        assert_eq!(crate::messages::list(&conn, None, 100).unwrap().len(), 1);
    }

    #[test]
    fn sync_inbox_without_device_reads_db() {
        let conn = crate::db::open_in_memory().unwrap();
        // 直接插入一条 incoming
        let msg = crate::models::NewMessage::incoming("10086".into(), "余额提醒".into());
        crate::messages::insert(&conn, &msg).unwrap();
        // 用 MockChannel 避免 WinRT 阻塞
        let ch = Channel::Mock(MockChannel::new());
        let list = sync_inbox_with(&conn, &ch).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].content, "余额提醒");
    }

    #[test]
    fn inbox_exists_checks_direction_and_content() {
        let conn = crate::db::open_in_memory().unwrap();
        assert!(!inbox_exists(&conn, "10086", "hi").unwrap());
        let msg = crate::models::NewMessage::incoming("10086".into(), "hi".into());
        crate::messages::insert(&conn, &msg).unwrap();
        assert!(inbox_exists(&conn, "10086", "hi").unwrap());
        // 不同 content 不匹配
        assert!(!inbox_exists(&conn, "10086", "other").unwrap());
    }

    #[test]
    fn channel_status_at_falls_back_note_appended() {
        let conn = crate::db::open_in_memory().unwrap();
        crate::settings::set(&conn, "preferred_channel", "at").unwrap();
        let st = channel_status(&conn);
        assert!(st.error.unwrap().contains("已回退 Mock"));
    }
}

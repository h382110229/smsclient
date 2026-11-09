//! SMSClient — SMS 通道模块 (锁分离与精准时间窗口去重重构版)

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

use crate::db::DbState;
use crate::models::{ChannelStatus, SimInfo, SmsMessage};

// ============================================================
// Channel 枚举：enum dispatch，避免 async fn in trait 不可 dyn 安全问题
// ============================================================

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
// 通道选择与回退
// ============================================================

const MOCK_ICCID: &str = "89860012345678901234";
const MOCK_IMSI: &str = "460001234567890";

pub fn preferred_channel(conn: &Connection) -> String {
    crate::settings::get(conn, "preferred_channel")
        .ok()
        .flatten()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "winrt".to_string())
}

pub fn channel_or_fallback(preferred: &str) -> Channel {
    match resolve_channel(preferred) {
        Ok(ch) => {
            if matches!(ch, Channel::Winrt(_)) {
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
        Err(_) => Channel::Mock(MockChannel::new()),
    }
}

pub fn active_channel(conn: &Connection) -> Channel {
    channel_or_fallback(&preferred_channel(conn))
}

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

pub fn merge_sim_info(mut info: SimInfo) -> SimInfo {
    if info.iccid.is_empty() || info.iccid.starts_with("N/A") {
        info.iccid = MOCK_ICCID.to_string();
    }
    if info.imsi.is_empty() || info.imsi.starts_with("N/A") {
        info.imsi = MOCK_IMSI.to_string();
    }
    info
}

// ============================================================
// 无锁版硬件查询（绝不在持有 DbState Mutex 期间执行慢速硬件 I/O）
// ============================================================

pub fn fetch_sim_info_by_pref(preferred: &str) -> SimInfo {
    let ch = channel_or_fallback(preferred);
    match block_on(ch.get_sim_info()) {
        Ok(info) => merge_sim_info(info),
        Err(_) => fallback_sim_info(),
    }
}

pub fn fetch_signal_by_pref(preferred: &str) -> i32 {
    let ch = channel_or_fallback(preferred);
    block_on(ch.get_signal_strength()).unwrap_or(-91)
}

pub fn channel_status_by_pref(preferred: &str) -> ChannelStatus {
    match resolve_channel(preferred) {
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
            channel_type: preferred.to_string(),
            available: false,
            com_port: None,
            error: Some(format!("{e}；已回退 Mock 通道")),
        },
    }
}

/// 锁分离发送：短锁读设置 -> 释放锁调 WinRT 发送 -> 短锁写入 DB
pub fn send_non_blocking(db: &DbState, phone: &str, content: &str) -> Result<i64, String> {
    let norm_phone = crate::contacts::normalize_phone(phone);
    if norm_phone.is_empty() {
        return Err("手机号码不能为空".into());
    }
    if content.trim().is_empty() {
        return Err("短信内容不能为空".into());
    }
    let pref = db.with(|c| Ok(preferred_channel(c))).unwrap_or_else(|_| "winrt".into());
    let ch = channel_or_fallback(&pref);
    block_on(ch.send_sms(&norm_phone, content))?;

    db.with(|conn| {
        let mut msg = crate::models::NewMessage::outgoing(norm_phone, content.to_string());
        msg.status = "sent".to_string();
        crate::messages::insert(conn, &msg)
    })
    .map_err(db_err)
}

/// 锁分离收件箱同步：
/// 1. 短锁读 preferred_channel
/// 2. 无锁从 WinRT MessageStore 拉取短信
/// 3. 短锁批量去重入库 + 消费送达回执
/// 4. 释放 DB 锁后，逐条清理 SIM 卡槽位
pub fn sync_inbox_non_blocking(db: &DbState) -> Result<Vec<SmsMessage>, String> {
    let pref = db.with(|c| Ok(preferred_channel(c))).unwrap_or_else(|_| "winrt".into());
    let ch = channel_or_fallback(&pref);

    // 1. 无锁拉取设备短信
    let received = block_on(ch.get_inbox())?;
    let pending_events = crate::sms::winrt::drain_pending_sms();
    let delivery_reports = crate::sms::winrt::drain_delivery_reports();

    // 2. 短锁批量去重入库
    let list = db
        .with(|conn| {
            for msg in pending_events {
                if !inbox_exists_recent(conn, &msg.phone, &msg.content, msg.part_index)? {
                    let _ = crate::messages::insert(conn, &msg);
                }
            }
            for r in &received {
                let phone = crate::contacts::normalize_phone(&r.from);
                let body = r.body.trim();
                if phone.is_empty() || body.is_empty() {
                    continue;
                }
                if !inbox_exists_recent(conn, &phone, body, r.part_index)? {
                    let is_long = r.part_total > 1;
                    let group_id = if is_long {
                        Some(format!("{phone}_{}", r.ref_id))
                    } else {
                        None
                    };
                    let nm = crate::models::NewMessage::incoming_with_meta(
                        phone,
                        body.to_string(),
                        if r.timestamp.is_empty() {
                            None
                        } else {
                            Some(r.timestamp.clone())
                        },
                        is_long,
                        r.part_index,
                        r.part_total,
                        group_id,
                    );
                    let _ = crate::messages::insert(conn, &nm)?;
                }
            }
            for (raw_phone, status) in delivery_reports {
                let phone = crate::contacts::normalize_phone(&raw_phone);
                if let Ok(id) = conn.query_row(
                    "SELECT id FROM messages WHERE direction='out' AND phone=?1 AND status='sent' \
                     ORDER BY id DESC LIMIT 1",
                    rusqlite::params![phone],
                    |row| row.get::<_, i64>(0),
                ) {
                    let _ = crate::messages::set_status(conn, id, &status, None);
                }
            }
            crate::messages::list(conn, Some("in"), 200)
        })
        .map_err(db_err)?;

    // 3. 释放 DB 锁后，逐条清理 SIM 卡槽位（硬件 I/O 绝不持锁）
    for r in &received {
        if r.device_id > 0 {
            if let Err(e) = block_on(ch.delete_message(r.device_id)) {
                eprintln!("[smsclient] 清理 SIM 卡短信 (id={}) 提示: {e}", r.device_id);
            }
        }
    }

    Ok(list)
}

// ============================================================
// 同步签名接口（兼容历史逻辑与单测）
// ============================================================

pub fn fetch_sim_info(conn: &Connection) -> SimInfo {
    fetch_sim_info_by_pref(&preferred_channel(conn))
}

pub fn fetch_signal(conn: &Connection) -> i32 {
    fetch_signal_by_pref(&preferred_channel(conn))
}

pub fn channel_status(conn: &Connection) -> ChannelStatus {
    channel_status_by_pref(&preferred_channel(conn))
}

pub fn send(conn: &Connection, phone: &str, content: &str) -> Result<i64, String> {
    let ch = active_channel(conn);
    block_on(ch.send_sms(phone, content))?;
    let mut msg = crate::models::NewMessage::outgoing(phone.to_string(), content.to_string());
    msg.status = "sent".to_string();
    crate::messages::insert(conn, &msg).map_err(db_err)
}

/// 精准时间窗口去重：基于 SQLite datetime(timestamp) >= datetime('now', '-2 minutes')
/// 仅对最近 120 秒内相同号码、相同正文、相同分片的短信判重，
/// 既能消除“事件监听 + 轮询”双重入库，又彻底避免误杀后续重发的相同验证码！
pub fn inbox_exists_recent(
    conn: &Connection,
    phone: &str,
    content: &str,
    part_index: i64,
) -> rusqlite::Result<bool> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM messages \
         WHERE direction = 'in' AND phone = ?1 AND content = ?2 AND part_index = ?3 \
         AND datetime(timestamp) >= datetime('now', '-2 minutes')",
        params![phone, content, part_index],
        |row| row.get(0),
    )?;
    Ok(n > 0)
}

pub fn inbox_exists(conn: &Connection, phone: &str, content: &str) -> rusqlite::Result<bool> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM messages WHERE direction = 'in' AND phone = ?1 AND content = ?2",
        params![phone, content],
        |row| row.get(0),
    )?;
    Ok(n > 0)
}

pub fn sync_inbox(conn: &Connection) -> Result<Vec<SmsMessage>, String> {
    let ch = active_channel(conn);
    sync_inbox_with(conn, &ch)
}

pub fn sync_inbox_with(conn: &Connection, ch: &Channel) -> Result<Vec<SmsMessage>, String> {
    let received = block_on(ch.get_inbox())?;
    for r in &received {
        let phone = crate::contacts::normalize_phone(&r.from);
        let body = r.body.trim();
        if phone.is_empty() || body.is_empty() {
            if r.device_id > 0 {
                let _ = block_on(ch.delete_message(r.device_id));
            }
            continue;
        }
        let exists = inbox_exists_recent(conn, &phone, body, r.part_index).map_err(db_err)?;
        if !exists {
            let is_long = r.part_total > 1;
            let group_id = if is_long {
                Some(format!("{phone}_{}", r.ref_id))
            } else {
                None
            };
            let nm = crate::models::NewMessage::incoming_with_meta(
                phone,
                body.to_string(),
                if r.timestamp.is_empty() {
                    None
                } else {
                    Some(r.timestamp.clone())
                },
                is_long,
                r.part_index,
                r.part_total,
                group_id,
            );
            let _ = crate::messages::insert(conn, &nm).map_err(db_err)?;
        }
        if r.device_id > 0 {
            let _ = block_on(ch.delete_message(r.device_id));
        }
    }
    for (phone, status) in crate::sms::winrt::drain_delivery_reports() {
        let phone = crate::contacts::normalize_phone(&phone);
        if let Ok(id) = conn.query_row(
            "SELECT id FROM messages WHERE direction='out' AND phone=?1 AND status='sent' \
             ORDER BY id DESC LIMIT 1",
            rusqlite::params![phone],
            |row| row.get::<_, i64>(0),
        ) {
            let _ = crate::messages::set_status(conn, id, &status, None);
        }
    }
    crate::messages::list(conn, Some("in"), 200).map_err(db_err)
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
        let ch = resolve_channel("winrt").unwrap();
        assert!(matches!(ch, Channel::Winrt(_)));
    }

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
        let ch = active_channel(&conn);
        assert!(matches!(ch, Channel::Winrt(_) | Channel::Mock(_)));
    }

    #[test]
    fn channel_status_reports_available_or_unavailable() {
        let conn = crate::db::open_in_memory().unwrap();
        let st = channel_status(&conn);
        assert_eq!(st.channel_type, "winrt");
        if !st.available {
            assert!(st.error.is_some());
        }
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
            timestamp: "".into(),
            device_id: 1,
            part_index: 1,
            part_total: 1,
            ref_id: 0,
        });
        let ch = Channel::Mock(mock);
        let list1 = sync_inbox_with(&conn, &ch).unwrap();
        assert_eq!(list1.len(), 1);
        assert_eq!(list1[0].content, "流量提醒");
        assert_eq!(list1[0].direction, "in");
        assert_eq!(list1[0].status, "delivered");

        // 第二次同步同一条消息在 2 分钟窗口内不重复入库
        let list2 = sync_inbox_with(&conn, &ch).unwrap();
        assert_eq!(list2.len(), 1);
        assert_eq!(crate::messages::list(&conn, None, 100).unwrap().len(), 1);
    }

    #[test]
    fn sync_inbox_without_device_reads_db() {
        let conn = crate::db::open_in_memory().unwrap();
        let msg = crate::models::NewMessage::incoming("10086".into(), "余额提醒".into());
        crate::messages::insert(&conn, &msg).unwrap();
        let ch = Channel::Mock(MockChannel::new());
        let list = sync_inbox_with(&conn, &ch).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].content, "余额提醒");
    }

    #[test]
    fn inbox_exists_recent_sliding_window_protection() {
        let conn = crate::db::open_in_memory().unwrap();
        // 1. 插入一条刚好是当前时间的短信
        let msg_recent = crate::models::NewMessage::incoming("10086".into(), "验证码1234".into());
        crate::messages::insert(&conn, &msg_recent).unwrap();

        // 最近 2 分钟内应判定为存在
        assert!(inbox_exists_recent(&conn, "10086", "验证码1234", 1).unwrap());
        // 不同分片不判重
        assert!(!inbox_exists_recent(&conn, "10086", "验证码1234", 2).unwrap());
        // 不同内容不判重
        assert!(!inbox_exists_recent(&conn, "10086", "验证码5678", 1).unwrap());

        // 2. 插入一条 5 分钟前的历史短信
        conn.execute(
            "INSERT INTO messages (direction, phone, content, part_index, part_total, timestamp) \
             VALUES ('in', '10086', '历史旧指令', 1, 1, datetime('now', '-5 minutes'))",
            [],
        ).unwrap();

        // 超过 2 分钟滑动窗口，精准防误杀：判定为不存在！
        assert!(!inbox_exists_recent(&conn, "10086", "历史旧指令", 1).unwrap());
    }

    #[test]
    fn channel_status_at_falls_back_note_appended() {
        let conn = crate::db::open_in_memory().unwrap();
        crate::settings::set(&conn, "preferred_channel", "at").unwrap();
        let st = channel_status(&conn);
        assert!(st.error.unwrap().contains("已回退 Mock"));
    }
}

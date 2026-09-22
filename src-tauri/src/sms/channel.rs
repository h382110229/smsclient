//! SMSClient — SMS 通道抽象层 (PRD §4.2)
//!
//! 定义统一 trait，WinRT (通道 A) 和 AT Serial (通道 B, 预留) 均实现此接口。
//! 通道只负责与设备通信，持久化由 Tauri command 层完成。

use crate::models::{ChannelStatus, SimInfo};

/// 设备级消息 ID
pub type MessageId = u32;

/// 从设备读到的原始短信
#[derive(Debug, Clone)]
pub struct ReceivedSms {
    pub from: String,
    pub body: String,
    /// ISO-8601 UTC 字符串
    pub timestamp: String,
    /// 设备侧消息 ID（用于 delete_message）
    pub device_id: MessageId,
}

/// 通道 trait — 无论 WinRT 还是 AT，上层逻辑统一 (PRD §4.2)。
#[allow(async_fn_in_trait)]
pub trait SmsChannel: Send + Sync {
    /// 发送短信到指定号码，成功返回平台消息引用 ID。
    async fn send_sms(&self, to: &str, message: &str) -> Result<MessageId, String>;

    /// 读取设备上的全部收件（仅文本短信）。
    async fn get_inbox(&self) -> Result<Vec<ReceivedSms>, String>;

    /// 删除设备侧消息。
    async fn delete_message(&self, id: MessageId) -> Result<(), String>;

    /// 获取信号强度 (dBm, 负值)。
    async fn get_signal_strength(&self) -> Result<i32, String>;

    /// 获取 SIM / 网络信息。
    async fn get_sim_info(&self) -> Result<SimInfo, String>;

    /// 获取通道状态。
    async fn status(&self) -> ChannelStatus;
}

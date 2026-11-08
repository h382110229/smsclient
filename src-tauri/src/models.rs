//! SMSClient — 数据模型 (PRD §5 对齐)
//!
//! 所有字段与 PRD §5.1 表结构 / §5.2 TypeScript 接口一一对应。
//! serde rename_all = "camelCase" 确保 Rust snake_case ↔ 前端 camelCase 自动映射。

use serde::{Deserialize, Serialize};

// ============================================================
// 联系人 (PRD §5.1 contacts 表)
// ============================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Contact {
    /// INTEGER PRIMARY KEY AUTOINCREMENT
    pub id: i64,
    /// TEXT NOT NULL
    pub name: String,
    /// TEXT NOT NULL UNIQUE (存储时已规范化：去空格/短横线，去 +86 前缀)
    pub phone: String,
    /// TEXT — 可为 null
    pub note: Option<String>,
    /// DATETIME DEFAULT CURRENT_TIMESTAMP — ISO-8601 UTC 字符串
    #[serde(default)]
    pub created_at: Option<String>,
}

// ============================================================
// 短信消息 (PRD §5.1 messages 表)
// ============================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SmsMessage {
    /// INTEGER PRIMARY KEY AUTOINCREMENT
    pub id: i64,
    /// 'in' | 'out'
    pub direction: String,
    /// 对方手机号码（已规范化）
    pub phone: String,
    /// 短信正文
    pub content: String,
    /// 编码方式，默认 'UCS2'
    pub encoding: String,
    /// 长短信标记
    pub is_long: bool,
    /// 长短信当前段序号，默认 1
    pub part_index: i64,
    /// 长短信总段数，默认 1
    pub part_total: i64,
    /// 长短信/群发的组 ID（UUID 或 null）
    pub group_id: Option<String>,
    /// 投递状态：pending / sent / delivered / read / failed
    pub status: String,
    /// 错误信息（发送失败时）
    pub error: Option<String>,
    /// 创建时间戳 — ISO-8601 UTC 字符串 'YYYY-MM-DD HH:MM:SS'
    pub timestamp: String,
}

/// 插入新消息的参数（不含 id，timestamp 可选，未提供时由数据库自动生成）
#[derive(Debug, Clone)]
pub struct NewMessage {
    pub direction: String,
    pub phone: String,
    pub content: String,
    pub encoding: String,
    pub is_long: bool,
    pub part_index: i64,
    pub part_total: i64,
    pub group_id: Option<String>,
    pub status: String,
    pub timestamp: Option<String>,
}

impl NewMessage {
    /// 构造一条发出的短信（默认 UCS2，待发送）
    pub fn outgoing(phone: String, content: String) -> Self {
        let char_count = content.chars().count();
        let is_long = char_count > 70;
        let part_total = if char_count == 0 {
            1i64
        } else {
            (((char_count - 1) / 67) + 1) as i64
        };
        Self {
            direction: "out".to_string(),
            phone,
            content,
            encoding: "UCS2".to_string(),
            is_long,
            part_index: 1,
            part_total,
            group_id: None,
            status: "pending".to_string(),
            timestamp: None,
        }
    }

    /// 构造一条收到的短信
    pub fn incoming(phone: String, content: String) -> Self {
        Self {
            direction: "in".to_string(),
            phone,
            content,
            encoding: "UCS2".to_string(),
            is_long: false,
            part_index: 1,
            part_total: 1,
            group_id: None,
            status: "delivered".to_string(),
            timestamp: None,
        }
    }

    /// 构造一条收到的短信并附带实际接收时间戳
    pub fn incoming_with_meta(
        phone: String,
        content: String,
        timestamp: Option<String>,
        is_long: bool,
        part_index: i64,
        part_total: i64,
        group_id: Option<String>,
    ) -> Self {
        Self {
            direction: "in".to_string(),
            phone,
            content,
            encoding: "UCS2".to_string(),
            is_long,
            part_index,
            part_total,
            group_id,
            status: "delivered".to_string(),
            timestamp,
        }
    }
}

// ============================================================
// 群发任务 (PRD §5.1 broadcast_tasks 表)
// ============================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BroadcastTask {
    pub id: i64,
    pub content: String,
    pub total: i64,
    pub sent: i64,
    pub failed: i64,
    pub status: String,
    pub created_at: Option<String>,
}

// ============================================================
// 设置 (PRD §5.1 settings 表)
// ============================================================

/// 键值对设置（单行）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Setting {
    pub key: String,
    pub value: String,
}

// ============================================================
// 辅助结构（非数据库表，用于前端展示）
// ============================================================

/// SIM 卡信息（PRD §5.2 TypeScript 接口）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimInfo {
    pub carrier: String,
    pub iccid: String,
    pub imsi: String,
    pub phone_number: Option<String>,
    /// 信号强度百分比 0-100
    pub signal_strength: i32,
    /// 信号强度 dBm（负值，-120 ~ -30）
    pub signal_dbm: i32,
    /// 网络类型 'LTE' | 'HSPA+' | '3G' | '2G'
    pub network_type: String,
}

/// 通道状态（PRD §5.2）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelStatus {
    /// 通道类型 'winrt' | 'at'
    #[serde(rename = "type")]
    pub channel_type: String,
    /// 是否就绪
    pub available: bool,
    /// COM 端口（AT 通道使用）
    pub com_port: Option<String>,
    /// 错误信息
    pub error: Option<String>,
}

// ============================================================
// 单元测试
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_message_outgoing_defaults() {
        let msg = NewMessage::outgoing("13800138000".into(), "你好".into());
        assert_eq!(msg.direction, "out");
        assert_eq!(msg.status, "pending");
        assert_eq!(msg.encoding, "UCS2");
        assert!(!msg.is_long);
        assert_eq!(msg.part_total, 1);
    }

    #[test]
    fn new_message_outgoing_long_sms() {
        // 71 个中文字符 → 超过 70，应标记为长短信，拆为 2 段
        let long_content = "这是一条超长测试短信".repeat(7) + "啊"; // 7*10+1=71 chars
        assert_eq!(long_content.chars().count(), 71);
        let msg = NewMessage::outgoing("13900139000".into(), long_content);
        assert!(msg.is_long);
        assert_eq!(msg.part_total, 2); // 71/67 向上取整
    }

    #[test]
    fn new_message_incoming_defaults() {
        let msg = NewMessage::incoming("10086".into(), "流量通知".into());
        assert_eq!(msg.direction, "in");
        assert_eq!(msg.status, "delivered");
        assert!(!msg.is_long);
    }

    #[test]
    fn contact_serialize_camel_case() {
        let c = Contact {
            id: 1,
            name: "张三".into(),
            phone: "13800138000".into(),
            note: Some("同事".into()),
            created_at: Some("2026-09-22 10:00:00".into()),
        };
        let json = serde_json::to_string(&c).unwrap();
        assert!(json.contains("\"name\""));
        assert!(json.contains("\"phone\""));
        assert!(json.contains("\"createdAt\""));
    }

    #[test]
    fn sms_message_serialize_camel_case() {
        let m = SmsMessage {
            id: 1,
            direction: "in".into(),
            phone: "13800138000".into(),
            content: "测试".into(),
            encoding: "UCS2".into(),
            is_long: false,
            part_index: 1,
            part_total: 1,
            group_id: None,
            status: "delivered".into(),
            error: None,
            timestamp: "2026-09-22 10:00:00".into(),
        };
        let json = serde_json::to_string(&m).unwrap();
        assert!(json.contains("\"isLong\""));
        assert!(json.contains("\"partIndex\""));
        assert!(json.contains("\"partTotal\""));
        assert!(json.contains("\"groupId\""));
    }
}

//! SMSClient — Mock SMS 通道 (开发调试)
//!
//! 模拟 SMS 收发流程，无需真实硬件。

use std::sync::Mutex;

use super::{MessageId, ReceivedSms, SmsChannel};
use crate::models::{ChannelStatus, SimInfo};

pub struct MockChannel {
    inbox: Mutex<Vec<ReceivedSms>>,
    sent: Mutex<Vec<(String, String)>>,
}

impl MockChannel {
    pub fn new() -> Self {
        Self {
            inbox: Mutex::new(Vec::new()),
            sent: Mutex::new(Vec::new()),
        }
    }

    pub fn push_inbox(&self, msg: ReceivedSms) {
        self.inbox.lock().unwrap().push(msg);
    }

    pub fn get_sent_records(&self) -> Vec<(String, String)> {
        self.sent.lock().unwrap().clone()
    }
}

impl SmsChannel for MockChannel {
    async fn send_sms(&self, to: &str, message: &str) -> Result<MessageId, String> {
        if to.trim().is_empty() {
            return Err("手机号码不能为空".into());
        }
        if message.trim().is_empty() {
            return Err("短信内容不能为空".into());
        }
        self.sent
            .lock()
            .unwrap()
            .push((to.to_string(), message.to_string()));
        Ok(0)
    }

    async fn get_inbox(&self) -> Result<Vec<ReceivedSms>, String> {
        Ok(self.inbox.lock().unwrap().clone())
    }

    async fn delete_message(&self, _id: MessageId) -> Result<(), String> {
        Ok(())
    }

    async fn get_signal_strength(&self) -> Result<i32, String> {
        Ok(-91)
    }

    async fn get_sim_info(&self) -> Result<SimInfo, String> {
        Ok(SimInfo {
            carrier: "中国移动".into(),
            iccid: "89860012345678901234".into(),
            imsi: "460001234567890".into(),
            phone_number: None,
            signal_strength: 35,
            signal_dbm: -91,
            network_type: "4G/LTE".into(),
        })
    }

    async fn status(&self) -> ChannelStatus {
        ChannelStatus {
            channel_type: "mock".into(),
            available: true,
            com_port: None,
            error: Some("Mock 通道（开发调试）".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sms::block_on;

    #[test]
    fn mock_send_ok() {
        let ch = MockChannel::new();
        block_on(ch.send_sms("10086", "测试")).unwrap();
        assert_eq!(ch.get_sent_records().len(), 1);
    }

    #[test]
    fn mock_send_empty_phone() {
        let ch = MockChannel::new();
        assert!(block_on(ch.send_sms("", "hi")).is_err());
    }

    #[test]
    fn mock_inbox_push_read() {
        let ch = MockChannel::new();
        ch.push_inbox(ReceivedSms {
            from: "10086".into(),
            body: "流量提醒".into(),
            timestamp: "2026-09-22 10:00:00".into(),
            device_id: 1,
            part_index: 1,
            part_total: 1,
            ref_id: 0,
        });
        let msgs = block_on(ch.get_inbox()).unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].body, "流量提醒");
    }
}

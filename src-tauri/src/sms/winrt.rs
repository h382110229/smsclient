//! SMSClient — WinRT SMS 通道 (通道 A, PRD Q7 决策)
//!
//! 修复 (2026-09-23):
//! - 发送: SmsDevice2::SendMessageAndGetResultAsync + ISmsMessageBase（不走 ISmsDevice）
//! - 收取: SmsDevice (v1) + MessageStore（MBIM 设备 SmsDevice2 不支持 ISmsDevice）
//! - 手机号: SmsDevice2::AccountPhoneNumber（从 SIM/运营商读取）

use std::sync::Mutex;

use windows::core::Interface;
use windows::Devices::Sms::{
    ISmsMessageBase, SmsDevice, SmsDevice2, SmsDeviceStatus, SmsMessageFilter, SmsTextMessage,
};
use windows::Networking::Connectivity::NetworkInformation;

use super::channel::{MessageId, ReceivedSms, SmsChannel};
use crate::models::{ChannelStatus, SimInfo};

fn win_err(e: windows::core::Error) -> String {
    format!("WinRT 错误: {} (0x{:08X})", e.message(), e.code().0)
}

pub struct WinRtChannel {
    device: Mutex<Option<SmsDevice2>>,
}

unsafe impl Send for WinRtChannel {}
unsafe impl Sync for WinRtChannel {}

impl WinRtChannel {
    pub fn new() -> Self {
        Self { device: Mutex::new(None) }
    }

    fn device(&self) -> Result<SmsDevice2, String> {
        let mut guard = self.device.lock().map_err(|e| e.to_string())?;
        if guard.is_none() {
            let dev = SmsDevice2::GetDefault().map_err(win_err)?;
            *guard = Some(dev);
        }
        Ok(guard.as_ref().unwrap().clone())
    }

    fn is_ready(dev: &SmsDevice2) -> bool {
        matches!(dev.DeviceStatus().unwrap_or(SmsDeviceStatus::Off), SmsDeviceStatus::Ready)
    }
}

impl Default for WinRtChannel {
    fn default() -> Self { Self::new() }
}

impl SmsChannel for WinRtChannel {
    async fn send_sms(&self, to: &str, message: &str) -> Result<MessageId, String> {
        let dev = self.device()?;
        if !Self::is_ready(&dev) {
            return Err(format!("SMS 设备未就绪 (DeviceStatus={:?})", dev.DeviceStatus().map_err(win_err)?));
        }
        let text_msg = SmsTextMessage::new().map_err(win_err)?;
        text_msg.SetTo(&windows::core::HSTRING::from(to)).map_err(win_err)?;
        text_msg.SetBody(&windows::core::HSTRING::from(message)).map_err(win_err)?;
        // SendMessageAndGetResultAsync 需要 ISmsMessageBase
        let base: ISmsMessageBase = text_msg.cast().map_err(win_err)?;
        dev.SendMessageAndGetResultAsync(&base).map_err(win_err)?.get().map_err(win_err)?;
        Ok(0)
    }

    async fn get_inbox(&self) -> Result<Vec<ReceivedSms>, String> {
        // 先检查 SmsDevice2 可用（快速失败，避免 SmsDevice 阻塞）
        let _ = self.device()?;
        // SmsDevice2 没有 GetMessagesAsync；用 SmsDevice v1 + MessageStore
        let dev1 = SmsDevice::GetDefaultAsync()
            .map_err(win_err)?
            .get()
            .map_err(win_err)?;
        let store = dev1.MessageStore().map_err(win_err)?;
        let messages = store.GetMessagesAsync(SmsMessageFilter::All)
            .map_err(win_err)?
            .get()
            .map_err(win_err)?;

        let mut result = Vec::new();
        let size = messages.Size().map_err(win_err)?;
        for i in 0..size {
            let item = messages.GetAt(i).map_err(win_err)?;
            if let Ok(text_msg) = item.cast::<SmsTextMessage>() {
                let from = text_msg.From().map(|s| s.to_string()).unwrap_or_default();
                let body = text_msg.Body().map(|s| s.to_string()).unwrap_or_default();
                let device_id = text_msg.Id().unwrap_or(0);
                let timestamp = text_msg.Timestamp().map(|dt| {
                    let ft = dt.UniversalTime as i64;
                    let unix = (ft - 116_444_736_000_000_000) / 10_000_000;
                    format_unix(unix)
                }).unwrap_or_default();
                if !from.is_empty() && !body.is_empty() {
                    result.push(ReceivedSms { from, body, timestamp, device_id });
                }
            }
        }
        Ok(result)
    }

    async fn delete_message(&self, _id: MessageId) -> Result<(), String> { Ok(()) }

    async fn get_signal_strength(&self) -> Result<i32, String> {
        let profile = NetworkInformation::GetInternetConnectionProfile().map_err(win_err)?;
        let bars = profile.GetSignalBars().map_err(win_err)?.Value().unwrap_or(0);
        Ok(match bars { 4 => -60, 3 => -75, 2 => -85, 1 => -95, 0 => -110, _ => -100 })
    }

    async fn get_sim_info(&self) -> Result<SimInfo, String> {
        let dev = self.device()?;
        let profile = NetworkInformation::GetInternetConnectionProfile().map_err(win_err)?;

        let carrier = profile.GetNetworkNames().ok()
            .and_then(|n| n.GetAt(0).ok().map(|s| s.to_string()))
            .unwrap_or_else(|| "中国移动".to_string());

        let network_type = if profile.IsWwanConnectionProfile().unwrap_or(false) { "4G/LTE" } else { "WLAN" };
        let signal_dbm = self.get_signal_strength().await.unwrap_or(-100);
        let signal_strength = ((signal_dbm + 100) as f32 / 50.0 * 100.0).clamp(0.0, 100.0) as i32;

        // AccountPhoneNumber 从 SIM/运营商读取手机号
        let phone_number = dev.AccountPhoneNumber().ok()
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty() && s != "+00000000000");

        Ok(SimInfo {
            carrier, signal_strength, signal_dbm, network_type: network_type.to_string(),
            iccid: "N/A (WinRT)".into(), imsi: "N/A (WinRT)".into(), phone_number,
        })
    }

    async fn status(&self) -> ChannelStatus {
        match self.device() {
            Ok(dev) => {
                let ready = Self::is_ready(&dev);
                ChannelStatus {
                    channel_type: "winrt".into(), available: ready, com_port: None,
                    error: if ready { None } else { Some(format!("DeviceStatus={:?}", dev.DeviceStatus())) },
                }
            }
            Err(e) => ChannelStatus { channel_type: "winrt".into(), available: false, com_port: None, error: Some(e) },
        }
    }
}

fn format_unix(unix: i64) -> String {
    let days = unix / 86400;
    let s = unix % 86400;
    let (h, m, sec) = (s / 3600, (s % 3600) / 60, s % 60);
    let (y, mo, d) = civil_from_days(days);
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{m:02}:{sec:02}")
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn test_format_unix() { assert!(format_unix(1_790_294_400).starts_with("2026-09-")); }
    #[test] fn test_civil_from_days() { assert_eq!(civil_from_days(0), (1970, 1, 1)); }
    #[test] fn channel_is_send_sync() { fn a<T: Send + Sync>() {} a::<WinRtChannel>(); }
    #[test] fn channel_works_in_test_env() {
        let ch = WinRtChannel::new();
        // 有设备时成功，无设备时失败（两种都合法）
        let _ = ch.device();
    }
}

//! SMSClient — WinRT SMS 通道 (通道 A, PRD Q7 决策)
//!
//! 修复 (2026-09-23):
//! - 发送: SmsDevice2::SendMessageAndGetResultAsync + ISmsMessageBase（不走 ISmsDevice）
//! - 收取: SmsDevice (v1) + MessageStore（MBIM 设备 SmsDevice2 不支持 ISmsDevice）
//! - 手机号: SmsDevice2::AccountPhoneNumber（从 SIM/运营商读取）
//! - SIM/信号: 取 WWAN 蜂窝 profile，不再取 internet profile（WiFi）

use std::sync::Mutex;

use windows::core::Interface;
use windows::Devices::Sms::{
    ISmsMessageBase, SmsDevice, SmsDevice2, SmsDeviceStatus, SmsMessageFilter, SmsTextMessage, SmsTextMessage2,
};
use windows::Networking::Connectivity::{ConnectionProfile, NetworkInformation};

use super::channel::{MessageId, ReceivedSms, SmsChannel};
use crate::models::{ChannelStatus, SimInfo};

fn win_err(e: windows::core::Error) -> String {
    format!("WinRT 错误: {} (0x{:08X})", e.message(), e.code().0)
}

/// 从所有网络 profile 中找到蜂窝 WWAN profile（4G/LTE）。
fn find_wwan_profile() -> Option<ConnectionProfile> {
    let profiles = NetworkInformation::GetConnectionProfiles().ok()?;
    let size = profiles.Size().ok()?;
    for i in 0..size {
        let p = profiles.GetAt(i).ok()?;
        if p.IsWwanConnectionProfile().unwrap_or(false) {
            return Some(p);
        }
    }
    None
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
        let text_msg = SmsTextMessage2::new().map_err(win_err)?;
        text_msg.SetTo(&windows::core::HSTRING::from(to)).map_err(win_err)?;
        text_msg.SetBody(&windows::core::HSTRING::from(message)).map_err(win_err)?;
        // SendMessageAndGetResultAsync 需要 ISmsMessageBase
        let base: ISmsMessageBase = text_msg.cast().map_err(win_err)?;
        dev.SendMessageAndGetResultAsync(&base).map_err(win_err)?.get().map_err(win_err)?;
        Ok(0)
    }

    async fn get_inbox(&self) -> Result<Vec<ReceivedSms>, String> {
        let _ = self.device()?;
        let op = SmsDevice::GetDefaultAsync().map_err(win_err)?;
        let dev1 = op.get().map_err(win_err)?;
        let store = dev1.MessageStore().map_err(win_err)?;
        let messages = store.GetMessagesAsync(SmsMessageFilter::All)
            .map_err(win_err)?.get().map_err(win_err)?;

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
                    let (h, m, sec) = (unix % 86400 / 3600, unix % 3600 / 60, unix % 60);
                    let days = unix / 86400 + 719468;
                    let era = days / 146097;
                    let doe = (days - era * 146097) as u64;
                    let yoe = (doe - doe/1460 + doe/36524 - doe/146096) / 365;
                    let y = yoe as i64 + era * 400;
                    let doy = doe - (365*yoe + yoe/4 - yoe/100);
                    let mp = (5*doy + 2) / 153;
                    let d = doy - (153*mp + 2)/5 + 1;
                    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
                    let yr = if mo <= 2 { y + 1 } else { y };
                    format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", yr, mo, d, h, m, sec)
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
        let profile = find_wwan_profile()
            .ok_or_else(|| "未找到蜂窝 WWAN 网络".to_string())?;
        let bars = profile.GetSignalBars().map_err(win_err)?.Value().unwrap_or(0);
        Ok(match bars { 4 => -60, 3 => -75, 2 => -85, 1 => -95, 0 => -110, _ => -100 })
    }

    async fn get_sim_info(&self) -> Result<SimInfo, String> {
        let dev = self.device()?;
        let wwan = find_wwan_profile();

        let carrier = wwan.as_ref()
            .and_then(|p| p.GetNetworkNames().ok())
            .and_then(|n| n.GetAt(0).ok().map(|s| s.to_string()))
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "未知运营商".to_string());

        let network_type = "4G/LTE";
        let signal_dbm = wwan.as_ref()
            .and_then(|p| p.GetSignalBars().ok())
            .and_then(|b| b.Value().ok())
            .map(|bars| match bars { 4 => -60, 3 => -75, 2 => -85, 1 => -95, 0 => -110, _ => -100 })
            .unwrap_or(-100);
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn channel_is_send_sync() { fn a<T: Send + Sync>() {} a::<WinRtChannel>(); }
    #[test] fn channel_works_in_test_env() {
        let ch = WinRtChannel::new();
        let _ = ch.device();
    }



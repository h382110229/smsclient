//! SMSClient — WinRT SMS 通道 (通道 A, PRD Q7 决策)
//!
//! 通过 `windows` crate 调用 `Windows.Devices.Sms.SmsDevice2`，
//! 不需要 COM 口，不影响 4G 上网。
//!
//! 已知限制 (M3):
//! - ICCID/IMSI 不在 WinRT SMS API 暴露范围内，返回占位符（AT 通道可读取）
//! - GetSignalBars 返回 0-4 枚举，映射为 dBm 近似值
//! - 短信删除仅标记 DB，WinRT 侧删除在 M4+ 实现

use std::sync::Mutex;

use windows::core::Interface;
use windows::Devices::Sms::{
    ISmsDevice, SmsDevice2, SmsDeviceStatus, SmsMessageFilter, SmsTextMessage,
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

// Safety: SmsDevice2 是 COM 对象，WinRT MTA 模式下线程安全。
// Mutex 保证 Rust 侧互斥访问。这是 windows-rs 项目标准做法。
unsafe impl Send for WinRtChannel {}
unsafe impl Sync for WinRtChannel {}

impl WinRtChannel {
    pub fn new() -> Self {
        Self {
            device: Mutex::new(None),
        }
    }

    /// 获取（或缓存）SmsDevice2 默认实例，并 cast 到 ISmsDevice 以调用父接口方法。
    pub(crate) fn isms_device(&self) -> Result<(SmsDevice2, ISmsDevice), String> {
        let mut guard = self.device.lock().map_err(|e| e.to_string())?;
        if guard.is_none() {
            let dev = SmsDevice2::GetDefault().map_err(win_err)?;
            *guard = Some(dev);
        }
        let dev = guard.as_ref().unwrap().clone();
        let isms = dev.cast::<ISmsDevice>().map_err(win_err)?;
        Ok((dev, isms))
    }

    fn is_ready(dev: &SmsDevice2) -> bool {
        matches!(
            dev.DeviceStatus().unwrap_or(SmsDeviceStatus::Off),
            SmsDeviceStatus::Ready
        )
    }
}

impl Default for WinRtChannel {
    fn default() -> Self {
        Self::new()
    }
}

impl SmsChannel for WinRtChannel {
    async fn send_sms(&self, to: &str, message: &str) -> Result<MessageId, String> {
        let (dev, isms) = self.isms_device()?;
        if !Self::is_ready(&dev) {
            let status = dev.DeviceStatus().map_err(win_err)?;
            return Err(format!("SMS 设备未就绪 (DeviceStatus={status:?})"));
        }

        let text_msg = SmsTextMessage::new().map_err(win_err)?;
        text_msg
            .SetTo(&windows::core::HSTRING::from(to))
            .map_err(win_err)?;
        text_msg
            .SetBody(&windows::core::HSTRING::from(message))
            .map_err(win_err)?;

        // SendMessageAsync → IAsyncAction → .get() 阻塞等待
        isms.SendMessageAsync(&text_msg)
            .map_err(win_err)?
            .get()
            .map_err(win_err)?;
        Ok(0)
    }

    async fn get_inbox(&self) -> Result<Vec<ReceivedSms>, String> {
        let (dev, isms) = self.isms_device()?;
        if !Self::is_ready(&dev) {
            return Err("SMS 设备未就绪".into());
        }

        let store = isms.MessageStore().map_err(win_err)?;
        let messages = store
            .GetMessagesAsync(SmsMessageFilter::All)
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
                let timestamp = text_msg
                    .Timestamp()
                    .map(|dt| {
                        let ft = dt.UniversalTime as i64;
                        let unix = (ft - 116_444_736_000_000_000) / 10_000_000;
                        format_unix(unix)
                    })
                    .unwrap_or_default();
                if !from.is_empty() && !body.is_empty() {
                    result.push(ReceivedSms {
                        from,
                        body,
                        timestamp,
                        device_id,
                    });
                }
            }
        }
        Ok(result)
    }

    async fn delete_message(&self, _id: MessageId) -> Result<(), String> {
        // WinRT v1 API 不直接暴露 delete；M4+ 通过 MessageStore.DeleteMessage 实现
        Ok(())
    }

    async fn get_signal_strength(&self) -> Result<i32, String> {
        let profile = NetworkInformation::GetInternetConnectionProfile().map_err(win_err)?;
        let bars_ref = profile.GetSignalBars().map_err(win_err)?;
        // IReference<u8> → Value() → u8
        let bars = bars_ref.Value().unwrap_or(0);
        let dbm = match bars {
            4 => -60,
            3 => -75,
            2 => -85,
            1 => -95,
            0 => -110,
            _ => -100,
        };
        Ok(dbm)
    }

    async fn get_sim_info(&self) -> Result<SimInfo, String> {
        let profile = NetworkInformation::GetInternetConnectionProfile().map_err(win_err)?;

        let carrier = profile
            .GetNetworkNames()
            .ok()
            .and_then(|names| names.GetAt(0).ok().map(|s| s.to_string()))
            .unwrap_or_else(|| "中国移动".to_string());

        let network_type = if profile.IsWwanConnectionProfile().unwrap_or(false) {
            "4G/LTE".to_string()
        } else {
            "WLAN".to_string()
        };

        let signal_dbm = self.get_signal_strength().await.unwrap_or(-100);
        let signal_strength = ((signal_dbm + 100) as f32 / 50.0 * 100.0).clamp(0.0, 100.0) as i32;

        Ok(SimInfo {
            carrier,
            iccid: "N/A (WinRT 不支持)".to_string(),
            imsi: "N/A (WinRT 不支持)".to_string(),
            phone_number: None,
            signal_strength,
            signal_dbm,
            network_type,
        })
    }

    async fn status(&self) -> ChannelStatus {
        match self.isms_device() {
            Ok((dev, _)) => {
                let ready = Self::is_ready(&dev);
                let status = dev.DeviceStatus();
                ChannelStatus {
                    channel_type: "winrt".to_string(),
                    available: ready,
                    com_port: None,
                    error: if ready {
                        None
                    } else {
                        Some(format!("DeviceStatus={status:?}"))
                    },
                }
            }
            Err(e) => ChannelStatus {
                channel_type: "winrt".to_string(),
                available: false,
                com_port: None,
                error: Some(e),
            },
        }
    }
}

/// Unix 秒 → ISO-8601 UTC 字符串 'YYYY-MM-DD HH:MM:SS'
fn format_unix(unix: i64) -> String {
    let days = unix / 86400;
    let secs_of_day = unix % 86400;
    let (h, m, s) = (
        secs_of_day / 3600,
        (secs_of_day % 3600) / 60,
        secs_of_day % 60,
    );
    let (y, mo, d) = civil_from_days(days);
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{m:02}:{s:02}")
}

/// days since 1970-01-01 → (year, month, day) — Howard Hinnant 算法
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

    #[test]
    fn test_format_unix() {
        // 2026-09-22 00:00:00 UTC ≈ 1790294400
        let s = format_unix(1_790_294_400);
        assert!(s.starts_with("2026-09-"), "got {s}");
    }

    #[test]
    fn test_civil_from_days() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
    }

    #[test]
    fn channel_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<WinRtChannel>();
    }

    #[test]
    fn channel_no_device_in_test_env() {
        let ch = WinRtChannel::new();
        assert!(ch.isms_device().is_err());
    }
}

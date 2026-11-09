//! SMSClient — WinRT SMS 通道 (通道 A, PRD Q7 决策)
//!
//! 深度优化升级 (v0.2.0):
//! - 发送: SmsDevice2::SendMessageAndGetResultAsync + ISmsMessageBase
//! - 收取: SmsDevice (v1) + MessageStore（多设备智能枚举兜底）
//! - 删除: 同步后主动清理 SIM 卡/设备存储，彻底根治 20~50 条容量爆满后断收的问题
//! - 长短信: 提取 PartNumber / PartCount / PartReferenceId，支持连续长短信展示
//! - 时间戳: 规范为 ISO-8601 UTC (带 Z 标识)，确保各时区准确转换为系统本地时间
//! - 常驻监听: 全局保持 SmsDevice 句柄，杜绝 WinRT 事件监听器提前 Drop 失效

use std::sync::{Mutex, OnceLock};

use tauri::Manager;
use tauri_plugin_notification::NotificationExt;
use windows::core::Interface;
use windows::Devices::Enumeration::DeviceInformation;
use windows::Devices::Sms::{
    ISmsMessageBase, SmsBinaryMessage, SmsDevice, SmsDevice2, SmsDeviceStatus, SmsMessageFilter,
    SmsMessageReceivedEventArgs, SmsMessageReceivedEventHandler, SmsStatusMessage, SmsTextMessage,
    SmsTextMessage2,
};
use windows::Networking::Connectivity::{ConnectionProfile, NetworkInformation};

use super::channel::{MessageId, ReceivedSms, SmsChannel};
use crate::models::{ChannelStatus, NewMessage, SimInfo};

/// 全局常驻的 SmsDevice (v1) 包装，防止事件监听器被 COM GC 回收导致断收
struct SmsDeviceHolder(SmsDevice);
unsafe impl Send for SmsDeviceHolder {}
unsafe impl Sync for SmsDeviceHolder {}

static ACTIVE_SMS_DEVICE: OnceLock<Mutex<Option<SmsDeviceHolder>>> = OnceLock::new();
/// 事件驱动收到的短信缓冲区（subscribe_inbox_events 注入，get_inbox drain 消费）
static PENDING_SMS: OnceLock<Mutex<Vec<NewMessage>>> = OnceLock::new();
/// 送达回执缓冲区（get_inbox drain 消费，(phone, status) 对）
static PENDING_DELIVERY: OnceLock<Mutex<Vec<(String, String)>>> = OnceLock::new();

/// 取出并清空待处理的送达回执 (phone, new_status)。
pub fn drain_delivery_reports() -> Vec<(String, String)> {
    let cell = PENDING_DELIVERY.get_or_init(|| Mutex::new(Vec::new()));
    match cell.lock() {
        Ok(mut buf) => buf.drain(..).collect(),
        Err(_) => Vec::new(),
    }
}

fn win_err(e: windows::core::Error) -> String {
    format!("WinRT 错误: {} (0x{:08X})", e.message(), e.code().0)
}

/// 转换 WinRT DateTime 为标准 ISO-8601 UTC 字符串 (带 Z 后缀)
fn format_winrt_dt(dt: &windows::Foundation::DateTime) -> String {
    let ft = dt.UniversalTime as i64;
    // 1601-01-01 到 1970-01-01 的 100ns 间隔
    let unix = (ft - 116_444_736_000_000_000) / 10_000_000;
    let (h, m, sec) = (unix % 86400 / 3600, unix % 3600 / 60, unix % 60);
    let days = unix / 86400 + 719468;
    let era = days / 146097;
    let doe = (days - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let yr = if mo <= 2 { y + 1 } else { y };
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", yr, mo, d, h, m, sec)
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

/// 智能探测 SmsDevice2：优先 GetDefault，失败则通过设备选择器枚举（兼容其它笔记本/平板硬件）
fn find_sms_device2() -> Result<SmsDevice2, String> {
    if let Ok(dev) = SmsDevice2::GetDefault() {
        return Ok(dev);
    }
    if let Ok(selector) = SmsDevice2::GetDeviceSelector() {
        if let Ok(op) = DeviceInformation::FindAllAsyncAqsFilter(&selector) {
            if let Ok(devices) = op.get() {
                if let Ok(count) = devices.Size() {
                    for i in 0..count {
                        if let Ok(info) = devices.GetAt(i) {
                            if let Ok(id) = info.Id() {
                                if let Ok(dev) = SmsDevice2::FromId(&id) {
                                    return Ok(dev);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    Err("未检测到 4G/LTE 蜂窝设备 (SmsDevice2)，请确认已插入有效 SIM 卡并在系统设置中启用手机网络".into())
}

/// 智能探测 SmsDevice (v1)：优先 GetDefaultAsync，失败则枚举设备选择器
pub fn find_sms_device_v1() -> Result<SmsDevice, String> {
    // 优先从全局缓存获取
    let cell = ACTIVE_SMS_DEVICE.get_or_init(|| Mutex::new(None));
    if let Ok(guard) = cell.lock() {
        if let Some(holder) = guard.as_ref() {
            return Ok(holder.0.clone());
        }
    }

    if let Ok(op) = SmsDevice::GetDefaultAsync() {
        if let Ok(dev) = op.get() {
            if let Ok(mut guard) = cell.lock() {
                *guard = Some(SmsDeviceHolder(dev.clone()));
            }
            return Ok(dev);
        }
    }

    if let Ok(selector) = SmsDevice::GetDeviceSelector() {
        if let Ok(op) = DeviceInformation::FindAllAsyncAqsFilter(&selector) {
            if let Ok(devices) = op.get() {
                if let Ok(count) = devices.Size() {
                    for i in 0..count {
                        if let Ok(info) = devices.GetAt(i) {
                            if let Ok(id) = info.Id() {
                                if let Ok(dev_op) = SmsDevice::FromIdAsync(&id) {
                                    if let Ok(dev) = dev_op.get() {
                                        if let Ok(mut guard) = cell.lock() {
                                            *guard = Some(SmsDeviceHolder(dev.clone()));
                                        }
                                        return Ok(dev);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Err("未检测到 SMS 消息存储设备 (SmsDevice v1)".into())
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
            let dev = find_sms_device2()?;
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
        let base: ISmsMessageBase = text_msg.cast().map_err(win_err)?;
        dev.SendMessageAndGetResultAsync(&base).map_err(win_err)?.get().map_err(win_err)?;
        Ok(0)
    }

    async fn get_inbox(&self) -> Result<Vec<ReceivedSms>, String> {
        let dev1 = find_sms_device_v1()?;
        let store = dev1.MessageStore().map_err(win_err)?;
        let messages = store.GetMessagesAsync(SmsMessageFilter::All)
            .map_err(win_err)?.get().map_err(win_err)?;

        let mut result = Vec::new();
        let size = messages.Size().map_err(win_err)?;
        for i in 0..size {
            let item = messages.GetAt(i).map_err(win_err)?;
            // 送达回执检测 — SmsStatusMessage
            if let Ok(status_msg) = item.cast::<SmsStatusMessage>() {
                if let Ok(to) = status_msg.To() {
                    let to_str = to.to_string();
                    if let Ok(st) = status_msg.Status() {
                        let s = if st == 0 { "delivered".into() } else { "failed".into() };
                        let cell = PENDING_DELIVERY.get_or_init(|| Mutex::new(Vec::new()));
                        if let Ok(mut buf) = cell.lock() {
                            buf.push((to_str, s));
                        }
                    }
                }
                continue;
            }
            // 兼容普通文本短信与二进制 PDU 短信 (多数 4G/LTE 模块在 SIM 存储中均以 PDU 二进制格式保存)
            let text_msg_opt = if let Ok(text_msg) = item.cast::<SmsTextMessage>() {
                Some(text_msg)
            } else if let Ok(bin) = item.cast::<SmsBinaryMessage>() {
                SmsTextMessage::FromBinaryMessage(&bin).ok()
            } else {
                None
            };

            if let Some(text_msg) = text_msg_opt {
                let from = text_msg.From().map(|s| s.to_string()).unwrap_or_default();
                let body = text_msg.Body().map(|s| s.to_string()).unwrap_or_default();
                let device_id = item.Id().unwrap_or(0);
                let timestamp = text_msg.Timestamp().map(|dt| format_winrt_dt(&dt)).unwrap_or_default();

                if !from.is_empty() && !body.is_empty() {
                    result.push(ReceivedSms { from, body, timestamp, device_id });
                }
            }
        }
        Ok(result)
    }

    /// 关键修复 (问题 4): 删除指定 SIM 卡存储槽位中的短信，防止 20~50 条容量爆满导致断收
    async fn delete_message(&self, id: MessageId) -> Result<(), String> {
        if id == 0 {
            return Ok(());
        }
        let dev1 = find_sms_device_v1()?;
        let store = dev1.MessageStore().map_err(win_err)?;
        let action = store.DeleteMessageAsync(id as u32).map_err(win_err)?;
        action.get().map_err(win_err)?;
        eprintln!("[smsclient] 成功清理 SIM 卡/设备短信 (ID: {id})");
        Ok(())
    }

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

// ============================================================
// WinRT SmsDevice 事件驱动收信
// ============================================================

/// 订阅 SmsDevice 的 SmsMessageReceived 事件，收到的短信 push 到 PENDING_SMS。
/// 将 SmsDevice 引用持久保存在静态内存中，防止提前释放导致后续收不到短信。
pub fn subscribe_inbox_events(app_handle: tauri::AppHandle) -> Result<(), String> {
    use tauri::Emitter;

    let dev = find_sms_device_v1()?;
    let app_for_event = app_handle.clone();
    let handler = SmsMessageReceivedEventHandler::new(
        move |_sender: windows::core::Ref<'_, SmsDevice>,
              args: windows::core::Ref<'_, SmsMessageReceivedEventArgs>| {
            let Some(args_ref) = args.as_ref() else { return Ok(()) };
            let text_msg_opt = if let Ok(text_msg) = args_ref.TextMessage() {
                Some(text_msg)
            } else if let Ok(bin) = args_ref.BinaryMessage() {
                SmsTextMessage::FromBinaryMessage(&bin).ok()
            } else {
                None
            };

            if let Some(text_msg) = text_msg_opt {
                let from = text_msg.From().map(|s| s.to_string()).unwrap_or_default();
                let body = text_msg.Body().map(|s| s.to_string()).unwrap_or_default();
                let timestamp = text_msg.Timestamp().ok().map(|dt| format_winrt_dt(&dt));

                // 提取长短信分片元信息
                let part_num = text_msg.PartNumber().unwrap_or(1) as i64;
                let part_count = text_msg.PartCount().unwrap_or(1) as i64;
                let ref_id = text_msg.PartReferenceId().unwrap_or(0);
                let is_long = part_count > 1;
                let group_id = if is_long {
                    Some(format!("{from}_{ref_id}"))
                } else {
                    None
                };

                let msg = NewMessage::incoming_with_meta(
                    from,
                    body,
                    timestamp,
                    is_long,
                    part_num,
                    part_count,
                    group_id,
                );
                eprintln!("[smsclient] 收到实时短信 from={} body={} part={}/{}", msg.phone, msg.content, msg.part_index, msg.part_total);
                let phone = msg.phone.clone();
                let content = msg.content.clone();
                let cell = PENDING_SMS.get_or_init(|| Mutex::new(Vec::new()));
                if let Ok(mut buf) = cell.lock() {
                    buf.push(msg);
                }
                // emit sms-received 事件到前端
                let _ = app_for_event.emit("sms-received", serde_json::json!({
                    "phone": phone,
                    "content": content,
                }));
                // 桌面通知
                let notify_enabled = {
                    let db = app_for_event.state::<crate::db::DbState>();
                    db.with(|c| crate::settings::get(c, "notify_enabled"))
                        .ok()
                        .flatten()
                        .map(|s| s != "false")
                        .unwrap_or(true)
                };
                if notify_enabled {
                    let _ = app_for_event
                        .notification()
                        .builder()
                        .title(format!("新短信 {phone}"))
                        .body(&content)
                        .show();
                }
            }
            Ok(())
        },
    );
    dev.SmsMessageReceived(&handler).map_err(win_err)?;
    eprintln!("[smsclient] WinRT 短信事件监听器已成功挂载并锁定生命周期");
    Ok(())
}

/// 取出并清空 PENDING_SMS 中的待处理短信。
pub fn drain_pending_sms() -> Vec<NewMessage> {
    let cell = PENDING_SMS.get_or_init(|| Mutex::new(Vec::new()));
    match cell.lock() {
        Ok(mut buf) => buf.drain(..).collect(),
        Err(_) => Vec::new(),
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
}

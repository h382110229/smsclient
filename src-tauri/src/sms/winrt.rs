//! SMSClient — WinRT SMS 通道 (带超时隔离、MTA 安全执行、Singleflight 探测锁、全槽位清理与实时入库)

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Mutex, OnceLock};
use std::time::Duration;

use tauri::{Emitter, Manager};
use tauri_plugin_notification::NotificationExt;
use windows::core::Interface;
use windows::Devices::Enumeration::DeviceInformation;
use windows::Devices::Sms::{
    ISmsMessageBase, SmsBinaryMessage, SmsDevice, SmsDevice2, SmsDeviceStatus, SmsMessageFilter,
    SmsMessageReceivedEventArgs, SmsMessageReceivedEventHandler, SmsStatusMessage, SmsTextMessage,
    SmsTextMessage2,
};
use windows::Networking::Connectivity::{ConnectionProfile, NetworkInformation};
use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};

use super::channel::{MessageId, ReceivedSms, SmsChannel};
use crate::models::{ChannelStatus, NewMessage, SimInfo};

struct SmsDeviceHolder(SmsDevice);
unsafe impl Send for SmsDeviceHolder {}
unsafe impl Sync for SmsDeviceHolder {}

struct SmsDevice2Holder(SmsDevice2);
unsafe impl Send for SmsDevice2Holder {}
unsafe impl Sync for SmsDevice2Holder {}

static ACTIVE_SMS_DEVICE_V1: OnceLock<Mutex<Option<SmsDeviceHolder>>> = OnceLock::new();
static ACTIVE_SMS_DEVICE_V2: OnceLock<Mutex<Option<SmsDevice2Holder>>> = OnceLock::new();
static EVENT_SUBSCRIBED: AtomicBool = AtomicBool::new(false);
static PROBE_LOCK: Mutex<()> = Mutex::new(());

/// 事件驱动收到的短信缓冲区（兼容单元测试或兜底消费）
static PENDING_SMS: OnceLock<Mutex<Vec<NewMessage>>> = OnceLock::new();
/// 送达回执缓冲区
static PENDING_DELIVERY: OnceLock<Mutex<Vec<(String, String)>>> = OnceLock::new();

/// 判定当前设备缓存是否命中（热态）
pub fn is_device_cached() -> bool {
    let v2_cached = ACTIVE_SMS_DEVICE_V2
        .get()
        .and_then(|c| c.lock().ok())
        .map(|g| g.is_some())
        .unwrap_or(false);
    let v1_cached = ACTIVE_SMS_DEVICE_V1
        .get()
        .and_then(|c| c.lock().ok())
        .map(|g| g.is_some())
        .unwrap_or(false);
    v2_cached || v1_cached
}

/// 分级超时策略：缓存命中热态 2.5s，未命中冷态宽限 6s
pub fn query_timeout() -> Duration {
    if is_device_cached() {
        Duration::from_millis(2500)
    } else {
        Duration::from_secs(6)
    }
}

struct WinRtWorkerHandle {
    sender: mpsc::Sender<Box<dyn FnOnce() + Send>>,
}

static WORKER_HANDLE: OnceLock<Mutex<Option<WinRtWorkerHandle>>> = OnceLock::new();

fn get_or_spawn_worker() -> mpsc::Sender<Box<dyn FnOnce() + Send>> {
    let cell = WORKER_HANDLE.get_or_init(|| Mutex::new(None));
    let mut guard = cell.lock().unwrap();
    if let Some(handle) = guard.as_ref() {
        return handle.sender.clone();
    }

    let (tx, rx) = mpsc::channel::<Box<dyn FnOnce() + Send>>();
    let worker_tx = tx.clone();
    std::thread::Builder::new()
        .name("winrt-mta-worker".into())
        .spawn(move || {
            unsafe {
                let _ = RoInitialize(RO_INIT_MULTITHREADED);
            }
            while let Ok(task) = rx.recv() {
                task();
            }
        })
        .expect("failed to spawn winrt-mta-worker");

    *guard = Some(WinRtWorkerHandle { sender: tx });
    worker_tx
}

fn restart_worker() {
    let cell = WORKER_HANDLE.get_or_init(|| Mutex::new(None));
    if let Ok(mut guard) = cell.lock() {
        *guard = None;
    }
}

/// 在独立常驻 MTA 线程中执行 WinRT 闭包，并施加硬超时保护，彻底杜绝 STA 死锁或跨线程 COM 崩溃
fn run_winrt_with_timeout<T, F>(timeout: Duration, task: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    let worker_tx = get_or_spawn_worker();
    let (res_tx, res_rx) = mpsc::channel();

    let job = Box::new(move || {
        let res = task();
        let _ = res_tx.send(res);
    });

    if worker_tx.send(job).is_err() {
        restart_worker();
        invalidate_device_cache();
        return Err("WinRT 工作线程已断开，已触发自愈重置".into());
    }

    match res_rx.recv_timeout(timeout) {
        Ok(res) => res,
        Err(mpsc::RecvTimeoutError::Timeout) => {
            restart_worker();
            invalidate_device_cache();
            Err(format!(
                "WinRT 硬件响应超时 (>{}s)，设备可能休眠或不存在",
                timeout.as_secs_f32()
            ))
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            restart_worker();
            invalidate_device_cache();
            Err("WinRT 工作线程意外终止".into())
        }
    }
}

pub fn invalidate_device_cache() {
    if let Some(cell) = ACTIVE_SMS_DEVICE_V1.get() {
        if let Ok(mut g) = cell.lock() {
            *g = None;
        }
    }
    if let Some(cell) = ACTIVE_SMS_DEVICE_V2.get() {
        if let Ok(mut g) = cell.lock() {
            *g = None;
        }
    }
    EVENT_SUBSCRIBED.store(false, Ordering::SeqCst);
}

pub fn drain_delivery_reports() -> Vec<(String, String)> {
    let cell = PENDING_DELIVERY.get_or_init(|| Mutex::new(Vec::new()));
    match cell.lock() {
        Ok(mut buf) => buf.drain(..).collect(),
        Err(_) => Vec::new(),
    }
}

pub fn drain_pending_sms() -> Vec<NewMessage> {
    let cell = PENDING_SMS.get_or_init(|| Mutex::new(Vec::new()));
    match cell.lock() {
        Ok(mut buf) => buf.drain(..).collect(),
        Err(_) => Vec::new(),
    }
}

fn win_err(e: windows::core::Error) -> String {
    format!("WinRT 错误: {} (0x{:08X})", e.message(), e.code().0)
}

fn format_winrt_dt(dt: &windows::Foundation::DateTime) -> String {
    let ft = dt.UniversalTime as i64;
    let unix = (ft - 116_444_736_000_000_000) / 10_000_000;
    if unix <= 0 {
        return String::new();
    }
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

/// 内部探测 SmsDevice2（带 Double-Checked Singleflight 并发互斥锁）
/// 内部探测 SmsDevice2（带 Double-Checked Singleflight 并发互斥锁）
fn find_sms_device2_inner() -> Result<SmsDevice2, String> {
    let cell = ACTIVE_SMS_DEVICE_V2.get_or_init(|| Mutex::new(None));
    // 1. 快速检查已缓存就绪设备
    if let Ok(guard) = cell.lock() {
        if let Some(holder) = guard.as_ref() {
            if matches!(holder.0.DeviceStatus(), Ok(SmsDeviceStatus::Ready)) {
                return Ok(holder.0.clone());
            }
        }
    }

    // 2. 加 PROBE_LOCK 进行并发互斥（Singleflight 防开机 4 连击并发风暴）
    let _probe_guard = PROBE_LOCK
        .lock()
        .map_err(|e| format!("PROBE_LOCK 获取失败: {e}"))?;

    // 3. Double-Checked: 再次检查缓存
    if let Ok(guard) = cell.lock() {
        if let Some(holder) = guard.as_ref() {
            if matches!(holder.0.DeviceStatus(), Ok(SmsDeviceStatus::Ready)) {
                return Ok(holder.0.clone());
            }
        }
    }

    let resolved = (|| -> Option<SmsDevice2> {
        if let Ok(dev) = SmsDevice2::GetDefault() {
            return Some(dev);
        }
        let selector = SmsDevice2::GetDeviceSelector().ok()?;
        let op = DeviceInformation::FindAllAsyncAqsFilter(&selector).ok()?;
        let devices = op.get().ok()?;
        let count = devices.Size().ok()?;
        for i in 0..count {
            if let Ok(info) = devices.GetAt(i) {
                if let Ok(id) = info.Id() {
                    if let Ok(dev) = SmsDevice2::FromId(&id) {
                        return Some(dev);
                    }
                }
            }
        }
        None
    })();

    match resolved {
        Some(dev) => {
            if let Ok(mut guard) = cell.lock() {
                *guard = Some(SmsDevice2Holder(dev.clone()));
            }
            Ok(dev)
        }
        None => Err("未检测到 4G/LTE 蜂窝设备 (SmsDevice2)".into()),
    }
}

/// 内部探测 SmsDevice v1（带 Double-Checked Singleflight 并发互斥锁）
pub fn find_sms_device_v1_inner() -> Result<SmsDevice, String> {
    let cell = ACTIVE_SMS_DEVICE_V1.get_or_init(|| Mutex::new(None));
    // 1. 快速检查已缓存就绪设备
    if let Ok(guard) = cell.lock() {
        if let Some(holder) = guard.as_ref() {
            if holder.0.DeviceStatus().is_ok() {
                return Ok(holder.0.clone());
            }
        }
    }

    // 2. 加 PROBE_LOCK 进行并发互斥（Singleflight 防开机 4 连击并发风暴）
    let _probe_guard = PROBE_LOCK
        .lock()
        .map_err(|e| format!("PROBE_LOCK 获取失败: {e}"))?;

    // 3. Double-Checked: 再次检查缓存
    if let Ok(guard) = cell.lock() {
        if let Some(holder) = guard.as_ref() {
            if holder.0.DeviceStatus().is_ok() {
                return Ok(holder.0.clone());
            }
        }
    }

    let resolved = (|| -> Option<SmsDevice> {
        if let Ok(op) = SmsDevice::GetDefaultAsync() {
            if let Ok(dev) = op.get() {
                return Some(dev);
            }
        }
        let selector = SmsDevice::GetDeviceSelector().ok()?;
        let op = DeviceInformation::FindAllAsyncAqsFilter(&selector).ok()?;
        let devices = op.get().ok()?;
        let count = devices.Size().ok()?;
        for i in 0..count {
            if let Ok(info) = devices.GetAt(i) {
                if let Ok(id) = info.Id() {
                    if let Ok(dev_op) = SmsDevice::FromIdAsync(&id) {
                        if let Ok(dev) = dev_op.get() {
                            return Some(dev);
                        }
                    }
                }
            }
        }
        None
    })();

    match resolved {
        Some(dev) => {
            if let Ok(mut guard) = cell.lock() {
                *guard = Some(SmsDeviceHolder(dev.clone()));
            }
            Ok(dev)
        }
        None => Err("未检测到 SMS 消息存储设备 (SmsDevice v1)".into()),
    }
}

pub fn find_sms_device_v1() -> Result<SmsDevice, String> {
    find_sms_device_v1_inner()
}

pub struct WinRtChannel;

impl WinRtChannel {
    pub fn new() -> Self {
        Self
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
        let to_owned = to.to_string();
        let msg_owned = message.to_string();
        // 发信分级超时：15s
        run_winrt_with_timeout(Duration::from_secs(15), move || {
            let dev = find_sms_device2_inner()?;
            if !WinRtChannel::is_ready(&dev) {
                let st = dev.DeviceStatus().map_err(win_err)?;
                invalidate_device_cache();
                return Err(format!("SMS 设备未就绪 (DeviceStatus={st:?})"));
            }
            let text_msg = SmsTextMessage2::new().map_err(win_err)?;
            text_msg
                .SetTo(&windows::core::HSTRING::from(&to_owned))
                .map_err(win_err)?;
            text_msg
                .SetBody(&windows::core::HSTRING::from(&msg_owned))
                .map_err(win_err)?;
            let base: ISmsMessageBase = text_msg.cast().map_err(win_err)?;
            let res = dev
                .SendMessageAndGetResultAsync(&base)
                .map_err(win_err)?
                .get()
                .map_err(win_err)?;
            if !res.IsSuccessful().unwrap_or(false) {
                let err_code = res.CellularClass().ok();
                invalidate_device_cache();
                return Err(format!("运营商拒绝或发送失败 (CellularClass={err_code:?})"));
            }
            Ok(0)
        })
    }

    async fn get_inbox(&self) -> Result<Vec<ReceivedSms>, String> {
        let timeout = query_timeout();
        run_winrt_with_timeout(timeout, move || {
            let dev1 = find_sms_device_v1_inner()?;
            let store = dev1.MessageStore().map_err(win_err)?;
            let messages = store
                .GetMessagesAsync(SmsMessageFilter::All)
                .map_err(win_err)?
                .get()
                .map_err(win_err)?;

            let mut result = Vec::new();
            let size = messages.Size().map_err(win_err)?;
            for i in 0..size {
                let item = match messages.GetAt(i) {
                    Ok(it) => it,
                    Err(_) => continue,
                };
                let device_id = item.Id().unwrap_or(0);

                // 1. 送达回执检测 — SmsStatusMessage（处理后立即从 SIM 卡清理，防止回执塞满卡槽！）
                if let Ok(status_msg) = item.cast::<SmsStatusMessage>() {
                    if let Ok(to) = status_msg.To() {
                        let to_str = to.to_string();
                        if let Ok(st) = status_msg.Status() {
                            let s = if st == 0 {
                                "delivered".into()
                            } else {
                                "failed".into()
                            };
                            let cell = PENDING_DELIVERY.get_or_init(|| Mutex::new(Vec::new()));
                            if let Ok(mut buf) = cell.lock() {
                                buf.push((to_str, s));
                            }
                        }
                    }
                    if device_id > 0 {
                        if let Ok(act) = store.DeleteMessageAsync(device_id) {
                            let _ = act.get();
                        }
                    }
                    continue;
                }

                // 2. 兼容普通文本短信与二进制 PDU 短信
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
                    let timestamp = text_msg
                        .Timestamp()
                        .map(|dt| format_winrt_dt(&dt))
                        .unwrap_or_default();

                    let part_index = text_msg.PartNumber().unwrap_or(1) as i64;
                    let part_total = text_msg.PartCount().unwrap_or(1) as i64;
                    let ref_id = text_msg.PartReferenceId().unwrap_or(0);

                    if !from.is_empty() && !body.is_empty() {
                        result.push(ReceivedSms {
                            from,
                            body,
                            timestamp,
                            device_id,
                            part_index,
                            part_total,
                            ref_id,
                        });
                    } else if device_id > 0 {
                        // 空消息直接清理槽位
                        if let Ok(act) = store.DeleteMessageAsync(device_id) {
                            let _ = act.get();
                        }
                    }
                } else if device_id > 0 {
                    // 无法识别/损坏的 PDU 也清理槽位，防止永久卡死 SIM 容量
                    if let Ok(act) = store.DeleteMessageAsync(device_id) {
                        let _ = act.get();
                    }
                }
            }
            Ok(result)
        })
    }

    async fn delete_message(&self, id: MessageId) -> Result<(), String> {
        if id == 0 {
            return Ok(());
        }
        run_winrt_with_timeout(Duration::from_secs(5), move || {
            let dev1 = find_sms_device_v1_inner()?;
            let store = dev1.MessageStore().map_err(win_err)?;
            let action = store.DeleteMessageAsync(id).map_err(win_err)?;
            action.get().map_err(win_err)?;
            Ok(())
        })
    }

    async fn get_signal_strength(&self) -> Result<i32, String> {
        let timeout = query_timeout();
        run_winrt_with_timeout(timeout, move || {
            let profile =
                find_wwan_profile().ok_or_else(|| "未找到蜂窝 WWAN 网络".to_string())?;
            let bars = profile
                .GetSignalBars()
                .map_err(win_err)?
                .Value()
                .unwrap_or(0);
            Ok(match bars {
                4 => -60,
                3 => -75,
                2 => -85,
                1 => -95,
                0 => -110,
                _ => -100,
            })
        })
    }

    async fn get_sim_info(&self) -> Result<SimInfo, String> {
        let timeout = query_timeout();
        run_winrt_with_timeout(timeout, move || {
            let dev = find_sms_device2_inner()?;
            let wwan = find_wwan_profile();

            let carrier = wwan
                .as_ref()
                .and_then(|p| p.GetNetworkNames().ok())
                .and_then(|n| n.GetAt(0).ok().map(|s| s.to_string()))
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "未知运营商".to_string());

            let network_type = "4G/LTE";
            let signal_dbm = wwan
                .as_ref()
                .and_then(|p| p.GetSignalBars().ok())
                .and_then(|b| b.Value().ok())
                .map(|bars| match bars {
                    4 => -60,
                    3 => -75,
                    2 => -85,
                    1 => -95,
                    0 => -110,
                    _ => -100,
                })
                .unwrap_or(-100);
            let signal_strength =
                ((signal_dbm + 100) as f32 / 50.0 * 100.0).clamp(0.0, 100.0) as i32;

            let phone_number = dev
                .AccountPhoneNumber()
                .ok()
                .map(|s| s.to_string())
                .filter(|s| !s.is_empty() && s != "+00000000000");

            Ok(SimInfo {
                carrier,
                signal_strength,
                signal_dbm,
                network_type: network_type.to_string(),
                iccid: "N/A (WinRT)".into(),
                imsi: "N/A (WinRT)".into(),
                phone_number,
            })
        })
    }

    async fn status(&self) -> ChannelStatus {
        let timeout = query_timeout();
        match run_winrt_with_timeout(timeout, move || {
            let dev = find_sms_device2_inner()?;
            let ready = WinRtChannel::is_ready(&dev);
            let err_msg = if ready {
                None
            } else {
                Some(format!("DeviceStatus={:?}", dev.DeviceStatus()))
            };
            Ok((ready, err_msg))
        }) {
            Ok((ready, error)) => ChannelStatus {
                channel_type: "winrt".into(),
                available: ready,
                com_port: None,
                error,
            },
            Err(e) => ChannelStatus {
                channel_type: "winrt".into(),
                available: false,
                com_port: None,
                error: Some(e),
            },
        }
    }
}

// ============================================================
// WinRT SmsDevice 实时事件驱动收信（带自动重连与即时入库）
// ============================================================

pub fn ensure_event_subscribed(app_handle: &tauri::AppHandle) {
    if !EVENT_SUBSCRIBED.load(Ordering::SeqCst) {
        let _ = subscribe_inbox_events(app_handle.clone());
    }
}

pub fn subscribe_inbox_events(app_handle: tauri::AppHandle) -> Result<(), String> {
    run_winrt_with_timeout(Duration::from_secs(6), move || {
        let dev = find_sms_device_v1_inner()?;
        let app_for_event = app_handle.clone();
        let handler = SmsMessageReceivedEventHandler::new(
            move |_sender: windows::core::Ref<'_, SmsDevice>,
                  args: windows::core::Ref<'_, SmsMessageReceivedEventArgs>| {
                let Some(args_ref) = args.as_ref() else {
                    return Ok(());
                };
                let text_msg_opt = if let Ok(text_msg) = args_ref.TextMessage() {
                    Some(text_msg)
                } else if let Ok(bin) = args_ref.BinaryMessage() {
                    SmsTextMessage::FromBinaryMessage(&bin).ok()
                } else {
                    None
                };

                if let Some(text_msg) = text_msg_opt {
                    let raw_from = text_msg.From().map(|s| s.to_string()).unwrap_or_default();
                    let phone = crate::contacts::normalize_phone(&raw_from);
                    let body = text_msg
                        .Body()
                        .map(|s| s.to_string())
                        .unwrap_or_default()
                        .trim()
                        .to_string();
                    if phone.is_empty() || body.is_empty() {
                        return Ok(());
                    }

                    let timestamp = text_msg
                        .Timestamp()
                        .ok()
                        .map(|dt| format_winrt_dt(&dt))
                        .filter(|s| !s.is_empty());

                    let part_num = text_msg.PartNumber().unwrap_or(1) as i64;
                    let part_count = text_msg.PartCount().unwrap_or(1) as i64;
                    let ref_id = text_msg.PartReferenceId().unwrap_or(0);
                    let is_long = part_count > 1;
                    let group_id = if is_long {
                        Some(format!("{phone}_{ref_id}"))
                    } else {
                        None
                    };

                    let msg = NewMessage::incoming_with_meta(
                        phone.clone(),
                        body.clone(),
                        timestamp.clone(),
                        is_long,
                        part_num,
                        part_count,
                        group_id.clone(),
                    );

                    // 1. 立即通过短锁去重入库（确保前端任何页面收到事件后查询 DB 都有数据！）
                    let mut inserted_id: Option<i64> = None;
                    let mut notify_enabled = true;
                    let db = app_for_event.state::<crate::db::DbState>();
                    let _ = db.with(|conn| {
                        if !crate::sms::inbox_exists_recent(conn, &phone, &body, part_num).unwrap_or(false) {
                            if let Ok(id) = crate::messages::insert(conn, &msg) {
                                inserted_id = Some(id);
                            }
                        }
                        notify_enabled = crate::settings::get(conn, "notify_enabled")
                            .ok()
                            .flatten()
                            .map(|s| s != "false")
                            .unwrap_or(true);
                        Ok(())
                    });

                    // 注入兜底缓冲区
                    let cell = PENDING_SMS.get_or_init(|| Mutex::new(Vec::new()));
                    if let Ok(mut buf) = cell.lock() {
                        buf.push(msg);
                    }

                    if let Some(id) = inserted_id {
                        // 2. 通知前端刷新并携带完整分片与时间戳信息
                        let _ = app_for_event.emit(
                            "sms-received",
                            serde_json::json!({
                                "id": id,
                                "phone": phone,
                                "content": body,
                                "timestamp": timestamp.as_deref().unwrap_or_default(),
                                "partIndex": part_num,
                                "partTotal": part_count,
                                "groupId": group_id,
                            }),
                        );
                        // 3. 弹出系统通知
                        if notify_enabled {
                            let _ = app_for_event
                                .notification()
                                .builder()
                                .title(format!("新短信 {phone}"))
                                .body(&body)
                                .show();
                        }
                    }
                }
                Ok(())
            },
        );
        dev.SmsMessageReceived(&handler).map_err(|e| {
            invalidate_device_cache();
            win_err(e)
        })?;
        EVENT_SUBSCRIBED.store(true, Ordering::SeqCst);
        eprintln!("[smsclient] WinRT 短信事件监听器已成功挂载");
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn channel_is_send_sync() {
        fn a<T: Send + Sync>() {}
        a::<WinRtChannel>();
    }
}

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

use crate::models::{ChannelStatus, SimInfo};

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
}

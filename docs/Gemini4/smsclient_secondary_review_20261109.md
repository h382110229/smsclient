# SMSClient 二次审阅深水区加固专档 (v0.3.0 终极架构重构)

> **项目名称**：SMSClient (Windows 4G/LTE 蜂窝短信桌面客户端 v0.3.0)  
> **技术栈**：Tauri 2 (Rust) + React 19 + Tailwind CSS 4 + bundled SQLite (WAL) + Windows WinRT API (`windows` 0.61.3)  
> **审阅时间**：2026-11-09  
> **审阅结论**：Gemini 4 两次审阅全面拍板通过，架构级缺陷彻底根除

---

## 1. 核心架构重构与加固要点

### 1.1 COM STA 死锁与常驻 MTA 工作线程 (`winrt-mta-worker`)
- **根因消除**：Windows WinRT `SmsDevice` / `SmsDevice2` COM 接口在无 `IAgileObject` 支持下跨线程访问会导致 `0xc0000005 (STATUS_ACCESS_VIOLATION)`，同时在 UI STA 主线程直接阻塞等待会触发 COM 消息泵死锁。
- **加固方案**：引入常驻的 `winrt-mta-worker` 专用线程，初始化 `RoInitialize(RO_INIT_MULTITHREADED)`，统一管理 WinRT 句柄生命周期。所有硬件调用通过带硬超时的 `mpsc` 任务通道派发。一旦超时触发，自愈重置并生成全新工作线程。

### 1.2 并发探测互斥锁 Singleflight (`PROBE_LOCK`)
- **根因消除**：开机首帧前端并发请求 `get_channel_status`、`get_sim_info`、`get_signal_strength`、`get_inbox` 四连击，引发多次重复底层硬件枚举与 AT/MBIM 总线冲突。
- **加固方案**：设计 `PROBE_LOCK` 与 Double-Checked 缓存检测，首个请求进行硬件枚举，其余并发请求命中热缓存直接返回，零冗余硬件 I/O。

### 1.3 分级超时策略
- **冷态宽限**：缓存未命中或首次冷启动时赋予 6 秒宽限；
- **热态极速**：缓存命中后查询超时严格压减至 2.5 秒；
- **发信保护**：发信操作赋予 15 秒硬超时保护。

### 1.4 滑动窗口精准去重（2 分钟时间窗）
- **根因消除**：旧版无时间窗 `SELECT COUNT(*)` 导致历史收到过的相同短信（测试指令、相同验证码）被永久误杀。
- **加固方案**：基于 SQLite `datetime(timestamp) >= datetime('now', '-2 minutes')` 限制判重窗口，不仅彻底杜绝“事件监听 + 轮询”双通道重复入库，更杜绝了后续重新获取相同验证码时的静默丢弃。

### 1.5 完整长短信分片与全槽位排空
- **长短信支持**：`ReceivedSms` 完整扩展 `part_index`、`part_total`、`ref_id` 并在 `sync_inbox_non_blocking` 中构建 `group_id`。
- **全量卡槽清理**：在从 `MessageStore` 读出消息时，对普通文本短信、PDU 短信、`SmsStatusMessage` 送达回执以及损坏/空内容 PDU 全部执行 `DeleteMessageAsync`，确保 SIM 卡 20~50 条物理容量永不爆满。

### 1.6 Modern Standby 盒盖睡眠唤醒自愈
- **时钟跳变感知**：后台守护线程（15s 周期）记录 `last_tick.elapsed()`，当 `elapsed > 25s` 时识别到系统休眠唤醒，自动触发 `invalidate_device_cache()` 并执行 `ensure_event_subscribed()` 自愈重连。

### 1.7 便携模式数据库统一
- **路径解析收敛**：抽象统一的 `resolve_db_path()`，保证群发后台线程与主程序在便携模式（`exe所在目录/data/smsclient.db`）下操作同一数据库。

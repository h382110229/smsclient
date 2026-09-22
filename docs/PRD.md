# SMSClient — 需求规格说明书 (PRD)

> **版本**: v1.0  
> **日期**: 2026-09-22  
> **状态**: ✅ 已确认（Q7/Q8 已拍板 2026-09-22）—进入 M1 开发

---

## 1. 项目概述

### 1.1 项目定位
SMSClient 是一款运行在 Windows 桌面端的 **短信收发客户端**，通过蜂窝网络（4G/LTE）收发短信，支持中文、长短信拼接、群发、送达回执等功能，可单文件 exe 分发给其他朋友使用。

### 1.2 核心目标
| 优先级 | 目标 | 验收标准 |
|--------|------|----------|
| P0 | 收发真实短信 | 双击 exe → 发一条短信到手机 → 手机回复 → exe 收到并显示 |
| P0 | 不影响 4G 上网 | 收发短信全程，已有的 4G 数据连接（ping 223.5.5.5）不中断 |
| P1 | 中文支持 | 支持 UCS2 中文短信、>70 字自动长短信拆分拼接 |
| P1 | 本地持久化 | 短信存 SQLite，支持导出 CSV/文本 |
| P1 | 群发能力 | 多号码/通讯录导入，发送失败自动重试 |
| P2 | 送达/读取报告 | AT 层回执自动解析入库 |
| P2 | 桌面通知 | 新短信到达时系统通知+声音提醒 |
| P2 | 联系人管理 | 简易版，支持手动添加 + CSV 导入 |

### 1.3 不做的事（v1.0 排除）
- ❌ 自动回复 / 规则引擎（留给 v2.0）
- ❌ 语音通话
- ❌ RCS / iMessage
- ❌ 端到端加密
- ❌ 多 SIM 卡同时在线（当前只支持单 SIM，换卡需重新插入模块）

---

## 2. 硬件与环境基线（物理探针实测）

### 2.1 本机硬件
| 项目 | 实测值 |
|------|--------|
| 机型 | LENOVO 21MLA002CD (ThinkPad 系) |
| 操作系统 | Windows 11 25H2+ (Build 26200) |
| 4G 模块 | Quectel EM05-CN (USB VID_2C7C, PID_0310) |
| 模块固件 | EM05CNFDR08A12M1G_LNV (联想定制版) |
| 当前 SIM 卡 | **中国移动卡**（2026-09-22 已换装；原联通物联网卡 ICCID 8986************ 不支持短信，已退役） |
| 运营商 | CMCC（探针实测：提供商名称=CMCC，不漫游，MBIM 已连接） |
| 信号强度 | 35% / -91 dBm（移动卡实测 2026-09-22） |
| 4G 网络状态 | ✅ 正常 (ping 223.5.5.5 = 6-8ms) |
| MBIM 接口 | ✅ 已连接 "手机网络" (GSM, LTE) |
| AT 串口 | ❌ 当前未暴露（USB 组合仅含 GNSS + MBIM） |
| 管理员权限 | ✅ 已具备 |

### 2.2 驱动与服务状态
| 组件 | 状态 |
|------|------|
| Quectel GNSS Sensor (MI_00) | ✅ OK |
| Quectel EM05-CN MBIM 网卡 (MI_01) | ✅ OK, 已连接 |
| qcser 串口驱动 (oem140.inf) | ✅ 已安装（等待 MI_03/04/05 接口） |
| Quectel Mini DPR Service | ✅ Running, AutoStart |
| WWAN AutoConfig | ✅ Running |
| RasMan | ✅ Running |
| COM 串口实例 | ❌ 0 个（AT 口未暴露） |

### 2.3 MBIM SMS 能力（已验证）
```
SMS 功能:        PDU 接收, PDU 发送 ✅
短信格式:        PDU
服务中心地址:    +8613010344500
可用消息计数:    50
WinRT SmsDevice2 API: ✅ 可用
准备就绪发短信:  否 ← 联通物联网卡不支持（非程序问题）
```

### 2.4 USB 组合与 AT 口分析

**当前 USB 组合 (PID_0310, 仅 2 个接口):**
```
MI_00 → Quectel GNSS Sensor Device  (Sensor 类)
MI_01 → Quectel EM05-CN MBIM 网卡   (Net 类)
```

**目标 USB 组合 (PID_0312, 5 个接口, qcser.inf 已支持):**
```
MI_00 → Quectel GNSS Sensor Device  (Sensor 类, 保留)
MI_01 → Quectel EM05-CN MBIM 网卡   (Net 类, 保留, 上网不中断)
MI_03 → Quectel DM DIAG 串口        (qcser 驱动, COM 口)
MI_04 → Quectel NMEA GPS 串口       (qcser 驱动, COM 口)
MI_05 → Quectel AT2 命令串口        (qcser 驱动, COM 口) ★
```

**切换方式:** PID_0310→0312 切换需发送 AT 指令或 USB 控制传输，切换瞬间 4G 会断开重连约 10-30 秒。qusbccgp.inf 同时支持两个 PID，驱动就绪。

---

## 3. 技术决策记录

### 3.1 用户确认的决策

| 编号 | 问题 | 决策 |
|------|------|------|
| Q1 | 允许 AT 口探测实验 | ✅ 已执行探测，结果见 §2.4 |
| Q2 | 技术栈选择 | **Tauri 2 + React 19 + Shadcn/ui + Tailwind CSS 4**（ASTeam UI 设计系统直用）|
| Q3 | v1.0 功能范围 | ✅ 接受全部推荐功能（收发、中文 UCS2、长短信、群发、回执、SQLite、通知、简易联系人）|
| Q4.1 | 不影响 4G 上网 | ✅ 硬约束 |
| Q4.2 | 可分发给朋友 | ✅ 确认（影响打包策略：需自包含 exe，Tauri ~15-25MB）|
| Q5 | SIM 卡情况 | ✅ 已执行：2026-09-22 换装中国移动卡，探针确认 CMCC 已连接，可正常收发短信 |
| Q6 | 验收标准 | 双击 exe 能收发真实短信 + 不影响上网 |
| Q7 | AT 口策略 | ✅ **选 A**：先用 WinRT SMS API（零风险），AT Serial 通道 B 预留（换移动卡时再启用）|
| Q8 | 技术栈确认 | ✅ 确认：Tauri 2 + React 19 + Shadcn/ui + Tailwind CSS 4 |

### 3.2 待确认决策（阻塞项）

| 编号 | 问题 | 选项 | 推荐 | 状态 |
|------|------|------|------|------|
| Q7 | AT 口策略 | **选 A**：先用 WinRT SMS API（零风险），AT 通道 B 预留 | ✅ 已确认 2026-09-22 |
| Q8 | 技术栈确认 | Tauri 2 + React 19 + Shadcn/ui + Tailwind 4 | ✅ 已确认 2026-09-22 |

---

## 4. 架构设计

### 4.1 整体架构

```
┌─────────────────────────────────────────────────────────┐
│                    SMSClient.exe (Tauri 2)               │
│                                                          │
│  ┌─────────────────────────────────────────────────────┐ │
│  │              Frontend (React + Shadcn/ui)            │ │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────┐            │ │
│  │  │ 收件箱   │ │ 写短信   │ │ 联系人   │            │ │
│  │  │ (Inbox)  │ │ (Compose)│ │ (Contacts)│            │ │
│  │  └──────────┘ └──────────┘ └──────────┘            │ │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────┐            │ │
│  │  │ 已发送   │ │ 群发     │ │ 设置     │            │ │
│  │  │ (Sent)   │ │(Broadcast)│ │(Settings)│            │ │
│  │  └──────────┘ └──────────┘ └──────────┘            │ │
│  └─────────────────────┬───────────────────────────────┘ │
│                        │ invoke / listen                  │
│  ┌─────────────────────┴───────────────────────────────┐ │
│  │              Backend (Rust, Tauri Commands)           │ │
│  │  ┌──────────────────────────────────────────┐       │ │
│  │  │ SMS Engine (可插拔通道)                    │       │ │
│  │  │  ┌─────────────┐  ┌────────────────────┐ │       │ │
│  │  │  │ WinRT SMS    │  │ AT Serial Port     │ │       │ │
│  │  │  │ (通道 A)     │  │ (通道 B, 预留)     │ │       │ │
│  │  │  │ 零风险       │  │ 需 USB 组合切换    │ │       │ │
│  │  │  └─────────────┘  └────────────────────┘ │       │ │
│  │  └──────────────────────────────────────────┘       │ │
│  │  ┌────────────┐  ┌────────────┐  ┌──────────────┐  │ │
│  │  │ SQLite     │  │ Contacts   │  │ Notification │  │ │
│  │  │ Storage    │  │ Manager    │  │ Engine       │  │ │
│  │  └────────────┘  └────────────┘  └──────────────┘  │ │
│  └─────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────┘
```

### 4.2 SMS 通道抽象层

```rust
// 通道 trait —— 无论 WinRT 还是 AT，上层逻辑统一
pub trait SmsChannel: Send + Sync {
    async fn send_sms(&self, to: &str, message: &str) -> Result<MessageId>;
    async fn get_inbox(&self) -> Result<Vec<SmsMessage>>;
    async fn delete_message(&self, id: MessageId) -> Result<()>;
    async fn get_signal_strength(&self) -> Result<SignalInfo>;
    async fn get_sim_info(&self) -> Result<SimInfo>;
}
```

- **通道 A (WinRT SMS):** 通过 `windows` crate 调用 `Windows.Devices.Sms.SmsDevice2`，无需 COM 口，不影响上网
- **通道 B (AT Serial):** 通过 `tokio-serial` 调用 AT+CMGS/CMGL/CMGR，需 MI_05 AT 口就绪，提供完整 PDU 控制力

### 4.3 ASTeam UI 设计落地

设计系统完整搬运 `~/.asteam/skills/llmapi-ui-design.md` 中的规范：

| 设计令牌 | 值 | 用途 |
|----------|-----|------|
| --asteam-brand-primary | #006857 | 主按钮、导航高亮 |
| --asteam-brand-accent | #D31245 | 品牌标识（不用于错误） |
| --asteam-success | #15803D | 发送成功 |
| --asteam-error | #B42318 | 发送失败 |
| --asteam-warning | #B45309 | 信号弱、SIM 状态警告 |
| --asteam-info | #0369A1 | 提示信息 |

- Light/Dark 双主题切换
- 圆角: sm 6px / md 8px / lg 12px / pill 999px
- 字体: Inter + Noto Sans SC + JetBrains Mono
- 动画: fast 120ms / normal 180ms / slow 240ms, ease-out

### 4.4 页面结构

```
src/
├── app/
│   ├── layout.tsx              # 根布局 (字体, ThemeProvider, 侧边栏)
│   ├── page.tsx                # 仪表盘 (SIM 状态, 信号, 收发统计)
│   ├── inbox/page.tsx          # 收件箱 (会话列表, 短信详情)
│   ├── compose/page.tsx        # 写短信 (收件人选择, 中文输入, 长短信预览)
│   ├── sent/page.tsx           # 已发送
│   ├── contacts/page.tsx       # 联系人管理
│   ├── broadcast/page.tsx      # 群发
│   ├── settings/page.tsx       # 设置 (通道选择, 通知, 导出)
│   └── login/page.tsx          # (预留, 无认证则跳过)
├── components/
│   ├── ui/                     # Shadcn/ui 组件
│   ├── sidebar.tsx             # 侧边栏导航
│   ├── header.tsx              # 顶部栏 (信号强度, SIM 状态, 主题切换)
│   ├── message-list.tsx        # 短信列表组件
│   ├── message-detail.tsx      # 短信详情
│   ├── compose-form.tsx        # 写短信表单
│   ├── contact-card.tsx        # 联系人卡片
│   ├── stats-card.tsx          # 统计卡片
│   └── signal-indicator.tsx    # 信号强度指示器
└── lib/
    ├── tauri-bridge.ts         # Tauri invoke 封装
    ├── types.ts                # TypeScript 类型定义
    └── utils.ts                # 工具函数
```

---

## 5. 数据模型

### 5.1 SQLite 表结构

```sql
-- 联系人表
CREATE TABLE contacts (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    name        TEXT NOT NULL,
    phone       TEXT NOT NULL UNIQUE,
    note        TEXT,
    created_at  DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- 短信表
CREATE TABLE messages (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    direction   TEXT CHECK(direction IN ('in', 'out')) NOT NULL,
    phone       TEXT NOT NULL,
    content     TEXT NOT NULL,
    encoding    TEXT DEFAULT 'UCS2',
    is_long     BOOLEAN DEFAULT 0,
    part_index  INTEGER DEFAULT 1,
    part_total  INTEGER DEFAULT 1,
    group_id    TEXT,              -- 长短信/群发的组ID
    status      TEXT DEFAULT 'pending',  -- pending/sent/delivered/read/failed
    error       TEXT,
    timestamp   DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- 群发任务表
CREATE TABLE broadcast_tasks (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    content     TEXT NOT NULL,
    total       INTEGER NOT NULL,
    sent        INTEGER DEFAULT 0,
    failed      INTEGER DEFAULT 0,
    status      TEXT DEFAULT 'pending',
    created_at  DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- 设置表
CREATE TABLE settings (
    key         TEXT PRIMARY KEY,
    value       TEXT NOT NULL
);
```

### 5.2 核心数据类型 (TypeScript)

```typescript
interface SmsMessage {
  id: number;
  direction: 'in' | 'out';
  phone: string;
  content: string;
  encoding: string;
  isLong: boolean;
  partIndex: number;
  partTotal: number;
  groupId: string | null;
  status: 'pending' | 'sent' | 'delivered' | 'read' | 'failed';
  error: string | null;
  timestamp: string;
}

interface Contact {
  id: number;
  name: string;
  phone: string;
  note: string;
}

interface SimInfo {
  carrier: string;
  iccid: string;
  imsi: string;
  phoneNumber: string | null;
  signalStrength: number;    // 0-100
  signalDbm: number;         // -120 to -30
  networkType: string;       // 'LTE' | 'HSPA+' | '3G' | '2G'
}

interface ChannelStatus {
  type: 'winrt' | 'at';
  available: boolean;
  comPort: string | null;
  error: string | null;
}
```

---

## 6. 功能需求详述

### F1. 收件箱 (P0)
- 会话视图：按联系人/号码聚合短信
- 实时监听新短信（WinRT 事件回调 → Tauri emit → 前端 listen）
- 短信详情：内容、时间戳、送达状态、编码方式
- 搜索：按号码或内容关键词
- 分页加载（虚拟滚动，应对大量短信）

### F2. 写短信 / 发送 (P0)
- 收件人输入：手动输入号码 + 联系人选择器
- 内容输入：支持中文
- 长短信自动拆分：>70 中文字符自动 UCS2 拆分，显示"已拆为 N 条"提示
- 发送确认：二次确认弹窗
- 发送状态：实时更新 (pending → sent → delivered)
- 重发：失败后一键重发

### F3. 群发 (P1)
- 收件人来源：手动多选 + CSV 导入 + 通讯录选择
- 进度显示：已发/总数/失败数
- 失败重试：自动重试 2 次，间隔 5 秒
- 防骚扰限速：可配置每分钟最大发送条数（默认 10 条/分钟）

### F4. 送达/读取报告 (P2)
- AT 通道：通过 AT+CSMP / AT+CNMI 接收 SRI 报告
- WinRT 通道：通过 DeliveryStatus 枚举
- 自动匹配原消息并更新状态

### F5. 本地持久化 (P1)
- SQLite 存储所有短信和联系人
- 导出 CSV（全部短信 / 按联系人 / 按时间范围）
- 导出纯文本备份

### F6. 桌面通知 (P2)
- Windows 原生通知 (toast)
- 可配置声音提醒
- 通知点击跳转到对应会话

### F7. 联系人管理 (P2)
- 手动添加/编辑/删除
- CSV 导入（格式：姓名,号码）
- 联系人搜索

### F8. 设置页面
- SMS 通道切换（WinRT / AT）
- 通知开关 + 声音开关
- 限速配置（群发速率限制）
- 数据导出
- 主题切换 (Light/Dark)

---

## 7. 非功能需求

| 需求 | 要求 |
|------|------|
| 启动时间 | < 3 秒 (冷启动到界面可交互) |
| exe 体积 | < 30 MB (Tauri 单文件自包含) |
| 内存占用 | < 150 MB (运行态) |
| 短信延迟 | 发送：点击到网络提交 < 5 秒 |
| 数据安全 | SQLite 文件加密可选（v2.0）|
| 兼容性 | Windows 10 1903+ / Windows 11 |
| 分发 | 单文件 exe，无需安装，无需运行时依赖 |

---

## 8. 验收标准

### 8.1 功能验收（用真实 SIM 卡测试）
| # | 测试用例 | 预期结果 |
|---|----------|----------|
| AC-1 | 双击 exe 启动 | 主界面正常加载，显示 SIM 状态和信号强度 |
| AC-2 | 发送一条中文短信到测试手机 | 手机在 10 秒内收到短信 |
| AC-3 | 测试手机回复短信 | exe 在 10 秒内收到，显示在收件箱 |
| AC-4 | 发送一条 >70 字中文短信 | 自动拆分为 N 条，手机依次收到完整内容 |
| AC-5 | 群发到 3 个号码 | 全部发送成功，进度条正确 |
| AC-6 | 在收发短信期间持续 ping 外网 | 4G 数据连接不中断 (无丢包) |
| AC-7 | SQLite 数据持久化 | 重启 exe 后历史短信仍在 |
| AC-8 | 导出 CSV | 打开 CSV 文件内容完整正确 |

### 8.2 非功能验收
| # | 测试项 | 预期 |
|---|--------|------|
| NAC-1 | exe 体积 | < 30 MB |
| NAC-2 | 首次启动 | < 3 秒 |
| NAC-3 | 换 SIM 卡后 | 程序自动识别新卡，无需手动配置 |

---

## 9. 里程碑计划

| 阶段 | 内容 | 预计 |
|------|------|------|
| M0 | PRD 确认 + Q7/Q8 拍板 | ✅ 已完成 2026-09-22 |
| M1 | Tauri 工程初始化 + ASTeam UI 移植 + 设计令牌落地 | M0 后 |
| M2 | SQLite 数据层 + 联系人 CRUD | M1 后 |
| M3 | WinRT SMS 通道集成 (收发核心) | M2 后 |
| M4 | 收件箱/写短信/已发送 页面联调 | M3 后 |
| M5 | 群发 + 回执 + 通知 | M4 后 |
| M6 | 端到端验收 (换移动 SIM 卡实测) | M5 后 |

---

## 10. 开放问题

| # | 问题 | 影响范围 | 状态 |
|---|------|----------|------|
| Q7 | SMS 通道选 WinRT 先行还是 AT 先行 | M3 实现方式 | ✅ 选 A：WinRT 先行，AT 通道预留（2026-09-22）|
| Q8 | 技术栈 Tauri 2 是否确认 | 全局架构 | ✅ 已确认（2026-09-22）|
| R1 | 换移动卡后是否需要自动切换运营商配置 | F1 收件箱 SMSC 地址 | 待观察 |
| R2 | 长短信最大拆分长度（中文 67 字/条 vs 英文 153 字/条） | F2 发送逻辑 | 已知约束 |

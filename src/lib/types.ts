/**
 * SMSClient — Core Type Definitions
 * Worker C「前端骨架」— 2026-09-22
 *
 * PRD §5.2 数据模型冻结版本。
 * 字段命名遵循 camelCase（与 Rust #[serde(rename_all="camelCase")] 对齐）。
 * 枚举/联合类型以 string literal 表达，序列化时天然兼容。
 */

/** 短信消息 */
export interface SmsMessage {
  /** 唯一消息 ID (uuid) */
  id: string;
  /** 对方手机号码 */
  address: string;
  /** 短信正文 */
  body: string;
  /** 时间戳 (Unix seconds) */
  timestamp: number;
  /** 已读标记 */
  read: boolean;
  /** 投递状态 */
  status: "sending" | "sent" | "delivered" | "failed" | "received";
  /** 消息方向 */
  msgType: "inbox" | "sent";
  /** 长短信拆分总段数（可选） */
  parts?: number;
  /** 长短信当前段序号（可选） */
  partIndex?: number;
}

/** 联系人 */
export interface Contact {
  id: string;
  /** 姓名 */
  name: string;
  /** 手机号码 */
  phone: string;
  /** 头像 URL (可选) */
  avatar?: string;
  /** 备注 */
  note?: string;
  /** 标签 */
  tags?: string[];
}

/** SIM 卡信息 */
export interface SimInfo {
  /** 运营商 */
  carrier: string;
  /** ICCID */
  iccid: string;
  /** IMSI */
  imsi: string;
  /** 信号强度 (dBm, 负值) */
  signalStrength: number;
  /** 网络类型 */
  networkType: string;
}

/** 通道状态 */
export interface ChannelStatus {
  /** 通道类型 */
  channel: "winrt" | "at";
  /** 是否就绪 */
  ready: boolean;
  /** 错误信息 (可选) */
  error?: string;
}

/** 主题模式 */
export type ThemeMode = "light" | "dark" | "system";

/** 应用全局设置 */
export interface AppSettings {
  /** 主题 */
  theme: ThemeMode;
  /** 发送通道偏好 */
  preferredChannel: "winrt" | "at";
  /** 长短信拆分策略 */
  longSmsStrategy: "auto" | "always-gsm7" | "always-ucs2";
  /** 送达回执开关 */
  deliveryReport: boolean;
  /** 群发间隔 (毫秒) */
  broadcastInterval: number;
  /** SMSC 地址 (可选, 覆盖 SIM 卡默认值) */
  smscAddress?: string;
}

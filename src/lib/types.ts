/**
 * SMSClient — Core Type Definitions (PRD §5.2 对齐版)
 *
 * 所有字段与 PRD §5.2 TypeScript 接口一一对应。
 * camelCase 命名与 Rust #[serde(rename_all="camelCase")] 自动映射。
 *
 * 变更记录 (M2 2026-09-22):
 * - SmsMessage: address→phone, body→content, msgType→direction, read→status,
 *   加入 encoding/isLong/partTotal/groupId/error/status 字段
 * - Contact: id number, note: string | null, 加入 createdAt
 * - SimInfo: 加入 phoneNumber, signalStrength(0-100), signalDbm
 * - ChannelStatus: 加入 type 字段
 */

// ============================================================
// 短信消息 (PRD §5.1 messages 表 / §5.2)
// ============================================================

/** 短信消息 */
export interface SmsMessage {
  /** 唯一消息 ID (INTEGER PRIMARY KEY AUTOINCREMENT) */
  id: number;
  /** 消息方向: 'in'(收件) / 'out'(发件) */
  direction: "in" | "out";
  /** 对方手机号码 */
  phone: string;
  /** 短信正文 */
  content: string;
  /** 编码方式，默认 'UCS2' */
  encoding: string;
  /** 长短信标记 */
  isLong: boolean;
  /** 长短信当前段序号 */
  partIndex: number;
  /** 长短信总段数 */
  partTotal: number;
  /** 长短信/群发的组 ID */
  groupId: string | null;
  /**
   * 投递状态
   * - pending: 待发送 / 收件未读
   * - sent: 已发送
   * - delivered: 已送达 / 收件已投递
   * - read: 收件已读
   * - failed: 发送失败
   */
  status: "pending" | "sent" | "delivered" | "read" | "failed";
  /** 错误信息（发送失败时） */
  error: string | null;
  /** 创建时间戳 (ISO-8601 UTC 格式 'YYYY-MM-DD HH:MM:SS') */
  timestamp: string;
}

// ============================================================
// 联系人 (PRD §5.1 contacts 表 / §5.2)
// ============================================================

/** 联系人 */
export interface Contact {
  /** INTEGER PRIMARY KEY AUTOINCREMENT */
  id: number;
  /** 姓名 */
  name: string;
  /** 手机号码（已规范化） */
  phone: string;
  /** 备注 */
  note: string | null;
  /** 创建时间 (ISO-8601，可选) */
  createdAt?: string;
}

// ============================================================
// SIM 卡信息 (PRD §5.2)
// ============================================================

/** SIM 卡信息 */
export interface SimInfo {
  /** 运营商 */
  carrier: string;
  /** ICCID */
  iccid: string;
  /** IMSI */
  imsi: string;
  /** 手机号码（可为 null） */
  phoneNumber: string | null;
  /** 信号强度百分比 (0-100) */
  signalStrength: number;
  /** 信号强度 dBm（负值，-120 ~ -30） */
  signalDbm: number;
  /** 网络类型 'LTE' | 'HSPA+' | '3G' | '2G' */
  networkType: string;
}

// ============================================================
// 通道状态 (PRD §5.2)
// ============================================================

/** 通道状态 */
export interface ChannelStatus {
  /** 通道类型: 'winrt' | 'at' */
  type: "winrt" | "at";
  /** 是否就绪 */
  available: boolean;
  /** COM 端口（AT 通道使用，null = 不适用） */
  comPort: string | null;
  /** 错误信息 */
  error: string | null;
}

// ============================================================
// 应用级配置（M5 接入 settings 表）
// ============================================================

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

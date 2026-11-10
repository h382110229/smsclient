/**
 * SMSClient — Utility Functions
 * Worker C「前端骨架」— 2026-09-22
 * M2 更新: formatTimestamp 支持 ISO-8601 字符串 (PRD §5 timestamp)。
 *
 * cn() — clsx + tailwind-merge 组合，Shadcn/ui 标准工具。
 */
import { type ClassValue, clsx } from "clsx";
import { twMerge } from "tailwind-merge";
import type { MergedMessage, SmsMessage } from "./types";

/** 合并 Tailwind 类名（去重 + 冲突合并） */
export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

/**
 * 格式化时间戳为本地可读字符串（正确处理时区，解决 8 小时偏差）。
 * 支持输入：
 * - ISO-8601 UTC 字符串 'YYYY-MM-DDTHH:MM:SSZ'
 * - SQLite 默认 UTC 字符串 'YYYY-MM-DD HH:MM:SS'
 * - Unix 秒数（兼容旧数据）
 */
export function formatTimestamp(ts: string | number | null | undefined): string {
  if (!ts) return "";
  let date: Date;
  if (typeof ts === "number") {
    date = new Date(ts * 1000);
  } else {
    let str = ts.trim();
    if (!str.includes("T")) {
      str = str.replace(" ", "T");
    }
    // 若无时区后缀，补充 'Z' 确保 JS 视作 UTC 并准确转换为本地系统时区
    if (!str.endsWith("Z") && !/[+-]\d{2}(:\d{2})?$/.test(str)) {
      str += "Z";
    }
    date = new Date(str);
  }
  if (isNaN(date.getTime())) {
    return String(ts);
  }
  return new Intl.DateTimeFormat("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  }).format(date);
}

/** 信号强度 dBm → 满格百分比 (简化映射) */
export function signalToPercent(dbm: number): number {
  // -50 dBm → 100%, -100 dBm → 0%
  const clamped = Math.max(-100, Math.min(-50, dbm));
  return Math.round(((clamped + 100) / 50) * 100);
}

/** 手机号脱敏: 138****8000 */
export function maskPhone(phone: string): string {
  if (phone.length < 7) return phone;
  const prefix = phone.slice(0, 3);
  const suffix = phone.slice(-4);
  return `${prefix}****${suffix}`;
}

/** 生成 UUID v4 (无连字符) */
export function uuid(): string {
  return crypto.randomUUID().replace(/-/g, "");
}

/** 手机号规范化（与 Rust contacts::normalize_phone 逻辑对齐） */
export function normalizePhone(raw: string): string {
  let s = raw.trim().replace(/[\s\-()]/g, "");
  if (s.startsWith("+86") && s.length - 3 >= 11) {
    s = s.slice(3);
  } else if (s.startsWith("86") && s.length - 2 >= 11) {
    s = s.slice(2);
  }
  return s;
}

/** 简单手机号验证（中国大陆 11 位或短号如 10086） */
export function isValidPhone(phone: string): boolean {
  const p = normalizePhone(phone);
  return /^1\d{10}$/.test(p) || /^\d{3,6}$/.test(p);
}

/** 解析短信时间戳为毫秒数值 */
export function parseMsgTime(ts: string): number {
  if (!ts) return 0;
  let str = ts.trim();
  if (!str.includes("T")) str = str.replace(" ", "T");
  if (!str.endsWith("Z") && !/[+-]\d{2}(:\d{2})?$/.test(str)) str += "Z";
  const t = new Date(str).getTime();
  return isNaN(t) ? 0 : t;
}

/**
 * 将长短信分片（Concatenated SMS）按语序合并为单条完整短信
 * 解决长短信切片后显示成多行多气泡、断句阅读不便的问题。
 */
export function mergeLongMessages(messages: SmsMessage[]): MergedMessage[] {
  if (!messages || messages.length === 0) return [];

  // 1. 严格按时间升序 -> partIndex 升序 -> id 升序排列
  const sorted = [...messages].sort((a, b) => {
    const diff = parseMsgTime(a.timestamp) - parseMsgTime(b.timestamp);
    if (diff !== 0) return diff;
    if (a.partIndex !== b.partIndex) return a.partIndex - b.partIndex;
    return a.id - b.id;
  });

  const merged: MergedMessage[] = [];

  for (const msg of sorted) {
    if (merged.length === 0) {
      merged.push({
        id: msg.id,
        direction: msg.direction,
        phone: msg.phone,
        content: msg.content,
        timestamp: msg.timestamp,
        status: msg.status,
        rawIds: [msg.id],
        isLong: msg.isLong,
        partTotal: msg.partTotal,
        groupId: msg.groupId,
      });
      continue;
    }

    const prev = merged[merged.length - 1];
    const timeDiff = Math.abs(parseMsgTime(msg.timestamp) - parseMsgTime(prev.timestamp));

    // 严格判定是否属于同一条长短信的后续分片（Gemini 4 Bug 2：绝不合并两条独立的短消息！）
    // 条件 1: 具有相同的 groupId 且时间差在 120 秒内；
    // 条件 2: 具有 isLong/partTotal > 1 标记、同一方向且时间差在 60 秒内；
    // 条件 3: 时间差在 10 秒内且存在明确的分片序号或长短信标记。
    const isSameGroup =
      msg.direction === prev.direction &&
      msg.phone === prev.phone &&
      ((msg.groupId && prev.groupId && msg.groupId === prev.groupId && timeDiff <= 120000) ||
        (msg.partTotal > 1 && prev.partTotal === msg.partTotal && timeDiff <= 60000) ||
        (timeDiff <= 10000 && (msg.partIndex > 1 || msg.isLong || prev.isLong)));

    if (isSameGroup) {
      // 拼接正文
      prev.content += msg.content;
      prev.rawIds.push(msg.id);
      // 若任一分片未读，则聚合消息视为未读
      if (msg.status !== "read" && prev.status === "read") {
        prev.status = msg.status;
      }
      prev.timestamp = msg.timestamp;
    } else {
      merged.push({
        id: msg.id,
        direction: msg.direction,
        phone: msg.phone,
        content: msg.content,
        timestamp: msg.timestamp,
        status: msg.status,
        rawIds: [msg.id],
        isLong: msg.isLong,
        partTotal: msg.partTotal,
        groupId: msg.groupId,
      });
    }
  }

  return merged;
}

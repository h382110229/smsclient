/**
 * SMSClient — Utility Functions
 * Worker C「前端骨架」— 2026-09-22
 * M2 更新: formatTimestamp 支持 ISO-8601 字符串 (PRD §5 timestamp)。
 *
 * cn() — clsx + tailwind-merge 组合，Shadcn/ui 标准工具。
 */
import { type ClassValue, clsx } from "clsx";
import { twMerge } from "tailwind-merge";

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

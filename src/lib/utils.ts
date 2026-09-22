/**
 * SMSClient — Utility Functions
 * Worker C「前端骨架」— 2026-09-22
 *
 * cn() — clsx + tailwind-merge 组合，Shadcn/ui 标准工具。
 */
import { type ClassValue, clsx } from "clsx";
import { twMerge } from "tailwind-merge";

/** 合并 Tailwind 类名（去重 + 冲突合并） */
export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

/** 格式化 Unix 时间戳为本地可读字符串 */
export function formatTimestamp(ts: number): string {
  const date = new Date(ts * 1000);
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

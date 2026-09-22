/**
 * SMSClient — Tauri Bridge (invoke 封装)
 *
 * PRD §4.4 lib/tauri-bridge.ts：统一管理 Tauri invoke 调用。
 * M2 联系人 CRUD + 消息查询 + 设置存取的类型安全封装。
 */
import { invoke } from "@tauri-apps/api/core";
import type { Contact, SimInfo, SmsMessage } from "./types";

// ============================================================
// SIM / 信号 (M1 保留)
// ============================================================

/** 获取 SIM 卡信息（含信号强度百分比与 dBm） */
export async function getSimInfo(): Promise<SimInfo> {
  return invoke<SimInfo>("get_sim_info");
}

/** 获取信号强度 (dBm) */
export async function getSignalStrength(): Promise<number> {
  return invoke<number>("get_signal_strength");
}

// ============================================================
// 联系人 CRUD (M2)
// ============================================================

/** 列出联系人，search 非空时模糊匹配姓名/号码 */
export async function listContacts(search?: string): Promise<Contact[]> {
  return invoke<Contact[]>("list_contacts", { search: search || null });
}

/** 新增联系人 */
export async function addContact(
  name: string,
  phone: string,
  note?: string,
): Promise<Contact> {
  return invoke<Contact>("add_contact", { name, phone, note: note || null });
}

/** 更新联系人 */
export async function updateContact(
  id: number,
  name: string,
  phone: string,
  note?: string,
): Promise<Contact> {
  return invoke<Contact>("update_contact", {
    id,
    name,
    phone,
    note: note || null,
  });
}

/** 删除联系人 */
export async function deleteContact(id: number): Promise<void> {
  return invoke<void>("delete_contact", { id });
}

// ============================================================
// 短信 (M2)
// ============================================================

/** 获取收件箱 */
export async function getInbox(): Promise<SmsMessage[]> {
  return invoke<SmsMessage[]>("get_inbox");
}

/** 获取已发送 */
export async function getSent(): Promise<SmsMessage[]> {
  return invoke<SmsMessage[]>("get_sent");
}

/** 标记消息已读 */
export async function markMessageRead(id: number): Promise<void> {
  return invoke<void>("mark_message_read", { id });
}

/** 发送短信 (M2 mock，返回消息 ID 字符串) */
export async function sendSms(phone: string, message: string): Promise<string> {
  return invoke<string>("send_sms", { phone, message });
}

// ============================================================
// 设置 (M2)
// ============================================================

/** 读取设置 */
export async function getSetting(key: string): Promise<string | null> {
  return invoke<string | null>("get_setting", { key });
}

/** 写入设置 */
export async function setSetting(key: string, value: string): Promise<void> {
  return invoke<void>("set_setting", { key, value });
}

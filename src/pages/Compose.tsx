/**
 * SMSClient — Compose Page (写短信)
 * M3 增强: 联系人选择器 + 长短信拆分预览 + 发送确认弹窗。
 * 硬约束: 收发短信不得中断 4G 数据连接。
 */
import { useState, useCallback, useEffect, useMemo, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Send, CheckCircle, AlertCircle, User, Search, X } from "lucide-react";
import { sendSms } from "@/lib/tauri-bridge";
import type { Contact } from "@/lib/types";

/** 长短信 UCS2 编码每段字数 */
const CHARS_PER_SEGMENT = 67;

export function Compose() {
  const [phone, setPhone] = useState("");
  const [body, setBody] = useState("");
  const [sending, setSending] = useState(false);
  const [result, setResult] = useState<{
    type: "success" | "error";
    message: string;
  } | null>(null);

  // ---- 联系人选择器 ----
  const [contacts, setContacts] = useState<Contact[]>([]);
  const [pickerOpen, setPickerOpen] = useState(false);
  const [contactSearch, setContactSearch] = useState("");
  const pickerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const list = await invoke<Contact[]>("list_contacts", { search: null });
        if (!cancelled) setContacts(list);
      } catch {
        // 联系人加载失败不阻塞编写
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  // 点击外部关闭联系人下拉
  useEffect(() => {
    if (!pickerOpen) return;
    const onClickOutside = (e: MouseEvent) => {
      if (pickerRef.current && !pickerRef.current.contains(e.target as Node)) {
        setPickerOpen(false);
      }
    };
    document.addEventListener("mousedown", onClickOutside);
    return () => document.removeEventListener("mousedown", onClickOutside);
  }, [pickerOpen]);

  const filteredContacts = useMemo(() => {
    const q = contactSearch.trim().toLowerCase();
    if (!q) return contacts;
    return contacts.filter(
      (c) =>
        c.name.toLowerCase().includes(q) ||
        c.phone.toLowerCase().includes(q),
    );
  }, [contacts, contactSearch]);

  // 手机号匹配到的联系人（用于输入框旁显示姓名）
  const matchedContact = useMemo(() => {
    const p = phone.trim();
    if (!p) return null;
    return contacts.find((c) => c.phone === p) ?? null;
  }, [contacts, phone]);

  // ---- 长短信拆分预览 ----
  const charCount = body.length;
  const segmentCount = Math.ceil(charCount / CHARS_PER_SEGMENT) || 1;

  // ---- 发送确认弹窗 ----
  const [confirmOpen, setConfirmOpen] = useState(false);

  const handleSelectContact = useCallback((c: Contact) => {
    setPhone(c.phone);
    setPickerOpen(false);
    setContactSearch("");
  }, []);

  const handleSendClick = useCallback(() => {
    if (!phone.trim() || !body.trim()) return;
    setResult(null);
    setConfirmOpen(true);
  }, [phone, body]);

  const handleConfirmSend = useCallback(async () => {
    setConfirmOpen(false);
    setSending(true);
    setResult(null);
    try {
      const msgId = await sendSms(phone.trim(), body.trim());
      setResult({
        type: "success",
        message: `发送成功! 已入库, ID: ${msgId}`,
      });
      setBody(""); // 仅清空内容，保留收件人便于连续发送
    } catch (e) {
      setResult({ type: "error", message: `发送失败: ${e}` });
    } finally {
      setSending(false);
    }
  }, [phone, body]);

  return (
    <div className="space-y-6 max-w-2xl">
      <div>
        <h2 className="text-2xl font-bold text-foreground">写短信</h2>
        <p className="text-sm text-muted-foreground mt-1">
          通过 WinRT SMS API 发送短信（4G 数据不中断）
        </p>
      </div>

      {/* Phone Input + 联系人选择器 */}
      <div className="space-y-2" ref={pickerRef}>
        <label className="text-sm font-medium text-foreground">收件人</label>
        <div className="relative">
          <input
            type="tel"
            value={phone}
            onChange={(e) => setPhone(e.target.value)}
            onFocus={() => setPickerOpen(true)}
            placeholder="输入手机号码，如 13800138000"
            className="w-full px-3 py-2.5 rounded-md border border-border bg-card text-foreground text-sm placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-ring"
          />
          {/* 匹配联系人时显示姓名 */}
          {matchedContact && (
            <span className="absolute right-3 top-1/2 -translate-y-1/2 flex items-center gap-1 text-xs text-brand-accent bg-accent px-2 py-0.5 rounded-md">
              <User size={12} />
              {matchedContact.name}
            </span>
          )}
        </div>
        {matchedContact && (
          <p className="text-xs text-muted-foreground">
            联系人: {matchedContact.name}
            {matchedContact.note ? `（${matchedContact.note}）` : ""}
          </p>
        )}

        {/* 联系人下拉 */}
        {pickerOpen && (
          <div className="rounded-md border border-border bg-card shadow-md overflow-hidden">
            <div className="flex items-center gap-2 px-3 py-2 border-b border-border">
              <Search size={14} className="text-muted-foreground" />
              <input
                value={contactSearch}
                onChange={(e) => setContactSearch(e.target.value)}
                placeholder="搜索联系人姓名或号码..."
                className="flex-1 bg-transparent text-sm text-foreground placeholder:text-muted-foreground focus:outline-none"
              />
              <button
                onClick={() => setPickerOpen(false)}
                className="text-muted-foreground hover:text-foreground"
                aria-label="关闭联系人列表"
              >
                <X size={14} />
              </button>
            </div>
            <ul className="max-h-56 overflow-y-auto">
              {filteredContacts.length === 0 ? (
                <li className="px-3 py-4 text-sm text-muted-foreground text-center">
                  无匹配联系人
                </li>
              ) : (
                filteredContacts.map((c) => (
                  <li key={c.id}>
                    <button
                      onClick={() => handleSelectContact(c)}
                      className="w-full flex items-center gap-2 px-3 py-2 text-left text-sm hover:bg-accent transition-colors"
                    >
                      <User size={14} className="text-muted-foreground shrink-0" />
                      <span className="text-foreground font-medium">{c.name}</span>
                      <span className="text-muted-foreground">{c.phone}</span>
                    </button>
                  </li>
                ))
              )}
            </ul>
          </div>
        )}
      </div>

      {/* Body Input */}
      <div className="space-y-2">
        <div className="flex items-center justify-between">
          <label className="text-sm font-medium text-foreground">内容</label>
          <span
            className={`text-xs ${
              charCount > 67 ? "text-warning" : "text-muted-foreground"
            }`}
          >
            {charCount} 字
          </span>
        </div>
        <textarea
          value={body}
          onChange={(e) => setBody(e.target.value)}
          placeholder="输入短信内容..."
          rows={6}
          className="w-full px-3 py-2.5 rounded-md border border-border bg-card text-foreground text-sm placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-ring resize-none"
        />
        {charCount > 67 && (
          <p className="text-xs text-warning">
            ⚠ 将拆分为 {segmentCount} 条长短信（UCS2 编码，67 字/条）
          </p>
        )}
      </div>

      {/* Send Button */}
      <div className="flex items-center gap-4">
        <button
          onClick={handleSendClick}
          disabled={sending || !phone.trim() || !body.trim()}
          className="flex items-center gap-2 px-5 py-2.5 rounded-md bg-brand-primary text-white text-sm font-medium hover:opacity-90 disabled:opacity-50 disabled:cursor-not-allowed transition-opacity"
        >
          <Send size={14} />
          {sending ? "发送中..." : "发送"}
        </button>

        {/* Result */}
        {result && (
          <div
            className={`flex items-center gap-2 text-sm font-medium ${
              result.type === "success" ? "text-success" : "text-destructive"
            }`}
          >
            {result.type === "success" ? (
              <CheckCircle size={16} />
            ) : (
              <AlertCircle size={16} />
            )}
            <span>{result.message}</span>
          </div>
        )}
      </div>

      {/* 发送确认弹窗 */}
      {confirmOpen && (
        <div className="fixed inset-0 z-50 flex items-center justify-center">
          {/* 遮罩 */}
          <div
            className="absolute inset-0 bg-black/50"
            onClick={() => setConfirmOpen(false)}
          />
          {/* 对话框 */}
          <div className="relative w-full max-w-md mx-4 rounded-lg border border-border bg-card p-5 shadow-lg space-y-4">
            <div className="flex items-center justify-between">
              <h3 className="text-lg font-bold text-foreground">确认发送？</h3>
              <button
                onClick={() => setConfirmOpen(false)}
                className="text-muted-foreground hover:text-foreground"
                aria-label="关闭"
              >
                <X size={16} />
              </button>
            </div>

            <div className="space-y-2 text-sm">
              <div className="flex items-center gap-2">
                <User size={14} className="text-muted-foreground" />
                <span className="text-muted-foreground">收件人:</span>
                <span className="text-foreground font-medium">
                  {phone}
                  {matchedContact ? `（${matchedContact.name}）` : ""}
                </span>
              </div>
              <div>
                <span className="text-muted-foreground">内容:</span>
                <div className="mt-1 max-h-40 overflow-y-auto whitespace-pre-wrap rounded-md border border-border bg-accent px-3 py-2 text-foreground">
                  {body}
                </div>
              </div>
              {body.length > 67 && (
                <p className="text-xs text-warning">
                  ⚠ 将拆分为 {segmentCount} 条长短信（UCS2 编码，67 字/条）
                </p>
              )}
            </div>

            <div className="flex justify-end gap-3">
              <button
                onClick={() => setConfirmOpen(false)}
                className="px-4 py-2 rounded-md border border-border bg-card text-foreground text-sm font-medium hover:bg-accent transition-colors"
              >
                取消
              </button>
              <button
                onClick={handleConfirmSend}
                disabled={sending}
                className="flex items-center gap-2 px-4 py-2 rounded-md bg-brand-primary text-white text-sm font-medium hover:opacity-90 disabled:opacity-50 disabled:cursor-not-allowed transition-opacity"
              >
                <Send size={14} />
                确认发送
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

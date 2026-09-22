/**
 * SMSClient — Inbox Page (收件箱)
 * M2 更新: DB-backed (messages 表)，字段对齐 PRD §5.2
 * - phone / content / direction / status（替代旧 address / body / read）
 * - 点击消息自动标记已读
 */
import { useEffect, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { MessageCircle, RefreshCw, CheckCheck } from "lucide-react";
import type { SmsMessage } from "@/lib/types";
import { formatTimestamp } from "@/lib/utils";

export function Inbox() {
  const [messages, setMessages] = useState<SmsMessage[]>([]);
  const [loading, setLoading] = useState(true);

  const fetchInbox = useCallback(async () => {
    setLoading(true);
    try {
      const msgs = await invoke<SmsMessage[]>("get_inbox");
      setMessages(msgs);
    } catch (e) {
      console.error("Failed to fetch inbox:", e);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchInbox();
  }, [fetchInbox]);

  /** 点击消息 → 标记已读 + 刷新状态 */
  const handleRead = useCallback(async (msg: SmsMessage) => {
    if (msg.direction === "in" && msg.status !== "read") {
      try {
        await invoke<void>("mark_message_read", { id: msg.id });
        setMessages((prev) =>
          prev.map((m) =>
            m.id === msg.id ? { ...m, status: "read" as const } : m,
          ),
        );
      } catch (e) {
        console.error("Failed to mark as read:", e);
      }
    }
  }, []);

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-2xl font-bold text-foreground">收件箱</h2>
          <p className="text-sm text-muted-foreground mt-1">
            已收到 {messages.length} 条短信（DB 持久化）
          </p>
        </div>
        <button
          onClick={fetchInbox}
          disabled={loading}
          className="flex items-center gap-2 px-3 py-2 rounded-md text-sm font-medium border border-border bg-card text-foreground hover:bg-accent/10 transition-colors"
        >
          <RefreshCw
            size={14}
            className={loading ? "animate-spin" : ""}
          />
          刷新
        </button>
      </div>

      {/* Message List */}
      <div className="border border-border rounded-lg divide-y divide-border bg-card">
        {messages.length === 0 && !loading && (
          <div className="p-12 text-center text-muted-foreground">
            <MessageCircle size={40} className="mx-auto mb-3 opacity-30" />
            <p className="text-sm">暂无短信</p>
            <p className="text-xs mt-1">
              发送/接收短信后将显示在此
            </p>
          </div>
        )}
        {messages.map((msg) => {
          const isUnread =
            msg.direction === "in" && msg.status !== "read";
          return (
            <div
              key={msg.id}
              onClick={() => handleRead(msg)}
              className={`flex items-start gap-4 p-4 hover:bg-accent/5 transition-colors ${
                isUnread ? "cursor-pointer" : ""
              }`}
            >
              <div className="w-10 h-10 rounded-full bg-brand-primary/10 flex items-center justify-center shrink-0 mt-0.5">
                <MessageCircle
                  size={18}
                  className="text-brand-primary"
                />
              </div>
              <div className="flex-1 min-w-0">
                <div className="flex items-center gap-2 mb-1">
                  <span className="font-medium text-sm text-foreground font-mono">
                    {msg.phone}
                  </span>
                  {isUnread && (
                    <span className="inline-flex items-center px-1.5 py-0.5 rounded-full text-[10px] font-medium bg-brand-accent/10 text-brand-accent">
                      未读
                    </span>
                  )}
                  <span
                    className={`inline-flex items-center px-1.5 py-0.5 rounded-full text-[10px] font-medium ${
                      msg.direction === "out"
                        ? "bg-info/10 text-info"
                        : "bg-success/10 text-success"
                    }`}
                  >
                    {msg.direction === "out" ? "发" : "收"}
                  </span>
                </div>
                <p className="text-sm text-foreground leading-relaxed">
                  {msg.content}
                </p>
                <div className="flex items-center gap-3 mt-1.5">
                  <p className="text-xs text-muted-foreground">
                    {formatTimestamp(msg.timestamp)}
                  </p>
                  {msg.partTotal > 1 && (
                    <span className="text-xs text-muted-foreground">
                      · {msg.partTotal} 段拼接
                    </span>
                  )}
                  {msg.status === "failed" && (
                    <span className="text-xs text-destructive">发送失败</span>
                  )}
                </div>
              </div>
              {!isUnread && msg.status === "read" && (
                <CheckCheck
                  size={14}
                  className="text-brand-primary shrink-0 mt-1"
                />
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}

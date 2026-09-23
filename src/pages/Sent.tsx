/**
 * SMSClient — Sent Page (已发送)
 * M2 更新: DB-backed (messages 表 direction='out')，字段对齐 PRD §5.2
 * 增强: 失败短信支持「重发」，带加载态与成功/失败反馈
 */
import { useEffect, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Send, RefreshCw, AlertCircle, RotateCcw } from "lucide-react";
import type { SmsMessage } from "@/lib/types";
import { getSent } from "@/lib/tauri-bridge";
import { formatTimestamp } from "@/lib/utils";

const STATUS_LABEL: Record<SmsMessage["status"], { text: string; cls: string }> = {
  pending: { text: "待发送", cls: "bg-warning/10 text-warning" },
  sent: { text: "已发送", cls: "bg-info/10 text-info" },
  delivered: { text: "已送达", cls: "bg-success/10 text-success" },
  read: { text: "已读", cls: "bg-success/10 text-success" },
  failed: { text: "失败", cls: "bg-destructive/10 text-destructive" },
};

interface RetryFeedback {
  kind: "success" | "error";
  text: string;
}

export function Sent() {
  const [messages, setMessages] = useState<SmsMessage[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [retryingId, setRetryingId] = useState<string | number | null>(null);
  const [feedback, setFeedback] = useState<{ id: string | number } & RetryFeedback | null>(null);

  const fetchSent = useCallback(async () => {
    setLoading(true);
    try {
      setMessages(await getSent());
      setError(null);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchSent();
  }, [fetchSent]);

  const handleRetry = useCallback(
    async (msg: SmsMessage) => {
      if (retryingId !== null) return;
      setRetryingId(msg.id);
      setFeedback(null);
      try {
        await invoke("send_sms", { phone: msg.phone, message: msg.content });
        // 成功：本地将状态更新为 sent
        setMessages((prev) =>
          prev.map((m) => (m.id === msg.id ? { ...m, status: "sent" as const, error: null } : m))
        );
        setFeedback({ id: msg.id, kind: "success", text: "重发成功" });
      } catch (e) {
        setFeedback({ id: msg.id, kind: "error", text: `重发失败：${String(e)}` });
      } finally {
        setRetryingId(null);
        // 反馈短暂展示后自动清除
        setTimeout(() => setFeedback(null), 3000);
      }
    },
    [retryingId]
  );

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-2xl font-bold text-foreground">已发送</h2>
          <p className="text-sm text-muted-foreground mt-1">
            {messages.length} 条已发送短信（DB 持久化）
          </p>
        </div>
        <button
          onClick={fetchSent}
          disabled={loading}
          className="flex items-center gap-2 px-3 py-2 rounded-md text-sm font-medium border border-border bg-card text-foreground hover:bg-accent/10 transition-colors"
        >
          <RefreshCw size={14} className={loading ? "animate-spin" : ""} />
          刷新
        </button>
      </div>

      {error && (
        <div className="flex items-center gap-2 p-3 rounded-md bg-destructive/10 text-destructive border border-destructive/20 text-sm">
          <AlertCircle size={14} />
          <span>{error}</span>
        </div>
      )}

      <div className="border border-border rounded-lg divide-y divide-border bg-card">
        {messages.length === 0 && !loading && (
          <div className="p-12 text-center text-muted-foreground">
            <Send size={40} className="mx-auto mb-3 opacity-30" />
            <p className="text-sm">暂无已发送短信</p>
            <p className="text-xs mt-1">在「写短信」页发送后将显示在此</p>
          </div>
        )}
        {messages.map((msg) => {
          const st = STATUS_LABEL[msg.status] ?? {
            text: msg.status,
            cls: "bg-muted text-muted-foreground",
          };
          const isFailed = msg.status === "failed";
          const isRetrying = retryingId === msg.id;
          const fb = feedback && feedback.id === msg.id ? feedback : null;
          return (
            <div
              key={msg.id}
              className={`flex items-center gap-4 p-4 hover:bg-accent/5 transition-colors ${
                isFailed ? "border-l-2 border-l-destructive bg-destructive/5" : ""
              }`}
            >
              <div className="w-10 h-10 rounded-full bg-brand-primary/10 flex items-center justify-center shrink-0">
                <Send size={16} className="text-brand-primary" />
              </div>
              <div className="flex-1 min-w-0">
                <div className="flex items-center gap-2">
                  <span className="font-medium text-sm text-foreground font-mono">
                    {msg.phone}
                  </span>
                  <span
                    className={`inline-flex items-center px-1.5 py-0.5 rounded-full text-[10px] font-medium ${st.cls}`}
                  >
                    {st.text}
                  </span>
                  {isFailed && (
                    <span className="inline-flex items-center gap-0.5 text-[10px] text-destructive">
                      <AlertCircle size={10} />
                      可重发
                    </span>
                  )}
                  {msg.partTotal > 1 && (
                    <span className="text-[10px] text-muted-foreground">
                      {msg.partTotal} 段
                    </span>
                  )}
                </div>
                <p className="text-sm text-foreground mt-0.5 truncate">
                  {msg.content}
                </p>
                {msg.error && (
                  <p className="text-xs text-destructive mt-0.5">{msg.error}</p>
                )}
                {fb && (
                  <p
                    className={`text-xs mt-0.5 ${
                      fb.kind === "success" ? "text-success" : "text-destructive"
                    }`}
                  >
                    {fb.text}
                  </p>
                )}
              </div>
              <div className="flex items-center gap-3 shrink-0">
                {isFailed && (
                  <button
                    onClick={() => handleRetry(msg)}
                    disabled={retryingId !== null}
                    className="flex items-center gap-1.5 px-3 py-1.5 rounded-md text-xs font-medium bg-destructive/10 text-destructive border border-destructive/20 hover:bg-destructive/20 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
                  >
                    <RotateCcw size={12} className={isRetrying ? "animate-spin" : ""} />
                    {isRetrying ? "重发中…" : "重发"}
                  </button>
                )}
                <span className="text-xs text-muted-foreground">
                  {formatTimestamp(msg.timestamp)}
                </span>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}

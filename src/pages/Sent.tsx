/**
 * SMSClient — Sent Page (已发送)
 * M2 更新: DB-backed (messages 表 direction='out')，字段对齐 PRD §5.2
 */
import { useEffect, useState, useCallback } from "react";
import { Send, RefreshCw, AlertCircle } from "lucide-react";
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

export function Sent() {
  const [messages, setMessages] = useState<SmsMessage[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

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
          return (
            <div
              key={msg.id}
              className="flex items-center gap-4 p-4 hover:bg-accent/5 transition-colors"
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
              </div>
              <span className="text-xs text-muted-foreground shrink-0">
                {formatTimestamp(msg.timestamp)}
              </span>
            </div>
          );
        })}
      </div>
    </div>
  );
}

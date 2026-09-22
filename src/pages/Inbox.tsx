/**
 * SMSClient — Inbox Page (收件箱)
 * Worker C「前端骨架」— 2026-09-22
 */
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { MessageCircle, RefreshCw } from "lucide-react";
import type { SmsMessage } from "@/lib/types";
import { formatTimestamp } from "@/lib/utils";

export function Inbox() {
  const [messages, setMessages] = useState<SmsMessage[]>([]);
  const [loading, setLoading] = useState(true);

  const fetchInbox = async () => {
    setLoading(true);
    try {
      const msgs = await invoke<SmsMessage[]>("get_inbox");
      setMessages(msgs);
    } catch (e) {
      console.error("Failed to fetch inbox:", e);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchInbox();
  }, []);

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-2xl font-bold text-foreground">收件箱</h2>
          <p className="text-sm text-muted-foreground mt-1">
            已收到 {messages.length} 条短信
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
          </div>
        )}
        {messages.map((msg) => (
          <div
            key={msg.id}
            className="flex items-start gap-4 p-4 hover:bg-accent/5 transition-colors"
          >
            <div className="w-10 h-10 rounded-full bg-brand-primary/10 flex items-center justify-center shrink-0 mt-0.5">
              <MessageCircle size={18} className="text-brand-primary" />
            </div>
            <div className="flex-1 min-w-0">
              <div className="flex items-center gap-2 mb-1">
                <span className="font-medium text-sm text-foreground font-mono">
                  {msg.address}
                </span>
                {!msg.read && (
                  <span className="inline-flex items-center px-1.5 py-0.5 rounded-full text-[10px] font-medium bg-brand-accent/10 text-brand-accent">
                    未读
                  </span>
                )}
              </div>
              <p className="text-sm text-foreground leading-relaxed">
                {msg.body}
              </p>
              <p className="text-xs text-muted-foreground mt-1.5">
                {formatTimestamp(msg.timestamp)}
                {msg.parts && msg.parts > 1 && (
                  <span className="ml-2">· {msg.parts} 段拼接</span>
                )}
              </p>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

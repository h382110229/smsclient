/**
 * SMSClient — Sent Page (已发送)
 * Worker C「前端骨架」— 2026-09-22
 */
import { Send, ExternalLink } from "lucide-react";

// Mock data for skeleton display
const mockSent = [
  {
    id: "sent-001",
    address: "10086",
    body: "CXLL",
    timestamp: 1758510000,
    status: "delivered" as const,
  },
  {
    id: "sent-002",
    address: "+8613900139000",
    body: "测试短信已送达",
    timestamp: 1758506400,
    status: "sent" as const,
  },
];

export function Sent() {
  return (
    <div className="space-y-6">
      <div>
        <h2 className="text-2xl font-bold text-foreground">已发送</h2>
        <p className="text-sm text-muted-foreground mt-1">
          查看已发送的短信记录（M2 接入真实数据）
        </p>
      </div>

      <div className="border border-border rounded-lg divide-y divide-border bg-card">
        {mockSent.length === 0 ? (
          <div className="p-12 text-center text-muted-foreground">
            <Send size={40} className="mx-auto mb-3 opacity-30" />
            <p className="text-sm">暂无已发送短信</p>
          </div>
        ) : (
          mockSent.map((msg) => (
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
                    {msg.address}
                  </span>
                  <span
                    className={`inline-flex items-center px-1.5 py-0.5 rounded-full text-[10px] font-medium ${
                      msg.status === "delivered"
                        ? "bg-success/10 text-success"
                        : "bg-info/10 text-info"
                    }`}
                  >
                    {msg.status === "delivered" ? "已送达" : "已发送"}
                  </span>
                </div>
                <p className="text-sm text-foreground mt-0.5">{msg.body}</p>
              </div>
              <ExternalLink
                size={14}
                className="text-muted-foreground shrink-0"
              />
            </div>
          ))
        )}
      </div>
    </div>
  );
}

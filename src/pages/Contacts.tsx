/**
 * SMSClient — Contacts Page (联系人)
 * Worker C「前端骨架」— 2026-09-22
 *
 * M3 将接入持久化联系人管理。
 */
import { UserPlus, Users } from "lucide-react";

// Mock contacts for skeleton display
const mockContacts = [
  { id: "c-1", name: "张三", phone: "13800138000", note: "同事" },
  { id: "c-2", name: "李四", phone: "13900139000", note: "朋友" },
  { id: "c-3", name: "王五", phone: "13700137000", note: "" },
];

export function Contacts() {
  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-2xl font-bold text-foreground">联系人</h2>
          <p className="text-sm text-muted-foreground mt-1">
            管理常用联系人（M3 持久化）
          </p>
        </div>
        <button className="flex items-center gap-2 px-3 py-2 rounded-md text-sm font-medium bg-brand-primary text-white hover:opacity-90 transition-opacity">
          <UserPlus size={14} />
          新建联系人
        </button>
      </div>

      <div className="border border-border rounded-lg divide-y divide-border bg-card">
        {mockContacts.map((c) => (
          <div
            key={c.id}
            className="flex items-center gap-4 p-4 hover:bg-accent/5 transition-colors"
          >
            <div className="w-10 h-10 rounded-full bg-accent/20 flex items-center justify-center shrink-0">
              <Users size={16} className="text-accent-foreground" />
            </div>
            <div className="flex-1 min-w-0">
              <p className="text-sm font-medium text-foreground">{c.name}</p>
              <p className="text-xs text-muted-foreground font-mono">
                {c.phone}
              </p>
            </div>
            {c.note && (
              <span className="text-xs text-muted-foreground bg-muted px-2 py-0.5 rounded-full">
                {c.note}
              </span>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}

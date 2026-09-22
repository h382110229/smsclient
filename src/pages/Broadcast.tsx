/**
 * SMSClient — Broadcast Page (群发)
 * Worker C「前端骨架」— 2026-09-22
 *
 * M2/M3 将接入联系人群发 + 硬约束: 不中断 4G 数据连接。
 * 群发间隔 ≥ 500ms，避免运营商限流。
 */
import { Radio } from "lucide-react";

export function Broadcast() {
  return (
    <div className="space-y-6 max-w-2xl">
      <div>
        <h2 className="text-2xl font-bold text-foreground">群发短信</h2>
        <p className="text-sm text-muted-foreground mt-1">
          向多个联系人同时发送短信（M2/M3 完整实现）
        </p>
      </div>

      {/* Placeholder */}
      <div className="border-2 border-dashed border-border rounded-lg p-12 text-center">
        <Radio size={48} className="mx-auto mb-4 text-muted-foreground/30" />
        <h3 className="text-lg font-semibold text-foreground mb-2">
          群发功能开发中
        </h3>
        <p className="text-sm text-muted-foreground max-w-md mx-auto">
          M2 阶段将实现：联系人选择器 → 群发队列 →
          逐条发送（间隔可配置）→ 送达回执汇总。
        </p>
        <div className="mt-4 inline-flex items-center px-3 py-1.5 rounded-full text-xs font-medium bg-warning/10 text-warning">
          ⚠ 群发期间 4G 连接需保持正常（PRD 硬约束）
        </div>
      </div>
    </div>
  );
}

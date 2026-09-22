/**
 * SMSClient — Dashboard Page (首页 / 仪表盘)
 * Worker C「前端骨架」— 2026-09-22
 *
 * M1 关键验收页：调用全部四个 mock command，验证 invoke 通路打通。
 * 深浅色主题切换生效 + ASTeam 品牌色可见。
 */
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Link } from "react-router-dom";
import {
  Signal,
  Smartphone,
  Inbox,
  Send,
  MessageCircle,
  AlertCircle,
} from "lucide-react";
import type { SimInfo, SmsMessage } from "@/lib/types";
import { formatTimestamp } from "@/lib/utils";

export function Dashboard() {
  const [simInfo, setSimInfo] = useState<SimInfo | null>(null);
  const [signal, setSignal] = useState<number | null>(null);
  const [inbox, setInbox] = useState<SmsMessage[]>([]);
  const [sendResult, setSendResult] = useState<string>("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    async function fetchData() {
      try {
        const [sim, sig, msgs] = await Promise.all([
          invoke<SimInfo>("get_sim_info"),
          invoke<number>("get_signal_strength"),
          invoke<SmsMessage[]>("get_inbox"),
        ]);
        if (cancelled) return;
        setSimInfo(sim);
        setSignal(sig);
        setInbox(msgs);

        // Verify send_sms command
        const result = await invoke<string>("send_sms", {
          phone: "10086",
          message: "M1 invoke 验证测试",
        });
        if (cancelled) return;
        setSendResult(result);
      } catch (e) {
        if (!cancelled) setError(String(e));
      } finally {
        if (!cancelled) setLoading(false);
      }
    }
    fetchData();
    return () => {
      cancelled = true;
    };
  }, []);

  return (
    <div className="space-y-8">
      {/* Page Header */}
      <div>
        <h2 className="text-2xl font-bold text-foreground">仪表盘</h2>
        <p className="text-sm text-muted-foreground mt-1">
          SMSClient M1 Scaffold · 所有 mock command 调用验证
        </p>
      </div>

      {/* Loading State */}
      {loading && (
        <div className="flex items-center gap-2 text-muted-foreground">
          <div className="w-4 h-4 border-2 border-brand-primary border-t-transparent rounded-full animate-spin" />
          <span className="text-sm">正在调用 Tauri commands...</span>
        </div>
      )}

      {/* Error State */}
      {error && (
        <div className="flex items-center gap-2 p-4 rounded-md bg-destructive/10 text-destructive border border-destructive/20">
          <AlertCircle size={16} />
          <span className="text-sm font-medium">
            Command 调用失败: {error}
          </span>
        </div>
      )}

      {/* Command Results Grid */}
      {!loading && (
        <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-4 gap-4">
          {/* SIM Info Card */}
          <Card
            icon={<Smartphone size={20} />}
            iconColor="text-info"
            title="SIM 卡信息"
            subtitle="invoke(&quot;get_sim_info&quot;)"
          >
            {simInfo ? (
              <div className="space-y-1 text-sm">
                <Row label="运营商" value={simInfo.carrier} />
                <Row label="网络" value={simInfo.networkType} />
                <Row label="ICCID" value={simInfo.iccid} mono />
              </div>
            ) : (
              <EmptyText />
            )}
          </Card>

          {/* Signal Card */}
          <Card
            icon={<Signal size={20} />}
            iconColor="text-success"
            title="信号强度"
            subtitle="invoke(&quot;get_signal_strength&quot;)"
          >
            {signal !== null ? (
              <div className="space-y-2">
                <p className="text-3xl font-bold font-mono text-foreground">
                  {signal} <span className="text-sm font-normal text-muted-foreground">dBm</span>
                </p>
                <div className="w-full h-2 rounded-pill bg-muted overflow-hidden">
                  <div
                    className="h-full rounded-pill bg-success transition-all"
                    style={{
                      width: `${Math.min(100, Math.max(0, ((signal + 100) / 50) * 100))}%`,
                    }}
                  />
                </div>
              </div>
            ) : (
              <EmptyText />
            )}
          </Card>

          {/* Send SMS Card */}
          <Card
            icon={<Send size={20} />}
            iconColor="text-brand-accent"
            title="发送验证"
            subtitle="invoke(&quot;send_sms&quot;)"
          >
            {sendResult ? (
              <div className="space-y-1 text-sm">
                <p className="text-success font-medium">✓ 发送成功 (Mock)</p>
                <p className="text-muted-foreground font-mono text-xs break-all">
                  {sendResult}
                </p>
              </div>
            ) : (
              <EmptyText />
            )}
          </Card>

          {/* Inbox Card */}
          <Card
            icon={<Inbox size={20} />}
            iconColor="text-warning"
            title="收件箱"
            subtitle="invoke(&quot;get_inbox&quot;)"
          >
            {inbox.length > 0 ? (
              <div className="space-y-1 text-sm">
                <p className="text-foreground font-medium">
                  {inbox.length} 条未处理消息
                </p>
                <p className="text-muted-foreground text-xs">
                  未读: {inbox.filter((m) => !m.read).length}
                </p>
              </div>
            ) : (
              <EmptyText />
            )}
          </Card>
        </div>
      )}

      {/* Recent Messages Preview */}
      {!loading && inbox.length > 0 && (
        <div className="space-y-3">
          <div className="flex items-center justify-between">
            <h3 className="text-base font-semibold text-foreground">
              最近消息
            </h3>
            <Link
              to="/inbox"
              className="text-sm text-brand-primary hover:underline"
            >
              查看全部 →
            </Link>
          </div>
          <div className="border border-border rounded-lg divide-y divide-border">
            {inbox.map((msg) => (
              <div key={msg.id} className="flex items-center gap-4 p-4">
                <div className="w-9 h-9 rounded-full bg-muted flex items-center justify-center shrink-0">
                  <MessageCircle size={16} className="text-muted-foreground" />
                </div>
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-2">
                    <span className="text-sm font-medium text-foreground font-mono">
                      {msg.address}
                    </span>
                    {!msg.read && (
                      <span className="w-2 h-2 rounded-full bg-brand-accent" />
                    )}
                  </div>
                  <p className="text-sm text-muted-foreground truncate mt-0.5">
                    {msg.body}
                  </p>
                </div>
                <span className="text-xs text-muted-foreground shrink-0">
                  {formatTimestamp(msg.timestamp)}
                </span>
              </div>
            ))}
          </div>
        </div>
      )}

      {/* Status Bar */}
      <div className="flex items-center gap-4 pt-4 border-t border-border">
        <StatusDot color="bg-success" />
        <span className="text-xs text-muted-foreground">
          4G 数据连接正常 · ping 223.5.5.5 不丢包
        </span>
        <span className="text-xs text-muted-foreground">·</span>
        <span className="text-xs text-muted-foreground">
          通道: WinRT SMS API (AT 通道预留)
        </span>
      </div>
    </div>
  );
}

/* ===== Helper Sub-Components ===== */

function Card({
  icon,
  iconColor,
  title,
  subtitle,
  children,
}: {
  icon: React.ReactNode;
  iconColor: string;
  title: string;
  subtitle: string;
  children: React.ReactNode;
}) {
  return (
    <div className="border border-border rounded-lg p-4 bg-card space-y-3">
      <div className="flex items-center gap-2">
        <span className={iconColor}>{icon}</span>
        <div>
          <h4 className="text-sm font-semibold text-foreground">{title}</h4>
          <p className="text-[10px] text-muted-foreground font-mono">
            {subtitle}
          </p>
        </div>
      </div>
      <div className="pt-2">{children}</div>
    </div>
  );
}

function Row({ label, value, mono }: { label: string; value: string; mono?: boolean }) {
  return (
    <div className="flex items-center justify-between">
      <span className="text-muted-foreground">{label}</span>
      <span className={`text-foreground ${mono ? "font-mono text-xs" : ""}`}>
        {value}
      </span>
    </div>
  );
}

function StatusDot({ color }: { color: string }) {
  return <span className={`w-2 h-2 rounded-full ${color}`} />;
}

function EmptyText() {
  return (
    <p className="text-sm text-muted-foreground italic">
      数据加载中...
    </p>
  );
}

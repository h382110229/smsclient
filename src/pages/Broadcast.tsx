/**
 * SMSClient — Broadcast Page (群发)
 * M5 完整实现：联系人多选 / 手动输入 → 群发队列 → 实时进度 → 历史记录。
 * 硬约束: 群发不中断 4G 数据连接；发送间隔 ≥ broadcast_interval（默认 6000ms = 10 条/分钟）。
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  Radio,
  Send,
  X,
  User,
  Users,
  AlertCircle,
  CheckCircle,
  Clock,
  RefreshCw,
  Square,
} from "lucide-react";

/** 长短信 UCS2 编码每段字数 */
const CHARS_PER_SEGMENT = 67;
/** 默认群发限速: 10 条/分钟 */
const BROADCAST_RATE_LIMIT = "10 条/分钟";

interface Contact {
  id: number;
  name: string;
  phone: string;
  note: string | null;
  createdAt?: string;
}

interface BroadcastTask {
  id: number;
  content: string;
  total: number;
  sent: number;
  failed: number;
  status: string; // 'pending' | 'running' | 'completed' | 'cancelled' | 'failed'
  createdAt: string | null;
}

interface BroadcastProgress {
  taskId: number;
  total: number;
  sent: number;
  failed: number;
  currentPhone?: string;
  index?: number;
  status: string;
  done: boolean;
}

type RecipientMode = "contacts" | "manual";

/** 手动输入的手机号解析：支持换行 / 逗号 / 分号 / 空格分隔 */
function parsePhones(raw: string): string[] {
  return raw
    .split(/[\s,;，；]+/g)
    .map((p) => p.trim())
    .filter((p) => p.length > 0);
}

/** 任务状态 → 展示标签与配色 */
function statusBadge(status: string): {
  label: string;
  className: string;
  icon: React.ReactNode;
} {
  switch (status) {
    case "running":
      return {
        label: "发送中",
        className: "bg-info/10 text-info",
        icon: <Clock size={11} />,
      };
    case "completed":
      return {
        label: "已完成",
        className: "bg-success/10 text-success",
        icon: <CheckCircle size={11} />,
      };
    case "cancelled":
      return {
        label: "已取消",
        className: "bg-warning/10 text-warning",
        icon: <X size={11} />,
      };
    case "failed":
      return {
        label: "失败",
        className: "bg-destructive/10 text-destructive",
        icon: <AlertCircle size={11} />,
      };
    case "pending":
      return {
        label: "待发送",
        className: "bg-accent text-muted-foreground",
        icon: <Clock size={11} />,
      };
    default:
      return {
        label: status,
        className: "bg-accent text-muted-foreground",
        icon: <Clock size={11} />,
      };
  }
}

export function Broadcast() {
  // ── 联系人数据 ─────────────────────────────────────────────
  const [contacts, setContacts] = useState<Contact[]>([]);
  const [contactsLoading, setContactsLoading] = useState(true);
  const [selectedIds, setSelectedIds] = useState<Set<number>>(new Set());
  const [recipientMode, setRecipientMode] = useState<RecipientMode>("contacts");
  const [manualPhones, setManualPhones] = useState("");

  // ── 短信内容 ───────────────────────────────────────────────
  const [content, setContent] = useState("");

  // ── 发送状态 ───────────────────────────────────────────────
  const [sending, setSending] = useState(false);
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [progress, setProgress] = useState<BroadcastProgress | null>(null);

  // ── 历史记录 ───────────────────────────────────────────────
  const [tasks, setTasks] = useState<BroadcastTask[]>([]);
  const [tasksLoading, setTasksLoading] = useState(false);

  const unlistenRef = useRef<UnlistenFn | null>(null);

  // ── 数据加载 ───────────────────────────────────────────────

  const fetchContacts = useCallback(async () => {
    setContactsLoading(true);
    try {
      const list = await invoke<Contact[]>("list_contacts", { search: null });
      setContacts(list);
    } catch (e) {
      setError(`加载联系人失败: ${e}`);
    } finally {
      setContactsLoading(false);
    }
  }, []);

  const fetchTasks = useCallback(async () => {
    setTasksLoading(true);
    try {
      const list = await invoke<BroadcastTask[]>("list_broadcast_tasks");
      setTasks(list);
    } catch (e) {
      setError(`加载群发历史失败: ${e}`);
    } finally {
      setTasksLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchContacts();
    fetchTasks();
  }, [fetchContacts, fetchTasks]);

  // ── 实时进度监听 ───────────────────────────────────────────

  useEffect(() => {
    let disposed = false;
    let unlisten: UnlistenFn | null = null;
    (async () => {
      unlisten = await listen<BroadcastProgress>(
        "broadcast-progress",
        (event) => {
          const p = event.payload;
          setProgress(p);
          if (p.done) {
            setSending(false);
            fetchTasks();
          }
        },
      );
      if (disposed) {
        unlisten();
      } else {
        unlistenRef.current = unlisten;
      }
    })();
    return () => {
      disposed = true;
      if (unlisten) unlisten();
    };
  }, [fetchTasks]);

  // ── 收件人计算 ─────────────────────────────────────────────

  const selectedContacts = useMemo(
    () => contacts.filter((c) => selectedIds.has(c.id)),
    [contacts, selectedIds],
  );

  const manualPhoneList = useMemo(
    () => parsePhones(manualPhones),
    [manualPhones],
  );

  const recipients = useMemo(() => {
    return recipientMode === "contacts"
      ? selectedContacts.map((c) => c.phone)
      : manualPhoneList;
  }, [recipientMode, selectedContacts, manualPhoneList]);

  const totalRecipients = recipients.length;

  // ── 内容分段 ───────────────────────────────────────────────
  const charCount = content.length;
  const segmentCount = Math.ceil(charCount / CHARS_PER_SEGMENT) || 1;

  // ── 联系人多选 ─────────────────────────────────────────────

  const toggleContact = useCallback((id: number) => {
    setSelectedIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }, []);

  const toggleAllContacts = useCallback(() => {
    setSelectedIds((prev) => {
      if (prev.size === contacts.length) return new Set();
      return new Set(contacts.map((c) => c.id));
    });
  }, [contacts]);

  // ── 发送流程 ───────────────────────────────────────────────

  const handleSendClick = useCallback(() => {
    if (totalRecipients === 0 || !content.trim() || sending) return;
    setError(null);
    setConfirmOpen(true);
  }, [totalRecipients, content, sending]);

  const handleConfirmSend = useCallback(async () => {
    setConfirmOpen(false);
    setSending(true);
    setError(null);
    setProgress(null);
    try {
      await invoke<number>("start_broadcast", {
        content: content.trim(),
        phones: recipients,
      });
    } catch (e) {
      setSending(false);
      setError(`启动群发失败: ${e}`);
    }
  }, [content, recipients]);

  const handleCancel = useCallback(async () => {
    try {
      await invoke<void>("cancel_broadcast");
    } catch (e) {
      setError(`取消失败: ${e}`);
    }
  }, []);

  // ── 渲染 ───────────────────────────────────────────────────

  const isRunning = sending && progress && !progress.done;
  const progressPct = progress
    ? Math.round(
        ((progress.sent + progress.failed) / Math.max(progress.total, 1)) * 100,
      )
    : 0;

  return (
    <div className="space-y-6 max-w-3xl">
      {/* Header */}
      <div>
        <h2 className="text-2xl font-bold text-foreground">群发短信</h2>
        <p className="text-sm text-muted-foreground mt-1">
          向多个联系人同时发送短信（限速 {BROADCAST_RATE_LIMIT}，不中断 4G 数据连接）
        </p>
      </div>

      {/* 错误提示 */}
      {error && (
        <div className="flex items-center gap-2 p-3 rounded-md bg-destructive/10 text-destructive border border-destructive/20 text-sm">
          <AlertCircle size={14} />
          <span>{error}</span>
        </div>
      )}

      {/* ── 收件人区域 ─────────────────────────────────────── */}
      <section className="border border-border rounded-lg bg-card divide-y divide-border">
        {/* 模式切换 */}
        <div className="flex items-center gap-1 p-3">
          <div className="flex rounded-md bg-accent p-0.5">
            <button
              onClick={() => setRecipientMode("contacts")}
              className={`flex items-center gap-1.5 px-3 py-1.5 rounded-md text-sm font-medium transition-colors ${
                recipientMode === "contacts"
                  ? "bg-brand-primary text-white"
                  : "text-muted-foreground hover:text-foreground"
              }`}
            >
              <Users size={14} />
              从联系人选择
            </button>
            <button
              onClick={() => setRecipientMode("manual")}
              className={`flex items-center gap-1.5 px-3 py-1.5 rounded-md text-sm font-medium transition-colors ${
                recipientMode === "manual"
                  ? "bg-brand-primary text-white"
                  : "text-muted-foreground hover:text-foreground"
              }`}
            >
              <User size={14} />
              手动输入
            </button>
          </div>
          <div className="ml-auto text-xs text-muted-foreground">
            已选 <span className="font-mono text-foreground">{totalRecipients}</span> 人
          </div>
        </div>

        {/* 联系人模式 */}
        {recipientMode === "contacts" && (
          <div className="p-3">
            {contactsLoading ? (
              <div className="py-8 text-center text-sm text-muted-foreground">
                加载联系人中...
              </div>
            ) : contacts.length === 0 ? (
              <div className="py-8 text-center text-muted-foreground">
                <Users size={32} className="mx-auto mb-2 opacity-30" />
                <p className="text-sm">暂无联系人</p>
                <p className="text-xs mt-1">
                  请先到「联系人」页添加联系人
                </p>
              </div>
            ) : (
              <div className="space-y-2">
                <div className="flex items-center justify-between text-xs text-muted-foreground">
                  <label className="flex items-center gap-2 cursor-pointer select-none">
                    <input
                      type="checkbox"
                      checked={
                        selectedIds.size === contacts.length &&
                        contacts.length > 0
                      }
                      onChange={toggleAllContacts}
                      className="rounded border-border text-brand-primary focus:ring-ring"
                    />
                    <span>
                      全选（已选 {selectedIds.size} / {contacts.length}）
                    </span>
                  </label>
                  {selectedIds.size > 0 && (
                    <button
                      onClick={() => setSelectedIds(new Set())}
                      className="text-muted-foreground hover:text-foreground"
                    >
                      清空选择
                    </button>
                  )}
                </div>
                <ul className="max-h-64 overflow-y-auto divide-y divide-border rounded-md border border-border">
                  {contacts.map((c) => (
                    <li key={c.id}>
                      <label
                        className={`flex items-center gap-3 px-3 py-2 cursor-pointer transition-colors ${
                          selectedIds.has(c.id)
                            ? "bg-brand-primary/5"
                            : "hover:bg-accent/5"
                        }`}
                      >
                        <input
                          type="checkbox"
                          checked={selectedIds.has(c.id)}
                          onChange={() => toggleContact(c.id)}
                          className="rounded border-border text-brand-primary focus:ring-ring"
                        />
                        <div className="flex-1 min-w-0">
                          <p className="text-sm font-medium text-foreground">
                            {c.name}
                          </p>
                          <p className="text-xs text-muted-foreground font-mono">
                            {c.phone}
                          </p>
                        </div>
                        {c.note && (
                          <span className="text-xs text-muted-foreground bg-accent px-2 py-0.5 rounded-full shrink-0">
                            {c.note}
                          </span>
                        )}
                      </label>
                    </li>
                  ))}
                </ul>
              </div>
            )}
          </div>
        )}

        {/* 手动输入模式 */}
        {recipientMode === "manual" && (
          <div className="p-3 space-y-2">
            <label className="text-sm font-medium text-foreground">
              手机号码
            </label>
            <textarea
              value={manualPhones}
              onChange={(e) => setManualPhones(e.target.value)}
              placeholder={"每行一个号码，或用逗号分隔\n例如:\n13800138000\n13900139000, 13700137000"}
              rows={5}
              className="w-full px-3 py-2.5 rounded-md border border-border bg-card text-foreground text-sm font-mono placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-ring resize-none"
            />
            {manualPhones.trim() && (
              <p className="text-xs text-muted-foreground">
                解析到{" "}
                <span className="font-mono text-foreground">
                  {manualPhoneList.length}
                </span>{" "}
                个号码
              </p>
            )}
          </div>
        )}
      </section>

      {/* ── 内容区域 ───────────────────────────────────────── */}
      <section className="space-y-2">
        <div className="flex items-center justify-between">
          <label className="text-sm font-medium text-foreground">
            短信内容
          </label>
          <span
            className={`text-xs ${
              charCount > CHARS_PER_SEGMENT
                ? "text-warning"
                : "text-muted-foreground"
            }`}
          >
            {charCount} 字 / {segmentCount} 段
          </span>
        </div>
        <textarea
          value={content}
          onChange={(e) => setContent(e.target.value)}
          placeholder="输入短信内容..."
          rows={5}
          className="w-full px-3 py-2.5 rounded-md border border-border bg-card text-foreground text-sm placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-ring resize-none"
        />
        {charCount > CHARS_PER_SEGMENT && (
          <p className="text-xs text-warning">
            ⚠ 将拆分为 {segmentCount} 条长短信（UCS2 编码，67 字/条）
          </p>
        )}
      </section>

      {/* ── 发送按钮 + 确认 ────────────────────────────────── */}
      <div className="flex items-center gap-4">
        <button
          onClick={handleSendClick}
          disabled={sending || totalRecipients === 0 || !content.trim()}
          className="flex items-center gap-2 px-5 py-2.5 rounded-md bg-brand-primary text-white text-sm font-medium hover:opacity-90 disabled:opacity-50 disabled:cursor-not-allowed transition-opacity"
        >
          <Send size={14} />
          {sending ? "发送中..." : `群发 (${totalRecipients} 人)`}
        </button>
        <p className="text-xs text-muted-foreground">
          限速 {BROADCAST_RATE_LIMIT}，发送期间请勿关闭应用
        </p>
      </div>

      {/* ── 实时进度 ───────────────────────────────────────── */}
      {isRunning && progress && (
        <section className="border border-brand-primary/30 rounded-lg bg-brand-primary/5 p-4 space-y-3">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-semibold text-foreground flex items-center gap-2">
              <RefreshCw size={14} className="animate-spin" />
              正在群发...
            </h3>
            <button
              onClick={handleCancel}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-md text-sm font-medium bg-destructive text-white hover:opacity-90 transition-opacity"
            >
              <Square size={12} />
              停止发送
            </button>
          </div>

          {/* 进度条 */}
          <div className="w-full h-2 rounded-full bg-accent overflow-hidden">
            <div
              className="h-full rounded-full bg-brand-primary transition-all duration-300"
              style={{ width: `${progressPct}%` }}
            />
          </div>

          {/* 统计 */}
          <div className="flex items-center gap-4 text-sm">
            <span className="text-success">
              已发送 <span className="font-mono">{progress.sent}</span>
            </span>
            <span className="text-destructive">
              失败 <span className="font-mono">{progress.failed}</span>
            </span>
            <span className="text-muted-foreground">
              总数 <span className="font-mono">{progress.total}</span>
            </span>
            <span className="ml-auto text-xs text-muted-foreground font-mono">
              {progressPct}%
            </span>
          </div>

          {/* 当前发送号码 */}
          {progress.currentPhone && (
            <div className="flex items-center gap-2 text-xs text-muted-foreground">
              <Clock size={12} />
              <span>
                正在发送至{" "}
                <span className="font-mono text-foreground">
                  {progress.currentPhone}
                </span>
                {progress.index !== undefined && (
                  <span className="ml-1">
                    （{progress.index + 1}/{progress.total}）
                  </span>
                )}
              </span>
            </div>
          )}
        </section>
      )}

      {/* 发送确认弹窗 */}
      {confirmOpen && (
        <div className="fixed inset-0 z-50 flex items-center justify-center">
          <div
            className="absolute inset-0 bg-black/50"
            onClick={() => setConfirmOpen(false)}
          />
          <div className="relative w-full max-w-md mx-4 rounded-lg border border-border bg-card p-5 shadow-lg space-y-4">
            <div className="flex items-center justify-between">
              <h3 className="text-lg font-bold text-foreground">
                确认群发？
              </h3>
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
                <Users size={14} className="text-muted-foreground" />
                <span className="text-muted-foreground">收件人:</span>
                <span className="text-foreground font-medium">
                  {totalRecipients} 人
                </span>
              </div>
              <div>
                <span className="text-muted-foreground">内容:</span>
                <div className="mt-1 max-h-40 overflow-y-auto whitespace-pre-wrap rounded-md border border-border bg-accent px-3 py-2 text-foreground">
                  {content}
                </div>
              </div>
              {charCount > CHARS_PER_SEGMENT && (
                <p className="text-xs text-warning">
                  ⚠ 将拆分为 {segmentCount} 条长短信（UCS2 编码，67 字/条）
                </p>
              )}
              <p className="text-xs text-muted-foreground">
                限速 {BROADCAST_RATE_LIMIT}，预计{" "}
                <span className="font-mono">
                  {Math.max(Math.ceil(totalRecipients / 10), 1)}
                </span>{" "}
                分钟内完成
              </p>
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
                确认群发
              </button>
            </div>
          </div>
        </div>
      )}

      {/* ── 群发历史 ───────────────────────────────────────── */}
      <section className="space-y-3">
        <div className="flex items-center justify-between">
          <h3 className="text-sm font-semibold text-foreground">
            群发历史
          </h3>
          <button
            onClick={fetchTasks}
            disabled={tasksLoading}
            className="flex items-center gap-1.5 px-2 py-1 rounded text-xs font-medium text-muted-foreground hover:text-foreground hover:bg-accent/10 transition-colors disabled:opacity-50"
          >
            <RefreshCw size={12} className={tasksLoading ? "animate-spin" : ""} />
            刷新
          </button>
        </div>
        <div className="border border-border rounded-lg bg-card divide-y divide-border">
          {tasksLoading && tasks.length === 0 ? (
            <div className="py-8 text-center text-sm text-muted-foreground">
              加载历史中...
            </div>
          ) : tasks.length === 0 ? (
            <div className="py-8 text-center text-muted-foreground">
              <Radio size={32} className="mx-auto mb-2 opacity-30" />
              <p className="text-sm">暂无群发记录</p>
            </div>
          ) : (
            tasks.map((t) => {
              const badge = statusBadge(t.status);
              return (
                <div key={t.id} className="p-3 space-y-1.5">
                  <div className="flex items-center gap-2">
                    <span
                      className={`inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-xs font-medium ${badge.className}`}
                    >
                      {badge.icon}
                      {badge.label}
                    </span>
                    <span className="text-xs text-muted-foreground font-mono">
                      #{t.id}
                    </span>
                    {t.createdAt && (
                      <span className="ml-auto text-xs text-muted-foreground font-mono">
                        {t.createdAt}
                      </span>
                    )}
                  </div>
                  <p className="text-sm text-foreground line-clamp-2">
                    {t.content}
                  </p>
                  <div className="flex items-center gap-3 text-xs">
                    <span className="text-success">
                      已发送 <span className="font-mono">{t.sent}</span>
                    </span>
                    <span className="text-destructive">
                      失败 <span className="font-mono">{t.failed}</span>
                    </span>
                    <span className="text-muted-foreground">
                      总数 <span className="font-mono">{t.total}</span>
                    </span>
                  </div>
                </div>
              );
            })
          )}
        </div>
      </section>

      {/* ── 限速提示 ───────────────────────────────────────── */}
      <div className="inline-flex items-center gap-2 px-3 py-1.5 rounded-full text-xs font-medium bg-warning/10 text-warning">
        <AlertCircle size={12} />
        限速 {BROADCAST_RATE_LIMIT}，避免运营商限流（PRD 硬约束：不中断 4G）
      </div>
    </div>
  );
}

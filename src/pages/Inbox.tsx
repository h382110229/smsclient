/**
 * SMSClient — Inbox Page (收件箱)
 * 具备机构语义提取、验证码高亮、智能无感切焦与分片聚合的对话视窗
 */
import { useEffect, useState, useCallback, useMemo, useRef } from "react";
import { useSearchParams } from "react-router-dom";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { MergedMessage, SmsMessage } from "@/lib/types";
import { formatTimestamp, mergeLongMessages, parseMsgTime } from "@/lib/utils";
import {
  parseSmsContent,
  resolveConversationDisplayName,
} from "@/lib/sms-parser";
import {
  MessageCircle,
  RefreshCw,
  CheckCheck,
  Search,
  ArrowLeft,
  ShieldCheck,
  Copy,
  Check,
  Sparkles,
} from "lucide-react";

/* ------------------------------------------------------------------ */
/*  Types                                                              */
/* ------------------------------------------------------------------ */

interface Conversation {
  phone: string;
  messages: MergedMessage[];
  lastMessage: MergedMessage;
  unreadCount: number;
}

/* ------------------------------------------------------------------ */
/*  Helpers                                                            */
/* ------------------------------------------------------------------ */

function buildConversations(messages: SmsMessage[]): Conversation[] {
  const map = new Map<string, SmsMessage[]>();

  for (const msg of messages) {
    const existing = map.get(msg.phone);
    if (existing) {
      existing.push(msg);
    } else {
      map.set(msg.phone, [msg]);
    }
  }

  const conversations: Conversation[] = [];

  for (const [phone, rawMsgs] of map) {
    // 自动将长短信分片拼合为完整消息（解决分片多行显示、断句影响阅读的问题）
    const mergedMsgs = mergeLongMessages(rawMsgs);
    if (mergedMsgs.length === 0) continue;

    const lastMessage = mergedMsgs[mergedMsgs.length - 1];
    const unreadCount = mergedMsgs.filter(
      (m) => m.direction === "in" && m.status !== "read",
    ).length;

    conversations.push({ phone, messages: mergedMsgs, lastMessage, unreadCount });
  }

  // 按最新短信时间倒序排列，最新收到的会话严格置顶
  conversations.sort(
    (a, b) => parseMsgTime(b.lastMessage.timestamp) - parseMsgTime(a.lastMessage.timestamp),
  );

  return conversations;
}

function truncate(text: string, max: number): string {
  return text.length > max ? text.slice(0, max) + "…" : text;
}

/* ------------------------------------------------------------------ */
/*  Component                                                          */
/* ------------------------------------------------------------------ */

export function Inbox() {
  const [searchParams] = useSearchParams();
  const [messages, setMessages] = useState<SmsMessage[]>([]);
  const [loading, setLoading] = useState(true);
  const [selectedPhone, setSelectedPhone] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState("");
  const [showMobileThread, setShowMobileThread] = useState(false);
  const [newArrivalPhone, setNewArrivalPhone] = useState<string | null>(null);
  const [copiedOtpId, setCopiedOtpId] = useState<number | null>(null);

  const threadEndRef = useRef<HTMLDivElement>(null);
  const markReadTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const newArrivalTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  /* ---------- URL query param initialization ---------- */

  useEffect(() => {
    const queryPhone = searchParams.get("phone");
    if (queryPhone) {
      setSelectedPhone(queryPhone);
      setShowMobileThread(true);
    }
  }, [searchParams]);

  /* ---------- data fetching ---------- */

  const fetchInbox = useCallback(async (silent = false) => {
    if (!silent) setLoading(true);
    try {
      if (!silent) {
        // 用户主动点击“刷新”时，触发硬件卡槽主动拉取同步（带超时保护，避免仅读本地缓存）
        await invoke("sync_device_inbox").catch((e) => {
          console.warn("sync_device_inbox error (falling back to local db):", e);
        });
      }
      const msgs = await invoke<SmsMessage[]>("get_inbox");
      setMessages(msgs);
    } catch (e) {
      console.error("Failed to fetch inbox:", e);
    } finally {
      if (!silent) setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchInbox();
  }, [fetchInbox]);

  /* ---------- real-time listener & heartbeat fallback ---------- */

  useEffect(() => {
    const unlisten = listen("sms-received", () => {
      fetchInbox(true);
    });

    // 心跳定时器兜底（每 8 秒静默同步一次），防止 WinRT 事件丢包导致断收
    const interval = setInterval(() => {
      fetchInbox(true);
    }, 8000);

    return () => {
      unlisten.then((fn) => fn());
      clearInterval(interval);
    };
  }, [fetchInbox]);

  /* ---------- conversations ---------- */

  const conversations = useMemo(() => buildConversations(messages), [messages]);

  const filteredConversations = useMemo(() => {
    if (!searchQuery.trim()) return conversations;
    const q = searchQuery.toLowerCase();
    return conversations.filter(
      (c) =>
        c.phone.toLowerCase().includes(q) ||
        c.messages.some((m) => m.content.toLowerCase().includes(q)),
    );
  }, [conversations, searchQuery]);

  const selectedConversation = useMemo(
    () => conversations.find((c) => c.phone === selectedPhone) ?? null,
    [conversations, selectedPhone],
  );

  /* ---------- mark as read (Gemini 4 Bug 3 修复：延迟 1.5s 留存未读红点) ---------- */

  const markConversationRead = useCallback(
    async (phone: string) => {
      if (!phone) return;
      // 乐观更新本地未读状态
      setMessages((prev) =>
        prev.map((m) =>
          m.phone === phone && m.direction === "in" && m.status !== "read"
            ? { ...m, status: "read" as const }
            : m,
        ),
      );
      // 调用后端原子批量单次 SQL 极速更新
      await invoke("mark_conversation_read", { phone }).catch((e) => {
        console.error("mark_conversation_read failed:", e);
      });
    },
    [],
  );

  useEffect(() => {
    if (!selectedPhone) return;

    if (markReadTimerRef.current) clearTimeout(markReadTimerRef.current);

    // 仅当窗口获得焦点时才自动标记已读，并延迟 1500ms 留存未读红点视觉感知
    markReadTimerRef.current = setTimeout(() => {
      if (document.hasFocus()) {
        markConversationRead(selectedPhone);
      }
    }, 1500);

    return () => {
      if (markReadTimerRef.current) clearTimeout(markReadTimerRef.current);
    };
  }, [selectedPhone, messages, markConversationRead]);

  /* ---------- 智能事件监听：自动切焦 + 呼吸高亮动效 (Gemini 4 裁定) ---------- */

  useEffect(() => {
    const handleNewMessage = (e: Event) => {
      const customEvent = e as CustomEvent<{
        phone: string;
        content: string;
        isOtp: boolean;
        otpCode: string | null;
      }>;
      const detail = customEvent.detail;
      if (!detail || !detail.phone) return;

      // 1. 触发左侧卡片 4 秒呼吸高亮光晕
      setNewArrivalPhone(detail.phone);
      if (newArrivalTimerRef.current) clearTimeout(newArrivalTimerRef.current);
      newArrivalTimerRef.current = setTimeout(() => {
        setNewArrivalPhone(null);
      }, 4500);

      // 2. 验证码短信智能自动切焦（Auto Focus）保护条件判定
      if (detail.isOtp) {
        const activeTag = document.activeElement?.tagName?.toLowerCase();
        const isUserTyping = activeTag === "input" || activeTag === "textarea";
        // 若用户当前没有在输入框键入，则直接平滑自动聚焦到该验证码会话
        if (!isUserTyping) {
          setSelectedPhone(detail.phone);
          setShowMobileThread(true);
        }
      }
    };

    const handleSelectPhone = (e: Event) => {
      const customEvent = e as CustomEvent<{ phone: string }>;
      if (customEvent.detail?.phone) {
        setSelectedPhone(customEvent.detail.phone);
        setShowMobileThread(true);
      }
    };

    window.addEventListener("smsclient:new-message", handleNewMessage);
    window.addEventListener("smsclient:select-phone", handleSelectPhone);

    return () => {
      window.removeEventListener("smsclient:new-message", handleNewMessage);
      window.removeEventListener("smsclient:select-phone", handleSelectPhone);
    };
  }, []);

  /* ---------- select conversation ---------- */

  const handleSelectConversation = useCallback(
    (phone: string) => {
      setSelectedPhone(phone);
      setShowMobileThread(true);
      markConversationRead(phone);
    },
    [markConversationRead],
  );

  /* ---------- auto-scroll ---------- */

  useEffect(() => {
    if (threadEndRef.current) {
      threadEndRef.current.scrollIntoView({ behavior: "smooth" });
    }
  }, [selectedConversation?.messages.length]);

  /* ---------- copy inline OTP ---------- */

  const handleCopyInlineOtp = async (id: number, otp: string) => {
    await navigator.clipboard.writeText(otp);
    setCopiedOtpId(id);
    setTimeout(() => setCopiedOtpId(null), 1800);
  };

  /* ================================================================ */
  /*  RENDER                                                           */
  /* ================================================================ */

  return (
    <div className="flex h-full overflow-hidden rounded-lg border border-border bg-card">
      {/* ---------------------------------------------------------- */}
      {/*  Left panel — conversation list                            */}
      {/* ---------------------------------------------------------- */}
      <div
        className={`w-80 shrink-0 border-r border-border flex flex-col bg-card ${
          showMobileThread ? "hidden md:flex" : "flex"
        }`}
      >
        {/* Header */}
        <div className="flex items-center justify-between px-4 py-3 border-b border-border">
          <h2 className="text-lg font-bold text-foreground">收件箱</h2>
          <button
            onClick={() => fetchInbox(false)}
            disabled={loading}
            className="flex items-center gap-1.5 px-2 py-1.5 rounded-md text-xs font-medium border border-border bg-card text-foreground hover:bg-accent/10 transition-colors"
            title="强制向 SIM 卡物理硬件拉取最新未读短信"
          >
            <RefreshCw
              size={12}
              className={loading ? "animate-spin" : ""}
            />
            刷新
          </button>
        </div>

        {/* Search */}
        <div className="px-3 py-2 border-b border-border">
          <div className="relative">
            <Search
              size={14}
              className="absolute left-2.5 top-1/2 -translate-y-1/2 text-muted-foreground"
            />
            <input
              type="text"
              placeholder="搜索号码、机构或内容…"
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              className="w-full pl-8 pr-3 py-1.5 text-sm rounded-md border border-border bg-background text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-1 focus:ring-brand-primary/40"
            />
          </div>
        </div>

        {/* Conversation list */}
        <div className="flex-1 overflow-y-auto">
          {/* Empty states */}
          {!loading && conversations.length === 0 && (
            <div className="p-10 text-center text-muted-foreground">
              <MessageCircle size={36} className="mx-auto mb-3 opacity-30" />
              <p className="text-sm">暂无短信</p>
            </div>
          )}

          {conversations.length > 0 &&
            filteredConversations.length === 0 && (
              <div className="p-10 text-center text-muted-foreground">
                <Search size={36} className="mx-auto mb-3 opacity-30" />
                <p className="text-sm">没有匹配的会话</p>
              </div>
            )}

          {filteredConversations.map((conv) => {
            const isActive = conv.phone === selectedPhone;
            const isJustArrived = conv.phone === newArrivalPhone;
            const displayInfo = resolveConversationDisplayName(
              conv.phone,
              conv.messages,
            );
            const parsedLast = parseSmsContent(
              conv.phone,
              conv.lastMessage.content,
            );

            return (
              <button
                key={conv.phone}
                onClick={() => handleSelectConversation(conv.phone)}
                className={`w-full text-left px-4 py-3 border-b border-border/50 transition-all relative ${
                  isActive ? "bg-accent/10" : "hover:bg-accent/5"
                } ${
                  isJustArrived
                    ? "ring-2 ring-brand-primary bg-brand-primary/5 animate-pulse"
                    : ""
                }`}
              >
                {/* 顶部标题行：优先展示机构大标题 + 盾牌认证图标 */}
                <div className="flex items-center justify-between mb-0.5">
                  <div className="flex items-center gap-1.5 min-w-0 flex-1">
                    <span className="font-semibold text-sm text-foreground truncate">
                      {displayInfo.primaryTitle}
                    </span>
                    {displayInfo.isOfficial && (
                      <span title="官方可信服务号" className="inline-flex items-center">
                        <ShieldCheck
                          size={13}
                          className="text-brand-primary shrink-0"
                        />
                      </span>
                    )}
                    {isJustArrived && (
                      <span className="inline-flex items-center gap-0.5 rounded bg-emerald-500/15 px-1 py-0.2 text-[10px] font-bold text-emerald-600">
                        <Sparkles size={9} /> NEW
                      </span>
                    )}
                  </div>
                  <span className="text-xs text-muted-foreground ml-2 shrink-0">
                    {formatTimestamp(conv.lastMessage.timestamp)}
                  </span>
                </div>

                {/* 副标题：若有机构名，则次级展示物理号码 */}
                {displayInfo.secondaryPhone && (
                  <div className="mb-1 text-[11px] font-mono text-muted-foreground/80">
                    {displayInfo.secondaryPhone}
                  </div>
                )}

                {/* 预览摘要行：优先高亮提取验证码 */}
                <div className="flex items-center justify-between">
                  <p className="text-sm text-muted-foreground truncate pr-2">
                    {conv.lastMessage.direction === "out" ? "我: " : ""}
                    {parsedLast.isOtp && parsedLast.otpCode ? (
                      <span className="font-semibold text-brand-primary mr-1">
                        [验证码: {parsedLast.otpCode}]
                      </span>
                    ) : null}
                    {truncate(parsedLast.cleanBody, 32)}
                  </p>
                  {conv.unreadCount > 0 && (
                    <span className="shrink-0 inline-flex items-center justify-center min-w-[18px] h-[18px] px-1 rounded-full bg-brand-accent text-white text-xs font-bold shadow-sm">
                      {conv.unreadCount}
                    </span>
                  )}
                </div>
              </button>
            );
          })}
        </div>
      </div>

      {/* ---------------------------------------------------------- */}
      {/*  Right panel — message thread                              */}
      {/* ---------------------------------------------------------- */}
      <div
        className={`flex-1 flex flex-col min-w-0 ${
          showMobileThread ? "flex" : "hidden md:flex"
        }`}
      >
        {!selectedConversation ? (
          /* Empty state — no conversation selected */
          <div className="flex-1 flex flex-col items-center justify-center text-muted-foreground">
            <MessageCircle size={48} className="mb-4 opacity-20" />
            <p className="text-sm">选择一个会话查看详情</p>
          </div>
        ) : (
          <>
            {/* Thread header */}
            {(() => {
              const headerDisplay = resolveConversationDisplayName(
                selectedConversation.phone,
                selectedConversation.messages,
              );
              return (
                <div className="flex items-center gap-3 px-4 py-3 border-b border-border shrink-0">
                  <button
                    onClick={() => setShowMobileThread(false)}
                    className="md:hidden p-1 rounded-md hover:bg-accent/10 transition-colors text-foreground"
                    aria-label="返回会话列表"
                  >
                    <ArrowLeft size={18} />
                  </button>
                  <div className="w-9 h-9 rounded-full bg-brand-primary/10 flex items-center justify-center shrink-0">
                    <MessageCircle size={16} className="text-brand-primary" />
                  </div>
                  <div className="min-w-0">
                    <div className="flex items-center gap-1.5">
                      <p className="font-bold text-sm text-foreground truncate">
                        {headerDisplay.primaryTitle}
                      </p>
                      {headerDisplay.isOfficial && (
                        <span title="官方认证服务号" className="inline-flex items-center">
                          <ShieldCheck
                            size={13}
                            className="text-brand-primary"
                          />
                        </span>
                      )}
                    </div>
                    <p className="text-xs text-muted-foreground font-mono">
                      {headerDisplay.secondaryPhone || selectedConversation.phone} ·{" "}
                      {selectedConversation.messages.length} 条消息
                    </p>
                  </div>
                </div>
              );
            })()}

            {/* Messages */}
            <div className="flex-1 overflow-y-auto px-4 py-4 space-y-3">
              {selectedConversation.messages.map((msg) => {
                const isOutgoing = msg.direction === "out";
                const parsedMsg = parseSmsContent(msg.phone, msg.content);

                return (
                  <div
                    key={msg.id}
                    className={`flex ${isOutgoing ? "justify-end" : "justify-start"}`}
                  >
                    <div
                      className={`max-w-[75%] px-3.5 py-2.5 rounded-xl text-sm leading-relaxed shadow-sm transition-all ${
                        isOutgoing
                          ? "bg-brand-primary text-white"
                          : "bg-accent/15 text-foreground border border-border/40"
                      }`}
                    >
                      <p className="whitespace-pre-wrap break-words">
                        {msg.content}
                      </p>

                      {/* 验证码气泡内嵌一键复制快捷按钮 (Gemini 4 裁定 §2.2) */}
                      {!isOutgoing && parsedMsg.isOtp && parsedMsg.otpCode && (
                        <div className="mt-2 pt-2 border-t border-border/40 flex items-center justify-between gap-2">
                          <span className="text-xs text-muted-foreground font-medium">
                            验证码:{" "}
                            <span className="font-mono font-bold text-brand-primary text-sm">
                              {parsedMsg.otpCode}
                            </span>
                          </span>
                          <button
                            onClick={() =>
                              handleCopyInlineOtp(msg.id, parsedMsg.otpCode!)
                            }
                            className="inline-flex items-center gap-1 px-2.5 py-1 rounded bg-brand-primary text-white text-xs font-semibold hover:opacity-90 transition-opacity shadow-xs"
                          >
                            {copiedOtpId === msg.id ? (
                              <Check size={12} />
                            ) : (
                              <Copy size={12} />
                            )}
                            {copiedOtpId === msg.id ? "已复制" : "复制"}
                          </button>
                        </div>
                      )}

                      <div
                        className={`flex items-center gap-1.5 mt-1.5 ${
                          isOutgoing ? "justify-end text-white/80" : "justify-start text-muted-foreground"
                        }`}
                      >
                        <span className="text-[10px]">
                          {formatTimestamp(msg.timestamp)}
                        </span>
                        {isOutgoing &&
                          (msg.status === "read" || msg.status === "delivered") && (
                            <CheckCheck size={12} className="text-white" />
                          )}
                        {msg.status === "failed" && (
                          <span className="text-[10px] text-destructive">
                            失败
                          </span>
                        )}
                      </div>
                    </div>
                  </div>
                );
              })}
              <div ref={threadEndRef} />
            </div>
          </>
        )}
      </div>
    </div>
  );
}

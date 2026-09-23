/**
 * SMSClient — Inbox Page (收件箱)
 * Conversation-based view with aggregation, search, and real-time listening.
 */
import { useEffect, useState, useCallback, useMemo, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { SmsMessage } from "@/lib/types";
import { formatTimestamp } from "@/lib/utils";
import {
  MessageCircle,
  RefreshCw,
  CheckCheck,
  Search,
  ArrowLeft,
} from "lucide-react";

/* ------------------------------------------------------------------ */
/*  Types                                                              */
/* ------------------------------------------------------------------ */

interface Conversation {
  phone: string;
  messages: SmsMessage[];
  lastMessage: SmsMessage;
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

  for (const [phone, msgs] of map) {
    // Sort by timestamp ASC so last element is newest
    msgs.sort(
      (a, b) =>
        new Date(a.timestamp.replace(" ", "T")).getTime() -
        new Date(b.timestamp.replace(" ", "T")).getTime(),
    );
    const lastMessage = msgs[msgs.length - 1];
    const unreadCount = msgs.filter(
      (m) => m.direction === "in" && m.status !== "read",
    ).length;

    conversations.push({ phone, messages: msgs, lastMessage, unreadCount });
  }

  // Sort conversations newest-first
  conversations.sort(
    (a, b) =>
      new Date(b.lastMessage.timestamp.replace(" ", "T")).getTime() -
      new Date(a.lastMessage.timestamp.replace(" ", "T")).getTime(),
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
  const [messages, setMessages] = useState<SmsMessage[]>([]);
  const [loading, setLoading] = useState(true);
  const [selectedPhone, setSelectedPhone] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState("");
  const [showMobileThread, setShowMobileThread] = useState(false);

  const threadEndRef = useRef<HTMLDivElement>(null);

  /* ---------- data fetching ---------- */

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

  /* ---------- real-time listener ---------- */

  useEffect(() => {
    const unlisten = listen("sms-received", () => {
      fetchInbox();
    });

    return () => {
      unlisten.then((fn) => fn());
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

  /* ---------- mark as read ---------- */

  const markConversationRead = useCallback(
    async (phone: string) => {
      const unread = messages.filter(
        (m) =>
          m.phone === phone && m.direction === "in" && m.status !== "read",
      );
      if (unread.length === 0) return;

      // Optimistic local update
      const unreadIds = new Set(unread.map((m) => m.id));
      setMessages((prev) =>
        prev.map((m) =>
          unreadIds.has(m.id) ? { ...m, status: "read" as const } : m,
        ),
      );

      // Fire backend calls
      for (const msg of unread) {
        try {
          await invoke<void>("mark_message_read", { id: msg.id });
        } catch (e) {
          console.error(`Failed to mark message ${msg.id} as read:`, e);
        }
      }
    },
    [messages],
  );

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
            onClick={fetchInbox}
            disabled={loading}
            className="flex items-center gap-1.5 px-2 py-1.5 rounded-md text-xs font-medium border border-border bg-card text-foreground hover:bg-accent/10 transition-colors"
            title="刷新"
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
              placeholder="搜索号码或内容…"
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
            return (
              <button
                key={conv.phone}
                onClick={() => handleSelectConversation(conv.phone)}
                className={`w-full text-left px-4 py-3 border-b border-border/50 hover:bg-accent/5 transition-colors ${
                  isActive ? "bg-accent/10" : ""
                }`}
              >
                <div className="flex items-center justify-between mb-0.5">
                  <span className="font-medium text-sm text-foreground font-mono truncate">
                    {conv.phone}
                  </span>
                  <span className="text-xs text-muted-foreground ml-2 shrink-0">
                    {formatTimestamp(conv.lastMessage.timestamp)}
                  </span>
                </div>
                <div className="flex items-center justify-between">
                  <p className="text-sm text-muted-foreground truncate pr-2">
                    {conv.lastMessage.direction === "out" ? "我: " : ""}
                    {truncate(conv.lastMessage.content, 40)}
                  </p>
                  {conv.unreadCount > 0 && (
                    <span className="shrink-0 inline-flex items-center justify-center min-w-[18px] h-[18px] px-1 rounded-full bg-brand-accent text-white text-xs font-medium">
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
                <p className="font-medium text-sm text-foreground font-mono truncate">
                  {selectedConversation.phone}
                </p>
                <p className="text-xs text-muted-foreground">
                  {selectedConversation.messages.length} 条消息
                </p>
              </div>
            </div>

            {/* Messages */}
            <div className="flex-1 overflow-y-auto px-4 py-4 space-y-3">
              {selectedConversation.messages.map((msg) => {
                const isOutgoing = msg.direction === "out";
                return (
                  <div
                    key={msg.id}
                    className={`flex ${isOutgoing ? "justify-end" : "justify-start"}`}
                  >
                    <div
                      className={`max-w-[75%] px-3 py-2 rounded-lg text-sm leading-relaxed ${
                        isOutgoing
                          ? "bg-brand-primary/10 text-foreground"
                          : "bg-accent/10 text-foreground"
                      }`}
                    >
                      <p className="whitespace-pre-wrap break-words">
                        {msg.content}
                      </p>
                      <div
                        className={`flex items-center gap-1.5 mt-1 ${
                          isOutgoing ? "justify-end" : "justify-start"
                        }`}
                      >
                        <span className="text-[10px] text-muted-foreground">
                          {formatTimestamp(msg.timestamp)}
                        </span>
                        {isOutgoing &&
                          (msg.status === "read" || msg.status === "delivered") && (
                            <CheckCheck size={12} className="text-brand-primary" />
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
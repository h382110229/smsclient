import { useEffect, useState, useRef, useCallback } from "react";
import { listen } from "@tauri-apps/api/event";
import { useNavigate, useLocation } from "react-router-dom";
import { Bell, Copy, Check, ArrowRight, X } from "lucide-react";
import { getSetting } from "@/lib/tauri-bridge";
import { parseSmsContent, playNotificationChime } from "@/lib/sms-parser";

interface SmsEventPayload {
  id?: number;
  phone?: string;
  content?: string;
  timestamp?: string;
  partIndex?: number;
  partTotal?: number;
  groupId?: string | null;
}

interface BannerState {
  phone: string;
  signature: string | null;
  cleanBody: string;
  otpCode: string | null;
  count: number;
}

export function GlobalSmsBanner() {
  const [banner, setBanner] = useState<BannerState | null>(null);
  const [copied, setCopied] = useState(false);
  const navigate = useNavigate();
  const location = useLocation();
  const dismissTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const coalesceTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const pendingRef = useRef<{ phone: string; parts: string[] } | null>(null);

  const scheduleDismiss = useCallback((isOtp: boolean) => {
    if (dismissTimerRef.current) clearTimeout(dismissTimerRef.current);
    dismissTimerRef.current = setTimeout(
      () => setBanner(null),
      isOtp ? 12000 : 8000,
    );
  }, []);

  useEffect(() => {
    const unlisten = listen<SmsEventPayload | null>("sms-received", async (event) => {
      const payload = event.payload;
      if (!payload || !payload.phone || !payload.content) return;

      // 800ms 分片聚合防抖：避免长短信 2~3 个分片连续到达触发多次闪烁
      if (pendingRef.current && pendingRef.current.phone === payload.phone) {
        pendingRef.current.parts.push(payload.content);
      } else {
        pendingRef.current = {
          phone: payload.phone,
          parts: [payload.content],
        };
      }

      if (coalesceTimerRef.current) clearTimeout(coalesceTimerRef.current);
      coalesceTimerRef.current = setTimeout(async () => {
        const item = pendingRef.current;
        pendingRef.current = null;
        if (!item) return;

        const fullContent = item.parts.join("");
        const meta = parseSmsContent(item.phone, fullContent);

        // 检查声音设置并播放合成提示音
        const soundPref = await getSetting("notify_sound").catch(() => "true");
        if (soundPref !== "false") {
          playNotificationChime();
        }

        setCopied(false);
        setBanner((prev) => ({
          phone: item.phone,
          signature: meta.signature,
          cleanBody: meta.cleanBody,
          otpCode: meta.otpCode,
          count: prev ? prev.count + 1 : 1,
        }));
        scheduleDismiss(meta.isOtp);

        // 广播自定义 DOM 事件供 Inbox 页面执行智能切焦与呼吸高亮
        window.dispatchEvent(
          new CustomEvent("smsclient:new-message", {
            detail: {
              phone: item.phone,
              content: fullContent,
              isOtp: meta.isOtp,
              otpCode: meta.otpCode,
            },
          }),
        );
      }, 650);
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, [scheduleDismiss]);

  if (!banner) return null;

  const handleCopyOtp = async (e: React.MouseEvent) => {
    e.stopPropagation();
    if (!banner.otpCode) return;
    await navigator.clipboard.writeText(banner.otpCode);
    setCopied(true);
    setTimeout(() => setBanner(null), 1500);
  };

  const handleJumpToConversation = () => {
    const targetPhone = banner.phone;
    setBanner(null);
    if (location.pathname !== "/inbox") {
      navigate(`/inbox?phone=${encodeURIComponent(targetPhone)}`);
    } else {
      window.dispatchEvent(
        new CustomEvent("smsclient:select-phone", {
          detail: { phone: targetPhone },
        }),
      );
    }
  };

  return (
    <div
      onMouseEnter={() => {
        if (dismissTimerRef.current) clearTimeout(dismissTimerRef.current);
      }}
      onMouseLeave={() => scheduleDismiss(Boolean(banner.otpCode))}
      onClick={handleJumpToConversation}
      className="fixed top-3 left-1/2 -translate-x-1/2 z-50 w-[92%] max-w-xl cursor-pointer rounded-xl border-2 border-brand-primary bg-card/95 backdrop-blur-md px-4 py-3 shadow-2xl transition-all animate-in fade-in slide-in-from-top-4"
    >
      <div className="flex items-center justify-between gap-3">
        <div className="flex items-center gap-2.5 min-w-0 flex-1">
          <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-full bg-brand-primary text-white animate-pulse">
            <Bell size={15} />
          </span>
          <div className="min-w-0 flex-1">
            <div className="flex items-center gap-2">
              <span className="text-xs font-bold text-brand-primary">
                新短信到达
              </span>
              {banner.signature && (
                <span className="rounded bg-brand-primary/10 px-1.5 py-0.5 text-xs font-semibold text-brand-primary">
                  【{banner.signature}】
                </span>
              )}
              <span className="font-mono text-xs text-muted-foreground truncate">
                {banner.phone}
              </span>
              {banner.count > 1 && (
                <span className="rounded-full bg-brand-accent px-1.5 py-0.2 text-[10px] font-bold text-white">
                  +{banner.count}
                </span>
              )}
            </div>
            <p className="mt-0.5 text-sm font-medium text-foreground truncate">
              {banner.cleanBody}
            </p>
          </div>
        </div>

        <div className="flex items-center gap-2 shrink-0">
          {banner.otpCode && (
            <button
              onClick={handleCopyOtp}
              className="flex items-center gap-1.5 rounded-md bg-brand-primary px-3 py-1.5 text-xs font-semibold text-white hover:opacity-90 transition-opacity shadow-sm"
              title="复制验证码"
            >
              {copied ? <Check size={13} /> : <Copy size={13} />}
              {copied ? "已复制" : `复制验证码 ${banner.otpCode}`}
            </button>
          )}
          <button
            onClick={(e) => {
              e.stopPropagation();
              handleJumpToConversation();
            }}
            className="flex items-center gap-1 rounded-md border border-border bg-background px-2.5 py-1.5 text-xs font-medium text-foreground hover:bg-accent/10"
          >
            查看 <ArrowRight size={12} />
          </button>
          <button
            onClick={(e) => {
              e.stopPropagation();
              setBanner(null);
            }}
            className="p-1 text-muted-foreground hover:text-foreground"
            aria-label="关闭通知"
          >
            <X size={14} />
          </button>
        </div>
      </div>
    </div>
  );
}

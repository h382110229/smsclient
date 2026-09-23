/**
 * SMSClient — Settings Page (设置)
 * Worker C「前端骨架」— 2026-09-22
 *
 * M2 更新 (2026-09-22)：主题与通道偏好持久化到 SQLite settings 表（PRD F8）。
 * M3 更新 (2026-09-23)：通知开关/声音、送达回执、群发限速设置。
 * settings key：theme / preferred_channel / notify_enabled / notify_sound
 *              / delivery_report / broadcast_interval
 */
import { useEffect, useState, useCallback } from "react";
import {
  Monitor,
  Wifi,
  FileText,
  Bell,
  HardDrive,
  Volume2,
  VolumeX,
  Radio,
} from "lucide-react";
import type { ThemeMode } from "@/lib/types";
import { getSetting, setSetting } from "@/lib/tauri-bridge";

/** 通道选项（Q7 决策：WinRT 先行，AT 预留） */
const CHANNEL_OPTIONS = [
  { value: "winrt", label: "WinRT SMS API（推荐 · 不中断 4G）" },
  { value: "at", label: "AT 串口（预留 · 需 USB PID_0312 切换）" },
] as const;

/** 群发限速选项 */
const BROADCAST_RATE_OPTIONS = [
  { value: "3000", label: "20 条/分钟" },
  { value: "6000", label: "10 条/分钟（默认）" },
  { value: "10000", label: "6 条/分钟" },
  { value: "15000", label: "4 条/分钟" },
] as const;

/* ------------------------------------------------------------------ */
/*  Toggle Switch                                                     */
/* ------------------------------------------------------------------ */
function ToggleSwitch({
  pressed,
  onChange,
  label,
}: {
  pressed: boolean;
  onChange: (next: boolean) => void;
  label: string;
}) {
  return (
    <button
      role="switch"
      aria-checked={pressed}
      aria-label={label}
      onClick={() => onChange(!pressed)}
      className={`
        relative w-10 h-5 rounded-full transition-colors duration-200
        ${pressed ? "bg-brand-primary" : "bg-muted"}
      `}
    >
      <span
        className={`
          absolute top-0.5 left-0.5 w-4 h-4 rounded-full bg-white shadow
          transition-transform duration-200
          ${pressed ? "translate-x-5" : "translate-x-0"}
        `}
      />
    </button>
  );
}

/* ------------------------------------------------------------------ */
/*  Settings Page                                                     */
/* ------------------------------------------------------------------ */
export function Settings() {
  const [darkMode, setDarkMode] = useState(() =>
    document.documentElement.classList.contains("dark"),
  );
  const [theme, setTheme] = useState<ThemeMode>("light");
  const [channel, setChannel] = useState<string>("winrt");
  const [notifyEnabled, setNotifyEnabled] = useState(true);
  const [notifySound, setNotifySound] = useState(true);
  const [deliveryReport, setDeliveryReport] = useState(false);
  const [broadcastInterval, setBroadcastInterval] = useState("6000");

  /* ---- load persisted settings ---- */
  useEffect(() => {
    (async () => {
      try {
        const [t, ch, ne, ns, dr, bi] = await Promise.all([
          getSetting("theme"),
          getSetting("preferred_channel"),
          getSetting("notify_enabled"),
          getSetting("notify_sound"),
          getSetting("delivery_report"),
          getSetting("broadcast_interval"),
        ]);
        if (t === "dark" || t === "light") setTheme(t);
        if (ch === "winrt" || ch === "at") setChannel(ch);
        setDarkMode(t === "dark");
        document.documentElement.classList.toggle("dark", t === "dark");
        if (ne !== null) setNotifyEnabled(ne === "true");
        if (ns !== null) setNotifySound(ns === "true");
        if (dr !== null) setDeliveryReport(dr === "true");
        if (bi !== null && bi !== "null") setBroadcastInterval(bi);
      } catch (e) {
        console.error("Failed to load settings:", e);
      }
    })();
  }, []);

  /* ---- persist helpers ---- */
  const persistBool = useCallback(
    async (key: string, value: boolean, setter: (v: boolean) => void) => {
      setter(value);
      try {
        await setSetting(key, String(value));
      } catch (e) {
        console.error(`Failed to persist ${key}:`, e);
      }
    },
    [],
  );

  const applyTheme = async (next: ThemeMode) => {
    setTheme(next);
    const dark = next === "dark";
    setDarkMode(dark);
    document.documentElement.classList.toggle("dark", dark);
    try {
      await setSetting("theme", next);
    } catch (e) {
      console.error("Failed to persist theme:", e);
    }
  };

  const changeChannel = async (value: string) => {
    setChannel(value);
    try {
      await setSetting("preferred_channel", value);
    } catch (e) {
      console.error("Failed to persist channel:", e);
    }
  };

  const changeBroadcastInterval = async (value: string) => {
    setBroadcastInterval(value);
    try {
      await setSetting("broadcast_interval", value);
    } catch (e) {
      console.error("Failed to persist broadcast_interval:", e);
    }
  };

  return (
    <div className="space-y-6 max-w-2xl">
      <div>
        <h2 className="text-2xl font-bold text-foreground">设置</h2>
        <p className="text-sm text-muted-foreground mt-1">
          应用偏好设置（SQLite settings 表持久化）
        </p>
      </div>

      <div className="space-y-4">
        {/* Theme */}
        <SettingRow
          icon={<Monitor size={18} className="text-info" />}
          title="外观主题"
          description="切换深色/浅色模式（已持久化）"
        >
          <select
            value={theme}
            onChange={(e) => applyTheme(e.target.value as ThemeMode)}
            className="px-2 py-1.5 rounded-md border border-border bg-background text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
          >
            <option value="light">浅色</option>
            <option value="dark">深色</option>
            <option value="system">跟随系统</option>
          </select>
          <button
            onClick={() => applyTheme(darkMode ? "light" : "dark")}
            className="ml-2 px-3 py-1.5 rounded-md border border-border text-sm text-foreground hover:bg-accent/10 transition-colors"
          >
            {darkMode ? "☀️ 浅色" : "🌙 深色"}
          </button>
        </SettingRow>

        {/* Channel */}
        <SettingRow
          icon={<Wifi size={18} className="text-success" />}
          title="短信通道"
          description="Q7 决策：WinRT 先行，AT 通道预留（切换会断 4G 约 10-30 秒）"
        >
          <select
            value={channel}
            onChange={(e) => changeChannel(e.target.value)}
            className="px-2 py-1.5 rounded-md border border-border bg-background text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
          >
            {CHANNEL_OPTIONS.map((o) => (
              <option key={o.value} value={o.value}>
                {o.label}
              </option>
            ))}
          </select>
        </SettingRow>

        {/* Long SMS */}
        <SettingRow
          icon={<FileText size={18} className="text-warning" />}
          title="长短信拆分"
          description="自动检测并拆分超过 67 字的中文短信（UCS2）"
        >
          <span className="text-xs text-muted-foreground">自动</span>
        </SettingRow>

        {/* Notification toggle */}
        <SettingRow
          icon={<Bell size={18} className="text-brand-primary" />}
          title="通知开关"
          description="收到新短信时显示桌面通知"
        >
          <ToggleSwitch
            pressed={notifyEnabled}
            onChange={(v) =>
              persistBool("notify_enabled", v, setNotifyEnabled)
            }
            label="通知开关"
          />
        </SettingRow>

        {/* Notification sound — only visible when notifications are on */}
        {notifyEnabled && (
          <SettingRow
            icon={
              notifySound ? (
                <Volume2 size={18} className="text-success" />
              ) : (
                <VolumeX size={18} className="text-muted-foreground" />
              )
            }
            title="通知声音"
            description="收到新短信时播放提示音"
          >
            <ToggleSwitch
              pressed={notifySound}
              onChange={(v) =>
                persistBool("notify_sound", v, setNotifySound)
              }
              label="通知声音"
            />
          </SettingRow>
        )}

        {/* Delivery Report */}
        <SettingRow
          icon={<FileText size={18} className="text-brand-accent" />}
          title="送达回执"
          description="请求运营商返回送达确认，匹配已发送消息更新状态"
        >
          <ToggleSwitch
            pressed={deliveryReport}
            onChange={(v) =>
              persistBool("delivery_report", v, setDeliveryReport)
            }
            label="送达回执"
          />
        </SettingRow>

        {/* Broadcast Rate Limit */}
        <SettingRow
          icon={<Radio size={18} className="text-destructive" />}
          title="群发限速"
          description="每分钟最大发送条数，防止运营商限流"
        >
          <select
            value={broadcastInterval}
            onChange={(e) => changeBroadcastInterval(e.target.value)}
            className="px-2 py-1.5 rounded-md border border-border bg-background text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
          >
            {BROADCAST_RATE_OPTIONS.map((o) => (
              <option key={o.value} value={o.value}>
                {o.label}
              </option>
            ))}
          </select>
        </SettingRow>

        {/* Device Info */}
        <SettingRow
          icon={<HardDrive size={18} className="text-foreground" />}
          title="设备信息"
          description="Quectel EM05-CN · VID_2C7C · PID_0310"
        >
          <span className="text-xs text-muted-foreground font-mono">
            Win11 26200
          </span>
        </SettingRow>
      </div>
    </div>
  );
}

/* ------------------------------------------------------------------ */
/*  SettingRow                                                        */
/* ------------------------------------------------------------------ */
function SettingRow({
  icon,
  title,
  description,
  children,
}: {
  icon: React.ReactNode;
  title: string;
  description: string;
  children: React.ReactNode;
}) {
  return (
    <div className="flex items-center gap-4 p-4 border border-border rounded-lg bg-card">
      <span className="shrink-0">{icon}</span>
      <div className="flex-1 min-w-0">
        <h4 className="text-sm font-medium text-foreground">{title}</h4>
        <p className="text-xs text-muted-foreground mt-0.5">{description}</p>
      </div>
      <div className="shrink-0 flex items-center">{children}</div>
    </div>
  );
}

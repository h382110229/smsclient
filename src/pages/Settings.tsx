/**
 * SMSClient — Settings Page (设置)
 * Worker C「前端骨架」— 2026-09-22
 *
 * 应用设置骨架。M3 将接入持久化设置。
 */
import { useState } from "react";
import {
  Monitor,
  Wifi,
  FileText,
  Bell,
  HardDrive,
} from "lucide-react";

export function Settings() {
  const [darkMode, setDarkMode] = useState(() =>
    document.documentElement.classList.contains("dark"),
  );

  const toggleDark = () => {
    const next = !darkMode;
    setDarkMode(next);
    document.documentElement.classList.toggle("dark", next);
  };

  return (
    <div className="space-y-6 max-w-2xl">
      <div>
        <h2 className="text-2xl font-bold text-foreground">设置</h2>
        <p className="text-sm text-muted-foreground mt-1">
          应用偏好设置（M3 持久化到本地存储）
        </p>
      </div>

      <div className="space-y-4">
        {/* Theme */}
        <SettingRow
          icon={<Monitor size={18} className="text-info" />}
          title="外观主题"
          description="切换深色/浅色模式"
        >
          <button
            onClick={toggleDark}
            className="px-3 py-1.5 rounded-md border border-border text-sm text-foreground hover:bg-accent/10 transition-colors"
          >
            {darkMode ? "☀️ 浅色" : "🌙 深色"}
          </button>
        </SettingRow>

        {/* Channel */}
        <SettingRow
          icon={<Wifi size={18} className="text-success" />}
          title="短信通道"
          description="当前: WinRT SMS API（AT 通道预留）"
        >
          <span className="text-xs text-muted-foreground font-mono">
            WinRT (Q7 决策)
          </span>
        </SettingRow>

        {/* Long SMS */}
        <SettingRow
          icon={<FileText size={18} className="text-warning" />}
          title="长短信拆分"
          description="自动检测并拆分超过 67 字的中文短信"
        >
          <span className="text-xs text-muted-foreground">自动</span>
        </SettingRow>

        {/* Delivery Report */}
        <SettingRow
          icon={<Bell size={18} className="text-brand-accent" />}
          title="送达回执"
          description="请求运营商返回送达确认（未实现）"
        >
          <span className="text-xs text-muted-foreground">关闭</span>
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
      <div className="shrink-0">{children}</div>
    </div>
  );
}

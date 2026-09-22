/**
 * SMSClient — Top Header Bar
 * Worker C「前端骨架」— 2026-09-22
 *
 * 显示信号强度、SIM 状态、主题切换。
 * M2 将替换为 invoke("get_sim_info") 实时数据。
 */
import { useState, useCallback } from "react";
import { Sun, Moon, Signal, Smartphone } from "lucide-react";

export function Header() {
  const [dark, setDark] = useState(() =>
    document.documentElement.classList.contains("dark"),
  );

  const toggleTheme = useCallback(() => {
    const next = !dark;
    setDark(next);
    document.documentElement.classList.toggle("dark", next);
  }, [dark]);

  return (
    <header className="h-14 shrink-0 border-b border-border bg-card flex items-center justify-between px-6">
      {/* Left: Status Indicators */}
      <div className="flex items-center gap-5">
        {/* Signal */}
        <div className="flex items-center gap-1.5 text-sm text-muted-foreground">
          <Signal size={14} className="text-success" />
          <span className="font-mono text-xs">-75 dBm</span>
        </div>

        {/* SIM */}
        <div className="flex items-center gap-1.5 text-sm text-muted-foreground">
          <Smartphone size={14} className="text-info" />
          <span className="text-xs">
            中国移动 · <span className="font-mono">4G/LTE</span>
          </span>
        </div>

        {/* Channel Badge */}
        <span className="inline-flex items-center px-2 py-0.5 rounded-pill text-[10px] font-medium bg-brand-primary/10 text-brand-primary">
          WinRT SMS
        </span>
      </div>

      {/* Right: Theme Toggle */}
      <button
        onClick={toggleTheme}
        className="flex items-center justify-center w-8 h-8 rounded-md border border-border text-muted-foreground hover:bg-accent hover:text-accent-foreground transition-colors"
        style={{ transitionDuration: "var(--asteam-duration-fast)" }}
        title={dark ? "切换到浅色模式" : "切换到深色模式"}
      >
        {dark ? <Sun size={16} /> : <Moon size={16} />}
      </button>
    </header>
  );
}

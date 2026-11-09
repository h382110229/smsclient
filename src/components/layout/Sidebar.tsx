/**
 * SMSClient — Sidebar Navigation
 * Worker C「前端骨架」— 2026-09-22
 *
 * 使用 NavLink 实现激活态高亮。
 * 品牌色 primary 应用于激活态背景。
 */
import { NavLink } from "react-router-dom";
import {
  LayoutDashboard,
  Inbox,
  PenSquare,
  Send,
  Users,
  Radio,
  Settings,
} from "lucide-react";

interface NavItem {
  path: string;
  label: string;
  icon: React.ComponentType<{ className?: string; size?: number }>;
}

const navItems: NavItem[] = [
  { path: "/", label: "首页", icon: LayoutDashboard },
  { path: "/inbox", label: "收件箱", icon: Inbox },
  { path: "/compose", label: "写短信", icon: PenSquare },
  { path: "/sent", label: "已发送", icon: Send },
  { path: "/contacts", label: "联系人", icon: Users },
  { path: "/broadcast", label: "群发", icon: Radio },
  { path: "/settings", label: "设置", icon: Settings },
];

export function Sidebar() {
  return (
    <aside className="w-64 shrink-0 border-r border-border bg-card flex flex-col">
      {/* Brand Header */}
      <div className="h-14 flex items-center gap-2 px-5 border-b border-border">
        <div className="flex items-center justify-center w-8 h-8 rounded-lg bg-brand-primary text-white font-bold text-sm">
          S
        </div>
        <div>
          <h1 className="text-base font-bold text-foreground leading-tight">
            SMSClient
          </h1>
          <p className="text-[10px] text-muted-foreground leading-tight">
            ASTeam · v1.0.0
          </p>
        </div>
      </div>

      {/* Navigation */}
      <nav className="flex-1 p-3 space-y-1 overflow-y-auto">
        {navItems.map((item) => {
          const Icon = item.icon;
          return (
            <NavLink
              key={item.path}
              to={item.path}
              end={item.path === "/"}
              className={({ isActive }) =>
                [
                  "flex items-center gap-3 px-3 py-2.5 rounded-md text-sm font-medium",
                  "transition-all",
                  isActive
                    ? "bg-brand-primary text-primary-foreground shadow-sm"
                    : "text-muted-foreground hover:bg-accent/10 hover:text-foreground",
                ].join(" ")
              }
              style={{ transitionDuration: "var(--asteam-duration-fast)" }}
            >
              <Icon size={18} />
              <span>{item.label}</span>
            </NavLink>
          );
        })}
      </nav>

      {/* Footer */}
      <div className="p-4 border-t border-border">
        <p className="text-[10px] text-muted-foreground text-center">
          © 2026 ASTeam
        </p>
      </div>
    </aside>
  );
}

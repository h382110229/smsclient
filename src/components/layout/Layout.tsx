/**
 * SMSClient — Root Layout
 * Worker C「前端骨架」— 2026-09-22
 *
 * 侧边栏 + 顶部栏 + 内容区的骨架布局。
 * 侧边栏固定宽度 256px，主内容区 flex-1 自适应。
 */
import { Outlet } from "react-router-dom";
import { Sidebar } from "./Sidebar";
import { Header } from "./Header";
import { GlobalSmsBanner } from "./GlobalSmsBanner";

export function Layout() {
  return (
    <div className="flex h-screen overflow-hidden bg-background text-foreground relative">
      {/* 全局顶部短信通知浮窗 (Gemini 4 Review §3.4) */}
      <GlobalSmsBanner />

      {/* 左侧导航栏 */}
      <Sidebar />

      {/* 右侧内容区 */}
      <div className="flex flex-col flex-1 overflow-hidden">
        {/* 顶部状态栏 */}
        <Header />

        {/* 主内容 */}
        <main className="flex-1 overflow-y-auto p-6">
          <Outlet />
        </main>
      </div>
    </div>
  );
}

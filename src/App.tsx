/**
 * SMSClient — Root App with React Router
 * Worker C「前端骨架」— 2026-09-22
 *
 * Routes:
 *  /           Dashboard (首页/仪表盘)
 *  /inbox      Inbox (收件箱)
 *  /compose    Compose (写短信)
 *  /sent       Sent (已发送)
 *  /contacts   Contacts (联系人)
 *  /broadcast  Broadcast (群发)
 *  /settings   Settings (设置)
 */
import { BrowserRouter, Routes, Route } from "react-router-dom";
import { Layout } from "@/components/layout/Layout";
import { Dashboard } from "@/pages/Dashboard";
import { Inbox } from "@/pages/Inbox";
import { Compose } from "@/pages/Compose";
import { Sent } from "@/pages/Sent";
import { Contacts } from "@/pages/Contacts";
import { Broadcast } from "@/pages/Broadcast";
import { Settings } from "@/pages/Settings";

function App() {
  return (
    <BrowserRouter>
      <Routes>
        <Route element={<Layout />}>
          <Route path="/" element={<Dashboard />} />
          <Route path="/inbox" element={<Inbox />} />
          <Route path="/compose" element={<Compose />} />
          <Route path="/sent" element={<Sent />} />
          <Route path="/contacts" element={<Contacts />} />
          <Route path="/broadcast" element={<Broadcast />} />
          <Route path="/settings" element={<Settings />} />
        </Route>
      </Routes>
    </BrowserRouter>
  );
}

export default App;

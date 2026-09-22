/**
 * SMSClient — Compose Page (写短信)
 * Worker C「前端骨架」— 2026-09-22
 *
 * 发送表单骨架：手机号 + 内容 + 发送按钮。
 * 硬约束: 收发短信不得中断 4G 数据连接。
 * M2 将接入 WinRT SmsDevice2 发送逻辑。
 */
import { useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Send, CheckCircle, AlertCircle } from "lucide-react";

export function Compose() {
  const [phone, setPhone] = useState("");
  const [body, setBody] = useState("");
  const [sending, setSending] = useState(false);
  const [result, setResult] = useState<{
    type: "success" | "error";
    message: string;
  } | null>(null);

  const charCount = body.length;
  const maxChars = 70; // 中文 67 字/段 + 余量

  const handleSend = useCallback(async () => {
    if (!phone.trim() || !body.trim()) return;
    setSending(true);
    setResult(null);
    try {
      const msgId = await invoke<string>("send_sms", {
        phone: phone.trim(),
        message: body.trim(),
      });
      setResult({ type: "success", message: `发送成功! ID: ${msgId}` });
      setBody(""); // 清空输入
    } catch (e) {
      setResult({ type: "error", message: `发送失败: ${e}` });
    } finally {
      setSending(false);
    }
  }, [phone, body]);

  return (
    <div className="space-y-6 max-w-2xl">
      <div>
        <h2 className="text-2xl font-bold text-foreground">写短信</h2>
        <p className="text-sm text-muted-foreground mt-1">
          通过 WinRT SMS API 发送短信（4G 数据不中断）
        </p>
      </div>

      {/* Phone Input */}
      <div className="space-y-2">
        <label className="text-sm font-medium text-foreground">
          收件人
        </label>
        <input
          type="tel"
          value={phone}
          onChange={(e) => setPhone(e.target.value)}
          placeholder="输入手机号码，如 13800138000"
          className="w-full px-3 py-2.5 rounded-md border border-border bg-card text-foreground text-sm placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-ring"
        />
      </div>

      {/* Body Input */}
      <div className="space-y-2">
        <div className="flex items-center justify-between">
          <label className="text-sm font-medium text-foreground">内容</label>
          <span
            className={`text-xs ${
              charCount > maxChars ? "text-destructive" : "text-muted-foreground"
            }`}
          >
            {charCount} / {maxChars}
          </span>
        </div>
        <textarea
          value={body}
          onChange={(e) => setBody(e.target.value)}
          placeholder="输入短信内容..."
          rows={6}
          className="w-full px-3 py-2.5 rounded-md border border-border bg-card text-foreground text-sm placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-ring resize-none"
        />
        {charCount > 67 && (
          <p className="text-xs text-warning">
            ⚠ 超过 67 字将自动拆分为多条长短信
          </p>
        )}
      </div>

      {/* Send Button */}
      <div className="flex items-center gap-4">
        <button
          onClick={handleSend}
          disabled={sending || !phone.trim() || !body.trim()}
          className="flex items-center gap-2 px-5 py-2.5 rounded-md bg-brand-primary text-white text-sm font-medium hover:opacity-90 disabled:opacity-50 disabled:cursor-not-allowed transition-opacity"
        >
          <Send size={14} />
          {sending ? "发送中..." : "发送"}
        </button>

        {/* Result */}
        {result && (
          <div
            className={`flex items-center gap-2 text-sm font-medium ${
              result.type === "success"
                ? "text-success"
                : "text-destructive"
            }`}
          >
            {result.type === "success" ? (
              <CheckCircle size={16} />
            ) : (
              <AlertCircle size={16} />
            )}
            <span>{result.message}</span>
          </div>
        )}
      </div>
    </div>
  );
}

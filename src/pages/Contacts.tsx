/**
 * SMSClient — Contacts Page (联系人)
 * M2 完整实现：真实 CRUD（list / add / edit / delete / search）
 * 字段对齐 PRD §5.2（id: number, phone, note: string | null）
 */
import { useEffect, useState, useCallback, useRef } from "react";
import {
  UserPlus,
  Users,
  Pencil,
  Trash2,
  Search,
  X,
  Check,
  AlertCircle,
} from "lucide-react";
import { listContacts, addContact, updateContact, deleteContact } from "@/lib/tauri-bridge";
import type { Contact } from "@/lib/types";
import { isValidPhone, normalizePhone } from "@/lib/utils";

/** 空表单初始值 */
const emptyForm = { name: "", phone: "", note: "" };

export function Contacts() {
  const [contacts, setContacts] = useState<Contact[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [search, setSearch] = useState("");
  const searchTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  // 表单状态: null=隐藏, 'add'=新增, number=编辑某 id
  const [formMode, setFormMode] = useState<"add" | "edit" | null>(null);
  const [editId, setEditId] = useState<number | null>(null);
  const [form, setForm] = useState(emptyForm);
  const [saving, setSaving] = useState(false);

  // 删除确认: null=无, number=待确认删除的 id
  const [pendingDeleteId, setPendingDeleteId] = useState<number | null>(null);

  // ── 数据加载 ──────────────────────────────────────────────

  const fetchContacts = useCallback(async (q?: string) => {
    try {
      const data = await listContacts(q);
      setContacts(data);
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    setLoading(true);
    fetchContacts().finally(() => setLoading(false));
  }, [fetchContacts]);

  // 搜索去抖：200ms
  useEffect(() => {
    if (searchTimer.current) clearTimeout(searchTimer.current);
    searchTimer.current = setTimeout(() => {
      fetchContacts(search || undefined);
    }, 200);
    return () => {
      if (searchTimer.current) clearTimeout(searchTimer.current);
    };
  }, [search, fetchContacts]);

  // ── 新增 ──────────────────────────────────────────────────

  const handleAdd = () => {
    setFormMode("add");
    setEditId(null);
    setForm(emptyForm);
    setError(null);
  };

  // ── 编辑 ──────────────────────────────────────────────────

  const handleEdit = (c: Contact) => {
    setFormMode("edit");
    setEditId(c.id);
    setForm({ name: c.name, phone: c.phone, note: c.note || "" });
    setError(null);
  };

  // ── 保存（新增或编辑） ────────────────────────────────────

  const handleSave = async () => {
    const trimmed = {
      name: form.name.trim(),
      phone: form.phone.trim(),
      note: form.note.trim(),
    };
    if (!trimmed.name) {
      setError("姓名不能为空");
      return;
    }
    if (!trimmed.phone) {
      setError("手机号码不能为空");
      return;
    }
    if (!isValidPhone(trimmed.phone)) {
      setError("手机号码格式不正确（支持 11 位手机号或短号 10086）");
      return;
    }
    setSaving(true);
    try {
      if (formMode === "add") {
        await addContact(
          trimmed.name,
          trimmed.phone,
          trimmed.note || undefined,
        );
      } else if (formMode === "edit" && editId !== null) {
        await updateContact(
          editId,
          trimmed.name,
          trimmed.phone,
          trimmed.note || undefined,
        );
      }
      setFormMode(null);
      setEditId(null);
      setForm(emptyForm);
      setError(null);
      await fetchContacts(search || undefined);
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  };

  // ── 取消 ──────────────────────────────────────────────────

  const handleCancel = () => {
    setFormMode(null);
    setEditId(null);
    setForm(emptyForm);
    setError(null);
  };

  // ── 删除确认 ──────────────────────────────────────────────

  const handleDelete = async (id: number) => {
    try {
      await deleteContact(id);
      setPendingDeleteId(null);
      await fetchContacts(search || undefined);
    } catch (e) {
      setError(String(e));
      setPendingDeleteId(null);
    }
  };

  // ── 渲染 ──────────────────────────────────────────────────

  return (
    <div className="space-y-6">
      {/* Header + 搜索栏 */}
      <div className="flex items-center justify-between gap-4">
        <div>
          <h2 className="text-2xl font-bold text-foreground">联系人</h2>
          <p className="text-sm text-muted-foreground mt-1">
            {contacts.length} 位联系人（SQLite 持久化）
          </p>
        </div>
        <button
          onClick={handleAdd}
          className="flex items-center gap-2 px-3 py-2 rounded-md text-sm font-medium bg-brand-primary text-white hover:opacity-90 transition-opacity"
        >
          <UserPlus size={14} />
          新建联系人
        </button>
      </div>

      {/* 搜索框 */}
      <div className="relative">
        <Search
          size={14}
          className="absolute left-3 top-1/2 -translate-y-1/2 text-muted-foreground"
        />
        <input
          type="text"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          placeholder="搜索姓名或手机号..."
          className="w-full pl-9 pr-3 py-2.5 rounded-md border border-border bg-card text-foreground text-sm placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-ring"
        />
        {search && (
          <button
            onClick={() => setSearch("")}
            className="absolute right-3 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground"
          >
            <X size={14} />
          </button>
        )}
      </div>

      {/* 错误提示 */}
      {error && (
        <div className="flex items-center gap-2 p-3 rounded-md bg-destructive/10 text-destructive border border-destructive/20 text-sm">
          <AlertCircle size={14} />
          <span>{error}</span>
        </div>
      )}

      {/* 新增/编辑表单 */}
      {formMode && (
        <div className="border border-brand-primary/30 rounded-lg p-4 bg-brand-primary/5 space-y-3">
          <h4 className="text-sm font-semibold text-foreground">
            {formMode === "add" ? "新建联系人" : "编辑联系人"}
          </h4>
          <div className="grid grid-cols-1 sm:grid-cols-3 gap-3">
            <div>
              <label className="text-xs font-medium text-muted-foreground mb-1 block">
                姓名 *
              </label>
              <input
                type="text"
                value={form.name}
                onChange={(e) => setForm({ ...form, name: e.target.value })}
                placeholder="张三"
                autoFocus
                className="w-full px-3 py-2 rounded-md border border-border bg-card text-foreground text-sm placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-ring"
              />
            </div>
            <div>
              <label className="text-xs font-medium text-muted-foreground mb-1 block">
                手机号 *
              </label>
              <input
                type="tel"
                value={form.phone}
                onChange={(e) => setForm({ ...form, phone: e.target.value })}
                placeholder="13800138000"
                className="w-full px-3 py-2 rounded-md border border-border bg-card text-foreground text-sm font-mono placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-ring"
              />
            </div>
            <div>
              <label className="text-xs font-medium text-muted-foreground mb-1 block">
                备注
              </label>
              <input
                type="text"
                value={form.note}
                onChange={(e) => setForm({ ...form, note: e.target.value })}
                placeholder="同事"
                className="w-full px-3 py-2 rounded-md border border-border bg-card text-foreground text-sm placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-ring"
              />
            </div>
          </div>
          <div className="flex items-center gap-2 pt-1">
            <button
              onClick={handleSave}
              disabled={saving}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-md text-sm font-medium bg-brand-primary text-white hover:opacity-90 disabled:opacity-50 transition-opacity"
            >
              <Check size={12} />
              {saving ? "保存中..." : "保存"}
            </button>
            <button
              onClick={handleCancel}
              className="px-3 py-1.5 rounded-md text-sm font-medium text-muted-foreground hover:bg-accent/10 transition-colors"
            >
              取消
            </button>
            {form.phone && (
              <span className="text-xs text-muted-foreground font-mono ml-2">
                → {normalizePhone(form.phone)}
              </span>
            )}
          </div>
        </div>
      )}

      {/* 联系人列表 */}
      <div className="border border-border rounded-lg divide-y divide-border bg-card">
        {contacts.length === 0 && !loading && (
          <div className="p-12 text-center text-muted-foreground">
            <Users size={40} className="mx-auto mb-3 opacity-30" />
            <p className="text-sm">
              {search ? "未找到匹配联系人" : "暂无联系人"}
            </p>
            <p className="text-xs mt-1">
              点击「新建联系人」开始添加
            </p>
          </div>
        )}
        {contacts.map((c) => (
          <div
            key={c.id}
            className="flex items-center gap-4 p-4 hover:bg-accent/5 transition-colors"
          >
            <div className="w-10 h-10 rounded-full bg-accent/20 flex items-center justify-center shrink-0">
              <Users
                size={16}
                className="text-accent-foreground"
              />
            </div>
            <div className="flex-1 min-w-0">
              <p className="text-sm font-medium text-foreground">
                {c.name}
              </p>
              <p className="text-xs text-muted-foreground font-mono">
                {c.phone}
              </p>
            </div>
            {c.note && (
              <span className="text-xs text-muted-foreground bg-muted px-2 py-0.5 rounded-full">
                {c.note}
              </span>
            )}
            {/* 操作按钮 */}
            <div className="flex items-center gap-1 shrink-0">
              {pendingDeleteId === c.id ? (
                <>
                  <span className="text-xs text-destructive mr-1">
                    确认删除?
                  </span>
                  <button
                    onClick={() => handleDelete(c.id)}
                    className="px-2 py-1 rounded text-xs font-medium bg-destructive text-destructive-foreground hover:opacity-90 transition-opacity"
                  >
                    删除
                  </button>
                  <button
                    onClick={() => setPendingDeleteId(null)}
                    className="px-2 py-1 rounded text-xs font-medium text-muted-foreground hover:bg-accent/10 transition-colors"
                  >
                    取消
                  </button>
                </>
              ) : (
                <>
                  <button
                    onClick={() => handleEdit(c)}
                    className="p-1.5 rounded text-muted-foreground hover:text-foreground hover:bg-accent/10 transition-colors"
                    title="编辑"
                  >
                    <Pencil size={14} />
                  </button>
                  <button
                    onClick={() => setPendingDeleteId(c.id)}
                    className="p-1.5 rounded text-muted-foreground hover:text-destructive hover:bg-destructive/10 transition-colors"
                    title="删除"
                  >
                    <Trash2 size={14} />
                  </button>
                </>
              )}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

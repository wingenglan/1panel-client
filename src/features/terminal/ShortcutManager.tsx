import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Edit3, Plus, RotateCcw, Search, Sparkles, Trash2, X } from "lucide-react";
import { useMemo, useState, type FormEvent } from "react";
import { Button } from "../../components/ui/Button";
import { api } from "../../lib/api";
import { errorMessage } from "../../lib/errors";
import type { SaveShortcutInput, ShortcutRecord, ShortcutScope } from "../../types/server";

interface ShortcutManagerProps {
  serverId: string;
  open: boolean;
  onClose: () => void;
}

const emptyForm: SaveShortcutInput = { scope: "global", name: "", groupName: "自定义", commandTemplate: "", description: "", tags: [], enabled: true };

/** Provides local CRUD, filtering, and default restoration for terminal shortcuts. */
export function ShortcutManager({ serverId, open, onClose }: ShortcutManagerProps) {
  const queryClient = useQueryClient();
  const shortcuts = useQuery({ queryKey: ["shortcuts", serverId], queryFn: () => api.listShortcuts(serverId), enabled: open });
  const [query, setQuery] = useState("");
  const [groupFilter, setGroupFilter] = useState("all");
  const [form, setForm] = useState<SaveShortcutInput>(emptyForm);
  const [editing, setEditing] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const save = useMutation({ mutationFn: api.saveShortcut, onSuccess: async () => { setEditing(null); setForm(emptyForm); setError(null); await queryClient.invalidateQueries({ queryKey: ["shortcuts", serverId] }); } });
  const remove = useMutation({ mutationFn: api.deleteShortcut, onSuccess: () => queryClient.invalidateQueries({ queryKey: ["shortcuts", serverId] }) });
  const restore = useMutation({ mutationFn: api.restoreDefaultShortcuts, onSuccess: () => queryClient.invalidateQueries({ queryKey: ["shortcuts", serverId] }) });
  const visible = useMemo(() => {
    const needle = query.trim().toLocaleLowerCase();
    return (shortcuts.data ?? []).filter((item) => (groupFilter === "all" || (item.groupName || "未分组") === groupFilter) && (!needle || [item.name, item.groupName, item.commandTemplate, item.description, item.tags.join(" ")].join(" ").toLocaleLowerCase().includes(needle)));
  }, [groupFilter, query, shortcuts.data]);
  const groups = useMemo(() => [...new Set((shortcuts.data ?? []).map((item) => item.groupName || "未分组"))].sort((left, right) => left.localeCompare(right, "zh-CN")), [shortcuts.data]);
  const groupedVisible = useMemo(() => {
    const grouped = new Map<string, ShortcutRecord[]>();
    for (const item of visible) {
      const group = item.groupName || "未分组";
      grouped.set(group, [...(grouped.get(group) ?? []), item]);
    }
    return [...grouped.entries()];
  }, [visible]);
  if (!open) return null;

  /** Saves the current shortcut form while normalizing comma-separated tags. */
  const submit = (event: FormEvent) => {
    event.preventDefault();
    setError(null);
    const payload = { ...form, serverId: form.scope === "server" ? serverId : undefined, tags: form.tags.flatMap((tag) => tag.split(",")).map((tag) => tag.trim()).filter(Boolean) };
    save.mutate(payload, { onError: (reason) => setError(errorMessage(reason)) });
  };
  /** Loads a selected shortcut into the editor without changing its usage metadata. */
  const edit = (item: ShortcutRecord) => {
    setEditing(item.id);
    setForm({ id: item.id, scope: item.scope, serverId: item.serverId ?? undefined, name: item.name, groupName: item.groupName, commandTemplate: item.commandTemplate, description: item.description, tags: item.tags, enabled: item.enabled });
  };
  return <div className="shortcut-overlay" role="dialog" aria-modal="true" aria-label="快捷指令管理">
    <div className="shortcut-manager">
      <header className="shortcut-manager__header"><div><span className="section-kicker"><Sparkles size={13} /> Terminal library</span><h2>快捷指令</h2><p>把重复操作整理成可搜索、可插入的命令模板。Tab 只插入，Enter 才执行。</p></div><button className="icon-control" onClick={onClose} aria-label="关闭"><X size={17} /></button></header>
      <div className="shortcut-manager__body">
        <section className="shortcut-library">
          <div className="shortcut-toolbar"><label><Search size={14} /><input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="搜索名称、命令或标签" /></label><select className="shortcut-group-filter" value={groupFilter} onChange={(event) => setGroupFilter(event.target.value)} aria-label="按分组筛选"><option value="all">全部分组</option>{groups.map((group) => <option key={group} value={group}>{group}</option>)}</select><Button size="sm" onClick={() => { setEditing(null); setForm(emptyForm); }}><Plus size={14} /> 新建</Button></div>
          <div className="shortcut-list">{shortcuts.isLoading && <div className="shortcut-empty">正在载入快捷指令…</div>}{!shortcuts.isLoading && !visible.length && <div className="shortcut-empty">没有匹配的快捷指令</div>}{groupedVisible.map(([group, items]) => <section className="shortcut-group" key={group}><header><strong>{group}</strong><span>{items.length} 条指令</span></header>{items.map((item) => <article className="shortcut-card" key={item.id}><div className="shortcut-card__main"><div className="shortcut-card__title"><strong>{item.name}</strong><span>{item.scope === "global" ? "全局" : "当前服务器"}</span></div><code>{item.commandTemplate}</code><p>{item.description || "未添加说明"}</p><div className="shortcut-tags">{item.tags.map((tag) => <span key={tag}>{tag}</span>)}</div></div><div className="shortcut-card__actions"><button onClick={() => edit(item)} title="编辑"><Edit3 size={14} /></button><button className="is-danger" onClick={() => { if (window.confirm("删除快捷指令“" + item.name + "”？")) remove.mutate(item.id); }} title="删除"><Trash2 size={14} /></button></div></article>)}</section>)}</div>
          <button className="shortcut-restore" onClick={() => restore.mutate()} disabled={restore.isPending}><RotateCcw size={13} /> 恢复缺失的默认指令</button>
        </section>
        <form className="shortcut-form" onSubmit={submit}><div className="shortcut-form__heading"><span>{editing ? "编辑快捷指令" : "新建快捷指令"}</span><small>{editing ? "保留原有使用次数" : "保存后立即参与终端匹配"}</small></div><label><span>名称</span><input required maxLength={80} value={form.name} onChange={(event) => setForm((current) => ({ ...current, name: event.target.value }))} placeholder="例如：查看 Nginx 日志" /></label><label><span>分组</span><input maxLength={60} value={form.groupName} onChange={(event) => setForm((current) => ({ ...current, groupName: event.target.value }))} placeholder="例如：Docker、Systemd、网络" /></label><label><span>命令模板</span><textarea required maxLength={4000} value={form.commandTemplate} onChange={(event) => setForm((current) => ({ ...current, commandTemplate: event.target.value }))} placeholder="例如：journalctl -u {{service}} -n 100 --no-pager" /></label><small className="shortcut-help">使用 <code>{"{{变量}}"}</code> 添加插入前填写的参数；快捷指令不会自动执行。</small><label><span>说明</span><input maxLength={240} value={form.description} onChange={(event) => setForm((current) => ({ ...current, description: event.target.value }))} placeholder="这条命令用于什么场景？" /></label><label><span>标签</span><input value={form.tags.join(", ")} onChange={(event) => setForm((current) => ({ ...current, tags: [event.target.value] }))} placeholder="docker, 日志, 常用" /></label><label><span>作用范围</span><select value={form.scope} onChange={(event) => setForm((current) => ({ ...current, scope: event.target.value as ShortcutScope }))}><option value="global">全局快捷指令</option><option value="server">仅当前服务器</option></select></label><label className="shortcut-check"><input type="checkbox" checked={form.enabled} onChange={(event) => setForm((current) => ({ ...current, enabled: event.target.checked }))} /><span>启用匹配</span></label>{error && <div className="form-error">{error}</div>}<div className="dialog-actions"><Button type="button" onClick={() => { setEditing(null); setForm(emptyForm); }}>清空</Button><Button type="submit" variant="primary" disabled={save.isPending}>{save.isPending ? "保存中…" : editing ? "保存修改" : "保存指令"}</Button></div></form>
      </div>
    </div>
  </div>;
}

import { useMutation, useQueryClient } from "@tanstack/react-query";
import * as Dialog from "@radix-ui/react-dialog";
import { Archive, ArrowUpDown, ChevronDown, ChevronRight, FolderOpen, Globe2, Layers, LayoutGrid, LayoutList, LockKeyhole, Pencil, Pin, Plus, RefreshCw, Search, Send, Settings, ShieldAlert, SlidersHorizontal, Trash2, Upload } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useParams } from "react-router-dom";
import { Button } from "../../components/ui/Button";
import { api } from "../../lib/api";
import { errorMessage } from "../../lib/errors";
import type { WebsiteRecord } from "../../types/server";
import { PhpInstallDialog, SslCertificateDialog, WebsiteCreateDialog } from "./dialogs";
import { useWebsiteSnapshot } from "./useWebsiteSnapshot";

type ColumnKey = "domain" | "kind" | "path" | "status" | "protocol" | "expire" | "ssl" | "remark" | "operations";
const COLUMN_LABELS: Record<ColumnKey, string> = { domain: "名称", kind: "类型", path: "网站目录", status: "状态", protocol: "协议", expire: "过期时间", ssl: "证书过期时间", remark: "备注", operations: "操作" };
const DEFAULT_COLUMNS: ColumnKey[] = ["domain", "kind", "path", "status", "protocol", "expire", "operations"];
const ALL_COLUMNS: ColumnKey[] = ["domain", "kind", "path", "status", "protocol", "expire", "ssl", "remark", "operations"];

const COLUMN_WIDTHS: Record<ColumnKey, string> = { domain: "minmax(180px, 1.3fr)", kind: "96px", path: "62px", status: "100px", protocol: "80px", expire: "105px", ssl: "120px", remark: "minmax(120px, 1fr)", operations: "130px" };

function readJson<T>(key: string, fallback: T): T {
  try { const parsed = JSON.parse(localStorage.getItem(key) ?? "") as unknown; return Array.isArray(fallback) && !Array.isArray(parsed) ? fallback : (parsed ?? fallback) as T; } catch { return fallback; }
}
function writeJson(key: string, value: unknown) { localStorage.setItem(key, JSON.stringify(value)); }
const FAVORITES_KEY = "1panel-client.website-favorites";
const REMARKS_KEY = "1panel-client.website-remarks";
const ALIASES_KEY = "1panel-client.website-aliases";
const EXPIRIES_KEY = "1panel-client.website-expiries";
const COLUMNS_KEY = "1panel-client.website-columns.v2";
const GROUPS_KEY = "1panel-client.website-groups";
const GROUP_MAP_KEY = "1panel-client.website-group-map";

function fmtDate(raw: string): string {
  const date = new Date(raw);
  if (Number.isNaN(date.getTime())) return raw;
  const year = date.getFullYear();
  const month = `${date.getMonth() + 1}`.padStart(2, "0");
  const day = `${date.getDate()}`.padStart(2, "0");
  return `${year}/${month}/${day}`;
}
function sslTagClass(website: WebsiteRecord): string {
  if (!website.expiresAt) return "is-ok";
  const days = Math.ceil((new Date(website.expiresAt).getTime() - Date.now()) / 86_400_000);
  if (days < 0) return "is-danger";
  if (days <= 30) return "is-warn";
  return "is-ok";
}

/** 展示站点、OpenResty 状态和批量操作；操作弹窗在打开时挂载以重置草稿。 */
export function WebsitePage() {
  const { serverId = "" } = useParams();
  const queryClient = useQueryClient();
  const websites = useWebsiteSnapshot(serverId);
  const snapshot = websites.data;
  const [createOpen, setCreateOpen] = useState(false);
  const [certificateOpen, setCertificateOpen] = useState(false);
  const [certificateDefaults, setCertificateDefaults] = useState<{ domain?: string; action?: "issue" | "renew" } | undefined>(undefined);
  const [phpOpen, setPhpOpen] = useState(false);
  const [view, setView] = useState<"table" | "card">("table");
  const [kindFilter, setKindFilter] = useState("");
  const [groupFilter, setGroupFilter] = useState("");
  const [query, setQuery] = useState("");
  const [columns, setColumns] = useState<ColumnKey[]>(() => readJson<ColumnKey[]>(COLUMNS_KEY, DEFAULT_COLUMNS));
  const [columnsOpen, setColumnsOpen] = useState(false);
  const [groupsOpen, setGroupsOpen] = useState(false);
  const [advancedOpen, setAdvancedOpen] = useState(false);
  const [selected, setSelected] = useState<string[]>([]);
  const [batchAction, setBatchAction] = useState("");
  const [assignGroup, setAssignGroup] = useState(false);
  const [pendingAssign, setPendingAssign] = useState<string[]>([]);
  const [rowMenu, setRowMenu] = useState<{ domain: string; x: number; y: number } | null>(null);
  const [confirm, setConfirm] = useState<{ title: string; message: string; danger?: boolean; confirmText?: string; run: () => void } | null>(null);
  const [favorites, setFavorites] = useState<string[]>(() => readJson<string[]>(FAVORITES_KEY, []));
  const [remarks, setRemarks] = useState<Record<string, string>>(() => readJson<Record<string, string>>(REMARKS_KEY, {}));
  const [aliases, setAliases] = useState<Record<string, string>>(() => readJson<Record<string, string>>(ALIASES_KEY, {}));
  const [expiries, setExpiries] = useState<Record<string, string>>(() => readJson<Record<string, string>>(EXPIRIES_KEY, {}));
  const [expiryDomain, setExpiryDomain] = useState<string | null>(null);
  const [editingDomain, setEditingDomain] = useState<string | null>(null);
  const [aliasDraft, setAliasDraft] = useState("");
  const [groups, setGroups] = useState<{ id: string; name: string }[]>(() => readJson<{ id: string; name: string }[]>(GROUPS_KEY, []));
  const [groupMap, setGroupMap] = useState<Record<string, string>>(() => readJson<Record<string, string>>(GROUP_MAP_KEY, {}));

  useEffect(() => writeJson(FAVORITES_KEY, favorites), [favorites]);
  useEffect(() => writeJson(REMARKS_KEY, remarks), [remarks]);
  useEffect(() => writeJson(ALIASES_KEY, aliases), [aliases]);
  useEffect(() => writeJson(EXPIRIES_KEY, expiries), [expiries]);
  useEffect(() => writeJson(COLUMNS_KEY, columns), [columns]);
  useEffect(() => writeJson(GROUPS_KEY, groups), [groups]);
  useEffect(() => writeJson(GROUP_MAP_KEY, groupMap), [groupMap]);
  useEffect(() => {
    if (!rowMenu && !expiryDomain && !editingDomain) return;
    const close = (event: MouseEvent) => {
      if (event.target instanceof Element && event.target.closest(".web-row-menu, .web-expiry-pop")) return;
      setRowMenu(null); setExpiryDomain(null);
    };
    window.addEventListener("mousedown", close);
    return () => window.removeEventListener("mousedown", close);
  }, [rowMenu, expiryDomain, editingDomain]);

  const action = useMutation({
    mutationFn: (input: { domain: string; action: "enable" | "disable" | "delete" }) => api.websiteAction({ serverId, ...input, confirmed: true }),
    onSuccess: async (result, variables) => {
      setSelected((current) => current.filter((domain) => domain !== variables.domain));
      await Promise.all([queryClient.invalidateQueries({ queryKey: ["websites", serverId] }), queryClient.invalidateQueries({ queryKey: ["certificate-renewal-plan"] })]);
      if (result && Array.isArray(result.warnings)) setRowMenu(null);
    },
  });
  const nginxOp = useMutation({
    mutationFn: (operation: "stop" | "start" | "restart" | "reload") => api.websiteNginxService({ serverId, action: operation, confirmed: true }),
    onSuccess: async () => { await queryClient.invalidateQueries({ queryKey: ["websites", serverId] }); },
  });

  const rows = useMemo(() => {
    const list = snapshot?.websites ?? [];
    const keyword = query.trim().toLocaleLowerCase();
    const fav = new Set(favorites);
    return list
      .filter((website) => {
        if (kindFilter && website.kind !== kindFilter) return false;
        if (groupFilter && (groupMap[website.domain] ?? "") !== groupFilter) return false;
        if (keyword && !website.domain.toLocaleLowerCase().includes(keyword)) return false;
        return true;
      })
      .sort((a, b) => Number(fav.has(b.domain)) - Number(fav.has(a.domain)));
  }, [snapshot, kindFilter, groupFilter, query, groupMap, favorites]);

  const gridTemplate = `${columns.filter((key) => columns.includes(key)).map((key) => COLUMN_WIDTHS[key]).join(" ")}`;

  const openConfig = (website: WebsiteRecord) => window.open(`/servers/${serverId}/files?open=${encodeURIComponent(website.configPath)}`, "_self");
  const openRoot = (website: WebsiteRecord) => { if (website.rootPath) window.open(`/servers/${serverId}/files?open=${encodeURIComponent(website.rootPath)}`, "_self"); };
  const openSite = (website: WebsiteRecord) => {
    const protocol = website.ssl ? "https" : "http";
    const port = website.ssl ? 443 : website.listenPort;
    const suffix = port && port !== (website.ssl ? 443 : 80) ? `:${port}` : "";
    window.open(`${protocol}://${website.domain}${suffix}`, "_blank", "noopener");
  };
  const runAction = (domain: string, operation: "enable" | "disable" | "delete", alsoClearMenu = true) => {
    const verb = operation === "delete" ? "删除" : operation === "enable" ? "启动" : "停止";
    const detail = operation === "delete" ? "该操作会删除远端配置，请谨慎。" : operation === "disable" ? "停止后该站点将无法访问。" : "启动后该站点将恢复访问。";
    setConfirm({
      title: `确认${verb}网站 ${domain}？`,
      message: detail,
      danger: operation === "delete",
      confirmText: `确认${verb}`,
      run: () => { action.mutate({ domain, action: operation }); if (alsoClearMenu) setRowMenu(null); },
    });
  };
  const runNginxOp = (operation: "stop" | "start" | "restart" | "reload") => {
    const verb = operation === "stop" ? "停止" : operation === "start" ? "启动" : operation === "restart" ? "重启" : "重载";
    setConfirm({
      title: `确认${verb} OpenResty 服务？`,
      message: operation === "stop" ? "停止后站点将无法访问。" : `将对远端 OpenResty 服务执行${verb}操作。`,
      confirmText: `确认${verb}`,
      run: () => nginxOp.mutate(operation),
    });
  };
  const openNginxConfig = () => {
    if (snapshot?.managedConfDir) { setAdvancedOpen(false); window.open(`/servers/${serverId}/files?open=${encodeURIComponent(snapshot.managedConfDir)}`, "_self"); }
  };
  const startAliasEdit = (website: WebsiteRecord) => { setAliasDraft(aliases[website.domain] ?? website.domain); setEditingDomain(website.domain); };
  const commitAlias = (website: WebsiteRecord) => {
    const value = aliasDraft.trim();
    setAliases((current) => ({ ...current, [website.domain]: value && value !== website.domain ? value : current[website.domain] }));
    setEditingDomain(null);
  };
  const cancelAlias = () => setEditingDomain(null);
  const setExpiry = (domain: string, value: string) => { setExpiries((current) => { const next = { ...current }; if (value) next[domain] = value; else delete next[domain]; return next; }); setExpiryDomain(null); };
  const runBatch = () => {
    if (!selected.length || !batchAction || batchAction === "请选择") return;
    if (batchAction === "set-group") { setPendingAssign(selected); setAssignGroup(true); return; }
    if (batchAction === "set-https") { setCertificateDefaults({ domain: selected[0] }); setCertificateOpen(true); setBatchAction(""); return; }
    const verb = batchAction === "enable" ? "启动" : batchAction === "disable" ? "停止" : "删除";
    const op = batchAction === "enable" ? "enable" : batchAction === "disable" ? "disable" : "delete";
    setConfirm({
      title: `确认${verb}选中的 ${selected.length} 个网站？`,
      message: batchAction === "delete" ? "该操作会删除远端配置，请谨慎。" : batchAction === "disable" ? "停止后这些站点将无法访问。" : "启动后这些站点将恢复访问。",
      danger: batchAction === "delete",
      confirmText: `确认${verb}`,
      run: () => { selected.forEach((domain) => action.mutate({ domain, action: op })); setBatchAction(""); },
    });
  };
  const toggleFavorite = (domain: string) => setFavorites((current) => current.includes(domain) ? current.filter((item) => item !== domain) : [...current, domain]);
  const setRemark = (domain: string, value: string) => setRemarks((current) => ({ ...current, [domain]: value }));
  const toggleColumn = (key: ColumnKey) => setColumns((current) => current.includes(key) ? current.filter((item) => item !== key) : [...current, key]);
  const assignSelectedGroup = (groupId: string) => {
    setGroupMap((current) => { const next = { ...current }; pendingAssign.forEach((domain) => { if (groupId) next[domain] = groupId; else delete next[domain]; }); return next; });
    setAssignGroup(false); setPendingAssign([]); setBatchAction("");
  };
  const addGroupName = (name: string) => { if (name.trim()) setGroups((current) => [...current, { id: `g-${Date.now()}`, name: name.trim() }]); };
  const renameGroup = (id: string, name: string) => setGroups((current) => current.map((group) => group.id === id ? { ...group, name: name.trim() || group.name } : group));
  const deleteGroup = (id: string) => { setGroups((current) => current.filter((group) => group.id !== id)); setGroupMap((current) => { const next = { ...current }; Object.keys(next).forEach((domain) => { if (next[domain] === id) delete next[domain]; }); return next; }); };

  return <section className="website-page">
    {snapshot && <div className="web-openresty-banner"><span className="web-openresty-tag">OpenResty</span><span className={`web-nginx-state ${snapshot.supported ? "is-ok" : "is-down"}`}>{snapshot.supported ? "已启动" : "未运行"}</span><span className="web-nginx-version"><span>版本: {snapshot.nginxVersion ?? "—"}</span></span><span className="web-nginx-actions">
      {nginxOp.isPending
        ? <span className="web-nginx-op-pending"><RefreshCw size={12} className="spin" /> 处理中…</span>
        : <>{(snapshot.supported
          ? <button onClick={() => runNginxOp("stop")}>停止</button>
          : <button onClick={() => runNginxOp("start")}>启动</button>)}
        <span className="web-nginx-divider" />
        <button onClick={() => runNginxOp("restart")}>重启</button>
        <span className="web-nginx-divider" />
        <button onClick={() => runNginxOp("reload")} disabled={!snapshot.supported}>重载</button>
        <span className="web-nginx-divider" />
        <button onClick={openNginxConfig} disabled={!snapshot.managedConfDir}>设置</button></>}
    </span></div>}

    {nginxOp.error && <div className="form-error website-error">{errorMessage(nginxOp.error)}</div>}

    <div className="web-toolbar">
      <div className="web-toolbar__left">
        <Button variant="primary" onClick={() => setCreateOpen(true)} disabled={!snapshot?.supported}><Plus size={14} /> 创建</Button>
        <Button variant="secondary" onClick={() => setGroupsOpen(true)}><Layers size={14} /> 分组</Button>
        <Button variant="secondary" onClick={() => setAdvancedOpen(true)}><SlidersHorizontal size={14} /> 高级设置</Button>
      </div>
      <div className="web-toolbar__right">
        <span className="web-view-toggle">
          <button className={view === "table" ? "is-active" : ""} onClick={() => setView("table")} title="表格视图"><LayoutList size={15} /></button>
          <button className={view === "card" ? "is-active" : ""} onClick={() => setView("card")} title="卡片视图"><LayoutGrid size={15} /></button>
        </span>
        <label className="web-filter"><span>类型</span><select value={kindFilter} onChange={(event) => setKindFilter(event.target.value)}><option value="">请选择</option><option value="static">静态网站</option><option value="proxy">反向代理</option></select></label>
        <label className="web-filter"><span>分组</span><select value={groupFilter} onChange={(event) => setGroupFilter(event.target.value)}><option value="">所有</option>{groups.map((group) => <option key={group.id} value={group.id}>{group.name}</option>)}</select></label>
        <label className="web-search"><input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="搜索" /><Search size={14} /></label>
        <button className="icon-control" onClick={() => void websites.refetch()} disabled={websites.isFetching} title="刷新"><RefreshCw size={15} className={websites.isFetching ? "spin" : ""} /></button>
        <span className="web-toggle-anchor">
          <button className="icon-control" onClick={() => setColumnsOpen((open) => !open)} title="列设置"><Settings size={15} /></button>
          {columnsOpen && <div className="web-columns-menu" onMouseDown={(event) => event.stopPropagation()}><header>列设置</header>{ALL_COLUMNS.map((key) => <label key={key}><input type="checkbox" checked={columns.includes(key)} onChange={() => toggleColumn(key)} /><span>{COLUMN_LABELS[key]}</span></label>)}</div>}
        </span>
      </div>
    </div>

    {websites.isLoading && <div className="page-state">正在读取受控网站配置…</div>}
    {websites.error && <div className="page-state page-state--error">{errorMessage(websites.error)}</div>}
    {action.error && <div className="form-error website-error">{errorMessage(action.error)}</div>}
    {snapshot && !snapshot.supported && <div className="warning-panel"><Globe2 size={16} /><span>当前服务器没有可用的 Nginx/OpenResty 受控配置目录，网站功能不可用。{snapshot.warnings.join(" · ")}</span></div>}

    {snapshot && view === "table" && rows.length > 0 && <div className="web-table-wrap">
      <div className="web-table">
        <div className="ops-head web-grid" style={{ gridTemplateColumns: `44px ${gridTemplate}` }}>
          <label className="web-check"><input type="checkbox" checked={rows.length > 0 && rows.every((website) => selected.includes(website.domain))} onChange={(event) => setSelected(event.target.checked ? rows.map((website) => website.domain) : [])} aria-label="选择所有行" /></label>
          {columns.includes("domain") && <span>名称 <ArrowUpDown size={12} className="web-sort-icon" /></span>}
          {columns.includes("kind") && <span>类型 <ArrowUpDown size={12} className="web-sort-icon" /></span>}
          {columns.includes("path") && <span>网站目录</span>}
          {columns.includes("status") && <span>状态 <ArrowUpDown size={12} className="web-sort-icon" /></span>}
          {columns.includes("protocol") && <span>协议</span>}
          {columns.includes("expire") && <span>过期时间 <ArrowUpDown size={12} className="web-sort-icon" /></span>}
          {columns.includes("ssl") && <span>证书过期时间</span>}
          {columns.includes("remark") && <span>备注</span>}
          {columns.includes("operations") && <span className="web-ops-cell">操作</span>}
        </div>
        {rows.map((website) => (
          <div className="ops-row web-grid" style={{ gridTemplateColumns: `44px ${gridTemplate}` }} key={website.domain}>
            <label className="web-check"><input type="checkbox" checked={selected.includes(website.domain)} onChange={(event) => setSelected((current) => event.target.checked ? [...current, website.domain] : current.filter((domain) => domain !== website.domain))} aria-label={`选择 ${website.domain}`} /></label>
            {columns.includes("domain") && <div className="web-domain-cell">
              {editingDomain === website.domain
                ? <input className="web-domain-edit" autoFocus value={aliasDraft} onChange={(event) => setAliasDraft(event.target.value)} onBlur={() => commitAlias(website)} onKeyDown={(event) => { if (event.key === "Enter") commitAlias(website); if (event.key === "Escape") cancelAlias(); }} aria-label="编辑域名" />
                : <>
                  <button className="web-domain" title="打开网站配置" onClick={() => openConfig(website)}>{aliases[website.domain] ?? website.domain}</button>
                  <button className="web-icon-btn" title="打开网站" onClick={() => openSite(website)}><Send size={13} /></button>
                  <button className="web-icon-btn" title="重命名" onClick={() => startAliasEdit(website)}><Pencil size={13} /></button>
                  <button className={`web-icon-btn web-pin ${favorites.includes(website.domain) ? "is-fav" : ""}`} onClick={() => toggleFavorite(website.domain)} title={favorites.includes(website.domain) ? "取消置顶" : "置顶"}><Pin size={13} /></button>
                </>}
            </div>}
            {columns.includes("kind") && <span className="web-kind">{website.kind === "static" ? "静态网站" : "反向代理"}</span>}
            {columns.includes("path") && <div className="web-path-cell"><button className="web-icon-btn" onClick={() => openRoot(website)} disabled={!website.rootPath} title={website.rootPath ?? "无站点目录"}><FolderOpen size={14} /></button></div>}
            {columns.includes("status") && <button className={`web-state-pill ${website.enabled ? "is-ok" : "is-down"}`} onClick={() => runAction(website.domain, website.enabled ? "disable" : "enable")} title={website.enabled ? "点击停止" : "点击启动"}>{website.enabled ? "已启动" : "已停止"}<ChevronRight size={11} /></button>}
            {columns.includes("protocol") && <span className="web-protocol">{website.ssl ? "HTTPS" : "HTTP"}</span>}
            {columns.includes("expire") && <span className="web-expire-anchor">
              <span className="web-link web-expire-link" onClick={() => setExpiryDomain((current) => current === website.domain ? null : website.domain)}>{expiries[website.domain] ? fmtDate(expiries[website.domain]) : "永不过期"}</span>
              {expiryDomain === website.domain && <div className="web-expiry-pop" onMouseDown={(event) => event.stopPropagation()}>
                <div className="web-expiry-shortcuts"><button onClick={() => setExpiry(website.domain, "")}>永不过期</button><button onClick={() => { const date = new Date(); date.setFullYear(date.getFullYear() + 1); setExpiry(website.domain, `${date.getFullYear()}-${`${date.getMonth() + 1}`.padStart(2, "0")}-${`${date.getDate()}`.padStart(2, "0")}`); }}>明年</button></div>
                <div className="web-expiry-date"><input type="date" value={expiries[website.domain] ?? ""} onChange={(event) => setExpiry(website.domain, event.target.value)} /><button onClick={() => setExpiryDomain(null)}>确定</button></div>
              </div>}
            </span>}
            {columns.includes("ssl") && <span>{website.ssl && website.expiresAt
              ? <span className={`web-ssl-tag ${sslTagClass(website)}`}>{fmtDate(website.expiresAt)}</span>
              : website.ssl ? <span className="web-muted">—</span> : null}</span>}
            {columns.includes("remark") && <input className="web-remark-input" value={remarks[website.domain] ?? ""} placeholder="—" onChange={(event) => setRemark(website.domain, event.target.value)} />}
            {columns.includes("operations") && <div className="web-ops-cell">
              <button className="web-text-btn" onClick={() => openConfig(website)} disabled={action.isPending}>配置</button>
              <span className="web-more-anchor">
                <button className="web-text-btn web-more-btn" onClick={(event) => { event.stopPropagation(); setRowMenu((current) => current?.domain === website.domain ? null : { domain: website.domain, x: event.clientX, y: event.clientY }); }}>更多 <ChevronDown size={12} /></button>
                {rowMenu?.domain === website.domain && <div className="web-row-menu" style={{ left: -150, top: 26 }} onMouseDown={(event) => event.stopPropagation()}>
                  <button disabled title="网站备份由 1Panel 服务端管理，可在 Web 面板中操作"><Archive size={13} /> 备份列表</button>
                  <button disabled title="网站备份由 1Panel 服务端管理，可在 Web 面板中操作"><Upload size={13} /> 导入备份</button>
                  <button onClick={() => runAction(website.domain, "delete")} disabled={action.isPending}><Trash2 size={13} /> 删除</button>
                </div>}
              </span>
            </div>}
          </div>
        ))}
      </div>
      <div className="web-batch-bar">
        <div className="web-batch-bar__left">
          <label className="web-check"><input type="checkbox" checked={selected.length > 0 && rows.length > 0 && selected.length === rows.length} onChange={(event) => setSelected(event.target.checked ? rows.map((website) => website.domain) : [])} aria-label="批量选择" /></label>
          <select value={batchAction} onChange={(event) => setBatchAction(event.target.value)}><option value="">请选择</option><option value="enable">启动网站</option><option value="disable">停止网站</option><option value="delete">删除网站</option><option value="set-group">设置分组</option><option value="set-https">设置SSL</option></select>
          <Button variant="primary" size="sm" onClick={runBatch} disabled={!selected.length || !batchAction}>批量操作{selected.length ? `(${selected.length})` : ""}</Button>
        </div>
        <div className="web-batch-bar__right">
          <span className="web-total">共 {rows.length} 条</span>
          <select className="web-page-size" defaultValue="20"><option value="10">10 条/页</option><option value="20">20 条/页</option><option value="50">50 条/页</option><option value="100">100 条/页</option></select>
          <span className="web-pager"><button disabled aria-label="上一页">‹</button><span className="is-active">1</span><button disabled aria-label="下一页">›</button></span>
          <span className="web-jump">前往 <input className="web-jump-input" defaultValue="1" aria-label="跳转页码" /> 页</span>
        </div>
      </div>
    </div>}

    {snapshot && view === "card" && rows.length > 0 && <div className="website-list">
      {rows.map((website) => <article className="website-card" key={website.domain}><div className="website-card__icon"><Globe2 size={20} /></div><div className="website-card__body"><header><div><h2>{aliases[website.domain] ?? website.domain}</h2><span className="website-kind">{website.kind === "static" ? "静态网站" : "反向代理"}</span></div><span className={website.enabled ? "status-chip status-chip--ok" : "status-chip status-chip--warn"}>{website.enabled ? "已启动" : "已停止"}</span></header><div className="website-meta"><span>监听 {website.listenPort}</span>{website.rootPath && <span className="mono">root {website.rootPath}</span>}{website.upstream && <span className="mono">upstream {website.upstream}</span>}{website.ssl && <span><LockKeyhole size={12} /> HTTPS {website.expiresAt ? `· 到期 ${fmtDate(website.expiresAt)}` : "· 已配置"}</span>}</div><small className="mono">{website.configPath}</small></div><div className="website-actions">{website.enabled ? <Button variant="ghost" size="sm" onClick={() => runAction(website.domain, "disable")} disabled={action.isPending}>停止</Button> : <Button variant="primary" size="sm" onClick={() => runAction(website.domain, "enable")} disabled={action.isPending}>启动</Button>}<Button variant="ghost" size="sm" onClick={() => openConfig(website)}>配置</Button><Button variant="danger" size="sm" onClick={() => runAction(website.domain, "delete")} disabled={action.isPending}><Trash2 size={13} />删除</Button></div></article>)}
    </div>}

    {snapshot && rows.length === 0 && <div className="empty-panel website-empty"><Globe2 size={28} /><h2>{query || kindFilter ? "没有匹配的网站" : "还没有受控网站"}</h2><p>{query || kindFilter ? "调整搜索或类型筛选条件。" : "创建静态站点或反向代理后，客户端会在远端 conf.d 中生成独立配置。"}</p>{!query && !kindFilter && <Button variant="primary" onClick={() => setCreateOpen(true)} disabled={!snapshot.supported}><Plus size={14} /> 创建第一个网站</Button>}</div>}

    {snapshot && snapshot.warnings.length > 0 && <div className="warning-panel"><RefreshCw size={16} /><span>{snapshot.warnings.join(" · ")}</span></div>}

    {createOpen && <WebsiteCreateDialog key={serverId} serverId={serverId} snapshot={snapshot} open={createOpen} onClose={() => setCreateOpen(false)} />}
    {certificateOpen && <SslCertificateDialog key={serverId} serverId={serverId} snapshot={snapshot} open={certificateOpen} onClose={() => setCertificateOpen(false)} defaults={certificateDefaults} />}
    {phpOpen && <PhpInstallDialog key={serverId} serverId={serverId} open={phpOpen} onClose={() => setPhpOpen(false)} />}

    <Dialog.Root open={!!confirm} onOpenChange={(open) => !open && setConfirm(null)}><Dialog.Portal><Dialog.Overlay className="dialog-overlay" /><Dialog.Content className="dialog-content dialog-content--narrow confirm-dialog"><div className="destructive-icon"><ShieldAlert size={22} /></div><Dialog.Title>{confirm?.title}</Dialog.Title><Dialog.Description>{confirm?.message}</Dialog.Description>{action.error && <div className="form-error">{errorMessage(action.error)}</div>}<div className="dialog-actions"><Button variant="ghost" onClick={() => setConfirm(null)}>取消</Button><Button variant={confirm?.danger ? "danger" : "primary"} onClick={() => { const run = confirm?.run; setConfirm(null); run?.(); }} disabled={action.isPending || nginxOp.isPending}>{action.isPending || nginxOp.isPending ? "执行中…" : confirm?.confirmText ?? "确认执行"}</Button></div></Dialog.Content></Dialog.Portal></Dialog.Root>

    {groupsOpen && <div className="website-form-backdrop" onMouseDown={() => setGroupsOpen(false)}><section className="website-form-card website-group-card" onMouseDown={(event) => event.stopPropagation()}>
      <header><div><span className="section-kicker">Groups</span><h2>网站分组</h2><p>分组仅用于客户端筛选展示，远端配置不受影响。</p></div><button type="button" className="icon-control" onClick={() => setGroupsOpen(false)} aria-label="关闭">×</button></header>
      <div className="website-group-list">
        {groups.map((group) => <div className="website-group-row" key={group.id}><input value={group.name} onChange={(event) => renameGroup(group.id, event.target.value)} /><button className="web-text-btn is-danger" onClick={() => deleteGroup(group.id)}>删除</button></div>)}
        {!groups.length && <div className="empty-panel empty-panel--small"><span>还没有分组。</span></div>}
      </div>
      <footer><input className="website-group-input" placeholder="新分组名称" onKeyDown={(event) => { if (event.key === "Enter") { addGroupName(event.currentTarget.value); event.currentTarget.value = ""; } }} /><Button variant="primary" size="sm" onClick={() => { const input = document.querySelector<HTMLInputElement>(".website-group-input"); if (input?.value.trim()) { addGroupName(input.value); input.value = ""; } }}>添加</Button></footer>
    </section></div>}

    {advancedOpen && <div className="website-form-backdrop" onMouseDown={() => setAdvancedOpen(false)}><section className="website-form-card website-group-card" onMouseDown={(event) => event.stopPropagation()}>
      <header><div><span className="section-kicker">Advanced</span><h2>高级设置</h2><p>站点级进阶能力与 web 面板一致，客户端保留可操作的本地项。</p></div><button type="button" className="icon-control" onClick={() => setAdvancedOpen(false)} aria-label="关闭">×</button></header>
      <div className="website-advanced-list">
        <div className="website-advanced-row"><div><strong>列设置</strong><small>自定义表格显示的列。</small></div><Button variant="secondary" size="sm" onClick={() => { setColumnsOpen(true); setAdvancedOpen(false); }}>调整</Button></div>
        <div className="website-advanced-row"><div><strong>OpenResty 配置</strong><small>打开远端 conf.d 配置文件目录。</small></div><Button variant="secondary" size="sm" onClick={openNginxConfig} disabled={!snapshot?.managedConfDir}>打开</Button></div>
        <div className="website-advanced-row"><div><strong>PHP-FPM 运行时</strong><small>探测并安装 PHP 运行时。</small></div><Button variant="secondary" size="sm" onClick={() => { setPhpOpen(true); setAdvancedOpen(false); }}>PHP-FPM</Button></div>
      </div>
    </section></div>}

    {assignGroup && <div className="website-form-backdrop" onMouseDown={() => setAssignGroup(false)}><section className="website-form-card website-group-card" onMouseDown={(event) => event.stopPropagation()}>
      <header><div><span className="section-kicker">Assign group</span><h2>设置分组 · {pendingAssign.length} 个网站</h2><p>选择要归入的分组。</p></div><button type="button" className="icon-control" onClick={() => setAssignGroup(false)} aria-label="关闭">×</button></header>
      <div className="website-group-list">
        <button className={`website-group-pick ${pendingAssign.every((domain) => !groupMap[domain]) ? "is-selected" : ""}`} onClick={() => assignSelectedGroup("")}><span>不分组</span></button>
        {groups.map((group) => <button key={group.id} className={`website-group-pick ${pendingAssign.every((domain) => groupMap[domain] === group.id) ? "is-selected" : ""}`} onClick={() => assignSelectedGroup(group.id)}><span>{group.name}</span><small>{Object.values(groupMap).filter((id) => id === group.id).length} 个网站</small></button>)}
        {!groups.length && <div className="empty-panel empty-panel--small"><span>还没有分组，请先在「分组」中创建。</span></div>}
      </div>
    </section></div>}
  </section>;
}

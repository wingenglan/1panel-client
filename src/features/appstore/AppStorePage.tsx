import * as Dialog from "@radix-ui/react-dialog";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ChevronDown, Download, ExternalLink, Eye, FolderOpen, Info, MoreHorizontal, Package, Play, RefreshCw, Search, Settings2, X, Zap } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useParams } from "react-router-dom";
import { Button } from "../../components/ui/Button";
import { Pager } from "../../components/ui/Pager";
import { api } from "../../lib/api";
import { errorMessage } from "../../lib/errors";
import type { AppCatalogItem, AppDetail, AppUpdatePreview, InstalledApp } from "../../types/server";

type AppTab = "catalog" | "installed" | "upgrades" | "settings";

/** 与 Web 端应用商店一致的固定分类顺序，主行显示前 12 项，其余收进「更多」。 */
const CATEGORY_ORDER = ["全部", "AI", "建站", "数据库", "Web 服务器", "运行环境", "实用工具", "云存储", "BI", "CRM", "安全", "开发工具", "DevOps", "中间件", "多媒体", "邮件服务", "休闲游戏", "本地"];
const MAIN_CATEGORIES = CATEGORY_ORDER.slice(0, 12);
const MORE_CATEGORIES = CATEGORY_ORDER.slice(12);
const CATALOG_PAGE_SIZE = 60;
const INSTALLED_PAGE_SIZE = 20;

const PREFS_KEY = "appstore-behavior-preferences";
interface AppBehaviorPreferences { uninstallDeleteBackup: boolean; uninstallDeleteImage: boolean; backupBeforeUpgrade: boolean; upgradeDeleteOldImage: boolean; defaultOpenPort: boolean; }
const DEFAULT_PREFS: AppBehaviorPreferences = { uninstallDeleteBackup: false, uninstallDeleteImage: false, backupBeforeUpgrade: true, upgradeDeleteOldImage: false, defaultOpenPort: false };
function loadPrefs(): AppBehaviorPreferences {
  try { return { ...DEFAULT_PREFS, ...JSON.parse(localStorage.getItem(PREFS_KEY) ?? "{}") }; } catch { return DEFAULT_PREFS; }
}

const IGNORE_KEY = (serverId: string) => `appstore-ignored-${serverId}`;
function loadIgnored(serverId: string): string[] {
  try { return JSON.parse(localStorage.getItem(IGNORE_KEY(serverId)) ?? "[]"); } catch { return []; }
}

const ICON_COLORS = ["#2472c8", "#2f9e4f", "#e8a33d", "#c86c3c", "#7a4fc8", "#3a9d9d", "#c84f6c", "#4f7ac8", "#8a6fc8", "#3ea85c", "#c8653c", "#5a8fc8"];
function iconColor(key: string) {
  let hash = 0;
  for (const char of key) hash = (hash * 31 + char.charCodeAt(0)) >>> 0;
  return ICON_COLORS[hash % ICON_COLORS.length];
}

function formatUptime(seconds: number | null | undefined) {
  if (!seconds) return "";
  const days = Math.floor(seconds / 86400);
  const hours = Math.floor((seconds % 86400) / 3600);
  if (days > 0) return `${days} 天 ${hours} 小时`;
  const minutes = Math.floor((seconds % 3600) / 60);
  if (hours > 0) return `${hours} 小时 ${minutes} 分钟`;
  return `${minutes} 分钟`;
}

/** 展示 Web 端 1:1 的应用商店：分类行、工具栏、卡片网格、分页和三类列表页。 */
export function AppStorePage() {
  const { serverId = "" } = useParams();
  const queryClient = useQueryClient();
  const [tab, setTab] = useState<AppTab>("catalog");
  const [category, setCategory] = useState("全部");
  const [query, setQuery] = useState("");
  const [catalogPage, setCatalogPage] = useState(1);
  const [catalogPageSize, setCatalogPageSize] = useState(CATALOG_PAGE_SIZE);
  const [installedPage, setInstalledPage] = useState(1);
  const [installedPageSize, setInstalledPageSize] = useState(INSTALLED_PAGE_SIZE);
  const [archOnly, setArchOnly] = useState(false);
  const [localOnly, setLocalOnly] = useState(true);
  const [moreOpen, setMoreOpen] = useState(false);
  const [sortKey, setSortKey] = useState<"name" | "status" | "uptime">("uptime");
  const [sortOpen, setSortOpen] = useState(false);
  const [ignored, setIgnored] = useState<string[]>(() => loadIgnored(serverId));
  const [prefs, setPrefs] = useState<AppBehaviorPreferences>(loadPrefs);
  const [backupInfoApp, setBackupInfoApp] = useState<InstalledApp | null>(null);
  const [ignoredOpen, setIgnoredOpen] = useState(false);
  const moreRef = useRef<HTMLSpanElement | null>(null);
  const moreMenuRef = useRef<HTMLDivElement | null>(null);
  const [selected, setSelected] = useState<AppDetail | null>(null);
  const [version, setVersion] = useState("");
  const [project, setProject] = useState("");
  const [installPath, setInstallPath] = useState("");
  const [environment, setEnvironment] = useState("");
  const [environmentApp, setEnvironmentApp] = useState<InstalledApp | null>(null);
  const [environmentOverrides, setEnvironmentOverrides] = useState("");
  const [healthApp, setHealthApp] = useState<InstalledApp | null>(null);
  const [previewApp, setPreviewApp] = useState<InstalledApp | null>(null);

  const catalog = useQuery({ queryKey: ["app-catalog"], queryFn: api.appCatalog, staleTime: 10 * 60_000 });
  const installed = useQuery({ queryKey: ["installed-apps", serverId], queryFn: () => api.installedApps(serverId), enabled: Boolean(serverId), refetchInterval: 15_000 });
  const upgradeable = useQuery({
    queryKey: ["appstore-upgradeable", serverId, installed.dataUpdatedAt],
    enabled: tab === "upgrades" && Boolean(installed.data?.apps.length),
    staleTime: 5 * 60_000,
    queryFn: async () => {
      const results = await Promise.all((installed.data?.apps ?? []).map(async (app) => {
        try {
          const preview = await api.appUpdatePreview({ serverId, key: app.key, project: app.project, installPath: app.path });
          return preview.changed ? app : null;
        } catch { return null; }
      }));
      return results.filter((app): app is InstalledApp => app !== null);
    },
  });
  const appEnvironment = useQuery({ queryKey: ["app-environment", serverId, environmentApp?.path], queryFn: () => api.appEnvironment(serverId, environmentApp!.path), enabled: Boolean(environmentApp) });
  const appHealth = useQuery({ queryKey: ["app-health", serverId, healthApp?.path], queryFn: () => api.appHealth({ serverId, project: healthApp!.project, installPath: healthApp!.path }), enabled: Boolean(healthApp), staleTime: 5_000 });
  const appPreview = useMutation({ mutationFn: api.appUpdatePreview });
  const detail = useMutation({ mutationFn: api.appDetail, onSuccess: (value) => { setSelected(value); setVersion(value.versions[0]?.version ?? ""); setProject(value.key); setInstallPath(`/opt/1panel/apps/${value.key}`); setEnvironment(""); } });
  const install = useMutation({ mutationFn: () => api.installApp({ serverId, key: selected!.key, version, project, installPath, environment: environment.split("\n").map((line) => line.trim()).filter(Boolean), confirmed: true }), onSuccess: async () => { setSelected(null); setTab("installed"); setInstalledPage(1); await queryClient.invalidateQueries({ queryKey: ["installed-apps", serverId] }); } });
  const action = useMutation({ mutationFn: (input: { app: InstalledApp; action: "start" | "stop" | "restart" | "pull" | "update" | "uninstall" | "restore" }) => api.appAction({ serverId, key: input.app.key, project: input.app.project, installPath: input.app.path, action: input.action, confirmed: true }), onSuccess: async () => { await queryClient.invalidateQueries({ queryKey: ["installed-apps", serverId] }); } });
  const saveEnvironment = useMutation({ mutationFn: () => api.saveAppEnvironment({ serverId, installPath: environmentApp!.path, values: environmentOverrides.split("\n").map((line) => line.trim()).filter(Boolean), confirmed: true }), onSuccess: async () => { setEnvironmentApp(null); setEnvironmentOverrides(""); await queryClient.invalidateQueries({ queryKey: ["app-environment", serverId] }); } });

  const categoryByKey = useMemo(() => new Map((catalog.data?.items ?? []).map((item) => [item.key, item.category])), [catalog.data]);
  const installedKeys = useMemo(() => new Set((installed.data?.apps ?? []).map((app) => app.key)), [installed.data]);
  const catalogItems = useMemo(() => filterCatalog(catalog.data?.items ?? [], query, category), [catalog.data, category, query]);
  const catalogSlice = catalogItems.slice((catalogPage - 1) * catalogPageSize, catalogPage * catalogPageSize);
  /** 按用户选择排序应用副本，保持查询缓存不变。 */
  const sortApps = useCallback((apps: InstalledApp[]) => {
    return [...apps].sort((a, b) => {
      if (sortKey === "name") return a.key.localeCompare(b.key);
      if (sortKey === "status") return (a.status === "running" ? 0 : 1) - (b.status === "running" ? 0 : 1);
      return (b.installedSeconds ?? 0) - (a.installedSeconds ?? 0);
    });
  }, [sortKey]);
  const installedApps = useMemo(() => filterInstalled(sortApps(installed.data?.apps ?? []), query, category, categoryByKey), [installed.data, query, category, categoryByKey, sortApps]);
  const installedSlice = installedApps.slice((installedPage - 1) * installedPageSize, installedPage * installedPageSize);
  const upgradeApps = useMemo(() => filterInstalled(sortApps((installed.data?.apps ?? []).filter((app) => !ignored.includes(app.key))), query, category, categoryByKey), [installed.data, query, category, categoryByKey, ignored, sortApps]);

  useEffect(() => {
    if (!moreOpen && !sortOpen) return;
    const close = (event: MouseEvent) => {
      if (moreRef.current && !moreRef.current.contains(event.target as Node) && !moreMenuRef.current?.contains(event.target as Node)) setMoreOpen(false);
      if (sortedRef.current && !sortedRef.current.contains(event.target as Node) && !sortMenuRef.current?.contains(event.target as Node)) setSortOpen(false);
    };
    document.addEventListener("mousedown", close);
    return () => document.removeEventListener("mousedown", close);
  }, [moreOpen, sortOpen]);
  const sortedRef = useRef<HTMLButtonElement | null>(null);
  const sortMenuRef = useRef<HTMLDivElement | null>(null);

  const openSettingsTab = () => setTab("settings");
  const openDetail = (item: AppCatalogItem) => detail.mutate(item.key);
  const runAction = (app: InstalledApp, actionName: "start" | "stop" | "restart" | "pull" | "update" | "uninstall" | "restore") => {
    if (["stop", "uninstall", "update"].includes(actionName) && !window.confirm(`确认对 ${app.key} 执行 ${actionName}？`)) return;
    action.mutate({ app, action: actionName });
  };
  const openEnvironment = (app: InstalledApp) => { setEnvironmentOverrides(""); setEnvironmentApp(app); };
  const openHealth = (app: InstalledApp) => setHealthApp(app);
  const openPreview = (app: InstalledApp) => { setPreviewApp(app); appPreview.mutate({ serverId, key: app.key, project: app.project, installPath: app.path }); };
  const toggleIgnored = (key: string) => {
    setIgnored((current) => {
      const next = current.includes(key) ? current.filter((item) => item !== key) : [...current, key];
      localStorage.setItem(IGNORE_KEY(serverId), JSON.stringify(next));
      return next;
    });
  };
  const updatePrefs = (next: AppBehaviorPreferences) => { setPrefs(next); localStorage.setItem(PREFS_KEY, JSON.stringify(next)); };
  const copyPath = async (app: InstalledApp) => { try { await navigator.clipboard.writeText(app.path); } catch { /* 剪贴板不可用时忽略 */ } };
  const selectCategory = (item: string) => { setCategory(item); setMoreOpen(false); setCatalogPage(1); setInstalledPage(1); };

  return <section className="appstore-page">
    <div className="appstore-radios">
      {(["catalog", "installed", "upgrades", "settings"] as const).map((key) => <button key={key} className={tab === key ? "is-active" : ""} onClick={() => key === "settings" ? openSettingsTab() : setTab(key)}>
        {key === "catalog" ? "全部" : key === "installed" ? "已安装" : key === "upgrades" ? <span className="appstore-radio-label">可升级{upgradeable.data?.length ? <sup>{upgradeable.data.length}</sup> : null}</span> : "设置"}
      </button>)}
    </div>
    {catalog.error && <div className="page-state page-state--error">{errorMessage(catalog.error)}</div>}
    {installed.error && <div className="page-state page-state--error">{errorMessage(installed.error)}</div>}
    {appEnvironment.error && <div className="page-state page-state--error">{errorMessage(appEnvironment.error)}</div>}
    {appHealth.error && <div className="page-state page-state--error">{errorMessage(appHealth.error)}</div>}
    {tab !== "settings" && <div className="appstore-categories">
      {MAIN_CATEGORIES.map((item) => <button key={item} className={category === item ? "is-active" : ""} onClick={() => selectCategory(item)}>{item}</button>)}
      <span className="appstore-more-anchor" ref={moreRef}>
        <button className="appstore-categories__more" onClick={() => setMoreOpen((open) => !open)}>{MORE_CATEGORIES.includes(category) ? category : "更多"}<ChevronDown size={13} /></button>
        {moreOpen && <div className="appstore-more-menu" ref={moreMenuRef}>{MORE_CATEGORIES.map((item) => <button key={item} className={item === category ? "is-selected" : ""} onClick={() => selectCategory(item)}>{item}</button>)}</div>}
      </span>
    </div>}
    {tab === "catalog" && <>
      <div className="appstore-toolbar">
        <Button variant="secondary" className="button--plain-round" onClick={() => void catalog.refetch()} disabled={catalog.isFetching}><RefreshCw className={catalog.isFetching ? "spin" : ""} size={14} />更新远程应用</Button>
        <Button variant="secondary" className="button--plain-round" onClick={() => void installed.refetch()} disabled={installed.isFetching}><Download size={14} />同步本地应用</Button>
        <div className="appstore-toolbar__right">
          <label className="app-checkbox"><input type="checkbox" checked={archOnly} onChange={(event) => setArchOnly(event.target.checked)} /><span>本服务器架构应用</span></label>
          <label className="app-checkbox"><input type="checkbox" checked={localOnly} onChange={(event) => setLocalOnly(event.target.checked)} /><span>本地应用</span></label>
          <label className="appstore-search"><input value={query} onChange={(event) => { setQuery(event.target.value); setCatalogPage(1); }} placeholder="搜索" /><Search size={15} /></label>
        </div>
      </div>
      <div className="appstore-alert"><Info size={14} /><span>部分应用的安装使用说明请在应用详情页查看</span></div>
      {catalog.isLoading && <div className="page-state">正在读取所选应用商店来源目录…</div>}
      <div className="appstore-grid">{catalogSlice.map((item) => <CatalogCard key={item.key} item={item} installed={installedKeys.has(item.key)} busy={detail.isPending} onInstall={() => openDetail(item)} />)}</div>
      {!catalog.isLoading && !catalogSlice.length && <div className="empty-panel"><Package size={28} /><h2>没有匹配的应用</h2><p>尝试清除搜索条件或刷新官方目录。</p></div>}
      <Pager total={catalogItems.length} page={catalogPage} pageSize={catalogPageSize} onPageChange={setCatalogPage} onPageSizeChange={setCatalogPageSize} />
    </>}
    {tab === "installed" && <>
      <div className="appstore-toolbar">
        <Button variant="secondary" className="button--plain-round" onClick={() => void installed.refetch()} disabled={installed.isFetching}><RefreshCw className={installed.isFetching ? "spin" : ""} size={14} />刷新</Button>
        <span ref={sortedRef}><Button variant="secondary" className="button--plain-round" onClick={() => setSortOpen((open) => !open)}>排序<ChevronDown size={13} /></Button></span>
        {sortOpen && <div className="appstore-more-menu appstore-sort-menu" ref={sortMenuRef}>
          <button className={sortKey === "uptime" ? "is-selected" : ""} onClick={() => { setSortKey("uptime"); setSortOpen(false); }}>默认排序</button>
          <button className={sortKey === "name" ? "is-selected" : ""} onClick={() => { setSortKey("name"); setSortOpen(false); }}>名称</button>
          <button className={sortKey === "status" ? "is-selected" : ""} onClick={() => { setSortKey("status"); setSortOpen(false); }}>状态</button>
        </div>}
        <label className="appstore-search appstore-search--right"><input value={query} onChange={(event) => { setQuery(event.target.value); setInstalledPage(1); }} placeholder="搜索" /><Search size={15} /></label>
      </div>
      <div className="appstore-alert"><Info size={14} /><span>配置镜像加速可以解决镜像拉取失败的问题</span><button className="appstore-alert__link" onClick={() => openSettingsTab()}><Zap size={13} />快速跳转</button></div>
      {!installed.data ? null : !installed.data.composeAvailable ? <div className="empty-panel"><Package size={28} /><h2>未发现 Docker Compose</h2><p>请先在工具箱安装 Docker Compose，再使用应用商店。</p></div> : !installed.data.apps.length ? <div className="empty-panel"><Package size={28} /><h2>尚未安装应用</h2><p>从应用目录选择一个官方模板开始。</p></div> : <div className="installed-rows">
        {installedSlice.map((app) => <InstalledRow key={app.composePath} app={app} busy={action.isPending} running={app.status === "running"} onAction={runAction} onEnvironment={openEnvironment} onHealth={openHealth} onPreview={openPreview} onCopyPath={copyPath} onBackupInfo={setBackupInfoApp} />)}
      </div>}
      <Pager total={installedApps.length} page={installedPage} pageSize={installedPageSize} onPageChange={setInstalledPage} onPageSizeChange={setInstalledPageSize} />
    </>}
    {tab === "upgrades" && <>
      <div className="appstore-toolbar">
        <Button variant="secondary" className="button--plain-round" onClick={() => setIgnoredOpen(true)}>查看忽略应用</Button>
        <label className="appstore-search appstore-search--right"><input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="搜索" /><Search size={15} /></label>
      </div>
      {upgradeable.isFetching && <div className="page-state small-state">正在检查可升级应用…</div>}
      {!installed.data?.apps.length ? <div className="empty-panel empty-panel--small"><Package size={20} /><span>还没有已安装的应用，无法检查升级。</span></div> : !upgradeApps.length ? <div className="empty-panel empty-panel--small"><Package size={20} /><span>所有已安装应用都是最新版本。</span></div> : <div className="installed-rows">
        {upgradeApps.map((app) => <UpgradeRow key={app.composePath} app={app} busy={action.isPending} running={app.status === "running"} upgradable={Boolean(upgradeable.data?.some((item) => item.key === app.key))} deprecated={ignored.includes(app.key)} onIgnore={() => toggleIgnored(app.key)} onUpgrade={() => runAction(app, "update")} onCopyPath={copyPath} />)}
      </div>}
    </>}
    {tab === "settings" && <SettingsPanel prefs={prefs} onPrefsChange={updatePrefs} />}

    <Dialog.Root open={selected !== null} onOpenChange={(open) => !open && setSelected(null)}><Dialog.Portal><Dialog.Overlay className="dialog-overlay" /><Dialog.Content className="dialog-content"><div className="dialog-header"><div><Dialog.Title>{selected?.name ?? "应用详情"}</Dialog.Title><Dialog.Description>{selected?.description ?? ""}</Dialog.Description></div><Dialog.Close asChild><button className="icon-control" aria-label="关闭"><X size={17} /></button></Dialog.Close></div>{selected && <div className="app-install-form"><div className="app-detail-meta"><span className="mono">{selected.key}</span>{selected.tags.map((tag) => <span key={tag}>{tag}</span>)}{selected.website && <a href={selected.website} target="_blank" rel="noreferrer">官网 <ExternalLink size={12} /></a>}</div><label><span>版本</span><select value={version} onChange={(event) => setVersion(event.target.value)}><option value="" disabled>暂无可安装版本</option>{selected.versions.map((item) => <option key={item.version} value={item.version}>{item.version}</option>)}</select></label><label><span>Compose 项目名</span><input value={project} onChange={(event) => setProject(event.target.value)} /></label><label><span>安装目录</span><input value={installPath} onChange={(event) => setInstallPath(event.target.value)} /><small>默认写入 /opt/1panel/apps/&lt;key&gt;，不会覆盖其他目录。</small></label><label><span>环境变量（可选，每行 KEY=VALUE）</span><textarea rows={5} value={environment} onChange={(event) => setEnvironment(event.target.value)} placeholder="TZ=Asia/Shanghai" /></label>{install.error && <div className="form-error">{errorMessage(install.error)}</div>}<div className="security-note"><Package size={18} /><span>模板来自当前应用商店来源；安装前会执行 docker compose config -q，失败不会启动服务。</span></div><div className="dialog-actions"><Button variant="ghost" onClick={() => setSelected(null)}>取消</Button><Button variant="primary" onClick={() => install.mutate()} disabled={!version || install.isPending}>{install.isPending ? "安装中…" : "安装并启动"}</Button></div></div>}</Dialog.Content></Dialog.Portal></Dialog.Root>
    <Dialog.Root open={environmentApp !== null} onOpenChange={(open) => !open && setEnvironmentApp(null)}><Dialog.Portal><Dialog.Overlay className="dialog-overlay" /><Dialog.Content className="dialog-content"><div className="dialog-header"><div><Dialog.Title>应用环境变量</Dialog.Title><Dialog.Description>{environmentApp?.key ?? "应用"} · 远端 `.env` 键摘要与覆盖编辑</Dialog.Description></div><Dialog.Close asChild><button className="icon-control" aria-label="关闭"><X size={17} /></button></Dialog.Close></div><div className="app-install-form">{appEnvironment.isLoading && <div className="page-state">正在读取远端环境变量键…</div>}{appEnvironment.data && <div className="app-detail-meta">{appEnvironment.data.entries.length ? appEnvironment.data.entries.map((entry) => <span key={entry.key} className="mono">{entry.key}={entry.maskedValue}</span>) : <span>当前 `.env` 没有可识别的键。</span>}</div>}<label><span>覆盖或新增变量（每行 KEY=VALUE）</span><textarea rows={7} value={environmentOverrides} onChange={(event) => setEnvironmentOverrides(event.target.value)} placeholder="TZ=Asia/Shanghai" /><small>只覆盖你提交的键；未提交的远端值会保留。密码等秘密不会回传到客户端。</small></label>{saveEnvironment.error && <div className="form-error">{errorMessage(saveEnvironment.error)}</div>}<div className="security-note"><Settings2 size={18} /><span>保存会合并写入 {appEnvironment.data?.path ?? "远端 .env"}，不会自动重启 Compose 服务。</span></div><div className="dialog-actions"><Button variant="ghost" onClick={() => setEnvironmentApp(null)}>取消</Button><Button variant="primary" onClick={() => saveEnvironment.mutate()} disabled={!environmentOverrides.trim() || saveEnvironment.isPending}>{saveEnvironment.isPending ? "保存中…" : "保存环境变量"}</Button></div></div></Dialog.Content></Dialog.Portal></Dialog.Root>
    <Dialog.Root open={healthApp !== null} onOpenChange={(open) => !open && setHealthApp(null)}><Dialog.Portal><Dialog.Overlay className="dialog-overlay" /><Dialog.Content className="dialog-content"><div className="dialog-header"><div><Dialog.Title>应用健康检查</Dialog.Title><Dialog.Description>{healthApp?.key ?? "应用"} · 读取 Compose 服务状态和 healthcheck</Dialog.Description></div><Dialog.Close asChild><button className="icon-control" aria-label="关闭"><X size={17} /></button></Dialog.Close></div><div className="app-install-form">{appHealth.isLoading && <div className="page-state">正在读取容器健康状态…</div>}{appHealth.data && <><div className="app-detail-meta"><span>整体状态：{appHealth.data.overall}</span><span>{appHealth.data.services.length} 个服务</span><span className="mono">{appHealth.data.path}</span></div>{appHealth.data.services.length ? <div className="database-table"><div className="ops-head"><span>服务</span><span>镜像</span><span>状态</span><span>Health</span><span>退出码</span></div>{appHealth.data.services.map((service) => <div className="ops-row" key={service.name}><span className="mono">{service.name}</span><span className="mono">{service.image}</span><span>{service.state}</span><span className={service.health === "healthy" ? "text-ok" : service.health === "unhealthy" ? "text-danger" : "text-muted"}>{service.health}</span><span>{service.exitCode}</span></div>)}</div> : <div className="empty-panel empty-panel--small"><Package size={20} /><span>Compose 项目当前没有运行中的服务。</span></div>}</>}{appHealth.error && <div className="form-error">{errorMessage(appHealth.error)}</div>}<div className="security-note"><Settings2 size={18} /><span>只读取容器状态、镜像名和 healthcheck，不返回环境变量、挂载内容或日志正文。</span></div><div className="dialog-actions"><Button variant="secondary" onClick={() => appHealth.refetch()} disabled={appHealth.isFetching}>{appHealth.isFetching ? "刷新中…" : "刷新健康状态"}</Button><Button variant="ghost" onClick={() => setHealthApp(null)}>关闭</Button></div></div></Dialog.Content></Dialog.Portal></Dialog.Root>
    <Dialog.Root open={previewApp !== null} onOpenChange={(open) => { if (!open) { setPreviewApp(null); appPreview.reset(); } }}><Dialog.Portal><Dialog.Overlay className="dialog-overlay" /><Dialog.Content className="dialog-content dialog-content--narrow"><div className="dialog-header"><div><Dialog.Title>升级差异预览</Dialog.Title><Dialog.Description>{previewApp?.key ?? "应用"} · 对比当前来源最新 Compose 模板</Dialog.Description></div><Dialog.Close asChild><button className="icon-control" aria-label="关闭"><X size={17} /></button></Dialog.Close></div><div className="app-install-form">{appPreview.isPending && <div className="page-state">正在读取当前来源最新模板并在远端计算摘要…</div>}{appPreview.error && <div className="form-error">{errorMessage(appPreview.error)}</div>}{appPreview.data && <UpdatePreviewSummary preview={appPreview.data} />}{appPreview.data && <div className="security-note"><Settings2 size={18} /><span>此预览不会修改 Compose、镜像或容器；确认后仍需点击“更新”执行备份、校验、拉取和启动。</span></div>}<div className="dialog-actions"><Button variant="ghost" onClick={() => { setPreviewApp(null); appPreview.reset(); }}>关闭</Button></div></div></Dialog.Content></Dialog.Portal></Dialog.Root>
    <Dialog.Root open={backupInfoApp !== null} onOpenChange={(open) => !open && setBackupInfoApp(null)}><Dialog.Portal><Dialog.Overlay className="dialog-overlay" /><Dialog.Content className="dialog-content dialog-content--narrow"><div className="dialog-header"><div><Dialog.Title>应用备份</Dialog.Title><Dialog.Description>{backupInfoApp?.key ?? "应用"} · 备份说明</Dialog.Description></div><Dialog.Close asChild><button className="icon-control" aria-label="关闭"><X size={17} /></button></Dialog.Close></div><div className="app-install-form"><div className="security-note"><Package size={18} /><span>客户端当前不托管应用数据备份；建议在服务器上对 {backupInfoApp?.path ?? "应用目录"} 与关联数据卷执行快照。升级前会先备份应用（可在「设置-应用升级前备份应用」开关中调整偏好）。</span></div><div className="dialog-actions"><Button variant="ghost" onClick={() => setBackupInfoApp(null)}>关闭</Button></div></div></Dialog.Content></Dialog.Portal></Dialog.Root>
    <Dialog.Root open={ignoredOpen} onOpenChange={setIgnoredOpen}><Dialog.Portal><Dialog.Overlay className="dialog-overlay" /><Dialog.Content className="dialog-content dialog-content--narrow"><div className="dialog-header"><div><Dialog.Title>忽略升级的应用</Dialog.Title><Dialog.Description>这些应用不会出现在可升级列表中</Dialog.Description></div><Dialog.Close asChild><button className="icon-control" aria-label="关闭"><X size={17} /></button></Dialog.Close></div><div className="app-install-form">{!ignored.length ? <div className="empty-panel empty-panel--small"><Package size={20} /><span>没有忽略升级的应用。</span></div> : <div className="installed-actions">{ignored.map((key) => <span className="mono" key={key}>{key}<Button size="sm" variant="ghost" onClick={() => toggleIgnored(key)}>取消忽略</Button></span>)}</div>}<div className="dialog-actions"><Button variant="ghost" onClick={() => setIgnoredOpen(false)}>关闭</Button></div></div></Dialog.Content></Dialog.Portal></Dialog.Root>
  </section>;
}

/** 目录卡片：图标、名称、已安装角标、描述、分类与安装按钮。 */
function CatalogCard({ item, installed, busy, onInstall }: { item: AppCatalogItem; installed: boolean; busy: boolean; onInstall: () => void }) {
  return <article className="app-card" onClick={onInstall} role="button" tabIndex={0} onKeyDown={(event) => (event.key === "Enter" || event.key === " ") && onInstall()}>
    <div className="app-card__icon" style={{ background: iconColor(item.key), color: "#fff" }}>{(item.name[0] ?? item.key[0] ?? "?").toUpperCase()}</div>
    <div className="app-card__content">
      <div className="app-card__title"><strong>{item.name}</strong>{installed && <span className="app-tag app-tag--success">已安装</span>}</div>
      <p className="app-card__desc">{item.description}</p>
      <div className="app-card__bottom"><span className="app-card__cat">{item.category}</span><Button size="sm" variant="secondary" className="button--plain-round" disabled={installed || busy} onClick={(event) => { event.stopPropagation(); onInstall(); }}>安装</Button></div>
    </div>
  </article>;
}

/** 已安装行卡：状态、操作按钮与版本/端口/安装时长信息。 */
function InstalledRow({ app, busy, running, onAction, onEnvironment, onHealth, onPreview, onCopyPath, onBackupInfo }: { app: InstalledApp; busy: boolean; running: boolean; onAction: (app: InstalledApp, action: "start" | "stop" | "restart" | "pull" | "update" | "uninstall" | "restore") => void; onEnvironment: (app: InstalledApp) => void; onHealth: (app: InstalledApp) => void; onPreview: (app: InstalledApp) => void; onCopyPath: (app: InstalledApp) => void; onBackupInfo: (app: InstalledApp) => void }) {
  const [moreOpen, setMoreOpen] = useState(false);
  return <article className="installed-app-card">
    <div className="installed-app-card__icon" style={{ background: iconColor(app.key), color: "#fff" }}>{(app.key[0] ?? "?").toUpperCase()}</div>
    <div className="installed-app-card__body">
      <div className="installed-app-card__title">
        <strong>{app.key}</strong>
        <span className={running ? "app-tag app-tag--success" : "app-tag"}>{running ? "已启动" : app.status === "stopped" ? "已停止" : app.status || "状态未知"}</span>
        <span className="installed-app-card__tools">
          <button className="installed-app-card__iconbtn" title="打开目录（复制路径）" onClick={() => onCopyPath(app)}><FolderOpen size={15} /></button>
          <button className="installed-app-card__iconbtn" title="升级预览" onClick={() => onPreview(app)}><Eye size={15} /></button>
          <button className="installed-app-card__iconbtn" title="更多" onClick={() => setMoreOpen((open) => !open)}><MoreHorizontal size={15} /></button>
          {moreOpen && <div className="appstore-more-menu installed-app-card__more"><button onClick={() => { setMoreOpen(false); onHealth(app); }}><Play size={13} />健康检查</button><button onClick={() => { setMoreOpen(false); onEnvironment(app); }}><Settings2 size={13} />环境变量</button><button onClick={() => { setMoreOpen(false); onAction(app, "update"); }}><RefreshCw size={13} />更新应用</button></div>}
        </span>
        <span className="installed-app-card__side">
          <Button size="sm" variant="secondary" className="button--plain-round" onClick={() => onBackupInfo(app)} disabled={busy}>导入备份</Button>
          <Button size="sm" variant="secondary" className="button--plain-round" onClick={() => onBackupInfo(app)} disabled={busy}>备份</Button>
        </span>
      </div>
      <div className="installed-app-card__meta">
        {app.version && <span className="app-tag app-tag--meta">版本：{app.version}</span>}
        {(app.hostPorts ?? []).map((port) => <span className="app-tag app-tag--meta" key={port}>端口：{port}</span>)}
        {formatUptime(app.installedSeconds) && <span className="installed-app-card__uptime">已安装： {formatUptime(app.installedSeconds)}</span>}
      </div>
      <div className="installed-app-card__actions">
        <Button size="sm" variant="secondary" className="button--plain-round" disabled={busy} onClick={() => onAction(app, "restore")}>重建</Button>
        <Button size="sm" variant="secondary" className="button--plain-round" disabled={busy} onClick={() => onAction(app, "restart")}>重启</Button>
        <Button size="sm" variant="secondary" className="button--plain-round" disabled={busy || running} onClick={() => onAction(app, "start")}>启动</Button>
        <Button size="sm" variant="secondary" className="button--plain-round" disabled={busy} onClick={() => onAction(app, "stop")}>停止</Button>
        <Button size="sm" variant="secondary" className="button--plain-round" disabled={busy} onClick={() => onAction(app, "uninstall")}>卸载</Button>
        <Button size="sm" variant="secondary" className="button--plain-round" disabled={busy} onClick={() => onEnvironment(app)}>参数</Button>
      </div>
    </div>
  </article>;
}

/** 可升级行卡：忽略升级/升级按钮与版本信息。 */
function UpgradeRow({ app, busy, running, upgradable, deprecated, onIgnore, onUpgrade, onCopyPath }: { app: InstalledApp; busy: boolean; running: boolean; upgradable: boolean; deprecated: boolean; onIgnore: () => void; onUpgrade: () => void; onCopyPath: (app: InstalledApp) => void }) {
  return <article className="installed-app-card">
    <div className="installed-app-card__icon" style={{ background: iconColor(app.key), color: "#fff" }}>{(app.key[0] ?? "?").toUpperCase()}</div>
    <div className="installed-app-card__body">
      <div className="installed-app-card__title">
        <strong>{app.key}</strong>
        <span className={running ? "app-tag app-tag--success" : "app-tag"}>{running ? "已启动" : app.status === "stopped" ? "已停止" : app.status || "状态未知"}</span>
        <span className="installed-app-card__tools"><button className="installed-app-card__iconbtn" title="打开目录（复制路径）" onClick={() => onCopyPath(app)}><FolderOpen size={15} /></button></span>
        <span className="installed-app-card__side">
          <Button size="sm" variant="secondary" className="button--plain-round" onClick={onIgnore} disabled={busy}>忽略升级</Button>
          <Button size="sm" variant="secondary" className="button--plain-round" onClick={onUpgrade} disabled={busy || !upgradable}>{deprecated ? "重新加入升级" : "升级"}</Button>
        </span>
      </div>
      <div className="installed-app-card__meta">
        {app.version && <span className="app-tag app-tag--meta">版本：{app.version}</span>}
        {(app.hostPorts ?? []).map((port) => <span className="app-tag app-tag--meta" key={port}>端口：{port}</span>)}
        {formatUptime(app.installedSeconds) && <span className="installed-app-card__uptime">已安装： {formatUptime(app.installedSeconds)}</span>}
      </div>
    </div>
  </article>;
}

/** 设置页：与 Web 端 1:1 的行为开关列表与商业版提示。 */
function SettingsPanel({ prefs, onPrefsChange }: { prefs: AppBehaviorPreferences; onPrefsChange: (value: AppBehaviorPreferences) => void }) {
  const rows: { key: keyof AppBehaviorPreferences; label: string }[] = [
    { key: "uninstallDeleteBackup", label: "卸载应用-删除备份" },
    { key: "uninstallDeleteImage", label: "卸载应用-删除镜像" },
    { key: "backupBeforeUpgrade", label: "应用升级前备份应用" },
    { key: "upgradeDeleteOldImage", label: "升级应用-删除旧镜像" },
    { key: "defaultOpenPort", label: "安装应用默认打开端口外部访问" },
  ];
  return <div className="appstore-settings">
    <div className="settings-switches">
      {rows.map((row) => <label className="settings-switch-row" key={row.key}><span>{row.label}</span><input type="checkbox" checked={prefs[row.key]} onChange={(event) => onPrefsChange({ ...prefs, [row.key]: event.target.checked })} /><span className="settings-switch-track" aria-hidden="true"><span /></span></label>)}
      <div className="settings-commercial"><span>商业版支持自定义应用仓库功能</span><button className="appstore-alert__link" onClick={() => window.alert("概览页可查看商业版说明；客户端已提供自定义目录来源能力。")}>升级商业版</button></div>
    </div>
  </div>;
}

/** 根据搜索词和分类筛选应用清单。 */
function filterCatalog(items: AppCatalogItem[], query: string, category: string) {
  const needle = query.trim().toLocaleLowerCase();
  return items.filter((item) => (category === "全部" || item.category === category) && (!needle || `${item.key} ${item.name} ${item.description}`.toLocaleLowerCase().includes(needle)));
}

/** 根据搜索词和分类筛选已安装/可升级应用。 */
function filterInstalled(apps: InstalledApp[], query: string, category: string, categoryByKey: Map<string, string>) {
  const needle = query.trim().toLocaleLowerCase();
  return apps.filter((app) => (category === "全部" || categoryByKey.get(app.key) === category) && (!needle || `${app.key} ${app.project} ${app.path}`.toLocaleLowerCase().includes(needle)));
}

/** 以不泄露 Compose 正文的方式展示官方升级差异摘要。 */
function UpdatePreviewSummary({ preview }: { preview: AppUpdatePreview }) {
  return <div className="app-detail-meta"><span className={preview.changed ? "text-danger" : "text-ok"}>{preview.currentMissing ? "当前配置缺失" : preview.changed ? "检测到配置变化" : "配置无变化"}</span><span>最新版本：{preview.latestVersion}</span><span>当前行数：{preview.currentLines}</span><span>最新行数：{preview.latestLines}</span><span className="mono">当前：{preview.currentHash ?? "不存在"}</span><span className="mono">最新：{preview.latestHash}</span></div>;
}

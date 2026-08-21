import * as Dialog from "@radix-ui/react-dialog";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Boxes, Download, ExternalLink, Package, Play, RefreshCw, Search, Settings2, ShieldCheck, Square, Trash2, X } from "lucide-react";
import { useMemo, useState } from "react";
import { NavLink, useParams } from "react-router-dom";
import { Button } from "../../components/ui/Button";
import { api } from "../../lib/api";
import { errorMessage } from "../../lib/errors";
import type { AppCatalogItem, AppDetail, AppStoreSettings, AppUpdatePreview, InstalledApp } from "../../types/server";

type AppTab = "catalog" | "installed";

/** 展示当前应用商店来源目录，并把 Compose 安装和生命周期交给远端 SSH 执行。 */
export function AppStorePage() {
  const { serverId = "" } = useParams();
  const queryClient = useQueryClient();
  const [tab, setTab] = useState<AppTab>("catalog");
  const [query, setQuery] = useState("");
  const [category, setCategory] = useState("全部");
  const [selected, setSelected] = useState<AppDetail | null>(null);
  const [version, setVersion] = useState("");
  const [project, setProject] = useState("");
  const [installPath, setInstallPath] = useState("");
  const [environment, setEnvironment] = useState("");
  const [environmentApp, setEnvironmentApp] = useState<InstalledApp | null>(null);
  const [environmentOverrides, setEnvironmentOverrides] = useState("");
  const [healthApp, setHealthApp] = useState<InstalledApp | null>(null);
  const [previewApp, setPreviewApp] = useState<InstalledApp | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [settingsDraft, setSettingsDraft] = useState<AppStoreSettings>({ source: "official", mirrorBaseUrl: null, mirrorBaseUrls: [], cacheTtlSeconds: 3600, offlineMode: false, mirrorKeyId: null, signatureConfigured: false });
  const [mirrorUrlsText, setMirrorUrlsText] = useState("");
  const [mirrorGeneratorOpen, setMirrorGeneratorOpen] = useState(false);
  const catalog = useQuery({ queryKey: ["app-catalog"], queryFn: api.appCatalog, staleTime: 10 * 60_000 });
  const appStoreSettings = useQuery({ queryKey: ["appstore-settings"], queryFn: api.appStoreSettings, staleTime: Infinity });
  const installed = useQuery({ queryKey: ["installed-apps", serverId], queryFn: () => api.installedApps(serverId), enabled: Boolean(serverId), refetchInterval: 15_000 });
  const appEnvironment = useQuery({ queryKey: ["app-environment", serverId, environmentApp?.path], queryFn: () => api.appEnvironment(serverId, environmentApp!.path), enabled: Boolean(environmentApp) });
  const appHealth = useQuery({ queryKey: ["app-health", serverId, healthApp?.path], queryFn: () => api.appHealth({ serverId, project: healthApp!.project, installPath: healthApp!.path }), enabled: Boolean(healthApp), staleTime: 5_000 });
  const appPreview = useMutation({ mutationFn: api.appUpdatePreview, onSuccess: () => undefined });
  const saveSettings = useMutation({ mutationFn: (value: AppStoreSettings) => api.saveAppStoreSettings({ ...value, mirrorBaseUrls: mirrorUrlsText.split(/\r?\n/).map((item) => item.trim()).filter(Boolean), mirrorBaseUrl: mirrorUrlsText.split(/\r?\n/).map((item) => item.trim()).filter(Boolean)[0] ?? null }), onSuccess: async (value) => { setSettingsDraft(value); setMirrorUrlsText(value.mirrorBaseUrls.join("\n")); setSettingsOpen(false); await queryClient.invalidateQueries({ queryKey: ["app-catalog"] }); } });
  const clearCache = useMutation({ mutationFn: api.clearAppStoreCache, onSuccess: async () => { setSelected(null); await queryClient.invalidateQueries({ queryKey: ["app-catalog"] }); } });
  const detail = useMutation({ mutationFn: api.appDetail, onSuccess: (value) => { setSelected(value); setVersion(value.versions[0]?.version ?? ""); setProject(value.key); setInstallPath(`/opt/1panel/apps/${value.key}`); setEnvironment(""); } });
  const install = useMutation({ mutationFn: () => api.installApp({ serverId, key: selected!.key, version, project, installPath, environment: environment.split("\n").map((line) => line.trim()).filter(Boolean), confirmed: true }), onSuccess: async () => { setSelected(null); setTab("installed"); await queryClient.invalidateQueries({ queryKey: ["installed-apps", serverId] }); } });
  const action = useMutation({ mutationFn: (input: { app: InstalledApp; action: "start" | "stop" | "restart" | "pull" | "update" | "uninstall" | "restore" }) => api.appAction({ serverId, key: input.app.key, project: input.app.project, installPath: input.app.path, action: input.action, confirmed: true }), onSuccess: async () => { await queryClient.invalidateQueries({ queryKey: ["installed-apps", serverId] }); } });
  const saveEnvironment = useMutation({ mutationFn: () => api.saveAppEnvironment({ serverId, installPath: environmentApp!.path, values: environmentOverrides.split("\n").map((line) => line.trim()).filter(Boolean), confirmed: true }), onSuccess: async () => { setEnvironmentApp(null); setEnvironmentOverrides(""); await queryClient.invalidateQueries({ queryKey: ["app-environment", serverId] }); } });
  const categories = useMemo(() => ["全部", ...new Set((catalog.data?.items ?? []).map((item) => item.category))], [catalog.data]);
  const items = useMemo(() => filterCatalog(catalog.data?.items ?? [], query, category), [catalog.data, category, query]);
  const activeSettings = appStoreSettings.data ?? settingsDraft;

  /** 打开来源设置并用最近一次持久化值初始化表单。 */
  const openSettings = () => { if (appStoreSettings.data) { setSettingsDraft(appStoreSettings.data); setMirrorUrlsText(appStoreSettings.data.mirrorBaseUrls.join("\n") || appStoreSettings.data.mirrorBaseUrl || ""); } setSettingsOpen(true); };
  /** 打开静态镜像生成器；生成器会在本地写目录并可保存验签令牌。 */
  const openMirrorGenerator = () => setMirrorGeneratorOpen(true);
  /** 同步镜像生成器保存的验签配置到当前页面，避免等待下一次查询刷新。 */
  const handleMirrorSettingsSaved = (value: AppStoreSettings) => { setSettingsDraft(value); setMirrorUrlsText(value.mirrorBaseUrls.join("\n") || value.mirrorBaseUrl || ""); void queryClient.invalidateQueries({ queryKey: ["appstore-settings"] }); };
  /** 打开应用详情并延迟读取所选来源 metadata，避免目录页为每个应用发起网络请求。 */
  const openDetail = (item: AppCatalogItem) => detail.mutate(item.key);
  /** 让破坏性卸载操作必须经过用户确认。 */
  const runAction = (app: InstalledApp, actionName: "start" | "stop" | "restart" | "pull" | "update" | "uninstall" | "restore") => {
    if (["stop", "uninstall"].includes(actionName) && !window.confirm(`确认对 ${app.key} 执行 ${actionName}？`)) return;
    action.mutate({ app, action: actionName });
  };
  /** 打开环境变量摘要和安全的覆盖编辑器，远端原有秘密不会回传。 */
  const openEnvironment = (app: InstalledApp) => { setEnvironmentOverrides(""); setEnvironmentApp(app); };
  /** 打开应用健康检查，只读取容器状态和 healthcheck 摘要。 */
  const openHealth = (app: InstalledApp) => setHealthApp(app);
  /** 打开所选来源的升级差异预览，只读取当前 Compose 的摘要哈希和行数。 */
  const openPreview = (app: InstalledApp) => { setPreviewApp(app); appPreview.mutate({ serverId, key: app.key, project: app.project, installPath: app.path }); };

  return <section className="appstore-page">
    <div className="workspace-header"><div><div className="breadcrumb">服务器 / <span>应用商店</span></div><h1>应用商店</h1><p>{catalog.data?.cached ? "本地缓存目录" : activeSettings.source === "mirror" ? "静态镜像应用模板" : "官方 1Panel 应用模板"}，按当前服务器通过 Docker Compose 安装</p></div><div className="workspace-header__actions"><Button variant="secondary" onClick={openSettings}><Settings2 size={14} />来源设置</Button><Button variant="secondary" onClick={() => { void catalog.refetch(); void installed.refetch(); }} disabled={catalog.isFetching || installed.isFetching}><RefreshCw className={catalog.isFetching || installed.isFetching ? "spin" : ""} size={14} />刷新目录</Button></div></div>
    <AppTabs serverId={serverId} tab={tab} onChange={setTab} installedCount={installed.data?.apps.length ?? 0} />
    <div className="workspace-header__actions appstore-tools"><Button variant="secondary" onClick={openMirrorGenerator}><Download size={14} />生成静态镜像</Button></div>
    {appStoreSettings.error && <div className="page-state page-state--error">{errorMessage(appStoreSettings.error)}</div>}
    {catalog.error && <div className="page-state page-state--error">{errorMessage(catalog.error)}</div>}
    {installed.error && <div className="page-state page-state--error">{errorMessage(installed.error)}</div>}
    {appEnvironment.error && <div className="page-state page-state--error">{errorMessage(appEnvironment.error)}</div>}
    {appHealth.error && <div className="page-state page-state--error">{errorMessage(appHealth.error)}</div>}
    {tab === "catalog" && <>
      <div className="appstore-toolbar"><label className="appstore-search"><Search size={15} /><input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="搜索应用名称或 key" /></label><div className="appstore-categories">{categories.map((item) => <button className={category === item ? "is-active" : ""} key={item} onClick={() => setCategory(item)}>{item}</button>)}</div><span className="appstore-count">{items.length} / {catalog.data?.items.length ?? 0} 个应用{catalog.data?.sourceRevision ? ` · ${activeSettings.source === "mirror" ? "镜像" : "官方"} ${catalog.data.sourceRevision.slice(0, 8)}` : ""}{catalog.data?.cached ? ` · 缓存 ${catalog.data.cacheAgeSeconds ?? 0}s` : ""}{activeSettings.source === "mirror" && catalog.data ? ` · ${catalog.data.signatureVerified ? "签名已验证" : catalog.data.signaturePresent ? "签名未验证" : "无签名"}` : ""}</span></div>
      {catalog.isLoading && <div className="page-state">正在读取所选应用商店来源目录…</div>}
      <div className="appstore-grid">{items.map((item) => <article className="app-card" key={item.key}><div className="app-card__icon"><Boxes size={22} /></div><div className="app-card__body"><div className="app-card__title"><strong>{item.name}</strong><span>{item.category}</span></div><p>{item.description}</p><small className="mono">{item.key}</small></div><Button size="sm" variant="primary" onClick={() => openDetail(item)} disabled={detail.isPending}>查看详情</Button></article>)}</div>
      {!catalog.isLoading && !items.length && <div className="empty-panel"><Package size={28} /><h2>没有匹配的应用</h2><p>尝试清除搜索条件或刷新官方目录。</p></div>}
    </>}
    {tab === "installed" && <InstalledApps apps={installed.data?.apps ?? []} composeAvailable={installed.data?.composeAvailable ?? false} busy={action.isPending || appPreview.isPending} onAction={runAction} onEnvironment={openEnvironment} onHealth={openHealth} onPreview={openPreview} />}
    <Dialog.Root open={selected !== null} onOpenChange={(open) => !open && setSelected(null)}><Dialog.Portal><Dialog.Overlay className="dialog-overlay" /><Dialog.Content className="dialog-content"><div className="dialog-header"><div><Dialog.Title>{selected?.name ?? "应用详情"}</Dialog.Title><Dialog.Description>{selected?.description ?? ""}</Dialog.Description></div><Dialog.Close asChild><button className="icon-control" aria-label="关闭"><X size={17} /></button></Dialog.Close></div>{selected && <div className="app-install-form"><div className="app-detail-meta"><span className="mono">{selected.key}</span>{selected.tags.map((tag) => <span key={tag}>{tag}</span>)}{selected.website && <a href={selected.website} target="_blank" rel="noreferrer">官网 <ExternalLink size={12} /></a>}</div><label><span>版本</span><select value={version} onChange={(event) => setVersion(event.target.value)}><option value="" disabled>暂无可安装版本</option>{selected.versions.map((item) => <option key={item.version} value={item.version}>{item.version}</option>)}</select></label><label><span>Compose 项目名</span><input value={project} onChange={(event) => setProject(event.target.value)} /></label><label><span>安装目录</span><input value={installPath} onChange={(event) => setInstallPath(event.target.value)} /><small>默认写入 /opt/1panel/apps/&lt;key&gt;，不会覆盖其他目录。</small></label><label><span>环境变量（可选，每行 KEY=VALUE）</span><textarea rows={5} value={environment} onChange={(event) => setEnvironment(event.target.value)} placeholder="TZ=Asia/Shanghai" /></label>{install.error && <div className="form-error">{errorMessage(install.error)}</div>}<div className="security-note"><Package size={18} /><span>模板来自当前应用商店来源；安装前会执行 docker compose config -q，失败不会启动服务。</span></div><div className="dialog-actions"><Button variant="ghost" onClick={() => setSelected(null)}>取消</Button><Button variant="primary" onClick={() => install.mutate()} disabled={!version || install.isPending}>{install.isPending ? "安装中…" : "安装并启动"}</Button></div></div>}</Dialog.Content></Dialog.Portal></Dialog.Root>
    <Dialog.Root open={environmentApp !== null} onOpenChange={(open) => !open && setEnvironmentApp(null)}><Dialog.Portal><Dialog.Overlay className="dialog-overlay" /><Dialog.Content className="dialog-content"><div className="dialog-header"><div><Dialog.Title>应用环境变量</Dialog.Title><Dialog.Description>{environmentApp?.key ?? "应用"} · 远端 `.env` 键摘要与覆盖编辑</Dialog.Description></div><Dialog.Close asChild><button className="icon-control" aria-label="关闭"><X size={17} /></button></Dialog.Close></div><div className="app-install-form">{appEnvironment.isLoading && <div className="page-state">正在读取远端环境变量键…</div>}{appEnvironment.data && <div className="app-detail-meta">{appEnvironment.data.entries.length ? appEnvironment.data.entries.map((entry) => <span key={entry.key} className="mono">{entry.key}={entry.maskedValue}</span>) : <span>当前 `.env` 没有可识别的键。</span>}</div>}<label><span>覆盖或新增变量（每行 KEY=VALUE）</span><textarea rows={7} value={environmentOverrides} onChange={(event) => setEnvironmentOverrides(event.target.value)} placeholder="TZ=Asia/Shanghai" /><small>只覆盖你提交的键；未提交的远端值会保留。密码等秘密不会回传到客户端。</small></label>{saveEnvironment.error && <div className="form-error">{errorMessage(saveEnvironment.error)}</div>}<div className="security-note"><Settings2 size={18} /><span>保存会合并写入 {appEnvironment.data?.path ?? "远端 .env"}，不会自动重启 Compose 服务。</span></div><div className="dialog-actions"><Button variant="ghost" onClick={() => setEnvironmentApp(null)}>取消</Button><Button variant="primary" onClick={() => saveEnvironment.mutate()} disabled={!environmentOverrides.trim() || saveEnvironment.isPending}>{saveEnvironment.isPending ? "保存中…" : "保存环境变量"}</Button></div></div></Dialog.Content></Dialog.Portal></Dialog.Root>
    <Dialog.Root open={healthApp !== null} onOpenChange={(open) => !open && setHealthApp(null)}><Dialog.Portal><Dialog.Overlay className="dialog-overlay" /><Dialog.Content className="dialog-content"><div className="dialog-header"><div><Dialog.Title>应用健康检查</Dialog.Title><Dialog.Description>{healthApp?.key ?? "应用"} · 读取 Compose 服务状态和 healthcheck</Dialog.Description></div><Dialog.Close asChild><button className="icon-control" aria-label="关闭"><X size={17} /></button></Dialog.Close></div><div className="app-install-form">{appHealth.isLoading && <div className="page-state">正在读取容器健康状态…</div>}{appHealth.data && <><div className="app-detail-meta"><span>整体状态：{appHealth.data.overall}</span><span>{appHealth.data.services.length} 个服务</span><span className="mono">{appHealth.data.path}</span></div>{appHealth.data.services.length ? <div className="database-table"><div className="ops-head"><span>服务</span><span>镜像</span><span>状态</span><span>Health</span><span>退出码</span></div>{appHealth.data.services.map((service) => <div className="ops-row" key={service.name}><span className="mono">{service.name}</span><span className="mono">{service.image}</span><span>{service.state}</span><span className={service.health === "healthy" ? "text-ok" : service.health === "unhealthy" ? "text-danger" : "text-muted"}>{service.health}</span><span>{service.exitCode}</span></div>)}</div> : <div className="empty-panel empty-panel--small"><Package size={20} /><span>Compose 项目当前没有运行中的服务。</span></div>}</>}{appHealth.error && <div className="form-error">{errorMessage(appHealth.error)}</div>}<div className="security-note"><Settings2 size={18} /><span>只读取容器状态、镜像名和 healthcheck，不返回环境变量、挂载内容或日志正文。</span></div><div className="dialog-actions"><Button variant="secondary" onClick={() => appHealth.refetch()} disabled={appHealth.isFetching}>{appHealth.isFetching ? "刷新中…" : "刷新健康状态"}</Button><Button variant="ghost" onClick={() => setHealthApp(null)}>关闭</Button></div></div></Dialog.Content></Dialog.Portal></Dialog.Root>
    <Dialog.Root open={previewApp !== null} onOpenChange={(open) => { if (!open) { setPreviewApp(null); appPreview.reset(); } }}><Dialog.Portal><Dialog.Overlay className="dialog-overlay" /><Dialog.Content className="dialog-content dialog-content--narrow"><div className="dialog-header"><div><Dialog.Title>升级差异预览</Dialog.Title><Dialog.Description>{previewApp?.key ?? "应用"} · 对比当前来源最新 Compose 模板</Dialog.Description></div><Dialog.Close asChild><button className="icon-control" aria-label="关闭"><X size={17} /></button></Dialog.Close></div><div className="app-install-form">{appPreview.isPending && <div className="page-state">正在读取当前来源最新模板并在远端计算摘要…</div>}{appPreview.error && <div className="form-error">{errorMessage(appPreview.error)}</div>}{appPreview.data && <UpdatePreviewSummary preview={appPreview.data} />}{appPreview.data && <div className="security-note"><Settings2 size={18} /><span>此预览不会修改 Compose、镜像或容器；确认后仍需点击“更新”执行备份、校验、拉取和启动。</span></div>}<div className="dialog-actions"><Button variant="ghost" onClick={() => { setPreviewApp(null); appPreview.reset(); }}>关闭</Button></div></div></Dialog.Content></Dialog.Portal></Dialog.Root>
    <Dialog.Root open={settingsOpen} onOpenChange={setSettingsOpen}><Dialog.Portal><Dialog.Overlay className="dialog-overlay" /><Dialog.Content className="dialog-content"><div className="dialog-header"><div><Dialog.Title>应用商店来源</Dialog.Title><Dialog.Description>选择官方仓库或符合静态目录契约的镜像，并控制本地缓存。</Dialog.Description></div><Dialog.Close asChild><button className="icon-control" aria-label="关闭"><X size={17} /></button></Dialog.Close></div><div className="app-install-form"><label><span>目录来源</span><select value={settingsDraft.source} onChange={(event) => setSettingsDraft((current) => ({ ...current, source: event.target.value as AppStoreSettings["source"] }))}><option value="official">官方 GitHub</option><option value="mirror">静态镜像</option></select></label>{settingsDraft.source === "mirror" && <label><span>镜像节点列表（每行一个，第一项为主节点）</span><textarea rows={4} value={mirrorUrlsText} onChange={(event) => setMirrorUrlsText(event.target.value)} placeholder="https://mirror-a.example.com/1panel\nhttps://mirror-b.example.com/1panel" /><small>最多 8 个节点；客户端按顺序故障转移。必须提供 catalog.json、apps/&lt;key&gt;/data.yml、versions.json 和 Compose 文件；HTTP 仅允许本机调试地址。</small></label>}<label><span>缓存有效期（秒）</span><input type="number" min={300} max={86400} value={settingsDraft.cacheTtlSeconds} onChange={(event) => setSettingsDraft((current) => ({ ...current, cacheTtlSeconds: Number(event.target.value) || 300 }))} /></label><label className="checkbox-row"><input type="checkbox" checked={settingsDraft.offlineMode} onChange={(event) => setSettingsDraft((current) => ({ ...current, offlineMode: event.target.checked }))} /><span>离线模式（只使用已有缓存）</span></label>{saveSettings.error && <div className="form-error">{errorMessage(saveSettings.error)}</div>}{clearCache.error && <div className="form-error">{errorMessage(clearCache.error)}</div>}<div className="dialog-actions"><Button variant="ghost" onClick={() => clearCache.mutate()} disabled={clearCache.isPending}>{clearCache.isPending ? "清理中…" : "清理缓存"}</Button><Button variant="ghost" onClick={() => setSettingsOpen(false)}>取消</Button><Button variant="primary" onClick={() => saveSettings.mutate(settingsDraft)} disabled={saveSettings.isPending || appStoreSettings.isLoading}>{saveSettings.isPending ? "保存中…" : "保存设置"}</Button></div></div></Dialog.Content></Dialog.Portal></Dialog.Root>
    <MirrorGeneratorDialog open={mirrorGeneratorOpen} onOpenChange={setMirrorGeneratorOpen} settings={activeSettings} onSettingsSaved={handleMirrorSettingsSaved} />
  </section>;
}

/** 提供静态应用商店镜像生成、HMAC 验签配置和结果摘要，不在结果中显示令牌。 */
function MirrorGeneratorDialog({ open, onOpenChange, settings, onSettingsSaved }: { open: boolean; onOpenChange: (open: boolean) => void; settings: AppStoreSettings; onSettingsSaved: (value: AppStoreSettings) => void }) {
  const [destination, setDestination] = useState("");
  const [keyId, setKeyId] = useState(settings.mirrorKeyId ?? "mirror-main");
  const [signingSecret, setSigningSecret] = useState("");
  const [maxApps, setMaxApps] = useState(512);
  const [rememberVerification, setRememberVerification] = useState(true);
  const [confirmed, setConfirmed] = useState(false);
  const generation = useMutation({
    mutationFn: async () => {
      if (rememberVerification) {
        const saved = await api.saveAppStoreSettings({ ...settings, mirrorKeyId: keyId.trim() || null, mirrorVerificationSecret: signingSecret, clearMirrorVerificationSecret: false });
        onSettingsSaved(saved);
      }
      return api.generateAppStoreMirror({ destination, keyId, signingSecret, maxApps, confirmed });
    },
  });
  /** 只在表单通过确认勾选时启动本地镜像生成。 */
  const submit = () => { generation.mutate(); };
  return <Dialog.Root open={open} onOpenChange={onOpenChange}><Dialog.Portal><Dialog.Overlay className="dialog-overlay" /><Dialog.Content className="dialog-content"><div className="dialog-header"><div><Dialog.Title>生成静态应用镜像</Dialog.Title><Dialog.Description>从当前应用商店来源下载目录、metadata、版本 Compose 和环境模板，写入本机静态目录。</Dialog.Description></div><Dialog.Close asChild><button className="icon-control" aria-label="关闭"><X size={17} /></button></Dialog.Close></div><div className="app-install-form"><label><span>输出目录（绝对路径）</span><input value={destination} onChange={(event) => setDestination(event.target.value)} placeholder="C:\\1panel-mirror 或 /srv/1panel-mirror" /></label><label><span>验签 key ID</span><input value={keyId} onChange={(event) => setKeyId(event.target.value)} placeholder="mirror-main" /></label><label><span>HMAC 验签令牌</span><input type="password" value={signingSecret} onChange={(event) => setSigningSecret(event.target.value)} autoComplete="new-password" placeholder="至少 16 个字符" /><small>只发送给本地 Rust 后端；不会写入普通配置文件。生成后可保存到操作系统密钥链。</small></label><label><span>最多生成应用数</span><input type="number" min={1} max={512} value={maxApps} onChange={(event) => setMaxApps(Math.max(1, Math.min(512, Number(event.target.value) || 1)))} /></label><label className="checkbox-row"><input type="checkbox" checked={rememberVerification} onChange={(event) => setRememberVerification(event.target.checked)} /><span>将令牌保存为当前客户端的镜像验签配置</span></label><label className="checkbox-row"><input type="checkbox" checked={confirmed} onChange={(event) => setConfirmed(event.target.checked)} /><span>我确认允许在本机创建或覆盖镜像目录文件</span></label>{generation.error && <div className="form-error">{errorMessage(generation.error)}</div>}{generation.data && <div className="security-note"><ShieldCheck size={18} /><span>已生成 {generation.data.appCount} 个应用、{generation.data.versionCount} 个版本、{generation.data.fileCount} 个文件；目录摘要 {generation.data.catalogSha256.slice(0, 16)}…</span></div>}<div className="dialog-actions"><Button variant="ghost" onClick={() => onOpenChange(false)}>关闭</Button><Button variant="primary" onClick={submit} disabled={!destination.trim() || !keyId.trim() || !signingSecret.trim() || !confirmed || generation.isPending}>{generation.isPending ? "生成中…" : "生成并签名"}</Button></div></div></Dialog.Content></Dialog.Portal></Dialog.Root>;
}

/** 根据搜索词和官方分类筛选应用清单。 */
function filterCatalog(items: AppCatalogItem[], query: string, category: string) {
  const needle = query.trim().toLocaleLowerCase();
  return items.filter((item) => (category === "全部" || item.category === category) && (!needle || `${item.key} ${item.name} ${item.description}`.toLocaleLowerCase().includes(needle)));
}

/** 以不泄露 Compose 正文的方式展示官方升级差异摘要。 */
function UpdatePreviewSummary({ preview }: { preview: AppUpdatePreview }) {
  return <div className="app-detail-meta"><span className={preview.changed ? "text-danger" : "text-ok"}>{preview.currentMissing ? "当前配置缺失" : preview.changed ? "检测到配置变化" : "配置无变化"}</span><span>最新版本：{preview.latestVersion}</span><span>当前行数：{preview.currentLines}</span><span>最新行数：{preview.latestLines}</span><span className="mono">当前：{preview.currentHash ?? "不存在"}</span><span className="mono">最新：{preview.latestHash}</span></div>;
}

/** 复用服务器工作区导航并切换应用商店与已安装应用。 */
function AppTabs({ serverId, tab, onChange, installedCount }: { serverId: string; tab: AppTab; onChange: (tab: AppTab) => void; installedCount: number }) {
  return <nav className="workspace-tabs"><NavLink to={`/servers/${serverId}`}>概览</NavLink><button className={tab === "catalog" ? "active" : ""} onClick={() => onChange("catalog")}>应用目录</button><button className={tab === "installed" ? "active" : ""} onClick={() => onChange("installed")}>已安装 {installedCount ? `(${installedCount})` : ""}</button><NavLink to={`/servers/${serverId}/nginx`}>网站</NavLink><NavLink to={`/servers/${serverId}/database`}>数据库</NavLink><NavLink to={`/servers/${serverId}/docker`}>容器</NavLink><NavLink to={`/servers/${serverId}/tools`}>工具箱</NavLink></nav>;
}

/** 展示远端固定应用目录中的 Compose 项目，并提供生命周期动作。 */
function InstalledApps({ apps, composeAvailable, busy, onAction, onEnvironment, onHealth, onPreview }: { apps: InstalledApp[]; composeAvailable: boolean; busy: boolean; onAction: (app: InstalledApp, action: "start" | "stop" | "restart" | "pull" | "update" | "uninstall" | "restore") => void; onEnvironment: (app: InstalledApp) => void; onHealth: (app: InstalledApp) => void; onPreview: (app: InstalledApp) => void }) {
  if (!composeAvailable) return <div className="empty-panel"><Package size={28} /><h2>未发现 Docker Compose</h2><p>请先在工具箱安装 Docker Compose，再使用应用商店。</p></div>;
  if (!apps.length) return <div className="empty-panel"><Package size={28} /><h2>尚未安装应用</h2><p>从应用目录选择一个官方模板开始。</p></div>;
  return <div className="installed-app-list">{apps.map((app) => <article className="installed-app-card" key={app.composePath}><div className="app-card__icon"><Package size={20} /></div><div><strong>{app.key}</strong><p className="mono">{app.path}</p><small className={app.status === "running" ? "text-ok" : "text-muted"}>{app.status || "状态未知"}</small></div><div className="installed-app-actions"><Button size="sm" variant="ghost" onClick={() => onHealth(app)} disabled={busy}><Settings2 size={13} />健康</Button><Button size="sm" variant="ghost" onClick={() => onPreview(app)} disabled={busy}><RefreshCw size={13} />升级预览</Button><Button size="sm" variant="ghost" onClick={() => onEnvironment(app)} disabled={busy}><Settings2 size={13} />环境</Button><Button size="sm" variant="ghost" onClick={() => onAction(app, "start")} disabled={busy}><Play size={13} />启动</Button>{app.status !== "running" && <Button size="sm" variant="ghost" onClick={() => onAction(app, "restore")} disabled={busy}><Play size={13} />恢复</Button>}<Button size="sm" variant="ghost" onClick={() => onAction(app, "stop")} disabled={busy}><Square size={13} />停止</Button><Button size="sm" variant="ghost" onClick={() => onAction(app, "restart")} disabled={busy}><RefreshCw size={13} />重启</Button><Button size="sm" variant="ghost" onClick={() => onAction(app, "update")} disabled={busy}><RefreshCw size={13} />更新</Button><Button size="sm" variant="danger" onClick={() => onAction(app, "uninstall")} disabled={busy}><Trash2 size={13} />卸载</Button></div></article>)}</div>;
}

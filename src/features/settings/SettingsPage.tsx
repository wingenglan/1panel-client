import * as Dialog from "@radix-ui/react-dialog";
import { CheckCircle2, ClipboardList, Cloud, Download, HardDrive, Info, KeyRound, Languages, Layers, LockKeyhole, Moon, Package, Plus, RotateCcw, RefreshCw, Server, ShieldCheck, Trash2, Upload, X } from "lucide-react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { type ChangeEvent, useEffect, useRef, useState } from "react";
import { Button } from "../../components/ui/Button";
import { api } from "../../lib/api";
import { errorMessage } from "../../lib/errors";
import { applyLocale, readLocale, saveLocale, type Locale } from "../../lib/i18n";
import { pushNotice } from "../../lib/noticeStore";
import type { AppStoreSettings, BackupAccount, CronOfflineSchedulerSettings, PublicServerImport } from "../../types/server";

type BackupAccountKind = "local" | "webdav" | "s3" | "sftp";
type BackupAccountDraft = { id?: string; name: string; kind: BackupAccountKind; serverId: string; endpoint: string; remotePath: string; bucket: string; region: string; username: string; privateKeyPath: string; hostKeyFingerprint: string; secret: string };

/** 创建一个不会携带旧 secret 的备份账号编辑草稿。 */
function emptyBackupAccountDraft(): BackupAccountDraft {
  return { name: "", kind: "local", serverId: "", endpoint: "", remotePath: "C:\\Backups\\1panel-client", bucket: "", region: "us-east-1", username: "", privateKeyPath: "", hostKeyFingerprint: "", secret: "" };
}

/** 将后端账号公共字段映射到前端编辑器，并故意清空密钥输入框。 */
function backupAccountDraft(account: BackupAccount): BackupAccountDraft {
  return { id: account.id, name: account.name, kind: (account.kind === "webdav" || account.kind === "s3" || account.kind === "sftp" ? account.kind : "local"), serverId: account.serverId ?? "", endpoint: account.endpoint ?? "", remotePath: account.remotePath, bucket: account.bucket ?? "", region: account.region ?? "us-east-1", username: account.username ?? "", privateKeyPath: account.privateKeyPath ?? "", hostKeyFingerprint: account.hostKeyFingerprint ?? "", secret: "" };
}

/** 返回账号类型的中文名称，供设置页列表和表单提示复用。 */
function backupAccountKindLabel(kind: string): string {
  return kind === "local" ? "本机目录" : kind === "webdav" ? "WebDAV" : kind === "s3" ? "S3 兼容" : "SFTP";
}

/** 展示本地偏好和配置导入/导出；共享设置查询变化时同步编辑草稿。 */
export function SettingsPage() {
  const inputRef = useRef<HTMLInputElement>(null);
  const fullBackupInputRef = useRef<HTMLInputElement>(null);
  const queryClient = useQueryClient();
  const [busy, setBusy] = useState(false);
  const [backupPassword, setBackupPassword] = useState("");
  const [theme, setTheme] = useState<"system" | "dark" | "light">(() => (localStorage.getItem("1panel-client.theme") as "system" | "dark" | "light" | null) ?? "light");
  const [locale, setLocale] = useState<Locale>(() => readLocale());
  const [restoreWorkspace, setRestoreWorkspace] = useState(() => localStorage.getItem("1panel-client.restoreWorkspace") !== "false");
  const [pageTabs, setPageTabs] = useState(() => localStorage.getItem("1panel-client.pageTabs") !== "false");
  const [appStoreDraft, setAppStoreDraft] = useState<AppStoreSettings>({ source: "official", mirrorBaseUrl: null, mirrorBaseUrls: [], cacheTtlSeconds: 3600, offlineMode: false, mirrorKeyId: null, signatureConfigured: false });
  const [mirrorUrlsText, setMirrorUrlsText] = useState("");
  const [mirrorGeneratorOpen, setMirrorGeneratorOpen] = useState(false);
  const appStoreSettings = useQuery({ queryKey: ["appstore-settings"], queryFn: api.appStoreSettings, staleTime: Infinity });
  const saveAppStore = useMutation({
    mutationFn: (value: AppStoreSettings) => api.saveAppStoreSettings({ ...value, mirrorBaseUrls: mirrorUrlsText.split(/\r?\n/).map((item) => item.trim()).filter(Boolean), mirrorBaseUrl: mirrorUrlsText.split(/\r?\n/).map((item) => item.trim()).filter(Boolean)[0] ?? null }),
    onSuccess: async (value) => { setAppStoreDraft(value); setMirrorUrlsText(value.mirrorBaseUrls.join("\n")); await queryClient.invalidateQueries({ queryKey: ["app-catalog"] }); },
  });
  const clearAppStoreCache = useMutation({ mutationFn: api.clearAppStoreCache, onSuccess: async () => { await queryClient.invalidateQueries({ queryKey: ["app-catalog"] }); setMessage("应用商店缓存已清理"); } });

  const [lastAppStoreSettings, setLastAppStoreSettings] = useState<AppStoreSettings>();
  // 查询结果变化时同步草稿，避免通过 effect 追加一次渲染。
  if (appStoreSettings.data && appStoreSettings.data !== lastAppStoreSettings) {
    setLastAppStoreSettings(appStoreSettings.data);
    setAppStoreDraft(appStoreSettings.data);
    setMirrorUrlsText(appStoreSettings.data.mirrorBaseUrls.join("\n") || appStoreSettings.data.mirrorBaseUrl || "");
  }
  /** 镜像设置保存后更新草稿，并刷新共享查询。 */
  const handleMirrorSettingsSaved = (value: AppStoreSettings) => {
    setAppStoreDraft(value);
    setMirrorUrlsText(value.mirrorBaseUrls.join("\n") || value.mirrorBaseUrl || "");
    void queryClient.invalidateQueries({ queryKey: ["appstore-settings"] });
  };

  useEffect(() => {
    const media = window.matchMedia("(prefers-color-scheme: light)");
    const apply = () => { document.documentElement.dataset.theme = theme === "system" ? (media.matches ? "light" : "dark") : theme; };
    apply();
    localStorage.setItem("1panel-client.theme", theme);
    if (theme === "system") media.addEventListener("change", apply);
    return () => media.removeEventListener("change", apply);
  }, [theme]);
  useEffect(() => { localStorage.setItem("1panel-client.restoreWorkspace", String(restoreWorkspace)); }, [restoreWorkspace]);
  /** 菜单标签页单选与侧栏共享：写入本地存储并通知 AppShell 即时生效。 */
  const applyPageTabs = (checked: boolean) => {
    setPageTabs(checked);
    localStorage.setItem("1panel-client.pageTabs", String(checked));
    window.dispatchEvent(new Event("1panel-client:prefs"));
  };
  useEffect(() => { saveLocale(locale); applyLocale(locale); }, [locale]);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => { if (message) pushNotice("success", message); }, [message]);
  useEffect(() => { if (error) pushNotice("error", error); }, [error]);
  const audit = useQuery({ queryKey: ["audit-events"], queryFn: () => api.listAuditEvents(30) });
  const backupAccounts = useQuery({ queryKey: ["backup-accounts"], queryFn: api.backupAccounts });
  const offlineScheduler = useQuery<CronOfflineSchedulerSettings>({ queryKey: ["cron-offline-scheduler"], queryFn: api.cronOfflineSchedulerSettings });
  const servers = useQuery({ queryKey: ["servers"], queryFn: api.listServers });
  const [accountDraft, setAccountDraft] = useState<BackupAccountDraft>(() => emptyBackupAccountDraft());
  const [accountMessage, setAccountMessage] = useState<string | null>(null);
  const saveBackupAccount = useMutation({ mutationFn: () => api.saveBackupAccount({ id: accountDraft.id, name: accountDraft.name, kind: accountDraft.kind, serverId: accountDraft.kind === "local" ? accountDraft.serverId || undefined : undefined, endpoint: accountDraft.kind === "local" ? undefined : accountDraft.endpoint || undefined, remotePath: accountDraft.remotePath, bucket: accountDraft.kind === "s3" ? accountDraft.bucket || undefined : undefined, region: accountDraft.kind === "s3" ? accountDraft.region || undefined : undefined, username: accountDraft.kind === "local" ? undefined : accountDraft.username || undefined, privateKeyPath: accountDraft.kind === "sftp" ? accountDraft.privateKeyPath || undefined : undefined, hostKeyFingerprint: accountDraft.kind === "sftp" ? accountDraft.hostKeyFingerprint.trim() || undefined : undefined, secret: accountDraft.secret || undefined, clearSecret: false, confirmed: true }), onSuccess: async (value) => { setAccountDraft(emptyBackupAccountDraft()); setAccountMessage(`已保存备份账号：${value.name}`); await queryClient.invalidateQueries({ queryKey: ["backup-accounts"] }); } });
  const testBackupAccount = useMutation({ mutationFn: (id: string) => api.testBackupAccount(id), onSuccess: (value) => setAccountMessage(`${value.detail}${value.statusCode ? `（${value.statusCode}）` : ""}`) });
  const deleteBackupAccount = useMutation({ mutationFn: (id: string) => api.deleteBackupAccount(id, true), onSuccess: async () => { setAccountMessage("备份账号已删除"); await queryClient.invalidateQueries({ queryKey: ["backup-accounts"] }); } });
  const saveOfflineScheduler = useMutation({ mutationFn: (enabled: boolean) => api.saveCronOfflineSchedulerSettings({ enabled, confirmed: true }), onSuccess: async (value) => { await queryClient.invalidateQueries({ queryKey: ["cron-offline-scheduler"] }); setMessage(value.enabled ? "离线归档补传已启用" : "离线归档补传已停用"); } });

  /** 读取公共配置并下载 JSON；响应内容不包含 Keychain secret。 */
  const exportServers = async () => {
    setBusy(true); setError(null); setMessage(null);
    try {
      const payload = await api.exportServers();
      const blob = new Blob([JSON.stringify(payload, null, 2)], { type: "application/json" });
      const url = URL.createObjectURL(blob);
      const anchor = document.createElement("a");
      anchor.href = url; anchor.download = "1panel-client-server-config.json"; anchor.click();
      URL.revokeObjectURL(url);
      setMessage(`已导出 ${payload.servers.length} 台服务器的非敏感配置`);
    } catch (reason) { setError(errorMessage(reason)); } finally { setBusy(false); }
  };

  /** 校验版本化公共 JSON 后交给 Rust 端生成新服务器档案。 */
  const importServers = async (event: ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0]; event.target.value = "";
    if (!file) return;
    setBusy(true); setError(null); setMessage(null);
    try {
      const payload = JSON.parse(await file.text()) as { format?: string; version?: number; encrypted?: boolean; servers?: PublicServerImport[] };
      if (payload.format !== "1panel-client-backup" || payload.version !== 1 || payload.encrypted || !Array.isArray(payload.servers)) throw new Error("不是受支持的公共服务器配置文件");
      const imported = await api.importServers(payload.servers);
      await queryClient.invalidateQueries({ queryKey: ["servers"] });
      setMessage(`已导入 ${imported.length} 台服务器；密码和 sudo 凭据未包含，请逐台重新配置`);
    } catch (reason) { setError(errorMessage(reason)); } finally { setBusy(false); }
  };

  /** 使用用户临时输入的密码导出包含凭据的加密备份；密码不写入本地设置。 */
  const exportFullBackup = async () => {
    if (!backupPassword) { setError("请输入完整备份密码"); return; }
    setBusy(true); setError(null); setMessage(null);
    try {
      const payload = await api.exportFullBackup(backupPassword);
      const blob = new Blob([payload], { type: "application/json" });
      const url = URL.createObjectURL(blob);
      const anchor = document.createElement("a");
      anchor.href = url; anchor.download = "1panel-client-full-backup.json"; anchor.click();
      URL.revokeObjectURL(url);
      setMessage("已导出加密完整备份；请单独安全保存备份密码");
    } catch (reason) { setError(errorMessage(reason)); } finally { setBusy(false); }
  };

  /** 将加密备份交给 Rust 解密并重新写入系统凭据库。 */
  const importFullBackup = async (event: ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0]; event.target.value = "";
    if (!file) return;
    if (!backupPassword) { setError("请先输入完整备份密码"); return; }
    setBusy(true); setError(null); setMessage(null);
    try {
      const imported = await api.importFullBackup(await file.text(), backupPassword);
      await queryClient.invalidateQueries({ queryKey: ["servers"] });
      setMessage(`已解密并导入 ${imported.length} 台服务器；每条记录均生成了新的本地 ID`);
    } catch (reason) { setError(errorMessage(reason)); } finally { setBusy(false); }
  };

  /** 生成脱敏诊断 JSON 下载；导出内容不包含密码、私钥内容或远端命令输出。 */
  const exportDiagnostics = async () => {
    setBusy(true); setError(null); setMessage(null);
    try {
      const payload = await api.exportDiagnostics();
      const blob = new Blob([JSON.stringify(payload, null, 2)], { type: "application/json" });
      const url = URL.createObjectURL(blob);
      const anchor = document.createElement("a");
      anchor.href = url; anchor.download = "1panel-client-diagnostics.json"; anchor.click();
      URL.revokeObjectURL(url);
      setMessage("已导出脱敏诊断信息；其中不包含凭据和远端输出");
    } catch (reason) { setError(errorMessage(reason)); } finally { setBusy(false); }
  };

  return (
    <section className="settings-page">
      <div className="settings-list">
        <article><Moon /><div><strong>外观</strong><p>主题选择保存在本机，不影响服务器。</p></div><div className="theme-radio-group" role="radiogroup" aria-label="外观主题">{([["light", "亮色"], ["dark", "深色"], ["system", "跟随系统"]] as const).map(([value, label]) => <label key={value} className={`theme-radio${theme === value ? " is-active" : ""}`}><input type="radio" name="theme" value={value} checked={theme === value} onChange={() => setTheme(value)} /><span>{label}</span></label>)}</div></article>
        <article><Layers /><div><strong>菜单标签页</strong><p>在内容区顶部显示已打开的页面页签，与 Web 面板一致。</p></div><div className="theme-radio-group" role="radiogroup" aria-label="菜单标签页"><label className={`theme-radio${pageTabs ? " is-active" : ""}`}><input type="radio" name="pageTabs" value="enabled" checked={pageTabs} onChange={() => applyPageTabs(true)} /><span>启用</span></label><label className={`theme-radio${!pageTabs ? " is-active" : ""}`}><input type="radio" name="pageTabs" value="disabled" checked={!pageTabs} onChange={() => applyPageTabs(false)} /><span>停用</span></label></div></article>
        <article><Languages /><div><strong>语言偏好</strong><p>选择保存在本机；翻译资源会按同一 locale key 扩展。</p></div><select value={locale} onChange={(event) => setLocale(event.target.value as Locale)}><option value="zh-CN">简体中文</option><option value="en-US">English</option></select></article>
        <article><LockKeyhole /><div><strong>凭据</strong><p>SSH 与 sudo 密码存储在操作系统安全存储，不写入数据库。</p></div><span className="settings-badge">已保护</span></article>
        <article><RotateCcw /><div><strong>连接恢复</strong><p>保存是否恢复上次打开的工作区偏好；不会自动重放危险任务。</p></div><input type="checkbox" checked={restoreWorkspace} onChange={(event) => setRestoreWorkspace(event.target.checked)} /></article>
        <article className="settings-actions"><RefreshCw /><div><strong>更新能力</strong><p>更新通道和签名包校验入口已预留；当前版本以应用发布包为准，不会静默更新。</p></div><Button variant="ghost" size="sm" disabled><RefreshCw size={13} /> 检查更新（预留）</Button></article>
        <article className="settings-actions"><Download /><div><strong>服务器配置</strong><p>普通导出不包含密码、私钥内容或 sudo 凭据；导入会生成新档案，避免覆盖现有配置。</p></div><div><Button variant="secondary" size="sm" onClick={() => void exportServers()} disabled={busy}><Download size={13} /> 导出 JSON</Button><Button variant="ghost" size="sm" onClick={() => inputRef.current?.click()} disabled={busy}><Upload size={13} /> 导入 JSON</Button><input ref={inputRef} type="file" accept="application/json,.json" hidden onChange={(event) => void importServers(event)} /></div></article>
        <article className="settings-actions settings-backup"><LockKeyhole /><div><strong>加密完整备份</strong><p>使用 Argon2id + AES-256-GCM 加密配置和系统凭据；密码只在本次操作中使用。</p><label className="settings-secret"><span>备份密码</span><input type="password" autoComplete="new-password" value={backupPassword} onChange={(event) => setBackupPassword(event.target.value)} /></label></div><div><Button variant="secondary" size="sm" onClick={() => void exportFullBackup()} disabled={busy || !backupPassword}><Download size={13} /> 导出加密备份</Button><Button variant="ghost" size="sm" onClick={() => fullBackupInputRef.current?.click()} disabled={busy || !backupPassword}><Upload size={13} /> 导入加密备份</Button><input ref={fullBackupInputRef} type="file" accept="application/json,.json" hidden onChange={(event) => void importFullBackup(event)} /></div></article>
        <article className="settings-actions settings-backup-accounts"><Cloud /><div className="settings-backup-accounts__body"><strong>备份账号</strong><p>为计划任务准备本机目录、WebDAV、S3 兼容对象存储或 SFTP 目标；密码和 secret 只进入系统密钥链。</p><div className="settings-backup-accounts__form"><label><span>名称</span><input value={accountDraft.name} onChange={(event) => setAccountDraft((current) => ({ ...current, name: event.target.value }))} placeholder="生产备份" /></label><label><span>类型</span><select value={accountDraft.kind} onChange={(event) => setAccountDraft((current) => ({ ...current, kind: event.target.value as BackupAccountKind }))}><option value="local">本机目录</option><option value="webdav">WebDAV</option><option value="s3">S3 兼容对象存储</option><option value="sftp">SFTP</option></select></label>{accountDraft.kind === "local" && <label><span>绑定服务器</span><select value={accountDraft.serverId} onChange={(event) => setAccountDraft((current) => ({ ...current, serverId: event.target.value }))}><option value="">选择服务器</option>{(servers.data ?? []).map((server) => <option key={server.id} value={server.id}>{server.name} · {server.host}</option>)}</select></label>}{accountDraft.kind !== "local" && <label className="settings-backup-accounts__wide"><span>Endpoint</span><input value={accountDraft.endpoint} onChange={(event) => setAccountDraft((current) => ({ ...current, endpoint: event.target.value }))} placeholder={accountDraft.kind === "sftp" ? "sftp://backup.example.com:22" : "https://storage.example.com"} /></label>}<label className={accountDraft.kind !== "local" ? "settings-backup-accounts__wide" : ""}><span>{accountDraft.kind === "local" ? "本机目标目录" : "远端路径前缀"}</span><input value={accountDraft.remotePath} onChange={(event) => setAccountDraft((current) => ({ ...current, remotePath: event.target.value }))} placeholder={accountDraft.kind === "local" ? "C:\\Backups" : "1panel"} /><small>{accountDraft.kind === "local" ? "归档会从服务器下载到当前电脑此目录。" : "只允许安全 POSIX 路径，不含查询参数或父目录跳转。"}</small></label>{accountDraft.kind === "s3" && <><label><span>Bucket</span><input value={accountDraft.bucket} onChange={(event) => setAccountDraft((current) => ({ ...current, bucket: event.target.value }))} placeholder="backups" /></label><label><span>Region</span><input value={accountDraft.region} onChange={(event) => setAccountDraft((current) => ({ ...current, region: event.target.value }))} placeholder="us-east-1" /></label></>}{accountDraft.kind !== "local" && <><label><span>{accountDraft.kind === "s3" ? "Access key" : "用户名"}</span><input value={accountDraft.username} onChange={(event) => setAccountDraft((current) => ({ ...current, username: event.target.value }))} /></label><label><span>{accountDraft.kind === "sftp" ? "私钥路径（可选）" : "Secret / 密码"}</span><input type={accountDraft.kind === "sftp" ? "text" : "password"} value={accountDraft.kind === "sftp" ? accountDraft.privateKeyPath : accountDraft.secret} onChange={(event) => setAccountDraft((current) => ({ ...current, [accountDraft.kind === "sftp" ? "privateKeyPath" : "secret"]: event.target.value }))} placeholder={accountDraft.id ? "留空保持现有 secret" : "请输入 secret"} /></label>{accountDraft.kind === "sftp" && <><label><span>SFTP 密码（可选）</span><input type="password" value={accountDraft.secret} onChange={(event) => setAccountDraft((current) => ({ ...current, secret: event.target.value }))} /></label><label className="settings-backup-accounts__wide"><span>Host Key 指纹（可选）</span><input value={accountDraft.hostKeyFingerprint} onChange={(event) => setAccountDraft((current) => ({ ...current, hostKeyFingerprint: event.target.value }))} placeholder="SHA256:..." /><small>填写后会拒绝 SFTP 服务器指纹变化；留空表示仅校验账号参数。</small></label></>}</>}</div><div className="dialog-actions settings-backup-accounts__actions"><Button size="sm" variant="primary" onClick={() => saveBackupAccount.mutate()} disabled={saveBackupAccount.isPending || !accountDraft.name.trim() || !accountDraft.remotePath.trim()}><Plus size={13} />{saveBackupAccount.isPending ? "保存中…" : accountDraft.id ? "更新账号" : "添加账号"}</Button>{accountDraft.id && <Button size="sm" variant="ghost" onClick={() => setAccountDraft(emptyBackupAccountDraft())}>取消编辑</Button>}</div>{(saveBackupAccount.error || deleteBackupAccount.error || testBackupAccount.error) && <div className="form-error">{errorMessage(saveBackupAccount.error ?? deleteBackupAccount.error ?? testBackupAccount.error)}</div>}{accountMessage && <div className="settings-backup-accounts__message"><CheckCircle2 size={14} />{accountMessage}</div>}{backupAccounts.isLoading && <small>正在读取备份账号…</small>}{backupAccounts.data?.length ? <div className="settings-backup-accounts__list">{backupAccounts.data.map((account) => <div className="settings-backup-account" key={account.id}><span className="settings-backup-account__icon">{account.kind === "local" ? <HardDrive size={14} /> : account.kind === "s3" ? <Cloud size={14} /> : account.kind === "sftp" ? <Server size={14} /> : <KeyRound size={14} />}</span><div><strong>{account.name}</strong><small>{backupAccountKindLabel(account.kind)} · {account.remotePath}{account.hasSecret ? " · secret 已配置" : " · 缺少 secret"}</small></div><div className="settings-backup-account__actions"><Button size="sm" variant="ghost" onClick={() => setAccountDraft(backupAccountDraft(account))} disabled={saveBackupAccount.isPending}>编辑</Button><Button size="sm" variant="ghost" onClick={() => testBackupAccount.mutate(account.id)} disabled={testBackupAccount.isPending}>测试</Button><Button size="sm" variant="danger" onClick={() => window.confirm(`确认删除备份账号“${account.name}”？`) && deleteBackupAccount.mutate(account.id)} disabled={deleteBackupAccount.isPending}><Trash2 size={13} /></Button></div></div>)}</div> : !backupAccounts.isLoading && <small>尚未配置备份账号。</small>}</div></article>
        <article className="settings-actions"><RefreshCw /><div><strong>离线归档补传</strong><p>客户端在线时读取服务器归档事件，并自动补传到计划任务选择的备份账号；状态保存在本机，不会写入远端凭据。</p></div><label className="check-field"><input type="checkbox" checked={offlineScheduler.data?.enabled ?? true} onChange={(event) => saveOfflineScheduler.mutate(event.target.checked)} disabled={offlineScheduler.isLoading || saveOfflineScheduler.isPending} /><span>{offlineScheduler.data?.enabled === false ? "已停用" : "已启用"}</span></label>{saveOfflineScheduler.error && <small className="text-danger">{errorMessage(saveOfflineScheduler.error)}</small>}</article>
        <article className="settings-actions"><ClipboardList /><div><strong>诊断与审计</strong><p>导出本地连接状态、非敏感档案和最近审计记录，便于排障；不包含凭据和远端输出。</p></div><Button variant="secondary" size="sm" onClick={() => void exportDiagnostics()} disabled={busy}><Download size={13} /> 导出诊断 JSON</Button></article>
        <article className="settings-audit"><div><strong>最近审计记录</strong><p>只保存本地操作元数据，不保存命令输出。</p></div>{audit.isLoading && <small>正在读取…</small>}{audit.error && <small className="text-danger">{errorMessage(audit.error)}</small>}{audit.data?.length ? <div className="settings-audit__list">{audit.data.slice(0, 8).map((event) => <div key={event.id}><span className={`audit-result is-${event.result}`}>{event.result}</span><strong>{event.summary}</strong><small>{new Date(event.createdAt).toLocaleString()}</small></div>)}</div> : !audit.isLoading && <small>尚无审计记录。</small>}</article>
        <article className="settings-appstore">
          <div className="settings-appstore__head"><Package /><div><strong>App store 目录与缓存</strong><p>选择本地应用商店来源并管理目录、详情与镜像缓存；改动只会影响本机，不会修改远端面板。</p></div><span className="settings-badge">{appStoreSettings.data?.source === "mirror" ? "镜像" : appStoreSettings.data?.offlineMode ? "离线" : "官方"}</span></div>
          <div className="appstore-settings-form">
            <label><span>应用商店来源</span><select value={appStoreDraft.source} onChange={(event) => setAppStoreDraft((current) => ({ ...current, source: event.target.value as "official" | "mirror" }))}><option value="official">官方仓库</option><option value="mirror">自定义镜像仓库</option></select></label>
            <label><span>镜像仓库地址（每行一个，优先使用第一个可用地址）</span><textarea rows={5} value={mirrorUrlsText} onChange={(event) => setMirrorUrlsText(event.target.value)} placeholder="https://mirror.example.com/1panel-apps" /></label>
            <label><span>目录缓存有效期（秒）</span><input type="number" min={60} value={appStoreDraft.cacheTtlSeconds} onChange={(event) => setAppStoreDraft((current) => ({ ...current, cacheTtlSeconds: Math.max(60, Number(event.target.value) || 60) }))} /></label>
            <label className="checkbox-row"><input type="checkbox" checked={appStoreDraft.offlineMode} onChange={(event) => setAppStoreDraft((current) => ({ ...current, offlineMode: event.target.checked }))} /><span>离线模式（只使用本地缓存，不访问网络）</span></label>
            {appStoreSettings.error && <div className="form-error">{errorMessage(appStoreSettings.error)}</div>}
            {saveAppStore.error && <div className="form-error">{errorMessage(saveAppStore.error)}</div>}
            {clearAppStoreCache.error && <div className="form-error">{errorMessage(clearAppStoreCache.error)}</div>}
            <div className="dialog-actions">
              <Button size="sm" variant="primary" onClick={() => saveAppStore.mutate(appStoreDraft)} disabled={saveAppStore.isPending}>{saveAppStore.isPending ? "保存中…" : "保存设置"}</Button>
              <Button size="sm" variant="secondary" onClick={() => clearAppStoreCache.mutate()} disabled={clearAppStoreCache.isPending}>{clearAppStoreCache.isPending ? "清理中…" : "清理目录缓存"}</Button>
              <Button size="sm" variant="ghost" onClick={() => setMirrorGeneratorOpen(true)}><Package size={13} />生成静态镜像</Button>
            </div>
          </div>
        </article>
        <article><Info /><div><strong>关于 1Panel Client</strong><p>版本 0.1.0 · 多服务器、本地优先桌面客户端 · GPL-3.0</p></div><span className="settings-badge">社区版</span></article>
      </div>
      {message && <div className="settings-feedback is-success">{message}</div>}
      {error && <div className="settings-feedback is-error">{error}</div>}
      <MirrorGeneratorDialog open={mirrorGeneratorOpen} onOpenChange={setMirrorGeneratorOpen} settings={appStoreSettings.data ?? appStoreDraft} onSettingsSaved={handleMirrorSettingsSaved} />
    </section>
  );
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
  const submit = () => { generation.mutate(); };
  return <Dialog.Root open={open} onOpenChange={onOpenChange}><Dialog.Portal><Dialog.Overlay className="dialog-overlay" /><Dialog.Content className="dialog-content"><div className="dialog-header"><div><Dialog.Title>生成静态应用镜像</Dialog.Title><Dialog.Description>从当前应用商店来源下载目录、metadata、版本 Compose 和环境模板，写入本机静态目录。</Dialog.Description></div><Dialog.Close asChild><button className="icon-control" aria-label="关闭"><X size={17} /></button></Dialog.Close></div><div className="app-install-form"><label><span>输出目录（绝对路径）</span><input value={destination} onChange={(event) => setDestination(event.target.value)} placeholder="C:\\1panel-mirror 或 /srv/1panel-mirror" /></label><label><span>验签 key ID</span><input value={keyId} onChange={(event) => setKeyId(event.target.value)} placeholder="mirror-main" /></label><label><span>HMAC 验签令牌</span><input type="password" value={signingSecret} onChange={(event) => setSigningSecret(event.target.value)} autoComplete="new-password" placeholder="至少 16 个字符" /><small>只发送给本地 Rust 后端；不会写入普通配置文件。生成后可保存到操作系统密钥链。</small></label><label><span>最多生成应用数</span><input type="number" min={1} max={512} value={maxApps} onChange={(event) => setMaxApps(Math.max(1, Math.min(512, Number(event.target.value) || 1)))} /></label><label className="checkbox-row"><input type="checkbox" checked={rememberVerification} onChange={(event) => setRememberVerification(event.target.checked)} /><span>将令牌保存为当前客户端的镜像验签配置</span></label><label className="checkbox-row"><input type="checkbox" checked={confirmed} onChange={(event) => setConfirmed(event.target.checked)} /><span>我确认允许在本机创建或覆盖镜像目录文件</span></label>{generation.error && <div className="form-error">{errorMessage(generation.error)}</div>}{generation.data && <div className="security-note"><ShieldCheck size={18} /><span>已生成 {generation.data.appCount} 个应用、{generation.data.versionCount} 个版本、{generation.data.fileCount} 个文件；目录摘要 {generation.data.catalogSha256.slice(0, 16)}…</span></div>}<div className="dialog-actions"><Button variant="ghost" onClick={() => onOpenChange(false)}>关闭</Button><Button variant="primary" onClick={submit} disabled={!destination.trim() || !keyId.trim() || !signingSecret.trim() || !confirmed || generation.isPending}>{generation.isPending ? "生成中…" : "生成并签名"}</Button></div></div></Dialog.Content></Dialog.Portal></Dialog.Root>;
}

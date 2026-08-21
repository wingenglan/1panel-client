import { CheckCircle2, ClipboardList, Cloud, Download, HardDrive, Info, KeyRound, Languages, LockKeyhole, Moon, Plus, RotateCcw, RefreshCw, Server, Trash2, Upload } from "lucide-react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { type ChangeEvent, useEffect, useRef, useState } from "react";
import { Button } from "../../components/ui/Button";
import { api } from "../../lib/api";
import { errorMessage } from "../../lib/errors";
import { applyLocale, readLocale, saveLocale, type Locale } from "../../lib/i18n";
import { pushNotice } from "../../lib/noticeStore";
import type { BackupAccount, CronOfflineSchedulerSettings, PublicServerImport } from "../../types/server";

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

/** 展示本地偏好和不含 secret 的服务器配置导入/导出操作。 */
export function SettingsPage() {
  const inputRef = useRef<HTMLInputElement>(null);
  const fullBackupInputRef = useRef<HTMLInputElement>(null);
  const queryClient = useQueryClient();
  const [busy, setBusy] = useState(false);
  const [backupPassword, setBackupPassword] = useState("");
  const [theme, setTheme] = useState<"system" | "dark" | "light">(() => (localStorage.getItem("1panel-client.theme") as "system" | "dark" | "light" | null) ?? "light");
  const [locale, setLocale] = useState<Locale>(() => readLocale());
  const [restoreWorkspace, setRestoreWorkspace] = useState(() => localStorage.getItem("1panel-client.restoreWorkspace") !== "false");

  useEffect(() => {
    const media = window.matchMedia("(prefers-color-scheme: light)");
    const apply = () => { document.documentElement.dataset.theme = theme === "system" ? (media.matches ? "light" : "dark") : theme; };
    apply();
    localStorage.setItem("1panel-client.theme", theme);
    if (theme === "system") media.addEventListener("change", apply);
    return () => media.removeEventListener("change", apply);
  }, [theme]);
  useEffect(() => { localStorage.setItem("1panel-client.restoreWorkspace", String(restoreWorkspace)); }, [restoreWorkspace]);
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
      <div className="workspace-header">
        <div><div className="breadcrumb">应用</div><h1>设置</h1><p>本地偏好与安全策略</p></div>
      </div>
      <div className="settings-list">
        <article><Moon /><div><strong>外观</strong><p>主题选择保存在本机，不影响服务器。</p></div><select value={theme} onChange={(event) => setTheme(event.target.value as "system" | "dark" | "light")}><option value="system">跟随系统</option><option value="dark">深色</option><option value="light">浅色</option></select></article>
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
        <article><Info /><div><strong>关于 1Panel Client</strong><p>版本 0.1.0 · 多服务器、本地优先桌面客户端 · GPL-3.0</p></div><span className="settings-badge">社区版</span></article>
      </div>
      {message && <div className="settings-feedback is-success">{message}</div>}
      {error && <div className="settings-feedback is-error">{error}</div>}
    </section>
  );
}

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Archive, Bell, Clock3, Download, History, Play, Plus, RefreshCw, ShieldAlert, Trash2, Upload } from "lucide-react";
import { type ChangeEvent, useRef, useState } from "react";
import { useParams } from "react-router-dom";
import { Button } from "../../components/ui/Button";
import { api } from "../../lib/api";
import { errorMessage } from "../../lib/errors";
import { pushNotice } from "../../lib/noticeStore";
import type { BackupAccount, CronJob, CronJobExport, CronJobHistoryEntry, CronNotificationSettings, InstalledAppsSnapshot, WebsiteSnapshot } from "../../types/server";

type CronKind = "shell" | "url" | "directory" | "database" | "log" | "website" | "app";
type CronNotificationProvider = "generic" | "slack" | "discord" | "dingtalk" | "wecom";
type CronNotificationDraft = {
  notifyInApp: boolean;
  notifyWebhook: boolean;
  provider: CronNotificationProvider;
  webhookUrl: string;
  signingSecret: string;
  clearWebhook: boolean;
  clearSigningSecret: boolean;
};
type CronForm = {
  schedule: string;
  kind: CronKind;
  command: string;
  urls: string;
  sourcePaths: string;
  destination: string;
  databaseEngine: "mysql" | "mariadb" | "postgresql";
  databaseName: string;
  excludePaths: string;
  websiteDomain: string;
  appInstallPath: string;
  retentionCount: string;
  retentionDays: string;
  backupAccountIds: string[];
  user: string;
};

/** 将多行输入转换为非空远端参数，保持 UI 显示内容与提交内容一致。 */
function splitCronLines(value: string): string[] {
  return value.split(/\r?\n/).map((item) => item.trim()).filter(Boolean);
}

/** 根据任务类型判断创建表单是否具备最少的可执行参数。 */
function canSubmitCronForm(form: CronForm): boolean {
  if (!form.schedule.trim()) return false;
  if (form.kind === "shell") return Boolean(form.command.trim());
  if (form.kind === "url") return splitCronLines(form.urls).length > 0;
  if (form.kind === "directory") return splitCronLines(form.sourcePaths).length > 0 && Boolean(form.destination.trim());
  if (form.kind === "database") return Boolean(form.databaseName.trim() && form.destination.trim());
  if (form.kind === "website") return Boolean(form.websiteDomain.trim() && form.destination.trim());
  if (form.kind === "app") return Boolean(form.appInstallPath.trim() && form.destination.trim());
  return Boolean(form.destination.trim());
}

/** 将可选的保留输入转换为正整数；空值保持未启用，范围由 Rust 端再次校验。 */
function optionalRetentionValue(value: string): number | undefined {
  const trimmed = value.trim();
  if (!trimmed) return undefined;
  const parsed = Number(trimmed);
  return Number.isInteger(parsed) && parsed > 0 ? parsed : undefined;
}

/** 为任务列表生成不暴露完整命令的保留策略摘要。 */
function cronRetentionLabel(job: CronJob): string | null {
  const values = [job.retentionCount ? `保留 ${job.retentionCount} 份` : "", job.retentionDays ? `${job.retentionDays} 天` : ""].filter(Boolean);
  return values.length ? values.join(" / ") : null;
}

/** 为任务列表生成外部归档账号数量摘要，不显示账号 endpoint 或凭据。 */
function cronBackupAccountLabel(job: CronJob): string | null {
  return job.backupAccountIds.length ? `上传到 ${job.backupAccountIds.length} 个账号` : null;
}

/** 将后端任务类型映射为列表中的简短中文标签。 */
function cronKindLabel(kind: CronJob["kind"]): string {
  switch (kind) {
    case "url": return "URL";
    case "directory": return "目录备份";
    case "database": return "数据库备份";
    case "log": return "日志备份";
    case "website": return "网站备份";
    case "app": return "应用备份";
    default: return "Shell";
  }
}

/** 校验用户选择的计划任务 JSON 外壳，详细字段仍由 Rust 端再次验证。 */
function isCronJobExport(value: unknown): value is CronJobExport {
  if (!value || typeof value !== "object") return false;
  const payload = value as Partial<CronJobExport>;
  return payload.format === "1panel-client-cronjobs"
    && payload.version === 1
    && Array.isArray(payload.jobs)
    && payload.jobs.length > 0
    && payload.jobs.length <= 200;
}

/** 下载计划任务 JSON；文件只由用户显式触发生成，不写入浏览器持久化存储。 */
function downloadCronExport(payload: CronJobExport) {
  const blob = new Blob([JSON.stringify(payload, null, 2)], { type: "application/json" });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = "1panel-client-cronjobs.json";
  anchor.click();
  URL.revokeObjectURL(url);
}

/** 展示真实 crontab 与 systemd timer，并支持任务创建、执行、导入导出和本地历史管理。 */
export function CronjobPage() {
  const { serverId = "" } = useParams();
  const queryClient = useQueryClient();
  const importInputRef = useRef<HTMLInputElement>(null);
  const [form, setForm] = useState<CronForm>({ schedule: "0 * * * *", kind: "shell", command: "", urls: "", sourcePaths: "", destination: "", databaseEngine: "mysql", databaseName: "", excludePaths: "", websiteDomain: "", appInstallPath: "", retentionCount: "", retentionDays: "", backupAccountIds: [], user: "" });
  const [notificationOpen, setNotificationOpen] = useState(false);
  const [notificationDraftOverride, setNotificationDraftOverride] = useState<{ serverId: string; draft: CronNotificationDraft } | null>(null);
  const cronjobs = useQuery({ queryKey: ["cronjobs", serverId], queryFn: () => api.cronjobs(serverId), enabled: Boolean(serverId) });
  const history = useQuery({ queryKey: ["cronjob-history", serverId], queryFn: () => api.cronjobHistory(serverId), enabled: Boolean(serverId) });
  const notification = useQuery<CronNotificationSettings>({ queryKey: ["cron-notification-settings", serverId], queryFn: () => api.cronNotificationSettings(serverId), enabled: Boolean(serverId) });
  const notificationDraft = notificationDraftOverride?.serverId === serverId ? notificationDraftOverride.draft : { notifyInApp: notification.data?.notifyInApp ?? true, notifyWebhook: notification.data?.notifyWebhook ?? false, provider: (notification.data?.provider as CronNotificationProvider | undefined) ?? "generic", webhookUrl: "", signingSecret: "", clearWebhook: false, clearSigningSecret: false };
  const websites = useQuery<WebsiteSnapshot>({ queryKey: ["websites", serverId, "cron-selection"], queryFn: () => api.websites(serverId), enabled: Boolean(serverId) && form.kind === "website" });
  const installedApps = useQuery<InstalledAppsSnapshot>({ queryKey: ["installed-apps", serverId, "cron-selection"], queryFn: () => api.installedApps(serverId), enabled: Boolean(serverId) && form.kind === "app" });
  const backupAccounts = useQuery<BackupAccount[]>({ queryKey: ["backup-accounts", "cronjob"], queryFn: api.backupAccounts });
  const selectedBackupAccounts = backupAccounts.data?.filter((account) => form.backupAccountIds.includes(account.id)) ?? [];
  const save = useMutation({ mutationFn: () => api.saveCronjob({ serverId, schedule: form.schedule, kind: form.kind, command: form.kind === "shell" ? form.command : "", urls: form.kind === "url" ? splitCronLines(form.urls) : undefined, sourcePaths: form.kind === "directory" ? splitCronLines(form.sourcePaths) : undefined, destination: ["directory", "database", "log", "website", "app"].includes(form.kind) ? form.destination || undefined : undefined, databaseEngine: form.kind === "database" ? form.databaseEngine : undefined, databaseName: form.kind === "database" ? form.databaseName || undefined : undefined, excludePaths: ["directory", "website", "app"].includes(form.kind) ? splitCronLines(form.excludePaths) : undefined, websiteDomain: form.kind === "website" ? form.websiteDomain || undefined : undefined, appInstallPath: form.kind === "app" ? form.appInstallPath || undefined : undefined, retentionCount: ["directory", "database", "log", "website", "app"].includes(form.kind) ? optionalRetentionValue(form.retentionCount) : undefined, retentionDays: ["directory", "database", "log", "website", "app"].includes(form.kind) ? optionalRetentionValue(form.retentionDays) : undefined, backupAccountIds: ["directory", "database", "log", "website", "app"].includes(form.kind) ? form.backupAccountIds : undefined, defaultBackupAccountId: selectedBackupAccounts[0]?.id, user: form.user || undefined, enabled: true, confirmed: true }), onSuccess: async () => { setForm((current) => ({ ...current, command: "", urls: "", sourcePaths: "", destination: "", databaseName: "", excludePaths: "", websiteDomain: "", appInstallPath: "", retentionCount: "", retentionDays: "", backupAccountIds: [] })); await queryClient.invalidateQueries({ queryKey: ["cronjobs", serverId] }); } });
  const saveNotification = useMutation({ mutationFn: () => api.saveCronNotificationSettings({ serverId, notifyInApp: notificationDraft.notifyInApp, notifyWebhook: notificationDraft.notifyWebhook, provider: notificationDraft.provider, webhookUrl: notificationDraft.webhookUrl.trim() || undefined, webhookSigningSecret: notificationDraft.signingSecret.trim() || undefined, clearWebhook: notificationDraft.clearWebhook, clearSigningSecret: notificationDraft.clearSigningSecret, confirmed: true }), onSuccess: async (value) => { setNotificationDraft({ ...notificationDraft, webhookUrl: "", signingSecret: "", clearWebhook: false, clearSigningSecret: false }); await queryClient.invalidateQueries({ queryKey: ["cron-notification-settings", serverId] }); pushNotice("success", value.notifyWebhook ? "计划任务报告通知已启用" : "计划任务报告通知已保存"); } });
  const action = useMutation({ mutationFn: (input: { job: CronJob; action: "delete" | "run" }) => api.cronjobAction({ serverId, id: input.job.id, command: input.job.command, user: input.job.user, backupAccountIds: input.job.backupAccountIds, action: input.action, confirmed: true }), onSuccess: async (result, variables) => { await queryClient.invalidateQueries({ queryKey: ["cronjobs", serverId] }); await queryClient.invalidateQueries({ queryKey: ["cronjob-history", serverId] }); if (variables.action === "run" && notification.data?.notifyInApp) pushNotice("success", `计划任务已执行：${result.id}`); } });
  const clearHistory = useMutation({ mutationFn: () => api.clearCronjobHistory(serverId), onSuccess: async () => { await queryClient.invalidateQueries({ queryKey: ["cronjob-history", serverId] }); pushNotice("success", "计划任务本地执行历史已清除"); } });

  /** 更新计划任务表单，并让输入内容在提交前保持可见。 */
  const update = <K extends keyof CronForm>(key: K, value: CronForm[K]) => setForm((current) => ({ ...current, [key]: value }));
  /** 将后端通知策略同步到表单；敏感字段始终保持空白，避免回填到页面。 */
  const setNotificationDraft = (draft: CronNotificationDraft) => setNotificationDraftOverride({ serverId, draft });
  /** 更新报告通知表单，webhook URL 和签名密钥只在用户保存时发送到 Rust 密钥链。 */
  const updateNotification = <K extends keyof CronNotificationDraft>(key: K, value: CronNotificationDraft[K]) => setNotificationDraft({ ...notificationDraft, [key]: value });
  /** 提交 Shell、URL、网站/应用或备份任务；Rust 端负责 cron 语法和远端命令安全校验。 */
  const submit = () => { if (canSubmitCronForm(form)) save.mutate(); };
  /** 立即运行用户选中的任务，并在执行前再次确认远端副作用。 */
  const run = (job: CronJob) => { if (window.confirm(`立即运行任务？\n${job.command}${job.backupAccountIds.length ? `\n归档完成后上传到 ${job.backupAccountIds.length} 个备份账号` : ""}`)) action.mutate({ job, action: "run" }); };
  /** 只允许删除本客户端写入的 marker 任务。 */
  const remove = (job: CronJob) => { if (job.managed && window.confirm(`确认删除计划任务？\n${job.command}`)) action.mutate({ job, action: "delete" }); };
  /** 导出当前远端 crontab 快照，systemd timer 不属于可写任务因此不导出。 */
  const exportJobs = async () => { try { const payload = await api.cronjobExport(serverId); downloadCronExport(payload); pushNotice("success", `已导出 ${payload.jobs.length} 条计划任务`); } catch (reason) { pushNotice("error", errorMessage(reason)); } };
  /** 读取并确认版本化任务文件；受支持类型保留 marker，未知类型安全降级为 Shell。 */
  const importJobs = async (event: ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    event.target.value = "";
    if (!file) return;
    try {
      const payload: unknown = JSON.parse(await file.text());
      if (!isCronJobExport(payload)) throw new Error("不是受支持的计划任务 JSON 文件");
      if (!window.confirm(`将导入 ${payload.jobs.length} 条任务并写入远端 crontab。受支持类型会保留任务类型，未知类型将安全降级为 Shell，是否继续？`)) return;
      const result = await api.cronjobImport({ serverId, jobs: payload.jobs, confirmed: true });
      await queryClient.invalidateQueries({ queryKey: ["cronjobs", serverId] });
      const suffix = result.failures.length ? `，${result.failures.length} 条失败` : "";
      const accountSuffix = result.unresolvedBackupAccounts ? `，${result.unresolvedBackupAccounts} 个备份账号引用未恢复` : "";
      pushNotice(result.failures.length ? "error" : "success", `已导入 ${result.imported} 条任务${suffix}${accountSuffix}`);
    } catch (reason) { pushNotice("error", errorMessage(reason)); }
  };
  /** 删除本机保存的当前服务器执行历史，远端 crontab 保持不变。 */
  const clearLocalHistory = () => { if (history.data?.length && window.confirm("清除当前服务器的本地执行历史？不会删除远端任务。")) clearHistory.mutate(); };

  return <section className="cronjob-page">
    <div className="page-toolbar"><Button variant="ghost" onClick={() => setNotificationOpen((current) => !current)}><Bell size={14} /> 报告通知</Button><Button variant="ghost" onClick={() => void exportJobs()} disabled={cronjobs.isFetching || !cronjobs.data}><Download size={14} /> 导出任务</Button><Button variant="ghost" onClick={() => importInputRef.current?.click()} disabled={cronjobs.isFetching}><Upload size={14} /> 导入任务</Button><input ref={importInputRef} type="file" accept="application/json,.json" hidden onChange={(event) => void importJobs(event)} /><Button variant="secondary" onClick={() => cronjobs.refetch()} disabled={cronjobs.isFetching}><RefreshCw className={cronjobs.isFetching ? "spin" : ""} size={14} /> 刷新任务</Button></div>
    {notificationOpen && <section className="cron-notification-settings"><header><div><span className="section-kicker">Execution report</span><h2>计划任务报告通知</h2><p>手动运行任务完成后发送；URL 与签名密钥保存在本机系统密钥链。</p></div><Bell size={22} /></header><div className="field-grid field-grid--3"><label className="check-field"><input type="checkbox" checked={notificationDraft.notifyInApp} onChange={(event) => updateNotification("notifyInApp", event.target.checked)} /><span>应用内通知</span></label><label className="check-field"><input type="checkbox" checked={notificationDraft.notifyWebhook} onChange={(event) => updateNotification("notifyWebhook", event.target.checked)} /><span>Webhook 通知</span></label><label><span>渠道</span><select value={notificationDraft.provider} onChange={(event) => updateNotification("provider", event.target.value as CronNotificationProvider)}><option value="generic">通用 JSON</option><option value="slack">Slack</option><option value="discord">Discord</option><option value="dingtalk">钉钉</option><option value="wecom">企业微信</option></select></label>{notificationDraft.notifyWebhook && <><label className="field-grid__wide"><span>Webhook URL {notification.data?.webhookConfigured ? "（已配置，留空保持不变）" : ""}</span><input type="url" value={notificationDraft.webhookUrl} onChange={(event) => updateNotification("webhookUrl", event.target.value)} placeholder="https://example.invalid/webhook" />{notification.data?.webhookConfigured && <Button size="sm" variant="ghost" onClick={() => updateNotification("clearWebhook", !notificationDraft.clearWebhook)}>{notificationDraft.clearWebhook ? "撤销清除 URL" : "清除已保存 URL"}</Button>}</label><label><span>签名密钥 {notification.data?.signingSecretConfigured ? "（已配置，留空保持不变）" : ""}</span><input type="password" value={notificationDraft.signingSecret} onChange={(event) => updateNotification("signingSecret", event.target.value)} placeholder={notificationDraft.provider === "dingtalk" ? "钉钉加签密钥（可选）" : "可选"} />{notification.data?.signingSecretConfigured && <Button size="sm" variant="ghost" onClick={() => updateNotification("clearSigningSecret", !notificationDraft.clearSigningSecret)}>{notificationDraft.clearSigningSecret ? "撤销清除签名" : "清除已保存签名"}</Button>}</label></>}</div>{saveNotification.error && <div className="form-error">{errorMessage(saveNotification.error)}</div>}<div className="dialog-actions"><Button variant="primary" onClick={() => saveNotification.mutate()} disabled={saveNotification.isPending || (notificationDraft.notifyWebhook && !notificationDraft.webhookUrl.trim() && !notification.data?.webhookConfigured && !notificationDraft.clearWebhook)}><Bell size={14} />{saveNotification.isPending ? "保存中…" : "保存通知设置"}</Button></div></section>}
    {cronjobs.isLoading && <div className="page-state">正在读取远端计划任务…</div>}
    {cronjobs.error && <div className="page-state page-state--error">{errorMessage(cronjobs.error)}</div>}
    <section className="cron-create-card">
      <header><div><span className="section-kicker">新建任务</span><h2>添加 crontab 任务</h2><p>当前用户：{cronjobs.data?.user ?? "读取中…"}</p></div><Clock3 size={22} /></header>
      <div className="field-grid field-grid--3">
        <label><span>执行计划</span><input value={form.schedule} onChange={(event) => update("schedule", event.target.value)} placeholder="0 2 * * *" /><small>五段 cron 表达式，例如每天 02:00。</small></label>
        <label><span>任务类型</span><select value={form.kind} onChange={(event) => update("kind", event.target.value as CronKind)}><option value="shell">Shell 命令</option><option value="url">URL 请求</option><option value="directory">目录备份</option><option value="database">数据库备份</option><option value="log">日志备份</option><option value="website">网站备份</option><option value="app">应用备份</option></select></label>
        {form.kind === "shell" && <label className="field-grid__wide"><span>命令</span><input value={form.command} onChange={(event) => update("command", event.target.value)} placeholder="/usr/local/bin/backup.sh" /></label>}
        {form.kind === "url" && <label className="field-grid__wide"><span>URL 地址（每行一个，最多 20 个）</span><textarea value={form.urls} onChange={(event) => update("urls", event.target.value)} placeholder="https://example.com/health\nhttps://example.com/ping" rows={3} /><small>仅允许 HTTP/HTTPS；客户端固定使用 curl 请求，不支持在 URL 中放认证信息。</small></label>}
        {form.kind === "directory" && <><label className="field-grid__wide"><span>源路径（每行一个）</span><textarea value={form.sourcePaths} onChange={(event) => update("sourcePaths", event.target.value)} placeholder="/var/www/site\n/etc/nginx" rows={3} /><small>目录或文件均可，最多 32 个远端绝对路径。</small></label><label><span>目标归档文件</span><input value={form.destination} onChange={(event) => update("destination", event.target.value)} placeholder="/var/backups/site.tar.gz" /></label><label><span>排除路径（可选）</span><textarea value={form.excludePaths} onChange={(event) => update("excludePaths", event.target.value)} placeholder="/var/www/site/cache" rows={3} /></label></>}
        {form.kind === "database" && <><label><span>数据库引擎</span><select value={form.databaseEngine} onChange={(event) => update("databaseEngine", event.target.value as CronForm["databaseEngine"])}><option value="mysql">MySQL</option><option value="mariadb">MariaDB</option><option value="postgresql">PostgreSQL</option></select></label><label><span>数据库名称</span><input value={form.databaseName} onChange={(event) => update("databaseName", event.target.value)} placeholder="app_db" /></label><label><span>目标 SQL 文件</span><input value={form.destination} onChange={(event) => update("destination", event.target.value)} placeholder="/var/backups/app.sql" /></label></>}
        {form.kind === "log" && <label className="field-grid__wide"><span>目标日志归档文件</span><input value={form.destination} onChange={(event) => update("destination", event.target.value)} placeholder="/var/backups/logs.tar.gz" /><small>固定归档远端 /var/log，不执行用户自定义 Shell。</small></label>}
        {form.kind === "website" && <><label className="field-grid__wide"><span>受控网站</span><select value={form.websiteDomain} onChange={(event) => update("websiteDomain", event.target.value)}><option value="">选择网站</option>{websites.data?.websites.map((website) => <option value={website.domain} key={website.domain}>{website.domain} · {website.kind === "static" ? "静态" : "反向代理"}{website.rootPath ? ` · ${website.rootPath}` : ""}</option>)}</select><small>{websites.isLoading ? "正在读取远端网站…" : websites.error ? errorMessage(websites.error) : "服务端会再次按域名查找真实受控配置；静态站点同时归档根目录和配置。"}</small></label><label><span>目标归档文件</span><input value={form.destination} onChange={(event) => update("destination", event.target.value)} placeholder="/var/backups/example-site.tar.gz" /></label><label><span>排除路径（可选）</span><textarea value={form.excludePaths} onChange={(event) => update("excludePaths", event.target.value)} placeholder="/var/www/example/cache" rows={3} /></label></>}
        {form.kind === "app" && <><label className="field-grid__wide"><span>已安装应用</span><select value={form.appInstallPath} onChange={(event) => update("appInstallPath", event.target.value)}><option value="">选择应用</option>{installedApps.data?.apps.map((app) => <option value={app.path} key={app.composePath}>{app.key} · {app.path} · {app.status || "状态未知"}</option>)}</select><small>{installedApps.isLoading ? "正在读取远端已安装应用…" : installedApps.error ? errorMessage(installedApps.error) : "服务端会再次按固定 /opt/1panel/apps 快照校验；归档包含 Compose、环境文件和应用目录内容。"}</small></label><label><span>目标归档文件</span><input value={form.destination} onChange={(event) => update("destination", event.target.value)} placeholder="/var/backups/app.tar.gz" /></label><label><span>排除路径（可选）</span><textarea value={form.excludePaths} onChange={(event) => update("excludePaths", event.target.value)} placeholder="/opt/1panel/apps/demo/1.0/cache" rows={3} /></label></>}
        {["directory", "database", "log", "website", "app"].includes(form.kind) && <><label><span>保留份数（可选）</span><input type="number" min={1} max={1000} value={form.retentionCount} onChange={(event) => update("retentionCount", event.target.value)} placeholder="不启用" /><small>启用后按 UTC 时间戳生成轮换归档。</small></label><label><span>保留天数（可选）</span><input type="number" min={1} max={3650} value={form.retentionDays} onChange={(event) => update("retentionDays", event.target.value)} placeholder="不启用" /><small>只清理客户端生成的同前缀归档。</small></label><div className="security-note field-grid__wide"><Archive size={18} /><span>保留策略只作用于当前目标目录中由本任务生成的轮换文件，不会删除其它文件；服务端会再次校验目标文件名和范围。</span></div></>}
        {["directory", "database", "log", "website", "app"].includes(form.kind) && <div className="cron-backup-account-picker field-grid__wide"><span>归档上传账号（可选）</span>{backupAccounts.isLoading && <small>正在读取备份账号…</small>}{backupAccounts.data?.filter((account) => !account.serverId || account.serverId === serverId).map((account) => <label className="check-field" key={account.id}><input type="checkbox" checked={form.backupAccountIds.includes(account.id)} onChange={(event) => update("backupAccountIds", event.target.checked ? [...form.backupAccountIds, account.id] : form.backupAccountIds.filter((id) => id !== account.id))} /><span>{account.name} · {account.kind === "local" ? "本机目录" : account.kind === "s3" ? "S3" : account.kind === "webdav" ? "WebDAV" : "SFTP"}{account.hasSecret || account.kind === "local" ? "" : "（缺少 secret）"}</span></label>)}{!backupAccounts.isLoading && !backupAccounts.data?.length && <small>暂无账号，请先在“设置 → 备份账号”中添加。</small>}<small>手动运行成功后，客户端会从远端读取生成的归档并上传到所选账号；凭据不会写入 crontab。</small></div>}
        <label><span>目标用户（可选）</span><input value={form.user} onChange={(event) => update("user", event.target.value)} placeholder="当前 SSH 用户" /></label>
      </div>
      <div className="security-note"><ShieldAlert size={18} /><span>任务会写入远端 crontab；客户端会添加 marker，后续只允许删除自己创建的任务。网站和应用备份会在保存时实时校验远端对象，备份任务使用同目录临时文件并在成功后原子替换。</span></div>
      {save.error && <div className="form-error">{errorMessage(save.error)}</div>}
      <div className="dialog-actions"><Button variant="primary" onClick={submit} disabled={!canSubmitCronForm(form) || save.isPending}><Plus size={14} />{save.isPending ? "保存中…" : "添加任务"}</Button></div>
    </section>
    {cronjobs.data && <><section className="cron-section"><header><div><span className="section-kicker">Crontab</span><h2>计划任务</h2></div><span>{cronjobs.data.jobs.length} 条</span></header>{cronjobs.data.jobs.length ? <div className="cron-table"><div className="ops-head"><span>计划</span><span>类型 / 命令</span><span>用户</span><span>来源</span><span>操作</span></div>{cronjobs.data.jobs.map((job) => { const retention = cronRetentionLabel(job); const uploads = cronBackupAccountLabel(job); return <div className="ops-row" key={job.id}><span className="mono">{job.schedule}</span><span><span className="status-chip">{cronKindLabel(job.kind)}</span><strong className="cron-command">{job.command}</strong>{retention && <small className="cron-retention"><Archive size={12} />{retention}</small>}{uploads && <small className="cron-retention">{uploads}</small>}</span><span>{job.user}</span><span className={job.managed ? "text-ok" : "text-muted"}>{job.managed ? "Client" : "系统任务"}</span><span className="database-row-actions"><Button size="sm" variant="ghost" onClick={() => run(job)} disabled={action.isPending}><Play size={13} />运行</Button><Button size="sm" variant="danger" onClick={() => remove(job)} disabled={!job.managed || action.isPending}><Trash2 size={13} />删除</Button></span></div>; })}</div> : <div className="empty-panel empty-panel--small"><Clock3 size={20} /><span>没有发现 crontab 任务。</span></div>}{action.error && <div className="form-error">{errorMessage(action.error)}</div>}</section><section className="cron-section"><header><div><span className="section-kicker">Systemd</span><h2>Timers</h2></div><span>{cronjobs.data.timers.length} 条</span></header>{cronjobs.data.timers.length ? <div className="cron-table"><div className="ops-head"><span>Timer</span><span>下次运行</span><span>上次运行</span><span>激活服务</span><span /></div>{cronjobs.data.timers.map((timer) => <div className="ops-row" key={timer.name}><strong className="mono">{timer.name}</strong><span>{timer.nextRun}</span><span>{timer.lastRun}</span><span>{timer.activates}</span><span /></div>)}</div> : <div className="empty-panel empty-panel--small"><Clock3 size={20} /><span>没有发现 systemd timer。</span></div>}</section></>}
    {cronjobs.data && <CronHistoryPanel entries={history.data ?? []} onClear={clearLocalHistory} clearDisabled={clearHistory.isPending || !history.data?.length} />}
  </section>;
}

/** 展示本机保存的最近计划任务执行摘要，不渲染未脱敏的远端原始输出。 */
function CronHistoryPanel({ entries, onClear, clearDisabled }: { entries: CronJobHistoryEntry[]; onClear: () => void; clearDisabled: boolean }) {
  return <section className="cron-section"><header><div><span className="section-kicker">Execution history</span><h2>执行历史</h2><p>只保留最近 200 次运行摘要，输出已脱敏并截断。</p></div><div className="workspace-header__actions"><Button size="sm" variant="ghost" onClick={onClear} disabled={clearDisabled}><Trash2 size={13} />清除历史</Button><History size={20} /></div></header>{entries.length ? <div className="cron-history-list">{entries.slice(0, 100).map((entry) => <div className="cron-history-row" key={entry.id}><span className={`status-chip ${entry.success ? "status-chip--ok" : "status-chip--danger"}`}>{entry.success ? "成功" : "失败"}</span><span className="mono">{new Date(entry.finishedAt).toLocaleString()}</span><span>{entry.action === "run" ? "立即运行" : entry.action}</span><code>{entry.output || "（无输出）"}</code></div>)}</div> : <div className="empty-panel empty-panel--small"><History size={20} /><span>还没有立即运行记录。</span></div>}</section>;
}

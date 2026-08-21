import { Channel, invoke } from "@tauri-apps/api/core";
import type {
  ConnectionSnapshot,
  HostKeyChallenge,
  SaveServerInput,
  ServerProfile,
  ServerGroup,
  SystemOverview,
  OverviewMemo,
  DirectoryListing,
  RemoteTextFile,
  RemoteBinaryPreview,
  OperationsSnapshot,
  TerminationResult,
  StorageSnapshot,
  StorageActionResult,
  ToolInstallPlan,
  ToolInstallResult,
  ToolStatus,
  NginxSnapshot,
  WebsiteSnapshot,
  CertificateActionResult,
  PhpInstallPlan,
  PhpInstallResult,
  DockerActionResult,
  DockerLogs,
  DockerSnapshot,
  DockerTextResult,
  DockerEventsResult,
  DockerPullResult,
  DockerBuildResult,
  DockerRunResult,
  DatabaseSnapshot,
  DatabaseActionResult,
  DatabasePrivilegeSnapshot,
  DatabaseInstallPlan,
  DatabaseInstallResult,
  RedisSnapshot,
  RedisTransferResult,
  RedisMigrationResult,
  RedisValueResult,
  RedisComplexActionResult,
  CronSnapshot,
  CronJobExport,
  CronJobActionResult,
  CronJobHistoryEntry,
  CronJobImportResult,
  CronNotificationSettings,
  CronOfflineSchedulerSettings,
  BackupAccount,
  BackupAccountTestResult,
  BackupUploadResult,
  SecuritySnapshot,
  FirewallSnapshot,
  SshSecurityConfig,
  AdvancedSnapshot,
  HttpMonitorResult,
  HttpMonitorProfile,
  HttpMonitorCheck,
  WafRulesSnapshot,
  WafRuleSourcesSnapshot,
  WafRuleSourceActionResult,
  WafTemplate,
  WafAlertsSnapshot,
  WafAlertSettings,
  AppCatalogSnapshot,
  AppDetail,
  InstalledAppsSnapshot,
  AppHealthSnapshot,
  AppUpdatePreview,
  AppEnvironmentSnapshot,
  AppActionResult,
  AppStoreSettings,
  AppStoreMirrorGenerationResult,
  AiProvider,
  AiModel,
  AiChatResult,
  AiConversation,
  AiAgentResult,
  AiStreamEvent,
  McpServerConfig,
  McpProbeResult,
  DockerResourceActionResult,
  DockerComposeDetails,
  PublicServerExport,
  PublicServerImport,
  AuditEvent,
  DiagnosticsExport,
  ServiceDetail,
  ServiceLogs,
  LogQuery,
  LogSnapshot,
  ShortcutRecord,
  SaveShortcutInput,
  MetricSample,
  SaveTaskInput,
  TaskRecord,
} from "../types/server";

export const api = {
  listServers: () => invoke<ServerProfile[]>("list_servers"),
  listShortcuts: (serverId?: string) => invoke<ShortcutRecord[]>("list_shortcuts", { serverId }),
  saveShortcut: (input: SaveShortcutInput) => invoke<ShortcutRecord>("save_shortcut", { input }),
  deleteShortcut: (id: string) => invoke<void>("delete_shortcut", { id }),
  restoreDefaultShortcuts: () => invoke<void>("restore_default_shortcuts"),
  useShortcut: (id: string) => invoke<void>("use_shortcut", { id }),
  getServer: (serverId: string) => invoke<ServerProfile>("get_server", { serverId }),
  listServerGroups: () => invoke<ServerGroup[]>("list_server_groups"),
  createServerGroup: (name: string) => invoke<ServerGroup>("create_server_group", { name }),
  saveServer: (input: SaveServerInput) => invoke<ServerProfile>("save_server", { input }),
  duplicateServer: (serverId: string) => invoke<ServerProfile>("duplicate_server", { serverId }),
  deleteServer: (serverId: string) => invoke<void>("delete_server", { serverId }),
  connectionState: (serverId: string) =>
    invoke<ConnectionSnapshot>("connection_state", { serverId }),
  connectServer: (serverId: string) =>
    invoke<ConnectionSnapshot | HostKeyChallenge>("connect_server", { serverId }),
  reconnectServer: (serverId: string) =>
    invoke<ConnectionSnapshot | HostKeyChallenge>("reconnect_server", { serverId }),
  trustHostKey: (challenge: HostKeyChallenge) =>
    invoke<ConnectionSnapshot>("trust_host_key", { challenge }),
  disconnectServer: (serverId: string) => invoke<void>("disconnect_server", { serverId }),
  overview: (serverId: string) => invoke<SystemOverview>("get_system_overview", { serverId }),
  overviewMemo: (serverId: string) => invoke<OverviewMemo>("get_overview_memo", { serverId }),
  saveOverviewMemo: (input: { serverId: string; content: string }) => invoke<OverviewMemo>("save_overview_memo", { input }),
  metricHistory: (serverId: string, since: string) => invoke<MetricSample[]>("get_metric_history", { serverId, since }),
  saveTask: (input: SaveTaskInput) => invoke<TaskRecord>("save_task", { input }),
  listTasks: () => invoke<TaskRecord[]>("list_tasks"),
  clearFinishedTasks: () => invoke<void>("clear_finished_tasks"),
  openTerminal: (
    serverId: string,
    columns: number,
    rows: number,
    onEvent: (event: TerminalEvent) => void,
  ) => {
    const channel = new Channel<TerminalEvent>();
    channel.onmessage = onEvent;
    return invoke<string>("open_terminal", { serverId, columns, rows, onEvent: channel });
  },
  writeTerminal: (terminalId: string, data: Uint8Array) =>
    invoke<void>("write_terminal", { terminalId, data: Array.from(data) }),
  resizeTerminal: (terminalId: string, columns: number, rows: number) =>
    invoke<void>("resize_terminal", { terminalId, columns, rows }),
  closeTerminal: (terminalId: string) => invoke<void>("close_terminal", { terminalId }),
  listDirectory: (serverId: string, path: string) =>
    invoke<DirectoryListing>("list_remote_directory", { serverId, path }),
  readText: (serverId: string, path: string) =>
    invoke<RemoteTextFile>("read_remote_text", { serverId, path }),
  readImagePreview: (serverId: string, path: string) =>
    invoke<RemoteBinaryPreview>("read_remote_image_preview", { serverId, path }),
  readTail: (serverId: string, path: string, lines = 5000) =>
    invoke<RemoteTextFile>("read_remote_tail", { serverId, path, lines }),
  saveText: (input: {
    serverId: string; path: string; content: string; expectedSize: number;
    expectedModifiedAt: number | null; force?: boolean;
  }) => invoke<RemoteTextFile>("save_remote_text", { input }),
  saveTextPrivileged: (input: {
    serverId: string; path: string; content: string; expectedSize: number;
    expectedModifiedAt: number | null; force?: boolean;
  }) => invoke<RemoteTextFile>("save_remote_text_privileged", { input }),
  createEntry: (serverId: string, path: string, directory: boolean) =>
    invoke<void>("create_remote_entry", { serverId, path, directory }),
  renameEntry: (serverId: string, oldPath: string, newPath: string) =>
    invoke<void>("rename_remote_entry", { serverId, oldPath, newPath }),
  removeEntry: (serverId: string, path: string, recursive: boolean) =>
    invoke<void>("remove_remote_entry", { serverId, path, recursive }),
  chmod: (input: { serverId: string; path: string; mode: number }) =>
    invoke<void>("chmod_remote", { input }),
  createSymlink: (input: { serverId: string; targetPath: string; linkPath: string }) =>
    invoke<void>("create_remote_symlink", { input }),
  copyMove: (input: { serverId: string; sourcePath: string; destinationPath: string; operation: "copy" | "move"; recursive: boolean; confirmed: boolean }) =>
    invoke<void>("copy_move_remote", { input }),
  upload: (transferId: string, serverId: string, localPath: string, remoteDirectory: string, conflict: "replace" | "skip" | "rename", onEvent: (event: TransferEvent) => void) => {
    const channel = new Channel<TransferEvent>();
    channel.onmessage = onEvent;
    return invoke<void>("upload_remote", { transferId, serverId, localPath, remoteDirectory, conflict, onEvent: channel });
  },
  download: (transferId: string, serverId: string, remotePath: string, localDirectory: string, onEvent: (event: TransferEvent) => void) => {
    const channel = new Channel<TransferEvent>();
    channel.onmessage = onEvent;
    return invoke<void>("download_remote", { transferId, serverId, remotePath, localDirectory, onEvent: channel });
  },
  cancelTransfer: (transferId: string) => invoke<void>("cancel_transfer", { transferId }),
  operations: (serverId: string, privileged = false) => invoke<OperationsSnapshot>("get_operations", { serverId, privileged }),
  terminateProcess: (input: { serverId: string; pid: number; port?: number; force?: boolean; privileged?: boolean }) =>
    invoke<TerminationResult>("terminate_process", { input }),
  manageService: (serverId: string, service: string, action: "start" | "stop" | "restart" | "enable" | "disable") =>
    invoke<void>("manage_service", { serverId, service, action }),
  serviceDetail: (serverId: string, service: string) => invoke<ServiceDetail>("get_service_detail", { serverId, service }),
  serviceLogs: (serverId: string, service: string, lines = 200) => invoke<ServiceLogs>("get_service_logs", { serverId, service, lines }),
  storage: (serverId: string) => invoke<StorageSnapshot>("get_storage", { serverId }),
  storageAction: (input: { serverId: string; action: "mount" | "unmount" | "add_fstab" | "remove_fstab"; source?: string; mountpoint: string; filesystem?: string; options?: string; dump?: string; pass?: string; confirmed: boolean }) =>
    invoke<StorageActionResult>("storage_action", { input }),
  getLogs: (query: LogQuery) => invoke<LogSnapshot>("get_logs", { query }),
  followLogs: (query: LogQuery, taskId: string, onEvent: (event: CommandEvent) => void) => {
    const channel = new Channel<CommandEvent>();
    channel.onmessage = onEvent;
    return invoke<LogSnapshot>("follow_logs", { query, taskId, onEvent: channel });
  },
  // Server config import/export intentionally has no credential payload.
  exportServers: () => invoke<PublicServerExport>("export_servers"),
  importServers: (values: PublicServerImport[]) => invoke<ServerProfile[]>("import_servers", { values }),
  exportDiagnostics: () => invoke<DiagnosticsExport>("export_diagnostics"),
  listAuditEvents: (limit = 50) => invoke<AuditEvent[]>("list_audit_events", { limit }),
  exportFullBackup: (password: string) => invoke<string>("export_full_backup", { input: { password } }),
  importFullBackup: (backup: string, password: string) => invoke<ServerProfile[]>("import_full_backup", { input: { backup, password } }),
  backupAccounts: () => invoke<BackupAccount[]>("list_backup_accounts"),
  saveBackupAccount: (input: { id?: string; name: string; kind: "local" | "webdav" | "s3" | "sftp"; serverId?: string; endpoint?: string; remotePath: string; bucket?: string; region?: string; username?: string; privateKeyPath?: string; hostKeyFingerprint?: string; secret?: string; clearSecret: boolean; confirmed: boolean }) => invoke<BackupAccount>("save_backup_account", { input }),
  deleteBackupAccount: (id: string, confirmed: boolean) => invoke<void>("delete_backup_account", { id, confirmed }),
  testBackupAccount: (id: string) => invoke<BackupAccountTestResult>("test_backup_account", { id }),
  uploadBackupArtifact: (input: { serverId: string; accountId: string; remotePath: string; confirmed: boolean }) => invoke<BackupUploadResult>("upload_backup_artifact", { input }),
  listTools: (serverId: string) => invoke<ToolStatus[]>("list_tools", { serverId }),
  toolInstallPlan: (serverId: string, toolId: string) =>
    invoke<ToolInstallPlan>("get_tool_install_plan", { serverId, toolId }),
  /** 通过可取消 task id 流式执行用户确认的工具安装。 */
  installTool: (input: { serverId: string; toolId: string; taskId: string }, onEvent: (event: CommandEvent) => void) => {
    const channel = new Channel<CommandEvent>();
    channel.onmessage = onEvent;
    return invoke<ToolInstallResult>("install_tool", { input, onEvent: channel });
  },
  nginx: (serverId: string) => invoke<NginxSnapshot>("get_nginx", { serverId }),
  testNginx: (serverId: string) => invoke<boolean>("test_nginx_config", { serverId }),
  probeNginxBackend: (input: { serverId: string; scheme: "http" | "https"; targetHost: string; targetPort: number }) =>
    invoke<{ reachable: boolean; statusCode: number | null; latencyMs: number | null; detail: string }>("probe_nginx_backend", { input }),
  saveNginxProxy: (input: {
    serverId: string; name: string; serverName: string; listenPort: number; enableHttps: boolean; httpsPort: number; certificatePath?: string; certificateKeyPath?: string;
    location: string; upstreamScheme: "http" | "https"; targetHost: string;
    targetPort: number; websocket: boolean; preserveHost: boolean;
  }) => invoke<NginxSnapshot>("save_nginx_proxy", { input }),
  websites: (serverId: string) => invoke<WebsiteSnapshot>("get_websites", { serverId }),
  saveWebsite: (input: { serverId: string; domain: string; kind: "static" | "proxy"; listenPort: number; rootPath?: string; phpRuntime?: string; upstreamScheme?: "http" | "https"; upstreamHost?: string; upstreamPort?: number; enableHttps: boolean; httpsPort: number; certificatePath?: string; certificateKeyPath?: string; confirmed: boolean }) =>
    invoke<WebsiteSnapshot>("save_website", { input }),
  websiteAction: (input: { serverId: string; domain: string; action: "enable" | "disable" | "delete"; confirmed: boolean }) =>
    invoke<WebsiteSnapshot>("website_action", { input }),
  certificateAction: (input: { serverId: string; domain: string; email: string; webroot: string; action: "issue" | "renew"; challenge?: "http01" | "dns01"; dnsProvider?: "cloudflare" | "aliyun" | "dnspod" | "tencent" | "aws"; dnsApiToken?: string; confirmed: boolean }) =>
    invoke<CertificateActionResult>("website_certificate_action", { input }),
  bindWebsiteCertificate: (input: { serverId: string; domain: string; certificatePath: string; certificateKeyPath: string; confirmed: boolean }) =>
    invoke<WebsiteSnapshot>("bind_website_certificate", { input }),
  phpInstallPlan: (serverId: string) => invoke<PhpInstallPlan>("get_php_install_plan", { serverId }),
  installPhp: (input: { serverId: string; confirmed: boolean }) =>
    invoke<PhpInstallResult>("install_php_runtime", { input }),
  advanced: (serverId: string) => invoke<AdvancedSnapshot>("get_advanced", { serverId }),
  probeHttpMonitor: (input: { serverId: string; url: string; expectedStatus?: number }) =>
    invoke<HttpMonitorResult>("probe_http_monitor", { input }),
  httpMonitors: (serverId: string) => invoke<HttpMonitorProfile[]>("get_http_monitors", { serverId }),
  saveHttpMonitor: (input: { id?: string; serverId: string; name: string; url: string; expectedStatus?: number; intervalSeconds: number; enabled: boolean }) =>
    invoke<HttpMonitorProfile>("save_http_monitor", { input }),
  deleteHttpMonitor: (monitorId: string) => invoke<void>("delete_http_monitor", { monitorId }),
  runHttpMonitor: (monitorId: string) => invoke<HttpMonitorResult>("run_http_monitor", { monitorId }),
  httpMonitorHistory: (monitorId: string) => invoke<HttpMonitorCheck[]>("get_http_monitor_history", { monitorId }),
  wafRules: (serverId: string) => invoke<WafRulesSnapshot>("get_waf_rules", { serverId }),
  wafRuleSources: (serverId: string) => invoke<WafRuleSourcesSnapshot>("get_waf_rule_sources", { serverId }),
  wafTemplates: () => invoke<WafTemplate[]>("get_waf_templates"),
  wafAlerts: (serverId: string) => invoke<WafAlertsSnapshot>("get_waf_alerts", { serverId }),
  wafAlertSettings: (serverId: string) => invoke<WafAlertSettings>("get_waf_alert_settings", { serverId }),
  saveWafAlertSettings: (serverId: string, input: { minSeverity: "warning" | "error" | "critical"; notifyInApp: boolean; historyLimit: number; notifyWebhook: boolean; notifyProvider: WafAlertSettings["notifyProvider"]; webhookUrl?: string; webhookSigningSecret?: string; clearWebhook?: boolean }) => invoke<WafAlertSettings>("save_waf_alert_settings", { serverId, input }),
  clearWafAlertHistory: (serverId: string) => invoke<void>("clear_waf_alert_history", { serverId }),
  wafRuleAction: (input: { serverId: string; action: "add" | "delete"; lineNumber?: number; rule?: string; confirmed: boolean }) =>
    invoke<WafRulesSnapshot>("waf_rule_action", { input }),
  wafTemplateAction: (input: { serverId: string; templateId: string; confirmed: boolean }) =>
    invoke<WafRulesSnapshot>("waf_template_action", { input }),
  wafRuleSourceAction: (input: { serverId: string; sourceId: string; action: "install" | "update" | "remove"; confirmed: boolean }) =>
    invoke<WafRuleSourceActionResult>("waf_rule_source_action", { input }),
  docker: (serverId: string, privileged = false) => invoke<DockerSnapshot>("get_docker", { serverId, privileged }),
  /** 读取最近 Docker daemon 事件的有界摘要，不保存原始 actor 属性。 */
  dockerEvents: (serverId: string, sinceSeconds = 300, privileged = false) =>
    invoke<DockerEventsResult>("get_docker_events", { serverId, sinceSeconds, privileged }),
  dockerContainerAction: (input: { serverId: string; containerId: string; action: string; newName?: string; force?: boolean; sudo?: boolean; confirmed?: boolean }) =>
    invoke<DockerActionResult>("docker_container_action", { input }),
  dockerContainerLogs: (serverId: string, containerId: string, tail = 200, privileged = false) =>
    invoke<DockerLogs>("docker_container_logs", { serverId, containerId, tail, privileged }),
  dockerContainerInspect: (serverId: string, containerId: string, privileged = false) =>
    invoke<DockerTextResult>("docker_container_inspect", { serverId, containerId, privileged }),
  dockerContainerStats: (serverId: string, containerId: string, privileged = false) =>
    invoke<DockerTextResult>("docker_container_stats", { serverId, containerId, privileged }),
  dockerContainerTop: (serverId: string, containerId: string, privileged = false) =>
    invoke<DockerTextResult>("docker_container_top", { serverId, containerId, privileged }),
  dockerContainerExec: (input: { serverId: string; containerId: string; command: string; sudo?: boolean }) =>
    invoke<DockerTextResult>("docker_container_exec", { input }),
  /** 通过可取消 task id 读取容器 follow 日志。 */
  dockerContainerFollowLogs: (serverId: string, containerId: string, tail: number, sudo: boolean, taskId: string, onEvent: (event: CommandEvent) => void) => {
    const channel = new Channel<CommandEvent>();
    channel.onmessage = onEvent;
    return invoke<DockerLogs>("docker_container_follow_logs", { serverId, containerId, tail, sudo, taskId, onEvent: channel });
  },
  dockerResourceAction: (input: { serverId: string; kind: "volume" | "network"; name: string; action: "create" | "remove"; sudo?: boolean; confirmed?: boolean }) =>
    invoke<DockerResourceActionResult>("docker_resource_action", { input }),
  dockerImageAction: (input: { serverId: string; image: string; action: "remove"; force?: boolean; sudo?: boolean; confirmed?: boolean }) =>
    invoke<DockerResourceActionResult>("docker_image_action", { input }),
  dockerResourceInspect: (input: { serverId: string; kind: "volume" | "network"; name: string; sudo?: boolean }) =>
    invoke<DockerTextResult>("docker_resource_inspect", { input }),
  /** 执行 Compose 生命周期或显式 cleanup，并由 Rust 校验 destructive confirmation。 */
  dockerComposeAction: (input: { serverId: string; project: string; workingDir?: string; action: "up" | "down" | "start" | "stop" | "restart" | "pull" | "build" | "cleanup"; sudo?: boolean; confirmed?: boolean }) =>
    invoke<DockerResourceActionResult>("docker_compose_action", { input }),
  /** 保存 Compose 原始 YAML；Rust 端会先 config -q，失败自动恢复。 */
  dockerComposeSaveYaml: (input: { serverId: string; project: string; workingDir?: string; configPath: string; content: string; expectedSize: number; expectedModifiedAt: number | null; force?: boolean; sudo?: boolean; confirmed: boolean }) =>
    invoke<RemoteTextFile>("docker_compose_save_yaml", { input }),
  /** 读取 Compose 服务、脱敏渲染配置和资源候选。 */
  dockerComposeDetails: (serverId: string, project: string, workingDir: string | undefined, sudo = false) =>
    invoke<DockerComposeDetails>("docker_compose_details", { serverId, project, workingDir, sudo }),
  /** 读取 Compose 项目或服务的最近日志。 */
  dockerComposeLogs: (serverId: string, project: string, workingDir: string | undefined, service: string | undefined, tail = 200, sudo = false) =>
    invoke<DockerLogs>("docker_compose_logs", { serverId, project, workingDir, service, tail, sudo }),
  /** 通过可取消 task id 流式拉取镜像层输出。 */
  dockerPullImage: (input: { serverId: string; image: string; taskId: string; sudo?: boolean }, onEvent: (event: CommandEvent) => void) => {
    const channel = new Channel<CommandEvent>();
    channel.onmessage = onEvent;
    return invoke<DockerPullResult>("docker_pull_image", { input, onEvent: channel });
  },
  /** 通过可取消 task id 流式构建远端 Docker 镜像。 */
  dockerBuildImage: (input: { serverId: string; contextPath: string; dockerfilePath?: string; image: string; buildArgs: string[]; taskId: string; sudo?: boolean }, onEvent: (event: CommandEvent) => void) => {
    const channel = new Channel<CommandEvent>();
    channel.onmessage = onEvent;
    return invoke<DockerBuildResult>("docker_build_image", { input, onEvent: channel });
  },
  /** 请求 Rust 关闭指定流式 SSH 命令的远端 channel。 */
  cancelCommandTask: (taskId: string) => invoke<void>("cancel_command_task", { taskId }),
  /** 调用受控 Run 向导；API 层固定带上用户已完成表单确认。 */
  dockerRunContainer: (input: { serverId: string; image: string; name?: string; ports: string[]; environment: string[]; network?: string; restartPolicy?: string; autoRemove: boolean; privileged: boolean; sudo?: boolean; confirmed?: boolean }) =>
    invoke<DockerRunResult>("docker_run_container", { input: { ...input, confirmed: true } }),
  database: (serverId: string) => invoke<DatabaseSnapshot>("get_databases", { serverId }),
  databaseAction: (input: { serverId: string; engine: string; name: string; action: "create" | "drop"; confirmed: boolean }) =>
    invoke<DatabaseActionResult>("database_action", { input }),
  databaseBackup: (input: { serverId: string; engine: string; name: string; destination: string; confirmed: boolean }) =>
    invoke<DatabaseActionResult>("backup_database", { input }),
  databaseRestore: (input: { serverId: string; engine: string; name: string; source: string; confirmed: boolean }) =>
    invoke<DatabaseActionResult>("restore_database", { input }),
  databaseUserAction: (input: { serverId: string; engine: string; username: string; host?: string; database?: string; privileges?: string; password?: string; action: "create" | "drop" | "grant" | "revoke" | "reset_password"; confirmed: boolean }) =>
    invoke<DatabaseActionResult>("database_user_action", { input }),
  databasePrivileges: (input: { serverId: string; engine: string; username: string; host?: string; redisUsername?: string; redisPassword?: string }) =>
    invoke<DatabasePrivilegeSnapshot>("get_database_privileges", { input }),
  databaseEngineAction: (input: { serverId: string; engine: string; action: "start" | "stop" | "restart"; confirmed: boolean }) =>
    invoke<DatabaseActionResult>("database_engine_action", { input }),
  databaseInstallPlan: (serverId: string, engine: string) =>
    invoke<DatabaseInstallPlan>("get_database_install_plan", { serverId, engine }),
  /** 通过可取消 task id 流式安装数据库引擎。 */
  installDatabaseEngine: (input: { serverId: string; engine: string; taskId: string; confirmed: boolean }, onEvent: (event: CommandEvent) => void) => {
    const channel = new Channel<CommandEvent>();
    channel.onmessage = onEvent;
    return invoke<DatabaseInstallResult>("install_database_engine", { input, onEvent: channel });
  },
  redisData: (input: { serverId: string; database: number; pattern?: string; limit?: number; username?: string; password?: string }) =>
    invoke<RedisSnapshot>("get_redis_data", { input }),
  redisDataAction: (input: { serverId: string; database: number; action: "delete" | "flushdb"; key?: string; confirmed: boolean; username?: string; password?: string }) =>
    invoke<{ database: number; action: string; key: string | null; output: string }>("redis_data_action", { input }),
  redisValueAction: (input: { serverId: string; database: number; key: string; action: "get" | "set"; value?: string; ttlSeconds?: number; confirmed: boolean; username?: string; password?: string }) =>
    invoke<RedisValueResult>("redis_value_action", { input }),
  redisComplexAction: (input: { serverId: string; database: number; key: string; kind: "hash" | "list" | "set" | "zset"; action: string; field?: string; value?: string; score?: number; confirmed: boolean; username?: string; password?: string }) =>
    invoke<RedisComplexActionResult>("redis_complex_action", { input }),
  redisTransfer: (input: { serverId: string; database: number; action: "export" | "import"; path: string; maxKeys?: number; confirmed: boolean; username?: string; password?: string }) =>
    invoke<RedisTransferResult>("redis_transfer_action", { input }),
  /** 在源服务器上用 Redis MIGRATE 迁移键，保留类型和 TTL。 */
  redisMigration: (input: { sourceServerId: string; sourceDatabase: number; sourceUsername?: string; sourcePassword?: string; targetHost: string; targetPort: number; targetDatabase: number; targetUsername?: string; targetPassword?: string; maxKeys?: number; confirmed: boolean }) =>
    invoke<RedisMigrationResult>("redis_migration_action", { input }),
  cronjobs: (serverId: string) => invoke<CronSnapshot>("get_cronjobs", { serverId }),
  cronjobExport: (serverId: string) => invoke<CronJobExport>("export_cronjobs", { serverId }),
  cronjobImport: (input: { serverId: string; jobs: CronJobExport["jobs"]; confirmed: boolean }) =>
    invoke<CronJobImportResult>("import_cronjobs", { input: { serverId: input.serverId, jobs: input.jobs, confirmed: input.confirmed } }),
  saveCronjob: (input: { serverId: string; id?: string; schedule: string; command: string; kind?: "shell" | "url" | "directory" | "database" | "log" | "website" | "app"; urls?: string[]; sourcePaths?: string[]; destination?: string; databaseEngine?: "mysql" | "mariadb" | "postgresql"; databaseName?: string; excludePaths?: string[]; websiteDomain?: string; appInstallPath?: string; retentionCount?: number; retentionDays?: number; backupAccountIds?: string[]; defaultBackupAccountId?: string; user?: string; enabled: boolean; confirmed: boolean }) =>
    invoke<CronJobActionResult>("save_cronjob", { input }),
  cronjobAction: (input: { serverId: string; id: string; command?: string; user?: string; backupAccountIds?: string[]; action: "delete" | "run"; confirmed: boolean }) =>
    invoke<CronJobActionResult>("cronjob_action", { input }),
  cronjobHistory: (serverId: string) => invoke<CronJobHistoryEntry[]>("get_cronjob_history", { serverId }),
  clearCronjobHistory: (serverId: string) => invoke<void>("clear_cronjob_history", { serverId }),
  cronNotificationSettings: (serverId: string) =>
    invoke<CronNotificationSettings>("get_cron_notification_settings", { serverId }),
  saveCronNotificationSettings: (input: {
    serverId: string;
    notifyInApp: boolean;
    notifyWebhook: boolean;
    provider: "generic" | "slack" | "discord" | "dingtalk" | "wecom";
    webhookUrl?: string;
    webhookSigningSecret?: string;
    clearWebhook: boolean;
    clearSigningSecret: boolean;
    confirmed: boolean;
  }) => invoke<CronNotificationSettings>("save_cron_notification_settings", { input }),
  cronOfflineSchedulerSettings: () =>
    invoke<CronOfflineSchedulerSettings>("get_cron_offline_scheduler_settings"),
  saveCronOfflineSchedulerSettings: (input: { enabled: boolean; confirmed: boolean }) =>
    invoke<CronOfflineSchedulerSettings>("save_cron_offline_scheduler_settings", { input }),
  security: (serverId: string) => invoke<SecuritySnapshot>("get_security", { serverId }),
  firewallRuleAction: (input: { serverId: string; action: "add" | "delete"; protocol: "tcp" | "udp" | "any"; port: string; source?: string; comment?: string; confirmed: boolean }) =>
    invoke<FirewallSnapshot>("firewall_rule_action", { input }),
  saveSshSecurity: (input: { serverId: string; port?: number; passwordAuthentication?: boolean; pubkeyAuthentication?: boolean; permitRootLogin?: "yes" | "no" | "prohibit-password" | "forced-commands-only"; confirmed: boolean }) =>
    invoke<SshSecurityConfig>("save_ssh_security", { input }),
  appCatalog: () => invoke<AppCatalogSnapshot>("get_app_catalog"),
  appDetail: (key: string) => invoke<AppDetail>("get_app_detail", { key }),
  appStoreSettings: () => invoke<AppStoreSettings>("get_appstore_settings"),
  saveAppStoreSettings: (input: AppStoreSettings & { mirrorVerificationSecret?: string; clearMirrorVerificationSecret?: boolean }) =>
    invoke<AppStoreSettings>("save_appstore_settings", { input }),
  generateAppStoreMirror: (input: { destination: string; keyId: string; signingSecret: string; maxApps: number; confirmed: boolean }) =>
    invoke<AppStoreMirrorGenerationResult>("generate_appstore_mirror", { input }),
  clearAppStoreCache: () => invoke<{ cleared: boolean }>("clear_appstore_cache"),
  installedApps: (serverId: string) => invoke<InstalledAppsSnapshot>("get_installed_apps", { serverId }),
  appHealth: (input: { serverId: string; project: string; installPath: string }) =>
    invoke<AppHealthSnapshot>("get_app_health", { input }),
  /** 读取官方最新 Compose 与已安装版本的哈希和行数差异。 */
  appUpdatePreview: (input: { serverId: string; key: string; project: string; installPath: string }) =>
    invoke<AppUpdatePreview>("app_update_preview", { input }),
  appEnvironment: (serverId: string, installPath: string) => invoke<AppEnvironmentSnapshot>("get_app_environment", { serverId, installPath }),
  saveAppEnvironment: (input: { serverId: string; installPath: string; values: string[]; confirmed: boolean }) =>
    invoke<AppActionResult>("save_app_environment", { input }),
  installApp: (input: { serverId: string; key: string; version: string; project: string; installPath: string; environment: string[]; confirmed: boolean }) =>
    invoke<AppActionResult>("install_app", { input }),
  appAction: (input: { serverId: string; key: string; project: string; installPath: string; action: "start" | "stop" | "restart" | "pull" | "update" | "uninstall" | "restore"; confirmed: boolean }) =>
    invoke<AppActionResult>("app_action", { input }),
  aiProviders: () => invoke<AiProvider[]>("list_ai_providers"),
  aiModels: (providerId: string) => invoke<AiModel[]>("ai_models", { providerId }),
  saveAiProvider: (input: { id?: string; name: string; baseUrl: string; model: string; enabled: boolean; apiKey?: string; clearApiKey: boolean }) =>
    invoke<AiProvider>("save_ai_provider", { input }),
  deleteAiProvider: (id: string) => invoke<void>("delete_ai_provider", { input: { id } }),
  aiConversations: (providerId?: string) => invoke<AiConversation[]>("list_ai_conversations", { providerId }),
  saveAiConversation: (input: { id?: string; providerId: string; title?: string; messages: Array<{ role: "system" | "user" | "assistant"; content: string }> }) =>
    invoke<AiConversation>("save_ai_conversation", { input }),
  deleteAiConversation: (id: string) => invoke<void>("delete_ai_conversation", { input: { id } }),
  clearAiConversations: (providerId?: string) => invoke<void>("clear_ai_conversations", { providerId }),
  aiChat: (input: { providerId: string; messages: Array<{ role: "system" | "user" | "assistant"; content: string }>; temperature?: number }) =>
    invoke<AiChatResult>("ai_chat", { input }),
  aiChatStream: (input: { providerId: string; messages: Array<{ role: "system" | "user" | "assistant"; content: string }>; temperature?: number; taskId?: string }, onEvent: (event: AiStreamEvent) => void) => {
    const channel = new Channel<AiStreamEvent>();
    channel.onmessage = onEvent;
    return invoke<AiChatResult>("ai_chat_stream", { input, onEvent: channel });
  },
  aiAgent: (input: { providerId: string; serverId: string; messages: Array<{ role: "system" | "user" | "assistant"; content: string }>; maxSteps?: number; mcpEnabled?: boolean }) =>
    invoke<AiAgentResult>("ai_agent", { input }),
  aiMcpServers: () => invoke<McpServerConfig[]>("list_ai_mcp_servers"),
  saveAiMcpServer: (input: { id?: string; name: string; transport: "stdio" | "http"; command: string; args: string[]; url?: string; authToken?: string; clearAuthToken: boolean; enabled: boolean; allowWrite: boolean; timeoutSeconds: number }) =>
    invoke<McpServerConfig>("save_ai_mcp_server", { input }),
  deleteAiMcpServer: (id: string) => invoke<void>("delete_ai_mcp_server", { input: { id } }),
  probeAiMcpServer: (serverId: string) => invoke<McpProbeResult>("probe_ai_mcp_server", { serverId }),
};

export type TerminalEvent =
  | { event: "data"; data: { data: string } }
  | { event: "exit"; data: { exitStatus: number } }
  | { event: "closed" };

export type CommandEvent =
  | { event: "output"; data: { stream: "stdout" | "stderr"; data: string } }
  | { event: "completed"; data: { exitCode: number } }
  | { event: "cancelled" };

export type TransferEvent =
  | { event: "started"; data: { transferId: string; totalBytes: number | null } }
  | { event: "progress"; data: { transferId: string; transferredBytes: number; totalBytes: number | null; bytesPerSecond: number; currentPath: string } }
  | { event: "completed"; data: { transferId: string; transferredBytes: number } }
  | { event: "cancelled"; data: { transferId: string; transferredBytes: number } };

export function isHostKeyChallenge(
  value: ConnectionSnapshot | HostKeyChallenge,
): value is HostKeyChallenge {
  return "fingerprint" in value;
}

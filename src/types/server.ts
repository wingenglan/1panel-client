export type AuthType = "password" | "private_key" | "ssh_agent";
export type SudoMode = "none" | "passwordless" | "password";

export interface ServerProfile {
  id: string;
  name: string;
  description: string;
  host: string;
  port: number;
  username: string;
  authType: AuthType;
  privateKeyPath: string | null;
  sudoMode: SudoMode;
  groupId: string | null;
  tags: string[];
  favorite: boolean;
  connectTimeout: number;
  keepalive: number;
  encoding: string;
  proxyJumpId: string | null;
  lastConnectedAt: string | null;
  createdAt: string;
  updatedAt: string;
}

export type TopologyIssueKind = "selfReference" | "orphan" | "cycle" | "depthExceeded";

export interface TopologyIssue {
  serverId: string;
  serverName: string;
  kind: TopologyIssueKind;
  message: string;
}

export interface SaveServerInput {
  id?: string;
  name: string;
  description: string;
  host: string;
  port: number;
  username: string;
  authType: AuthType;
  password?: string;
  privateKeyPath?: string;
  privateKeyPassphrase?: string;
  sudoMode: SudoMode;
  sudoPassword?: string;
  groupId?: string;
  connectTimeout?: number;
  keepalive?: number;
  encoding?: string;
  proxyJumpId?: string;
  tags: string[];
  favorite: boolean;
}

export interface ServerGroup {
  id: string;
  name: string;
  sortOrder: number;
  createdAt: string;
}

export interface PublicServerProfile {
  name: string;
  description: string;
  host: string;
  port: number;
  username: string;
  authType: AuthType;
  privateKeyPath: string | null;
  sudoMode: SudoMode;
  groupId: string | null;
  tags: string[];
  favorite: boolean;
  connectTimeout: number;
  keepalive: number;
  encoding: string;
  proxyJumpId: string | null;
}

export type PublicServerImport = Omit<PublicServerProfile, "sudoMode" | "groupId"> & { sudoMode?: SudoMode; groupId?: string | null };

export interface PublicServerExport {
  format: "1panel-client-backup";
  version: number;
  encrypted: false;
  servers: PublicServerProfile[];
}

export interface HostKeyChallenge {
  serverId: string;
  host: string;
  port: number;
  keyType: string;
  fingerprint: string;
}

export interface ConnectionSnapshot {
  serverId: string;
  status: "offline" | "connecting" | "online" | "error";
  connectedAt: string | null;
  error: AppError | null;
}

export interface SystemOverview {
  hostname: string;
  osName: string;
  osVersion: string;
  kernel: string;
  architecture: string;
  currentUser: string;
  currentTime: string;
  timezone: string;
  primaryIp: string;
  defaultGateway: string;
  packageManager: string;
  systemdRunning: boolean;
  uptimeSeconds: number;
  cpuModel: string;
  logicalCores: number;
  cpuUsagePercent: number | null;
  load: [number, number, number];
  memoryTotalBytes: number;
  memoryAvailableBytes: number;
  swapTotalBytes: number;
  swapFreeBytes: number;
  networkRxBytesPerSecond: number;
  networkTxBytesPerSecond: number;
  networkRxBytesTotal?: number;
  networkTxBytesTotal?: number;
  ioReadBytesPerSecond?: number;
  ioWriteBytesPerSecond?: number;
  ioCountPerSecond?: number;
  ioLatencyMs?: number;
  failedServices: number;
  listeningPorts: number;
  disks: Array<{ mount: string; totalBytes: number; usedBytes: number; usagePercent: number }>;
  topProcesses: Array<{ pid: number; name: string; cpuPercent: number; memoryPercent: number; command: string }>;
  mounts: Array<{ mount: string; source: string; filesystem: string; options: string }>;
  docker: { installed: boolean; running: boolean; version: string | null };
  nginx: { installed: boolean; running: boolean; version: string | null };
  capabilities: Record<string, boolean>;
  serverCapabilities: { adapter: string; packageManager: string; serviceManager: string; firewall: string | null; commandPaths: Record<string, string>; dockerCommand: string | null; nginxCommand: string | null; permissionDiagnostics: Array<{ scope: string; status: string; detail: string }> };
  permissionDiagnostics: Array<{ scope: string; status: string; detail: string }>;
  sampledAt: string;
}

export interface OverviewMemo {
  content: string;
  updatedAt: string | null;
}

export interface AppError {
  code: string;
  category: string;
  message: string;
  details?: string | null;
  serverId?: string | null;
  recoverable: boolean;
  suggestedAction?: string | null;
}

export type RemoteFileKind = "directory" | "file" | "symlink" | "other";

export interface RemoteFileEntry {
  name: string;
  path: string;
  kind: RemoteFileKind;
  size: number;
  permissions: string;
  owner: string;
  group: string;
  modifiedAt: number | null;
}

export interface DirectoryListing {
  path: string;
  entries: RemoteFileEntry[];
}

export interface RemoteTextFile {
  path: string;
  content: string;
  size: number;
  modifiedAt: number | null;
  permissions: number | null;
}

export interface OperationsSnapshot {
  processes: Array<{ pid: number; ppid: number; user: string; state: string; cpuPercent: number; memoryPercent: number; rssBytes: number; elapsedSeconds: number; name: string; command: string; systemdUnit: string | null }>;
  ports: Array<{ protocol: string; localAddress: string; port: number; pid: number | null; processName: string | null; ipv6: boolean; processVisible: boolean }>;
  portsSource: string;
  portsWarning: string | null;
  services: Array<{ name: string; load: string; active: string; sub: string; description: string }>;
}

export interface StorageDevice {
  name: string;
  path: string;
  kind: string;
  filesystem: string | null;
  sizeBytes: number;
  readonly: boolean;
  removable: boolean;
  mountpoint: string | null;
  model: string | null;
}

export interface StorageMount {
  mountpoint: string;
  source: string;
  filesystem: string;
  options: string;
  totalBytes: number;
  usedBytes: number;
  availableBytes: number;
  usagePercent: number;
}

export interface FstabEntry {
  lineNumber: number;
  source: string;
  mountpoint: string;
  filesystem: string;
  options: string;
  dump: string;
  pass: string;
}

export interface StorageTopology {
  disks: number;
  partitions: number;
  raidArrays: number;
  lvmVolumes: number;
  otherDevices: number;
}

export interface StorageSnapshot {
  devices: StorageDevice[];
  topology: StorageTopology;
  mounts: StorageMount[];
  fstab: FstabEntry[];
  warnings: string[];
  fetchedAt: string;
}

export interface StorageActionResult {
  action: string;
  mountpoint: string;
  fstabUpdated: boolean;
  mounted: boolean | null;
  output: string;
}

export type ShortcutScope = "global" | "server";

export interface ShortcutRecord {
  id: string;
  scope: ShortcutScope;
  serverId: string | null;
  name: string;
  groupName: string;
  commandTemplate: string;
  description: string;
  tags: string[];
  enabled: boolean;
  builtin: boolean;
  usageCount: number;
  createdAt: string;
  updatedAt: string;
}

export interface SaveShortcutInput {
  id?: string;
  scope: ShortcutScope;
  serverId?: string;
  name: string;
  groupName: string;
  commandTemplate: string;
  description: string;
  tags: string[];
  enabled: boolean;
}

export type LogSource = "system" | "systemd" | "nginx-access" | "nginx-error" | "docker" | "docker-compose";

export interface LogQuery {
  serverId: string;
  source: LogSource;
  target?: string;
  workingDir?: string;
  service?: string;
  tail: number;
  privileged: boolean;
}

export interface LogSnapshot {
  source: LogSource;
  target: string | null;
  output: string;
  fetchedAt: string;
  truncated: boolean;
}

export interface MetricSample {
  sampledAt: string;
  cpuUsagePercent: number | null;
  memoryUsedBytes: number;
  memoryTotalBytes: number;
  loadOne: number;
  networkRxBytesPerSecond: number;
  networkTxBytesPerSecond: number;
  ioReadBytesPerSecond?: number;
  ioWriteBytesPerSecond?: number;
  diskUsagePercent: number | null;
}

export type PersistedTaskStatus = "queued" | "running" | "success" | "failed" | "cancelled" | "interrupted";

export interface TaskRecord {
  id: string;
  taskType: string;
  serverId: string | null;
  title: string;
  status: PersistedTaskStatus;
  progress: number | null;
  bytesTransferred: number;
  totalBytes: number | null;
  startedAt: string;
  finishedAt: string | null;
  errorCode: string | null;
  errorMessage: string | null;
  cancelSupported: boolean;
  retryPayloadJson: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface SaveTaskInput {
  id: string;
  taskType: string;
  serverId?: string;
  title: string;
  status: PersistedTaskStatus;
  progress: number | null;
  bytesTransferred: number;
  totalBytes: number | null;
  startedAt: string;
  finishedAt?: string | null;
  errorCode?: string | null;
  errorMessage?: string | null;
  cancelSupported: boolean;
  retryPayloadJson?: string | null;
}

export interface RemoteBinaryPreview {
  path: string;
  mimeType: string;
  dataBase64: string;
  size: number;
  modifiedAt: number | null;
}

export interface TerminationResult {
  pid: number;
  signal: string;
  processExited: boolean;
  portReleased: boolean | null;
}

export interface ServiceDetail {
  name: string;
  description: string;
  load: string;
  active: string;
  sub: string;
  mainPid: number | null;
  fragmentPath: string;
  unitFileState: string;
}

export interface ServiceLogs {
  name: string;
  output: string;
}

export interface AuditEvent {
  id: string;
  serverId: string | null;
  action: string;
  resourceType: string;
  resourceId: string | null;
  result: string;
  summary: string;
  createdAt: string;
}

export interface DiagnosticsExport {
  generatedAt: string;
  appVersion: string;
  platform: string;
  architecture: string;
  servers: Array<{ id: string; name: string; host: string; port: number; username: string; authType: string; sudoMode: string; favorite: boolean; tags: string[] }>;
  connections: Array<{ serverId: string; status: string; connectedAt: string | null; errorCode: string | null }>;
  recentAudit: AuditEvent[];
}

export interface ToolStatus {
  id: string;
  name: string;
  description: string;
  installed: boolean;
  version: string | null;
  running: boolean | null;
  packageManager: string | null;
  installPackage: string | null;
  requiresSudo: boolean;
}

export interface ToolInstallPlan {
  tool: ToolStatus;
  command: string;
  risk: string;
}

export interface ToolInstallResult {
  toolId: string;
  output: string;
  verified: ToolStatus;
}

export interface ReverseProxy {
  serverNames: string[];
  listen: string[];
  location: string;
  upstream: string;
  targetHost: string;
  targetPort: number | null;
  ssl: boolean;
  sourceFile: string;
  sourceLine: number;
}

export interface NginxSnapshot {
  installed: boolean;
  running: boolean;
  flavor: string;
  binary: string | null;
  containerId: string | null;
  containerSiteRoot: string | null;
  siteHostRoot: string | null;
  version: string | null;
  configPath: string | null;
  configTest: boolean | null;
  managedConfSupported: boolean;
  managedConfDir: string | null;
  proxies: ReverseProxy[];
  certificates: Array<{ certificatePath: string; privateKeyPath: string | null; sourceFile: string; sourceLine: number; expiresAt: string | null; daysRemaining: number | null }>;
  configSources: string[];
  servers: number;
  upstreams: number;
  warnings: string[];
}

export interface WebsiteRecord {
  domain: string;
  kind: "static" | "proxy";
  enabled: boolean;
  listenPort: number;
  rootPath: string | null;
  upstream: string | null;
  phpRuntime: string | null;
  ssl: boolean;
  certificatePath: string | null;
  expiresAt: string | null;
  configPath: string;
}

export interface WebsiteSnapshot {
  supported: boolean;
  managedConfDir: string | null;
  nginxVersion: string | null;
  runtimeRoot: string | null;
  hostRoot: string | null;
  websites: WebsiteRecord[];
  phpRuntimes: PhpRuntime[];
  certificateTools: { certbot: boolean; acmeSh: boolean };
  warnings: string[];
  fetchedAt: string;
}

export interface PhpRuntime {
  id: string;
  version: string | null;
  binary: string | null;
  service: string | null;
  socketPath: string | null;
  installed: boolean;
  running: boolean;
}

export interface PhpInstallPlan {
  packageManager: string;
  packages: string[];
  services: string[];
  command: string;
  risk: string;
}

export interface PhpInstallResult {
  packageManager: string;
  runtimes: PhpRuntime[];
  output: string;
}

export interface CertificateActionResult {
  domain: string;
  action: string;
  challenge: "http01" | "dns01";
  dnsProvider: string | null;
  tool: string;
  certificatePath: string;
  certificateKeyPath: string;
  output: string;
}

export interface CertificateRenewalPlan {
  domain: string;
  action: "issue" | "renew";
  reason: "missing" | "expiring";
  expiresAt: string | null;
  certificatePath: string | null;
  renewBeforeDays: number;
}

export interface DockerSnapshot {
  installed: boolean;
  running: boolean;
  version: string | null;
  apiVersion: string | null;
  os: string | null;
  architecture: string | null;
  storageDriver: string | null;
  cgroupVersion: string | null;
  rootDir: string | null;
  socketPath: string | null;
  diskUsage: string | null;
  containers: DockerContainerInfo[];
  images: DockerImageInfo[];
  volumes: DockerVolumeInfo[];
  networks: DockerNetworkInfo[];
  composeProjects: DockerComposeProject[];
}

export interface DockerContainerInfo {
  id: string;
  name: string;
  image: string;
  status: string;
  state: string;
  health: string | null;
  created: string;
  ports: string;
  ipAddresses: string;
  composeProject: string | null;
  restartPolicy: string | null;
  cpuLimitNano: number | null;
  memoryLimitBytes: number | null;
  cpuPercent: number;
  memoryPercent: number;
}

export interface DockerImageInfo {
  repository: string;
  tag: string;
  id: string;
  size: string;
  created: string;
  dangling: boolean;
}

export interface DockerVolumeInfo {
  name: string;
  driver: string;
  mountpoint: string;
  labels: string;
}

export interface DockerNetworkInfo {
  id: string;
  name: string;
  driver: string;
  scope: string;
}

export interface DockerComposeProject {
  name: string;
  status: string;
  configFiles: string;
  workingDir: string;
}

export interface DockerComposeService {
  name: string;
  service: string;
  image: string;
  state: string;
  status: string;
  ports: string;
}

export interface DockerComposeDetails {
  project: string;
  services: DockerComposeService[];
  config: string;
  configPath: string | null;
  configSize: number | null;
  configModifiedAt: number | null;
  volumes: string[];
  networks: string[];
  cleanupVolumes: string[];
  cleanupNetworks: string[];
  orphanContainers: string[];
  cleanupWarnings: string[];
}

export interface DockerTextResult {
  containerId: string;
  output: string;
}

export interface DockerPruneResult {
  kind: "images" | "containers" | "volumes" | "builders";
  command: string;
  output: string;
}

export interface DockerEvent {
  eventType: string;
  action: string;
  actorName: string | null;
  actorId: string | null;
  timestamp: number | null;
}

export interface DockerEventsResult {
  sinceSeconds: number;
  events: DockerEvent[];
  fetchedAt: string;
}

export interface DockerPullResult {
  image: string;
  output: string;
}

export interface DockerBuildResult {
  image: string;
  contextPath: string;
  output: string;
}

export interface DockerRunResult {
  containerId: string;
  output: string;
}

export interface DockerActionResult {
  containerId: string;
  action: string;
  verifiedStatus: string;
}

export interface DockerResourceActionResult {
  kind: string;
  name: string;
  action: string;
  verified: boolean;
}

export interface DockerLogs {
  containerId: string;
  output: string;
}

export interface DatabaseEngine {
  id: string;
  name: string;
  installed: boolean;
  running: boolean;
  version: string | null;
  port: number | null;
}

export interface DatabaseRecord {
  engine: string;
  name: string;
  owner: string | null;
  charset: string | null;
  collation: string | null;
}

export interface DatabaseUser {
  engine: string;
  username: string;
  host: string | null;
  privileges: string | null;
  canLogin: boolean | null;
}

export interface DatabasePrivilegeEntry {
  database: string;
  privileges: string;
}

export interface DatabasePrivilegeSnapshot {
  engine: string;
  username: string;
  host: string | null;
  entries: DatabasePrivilegeEntry[];
  fetchedAt: string;
}

export interface PrivilegeDiagnostic {
  severity: "info" | "warning";
  category: string;
  message: string;
}

export interface DatabasePrivilegeDiagnostic {
  snapshot: DatabasePrivilegeSnapshot;
  diagnostics: PrivilegeDiagnostic[];
}

export interface DatabaseSnapshot {
  engines: DatabaseEngine[];
  databases: DatabaseRecord[];
  users: DatabaseUser[];
  fetchedAt: string;
}

export interface DatabaseActionResult {
  engine: string;
  name: string;
  action: string;
  output: string;
}

export interface DatabaseInstallPlan {
  engine: DatabaseEngine;
  packageManager: string;
  packages: string[];
  services: string[];
  command: string;
  risk: string;
}

export interface DatabaseInstallResult {
  engine: DatabaseEngine;
  packageManager: string;
  output: string;
}

export interface RedisKeyEntry {
  key: string;
  kind: string;
  ttlSeconds: number;
  sizeBytes: number | null;
}

export interface RedisSnapshot {
  available: boolean;
  database: number;
  totalKeys: number;
  keys: RedisKeyEntry[];
  fetchedAt: string;
}

export interface RedisDiagnostic {
  available: boolean;
  database: number;
  latencyMs: number | null;
  status: string | null;
  version: string | null;
  role: string | null;
  mode: string | null;
  uptimeSeconds: number | null;
  connectedClients: number | null;
  usedMemoryBytes: number | null;
  fetchedAt: string;
}

export interface RedisValueResult {
  database: number;
  key: string;
  kind: string;
  value: string;
  truncated: boolean;
}

export interface RedisComplexActionResult {
  database: number;
  key: string;
  kind: "hash" | "list" | "set" | "zset";
  action: string;
  output: string;
}

export interface RedisTransferResult {
  database: number;
  action: "export" | "import";
  path: string;
  keys: number;
  output: string;
}

export interface RedisMigrationResult {
  sourceDatabase: number;
  targetHost: string;
  targetPort: number;
  targetDatabase: number;
  keys: number;
  output: string;
}

export interface CronJob {
  id: string;
  schedule: string;
  command: string;
  kind: "shell" | "url" | "directory" | "database" | "log" | "website" | "app";
  user: string;
  managed: boolean;
  enabled: boolean;
  retentionCount: number | null;
  retentionDays: number | null;
  backupAccountIds: string[];
  defaultBackupAccountId: string | null;
  backupEventPath: string | null;
}

export interface CronJobHistoryEntry {
  id: string;
  jobId: string;
  action: string;
  success: boolean;
  output: string;
  startedAt: string;
  finishedAt: string;
}

export interface CronTimer {
  name: string;
  nextRun: string;
  lastRun: string;
  activates: string;
}

export interface CronSnapshot {
  user: string;
  jobs: CronJob[];
  timers: CronTimer[];
  fetchedAt: string;
}

export interface CronJobActionResult {
  id: string;
  action: string;
  output: string;
}

export interface CronNotificationSettings {
  serverId: string;
  notifyInApp: boolean;
  notifyWebhook: boolean;
  provider: "generic" | "slack" | "discord" | "dingtalk" | "wecom" | string;
  webhookConfigured: boolean;
  signingSecretConfigured: boolean;
}

export interface CronJobExportEntry {
  id: string;
  schedule: string;
  command: string;
  kind: CronJob["kind"] | string;
  user: string;
  managed: boolean;
  enabled: boolean;
  retentionCount?: number | null;
  retentionDays?: number | null;
  backupAccountIds?: string[];
  defaultBackupAccountId?: string | null;
  backupEventPath?: string | null;
}

export interface CronOfflineSchedulerSettings {
  enabled: boolean;
}

export interface CronJobExport {
  format: "1panel-client-cronjobs";
  version: number;
  serverId: string;
  exportedAt: string;
  jobs: CronJobExportEntry[];
}

export interface CronJobImportFailure {
  index: number;
  message: string;
}

export interface CronJobImportResult {
  imported: number;
  convertedToShell: number;
  unresolvedBackupAccounts: number;
  failures: CronJobImportFailure[];
}

export interface BackupAccount {
  id: string;
  name: string;
  kind: "local" | "webdav" | "s3" | "sftp" | string;
  serverId: string | null;
  endpoint: string | null;
  remotePath: string;
  bucket: string | null;
  region: string | null;
  username: string | null;
  privateKeyPath: string | null;
  hostKeyFingerprint: string | null;
  hasSecret: boolean;
  createdAt: string;
  updatedAt: string;
}

export interface BackupAccountTestResult {
  accountId: string;
  reachable: boolean;
  statusCode: number | null;
  detail: string;
  checkedAt: string;
}

export interface BackupUploadResult {
  accountId: string;
  kind: string;
  target: string;
  bytes: number;
  detail: string;
}

export interface FirewallRule {
  id: string;
  source: string;
  destination: string;
  protocol: string;
  port: string;
  action: string;
  raw: string;
}

export interface FirewallSnapshot {
  backend: string;
  installed: boolean;
  enabled: boolean;
  defaultIncoming: string | null;
  defaultOutgoing: string | null;
  rules: FirewallRule[];
  warnings: string[];
}

export interface SshSecurityConfig {
  configPath: string;
  port: number;
  passwordAuthentication: boolean | null;
  pubkeyAuthentication: boolean | null;
  permitRootLogin: string | null;
  effectiveLines: string[];
}

export interface SecuritySnapshot {
  firewall: FirewallSnapshot;
  ssh: SshSecurityConfig;
  warnings: string[];
}

export interface AdvancedSnapshot {
  wafEnabled: boolean;
  wafProvider: string | null;
  monitoringSupported: boolean;
  warnings: string[];
  fetchedAt: string;
}

export interface HttpMonitorResult {
  url: string;
  reachable: boolean;
  statusCode: number | null;
  latencyMs: number | null;
  detail: string;
  checkedAt: string;
}

export interface HttpMonitorProfile {
  id: string;
  serverId: string;
  name: string;
  url: string;
  expectedStatus: number | null;
  intervalSeconds: number;
  enabled: boolean;
  lastCheckedAt: string | null;
  lastReachable: boolean | null;
  lastStatusCode: number | null;
  lastLatencyMs: number | null;
  lastDetail: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface HttpMonitorCheck {
  id: number;
  monitorId: string;
  checkedAt: string;
  reachable: boolean;
  statusCode: number | null;
  latencyMs: number | null;
  detail: string;
}

export interface WafRule {
  lineNumber: number;
  directive: string;
  sourcePath: string;
}

export interface WafRulesSnapshot {
  supported: boolean;
  provider: string | null;
  configPath: string | null;
  target: WafRuleTarget | null;
  rules: WafRule[];
  warnings: string[];
}

export interface WafRuleSource {
  id: string;
  name: string;
  channel: string;
  version: string;
  url: string;
  sha256: string;
  signatureFingerprint: string;
  supported: boolean;
  installedVersion: string | null;
  installPath: string;
  updateAvailable: boolean;
}

export interface WafRuleSourcesSnapshot {
  supported: boolean;
  target: WafRuleTarget | null;
  sources: WafRuleSource[];
  warnings: string[];
  fetchedAt: string;
}

export interface WafRuleSourceActionResult {
  sourceId: string;
  action: "install" | "update" | "remove";
  version: string | null;
  installPath: string;
  output: string;
}

export interface WafTemplate {
  id: string;
  name: string;
  description: string;
  risk: string;
  ruleId: number;
  rule: string;
}

export interface WafRuleTarget {
  path: string;
  containerId: string | null;
}

export interface WafAlert {
  sourcePath: string;
  summary: string;
  severity: "warning" | "error" | "critical";
  fingerprint: string;
}

export interface WafAlertHistoryEntry {
  sourcePath: string;
  summary: string;
  severity: "warning" | "error" | "critical";
  fingerprint: string;
  firstSeenAt: string;
  lastSeenAt: string;
  occurrences: number;
}

export interface WafAlertTrendPoint {
  bucketAt: string;
  warning: number;
  error: number;
  critical: number;
  total: number;
}

export interface WafAlertSettings {
  minSeverity: "warning" | "error" | "critical";
  notifyInApp: boolean;
  historyLimit: number;
  notifyWebhook: boolean;
  notifyProvider: "generic" | "slack" | "discord" | "dingtalk" | "wecom";
  webhookConfigured: boolean;
  signingSecretConfigured: boolean;
}

export interface WafAlertsSnapshot {
  supported: boolean;
  sources: string[];
  alerts: WafAlert[];
  history: WafAlertHistoryEntry[];
  trend: WafAlertTrendPoint[];
  newAlerts: number;
  webhookSent: boolean;
  webhookError: string | null;
  settings: WafAlertSettings;
  warnings: string[];
  fetchedAt: string;
}

export interface AppCatalogItem {
  key: string;
  name: string;
  description: string;
  category: string;
  metadataUrl: string;
}

export interface AppCatalogSnapshot {
  repository: string;
  branch: string;
  sourceRevision: string;
  items: AppCatalogItem[];
  fetchedAt: string;
  cached: boolean;
  cacheAgeSeconds: number | null;
  signaturePresent: boolean;
  signatureVerified: boolean;
  resolvedMirrorBaseUrl: string | null;
}

export interface AppVersion {
  version: string;
  composeUrl: string;
  envUrl: string;
}

export interface AppDetail {
  key: string;
  name: string;
  description: string;
  tags: string[];
  website: string | null;
  github: string | null;
  versions: AppVersion[];
  fetchedAt: string;
  cached: boolean;
  cacheAgeSeconds: number | null;
  resolvedMirrorBaseUrl: string | null;
}

export interface AppStoreSettings {
  source: "official" | "mirror";
  mirrorBaseUrl: string | null;
  mirrorBaseUrls: string[];
  cacheTtlSeconds: number;
  offlineMode: boolean;
  mirrorKeyId: string | null;
  signatureConfigured: boolean;
}

export interface AppStoreMirrorGenerationResult {
  destination: string;
  sourceRevision: string;
  appCount: number;
  versionCount: number;
  fileCount: number;
  catalogSha256: string;
  signaturePath: string;
}

export interface InstalledApp {
  key: string;
  path: string;
  composePath: string;
  project: string;
  status: string;
  version?: string | null;
  hostPorts?: string[];
  installedSeconds?: number | null;
}

export interface InstalledAppsSnapshot {
  composeAvailable: boolean;
  apps: InstalledApp[];
  fetchedAt: string;
}

export interface AppServiceHealth {
  name: string;
  image: string;
  state: string;
  health: string;
  exitCode: number;
}

export interface AppHealthSnapshot {
  project: string;
  path: string;
  overall: "healthy" | "degraded" | "stopped";
  services: AppServiceHealth[];
  fetchedAt: string;
}

export interface AppUpdatePreview {
  key: string;
  project: string;
  latestVersion: string;
  currentHash: string | null;
  latestHash: string;
  currentLines: number;
  latestLines: number;
  changed: boolean;
  currentMissing: boolean;
  fetchedAt: string;
}

export interface AppEnvironmentEntry {
  key: string;
  configured: boolean;
  maskedValue: string;
}

export interface AppEnvironmentSnapshot {
  path: string;
  entries: AppEnvironmentEntry[];
}

export interface AppActionResult {
  key: string;
  project: string;
  action: string;
  output: string;
}

export interface AiProvider {
  id: string;
  name: string;
  baseUrl: string;
  model: string;
  enabled: boolean;
  hasApiKey: boolean;
}

export interface AiModel {
  id: string;
  ownedBy: string | null;
}

export interface AiChatResult {
  providerId: string;
  model: string;
  content: string;
  promptTokens: number | null;
  completionTokens: number | null;
}

export interface AiConversationMessage {
  role: "system" | "user" | "assistant";
  content: string;
}

export interface AiConversation {
  id: string;
  providerId: string;
  title: string;
  messages: AiConversationMessage[];
  createdAt: string;
  updatedAt: string;
}

export interface AiAgentResult {
  providerId: string;
  model: string;
  content: string;
  steps: number;
  toolCalls: number;
}

export interface McpServerConfig {
  id: string;
  name: string;
  transport: "stdio" | "http";
  command: string;
  args: string[];
  url?: string | null;
  authConfigured: boolean;
  enabled: boolean;
  allowWrite: boolean;
  timeoutSeconds: number;
}

export interface McpToolSummary {
  serverId: string;
  serverName: string;
  name: string;
  description: string;
  readOnly: boolean;
}

export interface McpProbeResult {
  serverId: string;
  tools: McpToolSummary[];
  checkedAt: string;
}

export type AiStreamEvent =
  | { event: "delta"; data: { content: string } }
  | { event: "completed"; data: { model: string; promptTokens: number | null; completionTokens: number | null } };

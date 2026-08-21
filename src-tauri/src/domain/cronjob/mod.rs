use crate::domain::ssh::SshConnectionManager;
use crate::errors::{AppError, AppResult};
use crate::infra::db::ServerRepository;
use crate::infra::local::LocalRepository;
use crate::security::{redact, shell_escape};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use hmac::{Hmac, KeyInit, Mac};
use reqwest::Client;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

/// 描述一条来自用户 crontab 的计划任务。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CronJob {
    pub id: String,
    pub schedule: String,
    pub command: String,
    pub kind: String,
    pub user: String,
    pub managed: bool,
    pub enabled: bool,
    /// 对归档类任务可选的保留份数；Shell/URL 任务始终为 None。
    pub retention_count: Option<u32>,
    /// 对归档类任务可选的保留天数；Shell/URL 任务始终为 None。
    pub retention_days: Option<u32>,
    /// 归档成功后由客户端上传的备份账号 ID；远程 crontab 不保存账号凭据。
    pub backup_account_ids: Vec<String>,
    /// 多账号上传时作为默认报告目标的账号 ID。
    pub default_backup_account_id: Option<String>,
    /// 远端成功归档事件文件路径；客户端在线时据此补传离线期间生成的归档。
    pub backup_event_path: Option<String>,
}

/// 描述 systemd timer，作为只读补充展示在计划任务页。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CronTimer {
    pub name: String,
    pub next_run: String,
    pub last_run: String,
    pub activates: String,
}

/// 返回 crontab 与 systemd timer 的统一快照。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CronSnapshot {
    pub user: String,
    pub jobs: Vec<CronJob>,
    pub timers: Vec<CronTimer>,
    pub fetched_at: chrono::DateTime<chrono::Utc>,
}

/// 创建或更新用户 crontab 任务的请求。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveCronJobInput {
    pub server_id: String,
    pub id: Option<String>,
    pub schedule: String,
    pub command: String,
    /// 任务类型；旧版本调用方不传时默认为 Shell。
    #[serde(default = "default_cron_kind")]
    pub kind: String,
    /// URL 类型任务的地址列表；Shell 类型忽略该字段。
    #[serde(default)]
    pub urls: Vec<String>,
    /// directory 类型任务要归档的远端路径列表。
    #[serde(default)]
    pub source_paths: Vec<String>,
    /// 归档或数据库备份的远端目标文件。
    #[serde(default)]
    pub destination: Option<String>,
    /// database 类型任务的固定数据库引擎和名称。
    #[serde(default)]
    pub database_engine: Option<String>,
    #[serde(default)]
    pub database_name: Option<String>,
    /// directory 类型任务的归档排除路径列表。
    #[serde(default)]
    pub exclude_paths: Vec<String>,
    /// website 类型任务选择的客户端受控域名。
    #[serde(default)]
    pub website_domain: Option<String>,
    /// app 类型任务选择的已安装 1Panel 应用目录。
    #[serde(default)]
    pub app_install_path: Option<String>,
    /// 归档类任务可选的滚动保留份数，留空表示不启用按份数清理。
    #[serde(default)]
    pub retention_count: Option<u32>,
    /// 归档类任务可选的滚动保留天数，留空表示不启用按天数清理。
    #[serde(default)]
    pub retention_days: Option<u32>,
    /// 归档成功后由客户端上传的备份账号 ID 列表。
    #[serde(default)]
    pub backup_account_ids: Vec<String>,
    /// 多账号上传时的默认报告目标账号。
    #[serde(default)]
    pub default_backup_account_id: Option<String>,
    /// 仅由受信任的导入路径设置；复用已验证的完整命令而不重新探测网站/应用。
    #[serde(skip)]
    pub(crate) preserve_command: bool,
    /// 仅由受信任的导入路径设置；沿用导出任务的事件文件 marker。
    #[serde(skip)]
    pub(crate) backup_event_path: Option<String>,
    pub user: Option<String>,
    pub enabled: bool,
    pub confirmed: bool,
}

/// 删除或立即运行计划任务的请求。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CronJobActionInput {
    pub server_id: String,
    pub id: String,
    pub command: Option<String>,
    pub user: Option<String>,
    /// 手动运行归档任务后，由客户端使用这些账号上传生成的归档。
    #[serde(default)]
    pub backup_account_ids: Vec<String>,
    pub action: String,
    pub confirmed: bool,
}

/// 返回计划任务写入、删除或立即运行结果。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CronJobActionResult {
    pub id: String,
    pub action: String,
    pub output: String,
}

/// 本地保存的一次性计划任务执行摘要，不包含完整命令输出或凭据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CronJobHistoryEntry {
    pub id: String,
    pub job_id: String,
    pub action: String,
    pub success: bool,
    pub output: String,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub finished_at: chrono::DateTime<chrono::Utc>,
}

/// 描述可下载的版本化计划任务文件；导出内容包含命令，因此只在用户明确点击导出时生成。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CronJobExport {
    pub format: String,
    pub version: u32,
    pub server_id: String,
    pub exported_at: chrono::DateTime<chrono::Utc>,
    pub jobs: Vec<CronJobExportEntry>,
}

/// 描述一条导入导出的计划任务记录；导入时会忽略远端原 ID 并重新生成 marker。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CronJobExportEntry {
    pub id: String,
    pub schedule: String,
    pub command: String,
    pub kind: String,
    pub user: String,
    pub managed: bool,
    pub enabled: bool,
    /// 导出归档任务的保留份数，旧文件缺失时保持 None。
    #[serde(default)]
    pub retention_count: Option<u32>,
    /// 导出归档任务的保留天数，旧文件缺失时保持 None。
    #[serde(default)]
    pub retention_days: Option<u32>,
    /// 导出归档任务的客户端上传账号 ID，旧文件缺失时保持空列表。
    #[serde(default)]
    pub backup_account_ids: Vec<String>,
    /// 导出归档任务的默认上传账号 ID。
    #[serde(default)]
    pub default_backup_account_id: Option<String>,
    /// 远端成功归档事件文件路径；旧导出文件缺失时保持 None。
    #[serde(default)]
    pub backup_event_path: Option<String>,
}

/// 接收版本化计划任务文件的导入请求；导入不会携带任何凭据字段。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CronJobImportInput {
    pub server_id: String,
    pub jobs: Vec<CronJobExportEntry>,
    pub confirmed: bool,
}

/// 返回计划任务导入结果，并报告无法导入的记录而不是回滚已经成功写入的任务。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CronJobImportResult {
    pub imported: usize,
    /// 不受支持的类型被安全降级为 Shell 的数量；受支持类型会保留 marker。
    pub converted_to_shell: usize,
    /// 被剔除的失效/跨客户端备份账号引用数量。
    pub unresolved_backup_accounts: usize,
    pub failures: Vec<CronJobImportFailure>,
}

/// 描述导入文件中单条任务失败的索引和脱敏错误信息。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CronJobImportFailure {
    pub index: usize,
    pub message: String,
}

const CRON_HISTORY_PREFIX: &str = "cron.history.";
const CRON_EXPORT_FORMAT: &str = "1panel-client-cronjobs";
const CRON_EXPORT_VERSION: u32 = 1;
const URL_CURL_PREFIX: &str = "curl --fail --silent --show-error --location --max-time 30 --";
const CRON_KIND_MARKER_PREFIX: &str = "# 1panel-client-cron-kind:";
const CRON_RETENTION_MARKER_PREFIX: &str = "# 1panel-client-cron-retention:";
const CRON_BACKUP_ACCOUNTS_MARKER_PREFIX: &str = "# 1panel-client-cron-backup-accounts:";
const CRON_BACKUP_EVENT_MARKER_PREFIX: &str = "# 1panel-client-cron-backup-event:";
const CRON_NOTIFICATION_SETTING_PREFIX: &str = "cron.notification.";
const CRON_NOTIFICATION_WEBHOOK_PREFIX: &str = "cron-webhook-";
const CRON_NOTIFICATION_SIGNING_PREFIX: &str = "cron-webhook-signing-";
const CRON_OFFLINE_STATE_PREFIX: &str = "cron.offline-state.";
const CRON_OFFLINE_SCHEDULER_SETTING_KEY: &str = "cron.offline-scheduler.enabled";

/// 描述计划任务执行报告通知设置；URL 和签名密钥只通过 hasWebhook/hasSigningSecret 暴露状态。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CronNotificationSettings {
    pub server_id: String,
    pub notify_in_app: bool,
    pub notify_webhook: bool,
    pub provider: String,
    pub webhook_configured: bool,
    pub signing_secret_configured: bool,
}

/// 接收计划任务报告通知设置；敏感字段不会写入本地 JSON。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveCronNotificationSettingsInput {
    pub server_id: String,
    pub notify_in_app: bool,
    pub notify_webhook: bool,
    pub provider: String,
    #[serde(default)]
    pub webhook_url: Option<SecretString>,
    #[serde(default)]
    pub webhook_signing_secret: Option<SecretString>,
    #[serde(default)]
    pub clear_webhook: bool,
    #[serde(default)]
    pub clear_signing_secret: bool,
    pub confirmed: bool,
}

/// 描述客户端离线归档补传调度器的全局开关。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CronOfflineSchedulerSettings {
    pub enabled: bool,
}

/// 接收离线归档补传调度器的全局开关；变更不会修改任何远端 crontab。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveCronOfflineSchedulerSettingsInput {
    pub enabled: bool,
    pub confirmed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredCronNotificationSettings {
    notify_in_app: bool,
    notify_webhook: bool,
    provider: String,
}

/// 为旧版本调用方提供 Shell 任务默认类型。
fn default_cron_kind() -> String {
    "shell".into()
}

/// 读取当前用户 crontab 和 systemd timer，不修改远端状态。
pub async fn snapshot(ssh: &SshConnectionManager, server_id: &str) -> AppResult<CronSnapshot> {
    let result = ssh
        .execute_system(server_id, &probe_command(), Duration::from_secs(30))
        .await?;
    if result.exit_code != 0 {
        return Err(
            AppError::new("CRON_PROBE_FAILED", "cronjob", "计划任务探测失败")
                .details(result.stderr)
                .for_server(server_id),
        );
    }
    parse_snapshot(&result.stdout).ok_or_else(|| {
        AppError::new("CRON_PARSE_FAILED", "cronjob", "计划任务探测结果无法解析")
            .for_server(server_id)
    })
}

/// 导出远端 crontab 任务的版本化 JSON 结构；systemd timer 仍保持只读展示，不写入导出文件。
pub async fn export_jobs(ssh: &SshConnectionManager, server_id: &str) -> AppResult<CronJobExport> {
    validate_server_id(server_id)?;
    let snapshot = snapshot(ssh, server_id).await?;
    if snapshot.jobs.len() > 500 {
        return Err(AppError::new(
            "CRON_EXPORT_TOO_LARGE",
            "cronjob",
            "计划任务数量超过导出上限",
        ));
    }
    Ok(CronJobExport {
        format: CRON_EXPORT_FORMAT.into(),
        version: CRON_EXPORT_VERSION,
        server_id: server_id.into(),
        exported_at: chrono::Utc::now(),
        jobs: snapshot
            .jobs
            .into_iter()
            .map(|job| CronJobExportEntry {
                id: job.id,
                schedule: job.schedule,
                command: job.command,
                kind: job.kind,
                user: job.user,
                managed: job.managed,
                enabled: job.enabled,
                retention_count: job.retention_count,
                retention_days: job.retention_days,
                backup_account_ids: job.backup_account_ids,
                default_backup_account_id: job.default_backup_account_id,
                backup_event_path: job.backup_event_path,
            })
            .collect(),
    })
}

/// 导入版本化任务文件；每条记录都重新生成 marker，受支持类型复用已校验命令，未知类型安全降级为 Shell。
pub async fn import_jobs(
    ssh: &SshConnectionManager,
    input: CronJobImportInput,
) -> AppResult<CronJobImportResult> {
    validate_server_id(&input.server_id)?;
    if !input.confirmed {
        return Err(AppError::new(
            "CONFIRMATION_REQUIRED",
            "cronjob",
            "请先确认计划任务导入会写入远端 crontab",
        ));
    }
    if input.jobs.is_empty() || input.jobs.len() > 200 {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "cronjob",
            "计划任务导入数量必须在 1 到 200 条之间",
        ));
    }
    let mut result = CronJobImportResult {
        imported: 0,
        converted_to_shell: 0,
        unresolved_backup_accounts: 0,
        failures: Vec::new(),
    };
    for (index, job) in input.jobs.into_iter().enumerate() {
        if let Err(error) = validate_import_entry(&job) {
            result.failures.push(CronJobImportFailure {
                index,
                message: redact(&error.message),
            });
            continue;
        }
        let preserve_kind = is_supported_kind(&job.kind) && job.kind != "shell";
        if !is_supported_kind(&job.kind) {
            result.converted_to_shell += 1;
        }
        let backup_account_ids = if preserve_kind {
            job.backup_account_ids.clone()
        } else {
            Vec::new()
        };
        let default_backup_account_id = if preserve_kind {
            job.default_backup_account_id.clone()
        } else {
            None
        };
        let save_input = SaveCronJobInput {
            server_id: input.server_id.clone(),
            id: None,
            schedule: job.schedule,
            command: job.command,
            kind: if preserve_kind {
                job.kind.clone()
            } else {
                "shell".into()
            },
            urls: Vec::new(),
            source_paths: Vec::new(),
            destination: None,
            database_engine: None,
            database_name: None,
            exclude_paths: Vec::new(),
            website_domain: None,
            app_install_path: None,
            // 导入时复用完整命令；保留策略已内嵌在命令中，账号引用单独重新写 marker。
            retention_count: None,
            retention_days: None,
            backup_account_ids,
            default_backup_account_id,
            preserve_command: preserve_kind,
            backup_event_path: if preserve_kind {
                job.backup_event_path.clone()
            } else {
                None
            },
            user: Some(job.user),
            enabled: job.enabled,
            confirmed: true,
        };
        match save(ssh, save_input).await {
            Ok(_) => result.imported += 1,
            Err(error) => result.failures.push(CronJobImportFailure {
                index,
                message: redact(&error.message),
            }),
        }
    }
    Ok(result)
}

/// 创建或更新一条带有 1Panel Client marker 的 crontab 记录。
pub async fn save(
    ssh: &SshConnectionManager,
    input: SaveCronJobInput,
) -> AppResult<CronJobActionResult> {
    validate_schedule(&input.schedule)?;
    validate_kind(&input.kind)?;
    validate_retention_policy(
        &input.kind,
        input.destination.as_deref(),
        input.retention_count,
        input.retention_days,
    )?;
    if !input.confirmed {
        return Err(AppError::new(
            "CONFIRMATION_REQUIRED",
            "cronjob",
            "请先确认计划任务变更",
        ));
    }
    let command = build_task_command_for_server(ssh, &input).await?;
    let id = match input.id {
        Some(value) => {
            if value.starts_with("line-") {
                return Err(AppError::new(
                    "VALIDATION_FAILED",
                    "cronjob",
                    "未托管的系统任务不能被客户端更新",
                ));
            }
            value
        }
        None => Uuid::new_v4().to_string(),
    };
    validate_id(&id)?;
    let enabled = input.enabled;
    let user = input.user.unwrap_or_else(|| "".into());
    validate_user(&user)?;
    let entry = if enabled {
        format!("{} {command}", input.schedule.trim())
    } else {
        format!("# disabled {} {}", input.schedule.trim(), command)
    };
    let retention_marker =
        build_retention_marker(&input.kind, input.retention_count, input.retention_days);
    let backup_accounts_marker = build_backup_accounts_marker(
        &input.kind,
        &input.backup_account_ids,
        input.default_backup_account_id.as_deref(),
    )?;
    let backup_event_path = if let Some(path) = input.backup_event_path.as_deref() {
        validate_backup_path(path, false)?;
        Some(path.to_string())
    } else {
        build_backup_event_path(&input.kind, input.destination.as_deref())
    };
    let script = rewrite_script(
        &id,
        &input.kind,
        &entry,
        &user,
        retention_marker.as_deref(),
        backup_accounts_marker.as_deref(),
        backup_event_path.as_deref(),
        false,
    );
    let result = ssh
        .execute_system(&input.server_id, &script, Duration::from_secs(30))
        .await?;
    if result.exit_code != 0 {
        return Err(
            AppError::new("CRON_SAVE_FAILED", "cronjob", "计划任务保存失败")
                .details(result.stderr)
                .for_server(input.server_id),
        );
    }
    Ok(CronJobActionResult {
        id,
        action: "save".into(),
        output: result.stdout,
    })
}

/// 删除一条由本客户端创建的 crontab 记录；不会删除未标记的系统任务。
pub async fn action(
    ssh: &SshConnectionManager,
    input: CronJobActionInput,
) -> AppResult<CronJobActionResult> {
    if !input.confirmed {
        return Err(AppError::new(
            "CONFIRMATION_REQUIRED",
            "cronjob",
            "请先确认计划任务操作",
        ));
    }
    validate_id(&input.id)?;
    let user = input.user.unwrap_or_else(|| "".into());
    validate_user(&user)?;
    let command = input.command.unwrap_or_default();
    if input.action == "run" {
        validate_command(&command)?;
    }
    let script = match input.action.as_str() {
        "delete" => rewrite_script(&input.id, "shell", "", &user, None, None, None, true),
        "run" => format!("sh -c {}", shell_escape(&command)),
        _ => {
            return Err(AppError::new(
                "VALIDATION_FAILED",
                "cronjob",
                "不支持的计划任务操作",
            ))
        }
    };
    let result = ssh
        .execute_system(
            &input.server_id,
            &script,
            Duration::from_secs(if input.action == "run" { 300 } else { 30 }),
        )
        .await?;
    if result.exit_code != 0 {
        return Err(
            AppError::new("CRON_ACTION_FAILED", "cronjob", "计划任务操作失败")
                .details(result.stderr)
                .for_server(input.server_id),
        );
    }
    Ok(CronJobActionResult {
        id: input.id,
        action: input.action,
        output: result.stdout,
    })
}

/// 从一次成功的备份命令输出中提取固定 marker 的归档路径，不接受任意 Shell 片段。
pub fn extract_backup_artifact_path(output: &str) -> Option<String> {
    output.lines().find_map(|line| {
        let fields = line.split('\t').collect::<Vec<_>>();
        if fields.len() == 3 && fields[0] == "__CRON_BACKUP__" {
            let path = fields[2];
            (path.starts_with('/')
                && !path.contains("..")
                && !path
                    .chars()
                    .any(|value| value.is_control() || value.is_whitespace()))
            .then(|| path.to_string())
        } else {
            None
        }
    })
}

/// 读取指定服务器最近的计划任务执行摘要，历史仅保存在本机 SQLite 设置表。
pub async fn history(
    local: &LocalRepository,
    server_id: &str,
) -> AppResult<Vec<CronJobHistoryEntry>> {
    validate_server_id(server_id)?;
    Ok(local
        .get_setting(&format!("{CRON_HISTORY_PREFIX}{server_id}"))
        .await?
        .and_then(|value| serde_json::from_str::<Vec<CronJobHistoryEntry>>(&value).ok())
        .unwrap_or_default())
}

/// 清除指定服务器的本地计划任务执行历史，不触碰远端 crontab 或任务文件。
pub async fn clear_history(local: &LocalRepository, server_id: &str) -> AppResult<()> {
    validate_server_id(server_id)?;
    local
        .delete_setting(&format!("{CRON_HISTORY_PREFIX}{server_id}"))
        .await
}

/// 记录一次计划任务执行结果并保留最近 200 条脱敏摘要；失败写入不会覆盖远端执行结果。
pub async fn record_history(
    local: &LocalRepository,
    server_id: &str,
    job_id: &str,
    action: &str,
    started_at: chrono::DateTime<chrono::Utc>,
    result: &AppResult<CronJobActionResult>,
) -> AppResult<()> {
    validate_server_id(server_id)?;
    validate_id(job_id)?;
    if !matches!(action, "run" | "offline_sync") {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "cronjob",
            "只允许记录计划任务执行或离线补传历史",
        ));
    }
    let mut entries = history(local, server_id).await?;
    let finished_at = chrono::Utc::now();
    let (success, output) = match result {
        Ok(value) => (true, value.output.clone()),
        Err(error) => (false, error.message.clone()),
    };
    let output = redact(&output).chars().take(4_096).collect::<String>();
    entries.push(CronJobHistoryEntry {
        id: Uuid::new_v4().to_string(),
        job_id: job_id.to_string(),
        action: action.to_string(),
        success,
        output,
        started_at,
        finished_at,
    });
    entries.sort_by_key(|entry| std::cmp::Reverse(entry.finished_at));
    entries.truncate(200);
    let value = serde_json::to_string(&entries).map_err(AppError::database)?;
    if value.len() > 2 * 1024 * 1024 {
        return Err(AppError::new(
            "CRON_HISTORY_TOO_LARGE",
            "cronjob",
            "计划任务执行历史超过本地存储上限",
        ));
    }
    local
        .set_setting(&format!("{CRON_HISTORY_PREFIX}{server_id}"), &value)
        .await
}

/// 读取指定服务器的计划任务通知策略，并探测密钥链中的 webhook 配置状态。
pub async fn notification_settings(
    local: &LocalRepository,
    credentials: &Arc<dyn crate::security::CredentialStore>,
    server_id: &str,
) -> AppResult<CronNotificationSettings> {
    validate_server_id(server_id)?;
    let stored = local
        .get_setting(&format!("{CRON_NOTIFICATION_SETTING_PREFIX}{server_id}"))
        .await?
        .and_then(|value| serde_json::from_str::<StoredCronNotificationSettings>(&value).ok())
        .unwrap_or(StoredCronNotificationSettings {
            notify_in_app: true,
            notify_webhook: false,
            provider: "generic".into(),
        });
    validate_notification_provider(&stored.provider)?;
    Ok(CronNotificationSettings {
        server_id: server_id.to_string(),
        notify_in_app: stored.notify_in_app,
        notify_webhook: stored.notify_webhook,
        provider: stored.provider,
        webhook_configured: credentials
            .get(&notification_webhook_key(server_id))
            .is_ok(),
        signing_secret_configured: credentials
            .get(&notification_signing_key(server_id))
            .is_ok(),
    })
}

/// 保存计划任务通知策略；通知 URL 与钉钉签名密钥只写入操作系统密钥链。
pub async fn save_notification_settings(
    local: &LocalRepository,
    credentials: &Arc<dyn crate::security::CredentialStore>,
    input: SaveCronNotificationSettingsInput,
) -> AppResult<CronNotificationSettings> {
    validate_server_id(&input.server_id)?;
    validate_notification_provider(&input.provider)?;
    if !input.confirmed {
        return Err(AppError::new(
            "CONFIRMATION_REQUIRED",
            "cronjob",
            "请先确认保存计划任务通知设置",
        ));
    }
    if let Some(url) = input.webhook_url.as_ref() {
        validate_notification_url(url.expose_secret())?;
        credentials.put(
            &notification_webhook_key(&input.server_id),
            SecretString::from(url.expose_secret().trim().to_string()),
        )?;
    } else if input.clear_webhook {
        credentials.delete(&notification_webhook_key(&input.server_id))?;
    }
    if let Some(secret) = input.webhook_signing_secret.as_ref() {
        validate_signing_secret(secret.expose_secret())?;
        credentials.put(
            &notification_signing_key(&input.server_id),
            SecretString::from(secret.expose_secret().trim().to_string()),
        )?;
    } else if input.clear_signing_secret {
        credentials.delete(&notification_signing_key(&input.server_id))?;
    }
    let stored = StoredCronNotificationSettings {
        notify_in_app: input.notify_in_app,
        notify_webhook: input.notify_webhook,
        provider: input.provider,
    };
    let value = serde_json::to_string(&stored).map_err(AppError::database)?;
    local
        .set_setting(
            &format!("{CRON_NOTIFICATION_SETTING_PREFIX}{}", input.server_id),
            &value,
        )
        .await?;
    let result = notification_settings(local, credentials, &input.server_id).await?;
    if result.notify_webhook && !result.webhook_configured {
        return Err(AppError::new(
            "CRON_NOTIFICATION_SECRET_MISSING",
            "cronjob",
            "启用外部通知前必须配置 webhook URL",
        ));
    }
    Ok(result)
}

/// 发送一次计划任务执行报告；通知失败不会改变远端任务本身的成功状态。
pub async fn send_execution_report(
    local: &LocalRepository,
    credentials: &Arc<dyn crate::security::CredentialStore>,
    server_id: &str,
    job_id: &str,
    result: &AppResult<CronJobActionResult>,
) -> AppResult<()> {
    let settings = notification_settings(local, credentials, server_id).await?;
    if !settings.notify_webhook {
        return Ok(());
    }
    let url = credentials
        .get(&notification_webhook_key(server_id))
        .map_err(|_| {
            AppError::new(
                "CRON_NOTIFICATION_SECRET_MISSING",
                "cronjob",
                "计划任务 webhook URL 不可用",
            )
        })?;
    let signing_secret = credentials.get(&notification_signing_key(server_id)).ok();
    let (success, output) = match result {
        Ok(value) => (true, value.output.clone()),
        Err(error) => (false, error.message.clone()),
    };
    let message = format!(
        "[1Panel Client] 计划任务 {} · 服务器 {} · {}\n{}",
        job_id,
        server_id,
        if success { "成功" } else { "失败" },
        redact(&output).chars().take(2_000).collect::<String>()
    );
    let payload = match settings.provider.as_str() {
        "slack" => serde_json::json!({"text": message}),
        "discord" => serde_json::json!({"content": message}),
        "dingtalk" | "wecom" => {
            serde_json::json!({"msgtype": "text", "text": {"content": message}})
        }
        _ => {
            serde_json::json!({"event": "cronjob_report", "serverId": server_id, "jobId": job_id, "success": success, "generatedAt": chrono::Utc::now(), "output": redact(&output).chars().take(2_000).collect::<String>()})
        }
    };
    let target = notification_target_url(
        url.expose_secret(),
        &settings.provider,
        signing_secret.as_ref(),
    )?;
    let response = Client::builder()
        .timeout(Duration::from_secs(8))
        .build()
        .map_err(|error| {
            AppError::new(
                "CRON_NOTIFICATION_FAILED",
                "cronjob",
                "无法初始化通知客户端",
            )
            .details(error)
        })?
        .post(target)
        .header("User-Agent", "1panel-client")
        .json(&payload)
        .send()
        .await
        .map_err(|error| {
            AppError::new(
                "CRON_NOTIFICATION_FAILED",
                "cronjob",
                "计划任务通知请求失败",
            )
            .details(error)
        })?;
    if !response.status().is_success() {
        return Err(AppError::new(
            "CRON_NOTIFICATION_FAILED",
            "cronjob",
            "计划任务通知返回非成功状态",
        )
        .details(format!("HTTP {}", response.status().as_u16())));
    }
    Ok(())
}

/// 读取客户端离线归档补传调度器设置；首次使用默认开启以保留计划任务的备份语义。
pub async fn offline_scheduler_settings(
    local: &LocalRepository,
) -> AppResult<CronOfflineSchedulerSettings> {
    let enabled = local
        .get_setting(CRON_OFFLINE_SCHEDULER_SETTING_KEY)
        .await?
        .and_then(|value| serde_json::from_str::<bool>(&value).ok())
        .unwrap_or(true);
    Ok(CronOfflineSchedulerSettings { enabled })
}

/// 保存客户端离线归档补传调度器设置；只写本地布尔值，不触碰服务器。
pub async fn save_offline_scheduler_settings(
    local: &LocalRepository,
    input: SaveCronOfflineSchedulerSettingsInput,
) -> AppResult<CronOfflineSchedulerSettings> {
    if !input.confirmed {
        return Err(AppError::new(
            "CONFIRMATION_REQUIRED",
            "cronjob",
            "请先确认保存离线归档补传设置",
        ));
    }
    let value = serde_json::to_string(&input.enabled).map_err(AppError::database)?;
    local
        .set_setting(CRON_OFFLINE_SCHEDULER_SETTING_KEY, &value)
        .await?;
    offline_scheduler_settings(local).await
}

/// 启动本地离线归档补传循环；任务只在客户端在线时读取远端事件并复用备份账号上传链路。
pub fn spawn_offline_scheduler(
    servers: ServerRepository,
    ssh: SshConnectionManager,
    local: LocalRepository,
    credentials: Arc<dyn crate::security::CredentialStore>,
) {
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(20)).await;
        let mut ticker = tokio::time::interval(Duration::from_secs(60));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            ticker.tick().await;
            let settings = match offline_scheduler_settings(&local).await {
                Ok(value) => value,
                Err(error) => {
                    tracing::warn!(error = %error, "读取离线归档补传设置失败");
                    continue;
                }
            };
            if !settings.enabled {
                continue;
            }
            let profiles = match servers.list().await {
                Ok(value) => value,
                Err(error) => {
                    tracing::warn!(error = %error, "读取服务器列表以同步离线归档失败");
                    continue;
                }
            };
            for profile in profiles {
                if let Err(error) =
                    sync_offline_backups(&ssh, &local, &credentials, &profile.id).await
                {
                    tracing::debug!(
                        error = %error,
                        server_id = %profile.id,
                        "离线归档补传跳过服务器"
                    );
                }
            }
        }
    });
}

/// 扫描一台服务器的客户端归档任务，并对每个任务独立处理事件，避免单个账号失败阻断其它任务。
async fn sync_offline_backups(
    ssh: &SshConnectionManager,
    local: &LocalRepository,
    credentials: &Arc<dyn crate::security::CredentialStore>,
    server_id: &str,
) -> AppResult<()> {
    let snapshot = snapshot(ssh, server_id).await?;
    for job in snapshot.jobs.into_iter().filter(|job| {
        job.managed
            && matches!(
                job.kind.as_str(),
                "directory" | "database" | "log" | "website" | "app"
            )
            && !job.backup_account_ids.is_empty()
            && job.backup_event_path.is_some()
    }) {
        if let Err(error) = sync_offline_job(ssh, local, credentials, server_id, &job).await {
            tracing::debug!(
                error = %error,
                server_id,
                job_id = %job.id,
                "离线归档任务同步失败"
            );
        }
    }
    Ok(())
}

/// 读取一个归档事件文件，为每个选中账号执行一次幂等状态跟踪的补传。
async fn sync_offline_job(
    ssh: &SshConnectionManager,
    local: &LocalRepository,
    credentials: &Arc<dyn crate::security::CredentialStore>,
    server_id: &str,
    job: &CronJob,
) -> AppResult<()> {
    let event_path = job.backup_event_path.as_deref().ok_or_else(|| {
        AppError::new(
            "CRON_OFFLINE_EVENT_MISSING",
            "cronjob",
            "计划任务缺少离线归档事件路径",
        )
    })?;
    validate_backup_path(event_path, false)?;
    let command = format!(
        "tail -n 100 -- {} 2>/dev/null || true",
        shell_escape(event_path)
    );
    let result = ssh
        .execute_system(server_id, &command, Duration::from_secs(30))
        .await?;
    if result.exit_code != 0 {
        return Ok(());
    }
    let mut events = Vec::new();
    for event in result
        .stdout
        .lines()
        .filter_map(parse_backup_event_line)
        .filter(|event| event.kind == job.kind && event_matches_job(event, event_path))
    {
        if !events
            .iter()
            .any(|value: &OfflineBackupEvent| value.key == event.key)
        {
            events.push(event);
        }
    }
    if events.is_empty() {
        return Ok(());
    }
    let state_key = offline_state_key(server_id, &job.id);
    let mut states = load_offline_states(local, &state_key).await?;
    let account_ids = unique_account_ids(&job.backup_account_ids);
    for event in events {
        let state =
            if let Some(value) = states.iter_mut().find(|value| value.event.key == event.key) {
                value
            } else {
                states.push(OfflineBackupEventState {
                    event: event.clone(),
                    ..OfflineBackupEventState::default()
                });
                states.last_mut().expect("just-pushed offline state")
            };
        if state.success_notified {
            continue;
        }
        let mut output = format!("offline event {}", state.event.timestamp);
        let mut all_uploaded = true;
        for account_id in &account_ids {
            if state
                .uploaded_account_ids
                .iter()
                .any(|value| value == account_id)
            {
                continue;
            }
            match crate::domain::backup_accounts::upload(
                ssh,
                local,
                credentials,
                crate::domain::backup_accounts::UploadBackupInput {
                    server_id: server_id.to_string(),
                    account_id: account_id.clone(),
                    remote_path: state.event.remote_path.clone(),
                    confirmed: true,
                },
            )
            .await
            {
                Ok(upload) => {
                    state.uploaded_account_ids.push(account_id.clone());
                    output.push_str(&format!(
                        "\n[BACKUP_UPLOAD_OK]\t{}\t{} bytes",
                        upload.kind, upload.bytes
                    ));
                }
                Err(error) => {
                    all_uploaded = false;
                    output.push_str(&format!(
                        "\n[BACKUP_UPLOAD_FAILED]\t{}",
                        redact(&error.message)
                    ));
                    if !state.failure_notified {
                        let report = Err(error.clone());
                        if let Err(report_error) =
                            send_execution_report(local, credentials, server_id, &job.id, &report)
                                .await
                        {
                            tracing::debug!(
                                error = %report_error,
                                server_id,
                                job_id = %job.id,
                                "离线归档失败通知发送失败"
                            );
                        } else {
                            state.failure_notified = true;
                        }
                    }
                }
            }
        }
        if all_uploaded {
            let action_result = Ok(CronJobActionResult {
                id: job.id.clone(),
                action: "offline_sync".into(),
                output,
            });
            if !state.success_notified {
                if let Err(report_error) =
                    send_execution_report(local, credentials, server_id, &job.id, &action_result)
                        .await
                {
                    tracing::debug!(
                        error = %report_error,
                        server_id,
                        job_id = %job.id,
                        "离线归档成功通知发送失败"
                    );
                } else {
                    let started_at = chrono::Utc::now();
                    if let Err(history_error) = record_history(
                        local,
                        server_id,
                        &job.id,
                        "offline_sync",
                        started_at,
                        &action_result,
                    )
                    .await
                    {
                        tracing::debug!(
                            error = %history_error,
                            server_id,
                            job_id = %job.id,
                            "离线归档历史写入失败"
                        );
                    }
                    state.success_notified = true;
                }
            }
        }
        save_offline_states(local, &state_key, &states).await?;
    }
    trim_offline_states(&mut states);
    save_offline_states(local, &state_key, &states).await
}

/// 将事件路径与任务 marker 的前缀绑定，防止被篡改的事件文件上传任意绝对路径。
fn event_matches_job(event: &OfflineBackupEvent, event_path: &str) -> bool {
    let Some(prefix) = event_path.strip_suffix(".1panel-client-events") else {
        return false;
    };
    let Some(event_name) = event.remote_path.rsplit('/').next() else {
        return false;
    };
    let Some(prefix_name) = prefix.rsplit('/').next() else {
        return false;
    };
    let Some(parent) = event_path.rsplit_once('/') else {
        return false;
    };
    event
        .remote_path
        .rsplit_once('/')
        .is_some_and(|(event_parent, _)| event_parent == parent.0)
        && (event_name == prefix_name
            || event_name.starts_with(&format!("{prefix_name}-"))
            || event_name.starts_with(&format!("{prefix_name}.")))
}

/// 生成离线归档事件的本地状态键；服务器和任务 ID 均来自本地数据库或已校验 marker。
fn offline_state_key(server_id: &str, job_id: &str) -> String {
    format!("{CRON_OFFLINE_STATE_PREFIX}{server_id}.{job_id}")
}

/// 读取一次任务的离线上传状态，旧版本或损坏设置按空状态处理并在本次运行修复。
async fn load_offline_states(
    local: &LocalRepository,
    key: &str,
) -> AppResult<Vec<OfflineBackupEventState>> {
    Ok(local
        .get_setting(key)
        .await?
        .and_then(|value| serde_json::from_str::<Vec<OfflineBackupEventState>>(&value).ok())
        .unwrap_or_default())
}

/// 持久化有界离线上传状态；只包含时间、路径和账号 UUID，不包含账号凭据。
async fn save_offline_states(
    local: &LocalRepository,
    key: &str,
    states: &[OfflineBackupEventState],
) -> AppResult<()> {
    let value = serde_json::to_string(states).map_err(AppError::database)?;
    local.set_setting(key, &value).await
}

/// 去除已经完成的最旧事件，避免客户端长时间运行导致本地设置无限增长。
fn trim_offline_states(states: &mut Vec<OfflineBackupEventState>) {
    while states.len() > 200 {
        let index = states
            .iter()
            .position(|value| value.success_notified)
            .unwrap_or(0);
        states.remove(index);
    }
}

/// 保持备份账号顺序并去重，避免导入文件中的重复 UUID 造成重复上传。
fn unique_account_ids(values: &[String]) -> Vec<String> {
    let mut result = Vec::new();
    for value in values {
        if !result.iter().any(|item| item == value) {
            result.push(value.clone());
        }
    }
    result
}

/// 校验通知渠道白名单，避免把任意 HTTP 请求伪装成报告通知。
fn validate_notification_provider(value: &str) -> AppResult<()> {
    if matches!(
        value,
        "generic" | "slack" | "discord" | "dingtalk" | "wecom"
    ) {
        Ok(())
    } else {
        Err(AppError::new(
            "VALIDATION_FAILED",
            "cronjob",
            "计划任务通知渠道无效",
        ))
    }
}

/// 校验通知 URL 必须是带主机的 HTTP(S) 地址，不允许查询参数注入凭据。
fn validate_notification_url(value: &str) -> AppResult<()> {
    let parsed = reqwest::Url::parse(value)
        .map_err(|_| AppError::new("VALIDATION_FAILED", "cronjob", "计划任务 webhook URL 无效"))?;
    if !matches!(parsed.scheme(), "http" | "https")
        || parsed.host_str().is_none()
        || value.chars().any(|character| character.is_control())
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "cronjob",
            "计划任务 webhook URL 无效",
        ));
    }
    Ok(())
}

/// 校验可选钉钉签名密钥，禁止空白和控制字符。
fn validate_signing_secret(value: &str) -> AppResult<()> {
    if value.is_empty()
        || value.len() > 512
        || value
            .chars()
            .any(|character| character.is_whitespace() || character.is_control())
    {
        Err(AppError::new(
            "VALIDATION_FAILED",
            "cronjob",
            "计划任务通知签名密钥无效",
        ))
    } else {
        Ok(())
    }
}

/// 生成钉钉签名 URL；其它渠道保持用户提供的 URL 不变。
fn notification_target_url(
    value: &str,
    provider: &str,
    signing_secret: Option<&SecretString>,
) -> AppResult<String> {
    let Some(secret) = signing_secret.filter(|_| provider == "dingtalk") else {
        return Ok(value.to_string());
    };
    let timestamp = chrono::Utc::now().timestamp_millis();
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.expose_secret().as_bytes())
        .map_err(|_| AppError::new("CRON_NOTIFICATION_FAILED", "cronjob", "无法初始化通知签名"))?;
    mac.update(format!("{timestamp}\n{}", secret.expose_secret()).as_bytes());
    let sign = BASE64.encode(mac.finalize().into_bytes());
    let mut parsed = reqwest::Url::parse(value)
        .map_err(|_| AppError::new("CRON_NOTIFICATION_FAILED", "cronjob", "通知 URL 无效"))?;
    parsed
        .query_pairs_mut()
        .append_pair("timestamp", &timestamp.to_string())
        .append_pair("sign", &sign);
    Ok(parsed.to_string())
}

/// 生成计划任务通知 URL 的密钥链引用，不包含服务器地址或 secret。
fn notification_webhook_key(server_id: &str) -> String {
    format!("{CRON_NOTIFICATION_WEBHOOK_PREFIX}{server_id}")
}

/// 生成计划任务通知签名密钥链引用。
fn notification_signing_key(server_id: &str) -> String {
    format!("{CRON_NOTIFICATION_SIGNING_PREFIX}{server_id}")
}

/// 生成只读 crontab/timer 探测脚本；任务命令不会被执行。
fn probe_command() -> String {
    r#"printf '__USER__\t%s\n' "$(id -un)"
printf '__CRON__\n'
marker=''
kind_marker=''
retention_marker=''
backup_accounts_marker=''
backup_event_marker=''
crontab -l 2>/dev/null | while IFS= read -r line; do
  case "$line" in
    '# 1panel-client-cron:'*) marker="${line#\# 1panel-client-cron:}"; continue ;;
    '# 1panel-client-cron-kind:'*) kind_marker="${line#\# 1panel-client-cron-kind:}"; continue ;;
    '# 1panel-client-cron-retention:'*) retention_marker="${line#\# 1panel-client-cron-retention:}"; continue ;;
    '# 1panel-client-cron-backup-accounts:'*) backup_accounts_marker="${line#\# 1panel-client-cron-backup-accounts:}"; continue ;;
    '# 1panel-client-cron-backup-event:'*) backup_event_marker="${line#\# 1panel-client-cron-backup-event:}"; continue ;;
    ''|'#'|'# '*) continue ;;
  esac
  schedule=$(printf '%s\n' "$line" | awk '{print $1" "$2" "$3" "$4" "$5}')
  command=$(printf '%s\n' "$line" | cut -d' ' -f6-)
  [ -n "$command" ] || continue
  if [ -n "$marker" ]; then id="$marker"; managed=1; else id="line-$(printf '%s' "$line" | cksum | awk '{print $1}')"; managed=0; fi
  printf '__JOB__\t%s\t%s\t%s\t%s\t%s\t1\t%s\t%s\t%s\t%s\n' "$id" "$schedule" "$command" "$(id -un)" "$managed" "$kind_marker" "$retention_marker" "$backup_accounts_marker" "$backup_event_marker"
  marker=''
  kind_marker=''
  retention_marker=''
  backup_accounts_marker=''
  backup_event_marker=''
done
printf '__TIMERS__\n'
if command -v systemctl >/dev/null 2>&1; then systemctl list-timers --all --no-legend --no-pager 2>/dev/null | awk 'NF >= 5 {print "__TIMER__\t" $1 "\t" $2 " " $3 "\t" $4 " " $5 "\t" $NF}'; fi"#.into()
}

/// 用临时文件原子地移除旧 marker 并写入新的 crontab 记录。
#[allow(clippy::too_many_arguments)]
fn rewrite_script(
    id: &str,
    kind: &str,
    entry: &str,
    user: &str,
    retention_marker: Option<&str>,
    backup_accounts_marker: Option<&str>,
    backup_event_path: Option<&str>,
    remove_only: bool,
) -> String {
    let target = shell_escape(&format!("# 1panel-client-cron:{id}"));
    let marker = shell_escape(&format!("# 1panel-client-cron:{id}"));
    let kind_marker = shell_escape(&format!("{CRON_KIND_MARKER_PREFIX}{kind}"));
    let line = shell_escape(entry);
    let user_arg = if user.is_empty() {
        String::new()
    } else {
        format!(" -u {}", shell_escape(user))
    };
    let retention = retention_marker
        .map(|value| format!("printf '%s\\n' {} >> \"$tmp\"; ", shell_escape(value)))
        .unwrap_or_default();
    let backup_accounts = backup_accounts_marker
        .map(|value| format!("printf '%s\\n' {} >> \"$tmp\"; ", shell_escape(value)))
        .unwrap_or_default();
    let backup_event = backup_event_path
        .map(|value| {
            format!(
                "printf '%s\\n' {} >> \"$tmp\"; ",
                shell_escape(&format!("{CRON_BACKUP_EVENT_MARKER_PREFIX}{value}"))
            )
        })
        .unwrap_or_default();
    let append = if remove_only {
        String::new()
    } else {
        format!(
            "printf '%s\\n%s\\n' {marker} {kind_marker} >> \"$tmp\"; {retention}{backup_accounts}{backup_event}printf '%s\\n' {line} >> \"$tmp\"; ",
            marker = marker,
            kind_marker = kind_marker,
            retention = retention,
            backup_accounts = backup_accounts,
            backup_event = backup_event,
            line = line,
        )
    };
    format!("tmp=$(mktemp); crontab{user_arg} -l 2>/dev/null | awk -v target={target} 'BEGIN{{skip=0}} $0 == target {{skip=1; next}} skip && $0 !~ /^#/ {{skip=0; next}} {{print}}' > \"$tmp\"; {append} crontab{user_arg} \"$tmp\"; status=$?; rm -f \"$tmp\"; exit $status")
}

/// 解析探测 marker，并兼容没有任何 crontab 或 timer 的服务器。
fn parse_snapshot(output: &str) -> Option<CronSnapshot> {
    let mut user = None;
    let mut jobs = Vec::new();
    let mut timers = Vec::new();
    for line in output.lines() {
        let fields = line.split('\t').collect::<Vec<_>>();
        match fields.first().copied() {
            Some("__USER__") => user = fields.get(1).map(|v| (*v).to_string()),
            Some("__JOB__") if fields.len() >= 6 => {
                let kind = fields
                    .get(7)
                    .filter(|value| is_supported_kind(value))
                    .map(|value| (*value).to_string())
                    .unwrap_or_else(|| classify_cron_command(fields[3]));
                let (retention_count, retention_days) = fields
                    .get(8)
                    .and_then(|value| parse_retention_marker(value))
                    .unwrap_or((None, None));
                let (backup_account_ids, default_backup_account_id) = fields
                    .get(9)
                    .and_then(|value| parse_backup_accounts_marker(value))
                    .unwrap_or_default();
                let backup_event_path = fields
                    .get(10)
                    .and_then(|value| parse_backup_event_marker(value));
                jobs.push(CronJob {
                    id: fields[1].into(),
                    schedule: fields[2].into(),
                    command: fields[3].into(),
                    kind,
                    user: fields[4].into(),
                    managed: fields[5] == "1",
                    enabled: fields.get(6).map(|v| *v != "0").unwrap_or(true),
                    retention_count,
                    retention_days,
                    backup_account_ids,
                    default_backup_account_id,
                    backup_event_path,
                });
            }
            Some("__TIMER__") if fields.len() >= 5 => timers.push(CronTimer {
                name: fields[1].into(),
                next_run: fields[2].into(),
                last_run: fields[3].into(),
                activates: fields[4].into(),
            }),
            _ => {}
        }
    }
    user.map(|user| CronSnapshot {
        user,
        jobs,
        timers,
        fetched_at: chrono::Utc::now(),
    })
}

/// 校验五段 cron 表达式，保留常用字符并禁止换行和 shell 控制字符。
fn validate_schedule(value: &str) -> AppResult<()> {
    let fields = value.split_whitespace().collect::<Vec<_>>();
    if fields.len() != 5
        || fields.iter().any(|field| {
            field.is_empty()
                || field.len() > 32
                || !field
                    .bytes()
                    .all(|b| b.is_ascii_digit() || matches!(b, b'*' | b'/' | b',' | b'-' | b'?'))
        })
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "cronjob",
            "计划表达式必须是五段合法 cron 语法",
        ));
    }
    Ok(())
}

/// 校验计划任务命令不包含换行，避免越权追加额外 crontab 行。
fn validate_command(value: &str) -> AppResult<()> {
    if value.trim().is_empty()
        || value.len() > 8_192
        || value.contains('\n')
        || value.contains('\r')
    {
        Err(AppError::new(
            "VALIDATION_FAILED",
            "cronjob",
            "计划任务命令不能为空、不能超过 8192 字节或包含换行",
        ))
    } else {
        Ok(())
    }
}

/// 校验导入条目的通用字段；导入端不信任文件中的远端 ID 或任务类型。
fn validate_import_entry(entry: &CronJobExportEntry) -> AppResult<()> {
    validate_schedule(&entry.schedule)?;
    validate_command(&entry.command)?;
    validate_user(&entry.user)
}

/// 校验计划任务类型；类型决定远端命令模板，不能由用户自定义。
fn validate_kind(value: &str) -> AppResult<()> {
    if is_supported_kind(value) {
        Ok(())
    } else {
        Err(AppError::new(
            "VALIDATION_FAILED",
            "cronjob",
            "计划任务类型无效",
        ))
    }
}

/// 判断类型 marker 是否属于客户端支持的固定任务集合。
fn is_supported_kind(value: &str) -> bool {
    matches!(
        value,
        "shell" | "url" | "directory" | "database" | "log" | "website" | "app"
    )
}

/// 校验归档任务的保留策略，并要求开启策略时目标文件名可安全用于固定 glob 清理。
fn validate_retention_policy(
    kind: &str,
    destination: Option<&str>,
    retention_count: Option<u32>,
    retention_days: Option<u32>,
) -> AppResult<()> {
    if retention_count.is_none() && retention_days.is_none() {
        return Ok(());
    }
    if !matches!(kind, "directory" | "database" | "log" | "website" | "app") {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "cronjob",
            "只有归档类任务支持保留策略",
        ));
    }
    if retention_count.is_some_and(|value| !(1..=1_000).contains(&value))
        || retention_days.is_some_and(|value| !(1..=3_650).contains(&value))
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "cronjob",
            "保留份数必须为 1-1000，保留天数必须为 1-3650",
        ));
    }
    let destination = destination.ok_or_else(|| {
        AppError::new(
            "VALIDATION_FAILED",
            "cronjob",
            "启用保留策略时必须指定备份目标",
        )
    })?;
    validate_retention_destination(destination)
}

/// 校验启用轮换时的目标文件名，避免用户输入变成 find 的 glob 或选项。
fn validate_retention_destination(value: &str) -> AppResult<()> {
    validate_backup_path(value, false)?;
    let file_name = value.rsplit('/').next().unwrap_or_default();
    if file_name.is_empty()
        || file_name.len() > 256
        || !file_name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "cronjob",
            "启用保留策略时目标文件名只能包含字母、数字、点、下划线和短横线",
        ));
    }
    Ok(())
}

/// 生成写入 crontab 的保留策略 marker；marker 不包含凭据或任意 shell 片段。
fn build_retention_marker(
    kind: &str,
    retention_count: Option<u32>,
    retention_days: Option<u32>,
) -> Option<String> {
    if retention_count.is_none() && retention_days.is_none() {
        return None;
    }
    matches!(kind, "directory" | "database" | "log" | "website" | "app").then_some(format!(
        "{CRON_RETENTION_MARKER_PREFIX}count={};days={}",
        retention_count.unwrap_or(0),
        retention_days.unwrap_or(0)
    ))
}

/// 解析远端 marker 中的保留份数和天数，格式异常时按未配置处理。
fn parse_retention_marker(value: &str) -> Option<(Option<u32>, Option<u32>)> {
    let marker = value
        .strip_prefix(CRON_RETENTION_MARKER_PREFIX)
        .unwrap_or(value);
    let mut count = None;
    let mut days = None;
    for pair in marker.split(';') {
        let (key, raw) = pair.split_once('=')?;
        let parsed = raw.parse::<u32>().ok()?;
        match key {
            "count" if (1..=1_000).contains(&parsed) => count = Some(parsed),
            "days" if (1..=3_650).contains(&parsed) => days = Some(parsed),
            "count" | "days" if parsed == 0 => {}
            _ => return None,
        }
    }
    (count.is_some() || days.is_some()).then_some((count, days))
}

/// 构造归档上传账号 marker；只保存 UUID 引用，不把 endpoint 或凭据写入远端 crontab。
fn build_backup_accounts_marker(
    kind: &str,
    account_ids: &[String],
    default_account_id: Option<&str>,
) -> AppResult<Option<String>> {
    if !matches!(kind, "directory" | "database" | "log" | "website" | "app") {
        if account_ids.is_empty() && default_account_id.is_none() {
            return Ok(None);
        }
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "cronjob",
            "只有归档类任务支持外部备份账号",
        ));
    }
    if account_ids.is_empty() {
        if default_account_id.is_some() {
            return Err(AppError::new(
                "VALIDATION_FAILED",
                "cronjob",
                "默认备份账号必须包含在账号列表中",
            ));
        }
        return Ok(None);
    }
    if account_ids.len() > 8 {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "cronjob",
            "单个计划任务最多选择 8 个备份账号",
        ));
    }
    let mut normalized = Vec::with_capacity(account_ids.len());
    for value in account_ids {
        validate_uuid(value)?;
        if normalized.iter().any(|item| item == value) {
            return Err(AppError::new(
                "VALIDATION_FAILED",
                "cronjob",
                "备份账号不能重复选择",
            ));
        }
        normalized.push(value.clone());
    }
    if let Some(default_id) = default_account_id {
        validate_uuid(default_id)?;
        if !normalized.iter().any(|value| value == default_id) {
            return Err(AppError::new(
                "VALIDATION_FAILED",
                "cronjob",
                "默认备份账号必须包含在账号列表中",
            ));
        }
    }
    Ok(Some(format!(
        "{CRON_BACKUP_ACCOUNTS_MARKER_PREFIX}ids={};default={}",
        normalized.join(","),
        default_account_id.unwrap_or_default()
    )))
}

/// 解析远端上传账号 marker；未知字段或非法 UUID 会让任务按未配置处理。
fn parse_backup_accounts_marker(value: &str) -> Option<(Vec<String>, Option<String>)> {
    let marker = value
        .strip_prefix(CRON_BACKUP_ACCOUNTS_MARKER_PREFIX)
        .unwrap_or(value);
    let mut ids = None;
    let mut default = None;
    for pair in marker.split(';') {
        let (key, raw) = pair.split_once('=')?;
        match key {
            "ids" => {
                let values = raw
                    .split(',')
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
                    .collect::<Vec<_>>();
                if values.is_empty() || values.len() > 8 {
                    return None;
                }
                for value in &values {
                    validate_uuid(value).ok()?;
                }
                ids = Some(values);
            }
            "default" if raw.is_empty() => {}
            "default" => {
                validate_uuid(raw).ok()?;
                default = Some(raw.to_string());
            }
            _ => return None,
        }
    }
    let ids = ids?;
    if default
        .as_deref()
        .is_some_and(|value| !ids.iter().any(|item| item == value))
    {
        return None;
    }
    Some((ids, default))
}

/// 将备份目标映射为固定的远端事件文件路径，事件文件与归档位于同一目录。
fn build_backup_event_path(kind: &str, destination: Option<&str>) -> Option<String> {
    if !matches!(kind, "directory" | "database" | "log" | "website" | "app") {
        return None;
    }
    let destination = destination?;
    validate_backup_path(destination, false).ok()?;
    let (prefix, _) = retained_name_parts(destination);
    let path = format!("{prefix}.1panel-client-events");
    validate_backup_path(&path, false).ok()?;
    Some(path)
}

/// 解析远端事件 marker；异常路径按未启用离线同步处理。
fn parse_backup_event_marker(value: &str) -> Option<String> {
    let path = value
        .strip_prefix(CRON_BACKUP_EVENT_MARKER_PREFIX)
        .unwrap_or(value)
        .trim();
    validate_backup_path(path, false).ok()?;
    Some(path.to_string())
}

/// 解析备份事件行，只接受客户端生成的 UTC 时间、类型和绝对归档路径。
fn parse_backup_event_line(value: &str) -> Option<OfflineBackupEvent> {
    let fields = value.split('\t').collect::<Vec<_>>();
    let timestamp = fields
        .first()
        .map(|value| value.as_bytes())
        .unwrap_or_default();
    if fields.len() != 3
        || timestamp.len() != 16
        || timestamp.get(8) != Some(&b'T')
        || timestamp.last() != Some(&b'Z')
        || !timestamp[..8].iter().all(u8::is_ascii_digit)
        || !timestamp[9..15].iter().all(u8::is_ascii_digit)
        || !matches!(
            fields[1],
            "directory" | "database" | "log" | "website" | "app"
        )
    {
        return None;
    }
    validate_backup_path(fields[2], false).ok()?;
    Some(OfflineBackupEvent {
        key: format!("{}\t{}", fields[0], fields[2]),
        timestamp: fields[0].to_string(),
        kind: fields[1].to_string(),
        remote_path: fields[2].to_string(),
    })
}

/// 描述一次远端 crontab 已完成的归档事件；不包含任何账号凭据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
struct OfflineBackupEvent {
    key: String,
    timestamp: String,
    kind: String,
    remote_path: String,
}

/// 记录某次离线归档已完成的账号上传和通知状态，支持失败重试而避免重复成功上传。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct OfflineBackupEventState {
    event: OfflineBackupEvent,
    uploaded_account_ids: Vec<String>,
    failure_notified: bool,
    success_notified: bool,
}

/// 校验上传账号引用只包含 UUID，避免 marker 被当作命令片段。
fn validate_uuid(value: &str) -> AppResult<()> {
    Uuid::parse_str(value)
        .map(|_| ())
        .map_err(|_| AppError::new("VALIDATION_FAILED", "cronjob", "备份账号引用无效"))
}

/// 按任务类型生成固定远端命令，并在归档类任务上可选启用保留策略。
fn build_task_command_with_policy(
    input: &SaveCronJobInput,
    retention_count: Option<u32>,
    retention_days: Option<u32>,
) -> AppResult<String> {
    match input.kind.as_str() {
        "shell" => {
            validate_command(&input.command)?;
            Ok(input.command.trim().to_string())
        }
        "url" => Ok(build_url_command(&normalize_urls(&input.urls)?)),
        "directory" => build_archive_command_with_policy(
            "directory",
            &input.source_paths,
            input.destination.as_deref(),
            &input.exclude_paths,
            retention_count,
            retention_days,
        ),
        "database" => build_database_backup_command_with_policy(
            input.database_engine.as_deref(),
            input.database_name.as_deref(),
            input.destination.as_deref(),
            retention_count,
            retention_days,
        ),
        "log" => build_archive_command_with_policy(
            "log",
            &["/var/log".into()],
            input.destination.as_deref(),
            &[],
            retention_count,
            retention_days,
        ),
        "website" | "app" => Err(AppError::new(
            "VALIDATION_FAILED",
            "cronjob",
            "网站或应用备份必须先选择远端对象",
        )),
        _ => Err(AppError::new(
            "VALIDATION_FAILED",
            "cronjob",
            "计划任务类型无效",
        )),
    }
}

/// 根据远端真实网站/应用快照解析备份源，再调用固定归档模板生成 crontab 命令。
async fn build_task_command_for_server(
    ssh: &SshConnectionManager,
    input: &SaveCronJobInput,
) -> AppResult<String> {
    if input.preserve_command {
        validate_command(&input.command)?;
        return Ok(input.command.trim().to_string());
    }
    match input.kind.as_str() {
        "website" => {
            let domain = input.website_domain.as_deref().ok_or_else(|| {
                AppError::new("VALIDATION_FAILED", "cronjob", "网站备份必须选择网站")
            })?;
            validate_selection_value(domain, "网站域名")?;
            let snapshot = crate::domain::website::snapshot(ssh, &input.server_id).await?;
            let website = snapshot
                .websites
                .iter()
                .find(|website| website.domain == domain)
                .ok_or_else(|| {
                    AppError::new(
                        "VALIDATION_FAILED",
                        "cronjob",
                        "所选网站不存在或不再受客户端管理",
                    )
                })?;
            let mapped_root = website
                .root_path
                .as_deref()
                .map(|root| map_website_runtime_root(&snapshot, root))
                .transpose()?;
            build_website_backup_command_with_policy(
                mapped_root.as_deref(),
                &website.config_path,
                input.destination.as_deref(),
                &input.exclude_paths,
                input.retention_count,
                input.retention_days,
            )
        }
        "app" => {
            let install_path = input.app_install_path.as_deref().ok_or_else(|| {
                AppError::new("VALIDATION_FAILED", "cronjob", "应用备份必须选择已安装应用")
            })?;
            validate_backup_path(install_path, false)?;
            let snapshot = crate::domain::appstore::installed(ssh, &input.server_id).await?;
            let app = snapshot
                .apps
                .iter()
                .find(|app| app.path == install_path)
                .ok_or_else(|| {
                    AppError::new(
                        "VALIDATION_FAILED",
                        "cronjob",
                        "所选应用不存在或不在固定应用目录内",
                    )
                })?;
            build_app_backup_command_with_policy(
                &app.path,
                input.destination.as_deref(),
                &input.exclude_paths,
                input.retention_count,
                input.retention_days,
            )
        }
        _ => build_task_command_with_policy(input, input.retention_count, input.retention_days),
    }
}

/// 校验网站/应用选择器传入的短标识，防止把控制字符带入快照查询。
fn validate_selection_value(value: &str, label: &str) -> AppResult<()> {
    if value.trim().is_empty()
        || value.len() > 512
        || value
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "cronjob",
            format!("{label}选择值无效"),
        ));
    }
    Ok(())
}

/// 将容器内网站根目录映射到 SSH 宿主机挂载路径，避免在宿主机直接归档不存在的容器路径。
fn map_website_runtime_root(
    snapshot: &crate::domain::website::WebsiteSnapshot,
    runtime_path: &str,
) -> AppResult<String> {
    let Some(container_root) = snapshot.runtime_root.as_deref() else {
        validate_backup_path(runtime_path, true)?;
        return Ok(runtime_path.to_string());
    };
    let Some(host_root) = snapshot.host_root.as_deref() else {
        validate_backup_path(runtime_path, true)?;
        return Ok(runtime_path.to_string());
    };
    if runtime_path == container_root {
        validate_backup_path(host_root, true)?;
        return Ok(host_root.to_string());
    }
    let relative = runtime_path
        .strip_prefix(&format!("{container_root}/"))
        .ok_or_else(|| {
            AppError::new(
                "VALIDATION_FAILED",
                "cronjob",
                "网站根目录不在 OpenResty 容器挂载目录内",
            )
        })?;
    let mapped = format!("{host_root}/{relative}");
    validate_backup_path(&mapped, true)?;
    Ok(mapped)
}

/// 校验远端备份路径，禁止相对路径、父目录穿越和控制字符。
fn validate_backup_path(value: &str, allow_root: bool) -> AppResult<()> {
    if (!allow_root && value == "/")
        || !value.starts_with('/')
        || value.len() > 4_096
        || value.contains("..")
        || value
            .chars()
            .any(|character| character.is_control() || character == '\n' || character == '\r')
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "cronjob",
            "备份路径必须是安全的远端绝对路径",
        ));
    }
    Ok(())
}

/// 规范化数量受限的远端路径列表，并拒绝重复项和空项。
fn normalize_backup_paths(values: &[String], max: usize) -> AppResult<Vec<String>> {
    if values.is_empty() || values.len() > max {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "cronjob",
            "备份源路径数量超出允许范围",
        ));
    }
    let mut paths = Vec::with_capacity(values.len());
    for value in values {
        let path = value.trim();
        validate_backup_path(path, true)?;
        if paths.iter().any(|item| item == path) {
            return Err(AppError::new(
                "VALIDATION_FAILED",
                "cronjob",
                "备份源路径不能重复",
            ));
        }
        paths.push(path.to_string());
    }
    Ok(paths)
}

/// 构造 tar.gz 归档命令，使用同目录临时文件和原子替换避免半成品。
#[cfg(test)]
fn build_archive_command(
    backup_kind: &str,
    sources: &[String],
    destination: &str,
    exclusions: &[String],
) -> AppResult<String> {
    build_archive_command_with_policy(
        backup_kind,
        sources,
        Some(destination),
        exclusions,
        None,
        None,
    )
}

/// 构造 tar.gz 归档命令，并可将每次成功归档写入带 UTC 时间戳的轮换文件后清理旧版本。
fn build_archive_command_with_policy(
    backup_kind: &str,
    sources: &[String],
    destination: Option<&str>,
    exclusions: &[String],
    retention_count: Option<u32>,
    retention_days: Option<u32>,
) -> AppResult<String> {
    validate_kind(backup_kind)?;
    let sources = normalize_backup_paths(sources, 32)?;
    let destination = destination
        .ok_or_else(|| AppError::new("VALIDATION_FAILED", "cronjob", "备份必须指定目标文件"))?;
    validate_backup_path(destination, false)?;
    validate_retention_policy(
        backup_kind,
        Some(destination),
        retention_count,
        retention_days,
    )?;
    let exclusions = if exclusions.is_empty() {
        Vec::new()
    } else {
        normalize_backup_paths(exclusions, 32)?
    };
    if sources
        .iter()
        .any(|source| backup_path_contains(source, destination))
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "cronjob",
            "备份目标不能位于备份源目录内",
        ));
    }
    let event_path = build_backup_event_path(backup_kind, Some(destination));
    let (destination_prefix, destination_suffix) = retained_name_parts(destination);
    let destination = shell_escape(destination);
    let retained = retention_count.is_some() || retention_days.is_some();
    let destination_expression = if retained {
        let prefix = shell_escape(destination_prefix);
        let suffix = shell_escape(destination_suffix);
        format!(
            "destination_prefix={prefix}; destination_suffix={suffix}; destination=\"$destination_prefix-$(date -u +%Y%m%dT%H%M%SZ)$destination_suffix\"; ",
            prefix = prefix,
            suffix = suffix,
        )
    } else {
        format!("destination={destination}; ", destination = destination)
    };
    let source_args = sources
        .iter()
        .map(|path| shell_escape(path))
        .collect::<Vec<_>>()
        .join(" ");
    let exclusion_args = exclusions
        .iter()
        .map(|path| format!("--exclude={}", shell_escape(path)))
        .collect::<Vec<_>>()
        .join(" ");
    let pattern_name = destination_prefix
        .rsplit('/')
        .next()
        .unwrap_or(destination_prefix);
    let pattern = format!("{pattern_name}-*{destination_suffix}");
    let cleanup = build_retention_cleanup(retention_count, retention_days, &pattern);
    let event_write = event_path
        .map(|path| build_backup_event_write(&path, backup_kind))
        .unwrap_or_default();
    Ok(format!(
        "set -eu; {destination_expression} mkdir -p -- \"$(dirname -- \"$destination\")\"; tmp=$(mktemp \"$destination.tmp.XXXXXX\"); trap 'rm -f -- \"$tmp\"' EXIT; tar -czf \"$tmp\" {exclusion_args} -- {source_args}; mv -f -- \"$tmp\" \"$destination\"; trap - EXIT; {cleanup}printf '__CRON_BACKUP__\\t%s\\t%s\\n' {kind} \"$destination\"; {event_write}",
        destination_expression = destination_expression,
        kind = shell_escape(backup_kind),
        exclusion_args = exclusion_args,
        source_args = source_args,
        cleanup = cleanup,
        event_write = event_write,
    ))
}

/// 在归档成功后原子追加一个有界事件文件；事件写入失败不会让已完成的备份任务失败。
fn build_backup_event_write(event_path: &str, kind: &str) -> String {
    format!(
        "event_path={event_path}; event_tmp=$(mktemp \"$event_path.tmp.XXXXXX\" 2>/dev/null) || event_tmp=''; if [ -n \"$event_tmp\" ]; then {{ (tail -n 99 -- \"$event_path\" 2>/dev/null || true; printf '%s\\t%s\\t%s\\n' \"$(date -u +%Y%m%dT%H%M%SZ)\" {kind} \"$destination\") > \"$event_tmp\" && mv -f -- \"$event_tmp\" \"$event_path\" || rm -f -- \"$event_tmp\"; }} fi",
        event_path = shell_escape(event_path),
        kind = shell_escape(kind),
    )
}

/// 将归档目标拆成可控的文件名前缀和后缀，轮换文件只在同一目录生成。
fn retained_name_parts(destination: &str) -> (&str, &str) {
    if let Some(prefix) = destination.strip_suffix(".tar.gz") {
        (prefix, ".tar.gz")
    } else if let Some(prefix) = destination.strip_suffix(".tgz") {
        (prefix, ".tgz")
    } else if let Some(prefix) = destination.strip_suffix(".sql") {
        (prefix, ".sql")
    } else {
        (destination, "")
    }
}

/// 生成只针对客户端轮换文件的按份数/按天数清理片段，避免触碰目录中的其它文件。
fn build_retention_cleanup(
    retention_count: Option<u32>,
    retention_days: Option<u32>,
    pattern: &str,
) -> String {
    if retention_count.is_none() && retention_days.is_none() {
        return String::new();
    }
    let pattern = shell_escape(pattern);
    let directory = "\"$(dirname -- \"$destination\")\"";
    let mut cleanup = String::new();
    if let Some(days) = retention_days {
        cleanup.push_str(&format!(
            "find {directory} -maxdepth 1 -type f -name {pattern} -mtime +{days} -delete; ",
            directory = directory,
            pattern = pattern,
            days = days,
        ));
    }
    if let Some(count) = retention_count {
        cleanup.push_str(&format!(
            "find {directory} -maxdepth 1 -type f -name {pattern} -printf '%T@ %p\\n' | sort -rn | awk -v keep={count} 'NR > keep {{ sub(/^[^ ]+ /, \"\"); print }}' | while IFS= read -r old; do [ -n \"$old\" ] && rm -f -- \"$old\"; done; ",
            directory = directory,
            pattern = pattern,
            count = count,
        ));
    }
    cleanup
}

/// 判断目标是否位于源目录内，避免归档文件把自身再次打包。
fn backup_path_contains(source: &str, destination: &str) -> bool {
    let source = source.trim_end_matches('/');
    destination == source || destination.starts_with(&format!("{source}/"))
}

/// 构造目录/文件备份任务命令，支持多个源路径和排除路径。
#[cfg(test)]
fn build_directory_backup_command(
    sources: &[String],
    destination: Option<&str>,
    exclusions: &[String],
) -> AppResult<String> {
    let destination = destination
        .ok_or_else(|| AppError::new("VALIDATION_FAILED", "cronjob", "目录备份必须指定目标文件"))?;
    build_archive_command("directory", sources, destination, exclusions)
}

/// 构造网站备份归档；静态站点同时归档根目录和受控配置，反向代理至少归档配置。
#[cfg(test)]
fn build_website_backup_command(
    root_path: Option<&str>,
    config_path: &str,
    destination: Option<&str>,
    exclusions: &[String],
) -> AppResult<String> {
    build_website_backup_command_with_policy(
        root_path,
        config_path,
        destination,
        exclusions,
        None,
        None,
    )
}

/// 构造网站归档命令，并可启用时间戳文件和滚动保留策略。
fn build_website_backup_command_with_policy(
    root_path: Option<&str>,
    config_path: &str,
    destination: Option<&str>,
    exclusions: &[String],
    retention_count: Option<u32>,
    retention_days: Option<u32>,
) -> AppResult<String> {
    let destination = destination
        .ok_or_else(|| AppError::new("VALIDATION_FAILED", "cronjob", "网站备份必须指定目标文件"))?;
    validate_backup_path(config_path, true)?;
    let mut sources = Vec::with_capacity(2);
    if let Some(root_path) = root_path {
        validate_backup_path(root_path, true)?;
        sources.push(root_path.to_string());
    }
    sources.push(config_path.to_string());
    build_archive_command_with_policy(
        "website",
        &sources,
        Some(destination),
        exclusions,
        retention_count,
        retention_days,
    )
}

/// 构造应用备份归档；源目录来自远端已安装应用快照，包含 Compose 与环境文件但不读取其内容。
#[cfg(test)]
fn build_app_backup_command(
    install_path: &str,
    destination: Option<&str>,
    exclusions: &[String],
) -> AppResult<String> {
    build_app_backup_command_with_policy(install_path, destination, exclusions, None, None)
}

/// 构造应用归档命令，并可启用时间戳文件和滚动保留策略。
fn build_app_backup_command_with_policy(
    install_path: &str,
    destination: Option<&str>,
    exclusions: &[String],
    retention_count: Option<u32>,
    retention_days: Option<u32>,
) -> AppResult<String> {
    let destination = destination
        .ok_or_else(|| AppError::new("VALIDATION_FAILED", "cronjob", "应用备份必须指定目标文件"))?;
    validate_backup_path(install_path, false)?;
    build_archive_command_with_policy(
        "app",
        &[install_path.to_string()],
        Some(destination),
        exclusions,
        retention_count,
        retention_days,
    )
}

/// 构造固定日志目录备份任务，覆盖常见 Linux 日志路径而不接受任意命令。
#[cfg(test)]
fn build_log_backup_command(destination: Option<&str>) -> AppResult<String> {
    let destination = destination
        .ok_or_else(|| AppError::new("VALIDATION_FAILED", "cronjob", "日志备份必须指定目标文件"))?;
    build_archive_command("log", &["/var/log".into()], destination, &[])
}

/// 校验数据库备份目标，并限制到已有数据库域支持的三种 SQL 引擎。
fn validate_database_backup_target<'a>(
    engine: Option<&'a str>,
    name: Option<&'a str>,
    destination: Option<&'a str>,
) -> AppResult<(&'a str, &'a str, &'a str)> {
    let engine = engine
        .ok_or_else(|| AppError::new("VALIDATION_FAILED", "cronjob", "数据库备份必须选择引擎"))?;
    let name = name
        .ok_or_else(|| AppError::new("VALIDATION_FAILED", "cronjob", "数据库备份必须选择数据库"))?;
    let destination = destination.ok_or_else(|| {
        AppError::new("VALIDATION_FAILED", "cronjob", "数据库备份必须指定目标文件")
    })?;
    if !matches!(engine, "mysql" | "mariadb" | "postgresql")
        || name.is_empty()
        || name.len() > 64
        || !name
            .as_bytes()
            .first()
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "cronjob",
            "数据库引擎或名称无效",
        ));
    }
    validate_backup_path(destination, false)?;
    Ok((engine, name, destination))
}

/// 构造无客户端凭据的数据库导出命令，PostgreSQL root 任务自动切换 postgres 用户。
#[cfg(test)]
fn build_database_backup_command(
    engine: Option<&str>,
    name: Option<&str>,
    destination: Option<&str>,
) -> AppResult<String> {
    build_database_backup_command_with_policy(engine, name, destination, None, None)
}

/// 构造数据库导出命令，并可将导出写入带时间戳的轮换文件后清理旧版本。
fn build_database_backup_command_with_policy(
    engine: Option<&str>,
    name: Option<&str>,
    destination: Option<&str>,
    retention_count: Option<u32>,
    retention_days: Option<u32>,
) -> AppResult<String> {
    let (engine, name, destination) = validate_database_backup_target(engine, name, destination)?;
    validate_retention_policy(
        "database",
        Some(destination),
        retention_count,
        retention_days,
    )?;
    let event_path = build_backup_event_path("database", Some(destination));
    let (destination_prefix, destination_suffix) = retained_name_parts(destination);
    let destination_expression = if retention_count.is_some() || retention_days.is_some() {
        format!(
            "destination_prefix={}; destination_suffix={}; destination=\"$destination_prefix-$(date -u +%Y%m%dT%H%M%SZ)$destination_suffix\"; ",
            shell_escape(destination_prefix),
            shell_escape(destination_suffix),
        )
    } else {
        format!("destination={}; ", shell_escape(destination))
    };
    let pattern_name = destination_prefix
        .rsplit('/')
        .next()
        .unwrap_or(destination_prefix);
    let pattern = format!("{pattern_name}-*{destination_suffix}");
    let cleanup = build_retention_cleanup(retention_count, retention_days, &pattern);
    let event_write = event_path
        .map(|path| build_backup_event_write(&path, "database"))
        .unwrap_or_default();
    let name = shell_escape(name);
    let dump = match engine {
        "mysql" | "mariadb" => format!(
            "mysqldump --single-transaction --routines --events --triggers --databases {name} > \"$tmp\""
        ),
        "postgresql" => format!("pg_dump {name} > \"$tmp\""),
        _ => unreachable!("validated database engine"),
    };
    let dump = if engine == "postgresql" {
        format!(
            "if [ \"$(id -u)\" = 0 ] && command -v runuser >/dev/null 2>&1; then runuser -u postgres -- sh -c {}; else sh -c {}; fi",
            shell_escape(&dump),
            shell_escape(&dump)
        )
    } else {
        dump
    };
    Ok(format!(
        "set -eu; {destination_expression} mkdir -p -- \"$(dirname -- \"$destination\")\"; tmp=$(mktemp \"$destination.tmp.XXXXXX\"); export tmp; trap 'rm -f -- \"$tmp\"' EXIT; {dump}; mv -f -- \"$tmp\" \"$destination\"; trap - EXIT; {cleanup}printf '__CRON_BACKUP__\\t%s\\t%s\\n' 'database' \"$destination\"; {event_write}",
        destination_expression = destination_expression,
        dump = dump,
        cleanup = cleanup,
        event_write = event_write,
    ))
}

/// 校验并规范化 URL 类型任务的地址列表，确保只允许 HTTP/HTTPS 请求。
fn normalize_urls(values: &[String]) -> AppResult<Vec<String>> {
    if values.is_empty() || values.len() > 20 {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "cronjob",
            "URL 任务必须包含 1 到 20 个地址",
        ));
    }
    let mut normalized = Vec::with_capacity(values.len());
    for value in values {
        let url = value.trim();
        if url.is_empty()
            || url.len() > 2_048
            || url
                .chars()
                .any(|character| character.is_whitespace() || character.is_control())
        {
            return Err(AppError::new(
                "VALIDATION_FAILED",
                "cronjob",
                "URL 地址不能为空、不能包含空白或控制字符，且长度不能超过 2048",
            ));
        }
        let parsed = reqwest::Url::parse(url)
            .map_err(|_| AppError::new("VALIDATION_FAILED", "cronjob", "URL 地址格式无效"))?;
        if !matches!(parsed.scheme(), "http" | "https")
            || parsed.host_str().is_none()
            || !parsed.username().is_empty()
            || parsed.password().is_some()
        {
            return Err(AppError::new(
                "VALIDATION_FAILED",
                "cronjob",
                "URL 任务只允许不含认证信息的 HTTP/HTTPS 地址",
            ));
        }
        normalized.push(url.to_string());
    }
    Ok(normalized)
}

/// 构造固定参数的 curl 命令，URL 仅作为受 shell 转义保护的位置参数。
fn build_url_command(urls: &[String]) -> String {
    let arguments = urls
        .iter()
        .map(|url| shell_escape(url))
        .collect::<Vec<_>>()
        .join(" ");
    format!("{URL_CURL_PREFIX} {arguments}")
}

/// 根据客户端固定 curl 前缀识别 URL 任务，旧任务统一视为 Shell。
fn classify_cron_command(command: &str) -> String {
    if command
        .trim_start()
        .starts_with(&format!("{URL_CURL_PREFIX} "))
    {
        "url".into()
    } else {
        "shell".into()
    }
}

/// 校验 marker id 只接受本客户端生成的 UUID 或只读 line id。
fn validate_id(value: &str) -> AppResult<()> {
    if value.starts_with("line-") || Uuid::parse_str(value).is_ok() {
        Ok(())
    } else {
        Err(AppError::new(
            "VALIDATION_FAILED",
            "cronjob",
            "计划任务标识无效",
        ))
    }
}

/// 校验 crontab 目标用户，空值表示当前 SSH 用户。
fn validate_user(value: &str) -> AppResult<()> {
    if value.is_empty()
        || (value.len() <= 32
            && value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-')))
    {
        Ok(())
    } else {
        Err(AppError::new(
            "VALIDATION_FAILED",
            "cronjob",
            "计划任务用户无效",
        ))
    }
}

/// 校验服务器 ID 可安全地作为本地计划任务历史设置键的一部分。
fn validate_server_id(value: &str) -> AppResult<()> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "cronjob",
            "服务器 ID 无效",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use crate::domain::appstore;
    use crate::domain::ssh::{ConnectOutcome, TrustHostKeyInput};
    use crate::domain::website::{self, SaveWebsiteInput, WebsiteActionInput};
    use crate::infra::db::ServerRepository;
    use crate::security::{shell_escape, CredentialStore, OsCredentialStore};
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

    use super::{
        action, build_app_backup_command, build_archive_command_with_policy,
        build_backup_event_path, build_database_backup_command,
        build_database_backup_command_with_policy, build_directory_backup_command,
        build_log_backup_command, build_url_command, build_website_backup_command,
        event_matches_job, export_jobs, import_jobs, normalize_urls, notification_target_url,
        parse_backup_event_line, parse_retention_marker, parse_snapshot, save, snapshot,
        validate_backup_path, validate_import_entry, validate_kind, validate_notification_provider,
        validate_notification_url, validate_retention_policy, validate_schedule,
        CronJobActionInput, CronJobExportEntry, CronJobImportInput, SaveCronJobInput,
        URL_CURL_PREFIX,
    };

    #[test]
    fn parses_cron_and_timer_markers() {
        let value = parse_snapshot("__USER__\troot\n__JOB__\t123\t*/5 * * * *\techo ok\troot\t1\n__TIMER__\tapt.timer\t2026-01-01\t2025-12-31\tapt.service\n").unwrap();
        assert_eq!(value.jobs[0].schedule, "*/5 * * * *");
        assert_eq!(value.jobs[0].kind, "shell");
        assert_eq!(value.timers[0].name, "apt.timer");
    }

    /// 验证离线事件只接受固定 UTC 时间格式、归档类型和任务目标目录。
    #[test]
    fn validates_offline_backup_events() {
        let event = parse_backup_event_line(
            "20260821T123456Z\tdirectory\t/var/backups/site-20260821T123456Z.tar.gz",
        )
        .unwrap();
        assert!(event_matches_job(
            &event,
            "/var/backups/site.1panel-client-events"
        ));
        assert!(parse_backup_event_line("2026-08-21T123456Z\tdirectory\t/var/backups/a").is_none());
        assert!(parse_backup_event_line("20260821T123456Z\tshell\t/var/backups/a").is_none());
        assert!(!event_matches_job(
            &event,
            "/var/other/site.1panel-client-events"
        ));
    }

    #[test]
    fn accepts_common_cron_schedules() {
        assert!(validate_schedule("*/5 * * * *").is_ok());
        assert!(validate_schedule("0 2 * * 1-5").is_ok());
        assert!(validate_schedule("every hour").is_err());
    }

    /// 验证计划任务通知渠道、URL 白名单和钉钉签名 URL 生成不会放行任意协议。
    #[test]
    fn validates_cron_notification_targets() {
        assert!(validate_notification_provider("generic").is_ok());
        assert!(validate_notification_provider("smtp").is_err());
        assert!(validate_notification_url("https://hooks.example.test/report").is_ok());
        assert!(validate_notification_url("file:///tmp/report").is_err());
        let target = notification_target_url(
            "https://oapi.example.test/robot/send",
            "dingtalk",
            Some(&secrecy::SecretString::from("signing-secret")),
        )
        .unwrap();
        assert!(target.contains("timestamp="));
        assert!(target.contains("sign="));
    }

    #[test]
    fn validates_multiple_http_urls() {
        let urls = vec![
            "https://example.com/health".to_string(),
            "http://127.0.0.1:8080/ping?full=1".to_string(),
        ];
        assert_eq!(normalize_urls(&urls).unwrap(), urls);
        assert!(normalize_urls(&["ftp://example.com".into()]).is_err());
        assert!(normalize_urls(&["https://user:pass@example.com".into()]).is_err());
        assert!(normalize_urls(&["https://example.com/a b".into()]).is_err());
        assert!(normalize_urls(&[]).is_err());
    }

    #[test]
    fn builds_fixed_url_command() {
        let command = build_url_command(&[
            "https://example.com/health".into(),
            "https://example.com/a'b".into(),
        ]);
        assert!(command.starts_with(&format!("{URL_CURL_PREFIX} ")));
        assert!(command.contains("'https://example.com/health'"));
        assert!(command.contains("a'\"'\"'b"));
    }

    #[test]
    fn parses_fixed_url_command_as_url_kind() {
        let command = build_url_command(&["https://example.com/health".into()]);
        let value = parse_snapshot(&format!(
            "__USER__\troot\n__JOB__\t123\t*/5 * * * *\t{command}\troot\t1\n"
        ))
        .unwrap();
        assert_eq!(value.jobs[0].kind, "url");
    }

    #[test]
    fn parses_explicit_backup_kind_marker() {
        let value = parse_snapshot(
            "__USER__\troot\n__JOB__\t123\t0 2 * * *\ttar -czf /tmp/archive.tgz -- /var/www\troot\t1\t1\tdirectory\tcount=7;days=30\tids=\t# 1panel-client-cron-backup-event:/tmp/archive.1panel-client-events\n",
        )
        .unwrap();
        assert_eq!(value.jobs[0].kind, "directory");
        assert!(value.jobs[0].enabled);
        assert_eq!(value.jobs[0].retention_count, Some(7));
        assert_eq!(value.jobs[0].retention_days, Some(30));
        assert_eq!(
            value.jobs[0].backup_event_path.as_deref(),
            Some("/tmp/archive.1panel-client-events")
        );
    }

    /// 验证保留 marker、范围校验和归档目标文件名边界，避免清理误删其它文件。
    #[test]
    fn validates_backup_retention_policy() {
        assert_eq!(
            parse_retention_marker("count=7;days=30"),
            Some((Some(7), Some(30)))
        );
        assert!(parse_retention_marker("count=0;days=0").is_none());
        assert!(parse_retention_marker("count=1001;days=30").is_none());
        assert!(validate_retention_policy(
            "directory",
            Some("/var/backups/site.tar.gz"),
            Some(7),
            Some(30)
        )
        .is_ok());
        assert!(validate_retention_policy("shell", Some("/var/backups/a"), Some(2), None).is_err());
        assert!(validate_retention_policy(
            "directory",
            Some("/var/backups/site;rm.tar.gz"),
            Some(2),
            None
        )
        .is_err());
    }

    #[test]
    fn builds_safe_directory_and_log_backup_commands() {
        let command = build_directory_backup_command(
            &["/var/www/site".into(), "/etc/nginx".into()],
            Some("/var/backups/site.tar.gz"),
            &["/var/www/site/cache".into()],
        )
        .unwrap();
        assert!(command.contains("tar -czf"));
        assert!(command.contains("--exclude='/var/www/site/cache'"));
        assert!(command.contains("mv -f"));
        assert!(!command.contains("; rm -rf"));
        assert!(build_directory_backup_command(
            &["/var/www/site".into()],
            Some("/var/www/site/backup.tar.gz"),
            &[],
        )
        .is_err());

        let log = build_log_backup_command(Some("/var/backups/logs.tar.gz")).unwrap();
        assert!(log.contains("'/var/log'"));
        assert!(log.contains("__CRON_BACKUP__"));
    }

    /// 验证归档保留策略生成 UTC 时间戳目标，并只清理固定前缀的旧文件。
    #[test]
    fn builds_rotating_archive_command() {
        let command = build_archive_command_with_policy(
            "directory",
            &["/var/www/site".into()],
            Some("/var/backups/site.tar.gz"),
            &[],
            Some(3),
            Some(7),
        )
        .unwrap();
        assert!(command.contains("date -u +%Y%m%dT%H%M%SZ"));
        assert!(command.contains("-name 'site-*.tar.gz'"));
        assert!(command.contains("-mtime +7"));
        assert!(command.contains("awk -v keep=3"));
        assert!(command.contains("mktemp \"$destination.tmp.XXXXXX\""));
        assert!(command.contains("/var/backups/site.1panel-client-events"));
        assert!(!command.contains("site;rm"));
    }

    /// 验证数据库轮换导出把临时路径导出给 PostgreSQL 子 shell，避免 root/runuser 丢失变量。
    #[test]
    fn builds_rotating_database_backup_command() {
        let command = build_database_backup_command_with_policy(
            Some("postgresql"),
            Some("app_db"),
            Some("/var/backups/app.sql"),
            Some(5),
            None,
        )
        .unwrap();
        assert!(command.contains("export tmp"));
        assert!(command.contains("date -u +%Y%m%dT%H%M%SZ"));
        assert!(command.contains("runuser -u postgres"));
        assert!(command.contains("-name 'app-*.sql'"));
        assert!(command.contains("/var/backups/app.1panel-client-events"));
    }

    /// 验证网站与应用备份都会使用固定 tar 模板，并拒绝把归档写入源目录。
    #[test]
    fn builds_website_and_app_backup_commands() {
        let website = build_website_backup_command(
            Some("/var/www/example"),
            "/etc/nginx/conf.d/site-example.conf",
            Some("/var/backups/example-site.tar.gz"),
            &[],
        )
        .unwrap();
        assert!(website.contains("'/var/www/example'"));
        assert!(website.contains("'/etc/nginx/conf.d/site-example.conf'"));
        assert!(website.contains("'website'"));

        let app = build_app_backup_command(
            "/opt/1panel/apps/demo/1.0",
            Some("/var/backups/demo-app.tar.gz"),
            &["/opt/1panel/apps/demo/1.0/cache".into()],
        )
        .unwrap();
        assert!(app.contains("'/opt/1panel/apps/demo/1.0'"));
        assert!(app.contains("--exclude='/opt/1panel/apps/demo/1.0/cache'"));
        assert!(app.contains("'app'"));
        assert!(build_app_backup_command(
            "/opt/1panel/apps/demo/1.0",
            Some("/opt/1panel/apps/demo/1.0/archive.tgz"),
            &[],
        )
        .is_err());
    }

    #[test]
    fn builds_database_backup_commands_without_raw_identifier_interpolation() {
        let mysql = build_database_backup_command(
            Some("mysql"),
            Some("app_db"),
            Some("/var/backups/app.sql"),
        )
        .unwrap();
        assert!(mysql.contains("mysqldump --single-transaction"));
        assert!(mysql.contains("--databases 'app_db'"));

        let postgres = build_database_backup_command(
            Some("postgresql"),
            Some("app_db"),
            Some("/var/backups/app.sql"),
        )
        .unwrap();
        assert!(postgres.contains("runuser -u postgres"));
        assert!(postgres.contains("pg_dump"));
        assert!(postgres.contains("app_db"));

        assert!(build_database_backup_command(
            Some("mysql"),
            Some("app;drop"),
            Some("/var/backups/app.sql"),
        )
        .is_err());
    }

    #[test]
    fn validates_backup_paths_and_task_kinds() {
        assert!(validate_backup_path("/var/backups/a.tgz", false).is_ok());
        assert!(validate_backup_path("relative.tgz", false).is_err());
        assert!(validate_backup_path("/var/../tmp/a.tgz", false).is_err());
        assert!(validate_kind("directory").is_ok());
        assert!(validate_kind("website").is_ok());
        assert!(validate_kind("app").is_ok());
    }

    #[test]
    fn keeps_legacy_save_payloads_as_shell_tasks() {
        let input: SaveCronJobInput = serde_json::from_str(
            r#"{"serverId":"server","schedule":"0 2 * * *","command":"echo ok","enabled":true,"confirmed":true}"#,
        )
        .unwrap();
        assert_eq!(input.kind, "shell");
        assert!(input.urls.is_empty());
        assert!(input.source_paths.is_empty());
    }

    /// 验证导入条目会复用 cron、命令和用户校验，并拒绝过长命令。
    #[test]
    fn validates_cron_import_entries() {
        let entry = CronJobExportEntry {
            id: "remote-id".into(),
            schedule: "0 2 * * *".into(),
            command: "echo ok".into(),
            kind: "directory".into(),
            user: "root".into(),
            managed: true,
            enabled: true,
            retention_count: None,
            retention_days: None,
            backup_account_ids: Vec::new(),
            default_backup_account_id: None,
            backup_event_path: None,
        };
        assert!(validate_import_entry(&entry).is_ok());
        assert!(validate_import_entry(&CronJobExportEntry {
            schedule: "every hour".into(),
            ..entry.clone()
        })
        .is_err());
        assert!(validate_import_entry(&CronJobExportEntry {
            command: "x".repeat(8_193),
            ..entry
        })
        .is_err());
    }

    /// 验证版本化导出条目可以稳定序列化为 camelCase JSON，供前端下载和再次导入。
    #[test]
    fn serializes_versioned_cron_export_entry() {
        let entry = CronJobExportEntry {
            id: "job".into(),
            schedule: "0 2 * * *".into(),
            command: "echo ok".into(),
            kind: "shell".into(),
            user: "root".into(),
            managed: true,
            enabled: false,
            retention_count: None,
            retention_days: None,
            backup_account_ids: Vec::new(),
            default_backup_account_id: None,
            backup_event_path: None,
        };
        let value = serde_json::to_value(&entry).unwrap();
        assert_eq!(value["managed"], true);
        assert_eq!(value["enabled"], false);
        assert_eq!(value["schedule"], "0 2 * * *");
    }

    /// 在显式提供本机应用数据库和服务器 ID 时，验证目录备份任务的真实 SSH 写入/运行/删除闭环。
    #[tokio::test]
    #[ignore = "需要用户已授权的真实测试节点环境变量"]
    async fn real_directory_backup_cron_round_trip() -> crate::errors::AppResult<()> {
        let db_path = std::env::var("ONEPANEL_CLIENT_DB").map_err(|_| {
            crate::errors::AppError::new("TEST_ENV_MISSING", "cronjob", "缺少本机测试数据库路径")
        })?;
        let server_id = std::env::var("ONEPANEL_CLIENT_SERVER_ID").map_err(|_| {
            crate::errors::AppError::new("TEST_ENV_MISSING", "cronjob", "缺少测试服务器 ID")
        })?;
        let options = SqliteConnectOptions::new().filename(db_path);
        let pool = SqlitePoolOptions::new()
            .max_connections(3)
            .connect_with(options)
            .await
            .map_err(crate::errors::AppError::database)?;
        let credentials: Arc<dyn CredentialStore> =
            Arc::new(OsCredentialStore::new("com.agentless.servermanager"));
        let servers = ServerRepository::new(pool, credentials);
        let ssh = crate::domain::ssh::SshConnectionManager::new(servers);
        let outcome = ssh.connect(&server_id).await?;
        if let ConnectOutcome::HostKey(challenge) = outcome {
            ssh.trust(TrustHostKeyInput {
                server_id: challenge.server_id,
                host: challenge.host,
                port: challenge.port,
                key_type: challenge.key_type,
                fingerprint: challenge.fingerprint,
            })
            .await?;
        }
        let suffix = uuid::Uuid::new_v4();
        let base = format!("/tmp/1panel-client-cron-smoke-{suffix}");
        let source = format!("{base}/source");
        let destination = format!("{base}/archive.tar.gz");
        let job_id = uuid::Uuid::new_v4().to_string();
        let run_result = async {
            let prepare = format!(
                "set -eu; mkdir -p -- {source}; printf '%s\\n' smoke > {file}",
                source = shell_escape(&source),
                file = shell_escape(&format!("{source}/value.txt")),
            );
            let prepared = ssh
                .execute_system(&server_id, &prepare, std::time::Duration::from_secs(30))
                .await?;
            if prepared.exit_code != 0 {
                return Err(crate::errors::AppError::new(
                    "TEST_REMOTE_PREPARE_FAILED",
                    "cronjob",
                    "远端计划任务测试目录准备失败",
                )
                .details(prepared.stderr));
            }
            let backup_command = build_archive_command_with_policy(
                "directory",
                std::slice::from_ref(&source),
                Some(&destination),
                &[],
                Some(2),
                Some(30),
            )?;
            let job = save(
                &ssh,
                SaveCronJobInput {
                    server_id: server_id.clone(),
                    id: Some(job_id.clone()),
                    schedule: "0 2 * * *".into(),
                    command: String::new(),
                    kind: "directory".into(),
                    urls: Vec::new(),
                    source_paths: vec![source.clone()],
                    destination: Some(destination.clone()),
                    database_engine: None,
                    database_name: None,
                    exclude_paths: Vec::new(),
                    website_domain: None,
                    app_install_path: None,
                    retention_count: Some(2),
                    retention_days: Some(30),
                    backup_account_ids: Vec::new(),
                    default_backup_account_id: None,
                    preserve_command: false,
                    backup_event_path: None,
                    user: None,
                    enabled: true,
                    confirmed: true,
                },
            )
            .await?;
            let snapshot_before = snapshot(&ssh, &server_id).await?;
            assert!(snapshot_before
                .jobs
                .iter()
                .any(|item| item.id == job.id
                    && item.kind == "directory"
                    && item.retention_count == Some(2)
                    && item.retention_days == Some(30)));
            let exported = export_jobs(&ssh, &server_id).await?;
            assert_eq!(exported.format, "1panel-client-cronjobs");
            let exported_job = exported
                .jobs
                .iter()
                .find(|item| item.id == job.id)
                .cloned()
                .ok_or_else(|| {
                    crate::errors::AppError::new(
                        "TEST_CRON_EXPORT_MISSING",
                        "cronjob",
                        "计划任务导出结果缺少刚创建的任务",
                    )
                })?;
            let imported = import_jobs(
                &ssh,
                CronJobImportInput {
                    server_id: server_id.clone(),
                    jobs: vec![exported_job],
                    confirmed: true,
                },
            )
            .await?;
            assert_eq!(imported.imported, 1);
            assert_eq!(imported.converted_to_shell, 0);
            let snapshot_imported = snapshot(&ssh, &server_id).await?;
            let imported_id = snapshot_imported
                .jobs
                .iter()
                .find(|item| item.command == backup_command && item.id != job.id)
                .map(|item| item.id.clone())
                .ok_or_else(|| {
                    crate::errors::AppError::new(
                        "TEST_CRON_IMPORT_MISSING",
                        "cronjob",
                        "计划任务导入结果缺少新 marker",
                    )
                })?;
            action(
                &ssh,
                CronJobActionInput {
                    server_id: server_id.clone(),
                    id: job.id.clone(),
                    command: Some(backup_command),
                    user: None,
                    backup_account_ids: Vec::new(),
                    action: "run".into(),
                    confirmed: true,
                },
            )
            .await?;
            let verified = ssh
                .execute_system(
                    &server_id,
                    &format!(
                        "set -eu; found=0; for file in {}-*.tar.gz; do [ -s \"$file\" ] || continue; found=1; break; done; [ \"$found\" = 1 ]",
                        shell_escape(destination.trim_end_matches(".tar.gz"))
                    ),
                    std::time::Duration::from_secs(30),
                )
                .await?;
            if verified.exit_code != 0 {
                return Err(crate::errors::AppError::new(
                    "TEST_REMOTE_BACKUP_MISSING",
                    "cronjob",
                    "远端目录备份文件未生成",
                ));
            }
            let event_path = build_backup_event_path("directory", Some(&destination)).ok_or_else(|| {
                crate::errors::AppError::new(
                    "TEST_EVENT_PATH_MISSING",
                    "cronjob",
                    "目录备份事件路径未生成",
                )
            })?;
            let event_log = ssh
                .execute_system(
                    &server_id,
                    &format!("tail -n 1 -- {}", shell_escape(&event_path)),
                    std::time::Duration::from_secs(30),
                )
                .await?;
            if event_log.exit_code != 0
                || !event_log.stdout.contains("\tdirectory\t")
                || !event_log
                    .stdout
                    .contains(destination.trim_end_matches(".tar.gz"))
            {
                return Err(crate::errors::AppError::new(
                    "TEST_REMOTE_EVENT_MISSING",
                    "cronjob",
                    "远端目录备份事件文件未生成",
                ));
            }
            action(
                &ssh,
                CronJobActionInput {
                    server_id: server_id.clone(),
                    id: job.id,
                    command: None,
                    user: None,
                    backup_account_ids: Vec::new(),
                    action: "delete".into(),
                    confirmed: true,
                },
            )
            .await?;
            action(
                &ssh,
                CronJobActionInput {
                    server_id: server_id.clone(),
                    id: imported_id,
                    command: None,
                    user: None,
                    backup_account_ids: Vec::new(),
                    action: "delete".into(),
                    confirmed: true,
                },
            )
            .await?;
            let snapshot_after = snapshot(&ssh, &server_id).await?;
            assert!(!snapshot_after.jobs.iter().any(|item| item.id == job_id));
            Ok::<(), crate::errors::AppError>(())
        }
        .await;
        if let Ok(cleanup_snapshot) = snapshot(&ssh, &server_id).await {
            for job in cleanup_snapshot
                .jobs
                .into_iter()
                .filter(|item| item.managed && item.command.contains(&base))
            {
                let _ = action(
                    &ssh,
                    CronJobActionInput {
                        server_id: server_id.clone(),
                        id: job.id,
                        command: None,
                        user: None,
                        backup_account_ids: Vec::new(),
                        action: "delete".into(),
                        confirmed: true,
                    },
                )
                .await;
            }
        }
        let _ = ssh
            .execute_system(
                &server_id,
                &format!("rm -rf -- {}", shell_escape(&base)),
                std::time::Duration::from_secs(30),
            )
            .await;
        let _ = ssh.disconnect(&server_id).await;
        run_result
    }

    /// 在显式提供本机应用数据库和服务器 ID 时，验证网站/应用备份选择器解析真实远端对象并完成归档闭环。
    #[tokio::test]
    #[ignore = "需要用户已授权的真实测试节点环境变量"]
    async fn real_website_and_app_backup_cron_round_trip() -> crate::errors::AppResult<()> {
        let db_path = std::env::var("ONEPANEL_CLIENT_DB").map_err(|_| {
            crate::errors::AppError::new("TEST_ENV_MISSING", "cronjob", "缺少本机测试数据库路径")
        })?;
        let server_id = std::env::var("ONEPANEL_CLIENT_SERVER_ID").map_err(|_| {
            crate::errors::AppError::new("TEST_ENV_MISSING", "cronjob", "缺少测试服务器 ID")
        })?;
        let options = SqliteConnectOptions::new().filename(db_path);
        let pool = SqlitePoolOptions::new()
            .max_connections(3)
            .connect_with(options)
            .await
            .map_err(crate::errors::AppError::database)?;
        let credentials: Arc<dyn CredentialStore> =
            Arc::new(OsCredentialStore::new("com.agentless.servermanager"));
        let servers = ServerRepository::new(pool, credentials);
        let ssh = crate::domain::ssh::SshConnectionManager::new(servers);
        let outcome = ssh.connect(&server_id).await?;
        if let ConnectOutcome::HostKey(challenge) = outcome {
            ssh.trust(TrustHostKeyInput {
                server_id: challenge.server_id,
                host: challenge.host,
                port: challenge.port,
                key_type: challenge.key_type,
                fingerprint: challenge.fingerprint,
            })
            .await?;
        }
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let base = format!("/tmp/1panel-client-website-app-smoke-{suffix}");
        let website_domain = format!("cron-smoke-{suffix}.test");
        let mut created_website = false;
        let mut website_root: Option<String> = None;
        let run_result = async {
            let website_port = 18_000
                + (u16::from_le_bytes([suffix.as_bytes()[0], suffix.as_bytes()[1]]) % 10_000);
            let website_snapshot = website::save(
                &ssh,
                SaveWebsiteInput {
                    server_id: server_id.clone(),
                    domain: website_domain.clone(),
                    kind: "static".into(),
                    listen_port: website_port,
                    root_path: None,
                    php_runtime: None,
                    php_socket: None,
                    upstream_scheme: None,
                    upstream_host: None,
                    upstream_port: None,
                    enable_https: false,
                    https_port: 443,
                    certificate_path: None,
                    certificate_key_path: None,
                    confirmed: true,
                },
            )
            .await?;
            created_website = true;
            let website = website_snapshot
                .websites
                .iter()
                .find(|item| item.domain == website_domain)
                .cloned()
                .ok_or_else(|| {
                    crate::errors::AppError::new(
                        "TEST_WEBSITE_MISSING",
                        "cronjob",
                        "网站备份 smoke 未找到刚创建的网站",
                    )
                })?;
            website_root = website.root_path.clone();
            let website_destination = format!("{base}/website.tar.gz");
            let website_job = save(
                &ssh,
                SaveCronJobInput {
                    server_id: server_id.clone(),
                    id: None,
                    schedule: "0 3 * * *".into(),
                    command: String::new(),
                    kind: "website".into(),
                    urls: Vec::new(),
                    source_paths: Vec::new(),
                    destination: Some(website_destination.clone()),
                    database_engine: None,
                    database_name: None,
                    exclude_paths: Vec::new(),
                    website_domain: Some(website_domain.clone()),
                    app_install_path: None,
                    retention_count: None,
                    retention_days: None,
                    backup_account_ids: Vec::new(),
                    default_backup_account_id: None,
                    preserve_command: false,
                    backup_event_path: None,
                    user: None,
                    enabled: true,
                    confirmed: true,
                },
            )
            .await?;
            let website_job = snapshot(&ssh, &server_id)
                .await?
                .jobs
                .into_iter()
                .find(|item| item.id == website_job.id)
                .ok_or_else(|| {
                    crate::errors::AppError::new(
                        "TEST_WEBSITE_CRON_MISSING",
                        "cronjob",
                        "网站备份 smoke 未找到 crontab 任务",
                    )
                })?;
            action(
                &ssh,
                CronJobActionInput {
                    server_id: server_id.clone(),
                    id: website_job.id.clone(),
                    command: Some(website_job.command),
                    user: None,
                    backup_account_ids: Vec::new(),
                    action: "run".into(),
                    confirmed: true,
                },
            )
            .await?;
            let website_file = ssh
                .execute_system(
                    &server_id,
                    &format!("test -s {}", shell_escape(&website_destination)),
                    Duration::from_secs(30),
                )
                .await?;
            if website_file.exit_code != 0 {
                return Err(crate::errors::AppError::new(
                    "TEST_WEBSITE_BACKUP_MISSING",
                    "cronjob",
                    "网站备份归档未生成",
                ));
            }
            action(
                &ssh,
                CronJobActionInput {
                    server_id: server_id.clone(),
                    id: website_job.id,
                    command: None,
                    user: None,
                    backup_account_ids: Vec::new(),
                    action: "delete".into(),
                    confirmed: true,
                },
            )
            .await?;

            let apps = appstore::installed(&ssh, &server_id).await?;
            let app = apps.apps.first().cloned().ok_or_else(|| {
                crate::errors::AppError::new(
                    "TEST_APP_MISSING",
                    "cronjob",
                    "应用备份 smoke 未找到已安装 Compose 应用",
                )
            })?;
            let app_destination = format!("{base}/app.tar.gz");
            let app_job = save(
                &ssh,
                SaveCronJobInput {
                    server_id: server_id.clone(),
                    id: None,
                    schedule: "0 4 * * *".into(),
                    command: String::new(),
                    kind: "app".into(),
                    urls: Vec::new(),
                    source_paths: Vec::new(),
                    destination: Some(app_destination.clone()),
                    database_engine: None,
                    database_name: None,
                    exclude_paths: Vec::new(),
                    website_domain: None,
                    app_install_path: Some(app.path),
                    retention_count: None,
                    retention_days: None,
                    backup_account_ids: Vec::new(),
                    default_backup_account_id: None,
                    preserve_command: false,
                    backup_event_path: None,
                    user: None,
                    enabled: true,
                    confirmed: true,
                },
            )
            .await?;
            let app_job = snapshot(&ssh, &server_id)
                .await?
                .jobs
                .into_iter()
                .find(|item| item.id == app_job.id)
                .ok_or_else(|| {
                    crate::errors::AppError::new(
                        "TEST_APP_CRON_MISSING",
                        "cronjob",
                        "应用备份 smoke 未找到 crontab 任务",
                    )
                })?;
            action(
                &ssh,
                CronJobActionInput {
                    server_id: server_id.clone(),
                    id: app_job.id.clone(),
                    command: Some(app_job.command),
                    user: None,
                    backup_account_ids: Vec::new(),
                    action: "run".into(),
                    confirmed: true,
                },
            )
            .await?;
            let app_file = ssh
                .execute_system(
                    &server_id,
                    &format!("test -s {}", shell_escape(&app_destination)),
                    Duration::from_secs(60),
                )
                .await?;
            if app_file.exit_code != 0 {
                return Err(crate::errors::AppError::new(
                    "TEST_APP_BACKUP_MISSING",
                    "cronjob",
                    "应用备份归档未生成",
                ));
            }
            action(
                &ssh,
                CronJobActionInput {
                    server_id: server_id.clone(),
                    id: app_job.id,
                    command: None,
                    user: None,
                    backup_account_ids: Vec::new(),
                    action: "delete".into(),
                    confirmed: true,
                },
            )
            .await?;
            Ok::<(), crate::errors::AppError>(())
        }
        .await;
        if let Ok(cleanup_snapshot) = snapshot(&ssh, &server_id).await {
            for job in cleanup_snapshot
                .jobs
                .into_iter()
                .filter(|item| item.managed && item.command.contains(&base))
            {
                let _ = action(
                    &ssh,
                    CronJobActionInput {
                        server_id: server_id.clone(),
                        id: job.id,
                        command: None,
                        user: None,
                        backup_account_ids: Vec::new(),
                        action: "delete".into(),
                        confirmed: true,
                    },
                )
                .await;
            }
        }
        if created_website {
            let _ = website::action(
                &ssh,
                WebsiteActionInput {
                    server_id: server_id.clone(),
                    domain: website_domain,
                    action: "delete".into(),
                    confirmed: true,
                },
            )
            .await;
        }
        if let Some(root) = website_root {
            if root != "/" && !root.contains("..") {
                let _ = ssh
                    .execute_system(
                        &server_id,
                        &format!("rm -rf -- {}", shell_escape(&root)),
                        Duration::from_secs(30),
                    )
                    .await;
            }
        }
        let _ = ssh
            .execute_system(
                &server_id,
                &format!("rm -rf -- {}", shell_escape(&base)),
                Duration::from_secs(30),
            )
            .await;
        let _ = ssh.disconnect(&server_id).await;
        run_result
    }
}

use crate::domain::nginx::NginxSnapshot;
use crate::domain::ssh::SshConnectionManager;
use crate::errors::{AppError, AppResult};
use crate::security::{redact, shell_escape, CredentialStore};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use chrono::Timelike;
use hmac::{Hmac, KeyInit, Mac};
use reqwest::Client;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use sqlx::FromRow;
use std::sync::Arc;
use std::time::Duration;

/// 高级能力快照；WAF 状态来自真实 Nginx/OpenResty 编译参数，而不是前端占位值。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdvancedSnapshot {
    pub waf_enabled: bool,
    pub waf_provider: Option<String>,
    pub monitoring_supported: bool,
    pub warnings: Vec<String>,
    pub fetched_at: chrono::DateTime<chrono::Utc>,
}

/// 远程 HTTP 探活请求；URL 只用于一次性检查，不会自动保存或访问本地客户端网络。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpMonitorInput {
    pub server_id: String,
    pub url: String,
    pub expected_status: Option<u16>,
}

/// 返回远程 curl 探活结果和延迟，响应体不会传回客户端。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpMonitorResult {
    pub url: String,
    pub reachable: bool,
    pub status_code: Option<u16>,
    pub latency_ms: Option<u64>,
    pub detail: String,
    pub checked_at: chrono::DateTime<chrono::Utc>,
}

/// Persisted website probe definition; only URL metadata and the latest result are stored locally.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpMonitorProfile {
    pub id: String,
    pub server_id: String,
    pub name: String,
    pub url: String,
    pub expected_status: Option<u16>,
    pub interval_seconds: u32,
    pub enabled: bool,
    pub last_checked_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_reachable: Option<bool>,
    pub last_status_code: Option<u16>,
    pub last_latency_ms: Option<u64>,
    pub last_detail: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Saves or updates one scheduled HTTP monitor definition.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveHttpMonitorInput {
    pub id: Option<String>,
    pub server_id: String,
    pub name: String,
    pub url: String,
    pub expected_status: Option<u16>,
    pub interval_seconds: u32,
    pub enabled: bool,
}

/// One bounded local history row for a monitor; response bodies are never stored.
#[derive(Debug, Clone, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct HttpMonitorCheck {
    pub id: i64,
    pub monitor_id: String,
    pub checked_at: chrono::DateTime<chrono::Utc>,
    pub reachable: bool,
    pub status_code: Option<u16>,
    pub latency_ms: Option<u64>,
    pub detail: String,
}

/// 描述 ModSecurity 配置中的一条 SecRule/SecAction 规则。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WafRule {
    pub line_number: u32,
    pub directive: String,
    pub source_path: String,
}

/// 返回远端 WAF 规则文件和受限规则摘要。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WafRulesSnapshot {
    pub supported: bool,
    pub provider: Option<String>,
    pub config_path: Option<String>,
    /// 规则文件实际所在目标；容器目标的 path 是容器内路径，避免把宿主机挂载路径误当成容器路径。
    pub target: Option<WafRuleTarget>,
    pub rules: Vec<WafRule>,
    pub warnings: Vec<String>,
}

/// 描述 WAF 规则文件位于宿主机还是指定 Web 服务器容器内。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WafRuleTarget {
    pub path: String,
    pub container_id: Option<String>,
}

/// One bounded WAF audit-log alert summary; raw request bodies and full log lines are never returned.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WafAlert {
    pub source_path: String,
    pub summary: String,
    pub severity: String,
    pub fingerprint: String,
}

/// 本地保存的 WAF 告警聚合项，只包含脱敏摘要和出现次数。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WafAlertHistoryEntry {
    pub source_path: String,
    pub summary: String,
    pub severity: String,
    pub fingerprint: String,
    pub first_seen_at: chrono::DateTime<chrono::Utc>,
    pub last_seen_at: chrono::DateTime<chrono::Utc>,
    pub occurrences: u32,
}

/// 本地保存的每小时 WAF 告警计数，用于趋势图而不保存请求正文或原始日志。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WafAlertTrendPoint {
    pub bucket_at: chrono::DateTime<chrono::Utc>,
    pub warning: u32,
    pub error: u32,
    pub critical: u32,
    pub total: u32,
}

/// WAF 告警过滤和通知设置；第三方 webhook URL 只进入系统密钥链。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WafAlertSettings {
    pub min_severity: String,
    pub notify_in_app: bool,
    pub history_limit: u32,
    pub notify_webhook: bool,
    pub notify_provider: String,
    pub webhook_configured: bool,
    pub signing_secret_configured: bool,
}

/// 保存 WAF 告警设置的输入；webhook URL 只通过 IPC 进入系统密钥链。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveWafAlertSettingsInput {
    pub min_severity: String,
    pub notify_in_app: bool,
    pub history_limit: u32,
    #[serde(default)]
    pub notify_webhook: bool,
    #[serde(default = "default_waf_webhook_provider")]
    pub notify_provider: String,
    #[serde(default)]
    pub webhook_url: Option<SecretString>,
    #[serde(default)]
    pub webhook_signing_secret: Option<SecretString>,
    #[serde(default)]
    pub clear_webhook: bool,
}

/// Returns recent real ModSecurity alert summaries from fixed candidate log paths.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WafAlertsSnapshot {
    pub supported: bool,
    pub sources: Vec<String>,
    pub alerts: Vec<WafAlert>,
    pub history: Vec<WafAlertHistoryEntry>,
    pub trend: Vec<WafAlertTrendPoint>,
    pub new_alerts: u32,
    pub webhook_sent: bool,
    pub webhook_error: Option<String>,
    pub settings: WafAlertSettings,
    pub warnings: Vec<String>,
    pub fetched_at: chrono::DateTime<chrono::Utc>,
}

/// WAF 规则增删请求；内容只允许单行 ModSecurity directive。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WafRuleActionInput {
    pub server_id: String,
    pub action: String,
    pub line_number: Option<u32>,
    pub rule: Option<String>,
    pub confirmed: bool,
}

/// 描述一个内置 WAF 防护策略；规则正文固定在 Rust 端，不接受前端拼接。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WafTemplate {
    pub id: String,
    pub name: String,
    pub description: String,
    pub risk: String,
    pub rule_id: u32,
    pub rule: String,
}

/// 应用一个内置 WAF 防护策略；仍沿用备份、配置测试、reload 和失败回滚流程。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WafTemplateActionInput {
    pub server_id: String,
    pub template_id: String,
    pub confirmed: bool,
}

/// 描述一个固定来源的第三方 WAF 规则集；URL、版本和 SHA-256 均由 Rust 后端维护。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WafRuleSource {
    pub id: String,
    pub name: String,
    pub channel: String,
    pub version: String,
    pub url: String,
    pub sha256: String,
    pub signature_fingerprint: String,
    pub supported: bool,
    pub installed_version: Option<String>,
    pub install_path: String,
    pub update_available: bool,
}

/// 返回第三方规则源、当前安装版本和远端能力提示。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WafRuleSourcesSnapshot {
    pub supported: bool,
    pub target: Option<WafRuleTarget>,
    pub sources: Vec<WafRuleSource>,
    pub warnings: Vec<String>,
    pub fetched_at: chrono::DateTime<chrono::Utc>,
}

/// 安装、更新或移除固定第三方 WAF 规则集的请求。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WafRuleSourceActionInput {
    pub server_id: String,
    pub source_id: String,
    pub action: String,
    pub confirmed: bool,
}

/// 返回第三方规则集变更结果，不包含下载内容或配置正文。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WafRuleSourceActionResult {
    pub source_id: String,
    pub action: String,
    pub version: Option<String>,
    pub install_path: String,
    pub output: String,
}

const WAF_ALERT_SETTINGS_PREFIX: &str = "waf.alert.settings.";
const WAF_ALERT_HISTORY_PREFIX: &str = "waf.alert.history.";
const WAF_ALERT_TREND_PREFIX: &str = "waf.alert.trend.";
const WAF_WEBHOOK_KEY_PREFIX: &str = "waf-webhook-";
const WAF_WEBHOOK_SECRET_KEY_PREFIX: &str = "waf-webhook-secret-";
const CRS_INSTALL_ROOT: &str = "/etc/1panel-client/waf/owasp-crs";
const CRS_VERSION_MARKER: &str = ".1panel-client-version";
const CRS_START_MARKER: &str = "# 1panel-client:owasp-crs:start";
const CRS_END_MARKER: &str = "# 1panel-client:owasp-crs:end";
const CRS_SIGNATURE_FINGERPRINT: &str = "36006F0E0BA167832158821138EEACA1AB8A6E72";

/// 返回兼容旧配置的默认通知渠道标识。
fn default_waf_webhook_provider() -> String {
    "generic".into()
}

/// 返回默认 WAF 告警策略：保留告警并在应用内提示 warning 及以上事件。
pub fn default_waf_alert_settings() -> WafAlertSettings {
    WafAlertSettings {
        min_severity: "warning".into(),
        notify_in_app: true,
        history_limit: 500,
        notify_webhook: false,
        notify_provider: default_waf_webhook_provider(),
        webhook_configured: false,
        signing_secret_configured: false,
    }
}

/// 读取指定服务器的本地 WAF 告警策略，缺失时返回默认值。
pub async fn get_waf_alert_settings(
    local: &crate::infra::local::LocalRepository,
    credentials: &Arc<dyn CredentialStore>,
    server_id: &str,
) -> AppResult<WafAlertSettings> {
    validate_local_server_id(server_id)?;
    let Some(value) = local
        .get_setting(&format!("{WAF_ALERT_SETTINGS_PREFIX}{server_id}"))
        .await?
    else {
        return Ok(WafAlertSettings {
            webhook_configured: credentials.get(&webhook_key(server_id)).is_ok(),
            signing_secret_configured: credentials.get(&webhook_secret_key(server_id)).is_ok(),
            ..default_waf_alert_settings()
        });
    };
    let input = serde_json::from_str::<SaveWafAlertSettingsInput>(&value).map_err(|error| {
        AppError::new("WAF_SETTINGS_INVALID", "advanced", "WAF 告警设置无法解析")
            .details(error)
            .for_server(server_id)
    })?;
    let mut settings =
        normalize_waf_alert_settings(input).map_err(|error| error.for_server(server_id))?;
    settings.webhook_configured = credentials.get(&webhook_key(server_id)).is_ok();
    settings.signing_secret_configured = credentials.get(&webhook_secret_key(server_id)).is_ok();
    Ok(settings)
}

/// 保存指定服务器的 WAF 告警过滤、通知和历史上限设置；URL 写入系统密钥链。
pub async fn save_waf_alert_settings(
    local: &crate::infra::local::LocalRepository,
    credentials: &Arc<dyn CredentialStore>,
    server_id: &str,
    input: SaveWafAlertSettingsInput,
) -> AppResult<WafAlertSettings> {
    validate_local_server_id(server_id)?;
    if let Some(url) = input.webhook_url.as_ref() {
        validate_waf_webhook_url(url.expose_secret())
            .map_err(|error| error.for_server(server_id))?;
        credentials.put(
            &webhook_key(server_id),
            SecretString::from(url.expose_secret().trim().to_owned()),
        )?;
    }
    if let Some(secret) = input.webhook_signing_secret.as_ref() {
        validate_waf_webhook_secret(secret.expose_secret())
            .map_err(|error| error.for_server(server_id))?;
        credentials.put(
            &webhook_secret_key(server_id),
            SecretString::from(secret.expose_secret().trim().to_owned()),
        )?;
    }
    if input.clear_webhook {
        credentials.delete(&webhook_key(server_id))?;
        credentials.delete(&webhook_secret_key(server_id))?;
    }
    let mut settings =
        normalize_waf_alert_settings(input).map_err(|error| error.for_server(server_id))?;
    settings.webhook_configured = credentials.get(&webhook_key(server_id)).is_ok();
    settings.signing_secret_configured = credentials.get(&webhook_secret_key(server_id)).is_ok();
    let value = serde_json::to_string(&settings).map_err(AppError::database)?;
    local
        .set_setting(&format!("{WAF_ALERT_SETTINGS_PREFIX}{server_id}"), &value)
        .await?;
    Ok(settings)
}

/// 删除指定服务器的本地 WAF 告警历史和趋势，不触碰远端日志或规则。
pub async fn clear_waf_alert_history(
    local: &crate::infra::local::LocalRepository,
    server_id: &str,
) -> AppResult<()> {
    validate_local_server_id(server_id)?;
    local
        .delete_setting(&format!("{WAF_ALERT_HISTORY_PREFIX}{server_id}"))
        .await?;
    local
        .delete_setting(&format!("{WAF_ALERT_TREND_PREFIX}{server_id}"))
        .await
}

/// 读取 WAF 编译模块和远程探活工具的真实能力。
pub async fn snapshot(ssh: &SshConnectionManager, server_id: &str) -> AppResult<AdvancedSnapshot> {
    let nginx = crate::domain::nginx::snapshot(ssh, server_id).await?;
    let command = waf_probe_command(&nginx);
    let result = ssh
        .execute_system(server_id, &command, Duration::from_secs(20))
        .await?;
    let mut warnings = Vec::new();
    let (waf_enabled, waf_provider) = parse_waf_probe(&result.stdout);
    if !nginx.installed {
        warnings.push("远端没有 Nginx/OpenResty，WAF 状态不可用".into());
    } else if !waf_enabled {
        warnings
            .push("未检测到 ModSecurity 或 OpenResty WAF 模块；可继续使用安全中心和防火墙".into());
    }
    if result.exit_code != 0 && result.stderr.trim().is_empty() {
        warnings.push("无法读取 Web 服务器编译模块".into());
    }
    Ok(AdvancedSnapshot {
        waf_enabled,
        waf_provider,
        monitoring_supported: result.stdout.contains("__CURL__yes"),
        warnings,
        fetched_at: chrono::Utc::now(),
    })
}

/// 读取固定候选路径中的 ModSecurity 规则摘要，不回传完整配置文件。
pub async fn waf_rules(ssh: &SshConnectionManager, server_id: &str) -> AppResult<WafRulesSnapshot> {
    let nginx = crate::domain::nginx::snapshot(ssh, server_id).await?;
    let result = ssh
        .execute_system(
            server_id,
            &waf_rules_probe_command(&nginx),
            Duration::from_secs(30),
        )
        .await?;
    if result.exit_code != 0 {
        return Err(
            AppError::new("WAF_PROBE_FAILED", "advanced", "读取 WAF 规则失败")
                .details(result.stderr)
                .for_server(server_id),
        );
    }
    let (target, rules) = parse_waf_rules(&result.stdout);
    let config_path = target.as_ref().map(|value| value.path.clone());
    let (enabled, provider) = parse_waf_probe(&result.stdout);
    let mut warnings = Vec::new();
    if !enabled && target.is_none() {
        warnings.push("未找到 ModSecurity 规则文件或 WAF 编译模块".into());
    } else if target.is_none() {
        warnings.push("检测到 WAF 编译能力，但没有固定候选规则文件".into());
    }
    Ok(WafRulesSnapshot {
        supported: target.is_some(),
        provider,
        config_path,
        target,
        rules,
        warnings,
    })
}

/// Returns conservative built-in WAF strategies; callers may review each rule before applying it.
pub fn waf_templates() -> Vec<WafTemplate> {
    vec![
        WafTemplate {
            id: "sensitive-files".into(),
            name: "保护敏感文件".into(),
            description: "阻止访问 .env、.git 和常见依赖锁文件，避免误泄露部署凭据。".into(),
            risk: "可能拦截需要公开下载的同名文件，请先确认站点路由。".into(),
            rule_id: 1_001_001,
            rule: r#"SecRule REQUEST_URI "@rx ^/(?:\.env(?:\.|/|$)|\.git(?:/|$)|composer\.json(?:\.lock)?$|package-lock\.json$)" "id:1001001,phase:1,deny,status:403,log,msg:'1Panel Client sensitive file protection'"#.into(),
        },
        WafTemplate {
            id: "dangerous-methods".into(),
            name: "限制危险 HTTP 方法".into(),
            description: "拒绝 TRACE 和 CONNECT 请求，降低反射和代理滥用风险。".into(),
            risk: "需要 API 调试 TRACE/CONNECT 的站点不应启用。".into(),
            rule_id: 1_001_002,
            rule: r#"SecRule REQUEST_METHOD "@rx ^(?:TRACE|CONNECT)$" "id:1001002,phase:1,deny,status:405,log,msg:'1Panel Client dangerous method protection'"#.into(),
        },
        WafTemplate {
            id: "known-scanners".into(),
            name: "拦截常见扫描器标识".into(),
            description: "按 User-Agent 拦截 sqlmap、nikto 等常见自动扫描器。".into(),
            risk: "安全测试、资产扫描或自定义健康检查可能使用这些标识。".into(),
            rule_id: 1_001_003,
            rule: r#"SecRule REQUEST_HEADERS:User-Agent "@rx (?i:(?:sqlmap|nikto|acunetix|nmap))" "id:1001003,phase:1,deny,status:403,log,msg:'1Panel Client scanner protection'"#.into(),
        },
        WafTemplate {
            id: "backup-leak-files".into(),
            name: "拦截常见备份与编辑器遗留文件".into(),
            description: "阻止访问 .bak/.old/.sql/.swp 等备份与编辑器遗留文件，避免配置或数据泄露。".into(),
            risk: "需要公开下载此类同名文件的站点不应启用。".into(),
            rule_id: 1_001_004,
            rule: r#"SecRule REQUEST_URI "@rx (?i)(?:\.(?:bak|old|sql|swp)$|/~$)" "id:1001004,phase:1,deny,status:403,log,msg:'1Panel Client backup/leak file protection'"#.into(),
        },
        WafTemplate {
            id: "path-traversal".into(),
            name: "拦截路径穿越".into(),
            description: "拒绝包含 ../、%2e%2e 等路径穿越特征，避免向站点目录之外读取文件。".into(),
            risk: "内部路由或静态资源可能使用包含连续点号的合法路径。".into(),
            rule_id: 1_001_005,
            rule: r#"SecRule REQUEST_URI|REQUEST_FILENAME "@rx (?i)(?:%2e%2e|\.\./|\.\.\\|/\.\./|/\.\.$)" "id:1001005,phase:1,deny,status:403,log,msg:'1Panel Client path traversal protection'"#.into(),
        },
        WafTemplate {
            id: "sql-injection".into(),
            name: "拦截常见 SQL 注入特征".into(),
            description: "按请求参数中的关键词拦截 union select、注入判断和延时函数等常见注入特征。".into(),
            risk: "表单、搜索或接口可能包含类似注入的合法文本，可能产生误报。".into(),
            rule_id: 1_001_006,
            rule: r#"SecRule ARGS|REQUEST_URI "@rx (?i)(?:union[\s+]+select|select[\s+]+[^;]*[\s+]+from|insert[\s+]+into|(?:or|and)[\s+]+[0-9]+=[0-9]+|sleep[\s+]*\()" "id:1001006,phase:2,deny,status:403,log,msg:'1Panel Client SQL injection protection'"#.into(),
        },
        WafTemplate {
            id: "cross-site-scripting".into(),
            name: "拦截常见跨站脚本特征".into(),
            description: "按请求参数拦截 <script、javascript:、事件处理器和 iframe 等常见 XSS 特征。".into(),
            risk: "富文本或安全公告可能包含此类片段，可能产生误报。".into(),
            rule_id: 1_001_007,
            rule: r#"SecRule ARGS|REQUEST_URI "@rx (?i)(?:<script|javascript:|onerror[\s+]*=|onload[\s+]*=|onclick[\s+]*=|<iframe)" "id:1001007,phase:2,deny,status:403,log,msg:'1Panel Client cross-site scripting protection'"#.into(),
        },
    ]
}

/// Reads bounded ModSecurity audit/error summaries from fixed host paths without exposing request payloads.
pub async fn waf_alerts(
    ssh: &SshConnectionManager,
    local: &crate::infra::local::LocalRepository,
    credentials: &Arc<dyn CredentialStore>,
    server_id: &str,
) -> AppResult<WafAlertsSnapshot> {
    let result = ssh
        .execute_system(
            server_id,
            &waf_alerts_probe_command(),
            Duration::from_secs(30),
        )
        .await?;
    if result.exit_code != 0 && result.stdout.trim().is_empty() {
        return Err(
            AppError::new("WAF_ALERT_PROBE_FAILED", "advanced", "读取 WAF 告警失败")
                .details(result.stderr)
                .for_server(server_id),
        );
    }
    let (sources, parsed_alerts) = parse_waf_alerts(&result.stdout);
    let trend = record_waf_alert_trend(local, server_id, &parsed_alerts).await?;
    let settings = get_waf_alert_settings(local, credentials, server_id).await?;
    let alerts = parsed_alerts
        .into_iter()
        .filter(|alert| severity_at_least(&alert.severity, &settings.min_severity))
        .collect::<Vec<_>>();
    let (history, new_alerts, new_alert_entries) =
        record_waf_alerts(local, server_id, &alerts, &settings).await?;
    let (webhook_sent, webhook_error) = if settings.notify_webhook && new_alerts > 0 {
        match credentials.get(&webhook_key(server_id)) {
            Ok(url) => {
                let signing_secret = credentials.get(&webhook_secret_key(server_id)).ok();
                match send_waf_webhook(
                    &url,
                    &settings.notify_provider,
                    signing_secret.as_ref(),
                    server_id,
                    &new_alert_entries,
                )
                .await
                {
                    Ok(()) => (true, None),
                    Err(error) => (false, Some(error.message)),
                }
            }
            Err(_) => (false, Some("webhook 未配置或系统密钥链不可用".into())),
        }
    } else {
        (false, None)
    };
    let mut warnings = Vec::new();
    if sources.is_empty() {
        warnings.push("未找到固定 ModSecurity 审计日志路径；容器内日志需单独配置同步".into());
    } else if alerts.is_empty() {
        warnings.push("已找到 WAF 日志，但最近 250 行没有识别到拒绝事件".into());
    }
    warnings.push(format!(
        "当前告警阈值：{}；本地历史 {} 条",
        settings.min_severity,
        history.len()
    ));
    if let Some(error) = &webhook_error {
        warnings.push(format!("外部 webhook 通知失败：{error}"));
    }
    Ok(WafAlertsSnapshot {
        supported: !sources.is_empty(),
        sources,
        alerts,
        history,
        trend,
        new_alerts,
        webhook_sent,
        webhook_error,
        settings,
        warnings,
        fetched_at: chrono::Utc::now(),
    })
}

/// 在规则文件存在且 Nginx 配置测试通过时增删一条受控 WAF 规则。
pub async fn waf_rule_action(
    ssh: &SshConnectionManager,
    input: WafRuleActionInput,
) -> AppResult<WafRulesSnapshot> {
    validate_waf_rule_action(&input)?;
    if !input.confirmed {
        return Err(AppError::new(
            "CONFIRMATION_REQUIRED",
            "advanced",
            "WAF 规则变更需要明确确认",
        )
        .for_server(&input.server_id));
    }
    let current = waf_rules(ssh, &input.server_id).await?;
    let target = current.target.clone().ok_or_else(|| {
        AppError::new(
            "WAF_UNSUPPORTED",
            "advanced",
            "远端没有可受控的 ModSecurity 规则文件",
        )
        .for_server(&input.server_id)
    })?;
    validate_waf_target(&target).map_err(|error| error.for_server(&input.server_id))?;
    let path = target.path;
    let nginx = crate::domain::nginx::snapshot(ssh, &input.server_id).await?;
    let backup = format!("{}.1panel-client-waf-backup-{}", path, uuid::Uuid::new_v4());
    let temporary = format!("{}.1panel-client-waf-tmp-{}", path, uuid::Uuid::new_v4());
    let (operation, install, restore, cleanup, control) =
        if let Some(container_id) = target.container_id.as_deref() {
            validate_waf_container_id(container_id)
                .map_err(|error| error.for_server(&input.server_id))?;
            let container = shell_escape(container_id);
            let operation = format!(
                "docker exec {container} sh -c {script}",
                container = container,
                script = shell_escape(&waf_rule_edit_script(&input, &path, &backup, &temporary,)),
            );
            let install = format!(
                "docker exec {container} sh -c {script}",
                container = container,
                script = shell_escape(&format!(
                    "cp -a -- {} {} && chmod 0644 -- {}",
                    shell_escape(&temporary),
                    shell_escape(&path),
                    shell_escape(&path)
                )),
            );
            let restore = format!(
                "docker exec {container} sh -c {script}",
                container = container,
                script = shell_escape(&format!(
                    "cp -a -- {} {}",
                    shell_escape(&backup),
                    shell_escape(&path)
                )),
            );
            let cleanup = format!(
                "docker exec {container} sh -c {script}",
                container = container,
                script = shell_escape(&format!(
                    "rm -f -- {} {}",
                    shell_escape(&temporary),
                    shell_escape(&backup)
                )),
            );
            let control = format!("docker exec {} openresty", shell_escape(container_id));
            (operation, install, restore, cleanup, control)
        } else {
            let operation = waf_rule_edit_script(&input, &path, &backup, &temporary);
            let control = nginx_control(&nginx);
            (
                operation,
                format!(
                    "install -m 0644 -- {} {}",
                    shell_escape(&temporary),
                    shell_escape(&path)
                ),
                format!("cp -a -- {} {}", shell_escape(&backup), shell_escape(&path)),
                format!(
                    "rm -f -- {} {}",
                    shell_escape(&temporary),
                    shell_escape(&backup)
                ),
                control,
            )
        };
    let command = format!(
        "set -e; {operation}; {install}; {cleanup_tmp}; if ! {control} -t; then {restore}; {cleanup}; {control} -t >/dev/null 2>&1 || true; exit 43; fi; if ! {control} -s reload; then {restore}; {cleanup}; {control} -t >/dev/null 2>&1 || true; exit 44; fi; {cleanup}",
        operation = operation,
        install = install,
        cleanup_tmp = if target.container_id.is_some() {
            format!(
                "docker exec {} sh -c {}",
                shell_escape(target.container_id.as_deref().unwrap_or_default()),
                shell_escape(&format!("rm -f -- {}", shell_escape(&temporary)))
            )
        } else {
            format!("rm -f -- {}", shell_escape(&temporary))
        },
        control = control,
        restore = restore,
        cleanup = cleanup,
    );
    let result = ssh
        .execute_system(&input.server_id, &command, Duration::from_secs(90))
        .await?;
    if result.exit_code != 0 {
        return Err(AppError::new(
            "WAF_ACTION_FAILED",
            "advanced",
            "WAF 规则变更或 Nginx reload 失败，已恢复备份",
        )
        .details(result.stderr)
        .for_server(input.server_id));
    }
    waf_rules(ssh, &input.server_id).await
}

/// Applies one built-in WAF strategy after rejecting duplicate rule IDs and requiring confirmation.
pub async fn waf_template_action(
    ssh: &SshConnectionManager,
    input: WafTemplateActionInput,
) -> AppResult<WafRulesSnapshot> {
    validate_waf_template_action(&input)?;
    if !input.confirmed {
        return Err(AppError::new(
            "CONFIRMATION_REQUIRED",
            "advanced",
            "WAF 模板变更需要明确确认",
        )
        .for_server(&input.server_id));
    }
    let template = waf_templates()
        .into_iter()
        .find(|value| value.id == input.template_id)
        .ok_or_else(|| AppError::new("WAF_TEMPLATE_NOT_FOUND", "advanced", "WAF 模板不存在"))?;
    let current = waf_rules(ssh, &input.server_id).await?;
    let rule_id = format!("id:{}", template.rule_id);
    if current
        .rules
        .iter()
        .any(|rule| rule.directive.to_ascii_lowercase().contains(&rule_id))
    {
        return Err(AppError::new(
            "WAF_TEMPLATE_ALREADY_APPLIED",
            "advanced",
            "该 WAF 模板的规则 ID 已存在，未重复写入",
        )
        .for_server(&input.server_id));
    }
    waf_rule_action(
        ssh,
        WafRuleActionInput {
            server_id: input.server_id,
            action: "add".into(),
            line_number: None,
            rule: Some(template.rule),
            confirmed: true,
        },
    )
    .await
}

/// 返回客户端内置的 OWASP CRS LTS 与最新稳定规则源定义。
pub fn waf_rule_source_definitions() -> Vec<WafRuleSource> {
    vec![
        WafRuleSource {
            id: "owasp-crs-4.25-lts".into(),
            name: "OWASP Core Rule Set 4.25.1 LTS".into(),
            channel: "LTS".into(),
            version: "4.25.1".into(),
            url: "https://github.com/coreruleset/coreruleset/archive/refs/tags/v4.25.1.tar.gz"
                .into(),
            sha256: "0539e66e7627fe71c160a644d8fb7ab6e450d53c9de208be5f95a35c70e1a154".into(),
            signature_fingerprint: CRS_SIGNATURE_FINGERPRINT.into(),
            supported: false,
            installed_version: None,
            install_path: CRS_INSTALL_ROOT.into(),
            update_available: false,
        },
        WafRuleSource {
            id: "owasp-crs-4.28".into(),
            name: "OWASP Core Rule Set 4.28.0".into(),
            channel: "Stable".into(),
            version: "4.28.0".into(),
            url: "https://github.com/coreruleset/coreruleset/archive/refs/tags/v4.28.0.tar.gz"
                .into(),
            sha256: "d8acc96f25ad07c8e3a595a23c797324f6d77e59ddf9e26e90dd95ebd2e676ce".into(),
            signature_fingerprint: CRS_SIGNATURE_FINGERPRINT.into(),
            supported: false,
            installed_version: None,
            install_path: CRS_INSTALL_ROOT.into(),
            update_available: false,
        },
    ]
}

/// 探测 WAF 规则目标和专用 CRS 目录，不读取规则正文或远程密钥。
pub async fn waf_rule_sources(
    ssh: &SshConnectionManager,
    server_id: &str,
) -> AppResult<WafRuleSourcesSnapshot> {
    let current = waf_rules(ssh, server_id).await?;
    let mut warnings = current.warnings.clone();
    let installed_version = if let Some(target) = current.target.as_ref() {
        probe_crs_version(ssh, server_id, target).await?
    } else {
        warnings.push("未找到可编辑的 ModSecurity 配置文件，第三方 CRS 仅可查看来源信息".into());
        None
    };
    let supported = current.target.is_some();
    let sources = waf_rule_source_definitions()
        .into_iter()
        .map(|mut source| {
            source.supported = supported;
            source.installed_version = installed_version.clone();
            source.update_available = supported
                && installed_version
                    .as_deref()
                    .is_some_and(|version| version != source.version);
            source
        })
        .collect();
    Ok(WafRuleSourcesSnapshot {
        supported,
        target: current.target,
        sources,
        warnings,
        fetched_at: chrono::Utc::now(),
    })
}

/// 在远端以固定 SHA-256 校验规则包，并通过配置测试/reload 后原子启用或移除 CRS。
pub async fn waf_rule_source_action(
    ssh: &SshConnectionManager,
    input: WafRuleSourceActionInput,
) -> AppResult<WafRuleSourceActionResult> {
    validate_waf_rule_source_action(&input)?;
    if !input.confirmed {
        return Err(AppError::new(
            "CONFIRMATION_REQUIRED",
            "advanced",
            "第三方 WAF 规则集变更需要明确确认",
        )
        .for_server(input.server_id));
    }
    let source = waf_rule_source_definitions()
        .into_iter()
        .find(|value| value.id == input.source_id)
        .ok_or_else(|| AppError::new("WAF_SOURCE_NOT_FOUND", "advanced", "WAF 规则源不存在"))?;
    let current = waf_rules(ssh, &input.server_id).await?;
    let target = current.target.clone().ok_or_else(|| {
        AppError::new(
            "WAF_UNSUPPORTED",
            "advanced",
            "远端没有可受控的 ModSecurity 配置文件",
        )
        .for_server(&input.server_id)
    })?;
    validate_waf_target(&target).map_err(|error| error.for_server(&input.server_id))?;
    let nginx = crate::domain::nginx::snapshot(ssh, &input.server_id).await?;
    let command = build_crs_source_action_command(&target, &nginx, &source, &input.action)?;
    let result = ssh
        .execute_system(&input.server_id, &command, Duration::from_secs(180))
        .await?;
    if result.exit_code != 0 {
        return Err(AppError::new(
            "WAF_CRS_ACTION_FAILED",
            "advanced",
            "第三方 WAF 规则集变更失败，已恢复原配置",
        )
        .details(redact(&result.stderr))
        .for_server(input.server_id));
    }
    let installed = (input.action != "remove").then(|| source.version.clone());
    Ok(WafRuleSourceActionResult {
        source_id: input.source_id,
        action: input.action,
        version: installed,
        install_path: CRS_INSTALL_ROOT.into(),
        output: redact(&result.stdout).chars().take(2_000).collect(),
    })
}

/// 校验第三方规则源操作只接受后端内置的来源和有限动作。
fn validate_waf_rule_source_action(input: &WafRuleSourceActionInput) -> AppResult<()> {
    if input.server_id.is_empty()
        || input.server_id.len() > 128
        || !input
            .server_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        || !matches!(input.action.as_str(), "install" | "update" | "remove")
        || !waf_rule_source_definitions()
            .iter()
            .any(|source| source.id == input.source_id)
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "advanced",
            "第三方 WAF 规则源或操作无效",
        )
        .for_server(&input.server_id));
    }
    Ok(())
}

/// 读取专用 CRS 目录中的版本 marker；规则内容永远不通过 IPC 返回。
async fn probe_crs_version(
    ssh: &SshConnectionManager,
    server_id: &str,
    target: &WafRuleTarget,
) -> AppResult<Option<String>> {
    validate_waf_target(target).map_err(|error| error.for_server(server_id))?;
    let marker = shell_escape(&format!("{CRS_INSTALL_ROOT}/{CRS_VERSION_MARKER}"));
    let command = if let Some(container_id) = target.container_id.as_deref() {
        validate_waf_container_id(container_id).map_err(|error| error.for_server(server_id))?;
        format!(
            "docker exec {} sh -c {}",
            shell_escape(container_id),
            shell_escape(&format!(
                "if [ -r {marker} ]; then printf '__CRS_VERSION__\\t%s\\n' \"$(cat -- {marker})\"; fi",
            )),
        )
    } else {
        format!(
            "if [ -r {marker} ]; then printf '__CRS_VERSION__\\t%s\\n' \"$(cat -- {marker})\"; fi"
        )
    };
    let result = ssh
        .execute_system(server_id, &command, Duration::from_secs(20))
        .await?;
    if result.exit_code != 0 {
        return Ok(None);
    }
    Ok(parse_crs_version(&result.stdout))
}

/// 解析 CRS 版本 marker，并限制版本字段长度和字符集。
fn parse_crs_version(output: &str) -> Option<String> {
    output.lines().find_map(|line| {
        let value = line.strip_prefix("__CRS_VERSION__\t")?.trim();
        (value.len() <= 32
            && !value.is_empty()
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-')))
        .then(|| value.to_string())
    })
}

/// 构造第三方 CRS 的受控安装/更新/移除命令，并在容器目标中保持所有文件操作位于容器内。
fn build_crs_source_action_command(
    target: &WafRuleTarget,
    nginx: &NginxSnapshot,
    source: &WafRuleSource,
    action: &str,
) -> AppResult<String> {
    validate_waf_target(target)?;
    if !matches!(action, "install" | "update" | "remove") {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "advanced",
            "第三方 WAF 规则源操作无效",
        ));
    }
    let control = if target.container_id.is_some() {
        "openresty".to_string()
    } else {
        nginx_control(nginx)
    };
    let script = build_crs_source_script(&target.path, source, action, &control);
    if let Some(container_id) = target.container_id.as_deref() {
        validate_waf_container_id(container_id)?;
        Ok(format!(
            "docker exec {} sh -c {}",
            shell_escape(container_id),
            shell_escape(&script)
        ))
    } else {
        Ok(script)
    }
}

/// 生成具备备份、固定 SHA-256 校验、配置测试、reload 和退出回滚的 CRS shell 脚本。
fn build_crs_source_script(
    config_path: &str,
    source: &WafRuleSource,
    action: &str,
    control: &str,
) -> String {
    let config = shell_escape(config_path);
    let root = shell_escape(CRS_INSTALL_ROOT);
    let marker = shell_escape(CRS_VERSION_MARKER);
    let start = shell_escape(CRS_START_MARKER);
    let end = shell_escape(CRS_END_MARKER);
    let url = shell_escape(&source.url);
    let sha256 = shell_escape(&source.sha256);
    let version = shell_escape(&source.version);
    let control = control.to_string();
    let mut script = String::from("set -eu; ");
    script.push_str(&format!(
        "config={config}; root={root}; backup_config=\"$config.1panel-client-crs-backup-$$\"; backup_root=\"$root.1panel-client-crs-backup-$$\"; temporary=\"$config.1panel-client-crs-tmp-$$\"; archive=\"/tmp/1panel-client-crs-$$.tar.gz\"; stage=\"/tmp/1panel-client-crs-stage-$$\"; marker={marker}; start={start}; end={end}; ok=0; ",
        config = config,
        root = root,
        marker = marker,
        start = start,
        end = end,
    ));
    script.push_str(
        "rollback() { status=$?; if [ \"$ok\" -ne 1 ]; then if [ -e \"$backup_config\" ]; then cp -a -- \"$backup_config\" \"$config\" 2>/dev/null || true; fi; if [ -e \"$backup_root\" ]; then rm -rf -- \"$root\" 2>/dev/null || true; mv -- \"$backup_root\" \"$root\" 2>/dev/null || true; fi; fi; rm -rf -- \"$stage\" 2>/dev/null || true; rm -f -- \"$archive\" \"$temporary\" 2>/dev/null || true; exit \"$status\"; }; trap rollback EXIT; ",
    );
    script.push_str(
        "[ -f \"$config\" ] || { printf '%s\\n' 'WAF 配置文件不存在' >&2; exit 20; }; cp -a -- \"$config\" \"$backup_config\"; ",
    );
    match action {
        "install" | "update" => {
            script.push_str(
                "if [ -e \"$root\" ]; then mv -- \"$root\" \"$backup_root\"; fi; mkdir -p -- \"$stage\"; ",
            );
            script.push_str(&format!(
                "if command -v curl >/dev/null 2>&1; then curl --fail --silent --show-error --location --connect-timeout 15 --max-time 180 --output \"$archive\" -- {url}; elif command -v wget >/dev/null 2>&1; then wget --timeout=20 --tries=2 --output-document=\"$archive\" -- {url}; else printf '%s\\n' '远端缺少 curl 或 wget' >&2; exit 21; fi; ",
                url = url,
            ));
            script.push_str(&format!(
                "if command -v sha256sum >/dev/null 2>&1; then actual=$(sha256sum -- \"$archive\" | awk '{{print $1}}'); elif command -v shasum >/dev/null 2>&1; then actual=$(shasum -a 256 -- \"$archive\" | awk '{{print $1}}'); else printf '%s\\n' '远端缺少 SHA-256 校验工具' >&2; exit 22; fi; [ \"$actual\" = {sha256} ] || {{ printf '%s\\n' 'CRS SHA-256 校验失败' >&2; exit 23; }}; ",
                sha256 = sha256,
            ));
            script.push_str(
                "if tar --help 2>&1 | grep -q -- '--no-same-owner'; then tar --no-same-owner -xzf \"$archive\" -C \"$stage\" --strip-components=1; else tar -xzf \"$archive\" -C \"$stage\" --strip-components=1; fi; [ -f \"$stage/crs-setup.conf.example\" ] || { printf '%s\\n' 'CRS 压缩包结构不完整' >&2; exit 24; }; cp -a -- \"$stage/crs-setup.conf.example\" \"$stage/crs-setup.conf\"; printf '%s\\n' ",
            );
            script.push_str(&version);
            script.push_str(
                " > \"$stage/$marker\"; chmod -R u=rwX,go=rX -- \"$stage\"; mv -- \"$stage\" \"$root\"; ",
            );
            script
                .push_str("if [ ! -r \"$root/$marker\" ] || [ \"$(cat -- \"$root/$marker\")\" != ");
            script.push_str(&version);
            script
                .push_str(" ] ; then printf '%s\\n' 'CRS 版本 marker 校验失败' >&2; exit 26; fi; ");
        }
        "remove" => {
            script.push_str(
                "[ -e \"$root\" ] || { printf '%s\\n' 'CRS 尚未安装' >&2; exit 25; }; mv -- \"$root\" \"$backup_root\"; ",
            );
        }
        _ => unreachable!(),
    }
    script.push_str(
        "awk -v start=\"$start\" -v end=\"$end\" ' $0 == start { skip=1; next } $0 == end { skip=0; next } !skip { print } ' \"$config\" > \"$temporary\"; ",
    );
    if action != "remove" {
        script.push_str(
            "printf '\\n%s\\nInclude /etc/1panel-client/waf/owasp-crs/crs-setup.conf\\nInclude /etc/1panel-client/waf/owasp-crs/rules/*.conf\\n%s\\n' \"$start\" \"$end\" >> \"$temporary\"; ",
        );
    }
    script.push_str("install -m 0644 -- \"$temporary\" \"$config\"; rm -f -- \"$temporary\"; ");
    script.push_str(&format!(
        "if ! {control} -t >/dev/null 2>&1; then exit 43; fi; if ! {control} -s reload >/dev/null 2>&1; then exit 44; fi; ok=1; rm -f -- \"$backup_config\"; rm -rf -- \"$backup_root\"; ",
        control = control,
    ));
    script
}

/// 从远程服务器发起一次受控 HTTP/HTTPS 探活，只返回状态码和延迟。
pub async fn probe_http(
    ssh: &SshConnectionManager,
    input: HttpMonitorInput,
) -> AppResult<HttpMonitorResult> {
    validate_monitor_input(&input)?;
    let url = shell_escape(&input.url);
    let command = format!(
        "if ! command -v curl >/dev/null 2>&1; then printf '__CURL__missing\\n'; exit 127; fi; curl --silent --show-error --location --max-time 15 --connect-timeout 5 --output /dev/null --write-out '__STATUS__%{{http_code}}\\n__TIME__%{{time_total}}\\n' -- {url}"
    );
    let result = ssh
        .execute_system(&input.server_id, &command, Duration::from_secs(25))
        .await?;
    if result.stdout.contains("__CURL__missing") {
        return Err(AppError::new(
            "TOOL_MISSING",
            "advanced",
            "远端没有 curl，无法执行站点探活",
        )
        .for_server(input.server_id));
    }
    let status_code = result
        .stdout
        .lines()
        .find_map(|line| line.strip_prefix("__STATUS__")?.parse::<u16>().ok());
    let latency_ms = result.stdout.lines().find_map(|line| {
        let value = line.strip_prefix("__TIME__")?.parse::<f64>().ok()?;
        Some((value * 1000.0).round() as u64)
    });
    let reachable = result.exit_code == 0
        && status_code.is_some_and(|status| {
            input
                .expected_status
                .map(|expected| status == expected)
                .unwrap_or((200..500).contains(&status))
        });
    let detail = if result.exit_code == 0 {
        if reachable {
            "HTTP 探活通过".into()
        } else {
            "HTTP 返回状态不符合预期".into()
        }
    } else {
        crate::security::redact(&result.stderr).trim().to_string()
    };
    Ok(HttpMonitorResult {
        url: input.url,
        reachable,
        status_code,
        latency_ms,
        detail,
        checked_at: chrono::Utc::now(),
    })
}

/// Validates a persisted monitor definition before it can be scheduled or sent to SQLite.
pub fn validate_http_monitor(input: &SaveHttpMonitorInput) -> AppResult<()> {
    if input.server_id.trim().is_empty()
        || input.server_id.len() > 128
        || input.name.trim().is_empty()
        || input.name.chars().count() > 120
        || input.interval_seconds < 30
        || input.interval_seconds > 86_400
        || input
            .id
            .as_deref()
            .is_some_and(|id| id.trim().is_empty() || id.len() > 128)
        || input
            .expected_status
            .is_some_and(|status| !(100..=599).contains(&status))
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "advanced",
            "探活任务名称、间隔或状态码无效",
        )
        .for_server(&input.server_id));
    }
    validate_monitor_input(&HttpMonitorInput {
        server_id: input.server_id.clone(),
        url: input.url.clone(),
        expected_status: input.expected_status,
    })
}

/// Runs one persisted monitor remotely, records success or failure, and returns the real probe result.
pub async fn run_saved_monitor(
    ssh: &SshConnectionManager,
    local: &crate::infra::local::LocalRepository,
    monitor_id: &str,
) -> AppResult<HttpMonitorResult> {
    let profile = local.http_monitor_by_id(monitor_id).await?;
    let input = HttpMonitorInput {
        server_id: profile.server_id.clone(),
        url: profile.url.clone(),
        expected_status: profile.expected_status,
    };
    match probe_http(ssh, input).await {
        Ok(result) => {
            local.record_http_monitor_check(monitor_id, &result).await?;
            Ok(result)
        }
        Err(error) => {
            let failure = HttpMonitorResult {
                url: profile.url,
                reachable: false,
                status_code: None,
                latency_ms: None,
                detail: format!("探活执行失败：{}", error.message),
                checked_at: chrono::Utc::now(),
            };
            if let Err(record_error) = local.record_http_monitor_check(monitor_id, &failure).await {
                tracing::warn!(error = %record_error, monitor_id, "写入探活失败历史失败");
            }
            Err(error)
        }
    }
}

/// Starts a lightweight persisted monitor loop; every tick only runs enabled profiles that are due.
pub fn spawn_monitor_scheduler(
    ssh: SshConnectionManager,
    local: crate::infra::local::LocalRepository,
) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(15));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            ticker.tick().await;
            let monitors = match local.list_http_monitors(None).await {
                Ok(monitors) => monitors,
                Err(error) => {
                    tracing::warn!(error = %error, "读取探活任务失败");
                    continue;
                }
            };
            let now = chrono::Utc::now();
            for monitor in monitors.into_iter().filter(|monitor| monitor.enabled) {
                let due = monitor
                    .last_checked_at
                    .map(|checked_at| {
                        now.signed_duration_since(checked_at).num_seconds()
                            >= i64::from(monitor.interval_seconds)
                    })
                    .unwrap_or(true);
                if due {
                    if let Err(error) = run_saved_monitor(&ssh, &local, &monitor.id).await {
                        tracing::warn!(error = %error, monitor_id = %monitor.id, "定时探活失败");
                    }
                }
            }
        }
    });
}

/// 构造固定 WAF 规则探测脚本，只访问预定义的 ModSecurity 配置路径。
fn waf_rules_probe_command(nginx: &NginxSnapshot) -> String {
    let paths = [
        "/etc/modsecurity/modsecurity.conf",
        "/etc/nginx/modsecurity.conf",
        "/etc/openresty/modsecurity.conf",
        "/opt/1panel/apps/openresty/openresty/conf/modsecurity.conf",
    ];
    let path_list = paths
        .iter()
        .map(|path| shell_escape(path))
        .collect::<Vec<_>>()
        .join(" ");
    if let Some(container_id) = nginx.container_id.as_deref() {
        let container_paths = [
            "/etc/modsecurity/modsecurity.conf",
            "/etc/nginx/modsecurity.conf",
            "/etc/openresty/modsecurity.conf",
            "/usr/local/openresty/nginx/conf/modsecurity.conf",
            "/usr/local/openresty/nginx/conf/conf.d/modsecurity.conf",
            "/opt/1panel/apps/openresty/openresty/conf/modsecurity.conf",
        ];
        let container_path_list = container_paths
            .iter()
            .map(|path| shell_escape(path))
            .collect::<Vec<_>>()
            .join(" ");
        return format!(
            "{}; container={container}; for path in {container_path_list}; do if docker exec {container} test -f \"$path\" >/dev/null 2>&1; then printf '__WAF_FILE__\\tcontainer\\t%s\\t%s\\n' {container} \"$path\"; docker exec {container} awk -v container_id={container} -v target_path=\"$path\" '/^[[:space:]]*(SecRule|SecAction|SecDefaultAction)[[:space:]]/ {{ line=$0; gsub(/[\\t]/, \" \", line); printf \"__WAF_RULE__\\tcontainer\\t%s\\t%s\\t%d\\t%s\\n\", container_id, target_path, NR, line }}' \"$path\"; break; fi; done",
            waf_probe_command(nginx),
            container = shell_escape(container_id),
            container_path_list = container_path_list,
        );
    }
    format!(
        "{}; for path in {path_list}; do if [ -f \"$path\" ]; then printf '__WAF_FILE__\\t%s\\n' \"$path\"; awk '/^[[:space:]]*(SecRule|SecAction|SecDefaultAction)[[:space:]]/ {{ line=$0; gsub(/[\\t]/, \" \", line); printf \"__WAF_RULE__\\t%s\\t%d\\t%s\\n\", FILENAME, NR, line }}' \"$path\"; break; fi; done",
        waf_probe_command(nginx),
        path_list = path_list,
    )
}

/// Builds a fixed-path, bounded WAF log probe; only lines containing denial markers are summarized.
fn waf_alerts_probe_command() -> String {
    let paths = [
        "/var/log/modsec_audit.log",
        "/var/log/modsecurity/audit.log",
        "/var/log/nginx/error.log",
        "/var/log/openresty/error.log",
    ];
    let path_list = paths
        .iter()
        .map(|path| shell_escape(path))
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        "set +e; for path in {path_list}; do if [ -f \"$path\" ]; then printf '__WAF_LOG__\\t%s\\n' \"$path\"; tail -n 250 -- \"$path\" | awk '/ModSecurity|mod_security|Access denied|access denied/ {{ line=$0; gsub(/[\\t\\r]/, \" \", line); if (length(line)>512) line=substr(line,1,512); printf \"__WAF_ALERT__\\t%s\\n\", line }}'; fi; done; if command -v docker >/dev/null 2>&1; then docker ps --format '{{{{.ID}}}}\\t{{{{.Names}}}}' | awk 'tolower($2) ~ /openresty|nginx|modsecurity|waf/ {{ print }}' | head -n 10 | while IFS='\\t' read -r container_id container_name; do [ -n \"$container_id\" ] || continue; printf '__WAF_CONTAINER__\\tdocker:%s\\n' \"$container_name\"; docker logs --tail 250 \"$container_id\" 2>&1 | awk '/ModSecurity|mod_security|Access denied|access denied/ {{ line=$0; gsub(/[\\t\\r]/, \" \", line); if (length(line)>512) line=substr(line,1,512); printf \"__WAF_ALERT__\\t%s\\n\", line }}'; done; fi",
        path_list = path_list,
    )
}

/// 解析规则文件 marker，保留行号和容器目标便于受控删除。
fn parse_waf_rules(output: &str) -> (Option<WafRuleTarget>, Vec<WafRule>) {
    let mut target = None;
    let mut rules = Vec::new();
    for line in output.lines() {
        let fields = line.split('\t').collect::<Vec<_>>();
        match fields.first().copied() {
            Some("__WAF_FILE__") => {
                target = parse_waf_target_marker(&fields);
            }
            Some("__WAF_RULE__") => {
                let (rule_target, line_number_index, directive_index) =
                    if fields.get(1) == Some(&"container") && fields.len() >= 6 {
                        (
                            Some(WafRuleTarget {
                                path: fields[3].to_string(),
                                container_id: Some(fields[2].to_string()),
                            }),
                            4,
                            5,
                        )
                    } else if fields.len() >= 4 {
                        (
                            Some(WafRuleTarget {
                                path: fields[1].to_string(),
                                container_id: None,
                            }),
                            2,
                            3,
                        )
                    } else {
                        (None, 0, 0)
                    };
                if let Some(rule_target) = rule_target {
                    target.get_or_insert(rule_target.clone());
                }
                if let Some(rule_target) = target.as_ref() {
                    if let Ok(line_number) = fields.get(line_number_index).unwrap_or(&"").parse() {
                        rules.push(WafRule {
                            source_path: rule_target.path.clone(),
                            line_number,
                            directive: fields.get(directive_index).unwrap_or(&"").to_string(),
                        });
                    }
                }
            }
            _ => {}
        }
    }
    (target, rules)
}

/// 将旧版宿主机 marker 和新版容器 marker 统一解析成规则目标。
fn parse_waf_target_marker(fields: &[&str]) -> Option<WafRuleTarget> {
    match fields {
        ["__WAF_FILE__", path] if !path.is_empty() => Some(WafRuleTarget {
            path: (*path).to_string(),
            container_id: None,
        }),
        ["__WAF_FILE__", "container", container_id, path]
            if !container_id.is_empty() && !path.is_empty() =>
        {
            Some(WafRuleTarget {
                path: (*path).to_string(),
                container_id: Some((*container_id).to_string()),
            })
        }
        _ => None,
    }
}

/// 生成宿主机或容器内都可执行的规则备份/编辑脚本；路径和规则内容由调用方统一 shell 转义。
fn waf_rule_edit_script(
    input: &WafRuleActionInput,
    path: &str,
    backup: &str,
    temporary: &str,
) -> String {
    let path = shell_escape(path);
    let backup = shell_escape(backup);
    let temporary = shell_escape(temporary);
    match input.action.as_str() {
        "add" => format!(
            "set -e; cp -a -- {path} {backup} && cp -a -- {path} {temporary} && printf '%s\\n' {rule} >> {temporary}",
            path = path,
            backup = backup,
            temporary = temporary,
            rule = shell_escape(input.rule.as_deref().unwrap_or_default())
        ),
        "delete" => format!(
            "set -e; cp -a -- {path} {backup} && cp -a -- {path} {temporary} && sed -i '{}d' {temporary}",
            input.line_number.unwrap_or_default(),
            path = path,
            backup = backup,
            temporary = temporary,
        ),
        _ => unreachable!(),
    }
}

/// 校验由远端探测返回的 Docker 容器 ID，避免把异常 marker 拼接进 docker exec。
fn validate_waf_container_id(value: &str) -> AppResult<()> {
    if value.is_empty() || value.len() > 128 || !value.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "advanced",
            "WAF 容器标识无效",
        ));
    }
    Ok(())
}

/// 校验规则目标只能落在固定的绝对路径，阻止异常远端 marker 参与文件操作。
fn validate_waf_target(target: &WafRuleTarget) -> AppResult<()> {
    if !target.path.starts_with('/')
        || target.path.len() > 512
        || target.path.contains("..")
        || target
            .path
            .chars()
            .any(|character| character == '\0' || character == '\n' || character == '\r')
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "advanced",
            "WAF 规则文件路径无效",
        ));
    }
    if let Some(container_id) = target.container_id.as_deref() {
        validate_waf_container_id(container_id)?;
    }
    Ok(())
}

/// Parses bounded WAF log markers and removes duplicate summaries before returning them to the UI.
fn parse_waf_alerts(output: &str) -> (Vec<String>, Vec<WafAlert>) {
    let mut sources = Vec::new();
    let mut alerts = Vec::new();
    for line in output.lines() {
        let mut fields = line.splitn(2, '\t');
        match fields.next() {
            Some("__WAF_LOG__") => {
                if let Some(path) = fields.next() {
                    let path = path.trim().to_string();
                    if !path.is_empty() && !sources.iter().any(|current| current == &path) {
                        sources.push(path);
                    }
                }
            }
            Some("__WAF_CONTAINER__") => {
                if let Some(container) = fields.next() {
                    let container = container.trim().to_string();
                    if !container.is_empty() && !sources.iter().any(|current| current == &container)
                    {
                        sources.push(container);
                    }
                }
            }
            Some("__WAF_ALERT__") => {
                if let Some(summary) = fields.next() {
                    let raw_summary = summary.trim();
                    let severity = classify_waf_severity(raw_summary);
                    let summary = sanitize_waf_alert_summary(raw_summary);
                    let source_path = sources.last().cloned().unwrap_or_default();
                    let fingerprint = waf_alert_fingerprint(&source_path, raw_summary);
                    if !alerts
                        .iter()
                        .any(|alert: &WafAlert| alert.fingerprint == fingerprint)
                    {
                        alerts.push(WafAlert {
                            source_path,
                            summary,
                            severity,
                            fingerprint,
                        });
                    }
                    if alerts.len() >= 100 {
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    (sources, alerts)
}

/// 将当前探测到的 WAF 事件聚合到 UTC 小时桶，并保留最近 336 个点供趋势图使用。
async fn record_waf_alert_trend(
    local: &crate::infra::local::LocalRepository,
    server_id: &str,
    alerts: &[WafAlert],
) -> AppResult<Vec<WafAlertTrendPoint>> {
    let key = format!("{WAF_ALERT_TREND_PREFIX}{server_id}");
    let mut trend = local
        .get_setting(&key)
        .await?
        .and_then(|value| serde_json::from_str::<Vec<WafAlertTrendPoint>>(&value).ok())
        .unwrap_or_default();
    let now = chrono::Utc::now();
    let bucket_at = now
        .with_minute(0)
        .and_then(|value| value.with_second(0))
        .and_then(|value| value.with_nanosecond(0))
        .unwrap_or(now);
    let (warning, error, critical) = count_waf_severities(alerts);
    let total = warning.saturating_add(error).saturating_add(critical);
    if let Some(point) = trend.iter_mut().find(|point| point.bucket_at == bucket_at) {
        point.warning = point.warning.max(warning);
        point.error = point.error.max(error);
        point.critical = point.critical.max(critical);
        point.total = point
            .warning
            .saturating_add(point.error)
            .saturating_add(point.critical);
    } else {
        trend.push(WafAlertTrendPoint {
            bucket_at,
            warning,
            error,
            critical,
            total,
        });
    }
    trend.sort_by_key(|point| point.bucket_at);
    if trend.len() > 336 {
        let excess = trend.len() - 336;
        trend.drain(..excess);
    }
    let value = serde_json::to_string(&trend).map_err(AppError::database)?;
    if value.len() > 512 * 1024 {
        return Err(AppError::new(
            "WAF_TREND_TOO_LARGE",
            "advanced",
            "WAF 趋势历史超过本地存储上限",
        ));
    }
    local.set_setting(&key, &value).await?;
    Ok(trend)
}

/// 统计一轮 WAF 事件的级别数量，供趋势聚合和解析器测试复用。
fn count_waf_severities(alerts: &[WafAlert]) -> (u32, u32, u32) {
    let warning = alerts
        .iter()
        .filter(|alert| alert.severity == "warning")
        .count() as u32;
    let error = alerts
        .iter()
        .filter(|alert| alert.severity == "error")
        .count() as u32;
    let critical = alerts
        .iter()
        .filter(|alert| alert.severity == "critical")
        .count() as u32;
    (warning, error, critical)
}

/// 根据脱敏前日志中的有限关键词推导告警级别，不返回原始日志正文。
fn classify_waf_severity(raw: &str) -> String {
    let lower = raw.to_ascii_lowercase();
    if lower.contains("critical") || lower.contains("emergency") || lower.contains("alert") {
        "critical".into()
    } else if lower.contains("error") || lower.contains("fatal") || lower.contains("denied") {
        "error".into()
    } else {
        "warning".into()
    }
}

/// 以稳定的 FNV-1a 摘要标识日志行，不持久化原始请求内容或完整日志正文。
fn waf_alert_fingerprint(source_path: &str, raw: &str) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in format!("{source_path}\n{raw}").bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

/// 读取、合并并有界保存脱敏 WAF 告警历史，返回本轮新增数量。
async fn record_waf_alerts(
    local: &crate::infra::local::LocalRepository,
    server_id: &str,
    alerts: &[WafAlert],
    settings: &WafAlertSettings,
) -> AppResult<(Vec<WafAlertHistoryEntry>, u32, Vec<WafAlert>)> {
    let key = format!("{WAF_ALERT_HISTORY_PREFIX}{server_id}");
    let mut history = local
        .get_setting(&key)
        .await?
        .and_then(|value| serde_json::from_str::<Vec<WafAlertHistoryEntry>>(&value).ok())
        .unwrap_or_default();
    let now = chrono::Utc::now();
    let mut new_alerts = 0;
    let mut new_alert_entries = Vec::new();
    for alert in alerts {
        if history
            .iter()
            .any(|entry| entry.fingerprint == alert.fingerprint)
        {
            continue;
        } else {
            new_alerts += 1;
            new_alert_entries.push(alert.clone());
            history.push(WafAlertHistoryEntry {
                source_path: alert.source_path.clone(),
                summary: alert.summary.clone(),
                severity: alert.severity.clone(),
                fingerprint: alert.fingerprint.clone(),
                first_seen_at: now,
                last_seen_at: now,
                occurrences: 1,
            });
        }
    }
    history.sort_by_key(|entry| std::cmp::Reverse(entry.last_seen_at));
    history.truncate(settings.history_limit as usize);
    let value = serde_json::to_string(&history).map_err(AppError::database)?;
    if value.len() > 2 * 1024 * 1024 {
        return Err(AppError::new(
            "WAF_HISTORY_TOO_LARGE",
            "advanced",
            "WAF 告警历史超过本地限制",
        ));
    }
    local.set_setting(&key, &value).await?;
    Ok((history, new_alerts, new_alert_entries))
}

/// 规范化 WAF 告警设置，限制阈值枚举和本地历史容量。
fn normalize_waf_alert_settings(input: SaveWafAlertSettingsInput) -> AppResult<WafAlertSettings> {
    if !matches!(
        input.min_severity.as_str(),
        "warning" | "error" | "critical"
    ) {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "advanced",
            "WAF 告警阈值无效",
        ));
    }
    if !(50..=2_000).contains(&input.history_limit) {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "advanced",
            "WAF 告警历史上限必须在 50 到 2000 条之间",
        ));
    }
    if !matches!(
        input.notify_provider.as_str(),
        "generic" | "slack" | "discord" | "dingtalk" | "wecom"
    ) {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "advanced",
            "WAF 通知渠道无效",
        ));
    }
    Ok(WafAlertSettings {
        min_severity: input.min_severity,
        notify_in_app: input.notify_in_app,
        history_limit: input.history_limit,
        notify_webhook: input.notify_webhook,
        notify_provider: input.notify_provider,
        webhook_configured: false,
        signing_secret_configured: false,
    })
}

/// 生成服务器专属 webhook 密钥链引用，不把服务器 ID 或 URL 写进明文设置值。
fn webhook_key(server_id: &str) -> String {
    format!("{WAF_WEBHOOK_KEY_PREFIX}{server_id}")
}

/// 生成服务器专属的 webhook 签名密钥链引用，不把签名密钥写入 SQLite。
fn webhook_secret_key(server_id: &str) -> String {
    format!("{WAF_WEBHOOK_SECRET_KEY_PREFIX}{server_id}")
}

/// 校验通用 WAF webhook URL，拒绝控制字符、空主机和非 HTTP(S) 协议。
fn validate_waf_webhook_url(value: &str) -> AppResult<()> {
    if value.is_empty()
        || value.len() > 2_048
        || value
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "advanced",
            "WAF webhook URL 无效",
        ));
    }
    let value = value.trim();
    let url = reqwest::Url::parse(value)
        .map_err(|_| AppError::new("VALIDATION_FAILED", "advanced", "WAF webhook URL 无效"))?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "advanced",
            "WAF webhook 必须是带主机的 HTTP(S) URL",
        ));
    }
    Ok(())
}

/// 校验可选的钉钉签名密钥，拒绝控制字符和过大的密钥值。
fn validate_waf_webhook_secret(value: &str) -> AppResult<()> {
    if value.is_empty()
        || value.len() > 512
        || value
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "advanced",
            "WAF webhook 签名密钥无效",
        ));
    }
    Ok(())
}

/// 将新增的脱敏告警摘要发送到配置的通用 webhook；失败只返回状态，不阻断 WAF 读取。
async fn send_waf_webhook(
    url: &SecretString,
    provider: &str,
    signing_secret: Option<&SecretString>,
    server_id: &str,
    alerts: &[WafAlert],
) -> AppResult<()> {
    let client = Client::builder()
        .timeout(Duration::from_secs(8))
        .build()
        .map_err(|error| {
            AppError::new(
                "WAF_WEBHOOK_FAILED",
                "advanced",
                "无法初始化 webhook 客户端",
            )
            .details(error)
        })?;
    let message = format_waf_notification_message(server_id, alerts);
    let payload = match provider {
        "slack" => serde_json::json!({"text": message}),
        "discord" => serde_json::json!({"content": message}),
        "dingtalk" => serde_json::json!({
            "msgtype": "text",
            "text": {"content": message},
        }),
        "wecom" => serde_json::json!({
            "msgtype": "text",
            "text": {"content": message},
        }),
        _ => serde_json::json!({
            "event": "waf_alerts",
            "serverId": server_id,
            "generatedAt": chrono::Utc::now(),
            "alerts": alerts.iter().take(20).collect::<Vec<_>>(),
        }),
    };
    let target_url = dingtalk_signed_url(url.expose_secret(), provider, signing_secret)?;
    let response = client
        .post(target_url)
        .header("User-Agent", "1panel-client")
        .json(&payload)
        .send()
        .await
        .map_err(|error| {
            AppError::new("WAF_WEBHOOK_FAILED", "advanced", "WAF webhook 请求失败").details(error)
        })?;
    if !response.status().is_success() {
        return Err(AppError::new(
            "WAF_WEBHOOK_FAILED",
            "advanced",
            "WAF webhook 返回非成功状态",
        )
        .details(format!("HTTP {}", response.status().as_u16())));
    }
    Ok(())
}

/// 生成各通知渠道共用的脱敏文本，不包含原始请求正文、URL 或完整日志行。
fn format_waf_notification_message(server_id: &str, alerts: &[WafAlert]) -> String {
    let mut message = format!("[1Panel Client] WAF 告警 · 服务器 {server_id}");
    for alert in alerts.iter().take(20) {
        message.push_str(&format!("\n- [{}] {}", alert.severity, alert.summary));
    }
    message
}

/// 为钉钉机器人 URL 添加官方 timestamp/sign 查询参数；其他渠道保持原 URL 不变。
fn dingtalk_signed_url(
    url: &str,
    provider: &str,
    signing_secret: Option<&SecretString>,
) -> AppResult<String> {
    let Some(secret) = signing_secret.filter(|_| provider == "dingtalk") else {
        return Ok(url.to_string());
    };
    let timestamp = chrono::Utc::now().timestamp_millis();
    let string_to_sign = format!("{timestamp}\n{}", secret.expose_secret());
    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret.expose_secret().as_bytes()).map_err(|error| {
            AppError::new("WAF_WEBHOOK_FAILED", "advanced", "无法初始化 webhook 签名")
                .details(error)
        })?;
    mac.update(string_to_sign.as_bytes());
    let sign = BASE64.encode(mac.finalize().into_bytes());
    let mut parsed = reqwest::Url::parse(url).map_err(|error| {
        AppError::new("WAF_WEBHOOK_FAILED", "advanced", "WAF webhook URL 无效").details(error)
    })?;
    parsed
        .query_pairs_mut()
        .append_pair("timestamp", &timestamp.to_string())
        .append_pair("sign", &sign);
    Ok(parsed.to_string())
}

/// 判断告警级别是否达到用户阈值；critical > error > warning。
fn severity_at_least(actual: &str, threshold: &str) -> bool {
    severity_rank(actual) >= severity_rank(threshold)
}

/// 将 WAF 告警级别映射为稳定的比较序号。
fn severity_rank(value: &str) -> u8 {
    match value {
        "critical" => 3,
        "error" => 2,
        "warning" => 1,
        _ => 0,
    }
}

/// 校验服务器 ID 可以安全地组成本地设置键。
fn validate_local_server_id(value: &str) -> AppResult<()> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "advanced",
            "服务器 ID 无效",
        ));
    }
    Ok(())
}

/// Reduces an audit line to a generic denial label plus an optional numeric rule id, avoiding URI/header leakage.
fn sanitize_waf_alert_summary(raw: &str) -> String {
    let normalized = raw.trim().replace(['\t', '\r', '\n'], " ");
    let lower = normalized.to_ascii_lowercase();
    for marker in ["id:", "id=", "id "] {
        if let Some(start) = lower.find(marker) {
            let digits = normalized[start + marker.len()..]
                .chars()
                .skip_while(|character| !character.is_ascii_digit())
                .take_while(|character| character.is_ascii_digit())
                .take(12)
                .collect::<String>();
            if !digits.is_empty() {
                return format!("ModSecurity 拒绝事件（规则 {}）", digits);
            }
        }
    }
    "ModSecurity 拒绝事件".into()
}

/// 校验 WAF 规则动作，阻止多行内容和非 ModSecurity directive 进入 shell。
fn validate_waf_rule_action(input: &WafRuleActionInput) -> AppResult<()> {
    if !matches!(input.action.as_str(), "add" | "delete") {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "advanced",
            "WAF 规则动作无效",
        ));
    }
    if input.action == "add" {
        let value = input.rule.as_deref().unwrap_or_default().trim();
        if value.is_empty()
            || value.len() > 4096
            || !(value.starts_with("SecRule ")
                || value.starts_with("SecAction ")
                || value.starts_with("SecDefaultAction "))
            || value
                .chars()
                .any(|character| character == '\0' || character == '\n' || character == '\r')
        {
            return Err(AppError::new(
                "VALIDATION_FAILED",
                "advanced",
                "WAF 规则必须是单行 SecRule/SecAction directive",
            ));
        }
    } else if input
        .line_number
        .is_none_or(|line| line == 0 || line > 1_000_000)
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "advanced",
            "WAF 规则行号无效",
        ));
    }
    Ok(())
}

/// Validates a built-in template request before any remote probe or configuration write.
fn validate_waf_template_action(input: &WafTemplateActionInput) -> AppResult<()> {
    if input.server_id.trim().is_empty()
        || input.server_id.len() > 128
        || input.template_id.trim().is_empty()
        || input.template_id.len() > 64
        || !input
            .template_id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err(
            AppError::new("VALIDATION_FAILED", "advanced", "WAF 模板标识无效")
                .for_server(&input.server_id),
        );
    }
    Ok(())
}

/// 构造固定 Nginx/OpenResty 控制命令，用于规则变更后的测试和 reload。
fn nginx_control(nginx: &NginxSnapshot) -> String {
    nginx
        .container_id
        .as_deref()
        .map(|id| format!("docker exec {} openresty", shell_escape(id)))
        .unwrap_or_else(|| shell_escape(nginx.binary.as_deref().unwrap_or("nginx")))
}

/// 构造固定 WAF/curl 探测脚本；容器 Web 服务器在容器内读取编译参数。
fn waf_probe_command(nginx: &NginxSnapshot) -> String {
    let version = nginx
        .container_id
        .as_deref()
        .map(|id| format!("docker exec {} openresty -V 2>&1", shell_escape(id)))
        .unwrap_or_else(|| {
            format!(
                "{} -V 2>&1",
                shell_escape(nginx.binary.as_deref().unwrap_or("nginx"))
            )
        });
    format!(
        "set +e; {version}; if command -v curl >/dev/null 2>&1; then printf '__CURL__yes\\n'; else printf '__CURL__no\\n'; fi; if printf '%s' \"$({version})\" | grep -Eqi 'modsecurity|waf'; then printf '__WAF__modsecurity\\n'; fi"
    )
}

/// 解析 WAF 探测 marker，识别 ModSecurity/OpenResty WAF 编译参数。
fn parse_waf_probe(output: &str) -> (bool, Option<String>) {
    let provider = output
        .lines()
        .find_map(|line| line.strip_prefix("__WAF__").map(str::to_string));
    (provider.is_some(), provider)
}

/// 校验探活 URL 和预期状态，阻止换行、shell 分隔符和不受支持的协议。
fn validate_monitor_input(input: &HttpMonitorInput) -> AppResult<()> {
    let value = input.url.trim();
    if value.len() > 2048
        || !(value.starts_with("http://") || value.starts_with("https://"))
        || value.chars().any(|character| {
            character == '\0' || character == '\n' || character == '\r' || character.is_whitespace()
        })
        || input.expected_status.is_some_and(|status| status == 0)
    {
        return Err(
            AppError::new("VALIDATION_FAILED", "advanced", "探活 URL 或预期状态无效")
                .for_server(&input.server_id),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        build_crs_source_action_command, build_crs_source_script, count_waf_severities,
        dingtalk_signed_url, format_waf_notification_message, normalize_waf_alert_settings,
        parse_crs_version, parse_waf_alerts, parse_waf_probe, parse_waf_rules, severity_at_least,
        validate_http_monitor, validate_monitor_input, validate_waf_container_id,
        validate_waf_rule_action, validate_waf_rule_source_action, validate_waf_target,
        validate_waf_template_action, validate_waf_webhook_secret, validate_waf_webhook_url,
        waf_alerts_probe_command, waf_rule_source_definitions, waf_rules_probe_command,
        waf_templates, HttpMonitorInput, SaveHttpMonitorInput, SaveWafAlertSettingsInput,
        WafRuleActionInput, WafRuleSourceActionInput, WafTemplateActionInput,
    };
    use crate::domain::nginx::NginxSnapshot;
    use crate::domain::ssh::{ConnectOutcome, TrustHostKeyInput};
    use crate::infra::db::ServerRepository;
    use crate::security::{CredentialStore, OsCredentialStore};
    use secrecy::SecretString;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use std::sync::Arc;

    #[test]
    fn parses_waf_and_curl_markers() {
        let (enabled, provider) = parse_waf_probe("__CURL__yes\n__WAF__modsecurity\n");
        assert!(enabled);
        assert_eq!(provider.as_deref(), Some("modsecurity"));
    }

    #[test]
    fn rejects_unsafe_monitor_urls() {
        let input = HttpMonitorInput {
            server_id: "server".into(),
            url: "https://example.com/path with space".into(),
            expected_status: None,
        };
        assert!(validate_monitor_input(&input).is_err());
    }

    #[test]
    fn rejects_monitor_intervals_below_scheduler_floor() {
        let input = SaveHttpMonitorInput {
            id: None,
            server_id: "server".into(),
            name: "health".into(),
            url: "https://example.com".into(),
            expected_status: Some(200),
            interval_seconds: 10,
            enabled: true,
        };
        assert!(validate_http_monitor(&input).is_err());
    }

    #[test]
    fn parses_waf_rule_markers_and_rejects_multiline_rules() {
        let (path, rules) =
            parse_waf_rules("__WAF_FILE__\t/etc/modsecurity/modsecurity.conf\n__WAF_RULE__\t/etc/modsecurity/modsecurity.conf\t12\tSecRule ARGS:test \"@contains x\" \"id:100,deny\"\n");
        assert_eq!(
            path.as_ref().map(|value| value.path.as_str()),
            Some("/etc/modsecurity/modsecurity.conf")
        );
        assert_eq!(
            path.as_ref()
                .and_then(|value| value.container_id.as_deref()),
            None
        );
        assert_eq!(rules[0].line_number, 12);
        let input = WafRuleActionInput {
            server_id: "server".into(),
            action: "add".into(),
            line_number: None,
            rule: Some("SecRule ARGS:test \"@rx x\"\nSecAction".into()),
            confirmed: true,
        };
        assert!(validate_waf_rule_action(&input).is_err());
    }

    /// 验证内置策略模板包含固定规则 ID，并拒绝不安全的模板标识。
    #[test]
    fn validates_waf_templates() {
        let templates = waf_templates();
        assert_eq!(templates.len(), 7);
        assert!(templates
            .iter()
            .all(|template| template.rule.contains("id:")));
        // 规则 ID 必须在全部内置模板间唯一，防止多模板同时启用时发生 ID 冲突。
        let ids: std::collections::HashSet<u32> =
            templates.iter().map(|template| template.rule_id).collect();
        assert_eq!(ids.len(), templates.len());
        // 新增的策略模板必须带内联 id，与结构体 rule_id 保持一致。
        for template in &templates {
            let expected = format!("id:{}", template.rule_id);
            assert!(
                template.rule.contains(&expected),
                "template {} rule id mismatch",
                template.id
            );
        }
        // 每条规则必须是单行 SecRule/SecAction directive，且不能包含换行或空字符。
        assert!(templates.iter().all(|template| {
            let value = template.rule.trim();
            (value.starts_with("SecRule ") || value.starts_with("SecAction "))
                && !value.contains('\0')
                && !value.contains('\n')
                && !value.contains('\r')
        }));
        assert!(validate_waf_template_action(&WafTemplateActionInput {
            server_id: "server".into(),
            template_id: "sensitive-files".into(),
            confirmed: true,
        })
        .is_ok());
        assert!(validate_waf_template_action(&WafTemplateActionInput {
            server_id: "server".into(),
            template_id: "sensitive/files".into(),
            confirmed: true,
        })
        .is_err());
    }

    /// 验证内置 CRS 来源带有固定版本、完整 SHA-256 和官方签名指纹。
    #[test]
    fn defines_pinned_crs_sources() {
        let sources = waf_rule_source_definitions();
        assert_eq!(sources.len(), 2);
        assert_eq!(sources[0].version, "4.25.1");
        assert_eq!(sources[1].version, "4.28.0");
        assert!(sources.iter().all(|source| source.sha256.len() == 64));
        assert!(sources
            .iter()
            .all(|source| source.signature_fingerprint == super::CRS_SIGNATURE_FINGERPRINT));
    }

    /// 验证 CRS 版本 marker 只接受有限字符，避免把远端输出当作命令片段。
    #[test]
    fn parses_crs_version_marker() {
        assert_eq!(
            parse_crs_version("__CRS_VERSION__\t4.25.1\n"),
            Some("4.25.1".into())
        );
        assert!(parse_crs_version("__CRS_VERSION__\t4.25.1;rm -rf /\n").is_none());
        assert!(parse_crs_version("unexpected\n").is_none());
    }

    /// 校验 CRS 动作只接受固定来源、合法服务器标识和 install/update/remove。
    #[test]
    fn validates_crs_source_actions() {
        let source_id = waf_rule_source_definitions()[0].id.clone();
        assert!(validate_waf_rule_source_action(&WafRuleSourceActionInput {
            server_id: "server-1".into(),
            source_id,
            action: "install".into(),
            confirmed: true,
        })
        .is_ok());
        assert!(validate_waf_rule_source_action(&WafRuleSourceActionInput {
            server_id: "server-1".into(),
            source_id: "unknown".into(),
            action: "install".into(),
            confirmed: true,
        })
        .is_err());
    }

    /// 验证宿主机 CRS 脚本包含下载校验、marker 管理、配置测试和失败回滚。
    #[test]
    fn builds_host_crs_source_command() {
        let source = waf_rule_source_definitions()[0].clone();
        let nginx = NginxSnapshot {
            installed: true,
            running: true,
            flavor: "nginx".into(),
            binary: Some("nginx".into()),
            container_id: None,
            container_site_root: None,
            site_host_root: None,
            version: None,
            config_path: Some("/etc/nginx/nginx.conf".into()),
            config_test: None,
            managed_conf_supported: false,
            managed_conf_dir: None,
            proxies: Vec::new(),
            certificates: Vec::new(),
            config_sources: Vec::new(),
            servers: 0,
            upstreams: 0,
            warnings: Vec::new(),
        };
        let target = super::WafRuleTarget {
            path: "/etc/nginx/nginx.conf".into(),
            container_id: None,
        };
        let command = build_crs_source_action_command(&target, &nginx, &source, "install")
            .expect("host command");
        assert!(command.contains("sha256sum"));
        assert!(command.contains(&source.url));
        assert!(command.contains(super::CRS_START_MARKER));
        assert!(command.contains("nginx' -t"));
        assert!(command.contains("rollback()"));
        let remove = build_crs_source_script("/etc/nginx/nginx.conf", &source, "remove", "'nginx'");
        assert!(!remove.contains("curl --fail"));
        assert!(remove.contains("CRS 尚未安装"));
    }

    /// 验证容器 CRS 命令通过 docker exec 进入容器，并在容器内调用 openresty。
    #[test]
    fn builds_container_crs_source_command() {
        let source = waf_rule_source_definitions()[1].clone();
        let nginx = NginxSnapshot {
            installed: true,
            running: true,
            flavor: "openresty".into(),
            binary: Some("openresty".into()),
            container_id: Some("0123abcd".into()),
            container_site_root: None,
            site_host_root: None,
            version: None,
            config_path: None,
            config_test: None,
            managed_conf_supported: false,
            managed_conf_dir: None,
            proxies: Vec::new(),
            certificates: Vec::new(),
            config_sources: Vec::new(),
            servers: 0,
            upstreams: 0,
            warnings: Vec::new(),
        };
        let target = super::WafRuleTarget {
            path: "/usr/local/openresty/nginx/conf/modsecurity.conf".into(),
            container_id: Some("0123abcd".into()),
        };
        let command = build_crs_source_action_command(&target, &nginx, &source, "update")
            .expect("container command");
        assert!(command.starts_with("docker exec '0123abcd' sh -c"));
        assert!(command.contains("openresty -t"));
        assert!(command.contains("/etc/1panel-client/waf/owasp-crs"));
    }

    /// 在用户显式提供测试数据库和服务器 ID 时，只读验证真实远端的 CRS 能力摘要。
    #[tokio::test]
    #[ignore = "需要用户已授权的真实测试节点环境变量"]
    async fn real_waf_rule_sources_capability() -> crate::errors::AppResult<()> {
        let db_path = std::env::var("ONEPANEL_CLIENT_DB").map_err(|_| {
            crate::errors::AppError::new("TEST_ENV_MISSING", "advanced", "缺少本机测试数据库路径")
        })?;
        let server_id = std::env::var("ONEPANEL_CLIENT_SERVER_ID").map_err(|_| {
            crate::errors::AppError::new("TEST_ENV_MISSING", "advanced", "缺少测试服务器 ID")
        })?;
        let pool = SqlitePoolOptions::new()
            .max_connections(3)
            .connect_with(SqliteConnectOptions::new().filename(db_path))
            .await
            .map_err(crate::errors::AppError::database)?;
        let credentials: Arc<dyn CredentialStore> =
            Arc::new(OsCredentialStore::new("com.agentless.servermanager"));
        let servers = ServerRepository::new(pool, credentials);
        let ssh = crate::domain::ssh::SshConnectionManager::new(servers);
        if let ConnectOutcome::HostKey(challenge) = ssh.connect(&server_id).await? {
            ssh.trust(TrustHostKeyInput {
                server_id: challenge.server_id,
                host: challenge.host,
                port: challenge.port,
                key_type: challenge.key_type,
                fingerprint: challenge.fingerprint,
            })
            .await?;
        }
        let value = super::waf_rule_sources(&ssh, &server_id).await?;
        assert_eq!(value.sources.len(), 2);
        Ok(())
    }

    /// 验证容器规则 marker 会保留容器 ID 和容器内路径，后续编辑不会误写宿主机路径。
    #[test]
    fn parses_container_waf_rule_target() {
        let (target, rules) = parse_waf_rules(
            "__WAF_FILE__\tcontainer\t0123abcd\t/etc/modsecurity/modsecurity.conf\n__WAF_RULE__\tcontainer\t0123abcd\t/etc/modsecurity/modsecurity.conf\t8\tSecRule ARGS \"@contains x\" \"id:8,deny\"\n",
        );
        assert_eq!(
            target.as_ref().map(|value| value.path.as_str()),
            Some("/etc/modsecurity/modsecurity.conf")
        );
        assert_eq!(
            target
                .as_ref()
                .and_then(|value| value.container_id.as_deref()),
            Some("0123abcd")
        );
        assert_eq!(rules[0].source_path, "/etc/modsecurity/modsecurity.conf");
        assert_eq!(rules[0].line_number, 8);
    }

    /// 校验规则动作只接受探测得到的十六进制 Docker 容器 ID。
    #[test]
    fn validates_waf_container_ids() {
        assert!(validate_waf_container_id("0123abcd").is_ok());
        assert!(validate_waf_container_id("container;rm -rf /").is_err());
    }

    /// 校验规则目标路径不能越过固定候选目录边界。
    #[test]
    fn rejects_unsafe_waf_rule_targets() {
        assert!(validate_waf_target(&super::WafRuleTarget {
            path: "/etc/modsecurity/modsecurity.conf".into(),
            container_id: Some("0123abcd".into()),
        })
        .is_ok());
        assert!(validate_waf_target(&super::WafRuleTarget {
            path: "/tmp/../etc/passwd".into(),
            container_id: None,
        })
        .is_err());
    }

    /// 验证容器规则探测只读取固定候选路径，并输出可供解析的容器目标 marker。
    #[test]
    fn builds_container_waf_rule_probe() {
        let snapshot = NginxSnapshot {
            installed: true,
            running: true,
            flavor: "openresty".into(),
            binary: Some("openresty".into()),
            container_id: Some("0123abcd".into()),
            container_site_root: None,
            site_host_root: None,
            version: None,
            config_path: None,
            config_test: None,
            managed_conf_supported: false,
            managed_conf_dir: None,
            proxies: Vec::new(),
            certificates: Vec::new(),
            config_sources: Vec::new(),
            servers: 0,
            upstreams: 0,
            warnings: Vec::new(),
        };
        let command = waf_rules_probe_command(&snapshot);
        assert!(command.contains("docker exec '0123abcd' test -f"));
        assert!(command.contains("__WAF_FILE__\\tcontainer"));
        assert!(command.contains("/usr/local/openresty/nginx/conf/modsecurity.conf"));
    }

    #[test]
    fn parses_waf_alert_markers_and_deduplicates_lines() {
        let (sources, alerts) = parse_waf_alerts(
            "__WAF_LOG__\t/var/log/modsec_audit.log\n__WAF_ALERT__\tAccess denied id=10001\n__WAF_ALERT__\tAccess denied id=10001\n",
        );
        assert_eq!(sources, vec!["/var/log/modsec_audit.log"]);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].source_path, "/var/log/modsec_audit.log");
        assert_eq!(alerts[0].summary, "ModSecurity 拒绝事件（规则 10001）");
        assert_eq!(alerts[0].severity, "error");
    }

    /// 验证告警级别阈值只保留 warning 及以上，并接受 critical 事件。
    #[test]
    fn classifies_waf_severity_and_thresholds() {
        let (_, alerts) = parse_waf_alerts(
            "__WAF_LOG__\t/var/log/modsec_audit.log\n__WAF_ALERT__\tcritical ModSecurity event\n__WAF_ALERT__\tAccess denied id=10002\n",
        );
        assert_eq!(alerts.len(), 2);
        assert!(alerts.iter().any(|alert| alert.severity == "critical"));
        assert!(alerts
            .iter()
            .all(|alert| severity_at_least(&alert.severity, "warning")));
        assert!(!severity_at_least("warning", "error"));
    }

    /// 验证趋势聚合按 warning/error/critical 分桶并保持总数一致。
    #[test]
    fn counts_waf_trend_severities() {
        let (_, alerts) = parse_waf_alerts(
            "__WAF_LOG__\t/var/log/modsec.log\n__WAF_ALERT__\twarning event\n__WAF_ALERT__\tAccess denied id=12\n__WAF_ALERT__\tcritical event\n",
        );
        assert_eq!(count_waf_severities(&alerts), (1, 1, 1));
    }

    /// 验证主机日志和匹配容器日志都能形成稳定的 WAF 来源。
    #[test]
    fn parses_waf_container_log_markers() {
        let (sources, alerts) = parse_waf_alerts(
            "__WAF_CONTAINER__\tdocker:openresty\n__WAF_ALERT__\tAccess denied id=10003\n",
        );
        assert_eq!(sources, vec!["docker:openresty"]);
        assert_eq!(alerts[0].source_path, "docker:openresty");
    }

    /// 验证 WAF 探测脚本包含有界 Docker 日志同步，且 format 占位符已在 Rust 侧展开。
    #[test]
    fn builds_bounded_waf_container_probe() {
        let command = waf_alerts_probe_command();
        assert!(command.contains("docker ps --format '{{.ID}}\\t{{.Names}}'"));
        assert!(command.contains("docker logs --tail 250"));
        assert!(!command.contains("{path_list}"));
    }

    /// 校验 webhook 只接受有主机的 HTTP(S) URL，并拒绝控制字符和伪造协议。
    #[test]
    fn validates_waf_webhook_url() {
        assert!(validate_waf_webhook_url("https://hooks.example.com/waf").is_ok());
        assert!(validate_waf_webhook_url("file:///etc/passwd").is_err());
        assert!(validate_waf_webhook_url("https://hooks.example.com/waf\n").is_err());
    }

    /// 验证钉钉签名 URL 会附加 timestamp/sign，而未配置签名时保持原 URL。
    #[test]
    fn builds_dingtalk_signed_webhook_url() {
        let secret = SecretString::from("secret".to_string());
        let signed = dingtalk_signed_url(
            "https://oapi.dingtalk.com/robot/send?access_token=token",
            "dingtalk",
            Some(&secret),
        )
        .unwrap();
        let parsed = reqwest::Url::parse(&signed).unwrap();
        assert!(parsed.query_pairs().any(|(key, _)| key == "timestamp"));
        assert!(parsed
            .query_pairs()
            .any(|(key, value)| key == "sign" && !value.is_empty()));
        assert_eq!(
            dingtalk_signed_url("https://hooks.example.com/waf", "slack", Some(&secret)).unwrap(),
            "https://hooks.example.com/waf"
        );
    }

    /// 校验钉钉签名密钥不接受空值、空白和控制字符。
    #[test]
    fn validates_waf_webhook_secret() {
        assert!(validate_waf_webhook_secret("secret").is_ok());
        assert!(validate_waf_webhook_secret("secret with space").is_err());
        assert!(validate_waf_webhook_secret("").is_err());
    }

    /// 验证 WAF 设置限制历史容量并拒绝未知级别，避免本地策略产生歧义。
    #[test]
    fn validates_waf_alert_settings() {
        let settings = normalize_waf_alert_settings(SaveWafAlertSettingsInput {
            min_severity: "critical".into(),
            notify_in_app: false,
            history_limit: 100,
            notify_webhook: true,
            notify_provider: "slack".into(),
            webhook_url: None,
            webhook_signing_secret: None,
            clear_webhook: false,
        })
        .unwrap();
        assert_eq!(settings.min_severity, "critical");
        assert_eq!(settings.notify_provider, "slack");
        assert!(normalize_waf_alert_settings(SaveWafAlertSettingsInput {
            min_severity: "info".into(),
            notify_in_app: true,
            history_limit: 100,
            notify_webhook: false,
            notify_provider: "generic".into(),
            webhook_url: None,
            webhook_signing_secret: None,
            clear_webhook: false,
        })
        .is_err());
        assert!(normalize_waf_alert_settings(SaveWafAlertSettingsInput {
            min_severity: "warning".into(),
            notify_in_app: true,
            history_limit: 10,
            notify_webhook: false,
            notify_provider: "generic".into(),
            webhook_url: None,
            webhook_signing_secret: None,
            clear_webhook: false,
        })
        .is_err());
        assert!(normalize_waf_alert_settings(SaveWafAlertSettingsInput {
            min_severity: "warning".into(),
            notify_in_app: true,
            history_limit: 100,
            notify_webhook: false,
            notify_provider: "unknown".into(),
            webhook_url: None,
            webhook_signing_secret: None,
            clear_webhook: false,
        })
        .is_err());
    }

    /// 验证外部通知文本只使用脱敏摘要，避免把原始日志内容发送给第三方渠道。
    #[test]
    fn formats_redacted_waf_notification_message() {
        let alerts = vec![super::WafAlert {
            source_path: "/var/log/modsec.log".into(),
            summary: "ModSecurity 拒绝事件（规则 10001）".into(),
            severity: "error".into(),
            fingerprint: "deadbeef".into(),
        }];
        let message = format_waf_notification_message("server", &alerts);
        assert!(message.contains("规则 10001"));
        assert!(!message.contains("deadbeef"));
        assert!(!message.contains("/var/log"));
    }
}

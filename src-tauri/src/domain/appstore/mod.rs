use crate::domain::ssh::SshConnectionManager;
use crate::errors::{AppError, AppResult};
use crate::infra::local::LocalRepository;
use crate::security::{shell_escape, CredentialStore};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use chrono::{DateTime, Utc};
use hmac::{Hmac, KeyInit, Mac};
use reqwest::Client;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncWriteExt;

mod catalog_meta;

const REPOSITORY: &str = "1Panel-dev/appstore";
const BRANCH: &str = "dev";
const API_BASE: &str = "https://api.github.com";
const RAW_BASE: &str = "https://raw.githubusercontent.com/1Panel-dev/appstore/dev";

/// 描述官方 1Panel 应用商店中的一个应用入口。
const SETTINGS_KEY: &str = "appstore.settings";
const CATALOG_CACHE_KEY: &str = "appstore.catalog.cache";
const DETAIL_CACHE_PREFIX: &str = "appstore.detail.";
const MIRROR_VERIFY_KEY_PREFIX: &str = "appstore-mirror-verify-";
const MAX_MIRROR_BASES: usize = 8;
const MAX_MIRROR_APPS: usize = 512;
const MAX_MIRROR_VERSIONS: usize = 4096;
const MAX_MIRROR_FILES: usize = 12_000;

/// Returns the conservative default number of applications included in a generated mirror.
fn default_mirror_max_apps() -> usize {
    MAX_MIRROR_APPS
}

/// 描述应用商店网络来源和本地缓存策略；不包含任何远端服务器凭据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppStoreSettings {
    pub source: String,
    pub mirror_base_url: Option<String>,
    #[serde(default)]
    pub mirror_base_urls: Vec<String>,
    pub cache_ttl_seconds: u64,
    pub offline_mode: bool,
    #[serde(default)]
    pub mirror_key_id: Option<String>,
    #[serde(default)]
    pub signature_configured: bool,
}

/// 保存应用商店来源设置；镜像使用约定的静态目录格式。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveAppStoreSettingsInput {
    pub source: String,
    pub mirror_base_url: Option<String>,
    #[serde(default)]
    pub mirror_base_urls: Vec<String>,
    pub cache_ttl_seconds: u64,
    pub offline_mode: bool,
    #[serde(default)]
    pub mirror_key_id: Option<String>,
    #[serde(default)]
    pub mirror_verification_secret: Option<SecretString>,
    #[serde(default)]
    pub clear_mirror_verification_secret: bool,
}

/// Carries a local destination and one-time signing secret for mirror generation.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateAppStoreMirrorInput {
    pub destination: String,
    pub key_id: String,
    pub signing_secret: SecretString,
    #[serde(default = "default_mirror_max_apps")]
    pub max_apps: usize,
    #[serde(default)]
    pub confirmed: bool,
}

/// Returns the result of writing a complete static mirror directory.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppStoreMirrorGenerationResult {
    pub destination: String,
    pub source_revision: String,
    pub app_count: usize,
    pub version_count: usize,
    pub file_count: usize,
    pub catalog_sha256: String,
    pub signature_path: String,
}

/// Describes the detached HMAC signature stored beside a generated catalog.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MirrorSignature {
    algorithm: String,
    key_id: String,
    catalog_sha256: String,
    signature: String,
}

/// 清理本地应用商店缓存后的结果摘要。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppStoreCacheClearResult {
    pub cleared: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedRecord<T> {
    cached_at: DateTime<Utc>,
    value: T,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MirrorVersion {
    version: String,
    #[serde(default)]
    compose_url: Option<String>,
    #[serde(default)]
    env_url: Option<String>,
}

/// 静态镜像的 catalog.json 文档格式；items 与官方目录保持同构。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MirrorCatalog {
    #[serde(default)]
    repository: Option<String>,
    #[serde(default)]
    branch: Option<String>,
    source_revision: String,
    items: Vec<AppCatalogItem>,
}

/// 描述官方 1Panel 应用商店中的一个应用入口。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppCatalogItem {
    pub key: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub metadata_url: String,
}

/// 返回应用商店目录及其上游版本信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppCatalogSnapshot {
    pub repository: String,
    pub branch: String,
    pub source_revision: String,
    pub items: Vec<AppCatalogItem>,
    pub fetched_at: DateTime<Utc>,
    pub cached: bool,
    pub cache_age_seconds: Option<u64>,
    #[serde(default)]
    pub signature_present: bool,
    #[serde(default)]
    pub signature_verified: bool,
    #[serde(default)]
    pub resolved_mirror_base_url: Option<String>,
}

/// 描述一个应用版本以及可直接下载的 Compose 文件。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppVersion {
    pub version: String,
    pub compose_url: String,
    pub env_url: String,
}

/// 返回应用详情、标签、链接和可安装版本。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppDetail {
    pub key: String,
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
    pub website: Option<String>,
    pub github: Option<String>,
    pub versions: Vec<AppVersion>,
    pub fetched_at: DateTime<Utc>,
    pub cached: bool,
    pub cache_age_seconds: Option<u64>,
    #[serde(default)]
    pub resolved_mirror_base_url: Option<String>,
}

/// 描述远端已安装的 Compose 应用目录。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledApp {
    pub key: String,
    pub path: String,
    pub compose_path: String,
    pub project: String,
    pub status: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub host_ports: Vec<String>,
    #[serde(default)]
    pub installed_seconds: Option<u64>,
}

/// 返回远端已安装应用和 Docker Compose 能力状态。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledAppsSnapshot {
    pub compose_available: bool,
    pub apps: Vec<InstalledApp>,
    pub fetched_at: DateTime<Utc>,
}

/// 描述 Compose 应用中单个服务的真实容器健康状态。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppServiceHealth {
    pub name: String,
    pub image: String,
    pub state: String,
    pub health: String,
    pub exit_code: i32,
}

/// 返回一个 Compose 应用的健康摘要，不读取容器环境变量或日志正文。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppHealthSnapshot {
    pub project: String,
    pub path: String,
    pub overall: String,
    pub services: Vec<AppServiceHealth>,
    pub fetched_at: DateTime<Utc>,
}

/// 应用健康检查请求；路径必须位于 1Panel 固定应用根目录。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppHealthInput {
    pub server_id: String,
    pub project: String,
    pub install_path: String,
}

/// 返回已安装应用的环境变量键摘要；值只以掩码形式出现，不回传秘密。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppEnvironmentEntry {
    pub key: String,
    pub configured: bool,
    pub masked_value: String,
}

/// 返回某个 Compose 应用的环境变量文件摘要。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppEnvironmentSnapshot {
    pub path: String,
    pub entries: Vec<AppEnvironmentEntry>,
}

/// 应用安装请求；Compose 内容来自官方目录，环境变量由用户明确提供。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallAppInput {
    pub server_id: String,
    pub key: String,
    pub version: String,
    pub project: String,
    pub install_path: String,
    #[serde(default)]
    pub environment: Vec<String>,
    pub confirmed: bool,
}

/// 应用 Compose 生命周期操作请求。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppActionInput {
    pub server_id: String,
    pub key: String,
    pub project: String,
    pub install_path: String,
    pub action: String,
    pub confirmed: bool,
}

/// 已安装应用升级预览请求；只读取远端 Compose 文件摘要，不执行更新。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdatePreviewInput {
    pub server_id: String,
    pub key: String,
    pub project: String,
    pub install_path: String,
}

/// 返回官方最新模板与远端当前模板的摘要差异，不包含 Compose 正文。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdatePreview {
    pub key: String,
    pub project: String,
    pub latest_version: String,
    pub current_hash: Option<String>,
    pub latest_hash: String,
    pub current_lines: u64,
    pub latest_lines: u64,
    pub changed: bool,
    pub current_missing: bool,
    pub fetched_at: DateTime<Utc>,
}

/// 合并写入应用环境变量；未提交的键会保留远端原值。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppEnvironmentInput {
    pub server_id: String,
    pub install_path: String,
    pub values: Vec<String>,
    pub confirmed: bool,
}

/// 返回应用商店安装或生命周期操作结果。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppActionResult {
    pub key: String,
    pub project: String,
    pub action: String,
    pub output: String,
}

#[derive(Debug, Deserialize)]
struct GithubEntry {
    name: String,
    #[serde(rename = "type")]
    entry_type: String,
}

#[derive(Debug, Deserialize)]
struct GithubBranch {
    commit: GithubCommit,
}

#[derive(Debug, Deserialize)]
struct GithubCommit {
    sha: String,
}

/// 返回默认应用商店来源设置。
pub fn default_settings() -> AppStoreSettings {
    AppStoreSettings {
        source: "official".into(),
        mirror_base_url: None,
        mirror_base_urls: Vec::new(),
        cache_ttl_seconds: 3600,
        offline_mode: false,
        mirror_key_id: None,
        signature_configured: false,
    }
}

/// 读取并校验应用商店来源设置；缺失设置时使用官方源默认值。
pub async fn settings(
    local: &LocalRepository,
    credentials: &Arc<dyn CredentialStore>,
) -> AppResult<AppStoreSettings> {
    let value = local.get_setting(SETTINGS_KEY).await?;
    let input = value
        .map(|raw| serde_json::from_str::<SaveAppStoreSettingsInput>(&raw))
        .transpose()
        .map_err(|error| {
            AppError::new(
                "APPSTORE_SETTINGS_INVALID",
                "appstore",
                "应用商店设置无法解析",
            )
            .details(error)
        })?;
    let key_id = input.as_ref().and_then(|value| value.mirror_key_id.clone());
    let mut settings = input
        .map(normalize_settings)
        .transpose()?
        .unwrap_or_else(default_settings);
    settings.mirror_key_id = key_id;
    settings.signature_configured = settings
        .mirror_key_id
        .as_deref()
        .map(|value| credentials.get(&mirror_verify_key(value)).is_ok())
        .unwrap_or(false);
    Ok(settings)
}

/// 保存并校验应用商店来源设置；镜像只接受无凭据的 HTTP(S) 基础地址。
pub async fn save_settings(
    local: &LocalRepository,
    credentials: &Arc<dyn CredentialStore>,
    input: SaveAppStoreSettingsInput,
) -> AppResult<AppStoreSettings> {
    let previous = settings(local, credentials).await?;
    let key_id = input
        .mirror_key_id
        .as_deref()
        .map(validate_mirror_key_id)
        .transpose()?;
    if let Some(secret) = input.mirror_verification_secret.as_ref() {
        let key_id = key_id.as_deref().ok_or_else(|| {
            AppError::new(
                "VALIDATION_FAILED",
                "appstore",
                "设置镜像验签令牌前必须填写 key ID",
            )
        })?;
        validate_mirror_secret(secret)?;
        credentials.put(
            &mirror_verify_key(key_id),
            SecretString::from(secret.expose_secret().trim().to_owned()),
        )?;
    } else if input.clear_mirror_verification_secret {
        if let Some(key_id) = key_id.as_deref().or(previous.mirror_key_id.as_deref()) {
            credentials.delete(&mirror_verify_key(key_id))?;
        }
    }
    if previous.mirror_key_id != key_id {
        if let Some(previous_key_id) = previous.mirror_key_id.as_deref() {
            credentials.delete(&mirror_verify_key(previous_key_id))?;
        }
    }
    let mut settings = normalize_settings(input)?;
    settings.mirror_key_id = key_id;
    settings.signature_configured = settings
        .mirror_key_id
        .as_deref()
        .map(|value| credentials.get(&mirror_verify_key(value)).is_ok())
        .unwrap_or(false);
    let serialized = serde_json::to_string(&settings).map_err(AppError::database)?;
    local.set_setting(SETTINGS_KEY, &serialized).await?;
    if previous.source != settings.source
        || previous.mirror_base_url != settings.mirror_base_url
        || previous.mirror_base_urls != settings.mirror_base_urls
        || previous.mirror_key_id != settings.mirror_key_id
    {
        clear_cache(local).await?;
    }
    Ok(settings)
}

/// 清理应用商店目录和所有已读取详情的本地缓存。
pub async fn clear_cache(local: &LocalRepository) -> AppResult<AppStoreCacheClearResult> {
    local.delete_setting(CATALOG_CACHE_KEY).await?;
    local.delete_settings_prefix(DETAIL_CACHE_PREFIX).await?;
    Ok(AppStoreCacheClearResult { cleared: true })
}

/// 从所选来源读取应用目录，并在网络不可用时回退到带年龄标记的缓存。
pub async fn catalog(
    local: &LocalRepository,
    credentials: &Arc<dyn CredentialStore>,
) -> AppResult<AppCatalogSnapshot> {
    let settings = settings(local, credentials).await?;
    let cached = read_cache::<AppCatalogSnapshot>(local, CATALOG_CACHE_KEY).await?;
    if settings.offline_mode {
        return cached.map(cached_catalog).ok_or_else(|| {
            AppError::new(
                "APPSTORE_CACHE_MISS",
                "appstore",
                "离线模式下没有应用商店目录缓存",
            )
        });
    }
    if let Some(record) = cached
        .as_ref()
        .filter(|record| cache_is_fresh(record.cached_at, settings.cache_ttl_seconds))
    {
        return Ok(cached_catalog(record.clone()));
    }
    match fetch_catalog(&settings, credentials).await {
        Ok(snapshot) => {
            write_cache(local, CATALOG_CACHE_KEY, &snapshot).await?;
            Ok(snapshot)
        }
        Err(error) => cached.map(cached_catalog).ok_or(error),
    }
}

/// 读取单个应用详情，并复用与目录相同的来源、TTL 和离线回退策略。
pub async fn detail(
    local: &LocalRepository,
    credentials: &Arc<dyn CredentialStore>,
    key: &str,
) -> AppResult<AppDetail> {
    validate_key(key)?;
    let settings = settings(local, credentials).await?;
    let cache_key = detail_cache_key(key);
    let cached = read_cache::<AppDetail>(local, &cache_key).await?;
    if settings.offline_mode {
        return cached.map(cached_detail).ok_or_else(|| {
            AppError::new(
                "APPSTORE_CACHE_MISS",
                "appstore",
                "离线模式下没有该应用详情缓存",
            )
        });
    }
    if let Some(record) = cached
        .as_ref()
        .filter(|record| cache_is_fresh(record.cached_at, settings.cache_ttl_seconds))
    {
        return Ok(cached_detail(record.clone()));
    }
    match fetch_detail(&settings, key).await {
        Ok(value) => {
            write_cache(local, &cache_key, &value).await?;
            Ok(value)
        }
        Err(error) => cached.map(cached_detail).ok_or(error),
    }
}

/// Generates a complete static mirror directory from the currently selected catalog source.
pub async fn generate_mirror(
    local: &LocalRepository,
    credentials: &Arc<dyn CredentialStore>,
    input: GenerateAppStoreMirrorInput,
) -> AppResult<AppStoreMirrorGenerationResult> {
    if !input.confirmed {
        return Err(AppError::new(
            "CONFIRMATION_REQUIRED",
            "appstore",
            "生成镜像目录前需要确认本地写入",
        ));
    }
    let destination = validate_mirror_destination(&input.destination)?;
    let key_id = validate_mirror_key_id(&input.key_id)?;
    validate_mirror_secret(&input.signing_secret)?;
    let max_apps = input.max_apps.clamp(1, MAX_MIRROR_APPS);
    let settings = settings(local, credentials).await?;
    let snapshot = catalog(local, credentials).await?;
    if snapshot.items.len() > max_apps {
        return Err(AppError::new(
            "APPSTORE_MIRROR_LIMIT",
            "appstore",
            format!(
                "目录包含 {} 个应用，超过本次生成上限 {max_apps}",
                snapshot.items.len()
            ),
        ));
    }
    tokio::fs::create_dir_all(&destination)
        .await
        .map_err(|error| {
            AppError::new(
                "APPSTORE_MIRROR_WRITE_FAILED",
                "appstore",
                "无法创建镜像目录",
            )
            .details(error)
        })?;
    let client = http_client()?;
    let mut items = Vec::with_capacity(snapshot.items.len());
    let mut version_count = 0_usize;
    let mut file_count = 0_usize;
    for item in &snapshot.items {
        validate_key(&item.key)?;
        let key = item.key.clone();
        let app_dir = destination.join("apps").join(&key);
        let metadata_url = source_asset_url_for_base(
            &settings,
            snapshot.resolved_mirror_base_url.as_deref(),
            &item.metadata_url,
        )?;
        let metadata = fetch_mirror_asset(&client, &metadata_url, true)
            .await?
            .ok_or_else(|| {
                AppError::new(
                    "APPSTORE_MIRROR_ASSET_MISSING",
                    "appstore",
                    "应用 metadata 缺失",
                )
            })?;
        write_mirror_file(&app_dir.join("data.yml"), metadata.as_bytes()).await?;
        file_count = file_count.saturating_add(1);
        let detail = detail(local, credentials, &key).await?;
        let detail_base = detail.resolved_mirror_base_url.clone();
        let mut versions = Vec::with_capacity(detail.versions.len());
        for version in detail.versions {
            if !valid_version(&version.version) {
                continue;
            }
            version_count = version_count.saturating_add(1);
            if version_count > MAX_MIRROR_VERSIONS
                || file_count.saturating_add(3) > MAX_MIRROR_FILES
            {
                return Err(AppError::new(
                    "APPSTORE_MIRROR_LIMIT",
                    "appstore",
                    "镜像版本或文件数量超过安全上限",
                ));
            }
            let version_name = version.version.clone();
            let version_path = app_dir.join(&version_name);
            let compose_url =
                source_asset_url_for_base(&settings, detail_base.as_deref(), &version.compose_url)?;
            let compose = fetch_mirror_asset(&client, &compose_url, true)
                .await?
                .ok_or_else(|| {
                    AppError::new(
                        "APPSTORE_MIRROR_ASSET_MISSING",
                        "appstore",
                        "应用 Compose 文件缺失",
                    )
                })?;
            write_mirror_file(&version_path.join("docker-compose.yml"), compose.as_bytes()).await?;
            let env_url =
                source_asset_url_for_base(&settings, detail_base.as_deref(), &version.env_url)?;
            let env = fetch_mirror_asset(&client, &env_url, false)
                .await?
                .unwrap_or_default();
            write_mirror_file(&version_path.join(".env"), env.as_bytes()).await?;
            file_count = file_count.saturating_add(2);
            versions.push(MirrorVersion {
                version: version_name.clone(),
                compose_url: Some(format!("apps/{key}/{version_name}/docker-compose.yml")),
                env_url: Some(format!("apps/{key}/{version_name}/.env")),
            });
        }
        let versions_path = app_dir.join("versions.json");
        let versions_payload = serde_json::to_vec_pretty(&versions).map_err(AppError::database)?;
        write_mirror_file(&versions_path, &versions_payload).await?;
        file_count = file_count.saturating_add(1);
        let mut mirror_item = item.clone();
        mirror_item.metadata_url = format!("apps/{key}/data.yml");
        items.push(mirror_item);
    }
    let mirror_catalog = MirrorCatalog {
        repository: Some(snapshot.repository),
        branch: Some(snapshot.branch),
        source_revision: snapshot.source_revision.clone(),
        items,
    };
    let catalog_payload = serde_json::to_vec_pretty(&mirror_catalog).map_err(AppError::database)?;
    write_mirror_file(&destination.join("catalog.json"), &catalog_payload).await?;
    let signature = sign_mirror_catalog(&catalog_payload, &key_id, &input.signing_secret)?;
    let signature_payload = serde_json::to_vec_pretty(&signature).map_err(AppError::database)?;
    write_mirror_file(&destination.join("catalog.sig"), &signature_payload).await?;
    file_count = file_count.saturating_add(2);
    Ok(AppStoreMirrorGenerationResult {
        destination: destination.display().to_string(),
        source_revision: snapshot.source_revision,
        app_count: snapshot.items.len(),
        version_count,
        file_count,
        catalog_sha256: signature.catalog_sha256,
        signature_path: destination.join("catalog.sig").display().to_string(),
    })
}

/// Validates a local mirror destination without deleting or traversing existing files.
fn validate_mirror_destination(value: &str) -> AppResult<PathBuf> {
    let value = value.trim();
    let path = PathBuf::from(value);
    if value.is_empty()
        || value.len() > 1024
        || value.chars().any(char::is_control)
        || !path.is_absolute()
        || path.parent().is_none()
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "appstore",
            "镜像输出目录必须是绝对路径",
        ));
    }
    Ok(path)
}

/// Resolves a source asset against a selected mirror node, including a fallback for legacy settings.
fn source_asset_url_for_base(
    settings: &AppStoreSettings,
    selected_base: Option<&str>,
    value: &str,
) -> AppResult<String> {
    if settings.source == "mirror" {
        let base = selected_base
            .or(settings.mirror_base_url.as_deref())
            .ok_or_else(|| {
                AppError::new(
                    "APPSTORE_SETTINGS_INVALID",
                    "appstore",
                    "镜像源缺少基础地址",
                )
            })?;
        let base_url =
            reqwest::Url::parse(&format!("{}/", base.trim_end_matches('/'))).map_err(|_| {
                AppError::new(
                    "APPSTORE_MIRROR_ASSET_INVALID",
                    "appstore",
                    "镜像基础地址无效",
                )
            })?;
        let url = reqwest::Url::parse(value)
            .or_else(|_| base_url.join(value))
            .map_err(|_| {
                AppError::new(
                    "APPSTORE_MIRROR_ASSET_INVALID",
                    "appstore",
                    "镜像资源地址无效",
                )
            })?;
        return validate_mirror_file_url(base, url.as_str());
    }
    let url = reqwest::Url::parse(value).map_err(|_| {
        AppError::new(
            "APPSTORE_MIRROR_ASSET_INVALID",
            "appstore",
            "官方资源地址无效",
        )
    })?;
    if url.scheme() != "https"
        || url.username() != ""
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(AppError::new(
            "APPSTORE_MIRROR_ASSET_INVALID",
            "appstore",
            "官方资源地址不安全",
        ));
    }
    Ok(url.to_string())
}

/// Fetches a text mirror asset with a bounded body; missing optional env files are allowed.
async fn fetch_mirror_asset(
    client: &Client,
    url: &str,
    required: bool,
) -> AppResult<Option<String>> {
    let response = client
        .get(url)
        .header("User-Agent", "1panel-client")
        .send()
        .await
        .map_err(|error| network_error("读取镜像资源失败", error))?;
    if response.status() == reqwest::StatusCode::NOT_FOUND && !required {
        return Ok(None);
    }
    let response = response
        .error_for_status()
        .map_err(|error| network_error("镜像资源响应失败", error))?;
    let bytes = response
        .bytes()
        .await
        .map_err(|error| network_error("读取镜像资源内容失败", error))?;
    if bytes.len() > 8 * 1024 * 1024 {
        return Err(AppError::new(
            "APPSTORE_MIRROR_ASSET_TOO_LARGE",
            "appstore",
            "镜像资源超过 8 MiB 限制",
        ));
    }
    String::from_utf8(bytes.to_vec())
        .map(Some)
        .map_err(|error| {
            AppError::new(
                "APPSTORE_MIRROR_ASSET_INVALID",
                "appstore",
                "镜像资源不是 UTF-8",
            )
            .details(error)
        })
}

/// Writes one generated mirror asset after creating only its destination parents.
async fn write_mirror_file(path: &Path, payload: &[u8]) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(|error| {
            AppError::new(
                "APPSTORE_MIRROR_WRITE_FAILED",
                "appstore",
                "无法创建镜像资源目录",
            )
            .details(error)
        })?;
    }
    tokio::fs::write(path, payload).await.map_err(|error| {
        AppError::new(
            "APPSTORE_MIRROR_WRITE_FAILED",
            "appstore",
            "无法写入镜像资源",
        )
        .details(error)
    })
}

/// 从官方 GitHub 仓库读取应用目录，不在客户端维护静态 Mock 清单。
async fn fetch_catalog(
    settings: &AppStoreSettings,
    credentials: &Arc<dyn CredentialStore>,
) -> AppResult<AppCatalogSnapshot> {
    if settings.source == "mirror" {
        return fetch_mirror_catalog(settings, credentials).await;
    }
    let client = http_client()?;
    let revision = client
        .get(format!("{API_BASE}/repos/{REPOSITORY}/branches/{BRANCH}"))
        .header("User-Agent", "1panel-client")
        .send()
        .await
        .map_err(|error| network_error("读取应用商店版本失败", error))?
        .error_for_status()
        .map_err(|error| network_error("应用商店版本响应失败", error))?
        .json::<GithubBranch>()
        .await
        .map_err(|error| network_error("解析应用商店版本失败", error))?
        .commit
        .sha;
    let entries = client
        .get(format!(
            "{API_BASE}/repos/{REPOSITORY}/contents/apps?ref={BRANCH}"
        ))
        .header("User-Agent", "1panel-client")
        .send()
        .await
        .map_err(|error| network_error("读取应用商店目录失败", error))?
        .error_for_status()
        .map_err(|error| network_error("应用商店目录响应失败", error))?
        .json::<Vec<GithubEntry>>()
        .await
        .map_err(|error| network_error("解析应用商店目录失败", error))?;
    let mut items = entries
        .into_iter()
        .filter(|entry| entry.entry_type == "dir" && valid_key(&entry.name))
        .map(|entry| {
            let meta = catalog_meta::for_key(&entry.name);
            AppCatalogItem {
                key: entry.name.clone(),
                name: meta
                    .map(|value| value.name.into())
                    .unwrap_or_else(|| display_name(&entry.name)),
                description: meta
                    .map(|value| value.description.into())
                    .unwrap_or_else(|| description_for(&entry.name).into()),
                category: meta
                    .map(|value| value.category.into())
                    .unwrap_or_else(|| category_for(&entry.name).into()),
                metadata_url: format!("{RAW_BASE}/apps/{}/data.yml", entry.name),
            }
        })
        .collect::<Vec<_>>();
    items.sort_by_key(|item| item.name.to_lowercase());
    Ok(AppCatalogSnapshot {
        repository: REPOSITORY.into(),
        branch: BRANCH.into(),
        source_revision: revision,
        items,
        fetched_at: Utc::now(),
        cached: false,
        cache_age_seconds: None,
        signature_present: false,
        signature_verified: false,
        resolved_mirror_base_url: None,
    })
}

/// 读取单个应用的官方 metadata 和版本目录，供安装表单使用。
async fn fetch_detail(settings: &AppStoreSettings, key: &str) -> AppResult<AppDetail> {
    validate_key(key)?;
    if settings.source == "mirror" {
        return fetch_mirror_detail(settings, key).await;
    }
    let client = http_client()?;
    let metadata = client
        .get(format!("{RAW_BASE}/apps/{key}/data.yml"))
        .header("User-Agent", "1panel-client")
        .send()
        .await
        .map_err(|error| network_error("读取应用详情失败", error))?
        .error_for_status()
        .map_err(|error| network_error("应用详情响应失败", error))?
        .text()
        .await
        .map_err(|error| network_error("读取应用 metadata 失败", error))?;
    let value = serde_yaml::from_str::<serde_yaml::Value>(&metadata).map_err(|error| {
        AppError::new(
            "APPSTORE_PARSE_FAILED",
            "appstore",
            "应用 metadata 无法解析",
        )
        .details(error)
    })?;
    let entries = client
        .get(format!(
            "{API_BASE}/repos/{REPOSITORY}/contents/apps/{key}?ref={BRANCH}"
        ))
        .header("User-Agent", "1panel-client")
        .send()
        .await
        .map_err(|error| network_error("读取应用版本失败", error))?
        .error_for_status()
        .map_err(|error| network_error("应用版本响应失败", error))?
        .json::<Vec<GithubEntry>>()
        .await
        .map_err(|error| network_error("解析应用版本失败", error))?;
    let mut versions = entries
        .into_iter()
        .filter(|entry| entry.entry_type == "dir" && valid_version(&entry.name))
        .map(|entry| AppVersion {
            version: entry.name.clone(),
            compose_url: format!("{RAW_BASE}/apps/{key}/{}/docker-compose.yml", entry.name),
            env_url: format!("{RAW_BASE}/apps/{key}/{}/.env", entry.name),
        })
        .collect::<Vec<_>>();
    versions.sort_by(|left, right| right.version.cmp(&left.version));
    Ok(AppDetail {
        key: key.into(),
        name: yaml_string(&value, "name").unwrap_or_else(|| display_name(key)),
        description: yaml_string(&value, "description")
            .unwrap_or_else(|| "官方 1Panel 应用模板".into()),
        tags: yaml_strings(&value, "tags"),
        website: yaml_nested_string(&value, &["additionalProperties", "website"]),
        github: yaml_nested_string(&value, &["additionalProperties", "github"]),
        versions,
        fetched_at: Utc::now(),
        cached: false,
        cache_age_seconds: None,
        resolved_mirror_base_url: None,
    })
}

/// 从静态镜像按配置顺序读取 catalog.json，并对每个节点执行可选 HMAC 验签。
async fn fetch_mirror_catalog(
    settings: &AppStoreSettings,
    credentials: &Arc<dyn CredentialStore>,
) -> AppResult<AppCatalogSnapshot> {
    let bases = mirror_bases(settings)?;
    let mut failures = Vec::with_capacity(bases.len());
    for (index, base) in bases.iter().enumerate() {
        match fetch_mirror_catalog_from_base(settings, credentials, base).await {
            Ok(snapshot) => return Ok(snapshot),
            Err(error) => failures.push(format!("节点 {}: {}", index + 1, error.message)),
        }
    }
    Err(aggregate_mirror_failures(failures))
}

/// 从一个镜像节点读取目录并验证签名；失败后由上层选择下一个节点。
async fn fetch_mirror_catalog_from_base(
    settings: &AppStoreSettings,
    credentials: &Arc<dyn CredentialStore>,
    base: &str,
) -> AppResult<AppCatalogSnapshot> {
    let client = http_client()?;
    let response = client
        .get(format!("{base}/catalog.json"))
        .send()
        .await
        .map_err(|error| network_error("读取应用商店镜像目录失败", error))?
        .error_for_status()
        .map_err(|error| network_error("应用商店镜像目录响应失败", error))?;
    let payload = response
        .bytes()
        .await
        .map_err(|error| network_error("读取应用商店镜像目录内容失败", error))?;
    let document = serde_json::from_slice::<MirrorCatalog>(&payload).map_err(|error| {
        AppError::new(
            "APPSTORE_PARSE_FAILED",
            "appstore",
            "解析应用商店镜像目录失败",
        )
        .details(error)
    })?;
    let signature = match client.get(format!("{base}/catalog.sig")).send().await {
        Ok(response) if response.status() == reqwest::StatusCode::NOT_FOUND => None,
        Ok(response) => Some(
            response
                .error_for_status()
                .map_err(|error| network_error("应用商店镜像签名响应失败", error))?
                .json::<MirrorSignature>()
                .await
                .map_err(|error| network_error("解析应用商店镜像签名失败", error))?,
        ),
        Err(error) => return Err(network_error("读取应用商店镜像签名失败", error)),
    };
    let signature_present = signature.is_some();
    let signature_verified = if settings.signature_configured {
        let signature = signature.as_ref().ok_or_else(|| {
            AppError::new(
                "APPSTORE_SIGNATURE_MISSING",
                "appstore",
                "镜像已配置验签，但没有 catalog.sig",
            )
        })?;
        let key_id = settings.mirror_key_id.as_deref().ok_or_else(|| {
            AppError::new(
                "APPSTORE_SIGNATURE_INVALID",
                "appstore",
                "镜像验签 key ID 缺失",
            )
        })?;
        let secret = credentials.get(&mirror_verify_key(key_id)).map_err(|_| {
            AppError::new(
                "APPSTORE_SIGNATURE_SECRET_MISSING",
                "appstore",
                "镜像验签令牌不可用",
            )
        })?;
        verify_mirror_signature(&payload, signature, key_id, &secret)?;
        true
    } else {
        false
    };
    let mut items = document
        .items
        .into_iter()
        .filter(|item| valid_key(&item.key))
        .collect::<Vec<_>>();
    items.sort_by_key(|item| item.name.to_lowercase());
    Ok(AppCatalogSnapshot {
        repository: document.repository.unwrap_or_else(|| "mirror".into()),
        branch: document.branch.unwrap_or_else(|| "static".into()),
        source_revision: document.source_revision,
        items,
        fetched_at: Utc::now(),
        cached: false,
        cache_age_seconds: None,
        signature_present,
        signature_verified,
        resolved_mirror_base_url: Some(base.to_string()),
    })
}

/// 从静态镜像按配置顺序读取 metadata YAML 与版本 JSON。
async fn fetch_mirror_detail(settings: &AppStoreSettings, key: &str) -> AppResult<AppDetail> {
    let bases = mirror_bases(settings)?;
    let mut failures = Vec::with_capacity(bases.len());
    for (index, base) in bases.iter().enumerate() {
        match fetch_mirror_detail_from_base(base, key).await {
            Ok(detail) => return Ok(detail),
            Err(error) => failures.push(format!("节点 {}: {}", index + 1, error.message)),
        }
    }
    Err(aggregate_mirror_failures(failures))
}

/// 从一个镜像节点读取单个应用详情；文件 URL 必须留在该节点目录内。
async fn fetch_mirror_detail_from_base(base: &str, key: &str) -> AppResult<AppDetail> {
    let client = http_client()?;
    let metadata = client
        .get(format!("{base}/apps/{key}/data.yml"))
        .send()
        .await
        .map_err(|error| network_error("读取镜像应用详情失败", error))?
        .error_for_status()
        .map_err(|error| network_error("镜像应用详情响应失败", error))?
        .text()
        .await
        .map_err(|error| network_error("读取镜像应用 metadata 失败", error))?;
    let value = serde_yaml::from_str::<serde_yaml::Value>(&metadata).map_err(|error| {
        AppError::new(
            "APPSTORE_PARSE_FAILED",
            "appstore",
            "镜像应用 metadata 无法解析",
        )
        .details(error)
    })?;
    let entries = client
        .get(format!("{base}/apps/{key}/versions.json"))
        .send()
        .await
        .map_err(|error| network_error("读取镜像应用版本失败", error))?
        .error_for_status()
        .map_err(|error| network_error("镜像应用版本响应失败", error))?
        .json::<Vec<MirrorVersion>>()
        .await
        .map_err(|error| network_error("解析镜像应用版本失败", error))?;
    let mut versions = Vec::with_capacity(entries.len());
    for entry in entries {
        if !valid_version(&entry.version) {
            continue;
        }
        let compose_url = entry
            .compose_url
            .map(|value| validate_mirror_file_url(base, &value))
            .transpose()?
            .unwrap_or_else(|| format!("{base}/apps/{key}/{}/docker-compose.yml", entry.version));
        let env_url = entry
            .env_url
            .map(|value| validate_mirror_file_url(base, &value))
            .transpose()?
            .unwrap_or_else(|| format!("{base}/apps/{key}/{}/.env", entry.version));
        versions.push(AppVersion {
            compose_url,
            env_url,
            version: entry.version,
        });
    }
    versions.sort_by(|left, right| right.version.cmp(&left.version));
    Ok(AppDetail {
        key: key.into(),
        name: yaml_string(&value, "name").unwrap_or_else(|| display_name(key)),
        description: yaml_string(&value, "description")
            .unwrap_or_else(|| "镜像 1Panel 应用模板".into()),
        tags: yaml_strings(&value, "tags"),
        website: yaml_nested_string(&value, &["additionalProperties", "website"]),
        github: yaml_nested_string(&value, &["additionalProperties", "github"]),
        versions,
        fetched_at: Utc::now(),
        cached: false,
        cache_age_seconds: None,
        resolved_mirror_base_url: Some(base.to_string()),
    })
}

/// 规范化应用商店设置，限制 TTL 和镜像地址，避免把凭据写入本地设置。
fn normalize_settings(input: SaveAppStoreSettingsInput) -> AppResult<AppStoreSettings> {
    if !matches!(input.source.as_str(), "official" | "mirror") {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "appstore",
            "应用商店来源无效",
        ));
    }
    if !(300..=86_400).contains(&input.cache_ttl_seconds) {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "appstore",
            "应用商店缓存 TTL 必须在 300 到 86400 秒之间",
        ));
    }
    let mut mirror_base_urls = input.mirror_base_urls;
    if mirror_base_urls.is_empty() {
        if let Some(value) = input.mirror_base_url {
            mirror_base_urls.push(value);
        }
    }
    if mirror_base_urls.len() > MAX_MIRROR_BASES {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "appstore",
            format!("镜像节点最多允许 {MAX_MIRROR_BASES} 个"),
        ));
    }
    let mut normalized_mirror_base_urls = Vec::with_capacity(mirror_base_urls.len());
    for value in mirror_base_urls {
        let value = validate_mirror_base_url(&value)?;
        if !normalized_mirror_base_urls.contains(&value) {
            normalized_mirror_base_urls.push(value);
        }
    }
    let mirror = normalized_mirror_base_urls.first().cloned();
    let mirror_key_id = input
        .mirror_key_id
        .map(|value| validate_mirror_key_id(&value))
        .transpose()?;
    if input.source == "mirror" && normalized_mirror_base_urls.is_empty() {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "appstore",
            "镜像来源必须填写基础地址",
        ));
    }
    Ok(AppStoreSettings {
        source: input.source,
        mirror_base_url: mirror,
        mirror_base_urls: normalized_mirror_base_urls,
        cache_ttl_seconds: input.cache_ttl_seconds,
        offline_mode: input.offline_mode,
        mirror_key_id,
        signature_configured: false,
    })
}

/// Returns the ordered mirror nodes, accepting legacy settings with only one base URL.
fn mirror_bases(settings: &AppStoreSettings) -> AppResult<Vec<String>> {
    let mut bases = settings.mirror_base_urls.clone();
    if bases.is_empty() {
        if let Some(base) = settings.mirror_base_url.as_ref() {
            bases.push(base.clone());
        }
    }
    if settings.source != "mirror" || bases.is_empty() {
        return Err(AppError::new(
            "APPSTORE_SETTINGS_INVALID",
            "appstore",
            "镜像源缺少基础地址",
        ));
    }
    Ok(bases)
}

/// Converts individual mirror failures into one recoverable federation error.
fn aggregate_mirror_failures(failures: Vec<String>) -> AppError {
    AppError::new(
        "APPSTORE_MIRROR_UNAVAILABLE",
        "appstore",
        "所有应用商店镜像节点均不可用",
    )
    .details(failures.join("；"))
}

/// Validates the public key identifier used to select a mirror verification secret.
fn validate_mirror_key_id(value: &str) -> AppResult<String> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 80
        || value
            .bytes()
            .any(|byte| !(byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.')))
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "appstore",
            "镜像 key ID 无效",
        ));
    }
    Ok(value.to_string())
}

/// Validates a mirror HMAC secret without returning it in any error or response.
fn validate_mirror_secret(secret: &SecretString) -> AppResult<()> {
    let value = secret.expose_secret().trim();
    if value.len() < 16 || value.len() > 4096 || value.chars().any(char::is_control) {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "appstore",
            "镜像验签令牌长度或字符无效",
        ));
    }
    Ok(())
}

/// Builds the OS keychain reference for one mirror verification key ID.
fn mirror_verify_key(key_id: &str) -> String {
    format!("{MIRROR_VERIFY_KEY_PREFIX}{key_id}")
}

/// 校验镜像基础地址不含凭据、查询参数或片段，并允许本地 HTTP 镜像调试。
fn validate_mirror_base_url(value: &str) -> AppResult<String> {
    let value = value.trim().trim_end_matches('/');
    if value.len() > 512 || value.chars().any(|character| character.is_control()) {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "appstore",
            "镜像地址无效",
        ));
    }
    let parsed = reqwest::Url::parse(value)
        .map_err(|_| AppError::new("VALIDATION_FAILED", "appstore", "镜像地址无效"))?;
    if parsed.username() != ""
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
        || !matches!(parsed.scheme(), "https" | "http")
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "appstore",
            "镜像地址必须是无凭据的 HTTP(S) URL",
        ));
    }
    if parsed.scheme() == "http"
        && !matches!(parsed.host_str(), Some("localhost" | "127.0.0.1" | "::1"))
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "appstore",
            "非本机镜像必须使用 HTTPS",
        ));
    }
    Ok(value.to_string())
}

/// 校验镜像版本文件 URL 与镜像同源，防止 catalog 把模板下载重定向到任意站点。
fn validate_mirror_file_url(base: &str, value: &str) -> AppResult<String> {
    let base_url = reqwest::Url::parse(&format!("{}/", base.trim_end_matches('/')))
        .map_err(|_| AppError::new("VALIDATION_FAILED", "appstore", "镜像基础地址无效"))?;
    let file_url = reqwest::Url::parse(value)
        .or_else(|_| base_url.join(value))
        .map_err(|_| AppError::new("APPSTORE_PARSE_FAILED", "appstore", "镜像版本文件地址无效"))?;
    if file_url.scheme() != base_url.scheme()
        || file_url.host_str() != base_url.host_str()
        || file_url.port_or_known_default() != base_url.port_or_known_default()
        || file_url.username() != ""
        || file_url.password().is_some()
        || file_url.query().is_some()
        || file_url.fragment().is_some()
    {
        return Err(AppError::new(
            "APPSTORE_PARSE_FAILED",
            "appstore",
            "镜像版本文件必须与镜像基础地址同源",
        ));
    }
    let base_path = base_url.path().trim_end_matches('/');
    let file_path = file_url.path();
    if !base_path.is_empty()
        && base_path != "/"
        && file_path != base_path
        && !file_path.starts_with(&format!("{base_path}/"))
    {
        return Err(AppError::new(
            "APPSTORE_PARSE_FAILED",
            "appstore",
            "镜像版本文件必须位于镜像基础目录内",
        ));
    }
    Ok(file_url.to_string())
}

/// Computes the SHA-256 digest used in the detached mirror signature metadata.
fn catalog_sha256(payload: &[u8]) -> String {
    let digest = Sha256::digest(payload);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Signs exact catalog bytes with HMAC-SHA256 for a private mirror federation key.
fn sign_mirror_catalog(
    payload: &[u8],
    key_id: &str,
    secret: &SecretString,
) -> AppResult<MirrorSignature> {
    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret.expose_secret().as_bytes()).map_err(|_| {
            AppError::new(
                "APPSTORE_SIGNATURE_INVALID",
                "appstore",
                "镜像签名令牌无法初始化",
            )
        })?;
    mac.update(payload);
    Ok(MirrorSignature {
        algorithm: "hmac-sha256".into(),
        key_id: key_id.into(),
        catalog_sha256: catalog_sha256(payload),
        signature: BASE64.encode(mac.finalize().into_bytes()),
    })
}

/// Verifies a detached mirror signature without exposing the verification secret.
fn verify_mirror_signature(
    payload: &[u8],
    signature: &MirrorSignature,
    expected_key_id: &str,
    secret: &SecretString,
) -> AppResult<()> {
    if signature.algorithm != "hmac-sha256"
        || signature.key_id != expected_key_id
        || signature.catalog_sha256 != catalog_sha256(payload)
    {
        return Err(AppError::new(
            "APPSTORE_SIGNATURE_INVALID",
            "appstore",
            "镜像目录签名元数据无效",
        ));
    }
    let provided = BASE64.decode(&signature.signature).map_err(|_| {
        AppError::new(
            "APPSTORE_SIGNATURE_INVALID",
            "appstore",
            "镜像目录签名编码无效",
        )
    })?;
    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret.expose_secret().as_bytes()).map_err(|_| {
            AppError::new(
                "APPSTORE_SIGNATURE_INVALID",
                "appstore",
                "镜像验签令牌无法初始化",
            )
        })?;
    mac.update(payload);
    mac.verify_slice(&provided).map_err(|_| {
        AppError::new(
            "APPSTORE_SIGNATURE_INVALID",
            "appstore",
            "镜像目录签名校验失败",
        )
    })
}

/// 读取缓存记录而不把损坏的缓存误当作网络错误。
async fn read_cache<T>(local: &LocalRepository, key: &str) -> AppResult<Option<CachedRecord<T>>>
where
    T: for<'de> Deserialize<'de>,
{
    let Some(raw) = local.get_setting(key).await? else {
        return Ok(None);
    };
    Ok(serde_json::from_str(&raw).ok())
}

/// 写入一个有限大小的应用商店 JSON 缓存。
async fn write_cache<T>(local: &LocalRepository, key: &str, value: &T) -> AppResult<()>
where
    T: Serialize,
{
    let record = CachedRecord {
        cached_at: Utc::now(),
        value,
    };
    let serialized = serde_json::to_string(&record).map_err(AppError::database)?;
    if serialized.len() > 8 * 1024 * 1024 {
        return Err(AppError::new(
            "APPSTORE_CACHE_TOO_LARGE",
            "appstore",
            "应用商店缓存过大",
        ));
    }
    local.set_setting(key, &serialized).await
}

/// 以秒为单位判断目录或详情缓存是否仍在 TTL 内。
fn cache_is_fresh(cached_at: DateTime<Utc>, ttl_seconds: u64) -> bool {
    cache_age_seconds(cached_at) < ttl_seconds
}

/// 返回非负缓存年龄，系统时钟回拨时按零处理。
fn cache_age_seconds(cached_at: DateTime<Utc>) -> u64 {
    Utc::now()
        .signed_duration_since(cached_at)
        .num_seconds()
        .max(0) as u64
}

/// 把目录缓存转换为前端可识别的缓存响应，并保留原始抓取时间。
fn cached_catalog(mut record: CachedRecord<AppCatalogSnapshot>) -> AppCatalogSnapshot {
    record.value.cached = true;
    record.value.cache_age_seconds = Some(cache_age_seconds(record.cached_at));
    record.value
}

/// 把详情缓存转换为前端可识别的缓存响应，并保留原始抓取时间。
fn cached_detail(mut record: CachedRecord<AppDetail>) -> AppDetail {
    record.value.cached = true;
    record.value.cache_age_seconds = Some(cache_age_seconds(record.cached_at));
    record.value
}

/// 生成安全的详情缓存键；key 已由调用方校验。
fn detail_cache_key(key: &str) -> String {
    format!("{DETAIL_CACHE_PREFIX}{key}")
}

/// 探测固定安装根目录下的 Compose 应用，不执行用户命令或读取 secrets。
pub async fn installed(
    ssh: &SshConnectionManager,
    server_id: &str,
) -> AppResult<InstalledAppsSnapshot> {
    let command = r#"set +e
compose=''
if docker compose version >/dev/null 2>&1; then compose='docker compose'; elif command -v docker-compose >/dev/null 2>&1; then compose='docker-compose'; fi
printf '__COMPOSE__\t%s\n' "$compose"
find /opt/1panel/apps -mindepth 3 -maxdepth 3 -type f -name docker-compose.yml -print 2>/dev/null | while IFS= read -r compose_path; do
  path=${compose_path%/docker-compose.yml}; project=$(basename "$path"); key=$(basename "$(dirname "$path")"); status='unknown'; details=''; compose_ports=''; env_ports=''; if [ -n "$compose" ]; then status=$($compose -f "$compose_path" -p "$project" ps --format '{{.State}}' 2>/dev/null | head -n 1); ids=$($compose -f "$compose_path" -p "$project" ps -q 2>/dev/null); if [ -n "$ids" ]; then details=$(docker inspect --format '{{.Config.Image}}|{{.Created}}|{{json .NetworkSettings.Ports}}|{{.HostConfig.NetworkMode}}|{{json .Config.ExposedPorts}}' $ids 2>/dev/null | tr '\n' ';'); fi; fi; [ -f "$compose_path" ] && compose_ports=$(grep -oE '^[[:space:]]*-[[:space:]]*"?[0-9]{1,5}:[0-9]{1,5}' "$compose_path" 2>/dev/null | grep -oE '[0-9]{1,5}:[0-9]{1,5}' | cut -d: -f1 | sort -n -u | tr '\n' ' ' | sed 's/[[:space:]]*$//' ); env_ports=$(grep -oE '^PANEL_APP_PORT[A-Z_]*=[0-9]+' "$path/.env" 2>/dev/null | cut -d= -f2 | sort -n -u | tr '\n' ' ' | sed 's/[[:space:]]*$//' ); printf '__APP__\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$key" "$path" "$compose_path" "$status" "$details" "$compose_ports" "$env_ports";
done"#;
    let result = ssh
        .execute_system(server_id, command, Duration::from_secs(45))
        .await?;
    if result.exit_code != 0 {
        return Err(
            AppError::new("APPSTORE_PROBE_FAILED", "appstore", "远端应用探测失败")
                .details(result.stderr)
                .for_server(server_id),
        );
    }
    parse_installed(&result.stdout).ok_or_else(|| {
        AppError::new(
            "APPSTORE_PARSE_FAILED",
            "appstore",
            "远端应用探测结果无法解析",
        )
        .for_server(server_id)
    })
}

/// 读取指定应用 `.env` 的键摘要，绝不把环境变量值通过 SSH 返回客户端。
/// 读取 Compose 项目的容器状态和 healthcheck，不把环境变量或完整 inspect 回传客户端。
pub async fn health(
    ssh: &SshConnectionManager,
    input: AppHealthInput,
) -> AppResult<AppHealthSnapshot> {
    validate_project(&input.project)?;
    let root = validate_install_path(&input.install_path)?;
    let compose_path = format!("{root}/docker-compose.yml");
    let command = format!(
        "set +e; compose=''; if docker compose version >/dev/null 2>&1; then compose='docker compose'; elif command -v docker-compose >/dev/null 2>&1; then compose='docker-compose'; fi; if [ -z \"$compose\" ]; then printf '__COMPOSE_MISSING__\\n'; exit 127; fi; if [ ! -f {compose_file} ]; then printf '__APP_HEALTH_MISSING__\\n'; exit 2; fi; ids=$($compose -f {compose_file} -p {project} ps -q 2>/dev/null); if [ -z \"$ids\" ]; then printf '__APP_HEALTH_EMPTY__\\n'; exit 0; fi; for id in $ids; do docker inspect --format '__APP_HEALTH__\\t{{{{.Name}}}}\\t{{{{.Config.Image}}}}\\t{{{{.State.Status}}}}\\t{{{{if .State.Health}}}}{{{{.State.Health.Status}}}}{{{{else}}}}none{{{{end}}}}\\t{{{{.State.ExitCode}}}}' \"$id\"; done",
        compose_file = shell_escape(&compose_path),
        project = shell_escape(&input.project),
    );
    let result = ssh
        .execute_system(&input.server_id, &command, Duration::from_secs(60))
        .await?;
    if result.stdout.contains("__COMPOSE_MISSING__") {
        return Err(
            AppError::new("COMMAND_UNAVAILABLE", "appstore", "远端没有 Docker Compose")
                .for_server(input.server_id),
        );
    }
    if result.stdout.contains("__APP_HEALTH_MISSING__") {
        return Err(AppError::new(
            "APP_NOT_FOUND",
            "appstore",
            "应用目录没有 docker-compose.yml",
        )
        .for_server(input.server_id));
    }
    if result.exit_code != 0 && !result.stdout.contains("__APP_HEALTH_EMPTY__") {
        return Err(
            AppError::new("APP_HEALTH_FAILED", "appstore", "读取应用健康状态失败")
                .details(result.stderr)
                .for_server(input.server_id),
        );
    }
    let services = parse_health(&result.stdout);
    Ok(AppHealthSnapshot {
        project: input.project,
        path: root,
        overall: overall_health(&services),
        services,
        fetched_at: Utc::now(),
    })
}

pub async fn environment(
    ssh: &SshConnectionManager,
    server_id: &str,
    install_path: &str,
) -> AppResult<AppEnvironmentSnapshot> {
    let root = validate_environment_path(install_path)?;
    let path = format!("{root}/.env");
    let command = format!(
        "if test -f {path}; then awk -F= '/^[A-Za-z_][A-Za-z0-9_]*=/ {{ print \"__ENV__\\t\" $1 }}' {path}; fi",
        path = shell_escape(&path)
    );
    let result = ssh
        .execute_system(server_id, &command, Duration::from_secs(30))
        .await?;
    if result.exit_code != 0 {
        return Err(
            AppError::new("APP_ENV_READ_FAILED", "appstore", "读取应用环境变量失败")
                .details(result.stderr)
                .for_server(server_id),
        );
    }
    Ok(AppEnvironmentSnapshot {
        path,
        entries: parse_environment_keys(&result.stdout),
    })
}

/// 将用户提交的环境变量合并写入远端 `.env`，保留未提交的原有秘密值。
pub async fn save_environment(
    ssh: &SshConnectionManager,
    input: AppEnvironmentInput,
) -> AppResult<AppActionResult> {
    if !input.confirmed {
        return Err(AppError::new(
            "CONFIRMATION_REQUIRED",
            "appstore",
            "保存应用环境变量需要明确确认",
        )
        .for_server(&input.server_id));
    }
    let root = validate_environment_path(&input.install_path)?;
    let environment = validate_environment(&input.values)?;
    let override_path = format!("/tmp/.1panel-client-env-{}", uuid::Uuid::new_v4());
    write_remote_file(ssh, &input.server_id, &override_path, &environment).await?;
    let env_path = format!("{root}/.env");
    let temporary_path = format!("{env_path}.tmp-{}", uuid::Uuid::new_v4());
    let command = format!(
        "set -e; mkdir -p -- {root}; if test -f {env}; then awk -F= 'NR==FNR {{ override[$1]=$0; next }} /^[A-Za-z_][A-Za-z0-9_]*=/ {{ if ($1 in override) {{ print override[$1]; delete override[$1]; next }} }} {{ print }} END {{ for (key in override) print override[key] }}' {override} {env} > {temporary}; else cp -- {override} {temporary}; fi; install -m 0600 -- {temporary} {env}; rm -f -- {temporary} {override}",
        root = shell_escape(&root),
        env = shell_escape(&env_path),
        override = shell_escape(&override_path),
        temporary = shell_escape(&temporary_path)
    );
    let result = ssh
        .execute_system(&input.server_id, &command, Duration::from_secs(30))
        .await?;
    if result.exit_code != 0 {
        let cleanup = format!(
            "rm -f -- {} {}",
            shell_escape(&temporary_path),
            shell_escape(&override_path)
        );
        let _ = ssh
            .execute_system(&input.server_id, &cleanup, Duration::from_secs(30))
            .await;
        return Err(
            AppError::new("APP_ENV_SAVE_FAILED", "appstore", "保存应用环境变量失败")
                .details(result.stderr)
                .for_server(&input.server_id),
        );
    }
    Ok(AppActionResult {
        key: root.rsplit('/').next().unwrap_or_default().into(),
        project: root.rsplit('/').next().unwrap_or_default().into(),
        action: "environment".into(),
        output: "应用环境变量已合并保存".into(),
    })
}

/// 下载当前应用商店来源的 Compose 模板、写入远端应用目录并执行 `docker compose up -d`。
pub async fn install(
    ssh: &SshConnectionManager,
    local: &LocalRepository,
    credentials: &Arc<dyn CredentialStore>,
    input: InstallAppInput,
) -> AppResult<AppActionResult> {
    validate_install(&input)?;
    let detail = detail(local, credentials, &input.key).await?;
    let version = detail
        .versions
        .iter()
        .find(|value| value.version == input.version)
        .ok_or_else(|| AppError::new("APP_VERSION_NOT_FOUND", "appstore", "应用版本不存在"))?;
    let compose = download_text(&version.compose_url).await?;
    if compose.trim().is_empty() || compose.len() > 4 * 1024 * 1024 {
        return Err(AppError::new(
            "APP_COMPOSE_INVALID",
            "appstore",
            "应用商店 Compose 文件为空或过大",
        ));
    }
    let compose_path = format!(
        "{}/docker-compose.yml",
        input.install_path.trim_end_matches('/')
    );
    let mkdir = format!("mkdir -p -- {}", shell_escape(&input.install_path));
    let result = ssh
        .execute_system(&input.server_id, &mkdir, Duration::from_secs(30))
        .await?;
    if result.exit_code != 0 {
        return Err(
            AppError::new("APP_INSTALL_FAILED", "appstore", "无法创建应用目录")
                .details(result.stderr)
                .for_server(&input.server_id),
        );
    }
    write_remote_file(ssh, &input.server_id, &compose_path, &compose).await?;
    if !input.environment.is_empty() {
        let env = validate_environment(&input.environment)?;
        write_remote_file(
            ssh,
            &input.server_id,
            &format!("{}/.env", input.install_path.trim_end_matches('/')),
            &env,
        )
        .await?;
    }
    let command = compose_command(&input.project, &compose_path, "config -q && up -d")?;
    let result = ssh
        .execute_system(&input.server_id, &command, Duration::from_secs(900))
        .await?;
    if result.exit_code != 0 {
        return Err(
            AppError::new("APP_INSTALL_FAILED", "appstore", "应用 Compose 启动失败")
                .details(result.stderr)
                .for_server(&input.server_id),
        );
    }
    Ok(AppActionResult {
        key: input.key,
        project: input.project,
        action: "install".into(),
        output: result.stdout,
    })
}

/// 执行应用的启动、停止、重启、更新、卸载或卸载后的安全恢复 Compose 生命周期。
pub async fn action(
    ssh: &SshConnectionManager,
    local: &LocalRepository,
    credentials: &Arc<dyn CredentialStore>,
    input: AppActionInput,
) -> AppResult<AppActionResult> {
    validate_action(&input)?;
    if input.action == "update" {
        return upgrade_latest(ssh, local, credentials, input).await;
    }
    let compose_path = format!(
        "{}/docker-compose.yml",
        input.install_path.trim_end_matches('/')
    );
    let operation = match input.action.as_str() {
        "start" => "start",
        "stop" => "stop",
        "restart" => "restart",
        "pull" => "pull",
        "uninstall" => "down",
        "restore" => "config -q && up -d",
        _ => {
            return Err(AppError::new(
                "VALIDATION_FAILED",
                "appstore",
                "不支持的应用操作",
            ))
        }
    };
    let command = compose_command(&input.project, &compose_path, operation)?;
    let result = ssh
        .execute_system(&input.server_id, &command, Duration::from_secs(900))
        .await?;
    if result.exit_code != 0 {
        return Err(
            AppError::new("APP_ACTION_FAILED", "appstore", "应用 Compose 操作失败")
                .details(result.stderr)
                .for_server(&input.server_id),
        );
    }
    Ok(AppActionResult {
        key: input.key,
        project: input.project,
        action: input.action,
        output: result.stdout,
    })
}

/// 下载当前来源最新 Compose 模板，并在远端生成只读差异摘要；不会修改现有应用或启动容器。
pub async fn update_preview(
    ssh: &SshConnectionManager,
    local: &LocalRepository,
    credentials: &Arc<dyn CredentialStore>,
    input: AppUpdatePreviewInput,
) -> AppResult<AppUpdatePreview> {
    validate_preview(&input)?;
    let detail = detail(local, credentials, &input.key).await?;
    let version = detail
        .versions
        .first()
        .ok_or_else(|| AppError::new("APP_VERSION_NOT_FOUND", "appstore", "应用没有可预览版本"))?;
    let compose = download_text(&version.compose_url).await?;
    if compose.trim().is_empty() || compose.len() > 4 * 1024 * 1024 {
        return Err(AppError::new(
            "APP_COMPOSE_INVALID",
            "appstore",
            "应用商店 Compose 文件为空或过大",
        ));
    }
    let compose_path = format!(
        "{}/docker-compose.yml",
        input.install_path.trim_end_matches('/')
    );
    let temporary_path = write_remote_temporary(ssh, &input.server_id, &compose).await?;
    let command = format!(
        "set +e; hash_file() {{ if command -v sha256sum >/dev/null 2>&1; then sha256sum -- \"$1\" | awk '{{print $1}}'; elif command -v shasum >/dev/null 2>&1; then shasum -a 256 -- \"$1\" | awk '{{print $1}}'; else printf '%s' ''; fi; }}; if [ ! -f {current} ]; then printf '__APP_UPDATE_PREVIEW__\\t\\t%s\\t0\\t%s\\tmissing\\n' \"$(hash_file {latest})\" {latest_lines}; rm -f -- {latest}; exit 0; fi; current_hash=$(hash_file {current}); latest_hash=$(hash_file {latest}); current_lines=$(wc -l < {current}); latest_lines=$(wc -l < {latest}); changed=0; cmp -s -- {current} {latest} || changed=1; printf '__APP_UPDATE_PREVIEW__\\t%s\\t%s\\t%s\\t%s\\t%s\\n' \"$current_hash\" \"$latest_hash\" \"$current_lines\" \"$latest_lines\" \"$changed\"; rm -f -- {latest}",
        current = shell_escape(&compose_path),
        latest = shell_escape(&temporary_path),
        latest_lines = compose.lines().count(),
    );
    let result = match ssh
        .execute_system(&input.server_id, &command, Duration::from_secs(60))
        .await
    {
        Ok(result) => result,
        Err(error) => {
            let _ = remove_remote_file(ssh, &input.server_id, &temporary_path).await;
            return Err(error);
        }
    };
    if result.exit_code != 0 {
        let _ = remove_remote_file(ssh, &input.server_id, &temporary_path).await;
        return Err(
            AppError::new("APP_PREVIEW_FAILED", "appstore", "读取应用升级差异失败")
                .details(result.stderr)
                .for_server(input.server_id),
        );
    }
    parse_update_preview(&result.stdout, &input.key, &input.project, &version.version).ok_or_else(
        || {
            AppError::new(
                "APP_PREVIEW_PARSE_FAILED",
                "appstore",
                "应用升级差异结果无法解析",
            )
            .for_server(input.server_id)
        },
    )
}

/// 下载当前来源最新 Compose 模板，原子替换远端配置并在失败时恢复旧版本。
async fn upgrade_latest(
    ssh: &SshConnectionManager,
    local: &LocalRepository,
    credentials: &Arc<dyn CredentialStore>,
    input: AppActionInput,
) -> AppResult<AppActionResult> {
    let detail = detail(local, credentials, &input.key).await?;
    let version = detail
        .versions
        .first()
        .ok_or_else(|| AppError::new("APP_VERSION_NOT_FOUND", "appstore", "应用没有可升级版本"))?;
    let compose = download_text(&version.compose_url).await?;
    if compose.trim().is_empty() || compose.len() > 4 * 1024 * 1024 {
        return Err(AppError::new(
            "APP_COMPOSE_INVALID",
            "appstore",
            "应用商店 Compose 文件为空或过大",
        ));
    }
    let compose_path = format!(
        "{}/docker-compose.yml",
        input.install_path.trim_end_matches('/')
    );
    let backup_path = format!(
        "{}.1panel-client-upgrade-{}",
        compose_path,
        uuid::Uuid::new_v4()
    );
    let backup = format!(
        "test -f {compose} && cp -p -- {compose} {backup}",
        compose = shell_escape(&compose_path),
        backup = shell_escape(&backup_path)
    );
    let backup_result = ssh
        .execute_system(&input.server_id, &backup, Duration::from_secs(30))
        .await?;
    if backup_result.exit_code != 0 {
        return Err(AppError::new(
            "APP_UPGRADE_FAILED",
            "appstore",
            "无法备份现有 Compose 配置",
        )
        .details(backup_result.stderr)
        .for_server(&input.server_id));
    }
    if let Err(error) = write_remote_file(ssh, &input.server_id, &compose_path, &compose).await {
        let _ = restore_compose(ssh, &input.server_id, &compose_path, &backup_path).await;
        return Err(error);
    }
    let command = compose_command(&input.project, &compose_path, "config -q && pull && up -d")?;
    let result = ssh
        .execute_system(&input.server_id, &command, Duration::from_secs(900))
        .await?;
    if result.exit_code != 0 {
        let _ = restore_compose(ssh, &input.server_id, &compose_path, &backup_path).await;
        return Err(AppError::new(
            "APP_UPGRADE_FAILED",
            "appstore",
            "应用升级失败，已尝试恢复旧配置",
        )
        .details(result.stderr)
        .for_server(&input.server_id));
    }
    let cleanup = format!("rm -f -- {}", shell_escape(&backup_path));
    let _ = ssh
        .execute_system(&input.server_id, &cleanup, Duration::from_secs(30))
        .await;
    Ok(AppActionResult {
        key: input.key,
        project: input.project,
        action: format!("update:{}", version.version),
        output: result.stdout,
    })
}

/// 从升级备份恢复旧 Compose 文件，并清理临时备份。
async fn restore_compose(
    ssh: &SshConnectionManager,
    server_id: &str,
    compose_path: &str,
    backup_path: &str,
) -> AppResult<()> {
    let command = format!(
        "if test -f {backup}; then install -m 0644 -- {backup} {compose}; rm -f -- {backup}; fi",
        backup = shell_escape(backup_path),
        compose = shell_escape(compose_path)
    );
    let result = ssh
        .execute_system(server_id, &command, Duration::from_secs(30))
        .await?;
    if result.exit_code != 0 {
        return Err(AppError::new(
            "APP_UPGRADE_ROLLBACK_FAILED",
            "appstore",
            "应用升级回滚失败",
        )
        .details(result.stderr)
        .for_server(server_id));
    }
    Ok(())
}

/// 构造带 Docker Compose 兼容探测的安全命令。
fn compose_command(project: &str, compose_path: &str, operation: &str) -> AppResult<String> {
    let project = validate_project(project)?;
    if !compose_path.starts_with('/') || compose_path.contains("..") || compose_path.contains('\n')
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "appstore",
            "应用 Compose 路径无效",
        ));
    }
    let compose_file = shell_escape(compose_path);
    let compose_project = shell_escape(&project);
    let command_template = match operation {
        "start" | "stop" | "restart" | "pull" | "down" => operation.to_string(),
        "pull && up -d" => format!(
            "pull && __COMPOSE__ -f {compose_file} -p {compose_project} up -d"
        ),
        "config -q && up -d" => format!(
            "config -q && __COMPOSE__ -f {compose_file} -p {compose_project} up -d"
        ),
        "config -q && pull && up -d" => format!(
            "config -q && __COMPOSE__ -f {compose_file} -p {compose_project} pull && __COMPOSE__ -f {compose_file} -p {compose_project} up -d"
        ),
        _ => {
            return Err(AppError::new(
                "VALIDATION_FAILED",
                "appstore",
                "不支持的 Compose 操作",
            ))
        }
    };
    let modern = command_template.replace("__COMPOSE__", "docker compose");
    let legacy = command_template.replace("__COMPOSE__", "docker-compose");
    Ok(format!(
        "if docker compose version >/dev/null 2>&1; then docker compose -f {compose_file} -p {compose_project} {modern}; elif command -v docker-compose >/dev/null 2>&1; then docker-compose -f {compose_file} -p {compose_project} {legacy}; else echo 'docker compose not found' >&2; exit 127; fi"
    ))
}

/// 将 Compose 文本通过 SFTP 写入远端，并确保临时文件不会残留。
async fn write_remote_file(
    ssh: &SshConnectionManager,
    server_id: &str,
    path: &str,
    content: &str,
) -> AppResult<()> {
    let temporary = format!("/tmp/.1panel-client-app-{}.tmp", uuid::Uuid::new_v4());
    let sftp = ssh.open_sftp(server_id).await?;
    let mut file = sftp.create(&temporary).await.map_err(|error| {
        AppError::new("SFTP_FAILED", "appstore", "无法创建应用临时文件")
            .details(error)
            .for_server(server_id)
    })?;
    file.write_all(content.as_bytes()).await.map_err(|error| {
        AppError::new("SFTP_FAILED", "appstore", "无法写入应用配置")
            .details(error)
            .for_server(server_id)
    })?;
    file.flush().await.map_err(|error| {
        AppError::new("SFTP_FAILED", "appstore", "无法刷新应用配置")
            .details(error)
            .for_server(server_id)
    })?;
    drop(file);
    let command = format!(
        "install -m 0644 -- {} {} && rm -f -- {}",
        shell_escape(&temporary),
        shell_escape(path),
        shell_escape(&temporary)
    );
    let result = ssh
        .execute_system(server_id, &command, Duration::from_secs(30))
        .await?;
    let _ = sftp.close().await;
    if result.exit_code != 0 {
        return Err(
            AppError::new("APP_INSTALL_FAILED", "appstore", "无法写入应用配置")
                .details(result.stderr)
                .for_server(server_id),
        );
    }
    Ok(())
}

/// 将官方模板写入远端临时文件，供升级预览比较后由调用方清理。
async fn write_remote_temporary(
    ssh: &SshConnectionManager,
    server_id: &str,
    content: &str,
) -> AppResult<String> {
    let temporary = format!("/tmp/.1panel-client-preview-{}.tmp", uuid::Uuid::new_v4());
    let sftp = ssh.open_sftp(server_id).await?;
    let mut file = sftp.create(&temporary).await.map_err(|error| {
        AppError::new("SFTP_FAILED", "appstore", "无法创建升级预览临时文件")
            .details(error)
            .for_server(server_id)
    })?;
    file.write_all(content.as_bytes()).await.map_err(|error| {
        AppError::new("SFTP_FAILED", "appstore", "无法写入升级预览临时文件")
            .details(error)
            .for_server(server_id)
    })?;
    file.flush().await.map_err(|error| {
        AppError::new("SFTP_FAILED", "appstore", "无法刷新升级预览临时文件")
            .details(error)
            .for_server(server_id)
    })?;
    drop(file);
    let _ = sftp.close().await;
    Ok(temporary)
}

/// 尝试删除远端预览临时文件；清理失败不覆盖原始业务结果。
async fn remove_remote_file(
    ssh: &SshConnectionManager,
    server_id: &str,
    path: &str,
) -> AppResult<()> {
    let result = ssh
        .execute_system(
            server_id,
            &format!("rm -f -- {}", shell_escape(path)),
            Duration::from_secs(30),
        )
        .await?;
    if result.exit_code != 0 {
        return Err(
            AppError::new("SFTP_FAILED", "appstore", "无法清理远端预览临时文件")
                .details(result.stderr)
                .for_server(server_id),
        );
    }
    Ok(())
}

/// 下载受来源 URL 校验保护的应用文本文件，并限制响应体大小。
async fn download_text(url: &str) -> AppResult<String> {
    let client = http_client()?;
    let response = client
        .get(url)
        .header("User-Agent", "1panel-client")
        .send()
        .await
        .map_err(|error| network_error("下载应用模板失败", error))?
        .error_for_status()
        .map_err(|error| network_error("应用模板响应失败", error))?;
    let bytes = response
        .bytes()
        .await
        .map_err(|error| network_error("读取应用模板失败", error))?;
    if bytes.len() > 4 * 1024 * 1024 {
        return Err(AppError::new(
            "APP_COMPOSE_INVALID",
            "appstore",
            "应用模板超过大小限制",
        ));
    }
    String::from_utf8(bytes.to_vec()).map_err(|error| {
        AppError::new("APP_COMPOSE_INVALID", "appstore", "应用模板不是 UTF-8").details(error)
    })
}

/// 解析远端应用探测 marker，兼容尚未安装任何应用的目录。
fn parse_installed(output: &str) -> Option<InstalledAppsSnapshot> {
    let mut compose_available = false;
    let mut apps = Vec::new();
    for line in output.lines() {
        let fields = line.split('\t').collect::<Vec<_>>();
        match fields.first().copied() {
            Some("__COMPOSE__") => {
                compose_available = fields.get(1).is_some_and(|value| !value.is_empty())
            }
            Some("__APP__") if fields.len() >= 5 => {
                let (version, mut host_ports, installed_seconds, exposed_ports) = fields
                    .get(5)
                    .map(|raw| parse_app_details(raw))
                    .unwrap_or_default();
                // 容器运行在 host 网络或 inspect 无端口映射时（NetworkSettings.Ports 为空），
                // 回退到 Compose 模板的 ports 段，与 Web 端安装记录中的端口一致。
                if host_ports.is_empty() {
                    host_ports = fields
                        .get(6)
                        .map(|raw| raw.split_whitespace().map(str::to_string).collect())
                        .unwrap_or_default();
                }
                // 模板未声明 ports 段（如 openresty 的 host 网络部署）时，
                // 读取安装目录 `.env` 中面板写入的 PANEL_APP_PORT_HTTP/HTTPS 安装参数
                // （与 Web 端安装记录中的端口一致），仅窃取端口数字、不回传环境变量原文。
                if host_ports.is_empty() {
                    host_ports = fields
                        .get(7)
                        .map(|raw| raw.split_whitespace().map(str::to_string).collect())
                        .unwrap_or_default();
                }
                // 以上来源均缺失时，按镜像声明的 ExposedPorts 兜底进端口列表。
                if host_ports.is_empty() {
                    host_ports = exposed_ports;
                }
                apps.push(InstalledApp {
                    key: fields[1].into(),
                    path: fields[2].into(),
                    compose_path: fields[3].into(),
                    project: fields[2].rsplit('/').next().unwrap_or_default().into(),
                    status: fields[4].into(),
                    version,
                    host_ports,
                    installed_seconds,
                });
            }
            _ => {}
        }
    }
    if !output.lines().any(|line| line.starts_with("__COMPOSE__")) {
        return None;
    }
    Some(InstalledAppsSnapshot {
        compose_available,
        apps,
        fetched_at: Utc::now(),
    })
}

/// 解析 inspect 摘要 `image|created|ports-json|network-mode|exposed-json`（多个容器以 `;`
/// 分隔后逐段解析），提取版本、宿主端口、创建时间与 host 网络下镜像声明的端口；空段直接跳过。
fn parse_app_details(raw: &str) -> (Option<String>, Vec<String>, Option<u64>, Vec<String>) {
    let mut version = None;
    let mut host_ports = Vec::new();
    let mut installed_seconds = None;
    let mut exposed_ports = Vec::new();
    let mut all_host_network = true;
    for segment in raw.split(';') {
        let segment = segment.trim();
        if segment.is_empty() {
            continue;
        }
        let mut parts = segment.splitn(5, '|');
        let image = parts.next().unwrap_or("").trim();
        let created = parts.next().unwrap_or("").trim();
        let ports = parts.next().unwrap_or("").trim();
        let network_mode = parts.next().unwrap_or("").trim();
        let exposed = parts.next().unwrap_or("").trim();
        if version.is_none() {
            version = image_version(image);
        }
        host_ports.extend(parse_host_ports(ports));
        if installed_seconds.is_none() {
            installed_seconds = parse_installed_seconds(created);
        }
        // 仅当所有容器都运行在 host 网络时才启用 ExposedPorts 兜底：
        // 桥接网络下容器端口不等同于宿主端口，直接展示镜像声明会误导用户。
        if !network_mode.eq_ignore_ascii_case("host") && !network_mode.is_empty() {
            all_host_network = false;
        }
        exposed_ports.extend(parse_exposed_ports(exposed));
    }
    exposed_ports.sort_by_key(|value| value.parse::<u64>().unwrap_or(u64::MAX));
    exposed_ports.dedup();
    if !all_host_network {
        exposed_ports.clear();
    }
    (version, host_ports, installed_seconds, exposed_ports)
}

/// 从镜像名提取标签版本；digest 或无名镜像不返回版本。
fn image_version(image: &str) -> Option<String> {
    if image.is_empty() || image.contains('@') {
        return None;
    }
    let tag = image.rsplit(':').next().unwrap_or("");
    if tag.is_empty() || tag == image {
        return None;
    }
    Some(tag.to_string())
}

/// 从 `docker inspect --format {{json .NetworkSettings.Ports}}` 输出提取宿主端口列表。
fn parse_host_ports(json_ports: &str) -> Vec<String> {
    if json_ports.is_empty() {
        return Vec::new();
    }
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json_ports) else {
        return Vec::new();
    };
    let Some(map) = value.as_object() else {
        return Vec::new();
    };
    let mut ports = Vec::new();
    for entry in map.values() {
        if let Some(bindings) = entry.as_array() {
            for binding in bindings {
                if let Some(host_port) = binding.get("HostPort").and_then(|value| value.as_str()) {
                    if !host_port.is_empty() {
                        ports.push(host_port.to_string());
                    }
                }
            }
        }
    }
    ports.sort_by_key(|value| value.parse::<u64>().unwrap_or(u64::MAX));
    ports.dedup();
    ports
}

/// 解析镜像声明的 `{"80/tcp": {}, "443/tcp": {}}`，提取裸端口并按数值排序去重。
fn parse_exposed_ports(json_exposed: &str) -> Vec<String> {
    if json_exposed.is_empty() {
        return Vec::new();
    }
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json_exposed) else {
        return Vec::new();
    };
    let Some(map) = value.as_object() else {
        return Vec::new();
    };
    let mut ports = Vec::new();
    for key in map.keys() {
        let port = key.split('/').next().unwrap_or("");
        if !port.is_empty() {
            ports.push(port.to_string());
        }
    }
    ports.sort_by_key(|value| value.parse::<u64>().unwrap_or(u64::MAX));
    ports.dedup();
    ports
}

/// 把 RFC3339 容器创建时间换算为距离现在秒数；无法解析时返回 None。
fn parse_installed_seconds(created: &str) -> Option<u64> {
    let created = created.trim();
    if created.is_empty() {
        return None;
    }
    let parsed = chrono::DateTime::parse_from_rfc3339(created).ok()?;
    Some(
        (Utc::now() - parsed.with_timezone(&Utc))
            .num_seconds()
            .max(0) as u64,
    )
}

/// 解析 Docker inspect marker，丢弃容器环境变量、挂载和完整配置等敏感字段。
fn parse_health(output: &str) -> Vec<AppServiceHealth> {
    output
        .lines()
        .filter_map(|line| {
            let fields = line.split('\t').collect::<Vec<_>>();
            if fields.first().copied() != Some("__APP_HEALTH__") || fields.len() < 6 {
                return None;
            }
            Some(AppServiceHealth {
                name: fields[1].trim_start_matches('/').to_string(),
                image: fields[2].to_string(),
                state: fields[3].to_string(),
                health: fields[4].to_string(),
                exit_code: fields[5].parse().unwrap_or(-1),
            })
        })
        .collect()
}

/// 根据服务状态计算应用整体健康级别，供 UI 以统一色彩展示。
fn overall_health(services: &[AppServiceHealth]) -> String {
    if services.is_empty() {
        return "stopped".into();
    }
    if services
        .iter()
        .any(|service| service.health == "unhealthy" || service.state == "dead")
    {
        return "degraded".into();
    }
    if services.iter().all(|service| service.state == "running") {
        "healthy".into()
    } else {
        "degraded".into()
    }
}

/// 解析远端环境变量 marker，仅保留安全键名和掩码状态。
fn parse_environment_keys(output: &str) -> Vec<AppEnvironmentEntry> {
    output
        .lines()
        .filter_map(|line| {
            let mut fields = line.split('\t');
            if fields.next() == Some("__ENV__") {
                Some(fields.next()?.to_string())
            } else {
                None
            }
        })
        .filter(|key| valid_environment_key(key))
        .map(|key| AppEnvironmentEntry {
            key,
            configured: true,
            masked_value: "••••••".into(),
        })
        .collect()
}

/// 解析升级预览 marker，只保留哈希、行数和变更状态。
fn parse_update_preview(
    output: &str,
    key: &str,
    project: &str,
    latest_version: &str,
) -> Option<AppUpdatePreview> {
    output.lines().find_map(|line| {
        let fields = line.split('\t').collect::<Vec<_>>();
        if fields.first().copied() != Some("__APP_UPDATE_PREVIEW__") || fields.len() < 6 {
            return None;
        }
        let current_hash = (!fields[1].is_empty()).then(|| fields[1].to_string());
        let latest_hash = (!fields[2].is_empty()).then(|| fields[2].to_string())?;
        let current_lines = fields[3].parse().ok()?;
        let latest_lines = fields[4].parse().ok()?;
        let current_missing = fields[5] == "missing";
        let changed = current_missing || fields[5] == "1";
        Some(AppUpdatePreview {
            key: key.into(),
            project: project.into(),
            latest_version: latest_version.into(),
            current_hash,
            latest_hash,
            current_lines,
            latest_lines,
            changed,
            current_missing,
            fetched_at: Utc::now(),
        })
    })
}

/// 创建复用 TLS、超时和 GitHub User-Agent 的 HTTP 客户端。
fn http_client() -> AppResult<Client> {
    Client::builder()
        .user_agent("1panel-client")
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| {
            AppError::new(
                "NETWORK_UNAVAILABLE",
                "appstore",
                "无法初始化应用商店网络客户端",
            )
            .details(error)
        })
}

/// 将 reqwest 错误转换为不泄露 URL 认证信息的业务错误。
fn network_error(message: &str, error: reqwest::Error) -> AppError {
    AppError::new("APPSTORE_NETWORK_FAILED", "appstore", message).details(error)
}

/// 校验官方应用 key，避免任意路径进入远端模板 URL。
fn validate_key(value: &str) -> AppResult<()> {
    if valid_key(value) {
        Ok(())
    } else {
        Err(AppError::new(
            "VALIDATION_FAILED",
            "appstore",
            "应用 key 无效",
        ))
    }
}

/// 校验应用版本目录名称，拒绝路径穿越和 shell 控制字符。
fn valid_version(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 80
        && !value.contains("..")
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b'+'))
}

/// 校验应用 key 只能包含官方目录允许的字符。
fn valid_key(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 80
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
}

/// 校验应用安装/生命周期参数和用户确认状态。
fn validate_install(input: &InstallAppInput) -> AppResult<()> {
    if !input.confirmed {
        return Err(AppError::new(
            "CONFIRMATION_REQUIRED",
            "appstore",
            "请先确认应用安装",
        ));
    }
    validate_key(&input.key)?;
    if !valid_version(&input.version)
        || !input.install_path.starts_with('/')
        || input.install_path.contains("..")
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "appstore",
            "应用版本或安装路径无效",
        ));
    }
    validate_project(&input.project)?;
    validate_environment(&input.environment).map(|_| ())
}

/// 校验应用生命周期操作，只允许固定动作和安全的远端路径。
fn validate_action(input: &AppActionInput) -> AppResult<()> {
    if !input.confirmed {
        return Err(AppError::new(
            "CONFIRMATION_REQUIRED",
            "appstore",
            "请先确认应用操作",
        ));
    }
    validate_key(&input.key)?;
    validate_project(&input.project)?;
    if !input.install_path.starts_with('/')
        || input.install_path.contains("..")
        || input.install_path.contains('\n')
        || input.install_path.contains('\r')
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "appstore",
            "应用安装路径无效",
        ));
    }
    if !matches!(
        input.action.as_str(),
        "start" | "stop" | "restart" | "pull" | "update" | "uninstall" | "restore"
    ) {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "appstore",
            "不支持的应用操作",
        ));
    }
    Ok(())
}

/// 校验升级预览只能读取固定应用目录和官方应用 key。
fn validate_preview(input: &AppUpdatePreviewInput) -> AppResult<()> {
    validate_key(&input.key)?;
    validate_project(&input.project)?;
    validate_install_path(&input.install_path)?;
    Ok(())
}

/// 限制环境变量只能写入 1Panel 应用根目录，避免越界覆盖任意文件。
fn validate_environment_path(value: &str) -> AppResult<String> {
    if value.starts_with("/opt/1panel/apps/")
        && value.len() > "/opt/1panel/apps/".len()
        && !value.contains("..")
        && !value.contains('\n')
        && !value.contains('\r')
    {
        Ok(value.trim_end_matches('/').to_string())
    } else {
        Err(AppError::new(
            "VALIDATION_FAILED",
            "appstore",
            "应用环境变量路径无效",
        ))
    }
}

/// 校验应用健康检查路径只能指向固定的 1Panel 应用目录。
fn validate_install_path(value: &str) -> AppResult<String> {
    let value = value.trim().trim_end_matches('/');
    if !value.starts_with("/opt/1panel/apps/")
        || value.len() > 240
        || value.contains("..")
        || value.contains('\n')
        || value.contains('\r')
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "appstore",
            "应用安装目录无效",
        ));
    }
    Ok(value.to_string())
}

/// 校验 Compose 项目名称，确保不会覆盖应用目录之外的项目。
fn validate_project(value: &str) -> AppResult<String> {
    if !value.is_empty()
        && value.len() <= 63
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
    {
        Ok(value.to_string())
    } else {
        Err(AppError::new(
            "VALIDATION_FAILED",
            "appstore",
            "Compose 项目名称无效",
        ))
    }
}

/// 校验用户提供的 .env 行，禁止换行和空键，保留值中的等号。
fn validate_environment(values: &[String]) -> AppResult<String> {
    let mut lines = Vec::with_capacity(values.len());
    for value in values {
        if value.contains('\n') || value.contains('\r') || !value.contains('=') {
            return Err(AppError::new(
                "VALIDATION_FAILED",
                "appstore",
                "环境变量必须是 KEY=VALUE 行",
            ));
        }
        let key = value
            .split_once('=')
            .map(|(key, _)| key)
            .unwrap_or_default();
        if !valid_environment_key(key) {
            return Err(AppError::new(
                "VALIDATION_FAILED",
                "appstore",
                "环境变量名称无效",
            ));
        }
        lines.push(value.clone());
    }
    Ok(if lines.is_empty() {
        String::new()
    } else {
        format!("{}\n", lines.join("\n"))
    })
}

/// 校验 `.env` 键名，避免把 shell 控制字符写入远端环境文件。
fn valid_environment_key(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
}

/// 识别常见应用分类，未知应用归入“其他”而不丢弃目录。
fn category_for(key: &str) -> &'static str {
    if matches!(key, "openresty" | "nginx" | "caddy" | "apache") {
        "Web 服务器"
    } else if matches!(
        key,
        "mysql"
            | "mariadb"
            | "postgresql"
            | "redis"
            | "mongodb"
            | "oracle"
            | "clickhouse"
            | "tidb"
            | "doris"
            | "starrocks"
            | "milvus"
            | "pgvector"
            | "sqlite"
            | "neo4j"
            | "influxdb"
            | "opengauss"
            | "manticore"
    ) {
        "数据库"
    } else if matches!(
        key,
        "wordpress" | "ghost" | "halo" | "typecho" | "docmost" | "bookstack"
    ) {
        "建站"
    } else if matches!(
        key,
        "maxkb"
            | "dify"
            | "fastgpt"
            | "ollama"
            | "anythingllm"
            | "langbot"
            | "openwebui"
            | "localai"
            | "deepseek-harness"
            | "openclaw"
            | "qwenpaw"
            | "sqlbot"
            | "hermes-agent"
            | "crawlab"
    ) {
        "AI"
    } else if matches!(
        key,
        "php" | "java" | "node" | "nodejs" | "go" | "python" | "dotnet"
    ) {
        "运行环境"
    } else if matches!(key, "minio" | "nextcloud" | "seafile" | "filebrowser") {
        "云存储"
    } else if matches!(key, "dataease" | "superset" | "metabase" | "datart") {
        "BI"
    } else if matches!(key, "cordys" | "erpnext" | "suitecrm" | "o2oa") {
        "CRM"
    } else if matches!(key, "vault" | "certbot" | "fail2ban" | "crowdsec") {
        "安全"
    } else if matches!(key, "code-server" | "vscode" | "coder") {
        "开发工具"
    } else if matches!(key, "gitea" | "gitlab" | "jenkins" | "gitee") {
        "DevOps"
    } else if matches!(key, "jellyfin" | "plex" | "emby" | "navidrome") {
        "多媒体"
    } else if matches!(key, "roundcube" | "mailserver" | "poste") {
        "邮件服务"
    } else if key.contains("dos") {
        "休闲游戏"
    } else {
        "其他"
    }
}

/// 为目录中常见应用提供与 Web 端一致的描述；未命中时使用模板文案。
fn description_for(key: &str) -> &'static str {
    match key {
        "openresty" => "基于 NGINX 和 LuaJIT 的 Web 平台",
        "nginx" => "高性能 Web 服务器与反向代理",
        "apache" => "老牌开源 HTTP Web 服务器",
        "caddy" => "快速且可扩展的多平台 HTTP/1-2-3 Web 服务器，支持自动启用 HTTPS",
        "mysql" => "开源关系型数据库",
        "mariadb" => "MySQL 的社区分支关系型数据库",
        "postgresql" => "功能强大的开源关系型数据库",
        "redis" => "高性能的开源键值数据库",
        "mongodb" => "面向文档的 NoSQL 数据库",
        "wordpress" => "全球流行的博客与内容管理系统",
        "ghost" => "专业的现代博客发布平台",
        "halo" => "强大易用的开源建站工具",
        "maxkb" => "强大易用的企业级智能体平台",
        "dify" => "开源 LLM 应用开发平台",
        "fastgpt" => "基于大语言模型的知识库问答系统",
        "ollama" => "本地一键运行大语言模型",
        "anythingllm" => "开源的私有化 AI 知识库与聊天平台",
        "openwebui" => "用户友好的 AI 界面（支持 Ollama、OpenAI API 等）",
        "localai" => "免费的开源 OpenAI 替代品",
        "langbot" => "开源的 LLM 原生 IM 机器人开发平台",
        "deepseek-harness" => "DeepSeek 开源智能体开发环境",
        "openclaw" => "开源、自托管的个人 AI 助理",
        "qwenpaw" => "阿里开源的 AI 个人助理",
        "sqlbot" => "基于大模型和 RAG 的智能问数系统",
        "hermes-agent" => "Nous Research 开源的自托管 AI 智能体",
        "php" => "PHP 运行环境",
        "java" => "Java 运行环境",
        "node" | "nodejs" => "Node 运行环境",
        "go" => "Go 运行环境",
        "python" => "Python 运行环境",
        "dotnet" => ".NET 运行环境",
        "minio" => "高性能对象存储服务",
        "nextcloud" => "自托管的云盘与协作平台",
        "seafile" => "开源云盘与文件同步服务",
        "filebrowser" => "浏览器中的文件管理器",
        "dataease" => "人人可用的开源 BI 工具",
        "superset" => "开源的数据分析与可视化 BI 平台",
        "metabase" => "开源的商业智能数据平台",
        "cordys" => "开源 AI CRM 系统，是 Salesforce CRM 的开源替代",
        "erpnext" => "开源的企业资源计划 ERP 系统",
        "gitea" => "轻量级的 Git 代码托管服务",
        "gitlab" => "完整的 DevOps 代码托管平台",
        "jenkins" => "自动化构建与部署平台",
        "vault" => "安全的密钥与凭据管理中心",
        "code-server" => "浏览器中运行的 VS Code",
        "jellyfin" => "自托管的开源媒体服务器",
        "navidrome" => "自托管的音乐流媒体服务",
        "roundcube" => "基于 Web 的多语言 IMAP 邮件客户端",
        "phpmyadmin" => "MySQL 和 MariaDB 的 Web 管理工具",
        "docmost" => "开源协作 wiki 与文档软件",
        "manticore" => "一个高性能、多存储的数据库，专为搜索和分析而设计",
        "onlyoffice" | "onlyoffice-docs" => "免费的在线办公套件",
        "teamspeak" => "一款出色的网络语音 (VoIP) 解决方案",
        "cyberchef" => "浏览器中的数据编码、解码与分析工具",
        _ => "官方 1Panel 应用模板",
    }
}

/// 将目录 key 转换为可读的中文/英文混合显示名称；详情页会以官方 metadata 覆盖。
fn display_name(key: &str) -> String {
    key.split(['-', '_'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// 从 YAML 顶层读取字符串字段，无法读取时返回 None。
fn yaml_string(value: &serde_yaml::Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(serde_yaml::Value::as_str)
        .map(str::to_string)
}

/// 从 YAML 顶层读取字符串数组字段。
fn yaml_strings(value: &serde_yaml::Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(serde_yaml::Value::as_sequence)
        .map(|values| {
            values
                .iter()
                .filter_map(serde_yaml::Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// 从 YAML 嵌套对象读取字符串字段，兼容缺失 additionalProperties 的旧模板。
fn yaml_nested_string(value: &serde_yaml::Value, path: &[&str]) -> Option<String> {
    let current = path
        .iter()
        .try_fold(value, |current, key| current.get(*key))?;
    current.as_str().map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::{
        compose_command, overall_health, parse_environment_keys, parse_health, parse_installed,
        parse_update_preview, sign_mirror_catalog, valid_key, valid_version,
        validate_environment_path, validate_install_path, validate_mirror_base_url,
        validate_mirror_destination, validate_mirror_file_url, validate_mirror_key_id,
        validate_mirror_secret, validate_preview, verify_mirror_signature, AppStoreSettings,
        AppUpdatePreviewInput, SaveAppStoreSettingsInput,
    };
    use chrono::Utc;
    use secrecy::SecretString;

    #[test]
    fn parses_installed_marker() {
        let value = parse_installed("__COMPOSE__\tdocker compose\n__APP__\topenresty\t/opt/1panel/apps/openresty/1.0\t/opt/1panel/apps/openresty/1.0/docker-compose.yml\trunning\n").unwrap();
        assert!(value.compose_available);
        assert_eq!(value.apps[0].project, "1.0");
    }

    /// 多容器项目：端口来自所有容器并集，版本取首个带标签的镜像。
    #[test]
    fn unions_host_ports_across_containers() {
        let output = "__COMPOSE__\tdocker compose\n__APP__\topenresty\t/opt/1panel/apps/openresty/1.0\t/opt/1panel/apps/openresty/1.0/docker-compose.yml\trunning\topenresty/openresty:1.31.1.1-0-noble|2026-06-12T08:00:00Z|{\"80/tcp\":[{\"HostIp\":\"0.0.0.0\",\"HostPort\":\"80\"}],\"443/tcp\":[{\"HostIp\":\"0.0.0.0\",\"HostPort\":\"443\"}]};openresty/openresty:1.31.1.1-0-noble|2026-06-12T08:00:00Z|{}\n";
        let value = parse_installed(output).unwrap();
        assert_eq!(value.apps[0].version.as_deref(), Some("1.31.1.1-0-noble"));
        assert_eq!(value.apps[0].host_ports, vec!["80", "443"]);
    }

    /// 容器无端口映射（host 网络）时，回退解析 Compose 模板第 6 字段的 ports 段。
    #[test]
    fn falls_back_to_compose_template_ports() {
        let output = "__COMPOSE__\tdocker compose\n__APP__\topenresty\t/opt/1panel/apps/openresty/1.0\t/opt/1panel/apps/openresty/1.0/docker-compose.yml\trunning\topenresty/openresty:1.31.1.1-0-noble|2026-06-12T08:00:00Z|{}\t80 443\n";
        let value = parse_installed(output).unwrap();
        assert_eq!(value.apps[0].host_ports, vec!["80", "443"]);
    }

    /// host 网络 + 模板无 ports 段时，从镜像 ExposedPorts 兜底（openresty EXPOSE 80/443）。
    #[test]
    fn falls_back_to_image_exposed_ports_on_host_network() {
        let output = "__COMPOSE__\tdocker compose\n__APP__\topenresty\t/opt/1panel/apps/openresty/1.0\t/opt/1panel/apps/openresty/1.0/docker-compose.yml\trunning\topenresty/openresty:1.31.1.1-0-noble|2026-06-12T08:00:00Z|{}|host|{\"80/tcp\":{},\"443/tcp\":{}}\t\n";
        let value = parse_installed(output).unwrap();
        assert_eq!(value.apps[0].host_ports, vec!["80", "443"]);
    }

    /// 所有端口来源缺失（host 网络、模板无 ports、镜像未 EXPOSE）时，
    /// 从安装目录 `.env` 的面板端口参数兜底（openresty PANEL_APP_PORT_HTTP=80 / HTTPS=443）。
    #[test]
    fn falls_back_to_env_install_ports() {
        let output = "__COMPOSE__\tdocker compose\n__APP__\topenresty\t/opt/1panel/apps/openresty/openresty\t/opt/1panel/apps/openresty/openresty/docker-compose.yml\trunning\topenresty/openresty:1.31.1.1-0-noble|2026-06-12T08:00:00Z|{}|host|{}\t\t80 443\n";
        let value = parse_installed(output).unwrap();
        assert_eq!(value.apps[0].host_ports, vec!["80", "443"]);
    }

    /// `.env` 端口兜底排在镜像 ExposedPorts 之前：安装参数与镜像声明不一致时以安装参数为准。
    #[test]
    fn env_install_ports_win_over_exposed_ports() {
        let output = "__COMPOSE__\tdocker compose\n__APP__\topenresty\t/opt/1panel/apps/openresty/openresty\t/opt/1panel/apps/openresty/openresty/docker-compose.yml\trunning\topenresty/openresty:1.0|2026-06-12T08:00:00Z|{}|host|{\"80/tcp\":{},\"443/tcp\":{}}\t\t8080\n";
        let value = parse_installed(output).unwrap();
        assert_eq!(value.apps[0].host_ports, vec!["8080"]);
    }

    /// `.env` 不存在或没有面板端口参数时保持空端口列表，不把其他环境变量当作端口。
    #[test]
    fn missing_env_ports_keeps_ports_empty() {
        let output = "__COMPOSE__\tdocker compose\n__APP__\tdemo\t/opt/1panel/apps/demo/1.0\t/opt/1panel/apps/demo/1.0/docker-compose.yml\trunning\tdemo:1|2026-06-12T08:00:00Z|{}|bridge|{}\t\t\n";
        let value = parse_installed(output).unwrap();
        assert!(value.apps[0].host_ports.is_empty());
    }

    /// 桥接网络下不使用 ExposedPorts 兜底，避免把容器端口当作宿主端口展示。
    #[test]
    fn rejects_exposed_ports_on_bridge_network() {
        let output = "__COMPOSE__\tdocker compose\n__APP__\tdemo\t/opt/1panel/apps/demo/1.0\t/opt/1panel/apps/demo/1.0/docker-compose.yml\trunning\tdemo:1|2026-06-12T08:00:00Z|{}|bridge|{\"8080/tcp\":{}}\t\n";
        let value = parse_installed(output).unwrap();
        assert!(value.apps[0].host_ports.is_empty());
    }

    #[test]
    fn rejects_path_control_values() {
        assert!(valid_key("openresty"));
        assert!(!valid_key("../openresty"));
        assert!(valid_version("1.31.1.1-2-3-noble"));
        assert!(!valid_version("../../etc"));
    }

    #[test]
    fn builds_compose_update_command_with_each_binary() {
        let command = compose_command(
            "demo",
            "/opt/1panel/apps/demo/docker-compose.yml",
            "config -q && pull && up -d",
        )
        .unwrap();
        assert!(command.contains("docker compose -f"));
        assert!(command.contains("docker-compose -f"));
        assert!(command.contains("config -q && docker compose"));
        assert!(!command.contains("pull && up -d"));
    }

    /// 确认卸载后的恢复动作仍先校验 Compose 配置，再启动项目。
    #[test]
    fn builds_compose_restore_command() {
        let command = compose_command(
            "demo",
            "/opt/1panel/apps/demo/docker-compose.yml",
            "config -q && up -d",
        )
        .unwrap();
        assert!(command.contains("config -q && docker compose"));
        assert!(command.contains("up -d"));
    }

    #[test]
    fn parses_compose_health_markers() {
        let health =
            parse_health("__APP_HEALTH__\t/api\tghcr.io/demo/api:1\trunning\thealthy\t0\n");
        assert_eq!(health[0].name, "api");
        assert_eq!(health[0].health, "healthy");
        assert_eq!(overall_health(&health), "healthy");
    }

    #[test]
    fn validates_health_path_and_degraded_state() {
        assert!(validate_install_path("/opt/1panel/apps/demo").is_ok());
        assert!(validate_install_path("/tmp/demo").is_err());
        let health = parse_health("__APP_HEALTH__\t/api\timage\trunning\tunhealthy\t1\n");
        assert_eq!(overall_health(&health), "degraded");
    }

    #[test]
    fn masks_environment_keys_without_values() {
        let values = parse_environment_keys("__ENV__\tDB_PASSWORD\n__ENV__\tAPP_PORT\n");
        assert_eq!(values.len(), 2);
        assert_eq!(values[0].key, "DB_PASSWORD");
        assert_eq!(values[0].masked_value, "••••••");
        assert!(validate_environment_path("/opt/1panel/apps/demo").is_ok());
        assert!(validate_environment_path("/etc/nginx").is_err());
    }

    #[test]
    fn parses_update_preview_without_compose_content() {
        let preview = parse_update_preview(
            "__APP_UPDATE_PREVIEW__\toldhash\tnewhash\t10\t12\t1\n",
            "demo",
            "demo",
            "1.2.3",
        )
        .unwrap();
        assert!(preview.changed);
        assert_eq!(preview.current_hash.as_deref(), Some("oldhash"));
        assert_eq!(preview.latest_lines, 12);
        let missing = parse_update_preview(
            "__APP_UPDATE_PREVIEW__\t\tnewhash\t0\t12\tmissing\n",
            "demo",
            "demo",
            "1.2.3",
        )
        .unwrap();
        assert!(missing.current_missing);
        assert!(missing.changed);
    }

    #[test]
    fn validates_update_preview_scope() {
        let input = AppUpdatePreviewInput {
            server_id: "server".into(),
            key: "openresty".into(),
            project: "openresty".into(),
            install_path: "/opt/1panel/apps/openresty".into(),
        };
        assert!(validate_preview(&input).is_ok());
        let mut unsafe_input = input;
        unsafe_input.install_path = "/etc".into();
        assert!(validate_preview(&unsafe_input).is_err());
    }

    /// 验证官方源、镜像源和离线缓存设置的边界，避免把不安全地址持久化。
    #[test]
    fn validates_appstore_source_settings() {
        let official = super::normalize_settings(SaveAppStoreSettingsInput {
            source: "official".into(),
            mirror_base_url: None,
            mirror_base_urls: Vec::new(),
            cache_ttl_seconds: 3600,
            offline_mode: false,
            mirror_key_id: None,
            mirror_verification_secret: None,
            clear_mirror_verification_secret: false,
        })
        .unwrap();
        assert_eq!(official, super::default_settings());
        let mirror = super::normalize_settings(SaveAppStoreSettingsInput {
            source: "mirror".into(),
            mirror_base_url: Some("https://mirror.example.com/1panel/".into()),
            mirror_base_urls: Vec::new(),
            cache_ttl_seconds: 300,
            offline_mode: true,
            mirror_key_id: Some("main".into()),
            mirror_verification_secret: None,
            clear_mirror_verification_secret: false,
        })
        .unwrap();
        assert_eq!(mirror.source, "mirror");
        assert!(super::normalize_settings(SaveAppStoreSettingsInput {
            source: "mirror".into(),
            mirror_base_url: None,
            mirror_base_urls: Vec::new(),
            cache_ttl_seconds: 3600,
            offline_mode: false,
            mirror_key_id: None,
            mirror_verification_secret: None,
            clear_mirror_verification_secret: false,
        })
        .is_err());
        assert!(super::normalize_settings(SaveAppStoreSettingsInput {
            source: "official".into(),
            mirror_base_url: None,
            mirror_base_urls: Vec::new(),
            cache_ttl_seconds: 299,
            offline_mode: false,
            mirror_key_id: None,
            mirror_verification_secret: None,
            clear_mirror_verification_secret: false,
        })
        .is_err());
    }

    /// 验证多镜像配置保留顺序、去重，并兼容旧版单地址字段。
    #[test]
    fn normalizes_ordered_mirror_nodes() {
        let settings = super::normalize_settings(SaveAppStoreSettingsInput {
            source: "mirror".into(),
            mirror_base_url: Some("https://legacy.example.com/1panel".into()),
            mirror_base_urls: vec![
                "https://primary.example.com/1panel/".into(),
                "https://backup.example.com/1panel".into(),
                "https://primary.example.com/1panel".into(),
            ],
            cache_ttl_seconds: 3600,
            offline_mode: false,
            mirror_key_id: None,
            mirror_verification_secret: None,
            clear_mirror_verification_secret: false,
        })
        .unwrap();
        assert_eq!(
            settings.mirror_base_urls,
            vec![
                "https://primary.example.com/1panel",
                "https://backup.example.com/1panel"
            ]
        );
        assert_eq!(
            super::mirror_bases(&settings).unwrap(),
            settings.mirror_base_urls
        );

        let legacy = super::normalize_settings(SaveAppStoreSettingsInput {
            source: "mirror".into(),
            mirror_base_url: Some("https://legacy.example.com/1panel/".into()),
            mirror_base_urls: Vec::new(),
            cache_ttl_seconds: 3600,
            offline_mode: false,
            mirror_key_id: None,
            mirror_verification_secret: None,
            clear_mirror_verification_secret: false,
        })
        .unwrap();
        assert_eq!(
            legacy.mirror_base_urls,
            vec!["https://legacy.example.com/1panel"]
        );
    }

    /// 防止镜像节点列表无限增长，避免配置和失败转移请求被滥用。
    #[test]
    fn rejects_excessive_mirror_nodes() {
        let nodes = (0..9)
            .map(|index| format!("https://mirror-{index}.example.com/1panel"))
            .collect();
        assert!(super::normalize_settings(SaveAppStoreSettingsInput {
            source: "mirror".into(),
            mirror_base_url: None,
            mirror_base_urls: nodes,
            cache_ttl_seconds: 3600,
            offline_mode: false,
            mirror_key_id: None,
            mirror_verification_secret: None,
            clear_mirror_verification_secret: false,
        })
        .is_err());
    }

    /// 验证镜像地址拒绝凭据、查询参数和公网明文 HTTP，允许本机调试镜像。
    #[test]
    fn validates_appstore_mirror_url() {
        assert_eq!(
            validate_mirror_base_url("http://localhost:8080/repo/").unwrap(),
            "http://localhost:8080/repo"
        );
        assert!(validate_mirror_base_url("http://mirror.example.com/repo").is_err());
        assert!(validate_mirror_base_url("https://user:pass@mirror.example.com/repo").is_err());
        assert!(validate_mirror_base_url("https://mirror.example.com/repo?token=secret").is_err());
    }

    /// 验证镜像目录签名可往返校验，且篡改正文或 key ID 会被拒绝。
    #[test]
    fn signs_and_verifies_mirror_catalog() {
        let payload = br#"{"source_revision":"abc","items":[]}"#;
        let secret = SecretString::from("local-test-mirror-secret");
        let signature = sign_mirror_catalog(payload, "main", &secret).unwrap();
        assert!(verify_mirror_signature(payload, &signature, "main", &secret).is_ok());
        assert!(verify_mirror_signature(payload, &signature, "other", &secret).is_err());
        assert!(verify_mirror_signature(br#"{}"#, &signature, "main", &secret).is_err());
    }

    /// 验证镜像资源可以使用同源相对路径，但不能跳到另一主机。
    #[test]
    fn validates_relative_mirror_file_urls() {
        assert_eq!(
            validate_mirror_file_url("https://mirror.example.com/repo", "apps/demo/data.yml")
                .unwrap(),
            "https://mirror.example.com/repo/apps/demo/data.yml"
        );
        assert!(validate_mirror_file_url(
            "https://mirror.example.com/repo",
            "https://evil.example/data.yml"
        )
        .is_err());
        assert!(validate_mirror_file_url(
            "https://mirror.example.com/repo",
            "https://mirror.example.com/private/data.yml"
        )
        .is_err());
    }

    /// 验证镜像签名参数和绝对输出目录的边界，避免把任意路径写入本地。
    #[test]
    fn validates_mirror_generation_inputs() {
        let destination = std::env::temp_dir().join("onepanel-client-mirror-test");
        assert!(validate_mirror_destination(&destination.display().to_string()).is_ok());
        assert!(validate_mirror_destination("relative/mirror").is_err());
        assert_eq!(validate_mirror_key_id("main.key").unwrap(), "main.key");
        assert!(validate_mirror_key_id("bad/key").is_err());
        assert!(validate_mirror_secret(&SecretString::from("short")).is_err());
        assert!(validate_mirror_secret(&SecretString::from("long-enough-local-secret")).is_ok());
    }

    /// 确认应用商店缓存响应会显式标记缓存状态和年龄，便于用户判断数据新鲜度。
    #[test]
    fn marks_cached_appstore_responses() {
        let value = AppStoreSettings {
            source: "official".into(),
            mirror_base_url: None,
            mirror_base_urls: Vec::new(),
            cache_ttl_seconds: 3600,
            offline_mode: false,
            mirror_key_id: None,
            signature_configured: false,
        };
        assert_eq!(value.source, "official");
        let record = super::CachedRecord {
            cached_at: Utc::now() - chrono::Duration::seconds(5),
            value: super::AppCatalogSnapshot {
                repository: "repo".into(),
                branch: "dev".into(),
                source_revision: "abc".into(),
                items: Vec::new(),
                fetched_at: Utc::now(),
                cached: false,
                cache_age_seconds: None,
                signature_present: false,
                signature_verified: false,
                resolved_mirror_base_url: None,
            },
        };
        let snapshot = super::cached_catalog(record);
        assert!(snapshot.cached);
        assert!(snapshot.cache_age_seconds.unwrap_or_default() >= 4);
    }
}

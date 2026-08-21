use crate::domain::ssh::SshConnectionManager;
use crate::errors::{AppError, AppResult};
use crate::infra::local::LocalRepository;
use crate::security::CredentialStore;
use chrono::{DateTime, Utc};
use hmac::{Hmac, KeyInit, Mac};
use reqwest::{Client, StatusCode, Url};
use russh::client;
use russh::keys::ssh_key::{HashAlg, PublicKey};
use russh_sftp::client::SftpSession;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use uuid::Uuid;

const ACCOUNTS_SETTING_KEY: &str = "backup.accounts";
const CREDENTIAL_PREFIX: &str = "backup-account-";
const MAX_ACCOUNTS: usize = 100;
const MAX_ACCOUNT_NAME: usize = 80;
const MAX_REMOTE_PATH: usize = 4_096;
const MAX_UPLOAD_BYTES: u64 = 8 * 1024 * 1024 * 1024;

/// 描述一个可被计划任务选用的外部备份账号；凭据字段只以 hasSecret 摘要返回。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupAccount {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub server_id: Option<String>,
    pub endpoint: Option<String>,
    pub remote_path: String,
    pub bucket: Option<String>,
    pub region: Option<String>,
    pub username: Option<String>,
    pub private_key_path: Option<String>,
    /// 可选 SHA-256 Host Key 指纹；填写后 SFTP 连接会拒绝指纹变化。
    #[serde(default)]
    pub host_key_fingerprint: Option<String>,
    pub has_secret: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 接收账号公共参数和一次性凭据；secret 不会写入 SQLite 或普通日志。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveBackupAccountInput {
    pub id: Option<String>,
    pub name: String,
    pub kind: String,
    #[serde(default)]
    pub server_id: Option<String>,
    #[serde(default)]
    pub endpoint: Option<String>,
    #[serde(default)]
    pub remote_path: String,
    #[serde(default)]
    pub bucket: Option<String>,
    #[serde(default)]
    pub region: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub private_key_path: Option<String>,
    #[serde(default)]
    pub host_key_fingerprint: Option<String>,
    #[serde(default)]
    pub secret: Option<SecretString>,
    #[serde(default)]
    pub clear_secret: bool,
    pub confirmed: bool,
}

/// 返回账号连通性检查结果，不返回响应体或账号凭据。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupAccountTestResult {
    pub account_id: String,
    pub reachable: bool,
    pub status_code: Option<u16>,
    pub detail: String,
    pub checked_at: DateTime<Utc>,
}

/// 描述一次远程归档上传结果，供计划任务报告和本地历史展示。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupUploadResult {
    pub account_id: String,
    pub kind: String,
    pub target: String,
    pub bytes: u64,
    pub detail: String,
}

/// 接收一条由 SSH 服务器生成的备份归档并上传到本机账号后端。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadBackupInput {
    pub server_id: String,
    pub account_id: String,
    pub remote_path: String,
    pub confirmed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredAccount {
    #[serde(flatten)]
    account: BackupAccount,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccountSecret {
    value: String,
}

/// 列出本地备份账号，并仅通过密钥链探测 secret 是否存在。
pub async fn list(
    local: &LocalRepository,
    credentials: &Arc<dyn CredentialStore>,
) -> AppResult<Vec<BackupAccount>> {
    let mut accounts = load_accounts(local).await?;
    for account in &mut accounts {
        account.has_secret = account.kind != "local"
            && (account.private_key_path.is_some()
                || credentials
                    .get(&credential_key(&account.id))
                    .map(|value| !value.expose_secret().is_empty())
                    .unwrap_or(false));
    }
    accounts.sort_by_key(|account| std::cmp::Reverse(account.updated_at));
    Ok(accounts)
}

/// 校验计划任务引用的备份账号存在且允许用于指定服务器，不读取或返回 secret 内容。
pub async fn validate_selection(
    local: &LocalRepository,
    credentials: &Arc<dyn CredentialStore>,
    server_id: &str,
    ids: &[String],
) -> AppResult<()> {
    if ids.len() > 8 {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "backup_account",
            "单个计划任务最多选择 8 个备份账号",
        ));
    }
    let accounts = list(local, credentials).await?;
    for id in ids {
        validate_id(id)?;
        let account = accounts
            .iter()
            .find(|value| value.id == *id)
            .ok_or_else(|| {
                AppError::new(
                    "BACKUP_ACCOUNT_NOT_FOUND",
                    "backup_account",
                    "计划任务引用的备份账号不存在",
                )
            })?;
        if account
            .server_id
            .as_deref()
            .is_some_and(|value| value != server_id)
        {
            return Err(AppError::new(
                "VALIDATION_FAILED",
                "backup_account",
                "计划任务引用的本机目录账号未绑定当前服务器",
            ));
        }
        if account.kind != "local" && !account.has_secret {
            return Err(AppError::new(
                "BACKUP_ACCOUNT_SECRET_MISSING",
                "backup_account",
                "计划任务引用的外部备份账号缺少 secret",
            ));
        }
    }
    Ok(())
}

/// 保存或更新一个账号；公共元数据写入 SQLite，secret 只写入操作系统密钥链。
pub async fn save(
    local: &LocalRepository,
    credentials: &Arc<dyn CredentialStore>,
    input: SaveBackupAccountInput,
) -> AppResult<BackupAccount> {
    if !input.confirmed {
        return Err(AppError::new(
            "CONFIRMATION_REQUIRED",
            "backup_account",
            "请先确认备份账号配置会保存到本机",
        ));
    }
    validate_input(&input)?;
    let mut accounts = load_accounts(local).await?;
    let id = input
        .id
        .clone()
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    validate_id(&id)?;
    let now = Utc::now();
    let previous = accounts.iter().find(|account| account.id == id).cloned();
    if previous.is_none() && accounts.len() >= MAX_ACCOUNTS {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "backup_account",
            "备份账号数量超过上限",
        ));
    }
    let secret_ref = credential_key(&id);
    if input.kind == "local" {
        credentials.delete(&secret_ref)?;
    } else if let Some(secret) = input.secret.as_ref() {
        let value = secret.expose_secret().trim();
        if value.is_empty() {
            return Err(AppError::new(
                "VALIDATION_FAILED",
                "backup_account",
                "备份账号 secret 不能为空",
            ));
        }
        let encoded = serde_json::to_string(&AccountSecret {
            value: value.to_string(),
        })
        .map_err(AppError::database)?;
        credentials.put(&secret_ref, SecretString::from(encoded))?;
    } else if input.clear_secret {
        credentials.delete(&secret_ref)?;
    } else {
        let has_existing_secret = credentials
            .get(&secret_ref)
            .map(|value| !value.expose_secret().is_empty())
            .unwrap_or(false);
        let has_private_key = input.kind == "sftp"
            && input
                .private_key_path
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty());
        if !has_existing_secret && !has_private_key {
            return Err(AppError::new(
                "VALIDATION_FAILED",
                "backup_account",
                if input.kind == "sftp" {
                    "SFTP 密码认证必须提供 secret"
                } else {
                    "新建外部备份账号必须提供 secret"
                },
            ));
        }
    }
    let account = BackupAccount {
        id: id.clone(),
        name: input.name.trim().to_string(),
        kind: input.kind.trim().to_string(),
        server_id: input
            .server_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        endpoint: input
            .endpoint
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        remote_path: input.remote_path.trim().to_string(),
        bucket: input
            .bucket
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        region: input
            .region
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        username: input
            .username
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        private_key_path: input
            .private_key_path
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        host_key_fingerprint: input
            .host_key_fingerprint
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        has_secret: input.kind != "local",
        created_at: previous
            .as_ref()
            .map(|value| value.created_at)
            .unwrap_or(now),
        updated_at: now,
    };
    if let Some(existing) = accounts.iter_mut().find(|value| value.id == id) {
        *existing = account.clone();
    } else {
        accounts.push(account.clone());
    }
    save_accounts(local, &accounts).await?;
    list(local, credentials)
        .await?
        .into_iter()
        .find(|value| value.id == account.id)
        .ok_or_else(|| {
            AppError::new(
                "BACKUP_ACCOUNT_FAILED",
                "backup_account",
                "保存账号后读取失败",
            )
        })
}

/// 删除备份账号和对应的密钥链条目，不会删除远程或本机已存在的归档文件。
pub async fn delete(
    local: &LocalRepository,
    credentials: &Arc<dyn CredentialStore>,
    id: &str,
    confirmed: bool,
) -> AppResult<()> {
    if !confirmed {
        return Err(AppError::new(
            "CONFIRMATION_REQUIRED",
            "backup_account",
            "请先确认删除备份账号",
        ));
    }
    validate_id(id)?;
    let mut accounts = load_accounts(local).await?;
    let before = accounts.len();
    accounts.retain(|value| value.id != id);
    if accounts.len() == before {
        return Err(AppError::new(
            "BACKUP_ACCOUNT_NOT_FOUND",
            "backup_account",
            "备份账号不存在",
        ));
    }
    credentials.delete(&credential_key(id))?;
    save_accounts(local, &accounts).await
}

/// 对账号执行只读连通性探测；本机目录仅检查父目录，不自动创建用户文件。
pub async fn test(
    local: &LocalRepository,
    credentials: &Arc<dyn CredentialStore>,
    id: &str,
) -> AppResult<BackupAccountTestResult> {
    validate_id(id)?;
    let account = list(local, credentials)
        .await?
        .into_iter()
        .find(|value| value.id == id)
        .ok_or_else(|| {
            AppError::new(
                "BACKUP_ACCOUNT_NOT_FOUND",
                "backup_account",
                "备份账号不存在",
            )
        })?;
    validate_account(&account)?;
    let checked_at = Utc::now();
    match account.kind.as_str() {
        "local" => {
            let path = PathBuf::from(&account.remote_path);
            let parent = path.parent().unwrap_or_else(|| Path::new("/"));
            let metadata = tokio::fs::metadata(parent).await;
            Ok(BackupAccountTestResult {
                account_id: id.to_string(),
                reachable: metadata.is_ok(),
                status_code: None,
                detail: if metadata.is_ok() {
                    "本机目标目录可访问".into()
                } else {
                    "本机目标目录不可访问".into()
                },
                checked_at,
            })
        }
        "webdav" => test_webdav(&account, credentials, checked_at).await,
        "s3" => test_s3(&account, credentials, checked_at).await,
        "sftp" => test_sftp(&account, credentials, checked_at).await,
        _ => Err(AppError::new(
            "VALIDATION_FAILED",
            "backup_account",
            "备份账号类型无效",
        )),
    }
}

/// 从远端 SSH 服务器下载一份归档并上传到选定的本机备份账号。
pub async fn upload(
    ssh: &SshConnectionManager,
    local: &LocalRepository,
    credentials: &Arc<dyn CredentialStore>,
    input: UploadBackupInput,
) -> AppResult<BackupUploadResult> {
    if !input.confirmed {
        return Err(AppError::new(
            "CONFIRMATION_REQUIRED",
            "backup_account",
            "请先确认归档上传会读取远端文件并写入备份账号",
        ));
    }
    validate_id(&input.account_id)?;
    validate_remote_artifact_path(&input.remote_path)?;
    let account = list(local, credentials)
        .await?
        .into_iter()
        .find(|value| value.id == input.account_id)
        .ok_or_else(|| {
            AppError::new(
                "BACKUP_ACCOUNT_NOT_FOUND",
                "backup_account",
                "备份账号不存在",
            )
        })?;
    if account
        .server_id
        .as_deref()
        .is_some_and(|id| id != input.server_id)
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "backup_account",
            "本机目录备份账号未绑定当前服务器",
        ));
    }
    validate_account(&account)?;
    let file_name = remote_artifact_file_name(&input.remote_path)?;
    let temporary =
        std::env::temp_dir().join(format!("1panel-client-upload-{}.part", Uuid::new_v4()));
    let bytes =
        download_remote_artifact(ssh, &input.server_id, &input.remote_path, &temporary).await?;
    let result = match account.kind.as_str() {
        "local" => upload_local(&account, &temporary, &file_name, bytes).await,
        "webdav" => upload_webdav(&account, credentials, &temporary, &file_name, bytes).await,
        "s3" => upload_s3(&account, credentials, &temporary, &file_name, bytes).await,
        "sftp" => upload_sftp(&account, credentials, &temporary, &file_name, bytes).await,
        _ => Err(AppError::new(
            "VALIDATION_FAILED",
            "backup_account",
            "备份账号类型无效",
        )),
    };
    let _ = tokio::fs::remove_file(&temporary).await;
    result.map(|mut value| {
        value.account_id = account.id;
        value.kind = account.kind;
        value
    })
}

/// 从远程 SFTP 读取归档到本机临时文件，完成后由调用方负责清理。
async fn download_remote_artifact(
    ssh: &SshConnectionManager,
    server_id: &str,
    remote_path: &str,
    local_path: &Path,
) -> AppResult<u64> {
    let sftp = ssh.open_sftp(server_id).await?;
    let metadata = sftp.symlink_metadata(remote_path).await.map_err(|error| {
        AppError::new("SFTP_FAILED", "backup_account", "无法读取远端归档信息")
            .details(error)
            .for_server(server_id)
    })?;
    let size = metadata.len();
    if size > MAX_UPLOAD_BYTES {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "backup_account",
            "归档超过上传大小上限",
        ));
    }
    let mut source = sftp.open(remote_path).await.map_err(|error| {
        AppError::new("SFTP_FAILED", "backup_account", "无法打开远端归档")
            .details(error)
            .for_server(server_id)
    })?;
    let part = local_path.with_extension("part");
    let mut target = tokio::fs::File::create(&part).await.map_err(|error| {
        AppError::new(
            "LOCAL_FILE_FAILED",
            "backup_account",
            "无法创建本机上传临时文件",
        )
        .details(error)
    })?;
    let mut buffer = vec![0_u8; 128 * 1024];
    let mut written = 0_u64;
    loop {
        let count = source.read(&mut buffer).await.map_err(|error| {
            AppError::new("SFTP_FAILED", "backup_account", "读取远端归档失败").details(error)
        })?;
        if count == 0 {
            break;
        }
        written = written.saturating_add(count as u64);
        if written > MAX_UPLOAD_BYTES {
            return Err(AppError::new(
                "VALIDATION_FAILED",
                "backup_account",
                "归档超过上传大小上限",
            ));
        }
        target.write_all(&buffer[..count]).await.map_err(|error| {
            AppError::new(
                "LOCAL_FILE_FAILED",
                "backup_account",
                "写入本机临时归档失败",
            )
            .details(error)
        })?;
    }
    target.flush().await.map_err(|error| {
        AppError::new(
            "LOCAL_FILE_FAILED",
            "backup_account",
            "刷新本机临时归档失败",
        )
        .details(error)
    })?;
    drop(target);
    tokio::fs::rename(&part, local_path)
        .await
        .map_err(|error| {
            AppError::new(
                "LOCAL_FILE_FAILED",
                "backup_account",
                "完成本机临时归档失败",
            )
            .details(error)
        })?;
    let _ = sftp.close().await;
    Ok(written)
}

/// 将归档原子复制到本机目录账号，避免留下半成品。
async fn upload_local(
    account: &BackupAccount,
    source: &Path,
    file_name: &str,
    bytes: u64,
) -> AppResult<BackupUploadResult> {
    let target = PathBuf::from(&account.remote_path).join(file_name);
    let parent = target
        .parent()
        .ok_or_else(|| AppError::new("VALIDATION_FAILED", "backup_account", "本机备份目录无效"))?;
    tokio::fs::create_dir_all(parent).await.map_err(|error| {
        AppError::new(
            "LOCAL_FILE_FAILED",
            "backup_account",
            "无法创建本机备份目录",
        )
        .details(error)
    })?;
    let part = target.with_extension("upload.part");
    tokio::fs::copy(source, &part).await.map_err(|error| {
        AppError::new(
            "LOCAL_FILE_FAILED",
            "backup_account",
            "复制备份到本机目录失败",
        )
        .details(error)
    })?;
    tokio::fs::rename(&part, &target).await.map_err(|error| {
        AppError::new(
            "LOCAL_FILE_FAILED",
            "backup_account",
            "完成本机备份替换失败",
        )
        .details(error)
    })?;
    Ok(BackupUploadResult {
        account_id: account.id.clone(),
        kind: account.kind.clone(),
        target: target.display().to_string(),
        bytes,
        detail: "归档已原子写入本机备份目录".into(),
    })
}

/// 上传归档到 WebDAV，使用 Basic Auth 和同名临时资源后 MOVE 替换。
async fn upload_webdav(
    account: &BackupAccount,
    credentials: &Arc<dyn CredentialStore>,
    source: &Path,
    file_name: &str,
    bytes: u64,
) -> AppResult<BackupUploadResult> {
    let secret = read_secret(credentials, account)?;
    let target = account_url(account, file_name)?;
    let temporary = format!("{target}.1panel-client-{}.part", Uuid::new_v4());
    let file = tokio::fs::File::open(source).await.map_err(|error| {
        AppError::new("LOCAL_FILE_FAILED", "backup_account", "无法打开待上传归档").details(error)
    })?;
    let client = Client::builder()
        .timeout(Duration::from_secs(300))
        .build()
        .map_err(|error| {
            AppError::new(
                "BACKUP_ACCOUNT_FAILED",
                "backup_account",
                "无法初始化 WebDAV 客户端",
            )
            .details(error)
        })?;
    let response = client
        .put(&temporary)
        .basic_auth(
            account.username.as_deref().unwrap_or_default(),
            Some(secret.clone()),
        )
        .header(reqwest::header::CONTENT_LENGTH, bytes)
        .body(reqwest::Body::wrap_stream(
            tokio_util::io::ReaderStream::new(file),
        ))
        .send()
        .await
        .map_err(|error| {
            AppError::new("BACKUP_UPLOAD_FAILED", "backup_account", "WebDAV 上传失败")
                .details(error)
        })?;
    if !response.status().is_success() {
        return Err(AppError::new(
            "BACKUP_UPLOAD_FAILED",
            "backup_account",
            "WebDAV 临时上传返回失败状态",
        )
        .details(response.status()));
    }
    let move_response = client
        .request(
            reqwest::Method::from_bytes(b"MOVE").expect("MOVE method"),
            &temporary,
        )
        .header("Destination", &target)
        .header("Overwrite", "T")
        .basic_auth(
            account.username.as_deref().unwrap_or_default(),
            Some(secret),
        )
        .send()
        .await
        .map_err(|error| {
            AppError::new(
                "BACKUP_UPLOAD_FAILED",
                "backup_account",
                "WebDAV 原子替换失败",
            )
            .details(error)
        })?;
    if !move_response.status().is_success() {
        return Err(AppError::new(
            "BACKUP_UPLOAD_FAILED",
            "backup_account",
            "WebDAV 原子替换返回失败状态",
        )
        .details(move_response.status()));
    }
    Ok(BackupUploadResult {
        account_id: account.id.clone(),
        kind: account.kind.clone(),
        target,
        bytes,
        detail: "归档已上传到 WebDAV".into(),
    })
}

/// 上传归档到 S3 兼容对象存储，并使用 AWS Signature V4 签名保护请求。
async fn upload_s3(
    account: &BackupAccount,
    credentials: &Arc<dyn CredentialStore>,
    source: &Path,
    file_name: &str,
    bytes: u64,
) -> AppResult<BackupUploadResult> {
    let secret = read_secret(credentials, account)?;
    let access_key = account.username.as_deref().ok_or_else(|| {
        AppError::new(
            "VALIDATION_FAILED",
            "backup_account",
            "S3 账号缺少 access key",
        )
    })?;
    let region = account.region.as_deref().unwrap_or("us-east-1");
    let target = account_url(account, file_name)?;
    let hash = sha256_file(source).await?;
    let now = Utc::now();
    let headers = signed_headers("PUT", &target, region, access_key, &secret, now, &hash)?;
    let client = Client::builder()
        .timeout(Duration::from_secs(300))
        .build()
        .map_err(|error| {
            AppError::new(
                "BACKUP_ACCOUNT_FAILED",
                "backup_account",
                "无法初始化 S3 客户端",
            )
            .details(error)
        })?;
    let file = tokio::fs::File::open(source).await.map_err(|error| {
        AppError::new("LOCAL_FILE_FAILED", "backup_account", "无法打开待上传归档").details(error)
    })?;
    let mut request = client.put(&target).body(reqwest::Body::wrap_stream(
        tokio_util::io::ReaderStream::new(file),
    ));
    for (key, value) in headers {
        request = request.header(key, value);
    }
    let response = request.send().await.map_err(|error| {
        AppError::new("BACKUP_UPLOAD_FAILED", "backup_account", "S3 上传失败").details(error)
    })?;
    if !response.status().is_success() {
        return Err(AppError::new(
            "BACKUP_UPLOAD_FAILED",
            "backup_account",
            "S3 上传返回失败状态",
        )
        .details(response.status()));
    }
    Ok(BackupUploadResult {
        account_id: account.id.clone(),
        kind: account.kind.clone(),
        target,
        bytes,
        detail: "归档已上传到 S3 兼容对象存储".into(),
    })
}

/// 连接独立的 SFTP 账号；密码或私钥均从系统密钥链/本机路径读取，绝不拼接进 Shell 命令。
async fn open_external_sftp(
    account: &BackupAccount,
    credentials: &Arc<dyn CredentialStore>,
) -> AppResult<(client::Handle<ExternalSftpHandler>, SftpSession)> {
    let endpoint = account.endpoint.as_deref().ok_or_else(|| {
        AppError::new(
            "VALIDATION_FAILED",
            "backup_account",
            "SFTP 账号缺少 endpoint",
        )
    })?;
    let parsed = Url::parse(endpoint)
        .map_err(|_| AppError::new("VALIDATION_FAILED", "backup_account", "SFTP endpoint 无效"))?;
    let host = parsed
        .host_str()
        .ok_or_else(|| {
            AppError::new(
                "VALIDATION_FAILED",
                "backup_account",
                "SFTP endpoint 缺少主机",
            )
        })?
        .to_string();
    let port = parsed.port().unwrap_or(22);
    let username = account.username.as_deref().ok_or_else(|| {
        AppError::new("VALIDATION_FAILED", "backup_account", "SFTP 账号缺少用户名")
    })?;
    let secret = read_optional_secret(credentials, account).unwrap_or_default();
    let config = client::Config {
        inactivity_timeout: None,
        keepalive_interval: Some(Duration::from_secs(30)),
        keepalive_max: 3,
        ..Default::default()
    };
    let handler = ExternalSftpHandler {
        expected_fingerprint: account.host_key_fingerprint.clone(),
    };
    let connection = tokio::time::timeout(
        Duration::from_secs(20),
        client::connect(Arc::new(config), (host.as_str(), port), handler),
    )
    .await
    .map_err(|_| AppError::new("NETWORK_TIMEOUT", "backup_account", "SFTP 连接超时"))?
    .map_err(|error| {
        AppError::new(
            "SFTP_CONNECT_FAILED",
            "backup_account",
            "无法连接 SFTP 备份账号",
        )
        .details(error)
    })?;
    let mut handle = connection;
    let authenticated = if let Some(path) = account.private_key_path.as_deref() {
        let key =
            russh::keys::load_secret_key(path, (!secret.is_empty()).then_some(secret.as_str()))
                .map_err(|error| {
                    AppError::new("SFTP_AUTH_FAILED", "backup_account", "无法读取 SFTP 私钥")
                        .details(error)
                })?;
        let hash_algorithm = handle
            .best_supported_rsa_hash()
            .await
            .map_err(|error| {
                AppError::new(
                    "SFTP_AUTH_FAILED",
                    "backup_account",
                    "无法协商 SFTP RSA 算法",
                )
                .details(error)
            })?
            .flatten();
        let key = russh::keys::PrivateKeyWithHashAlg::new(Arc::new(key), hash_algorithm);
        handle
            .authenticate_publickey(username, key)
            .await
            .map_err(|error| {
                AppError::new("SFTP_AUTH_FAILED", "backup_account", "SFTP 私钥认证失败")
                    .details(error)
            })?
            .success()
    } else {
        if secret.is_empty() {
            return Err(AppError::new(
                "SFTP_AUTH_FAILED",
                "backup_account",
                "SFTP 密码认证缺少 secret",
            ));
        }
        handle
            .authenticate_password(username, &secret)
            .await
            .map_err(|error| {
                AppError::new("SFTP_AUTH_FAILED", "backup_account", "SFTP 密码认证失败")
                    .details(error)
            })?
            .success()
    };
    if !authenticated {
        return Err(AppError::new(
            "SFTP_AUTH_FAILED",
            "backup_account",
            "SFTP 服务器拒绝认证",
        ));
    }
    let channel = handle.channel_open_session().await.map_err(|error| {
        AppError::new("SFTP_FAILED", "backup_account", "无法创建 SFTP channel").details(error)
    })?;
    channel
        .request_subsystem(true, "sftp")
        .await
        .map_err(|error| {
            AppError::new("SFTP_FAILED", "backup_account", "远端拒绝启动 SFTP 子系统")
                .details(error)
        })?;
    let session = SftpSession::new(channel.into_stream())
        .await
        .map_err(|error| {
            AppError::new("SFTP_FAILED", "backup_account", "SFTP 协议初始化失败").details(error)
        })?;
    session.set_timeout(30);
    Ok((handle, session))
}

/// 对 SFTP 账号执行真实连接、认证和根路径探测，返回不含响应体的结果。
async fn test_sftp(
    account: &BackupAccount,
    credentials: &Arc<dyn CredentialStore>,
    checked_at: DateTime<Utc>,
) -> AppResult<BackupAccountTestResult> {
    let (handle, session) = open_external_sftp(account, credentials).await?;
    let parent = sftp_parent_path(&account.remote_path);
    let probe = session.try_exists(&parent).await;
    let _ = session.close().await;
    let _ = handle
        .disconnect(
            russh::Disconnect::ByApplication,
            "SFTP probe complete",
            "en",
        )
        .await;
    let reachable = probe.unwrap_or(false);
    Ok(BackupAccountTestResult {
        account_id: account.id.clone(),
        reachable,
        status_code: None,
        detail: if reachable {
            "SFTP 连接、认证和目标目录检查成功".into()
        } else {
            "SFTP 已连接但目标目录不可访问".into()
        },
        checked_at,
    })
}

/// 将本机临时归档通过独立 SFTP 会话原子上传到目标路径。
async fn upload_sftp(
    account: &BackupAccount,
    credentials: &Arc<dyn CredentialStore>,
    source: &Path,
    file_name: &str,
    bytes: u64,
) -> AppResult<BackupUploadResult> {
    let (handle, session) = open_external_sftp(account, credentials).await?;
    let target = sftp_target_path(account, file_name);
    let temporary = format!("{target}.1panel-client-{}.part", Uuid::new_v4());
    let result = async {
        ensure_sftp_parent(&session, &target).await?;
        let mut remote = session.create(&temporary).await.map_err(|error| {
            AppError::new(
                "BACKUP_UPLOAD_FAILED",
                "backup_account",
                "无法创建 SFTP 临时归档",
            )
            .details(error)
        })?;
        let mut local_file = tokio::fs::File::open(source).await.map_err(|error| {
            AppError::new("LOCAL_FILE_FAILED", "backup_account", "无法打开待上传归档")
                .details(error)
        })?;
        tokio::io::copy(&mut local_file, &mut remote)
            .await
            .map_err(|error| {
                AppError::new(
                    "BACKUP_UPLOAD_FAILED",
                    "backup_account",
                    "SFTP 写入归档失败",
                )
                .details(error)
            })?;
        remote.shutdown().await.map_err(|error| {
            AppError::new(
                "BACKUP_UPLOAD_FAILED",
                "backup_account",
                "SFTP 归档关闭失败",
            )
            .details(error)
        })?;
        session.rename(&temporary, &target).await.map_err(|error| {
            AppError::new(
                "BACKUP_UPLOAD_FAILED",
                "backup_account",
                "SFTP 原子替换失败",
            )
            .details(error)
        })?;
        Ok::<(), AppError>(())
    }
    .await;
    if result.is_err() {
        let _ = session.remove_file(&temporary).await;
    }
    let _ = session.close().await;
    let _ = handle
        .disconnect(
            russh::Disconnect::ByApplication,
            "SFTP upload complete",
            "en",
        )
        .await;
    result?;
    Ok(BackupUploadResult {
        account_id: account.id.clone(),
        kind: account.kind.clone(),
        target,
        bytes,
        detail: "归档已通过 SFTP 原子上传".into(),
    })
}

/// 递归创建 SFTP 目标的父目录；父级跳转已在账号校验阶段拒绝。
async fn ensure_sftp_parent(session: &SftpSession, target: &str) -> AppResult<()> {
    let Some((parent, _)) = target.rsplit_once('/') else {
        return Ok(());
    };
    let mut current = if parent.starts_with('/') {
        "/".to_string()
    } else {
        String::new()
    };
    for component in parent
        .split('/')
        .filter(|value| !value.is_empty() && *value != ".")
    {
        if !current.is_empty() && !current.ends_with('/') {
            current.push('/');
        }
        current.push_str(component);
        if !session.try_exists(&current).await.map_err(|error| {
            AppError::new("SFTP_FAILED", "backup_account", "无法检查 SFTP 目标目录").details(error)
        })? {
            session.create_dir(&current).await.map_err(|error| {
                AppError::new("SFTP_FAILED", "backup_account", "无法创建 SFTP 目标目录")
                    .details(error)
            })?;
        }
    }
    Ok(())
}

/// 生成 SFTP 父目录；空前缀表示登录用户的当前目录。
fn sftp_parent_path(prefix: &str) -> String {
    let trimmed = prefix.trim_end_matches('/');
    if trimmed.is_empty() {
        ".".into()
    } else {
        trimmed.into()
    }
}

/// 拼接 SFTP 目标文件名，同时保留用户输入的绝对/相对路径语义。
fn sftp_target_path(account: &BackupAccount, file_name: &str) -> String {
    let prefix = account.remote_path.trim_end_matches('/');
    if prefix.is_empty() {
        file_name.to_string()
    } else {
        format!("{prefix}/{file_name}")
    }
}

/// 校验 SFTP 主机 Host Key；未配置指纹时保留兼容模式，配置后拒绝任何变化。
#[derive(Clone)]
struct ExternalSftpHandler {
    expected_fingerprint: Option<String>,
}

impl client::Handler for ExternalSftpHandler {
    type Error = russh::Error;

    async fn check_server_key(&mut self, public_key: &PublicKey) -> Result<bool, Self::Error> {
        let observed = public_key.fingerprint(HashAlg::Sha256).to_string();
        Ok(self
            .expected_fingerprint
            .as_deref()
            .map(|expected| expected == observed)
            .unwrap_or(true))
    }
}

/// 使用 HTTP HEAD 检查 WebDAV 端点并隐藏响应体。
async fn test_webdav(
    account: &BackupAccount,
    credentials: &Arc<dyn CredentialStore>,
    checked_at: DateTime<Utc>,
) -> AppResult<BackupAccountTestResult> {
    let secret = read_secret(credentials, account)?;
    let url = account_url(account, ".1panel-client-healthcheck")?;
    let response = Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|error| {
            AppError::new(
                "BACKUP_ACCOUNT_FAILED",
                "backup_account",
                "无法初始化 WebDAV 客户端",
            )
            .details(error)
        })?
        .head(url)
        .basic_auth(
            account.username.as_deref().unwrap_or_default(),
            Some(secret),
        )
        .send()
        .await
        .map_err(|error| {
            AppError::new(
                "BACKUP_ACCOUNT_FAILED",
                "backup_account",
                "WebDAV 连通性检查失败",
            )
            .details(error)
        })?;
    let status = response.status();
    Ok(BackupAccountTestResult {
        account_id: account.id.clone(),
        reachable: status.is_success()
            || status == StatusCode::NOT_FOUND
            || status == StatusCode::METHOD_NOT_ALLOWED,
        status_code: Some(status.as_u16()),
        detail: format!("WebDAV 返回 HTTP {}", status.as_u16()),
        checked_at,
    })
}

/// 使用签名 HEAD 检查 S3 bucket 端点和凭据。
async fn test_s3(
    account: &BackupAccount,
    credentials: &Arc<dyn CredentialStore>,
    checked_at: DateTime<Utc>,
) -> AppResult<BackupAccountTestResult> {
    let secret = read_secret(credentials, account)?;
    let access_key = account.username.as_deref().ok_or_else(|| {
        AppError::new(
            "VALIDATION_FAILED",
            "backup_account",
            "S3 账号缺少 access key",
        )
    })?;
    let region = account.region.as_deref().unwrap_or("us-east-1");
    let target = account_url(account, ".1panel-client-healthcheck")?;
    let hash = hex_sha256(b"");
    let headers = signed_headers(
        "HEAD",
        &target,
        region,
        access_key,
        &secret,
        Utc::now(),
        &hash,
    )?;
    let client = Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|error| {
            AppError::new(
                "BACKUP_ACCOUNT_FAILED",
                "backup_account",
                "无法初始化 S3 客户端",
            )
            .details(error)
        })?;
    let mut request = client.head(target);
    for (key, value) in headers {
        request = request.header(key, value);
    }
    let response = request.send().await.map_err(|error| {
        AppError::new(
            "BACKUP_ACCOUNT_FAILED",
            "backup_account",
            "S3 连通性检查失败",
        )
        .details(error)
    })?;
    let status = response.status();
    Ok(BackupAccountTestResult {
        account_id: account.id.clone(),
        reachable: status.is_success() || status == StatusCode::NOT_FOUND,
        status_code: Some(status.as_u16()),
        detail: format!("S3 返回 HTTP {}", status.as_u16()),
        checked_at,
    })
}

/// 构造账号归档 URL；路径只接受固定安全字符，避免把账号配置当作任意 URL 代理。
fn account_url(account: &BackupAccount, file_name: &str) -> AppResult<String> {
    let endpoint = account.endpoint.as_deref().ok_or_else(|| {
        AppError::new(
            "VALIDATION_FAILED",
            "backup_account",
            "外部账号缺少 endpoint",
        )
    })?;
    let mut base = endpoint.trim_end_matches('/').to_string();
    if account.kind == "s3" {
        let bucket = account.bucket.as_deref().ok_or_else(|| {
            AppError::new("VALIDATION_FAILED", "backup_account", "S3 账号缺少 bucket")
        })?;
        base.push('/');
        base.push_str(bucket);
    }
    let prefix = account.remote_path.trim_matches('/');
    if !prefix.is_empty() {
        base.push('/');
        base.push_str(prefix);
    }
    base.push('/');
    base.push_str(file_name);
    let parsed = Url::parse(&base)
        .map_err(|_| AppError::new("VALIDATION_FAILED", "backup_account", "账号目标 URL 无效"))?;
    if !matches!(parsed.scheme(), "http" | "https")
        || parsed.host_str().is_none()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "backup_account",
            "账号目标 URL 必须是无查询参数的 HTTP(S) 地址",
        ));
    }
    Ok(parsed.to_string())
}

/// 生成 AWS Signature V4 所需的规范请求头。
fn signed_headers(
    method: &str,
    target: &str,
    region: &str,
    access_key: &str,
    secret: &str,
    now: DateTime<Utc>,
    payload_hash: &str,
) -> AppResult<Vec<(String, String)>> {
    let url = Url::parse(target)
        .map_err(|_| AppError::new("VALIDATION_FAILED", "backup_account", "S3 目标 URL 无效"))?;
    let host = url.host_str().ok_or_else(|| {
        AppError::new(
            "VALIDATION_FAILED",
            "backup_account",
            "S3 endpoint 缺少主机",
        )
    })?;
    let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
    let short_date = now.format("%Y%m%d").to_string();
    let canonical_uri = if url.path().is_empty() {
        "/"
    } else {
        url.path()
    };
    let canonical_headers =
        format!("host:{host}\nx-amz-content-sha256:{payload_hash}\nx-amz-date:{amz_date}\n");
    let signed_names = "host;x-amz-content-sha256;x-amz-date";
    let canonical_request =
        format!("{method}\n{canonical_uri}\n\n{canonical_headers}\n{signed_names}\n{payload_hash}");
    let request_hash = hex_sha256(canonical_request.as_bytes());
    let scope = format!("{short_date}/{region}/s3/aws4_request");
    let string_to_sign = format!("AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{request_hash}");
    let signing_key = derive_signing_key(secret, &short_date, region);
    let signature = hex_encode(&hmac_bytes(&signing_key, string_to_sign.as_bytes()));
    let authorization = format!("AWS4-HMAC-SHA256 Credential={access_key}/{scope}, SignedHeaders={signed_names}, Signature={signature}");
    Ok(vec![
        ("Host".into(), host.into()),
        ("x-amz-content-sha256".into(), payload_hash.into()),
        ("x-amz-date".into(), amz_date),
        ("Authorization".into(), authorization),
    ])
}

/// 派生 AWS Signature V4 的四段 HMAC signing key。
fn derive_signing_key(secret: &str, date: &str, region: &str) -> Vec<u8> {
    let k_date = hmac_bytes(format!("AWS4{secret}").as_bytes(), date.as_bytes());
    let k_region = hmac_bytes(&k_date, region.as_bytes());
    let k_service = hmac_bytes(&k_region, b"s3");
    hmac_bytes(&k_service, b"aws4_request")
}

/// 计算 HMAC-SHA256，供签名和测试复用。
fn hmac_bytes(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC accepts arbitrary key length");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

/// 将 SHA-256 摘要编码为小写十六进制字符串。
fn hex_sha256(value: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value);
    hex_encode(&hasher.finalize())
}

/// 流式计算本机归档 SHA-256，避免把大文件一次性读入桌面客户端内存。
async fn sha256_file(path: &Path) -> AppResult<String> {
    let mut file = tokio::fs::File::open(path).await.map_err(|error| {
        AppError::new("LOCAL_FILE_FAILED", "backup_account", "读取待上传归档失败").details(error)
    })?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 128 * 1024];
    loop {
        let count = file.read(&mut buffer).await.map_err(|error| {
            AppError::new("LOCAL_FILE_FAILED", "backup_account", "计算归档摘要失败").details(error)
        })?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(hex_encode(&hasher.finalize()))
}

/// 将固定字节摘要转换为小写十六进制，不引入额外编码依赖。
fn hex_encode(value: &[u8]) -> String {
    value.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// 读取账号密钥链 secret 并解析出上传密码/secret key。
fn read_secret(
    credentials: &Arc<dyn CredentialStore>,
    account: &BackupAccount,
) -> AppResult<String> {
    let value = credentials.get(&credential_key(&account.id)).map_err(|_| {
        AppError::new(
            "BACKUP_ACCOUNT_SECRET_MISSING",
            "backup_account",
            "备份账号 secret 不可用，请重新保存账号",
        )
    })?;
    let secret: AccountSecret =
        serde_json::from_str(value.expose_secret()).map_err(AppError::database)?;
    if secret.value.trim().is_empty() {
        return Err(AppError::new(
            "BACKUP_ACCOUNT_SECRET_MISSING",
            "backup_account",
            "备份账号 secret 为空",
        ));
    }
    Ok(secret.value)
}

/// 读取可选的 SFTP 凭据；私钥无口令时允许密钥链中没有密码条目。
fn read_optional_secret(
    credentials: &Arc<dyn CredentialStore>,
    account: &BackupAccount,
) -> Option<String> {
    credentials
        .get(&credential_key(&account.id))
        .ok()
        .and_then(|value| serde_json::from_str::<AccountSecret>(value.expose_secret()).ok())
        .map(|secret| secret.value)
        .filter(|value| !value.trim().is_empty())
}

/// 读取本地账号元数据，不接受超过大小上限或损坏的设置。
async fn load_accounts(local: &LocalRepository) -> AppResult<Vec<BackupAccount>> {
    let Some(value) = local.get_setting(ACCOUNTS_SETTING_KEY).await? else {
        return Ok(Vec::new());
    };
    if value.len() > 2 * 1024 * 1024 {
        return Err(AppError::new(
            "BACKUP_ACCOUNT_FAILED",
            "backup_account",
            "备份账号设置超过本地上限",
        ));
    }
    let accounts: Vec<StoredAccount> = serde_json::from_str(&value).map_err(|_| {
        AppError::new(
            "BACKUP_ACCOUNT_FAILED",
            "backup_account",
            "备份账号设置无法解析",
        )
    })?;
    if accounts.len() > MAX_ACCOUNTS {
        return Err(AppError::new(
            "BACKUP_ACCOUNT_FAILED",
            "backup_account",
            "备份账号数量超过上限",
        ));
    }
    Ok(accounts.into_iter().map(|entry| entry.account).collect())
}

/// 保存本地账号元数据，不把任何密钥链内容混入 JSON。
async fn save_accounts(local: &LocalRepository, accounts: &[BackupAccount]) -> AppResult<()> {
    let value = serde_json::to_string(accounts).map_err(AppError::database)?;
    local.set_setting(ACCOUNTS_SETTING_KEY, &value).await
}

/// 校验账号输入字段和各后端必需参数。
fn validate_input(input: &SaveBackupAccountInput) -> AppResult<()> {
    let kind = input.kind.trim();
    if !matches!(kind, "local" | "webdav" | "s3" | "sftp") {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "backup_account",
            "不支持的备份账号类型",
        ));
    }
    let name = input.name.trim();
    if name.is_empty()
        || name.chars().count() > MAX_ACCOUNT_NAME
        || name.chars().any(|value| value.is_control())
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "backup_account",
            "备份账号名称无效",
        ));
    }
    validate_remote_path(&input.remote_path, kind == "local")?;
    if kind == "local" {
        if input
            .server_id
            .as_deref()
            .unwrap_or_default()
            .trim()
            .is_empty()
        {
            return Err(AppError::new(
                "VALIDATION_FAILED",
                "backup_account",
                "本机目录账号必须绑定服务器",
            ));
        }
        if input.endpoint.is_some()
            || input.bucket.is_some()
            || input.username.is_some()
            || input.host_key_fingerprint.is_some()
        {
            return Err(AppError::new(
                "VALIDATION_FAILED",
                "backup_account",
                "本机目录账号不应填写外部连接参数",
            ));
        }
        return Ok(());
    }
    if kind != "sftp" && input.host_key_fingerprint.is_some() {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "backup_account",
            "只有 SFTP 账号支持 Host Key 指纹",
        ));
    }
    if let Some(fingerprint) = input.host_key_fingerprint.as_deref() {
        validate_host_key_fingerprint(fingerprint)?;
    }
    let endpoint = input.endpoint.as_deref().unwrap_or_default();
    let parsed = Url::parse(endpoint).map_err(|_| {
        AppError::new(
            "VALIDATION_FAILED",
            "backup_account",
            "外部账号 endpoint 无效",
        )
    })?;
    let valid_scheme = match kind {
        "sftp" => parsed.scheme() == "sftp",
        _ => matches!(parsed.scheme(), "http" | "https"),
    };
    if !valid_scheme
        || parsed.host_str().is_none()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "backup_account",
            "外部账号 endpoint 必须是无查询参数的受支持 URL",
        ));
    }
    if input
        .username
        .as_deref()
        .unwrap_or_default()
        .trim()
        .is_empty()
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "backup_account",
            "外部账号必须填写用户名或 access key",
        ));
    }
    if kind == "s3"
        && input
            .bucket
            .as_deref()
            .unwrap_or_default()
            .trim()
            .is_empty()
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "backup_account",
            "S3 账号必须填写 bucket",
        ));
    }
    if let Some(region) = input.region.as_deref() {
        if region.len() > 64
            || region
                .bytes()
                .any(|byte| !(byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')))
        {
            return Err(AppError::new(
                "VALIDATION_FAILED",
                "backup_account",
                "S3 region 无效",
            ));
        }
    }
    Ok(())
}

/// 校验可选的 SHA-256 Host Key 指纹，避免把任意文本当成信任锚点。
fn validate_host_key_fingerprint(value: &str) -> AppResult<()> {
    if value.len() < 16
        || value.len() > 128
        || !value.starts_with("SHA256:")
        || value
            .chars()
            .any(|character| character.is_whitespace() || character.is_control())
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "backup_account",
            "SFTP Host Key 指纹必须是 SHA256: 格式",
        ));
    }
    Ok(())
}

/// 校验已经持久化的账号公共字段，防止旧版本或手工数据库修改绕过约束。
fn validate_account(account: &BackupAccount) -> AppResult<()> {
    let input = SaveBackupAccountInput {
        id: Some(account.id.clone()),
        name: account.name.clone(),
        kind: account.kind.clone(),
        server_id: account.server_id.clone(),
        endpoint: account.endpoint.clone(),
        remote_path: account.remote_path.clone(),
        bucket: account.bucket.clone(),
        region: account.region.clone(),
        username: account.username.clone(),
        private_key_path: account.private_key_path.clone(),
        host_key_fingerprint: account.host_key_fingerprint.clone(),
        secret: None,
        clear_secret: false,
        confirmed: true,
    };
    validate_input(&input)
}

/// 校验账号路径；本机目录接受当前平台和 POSIX 绝对路径，外部对象前缀只允许安全字符。
fn validate_remote_path(value: &str, local: bool) -> AppResult<()> {
    if value.len() > MAX_REMOTE_PATH
        || value
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
        || value.contains("..")
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "backup_account",
            "备份账号路径无效",
        ));
    }
    if local && !is_local_absolute_path(value) {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "backup_account",
            "本机目录必须是绝对路径",
        ));
    }
    if !local
        && value.bytes().any(|byte| {
            !(byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'.' | b'_' | b'-'))
        })
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "backup_account",
            "外部账号路径包含不支持的字符",
        ));
    }
    Ok(())
}

/// 判断桌面客户端上的本机目录是否为绝对路径，兼容 Windows 盘符和 POSIX 路径。
fn is_local_absolute_path(value: &str) -> bool {
    Path::new(value).is_absolute()
        || value.starts_with('/')
        || (value.len() >= 3
            && value.as_bytes()[0].is_ascii_alphabetic()
            && value.as_bytes()[1] == b':'
            && matches!(value.as_bytes()[2], b'\\' | b'/'))
}

/// 校验用于上传的远端归档路径，禁止目录、父级跳转和控制字符。
fn validate_remote_artifact_path(value: &str) -> AppResult<()> {
    if !value.starts_with('/')
        || value.len() > MAX_REMOTE_PATH
        || value.ends_with('/')
        || value.contains("..")
        || value
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "backup_account",
            "远端归档路径无效",
        ));
    }
    Ok(())
}

/// 提取并校验归档文件名，避免把远端路径中的分隔符或特殊字符带入外部目标。
fn remote_artifact_file_name(value: &str) -> AppResult<String> {
    let name = value.rsplit('/').next().unwrap_or_default();
    if name.is_empty()
        || name.len() > 255
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "backup_account",
            "远端归档文件名包含不支持的字符",
        ));
    }
    Ok(name.to_string())
}

/// 生成稳定的系统密钥链引用，不将 endpoint 或 secret 写入引用名。
fn credential_key(id: &str) -> String {
    format!("{CREDENTIAL_PREFIX}{id}")
}

/// 校验账号 ID 只接受本客户端生成的 UUID。
fn validate_id(value: &str) -> AppResult<()> {
    Uuid::parse_str(value)
        .map(|_| ())
        .map_err(|_| AppError::new("VALIDATION_FAILED", "backup_account", "备份账号 ID 无效"))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{
        account_url, delete, derive_signing_key, hex_sha256, remote_artifact_file_name, save,
        sftp_target_path, upload, validate_remote_path, BackupAccount, SaveBackupAccountInput,
        UploadBackupInput,
    };
    use crate::domain::ssh::{ConnectOutcome, TrustHostKeyInput};
    use crate::infra::db::ServerRepository;
    use crate::infra::local::LocalRepository;
    use crate::security::{CredentialStore, OsCredentialStore};
    use chrono::Utc;
    use secrecy::ExposeSecret;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

    #[test]
    fn validates_account_path_boundaries() {
        assert!(validate_remote_path("/var/backups", true).is_ok());
        assert!(validate_remote_path(r"C:\Backups", true).is_ok());
        assert!(validate_remote_path("relative", true).is_err());
        assert!(validate_remote_path("../escape", false).is_err());
        assert!(validate_remote_path("prefix/site-1", false).is_ok());
        assert_eq!(
            remote_artifact_file_name("/var/backups/site-20260821.tar.gz").unwrap(),
            "site-20260821.tar.gz"
        );
        assert!(remote_artifact_file_name("/var/backups/site;rm.tar.gz").is_err());
    }

    #[test]
    fn builds_s3_style_target_and_signing_key() {
        let account = BackupAccount {
            id: "id".into(),
            name: "s3".into(),
            kind: "s3".into(),
            server_id: None,
            endpoint: Some("https://s3.example.test".into()),
            remote_path: "client".into(),
            bucket: Some("bucket".into()),
            region: Some("us-east-1".into()),
            username: Some("access".into()),
            private_key_path: None,
            host_key_fingerprint: None,
            has_secret: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        assert_eq!(
            account_url(&account, "archive.tar.gz").expect("url"),
            "https://s3.example.test/bucket/client/archive.tar.gz"
        );
        assert_eq!(
            hex_sha256(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            derive_signing_key("secret", "20260821", "us-east-1").len(),
            32
        );
        assert_eq!(
            sftp_target_path(&account, "archive.tar.gz"),
            "client/archive.tar.gz"
        );
    }

    /// 在用户显式提供本机应用数据库和服务器 ID 时，验证外部 SFTP 账号真实认证、上传、原子替换和清理闭环。
    #[tokio::test]
    #[ignore = "需要用户已授权的真实测试节点环境变量"]
    async fn real_sftp_backup_account_round_trip() -> crate::errors::AppResult<()> {
        let db_path = std::env::var("ONEPANEL_CLIENT_DB").map_err(|_| {
            crate::errors::AppError::new(
                "TEST_ENV_MISSING",
                "backup_account",
                "缺少本机测试数据库路径",
            )
        })?;
        let server_id = std::env::var("ONEPANEL_CLIENT_SERVER_ID").map_err(|_| {
            crate::errors::AppError::new("TEST_ENV_MISSING", "backup_account", "缺少测试服务器 ID")
        })?;
        let options = SqliteConnectOptions::new().filename(db_path);
        let pool = SqlitePoolOptions::new()
            .max_connections(3)
            .connect_with(options)
            .await
            .map_err(crate::errors::AppError::database)?;
        let credentials: Arc<dyn CredentialStore> =
            Arc::new(OsCredentialStore::new("com.agentless.servermanager"));
        let local = LocalRepository::new(pool.clone());
        let servers = ServerRepository::new(pool, credentials.clone());
        let ssh = crate::domain::ssh::SshConnectionManager::new(servers.clone());
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
        let record = servers.record(&server_id).await?;
        let password_ref = record.password_secret_ref.as_deref().ok_or_else(|| {
            crate::errors::AppError::new(
                "TEST_ENV_MISSING",
                "backup_account",
                "测试节点不是密码认证",
            )
        })?;
        let password = servers.credential(password_ref)?;
        let suffix = uuid::Uuid::new_v4();
        let remote_base = format!("/tmp/1panel-client-sftp-account-{suffix}");
        let remote_source = format!("{remote_base}/source/source.tar.gz");
        let remote_target = format!("{remote_base}/target/source.tar.gz");
        let account_id = uuid::Uuid::new_v4().to_string();
        let result = async {
            let prepared = ssh
                .execute_system(
                    &server_id,
                    &format!(
                        "set -eu; mkdir -p -- {}/source; printf '%s\\n' sftp-smoke > {}",
                        crate::security::shell_escape(&remote_base),
                        crate::security::shell_escape(&remote_source),
                    ),
                    std::time::Duration::from_secs(30),
                )
                .await?;
            if prepared.exit_code != 0 {
                return Err(crate::errors::AppError::new(
                    "TEST_REMOTE_PREPARE_FAILED",
                    "backup_account",
                    "远端 SFTP 测试文件准备失败",
                ));
            }
            save(
                &local,
                &credentials,
                SaveBackupAccountInput {
                    id: Some(account_id.clone()),
                    name: "SFTP smoke".into(),
                    kind: "sftp".into(),
                    server_id: None,
                    endpoint: Some(format!("sftp://{}:{}", record.host, record.port)),
                    remote_path: format!("{remote_base}/target"),
                    bucket: None,
                    region: None,
                    username: Some(record.username.clone()),
                    private_key_path: None,
                    host_key_fingerprint: None,
                    secret: Some(secrecy::SecretString::from(
                        password.expose_secret().to_string(),
                    )),
                    clear_secret: false,
                    confirmed: true,
                },
            )
            .await?;
            let uploaded = upload(
                &ssh,
                &local,
                &credentials,
                UploadBackupInput {
                    server_id: server_id.clone(),
                    account_id: account_id.clone(),
                    remote_path: remote_source.clone(),
                    confirmed: true,
                },
            )
            .await?;
            assert!(uploaded.target.ends_with("source.tar.gz"));
            let verified = ssh
                .execute_system(
                    &server_id,
                    &format!(
                        "test -s {} && cmp -s {} {}",
                        crate::security::shell_escape(&remote_target),
                        crate::security::shell_escape(&remote_source),
                        crate::security::shell_escape(&remote_target),
                    ),
                    std::time::Duration::from_secs(30),
                )
                .await?;
            if verified.exit_code != 0 {
                return Err(crate::errors::AppError::new(
                    "TEST_REMOTE_VERIFY_FAILED",
                    "backup_account",
                    "SFTP 归档校验失败",
                ));
            }
            Ok::<(), crate::errors::AppError>(())
        }
        .await;
        let _ = delete(&local, &credentials, &account_id, true).await;
        let _ = ssh
            .execute_system(
                &server_id,
                &format!("rm -rf -- {}", crate::security::shell_escape(&remote_base)),
                std::time::Duration::from_secs(30),
            )
            .await;
        let _ = ssh.disconnect(&server_id).await;
        result
    }
}

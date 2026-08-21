use crate::domain::ssh::SshConnectionManager;
use crate::errors::{AppError, AppResult};
use crate::security::shell_escape;
use dashmap::DashMap;
use russh_sftp::client::SftpSession;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::ipc::Channel;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const BUFFER_SIZE: usize = 128 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "event", content = "data")]
pub enum TransferEvent {
    Started {
        transfer_id: String,
        total_bytes: Option<u64>,
    },
    Progress {
        transfer_id: String,
        transferred_bytes: u64,
        total_bytes: Option<u64>,
        bytes_per_second: u64,
        current_path: String,
    },
    Completed {
        transfer_id: String,
        transferred_bytes: u64,
    },
    Cancelled {
        transfer_id: String,
        transferred_bytes: u64,
    },
}

#[derive(Clone, Default)]
pub struct TransferManager {
    cancellations: Arc<DashMap<String, Arc<AtomicBool>>>,
}

impl TransferManager {
    pub fn begin(&self, transfer_id: &str) -> AppResult<TransferGuard> {
        if transfer_id.is_empty() || self.cancellations.contains_key(transfer_id) {
            return Err(AppError::new(
                "TRANSFER_ID_INVALID",
                "transfer",
                "传输任务标识无效或已存在",
            ));
        }
        let cancellation = Arc::new(AtomicBool::new(false));
        self.cancellations
            .insert(transfer_id.to_string(), cancellation.clone());
        Ok(TransferGuard {
            transfer_id: transfer_id.to_string(),
            cancellation,
            manager: self.clone(),
        })
    }

    pub fn cancel(&self, transfer_id: &str) -> AppResult<()> {
        let cancellation = self.cancellations.get(transfer_id).ok_or_else(|| {
            AppError::new("TRANSFER_NOT_FOUND", "transfer", "传输任务不存在或已结束")
        })?;
        cancellation.store(true, Ordering::Release);
        Ok(())
    }
}

pub struct TransferGuard {
    transfer_id: String,
    cancellation: Arc<AtomicBool>,
    manager: TransferManager,
}

impl TransferGuard {
    fn cancelled(&self) -> bool {
        self.cancellation.load(Ordering::Acquire)
    }
}

impl Drop for TransferGuard {
    fn drop(&mut self) {
        self.manager.cancellations.remove(&self.transfer_id);
    }
}

/// 上传本地文件或文件夹到远程目录，并按冲突策略完成原子替换。
#[allow(clippy::too_many_arguments)]
pub async fn upload(
    manager: &TransferManager,
    ssh: &SshConnectionManager,
    transfer_id: &str,
    server_id: &str,
    local_path: &str,
    remote_directory: &str,
    conflict: &str,
    events: &Channel<TransferEvent>,
) -> AppResult<()> {
    validate_conflict(conflict)?;
    let guard = manager.begin(transfer_id)?;
    let local_root = PathBuf::from(local_path);
    let metadata = tokio::fs::metadata(&local_root).await.map_err(|error| {
        AppError::new("LOCAL_FILE_FAILED", "file", "无法读取本地上传对象").details(error)
    })?;
    let total = if metadata.is_file() {
        Some(metadata.len())
    } else {
        None
    };
    send(
        events,
        TransferEvent::Started {
            transfer_id: transfer_id.to_string(),
            total_bytes: total,
        },
    )?;
    let sftp = ssh.open_sftp(server_id).await?;
    let name = local_root
        .file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .ok_or_else(|| AppError::new("INVALID_PATH", "validation", "本地路径没有文件名"))?;
    let remote_root = remote_join(remote_directory, &name);
    let mut transferred = 0;
    let started = Instant::now();
    if metadata.is_dir() {
        upload_directory(
            &guard,
            ssh,
            &sftp,
            server_id,
            &local_root,
            &remote_root,
            conflict,
            events,
            &mut transferred,
            started,
        )
        .await?;
    } else {
        upload_file(
            &guard,
            ssh,
            &sftp,
            server_id,
            &local_root,
            &remote_root,
            conflict,
            events,
            &mut transferred,
            total,
            started,
        )
        .await?;
    }
    let _ = sftp.close().await;
    if guard.cancelled() {
        send(
            events,
            TransferEvent::Cancelled {
                transfer_id: transfer_id.to_string(),
                transferred_bytes: transferred,
            },
        )?;
        return Err(AppError::new("CANCELLED", "transfer", "传输已取消"));
    }
    send(
        events,
        TransferEvent::Completed {
            transfer_id: transfer_id.to_string(),
            transferred_bytes: transferred,
        },
    )
}

#[allow(clippy::too_many_arguments)]
async fn upload_directory(
    guard: &TransferGuard,
    ssh: &SshConnectionManager,
    sftp: &SftpSession,
    server_id: &str,
    local_root: &Path,
    remote_root: &str,
    conflict: &str,
    events: &Channel<TransferEvent>,
    transferred: &mut u64,
    started: Instant,
) -> AppResult<()> {
    ensure_remote_dir(sftp, server_id, remote_root).await?;
    let mut pending = vec![(local_root.to_path_buf(), remote_root.to_string())];
    while let Some((local_directory, remote_directory)) = pending.pop() {
        let mut entries = tokio::fs::read_dir(&local_directory)
            .await
            .map_err(|error| {
                AppError::new("LOCAL_FILE_FAILED", "file", "无法读取本地文件夹").details(error)
            })?;
        while let Some(entry) = entries.next_entry().await.map_err(|error| {
            AppError::new("LOCAL_FILE_FAILED", "file", "遍历本地文件夹失败").details(error)
        })? {
            if guard.cancelled() {
                return Ok(());
            }
            let file_type = entry.file_type().await.map_err(|error| {
                AppError::new("LOCAL_FILE_FAILED", "file", "无法读取本地对象类型").details(error)
            })?;
            let remote_path = remote_join(
                &remote_directory,
                entry.file_name().to_string_lossy().as_ref(),
            );
            if file_type.is_dir() {
                ensure_remote_dir(sftp, server_id, &remote_path).await?;
                pending.push((entry.path(), remote_path));
            } else if file_type.is_file() {
                let size = entry.metadata().await.ok().map(|value| value.len());
                upload_file(
                    guard,
                    ssh,
                    sftp,
                    server_id,
                    &entry.path(),
                    &remote_path,
                    conflict,
                    events,
                    transferred,
                    size,
                    started,
                )
                .await?;
            }
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn upload_file(
    guard: &TransferGuard,
    ssh: &SshConnectionManager,
    sftp: &SftpSession,
    server_id: &str,
    local_path: &Path,
    remote_path: &str,
    conflict: &str,
    events: &Channel<TransferEvent>,
    transferred: &mut u64,
    total: Option<u64>,
    started: Instant,
) -> AppResult<()> {
    let Some(remote_path) = resolve_conflict(sftp, remote_path, conflict).await? else {
        return Ok(());
    };
    let temporary = format!("{remote_path}.relay-{}.part", uuid::Uuid::new_v4());
    let mut source = tokio::fs::File::open(local_path).await.map_err(|error| {
        AppError::new("LOCAL_FILE_FAILED", "file", "无法打开本地文件").details(error)
    })?;
    let mut target = sftp.create(&temporary).await.map_err(|error| {
        AppError::new("SFTP_FAILED", "sftp", "无法创建远程传输临时文件")
            .details(error)
            .for_server(server_id)
    })?;
    let mut buffer = vec![0; BUFFER_SIZE];
    loop {
        if guard.cancelled() {
            drop(target);
            let _ = sftp.remove_file(&temporary).await;
            return Ok(());
        }
        let count = source.read(&mut buffer).await.map_err(|error| {
            AppError::new("LOCAL_FILE_FAILED", "file", "读取本地上传文件失败").details(error)
        })?;
        if count == 0 {
            break;
        }
        target.write_all(&buffer[..count]).await.map_err(|error| {
            AppError::new("SFTP_FAILED", "sftp", "远程上传写入失败")
                .details(error)
                .for_server(server_id)
        })?;
        *transferred += count as u64;
        progress(events, guard, *transferred, total, started, &remote_path)?;
    }
    target.flush().await.map_err(|error| {
        AppError::new("SFTP_FAILED", "sftp", "远程上传刷新失败")
            .details(error)
            .for_server(server_id)
    })?;
    target.sync_all().await.map_err(|error| {
        AppError::new("SFTP_FAILED", "sftp", "远程上传同步失败")
            .details(error)
            .for_server(server_id)
    })?;
    drop(target);
    let result = ssh
        .execute(
            server_id,
            &format!(
                "mv -f -- {} {}",
                shell_escape(&temporary),
                shell_escape(&remote_path)
            ),
            Duration::from_secs(30),
        )
        .await?;
    if result.exit_code != 0 {
        let _ = sftp.remove_file(&temporary).await;
        return Err(
            AppError::new("REMOTE_COMMAND_FAILED", "file", "完成远程上传失败")
                .details(result.stderr)
                .for_server(server_id),
        );
    }
    Ok(())
}

/// 校验上传冲突策略，默认行为由前端显式选择而不是隐藏在 Rust 命令中。
fn validate_conflict(conflict: &str) -> AppResult<()> {
    if matches!(conflict, "replace" | "skip" | "rename") {
        Ok(())
    } else {
        Err(AppError::new(
            "VALIDATION_FAILED",
            "validation",
            "文件冲突策略无效",
        ))
    }
}

/// 根据冲突策略决定覆盖、跳过或生成不冲突的远程文件名。
async fn resolve_conflict(
    sftp: &SftpSession,
    remote_path: &str,
    conflict: &str,
) -> AppResult<Option<String>> {
    let exists = sftp.symlink_metadata(remote_path).await.is_ok();
    if !exists || conflict == "replace" {
        return Ok(Some(remote_path.to_string()));
    }
    if conflict == "skip" {
        return Ok(None);
    }
    let (stem, extension) = remote_path
        .rsplit_once('.')
        .map(|(stem, extension)| (stem.to_string(), format!(".{extension}")))
        .unwrap_or_else(|| (remote_path.to_string(), String::new()));
    for index in 1..=999 {
        let candidate = format!("{stem} ({index}){extension}");
        if sftp.symlink_metadata(&candidate).await.is_err() {
            return Ok(Some(candidate));
        }
    }
    Err(AppError::new(
        "CONFLICT_UNRESOLVED",
        "file",
        "无法为冲突文件生成新名称",
    ))
}

pub async fn download(
    manager: &TransferManager,
    ssh: &SshConnectionManager,
    transfer_id: &str,
    server_id: &str,
    remote_path: &str,
    local_directory: &str,
    events: &Channel<TransferEvent>,
) -> AppResult<()> {
    let guard = manager.begin(transfer_id)?;
    let sftp = ssh.open_sftp(server_id).await?;
    let metadata = sftp.symlink_metadata(remote_path).await.map_err(|error| {
        AppError::new("SFTP_FAILED", "sftp", "无法读取下载对象信息")
            .details(error)
            .for_server(server_id)
    })?;
    let total = (!metadata.is_dir()).then(|| metadata.len());
    send(
        events,
        TransferEvent::Started {
            transfer_id: transfer_id.to_string(),
            total_bytes: total,
        },
    )?;
    let name = remote_path
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::new("INVALID_PATH", "validation", "不能直接下载远程根目录"))?;
    let local_root = PathBuf::from(local_directory).join(name);
    let mut transferred = 0;
    let started = Instant::now();
    if metadata.is_dir() {
        download_directory(
            &guard,
            &sftp,
            server_id,
            remote_path,
            &local_root,
            events,
            &mut transferred,
            started,
        )
        .await?;
    } else {
        download_file(
            &guard,
            &sftp,
            server_id,
            remote_path,
            &local_root,
            events,
            &mut transferred,
            total,
            started,
        )
        .await?;
    }
    let _ = sftp.close().await;
    let final_event = if guard.cancelled() {
        TransferEvent::Cancelled {
            transfer_id: transfer_id.to_string(),
            transferred_bytes: transferred,
        }
    } else {
        TransferEvent::Completed {
            transfer_id: transfer_id.to_string(),
            transferred_bytes: transferred,
        }
    };
    send(events, final_event)?;
    if guard.cancelled() {
        Err(AppError::new("CANCELLED", "transfer", "传输已取消"))
    } else {
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
async fn download_directory(
    guard: &TransferGuard,
    sftp: &SftpSession,
    server_id: &str,
    remote_root: &str,
    local_root: &Path,
    events: &Channel<TransferEvent>,
    transferred: &mut u64,
    started: Instant,
) -> AppResult<()> {
    tokio::fs::create_dir_all(local_root)
        .await
        .map_err(|error| {
            AppError::new("LOCAL_FILE_FAILED", "file", "无法创建本地下载文件夹").details(error)
        })?;
    let mut pending = vec![(remote_root.to_string(), local_root.to_path_buf())];
    while let Some((remote_directory, local_directory)) = pending.pop() {
        let entries = sftp.read_dir(remote_directory).await.map_err(|error| {
            AppError::new("SFTP_FAILED", "sftp", "遍历远程下载文件夹失败")
                .details(error)
                .for_server(server_id)
        })?;
        for entry in entries {
            if guard.cancelled() {
                return Ok(());
            }
            let local_path = local_directory.join(entry.file_name());
            if entry.metadata().is_dir() {
                tokio::fs::create_dir_all(&local_path)
                    .await
                    .map_err(|error| {
                        AppError::new("LOCAL_FILE_FAILED", "file", "无法创建本地子文件夹")
                            .details(error)
                    })?;
                pending.push((entry.path(), local_path));
            } else if entry.metadata().is_regular() {
                let size = Some(entry.metadata().len());
                download_file(
                    guard,
                    sftp,
                    server_id,
                    &entry.path(),
                    &local_path,
                    events,
                    transferred,
                    size,
                    started,
                )
                .await?;
            }
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn download_file(
    guard: &TransferGuard,
    sftp: &SftpSession,
    server_id: &str,
    remote_path: &str,
    local_path: &Path,
    events: &Channel<TransferEvent>,
    transferred: &mut u64,
    total: Option<u64>,
    started: Instant,
) -> AppResult<()> {
    let part_path = local_path.with_extension(format!(
        "{}relay-part",
        local_path
            .extension()
            .map(|value| format!("{}.", value.to_string_lossy()))
            .unwrap_or_default()
    ));
    let mut source = sftp.open(remote_path).await.map_err(|error| {
        AppError::new("SFTP_FAILED", "sftp", "无法打开远程下载文件")
            .details(error)
            .for_server(server_id)
    })?;
    let mut target = tokio::fs::File::create(&part_path).await.map_err(|error| {
        AppError::new("LOCAL_FILE_FAILED", "file", "无法创建本地下载临时文件").details(error)
    })?;
    let mut buffer = vec![0; BUFFER_SIZE];
    loop {
        if guard.cancelled() {
            drop(target);
            let _ = tokio::fs::remove_file(&part_path).await;
            return Ok(());
        }
        let count = source.read(&mut buffer).await.map_err(|error| {
            AppError::new("SFTP_FAILED", "sftp", "远程下载读取失败")
                .details(error)
                .for_server(server_id)
        })?;
        if count == 0 {
            break;
        }
        target.write_all(&buffer[..count]).await.map_err(|error| {
            AppError::new("LOCAL_FILE_FAILED", "file", "本地下载写入失败").details(error)
        })?;
        *transferred += count as u64;
        progress(events, guard, *transferred, total, started, remote_path)?;
    }
    target.flush().await.map_err(|error| {
        AppError::new("LOCAL_FILE_FAILED", "file", "本地下载刷新失败").details(error)
    })?;
    drop(target);
    tokio::fs::rename(&part_path, local_path)
        .await
        .map_err(|error| {
            AppError::new("LOCAL_FILE_FAILED", "file", "完成本地下载失败").details(error)
        })?;
    Ok(())
}

async fn ensure_remote_dir(sftp: &SftpSession, server_id: &str, path: &str) -> AppResult<()> {
    let exists = sftp.try_exists(path).await.map_err(|error| {
        AppError::new("SFTP_FAILED", "sftp", "无法检查远程文件夹")
            .details(error)
            .for_server(server_id)
    })?;
    if !exists {
        sftp.create_dir(path).await.map_err(|error| {
            AppError::new("SFTP_FAILED", "sftp", "创建远程上传文件夹失败")
                .details(error)
                .for_server(server_id)
        })?;
    }
    Ok(())
}

fn progress(
    events: &Channel<TransferEvent>,
    guard: &TransferGuard,
    transferred: u64,
    total: Option<u64>,
    started: Instant,
    path: &str,
) -> AppResult<()> {
    let seconds = started.elapsed().as_secs_f64().max(0.001);
    send(
        events,
        TransferEvent::Progress {
            transfer_id: guard.transfer_id.clone(),
            transferred_bytes: transferred,
            total_bytes: total,
            bytes_per_second: (transferred as f64 / seconds) as u64,
            current_path: path.to_string(),
        },
    )
}

fn send(events: &Channel<TransferEvent>, event: TransferEvent) -> AppResult<()> {
    events.send(event).map_err(|error| {
        AppError::new("TRANSFER_CHANNEL_CLOSED", "transfer", "传输进度通道已关闭").details(error)
    })
}

fn remote_join(parent: &str, name: &str) -> String {
    if parent.ends_with('/') {
        format!("{parent}{name}")
    } else {
        format!("{parent}/{name}")
    }
}

#[cfg(test)]
mod tests {
    use super::remote_join;

    #[test]
    fn joins_posix_remote_paths() {
        assert_eq!(remote_join("/tmp", "a.txt"), "/tmp/a.txt");
        assert_eq!(remote_join("/", "a.txt"), "/a.txt");
    }
}

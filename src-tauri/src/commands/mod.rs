use crate::app::AppState;
use crate::domain::files::{DirectoryListing, RemoteTextFile, SaveTextInput};
use crate::domain::metrics::SystemOverview;
use crate::domain::server::{
    diagnose_proxy_jump_topology, SaveServerInput, ServerProfile, TopologyIssue,
};
use crate::domain::shortcuts::{SaveShortcutInput, ShortcutRecord};
use crate::domain::ssh::{ConnectOutcome, ConnectionSnapshot, TrustHostKeyInput};
use crate::errors::AppResult;
use crate::infra::local::{MetricSample, SaveTaskInput, TaskRecord};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tauri::{ipc::Channel, State};

const OVERVIEW_MEMO_KEY_PREFIX: &str = "overview.memo.";

/// Represents the non-sensitive local memo shown on one server's Overview page.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OverviewMemo {
    pub content: String,
    pub updated_at: Option<DateTime<Utc>>,
}

/// Carries the server-scoped memo text from the Overview editor to SQLite.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveOverviewMemoInput {
    pub server_id: String,
    pub content: String,
}

/// Builds a bounded settings key for a server Overview memo.
fn overview_memo_key(server_id: &str) -> String {
    format!("{OVERVIEW_MEMO_KEY_PREFIX}{server_id}")
}

/// Validates server identifiers before using them in local setting keys.
fn validate_overview_server_id(server_id: &str) -> AppResult<()> {
    if server_id.is_empty()
        || server_id.len() > 80
        || !server_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(crate::errors::AppError::new(
            "VALIDATION_FAILED",
            "overview",
            "服务器 ID 无效",
        ));
    }
    Ok(())
}

/// Rejects hidden control characters while allowing normal memo line breaks and tabs.
fn validate_overview_memo_content(content: &str) -> AppResult<()> {
    if content.chars().count() > 4000
        || content
            .chars()
            .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
    {
        return Err(crate::errors::AppError::new(
            "VALIDATION_FAILED",
            "overview",
            "备忘录内容无效或超过 4000 字符",
        ));
    }
    Ok(())
}

/// 尽力写入本地审计记录；审计失败不会掩盖已经完成的远端动作，但会写入应用日志。
async fn write_audit(
    state: &AppState,
    server_id: Option<&str>,
    action: &str,
    resource_type: &str,
    resource_id: Option<&str>,
    result: &str,
    summary: String,
) {
    if let Err(error) = state
        .servers
        .record_audit(
            server_id,
            action,
            resource_type,
            resource_id,
            result,
            &summary,
        )
        .await
    {
        tracing::warn!(error = %error, action, "写入本地审计记录失败");
    }
}

/// 将命令结果转换为成功/失败审计事件，避免业务错误被审计写入错误遮蔽。
async fn audit_outcome<T>(
    state: &AppState,
    server_id: Option<&str>,
    action: &str,
    resource_type: &str,
    resource_id: Option<&str>,
    outcome: &AppResult<T>,
    success_summary: String,
) {
    match outcome {
        Ok(_) => {
            write_audit(
                state,
                server_id,
                action,
                resource_type,
                resource_id,
                "success",
                success_summary,
            )
            .await
        }
        Err(error) => {
            write_audit(
                state,
                server_id,
                action,
                resource_type,
                resource_id,
                "failed",
                format!("{}：{}", success_summary, error.code),
            )
            .await
        }
    }
}

#[tauri::command]
pub async fn list_servers(state: State<'_, AppState>) -> AppResult<Vec<ServerProfile>> {
    state.servers.list().await
}

/// 对全部服务器档案执行 ProxyJump 拓扑批量诊断，不访问任何远端。
#[tauri::command]
pub async fn diagnose_server_topology(state: State<'_, AppState>) -> AppResult<Vec<TopologyIssue>> {
    let servers = state.servers.list().await?;
    Ok(diagnose_proxy_jump_topology(&servers))
}

/// Lists enabled global shortcuts plus enabled server-specific overrides for terminal completion.
#[tauri::command]
pub async fn list_shortcuts(
    server_id: Option<String>,
    state: State<'_, AppState>,
) -> AppResult<Vec<ShortcutRecord>> {
    state.local.list_shortcuts(server_id.as_deref()).await
}

/// Creates or updates a local terminal shortcut without sending anything to a remote server.
#[tauri::command]
pub async fn save_shortcut(
    input: SaveShortcutInput,
    state: State<'_, AppState>,
) -> AppResult<ShortcutRecord> {
    state.local.save_shortcut(input).await
}

/// Deletes a local terminal shortcut; built-ins can later be restored from the defaults action.
#[tauri::command]
pub async fn delete_shortcut(id: String, state: State<'_, AppState>) -> AppResult<()> {
    state.local.delete_shortcut(&id).await
}

/// Restores missing built-in terminal shortcuts without overwriting user edits.
#[tauri::command]
pub async fn restore_default_shortcuts(state: State<'_, AppState>) -> AppResult<()> {
    state.local.restore_default_shortcuts().await
}

/// Records one local shortcut insertion for usage-based completion ranking.
#[tauri::command]
pub async fn use_shortcut(id: String, state: State<'_, AppState>) -> AppResult<()> {
    state.local.use_shortcut(&id).await
}

#[tauri::command]
pub async fn get_server(server_id: String, state: State<'_, AppState>) -> AppResult<ServerProfile> {
    state.servers.get(&server_id).await
}

/// 列出本地服务器分组。
#[tauri::command]
pub async fn list_server_groups(
    state: State<'_, AppState>,
) -> AppResult<Vec<crate::domain::server::ServerGroup>> {
    state.servers.list_groups().await
}

/// 创建本地服务器分组。
#[tauri::command]
pub async fn create_server_group(
    name: String,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::server::ServerGroup> {
    state.servers.create_group(name).await
}

#[tauri::command]
pub async fn save_server(
    input: SaveServerInput,
    state: State<'_, AppState>,
) -> AppResult<ServerProfile> {
    let action = if input.id.is_some() {
        "update_server"
    } else {
        "create_server"
    };
    let profile = state.servers.save(input).await?;
    write_audit(
        &state,
        Some(&profile.id),
        action,
        "server",
        Some(&profile.id),
        "success",
        format!(
            "服务器档案已{}：{}",
            if action == "create_server" {
                "创建"
            } else {
                "更新"
            },
            profile.name
        ),
    )
    .await;
    Ok(profile)
}

/// 复制服务器公共配置并打开新档案；系统凭据不会复制到副本。
#[tauri::command]
pub async fn duplicate_server(
    server_id: String,
    state: State<'_, AppState>,
) -> AppResult<ServerProfile> {
    let profile = state.servers.duplicate(&server_id).await?;
    write_audit(
        &state,
        Some(&profile.id),
        "duplicate_server",
        "server",
        Some(&profile.id),
        "success",
        format!("已从服务器档案复制副本：{}", profile.name),
    )
    .await;
    Ok(profile)
}

#[tauri::command]
pub async fn delete_server(server_id: String, state: State<'_, AppState>) -> AppResult<()> {
    state.ssh.disconnect(&server_id).await?;
    state.servers.delete(&server_id).await?;
    write_audit(
        &state,
        None,
        "delete_server",
        "server",
        Some(&server_id),
        "success",
        "服务器档案已删除；远端未被修改".into(),
    )
    .await;
    Ok(())
}

#[tauri::command]
pub async fn connection_state(
    server_id: String,
    state: State<'_, AppState>,
) -> AppResult<ConnectionSnapshot> {
    state.servers.record(&server_id).await?;
    Ok(state.ssh.snapshot(&server_id))
}

#[tauri::command]
pub async fn connect_server(
    server_id: String,
    state: State<'_, AppState>,
) -> AppResult<ConnectOutcome> {
    let result = state.ssh.connect(&server_id).await;
    match &result {
        Ok(ConnectOutcome::Connected(_)) => {
            write_audit(
                &state,
                Some(&server_id),
                "connect_server",
                "connection",
                Some(&server_id),
                "success",
                "SSH 会话已建立".into(),
            )
            .await
        }
        Ok(ConnectOutcome::HostKey(_)) => {
            write_audit(
                &state,
                Some(&server_id),
                "connect_server",
                "connection",
                Some(&server_id),
                "pending",
                "等待用户核对 Host Key".into(),
            )
            .await
        }
        Err(error) => {
            write_audit(
                &state,
                Some(&server_id),
                "connect_server",
                "connection",
                Some(&server_id),
                "failed",
                format!("SSH 连接失败：{}", error.code),
            )
            .await
        }
    }
    result
}

/// 断开旧 SSH 会话并执行有限退避重连，审计结果与普通连接保持一致。
#[tauri::command]
pub async fn reconnect_server(
    server_id: String,
    state: State<'_, AppState>,
) -> AppResult<ConnectOutcome> {
    let result = state.ssh.reconnect(&server_id).await;
    match &result {
        Ok(ConnectOutcome::Connected(_)) => {
            write_audit(
                &state,
                Some(&server_id),
                "reconnect_server",
                "connection",
                Some(&server_id),
                "success",
                "SSH 会话已通过有限退避重连".into(),
            )
            .await
        }
        Ok(ConnectOutcome::HostKey(_)) => {
            write_audit(
                &state,
                Some(&server_id),
                "reconnect_server",
                "connection",
                Some(&server_id),
                "pending",
                "重连等待用户核对 Host Key".into(),
            )
            .await
        }
        Err(error) => {
            write_audit(
                &state,
                Some(&server_id),
                "reconnect_server",
                "connection",
                Some(&server_id),
                "failed",
                format!("SSH 重连失败：{}", error.code),
            )
            .await
        }
    }
    result
}

#[tauri::command]
pub async fn trust_host_key(
    challenge: TrustHostKeyInput,
    state: State<'_, AppState>,
) -> AppResult<ConnectionSnapshot> {
    let server_id = challenge.server_id.clone();
    let result = state.ssh.trust(challenge).await;
    match &result {
        Ok(_) => {
            write_audit(
                &state,
                Some(&server_id),
                "trust_host_key",
                "known_host",
                Some(&server_id),
                "success",
                "用户确认并信任 Host Key".into(),
            )
            .await
        }
        Err(error) => {
            write_audit(
                &state,
                Some(&server_id),
                "trust_host_key",
                "known_host",
                Some(&server_id),
                "failed",
                format!("Host Key 信任失败：{}", error.code),
            )
            .await
        }
    }
    result
}

#[tauri::command]
pub async fn disconnect_server(server_id: String, state: State<'_, AppState>) -> AppResult<()> {
    let result = state.ssh.disconnect(&server_id).await;
    match &result {
        Ok(_) => {
            write_audit(
                &state,
                Some(&server_id),
                "disconnect_server",
                "connection",
                Some(&server_id),
                "success",
                "SSH 会话已断开".into(),
            )
            .await
        }
        Err(error) => {
            write_audit(
                &state,
                Some(&server_id),
                "disconnect_server",
                "connection",
                Some(&server_id),
                "failed",
                format!("SSH 断开失败：{}", error.code),
            )
            .await
        }
    }
    result
}

#[tauri::command]
pub async fn get_system_overview(
    server_id: String,
    state: State<'_, AppState>,
) -> AppResult<SystemOverview> {
    let overview = crate::domain::metrics::probe(&state.ssh, &server_id).await?;
    if let Err(error) = state.local.record_metric(&server_id, &overview).await {
        tracing::warn!(error = %error, server_id, "写入本地监控采样失败");
    }
    Ok(overview)
}

/// Reads one server's local Overview memo without contacting the remote host.
#[tauri::command]
pub async fn get_overview_memo(
    server_id: String,
    state: State<'_, AppState>,
) -> AppResult<OverviewMemo> {
    validate_overview_server_id(&server_id)?;
    let value = state
        .local
        .get_setting(&overview_memo_key(&server_id))
        .await?;
    value
        .map(|value| {
            serde_json::from_str::<OverviewMemo>(&value).map_err(|error| {
                crate::errors::AppError::new("LOCAL_DATA_INVALID", "overview", "备忘录无法解析")
                    .details(error)
            })
        })
        .transpose()
        .map(|memo| {
            memo.unwrap_or(OverviewMemo {
                content: String::new(),
                updated_at: None,
            })
        })
}

/// Saves a server-scoped Overview memo locally; the text never leaves the client.
#[tauri::command]
pub async fn save_overview_memo(
    input: SaveOverviewMemoInput,
    state: State<'_, AppState>,
) -> AppResult<OverviewMemo> {
    validate_overview_server_id(&input.server_id)?;
    validate_overview_memo_content(&input.content)?;
    let memo = OverviewMemo {
        content: input.content,
        updated_at: Some(Utc::now()),
    };
    let value = serde_json::to_string(&memo).map_err(crate::errors::AppError::database)?;
    state
        .local
        .set_setting(&overview_memo_key(&input.server_id), &value)
        .await?;
    Ok(memo)
}

/// Returns bounded local monitoring history for one server after an RFC3339 timestamp.
#[tauri::command]
pub async fn get_metric_history(
    server_id: String,
    since: String,
    state: State<'_, AppState>,
) -> AppResult<Vec<MetricSample>> {
    let since = chrono::DateTime::parse_from_rfc3339(&since)
        .map_err(|error| {
            crate::errors::AppError::new("VALIDATION_FAILED", "metrics", "监控历史起始时间无效")
                .details(error)
        })?
        .with_timezone(&chrono::Utc);
    state.local.metric_history(&server_id, since, 500).await
}

/// Saves a non-sensitive task state transition in the local task ledger.
#[tauri::command]
pub async fn save_task(input: SaveTaskInput, state: State<'_, AppState>) -> AppResult<TaskRecord> {
    state.local.save_task(input).await
}

/// Lists recent persisted tasks for task-center hydration after application startup.
#[tauri::command]
pub async fn list_tasks(state: State<'_, AppState>) -> AppResult<Vec<TaskRecord>> {
    state.local.list_tasks(500).await
}

/// Removes successful, failed, and cancelled task metadata while retaining interruption history.
#[tauri::command]
pub async fn clear_finished_tasks(state: State<'_, AppState>) -> AppResult<()> {
    state.local.clear_finished_tasks().await
}

#[tauri::command]
pub async fn open_terminal(
    server_id: String,
    columns: u32,
    rows: u32,
    on_event: Channel<crate::domain::ssh::TerminalEvent>,
    state: State<'_, AppState>,
) -> AppResult<String> {
    let (terminal_id, mut events) = state.ssh.open_terminal(&server_id, columns, rows).await?;
    tauri::async_runtime::spawn(async move {
        while let Some(event) = events.recv().await {
            if on_event.send(event).is_err() {
                break;
            }
        }
    });
    Ok(terminal_id)
}

#[tauri::command]
pub async fn write_terminal(
    terminal_id: String,
    data: Vec<u8>,
    state: State<'_, AppState>,
) -> AppResult<()> {
    state.ssh.write_terminal(&terminal_id, &data).await
}

#[tauri::command]
pub async fn resize_terminal(
    terminal_id: String,
    columns: u32,
    rows: u32,
    state: State<'_, AppState>,
) -> AppResult<()> {
    state.ssh.resize_terminal(&terminal_id, columns, rows).await
}

#[tauri::command]
pub async fn close_terminal(terminal_id: String, state: State<'_, AppState>) -> AppResult<()> {
    state.ssh.close_terminal(&terminal_id).await
}

#[tauri::command]
pub async fn list_remote_directory(
    server_id: String,
    path: String,
    state: State<'_, AppState>,
) -> AppResult<DirectoryListing> {
    crate::domain::files::list(&state.ssh, &server_id, &path).await
}

#[tauri::command]
pub async fn read_remote_text(
    server_id: String,
    path: String,
    state: State<'_, AppState>,
) -> AppResult<RemoteTextFile> {
    crate::domain::files::read_text(&state.ssh, &server_id, &path).await
}

/// 读取远程图片预览数据，供文件页右侧预览面板使用。
#[tauri::command]
pub async fn read_remote_image_preview(
    server_id: String,
    path: String,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::files::RemoteBinaryPreview> {
    crate::domain::files::read_image_preview(&state.ssh, &server_id, &path).await
}

/// 读取大文件有限尾部供 Large File Viewer 展示，不将整文件载入本地。
#[tauri::command]
pub async fn read_remote_tail(
    server_id: String,
    path: String,
    lines: u32,
    state: State<'_, AppState>,
) -> AppResult<RemoteTextFile> {
    crate::domain::files::read_tail(&state.ssh, &server_id, &path, lines).await
}

#[tauri::command]
pub async fn save_remote_text(
    input: SaveTextInput,
    state: State<'_, AppState>,
) -> AppResult<RemoteTextFile> {
    let server_id = input.server_id.clone();
    let path = input.path.clone();
    let result = crate::domain::files::save_text(&state.ssh, input).await;
    audit_outcome(
        &state,
        Some(&server_id),
        "save_remote_text",
        "file",
        Some(&path),
        &result,
        format!("保存远程文件：{}", path),
    )
    .await;
    result
}

#[tauri::command]
pub async fn save_remote_text_privileged(
    input: SaveTextInput,
    state: State<'_, AppState>,
) -> AppResult<RemoteTextFile> {
    let server_id = input.server_id.clone();
    let path = input.path.clone();
    let result = crate::domain::files::save_text_privileged(&state.ssh, input).await;
    audit_outcome(
        &state,
        Some(&server_id),
        "save_remote_text_privileged",
        "file",
        Some(&path),
        &result,
        format!("sudo 保存远程文件：{}", path),
    )
    .await;
    result
}

#[tauri::command]
pub async fn create_remote_entry(
    server_id: String,
    path: String,
    directory: bool,
    state: State<'_, AppState>,
) -> AppResult<()> {
    let result = crate::domain::files::create(&state.ssh, &server_id, &path, directory).await;
    audit_outcome(
        &state,
        Some(&server_id),
        "create_remote_entry",
        "file",
        Some(&path),
        &result,
        format!(
            "创建远程{}：{}",
            if directory { "目录" } else { "文件" },
            path
        ),
    )
    .await;
    result
}

#[tauri::command]
pub async fn rename_remote_entry(
    server_id: String,
    old_path: String,
    new_path: String,
    state: State<'_, AppState>,
) -> AppResult<()> {
    let result = crate::domain::files::rename(&state.ssh, &server_id, &old_path, &new_path).await;
    audit_outcome(
        &state,
        Some(&server_id),
        "rename_remote_entry",
        "file",
        Some(&old_path),
        &result,
        format!("重命名远程对象为：{}", new_path),
    )
    .await;
    result
}

#[tauri::command]
pub async fn remove_remote_entry(
    server_id: String,
    path: String,
    recursive: bool,
    state: State<'_, AppState>,
) -> AppResult<()> {
    let result = crate::domain::files::remove(&state.ssh, &server_id, &path, recursive).await;
    audit_outcome(
        &state,
        Some(&server_id),
        "remove_remote_entry",
        "file",
        Some(&path),
        &result,
        format!("删除远程对象：{}", path),
    )
    .await;
    result
}

/// 修改远程文件或文件夹的 Unix 权限，并验证 SFTP 元数据结果。
#[tauri::command]
pub async fn chmod_remote(
    input: crate::domain::files::ChmodInput,
    state: State<'_, AppState>,
) -> AppResult<()> {
    let server_id = input.server_id.clone();
    let path = input.path.clone();
    let result = crate::domain::files::chmod(&state.ssh, input).await;
    audit_outcome(
        &state,
        Some(&server_id),
        "chmod_remote",
        "file",
        Some(&path),
        &result,
        format!("修改远程权限：{}", path),
    )
    .await;
    result
}

/// 创建远程符号链接并验证链接对象，不跟随目标执行写入。
#[tauri::command]
pub async fn create_remote_symlink(
    input: crate::domain::files::SymlinkInput,
    state: State<'_, AppState>,
) -> AppResult<()> {
    let server_id = input.server_id.clone();
    let link_path = input.link_path.clone();
    let result = crate::domain::files::symlink(&state.ssh, input).await;
    audit_outcome(
        &state,
        Some(&server_id),
        "create_remote_symlink",
        "file",
        Some(&link_path),
        &result,
        format!("创建远程符号链接：{}", link_path),
    )
    .await;
    result
}

/// 在同一台远程服务器内部复制或移动文件，并由后端拒绝覆盖已存在的目标。
#[tauri::command]
pub async fn copy_move_remote(
    input: crate::domain::files::CopyMoveInput,
    state: State<'_, AppState>,
) -> AppResult<()> {
    let server_id = input.server_id.clone();
    let source_path = input.source_path.clone();
    let result = crate::domain::files::copy_move(&state.ssh, input).await;
    audit_outcome(
        &state,
        Some(&server_id),
        "copy_move_remote",
        "file",
        Some(&source_path),
        &result,
        format!("复制或移动远程对象：{}", source_path),
    )
    .await;
    result
}

#[tauri::command]
pub async fn upload_remote(
    transfer_id: String,
    server_id: String,
    local_path: String,
    remote_directory: String,
    conflict: String,
    on_event: Channel<crate::domain::transfer::TransferEvent>,
    state: State<'_, AppState>,
) -> AppResult<()> {
    crate::domain::transfer::upload(
        &state.transfers,
        &state.ssh,
        &transfer_id,
        &server_id,
        &local_path,
        &remote_directory,
        &conflict,
        &on_event,
    )
    .await
}

#[tauri::command]
pub async fn download_remote(
    transfer_id: String,
    server_id: String,
    remote_path: String,
    local_directory: String,
    on_event: Channel<crate::domain::transfer::TransferEvent>,
    state: State<'_, AppState>,
) -> AppResult<()> {
    crate::domain::transfer::download(
        &state.transfers,
        &state.ssh,
        &transfer_id,
        &server_id,
        &remote_path,
        &local_directory,
        &on_event,
    )
    .await
}

#[tauri::command]
pub fn cancel_transfer(transfer_id: String, state: State<'_, AppState>) -> AppResult<()> {
    state.transfers.cancel(&transfer_id)
}

/// 取消一个正在执行的流式 SSH 命令，并关闭对应远端 channel。
#[tauri::command]
pub fn cancel_command_task(task_id: String, state: State<'_, AppState>) -> AppResult<()> {
    state.ssh.cancel_task(&task_id)
}

#[tauri::command]
pub async fn get_operations(
    server_id: String,
    privileged: bool,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::operations::OperationsSnapshot> {
    crate::domain::operations::snapshot(&state.ssh, &server_id, privileged).await
}

#[tauri::command]
pub async fn terminate_process(
    input: crate::domain::operations::TerminateProcessInput,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::operations::TerminationResult> {
    crate::domain::operations::terminate(&state.ssh, input).await
}

#[tauri::command]
pub async fn manage_service(
    server_id: String,
    service: String,
    action: String,
    state: State<'_, AppState>,
) -> AppResult<()> {
    let result =
        crate::domain::operations::service_action(&state.ssh, &server_id, &service, &action).await;
    audit_outcome(
        &state,
        Some(&server_id),
        "manage_service",
        "systemd_service",
        Some(&service),
        &result,
        format!("服务 {}：{}", action, service),
    )
    .await;
    result
}

/// 查询 systemd 服务的详细状态和来源路径。
#[tauri::command]
pub async fn get_service_detail(
    server_id: String,
    service: String,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::operations::ServiceDetail> {
    crate::domain::operations::service_detail(&state.ssh, &server_id, &service).await
}

/// 查询 systemd 服务最近日志。
#[tauri::command]
pub async fn get_service_logs(
    server_id: String,
    service: String,
    lines: u32,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::operations::ServiceLogs> {
    crate::domain::operations::service_logs(&state.ssh, &server_id, &service, lines).await
}

/// 读取远端块设备、挂载点和 /etc/fstab 的受控摘要。
#[tauri::command]
pub async fn get_storage(
    server_id: String,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::storage::StorageSnapshot> {
    crate::domain::storage::snapshot(&state.ssh, &server_id).await
}

/// 在用户确认后执行挂载、卸载或 fstab 变更，并记录不含路径内容的审计摘要。
#[tauri::command]
pub async fn storage_action(
    input: crate::domain::storage::StorageActionInput,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::storage::StorageActionResult> {
    let result = crate::domain::storage::action(&state.ssh, input.clone()).await;
    audit_outcome(
        &state,
        Some(&input.server_id),
        &format!("storage_{}", input.action),
        "storage",
        Some(&input.mountpoint),
        &result,
        format!("存储操作 {}", input.action),
    )
    .await;
    result
}

/// Reads a bounded supported log source without persisting the remote log content locally.
#[tauri::command]
pub async fn get_logs(
    query: crate::domain::logs::LogQuery,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::logs::LogSnapshot> {
    crate::domain::logs::read(&state.ssh, &query).await
}

/// Follows a supported log source through a cancellable SSH task and Tauri Channel.
#[tauri::command]
pub async fn follow_logs(
    query: crate::domain::logs::LogQuery,
    task_id: String,
    on_event: Channel<crate::domain::ssh::CommandEvent>,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::logs::LogSnapshot> {
    crate::domain::logs::follow(&state.ssh, &query, &task_id, &on_event).await
}

/// 导出不含 secret 的服务器档案配置。
#[tauri::command]
pub async fn export_servers(
    state: State<'_, AppState>,
) -> AppResult<crate::domain::server::PublicServerExport> {
    state.servers.export_public().await
}

/// 导入公共服务器配置并为每条记录生成新的本地 ID。
#[tauri::command]
pub async fn import_servers(
    values: Vec<crate::domain::server::PublicServerImport>,
    state: State<'_, AppState>,
) -> AppResult<Vec<crate::domain::server::ServerProfile>> {
    state.servers.import_public(values).await
}

/// 导出本地档案、连接状态和最近审计元数据；响应不含凭据、命令输出或私钥内容。
#[tauri::command]
pub async fn export_diagnostics(
    state: State<'_, AppState>,
) -> AppResult<crate::domain::diagnostics::DiagnosticsExport> {
    let profiles = state.servers.list().await?;
    let connections = profiles
        .iter()
        .map(|profile| state.ssh.snapshot(&profile.id))
        .collect();
    let recent_audit = state.servers.list_audit(100).await?;
    Ok(crate::domain::diagnostics::DiagnosticsExport::build(
        profiles,
        connections,
        recent_audit,
    ))
}

/// 读取最近的本地审计事件，不返回任何远端命令输出。
#[tauri::command]
pub async fn list_audit_events(
    limit: u32,
    state: State<'_, AppState>,
) -> AppResult<Vec<crate::infra::db::AuditEvent>> {
    state.servers.list_audit(limit).await
}

/// 读取配置和系统凭据并输出 Argon2id/AES-256-GCM 加密备份文本。
#[tauri::command]
pub async fn export_full_backup(
    input: crate::domain::backup::ExportBackupInput,
    state: State<'_, AppState>,
) -> AppResult<String> {
    let payload = state.servers.export_backup().await?;
    crate::domain::backup::encrypt(&payload, &input.password)
}

/// 解密完整备份并将服务器档案和 secret 导入本地安全存储。
#[tauri::command]
pub async fn import_full_backup(
    input: crate::domain::backup::ImportBackupInput,
    state: State<'_, AppState>,
) -> AppResult<Vec<crate::domain::server::ServerProfile>> {
    let payload = crate::domain::backup::decrypt(&input.backup, &input.password)?;
    state.servers.import_backup(payload).await
}

/// 列出本机备份账号；secret 只返回是否已配置，不返回密钥内容。
#[tauri::command]
pub async fn list_backup_accounts(
    state: State<'_, AppState>,
) -> AppResult<Vec<crate::domain::backup_accounts::BackupAccount>> {
    crate::domain::backup_accounts::list(&state.local, &state.credentials).await
}

/// 保存或更新本机备份账号，并把敏感 secret 写入操作系统密钥链。
#[tauri::command]
pub async fn save_backup_account(
    input: crate::domain::backup_accounts::SaveBackupAccountInput,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::backup_accounts::BackupAccount> {
    let result =
        crate::domain::backup_accounts::save(&state.local, &state.credentials, input.clone()).await;
    audit_outcome(
        &state,
        input.server_id.as_deref(),
        "save_backup_account",
        "backup_account",
        input.id.as_deref(),
        &result,
        format!("保存备份账号：{}", input.name.trim()),
    )
    .await;
    result
}

/// 删除本机备份账号和对应密钥链条目，不触碰已有归档文件。
#[tauri::command]
pub async fn delete_backup_account(
    id: String,
    confirmed: bool,
    state: State<'_, AppState>,
) -> AppResult<()> {
    let result =
        crate::domain::backup_accounts::delete(&state.local, &state.credentials, &id, confirmed)
            .await;
    audit_outcome(
        &state,
        None,
        "delete_backup_account",
        "backup_account",
        Some(&id),
        &result,
        "删除备份账号".into(),
    )
    .await;
    result
}

/// 对备份账号进行只读连通性检查，不会创建归档或上传数据。
#[tauri::command]
pub async fn test_backup_account(
    id: String,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::backup_accounts::BackupAccountTestResult> {
    crate::domain::backup_accounts::test(&state.local, &state.credentials, &id).await
}

/// 将指定服务器上的一次备份归档上传到本机备份账号，并返回脱敏结果。
#[tauri::command]
pub async fn upload_backup_artifact(
    input: crate::domain::backup_accounts::UploadBackupInput,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::backup_accounts::BackupUploadResult> {
    let result = crate::domain::backup_accounts::upload(
        &state.ssh,
        &state.local,
        &state.credentials,
        input.clone(),
    )
    .await;
    audit_outcome(
        &state,
        Some(&input.server_id),
        "upload_backup_artifact",
        "backup_account",
        Some(&input.account_id),
        &result,
        "上传计划任务归档".into(),
    )
    .await;
    result
}

/// 查询远端工具注册表的安装、版本和运行状态。
#[tauri::command]
pub async fn list_tools(
    server_id: String,
    state: State<'_, AppState>,
) -> AppResult<Vec<crate::domain::tools::ToolStatus>> {
    crate::domain::tools::list(&state.ssh, &server_id).await
}

/// 返回用户确认前可展示的工具安装计划。
#[tauri::command]
pub async fn get_tool_install_plan(
    server_id: String,
    tool_id: String,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::tools::ToolInstallPlan> {
    crate::domain::tools::install_plan(&state.ssh, &server_id, &tool_id).await
}

/// 执行用户明确确认的工具安装，并验证安装结果。
#[tauri::command]
pub async fn install_tool(
    input: crate::domain::tools::InstallToolInput,
    on_event: Channel<crate::domain::ssh::CommandEvent>,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::tools::ToolInstallResult> {
    let server_id = input.server_id.clone();
    let tool_id = input.tool_id.clone();
    let result = crate::domain::tools::install(&state.ssh, input, &on_event).await;
    audit_outcome(
        &state,
        Some(&server_id),
        "install_tool",
        "tool",
        Some(&tool_id),
        &result,
        format!("安装工具：{}", tool_id),
    )
    .await;
    result
}

/// 查询 Nginx 真实配置摘要和反向代理 source mapping。
#[tauri::command]
pub async fn get_nginx(
    server_id: String,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::nginx::NginxSnapshot> {
    crate::domain::nginx::snapshot(&state.ssh, &server_id).await
}

/// 运行只读的 Nginx 配置语法检查。
#[tauri::command]
pub async fn test_nginx_config(server_id: String, state: State<'_, AppState>) -> AppResult<bool> {
    crate::domain::nginx::test_config(&state.ssh, &server_id).await
}

/// 从远端服务器探测 Nginx 代理目标的可达性和 HTTP 状态。
#[tauri::command]
pub async fn probe_nginx_backend(
    input: crate::domain::nginx::NginxBackendProbeInput,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::nginx::NginxBackendProbeResult> {
    crate::domain::nginx::probe_backend(&state.ssh, input).await
}

/// 写入受控 managed conf，失败时由 Rust 端恢复备份并阻止 reload。
#[tauri::command]
pub async fn save_nginx_proxy(
    input: crate::domain::nginx::NginxProxyInput,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::nginx::NginxSnapshot> {
    let server_id = input.server_id.clone();
    let name = input.name.clone();
    let result = crate::domain::nginx::save_proxy(&state.ssh, input).await;
    audit_outcome(
        &state,
        Some(&server_id),
        "save_nginx_proxy",
        "nginx_proxy",
        Some(&name),
        &result,
        format!("保存 Nginx 代理：{}", name),
    )
    .await;
    result
}

/// 查询远程 Docker Engine、容器和镜像列表。
#[tauri::command]
pub async fn get_docker(
    server_id: String,
    privileged: bool,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::docker::DockerSnapshot> {
    crate::domain::docker::snapshot(&state.ssh, &server_id, privileged).await
}

/// 读取远程 Docker daemon 最近事件的有界摘要，不保存原始 actor 属性。
#[tauri::command]
pub async fn get_docker_events(
    server_id: String,
    since_seconds: u32,
    privileged: bool,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::docker::DockerEventsResult> {
    crate::domain::docker::events(&state.ssh, &server_id, since_seconds, privileged).await
}

/// 执行已确认的 Docker 容器动作并验证状态。
#[tauri::command]
pub async fn docker_container_action(
    input: crate::domain::docker::DockerActionInput,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::docker::DockerActionResult> {
    let server_id = input.server_id.clone();
    let container_id = input.container_id.clone();
    let action = input.action.clone();
    let result = crate::domain::docker::action(&state.ssh, input).await;
    audit_outcome(
        &state,
        Some(&server_id),
        "docker_container_action",
        "docker_container",
        Some(&container_id),
        &result,
        format!("Docker 容器 {}：{}", action, container_id),
    )
    .await;
    result
}

/// 读取远端容器最近日志，不将日志写入本地数据库。
#[tauri::command]
pub async fn docker_container_logs(
    server_id: String,
    container_id: String,
    tail: u32,
    privileged: bool,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::docker::DockerLogs> {
    crate::domain::docker::logs(&state.ssh, &server_id, &container_id, tail, privileged).await
}

/// 读取单个容器的原始 inspect JSON。
#[tauri::command]
pub async fn docker_container_inspect(
    server_id: String,
    container_id: String,
    privileged: bool,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::docker::DockerTextResult> {
    crate::domain::docker::inspect(&state.ssh, &server_id, &container_id, privileged).await
}

/// 读取单个容器的一次性资源统计。
#[tauri::command]
pub async fn docker_container_stats(
    server_id: String,
    container_id: String,
    privileged: bool,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::docker::DockerTextResult> {
    crate::domain::docker::stats(&state.ssh, &server_id, &container_id, privileged).await
}

/// 读取单个容器内的进程列表。
#[tauri::command]
pub async fn docker_container_top(
    server_id: String,
    container_id: String,
    privileged: bool,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::docker::DockerTextResult> {
    crate::domain::docker::top(&state.ssh, &server_id, &container_id, privileged).await
}

/// 在容器内执行受控命令并返回一次性输出。
#[tauri::command]
pub async fn docker_container_exec(
    input: crate::domain::docker::DockerExecInput,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::docker::DockerTextResult> {
    crate::domain::docker::exec(&state.ssh, input).await
}

/// 跟随容器日志最多 30 秒，并通过 Channel 转发输出块。
#[tauri::command]
pub async fn docker_container_follow_logs(
    server_id: String,
    container_id: String,
    tail: u32,
    sudo: bool,
    task_id: String,
    on_event: Channel<crate::domain::ssh::CommandEvent>,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::docker::DockerLogs> {
    crate::domain::docker::follow_logs(
        &state.ssh,
        &server_id,
        &container_id,
        tail,
        sudo,
        &task_id,
        &on_event,
    )
    .await
}

/// 创建或删除 Docker volume/network，并返回远端 inspect 验证结果。
#[tauri::command]
pub async fn docker_resource_action(
    input: crate::domain::docker::DockerResourceActionInput,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::docker::DockerResourceActionResult> {
    let server_id = input.server_id.clone();
    let name = input.name.clone();
    let action = input.action.clone();
    let result = crate::domain::docker::resource_action(&state.ssh, input).await;
    audit_outcome(
        &state,
        Some(&server_id),
        "docker_resource_action",
        "docker_resource",
        Some(&name),
        &result,
        format!("Docker 资源 {}：{}", action, name),
    )
    .await;
    result
}

/// 执行已确认的 Docker 镜像删除，并记录不含镜像输出的本地审计事件。
#[tauri::command]
pub async fn docker_image_action(
    input: crate::domain::docker::DockerImageActionInput,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::docker::DockerResourceActionResult> {
    let server_id = input.server_id.clone();
    let image = input.image.clone();
    let result = crate::domain::docker::image_action(&state.ssh, input).await;
    audit_outcome(
        &state,
        Some(&server_id),
        "docker_image_action",
        "docker_image",
        Some(&image),
        &result,
        format!("Docker 镜像操作：{image}"),
    )
    .await;
    result
}

/// 读取 Docker volume/network inspect JSON，并保持结果只在当前 UI 响应中存在。
#[tauri::command]
pub async fn docker_resource_inspect(
    input: crate::domain::docker::DockerResourceInspectInput,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::docker::DockerTextResult> {
    crate::domain::docker::resource_inspect(&state.ssh, input).await
}

/// 执行 Compose 项目生命周期操作并验证项目列表。
#[tauri::command]
pub async fn docker_compose_action(
    input: crate::domain::docker::DockerComposeActionInput,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::docker::DockerResourceActionResult> {
    let server_id = input.server_id.clone();
    let project = input.project.clone();
    let action = input.action.clone();
    let result = crate::domain::docker::compose_action(&state.ssh, input).await;
    audit_outcome(
        &state,
        Some(&server_id),
        "docker_compose_action",
        "docker_compose",
        Some(&project),
        &result,
        format!("Compose 项目 {}：{}", action, project),
    )
    .await;
    result
}

/// 保存 Compose 原始 YAML，先执行 `docker compose config -q`，失败时自动恢复备份。
#[tauri::command]
pub async fn docker_compose_save_yaml(
    input: crate::domain::docker::DockerComposeYamlInput,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::files::RemoteTextFile> {
    let server_id = input.server_id.clone();
    let path = input.config_path.clone();
    let result = crate::domain::docker::save_compose_yaml(&state.ssh, input).await;
    audit_outcome(
        &state,
        Some(&server_id),
        "docker_compose_save_yaml",
        "docker_compose",
        Some(&path),
        &result,
        format!("保存 Compose YAML：{}", path),
    )
    .await;
    result
}

/// 读取 Compose 项目的服务、渲染配置和资源候选，不修改远端状态。
#[tauri::command]
pub async fn docker_compose_details(
    server_id: String,
    project: String,
    working_dir: Option<String>,
    sudo: bool,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::docker::DockerComposeDetails> {
    crate::domain::docker::compose_details(
        &state.ssh,
        &server_id,
        &project,
        working_dir.as_deref(),
        sudo,
    )
    .await
}

/// 读取 Compose 项目或单个服务的最近日志，不保存日志内容。
#[tauri::command]
pub async fn docker_compose_logs(
    server_id: String,
    project: String,
    working_dir: Option<String>,
    service: Option<String>,
    tail: u32,
    sudo: bool,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::docker::DockerLogs> {
    crate::domain::docker::compose_logs(
        &state.ssh,
        &server_id,
        &project,
        working_dir.as_deref(),
        service.as_deref(),
        tail,
        sudo,
    )
    .await
}

/// 拉取单个 Docker 镜像，并通过 Channel 转发 layer 输出。
#[tauri::command]
pub async fn docker_pull_image(
    input: crate::domain::docker::DockerPullInput,
    on_event: Channel<crate::domain::ssh::CommandEvent>,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::docker::DockerPullResult> {
    let server_id = input.server_id.clone();
    let image = input.image.clone();
    let result = crate::domain::docker::pull(&state.ssh, input, &on_event).await;
    audit_outcome(
        &state,
        Some(&server_id),
        "docker_pull_image",
        "docker_image",
        Some(&image),
        &result,
        format!("拉取 Docker 镜像：{}", image),
    )
    .await;
    result
}

/// 构建远端 Docker 镜像，并把可取消的 CLI 输出交给任务中心。
#[tauri::command]
pub async fn docker_build_image(
    input: crate::domain::docker::DockerBuildInput,
    on_event: Channel<crate::domain::ssh::CommandEvent>,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::docker::DockerBuildResult> {
    let server_id = input.server_id.clone();
    let image = input.image.clone();
    let context_path = input.context_path.clone();
    let result = crate::domain::docker::build(&state.ssh, input, &on_event).await;
    audit_outcome(
        &state,
        Some(&server_id),
        "docker_build_image",
        "docker_image",
        Some(&image),
        &result,
        format!("构建 Docker 镜像：{}（上下文 {}）", image, context_path),
    )
    .await;
    result
}

/// 执行受控 Run Container 向导，并验证容器创建结果。
#[tauri::command]
pub async fn docker_run_container(
    input: crate::domain::docker::DockerRunInput,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::docker::DockerRunResult> {
    let server_id = input.server_id.clone();
    let image = input.image.clone();
    let result = crate::domain::docker::run(&state.ssh, input).await;
    audit_outcome(
        &state,
        Some(&server_id),
        "docker_run_container",
        "docker_container",
        None,
        &result,
        format!("运行 Docker 镜像：{}", image),
    )
    .await;
    result
}

/// 读取远端 MySQL、PostgreSQL、Redis 引擎和数据库列表。
#[tauri::command]
pub async fn get_databases(
    server_id: String,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::database::DatabaseSnapshot> {
    crate::domain::database::snapshot(&state.ssh, &server_id).await
}

/// 在用户确认后创建或删除远端数据库，并写入本地审计记录。
#[tauri::command]
pub async fn database_action(
    input: crate::domain::database::DatabaseActionInput,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::database::DatabaseActionResult> {
    let result = crate::domain::database::action(&state.ssh, input.clone()).await;
    write_audit(
        &state,
        Some(&input.server_id),
        &format!("database_{}", input.action),
        "database",
        Some(&input.name),
        if result.is_ok() { "success" } else { "failed" },
        format!("数据库 {} {}", input.action, input.name),
    )
    .await;
    result
}

/// 将远端数据库导出到用户指定的备份路径。
#[tauri::command]
pub async fn backup_database(
    input: crate::domain::database::DatabaseBackupInput,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::database::DatabaseActionResult> {
    let result = crate::domain::database::backup(&state.ssh, input.clone()).await;
    write_audit(
        &state,
        Some(&input.server_id),
        "database_backup",
        "database",
        Some(&input.name),
        if result.is_ok() { "success" } else { "failed" },
        format!("数据库备份 {}", input.name),
    )
    .await;
    result
}

/// 从远端 SQL 文件恢复数据库。
#[tauri::command]
pub async fn restore_database(
    input: crate::domain::database::DatabaseRestoreInput,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::database::DatabaseActionResult> {
    let result = crate::domain::database::restore(&state.ssh, input.clone()).await;
    write_audit(
        &state,
        Some(&input.server_id),
        "database_restore",
        "database",
        Some(&input.name),
        if result.is_ok() { "success" } else { "failed" },
        format!("数据库恢复 {}", input.name),
    )
    .await;
    result
}

/// 创建、删除或调整数据库账号权限，并写入脱敏审计事件。
#[tauri::command]
pub async fn database_user_action(
    input: crate::domain::database::DatabaseUserActionInput,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::database::DatabaseActionResult> {
    let result = crate::domain::database::user_action(&state.ssh, input.clone()).await;
    audit_outcome(
        &state,
        Some(&input.server_id),
        &format!("database_user_{}", input.action),
        "database_user",
        Some(&input.username),
        &result,
        format!("数据库账号操作：{} / {}", input.engine, input.username),
    )
    .await;
    result
}

/// 读取指定数据库账号的真实数据库级权限矩阵，不回传密码或完整授权 SQL。
#[tauri::command]
pub async fn get_database_privileges(
    input: crate::domain::database::DatabasePrivilegeInput,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::database::DatabasePrivilegeSnapshot> {
    let result = crate::domain::database::privilege_matrix(&state.ssh, input.clone()).await;
    audit_outcome(
        &state,
        Some(&input.server_id),
        "database_privileges",
        "database_user",
        Some(&input.username),
        &result,
        format!("读取数据库权限矩阵：{} / {}", input.engine, input.username),
    )
    .await;
    result
}

/// 读取数据库权限矩阵并返回安全诊断建议（通配主机、全库授权、ALL 权限、Redis 过宽 ACL）。
#[tauri::command]
pub async fn get_database_privilege_diagnostic(
    input: crate::domain::database::DatabasePrivilegeInput,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::database::DatabasePrivilegeDiagnostic> {
    crate::domain::database::database_privilege_diagnostic(&state.ssh, input).await
}

/// 管理数据库 systemd 服务生命周期，并写入脱敏审计事件。
#[tauri::command]
pub async fn database_engine_action(
    input: crate::domain::database::DatabaseEngineActionInput,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::database::DatabaseActionResult> {
    let result = crate::domain::database::engine_action(&state.ssh, input.clone()).await;
    audit_outcome(
        &state,
        Some(&input.server_id),
        &format!("database_service_{}", input.action),
        "database_service",
        Some(&input.engine),
        &result,
        format!("数据库服务操作：{} / {}", input.engine, input.action),
    )
    .await;
    result
}

/// 返回数据库引擎安装计划，不执行远端安装写入。
#[tauri::command]
pub async fn get_database_install_plan(
    server_id: String,
    engine: String,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::database::DatabaseInstallPlan> {
    crate::domain::database::install_plan(&state.ssh, &server_id, &engine).await
}

/// 在用户明确确认后安装数据库引擎，并写入脱敏审计事件。
#[tauri::command]
pub async fn install_database_engine(
    input: crate::domain::database::DatabaseInstallInput,
    on_event: Channel<crate::domain::ssh::CommandEvent>,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::database::DatabaseInstallResult> {
    let result = crate::domain::database::install(&state.ssh, input.clone(), &on_event).await;
    audit_outcome(
        &state,
        Some(&input.server_id),
        "database_engine_install",
        "database_engine",
        Some(&input.engine),
        &result,
        format!("安装数据库引擎：{}", input.engine),
    )
    .await;
    result
}

/// 读取 Redis 键空间摘要，不把键值写入本地审计或持久化存储。
#[tauri::command]
pub async fn get_redis_data(
    input: crate::domain::database::RedisQueryInput,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::database::RedisSnapshot> {
    crate::domain::database::redis_snapshot(&state.ssh, input).await
}

/// 对 Redis 逻辑库执行只读连接诊断（PING 延迟、版本、角色与内存摘要）。
#[tauri::command]
pub async fn redis_diagnostic(
    input: crate::domain::database::RedisDiagnosticInput,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::database::RedisDiagnostic> {
    crate::domain::database::redis_diagnostic(&state.ssh, input).await
}

/// 删除 Redis 键或清空逻辑库，并写入不含值内容的审计事件。
#[tauri::command]
pub async fn redis_data_action(
    input: crate::domain::database::RedisActionInput,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::database::RedisActionResult> {
    let result = crate::domain::database::redis_action(&state.ssh, input.clone()).await;
    audit_outcome(
        &state,
        Some(&input.server_id),
        &format!("redis_{}", input.action),
        "redis_data",
        input.key.as_deref(),
        &result,
        format!("Redis 数据操作：db{} / {}", input.database, input.action),
    )
    .await;
    result
}

/// 读取或写入 Redis 键值，并在审计中只记录键名和动作。
#[tauri::command]
pub async fn redis_value_action(
    input: crate::domain::database::RedisValueInput,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::database::RedisValueResult> {
    let result = crate::domain::database::redis_value(&state.ssh, input.clone()).await;
    audit_outcome(
        &state,
        Some(&input.server_id),
        &format!("redis_value_{}", input.action),
        "redis_value",
        Some(&input.key),
        &result,
        format!("Redis 键值操作：db{} / {}", input.database, input.action),
    )
    .await;
    result
}

/// Executes one explicitly confirmed Redis hash/list/set/zset mutation and writes a value-free audit entry.
#[tauri::command]
pub async fn redis_complex_action(
    input: crate::domain::database::RedisComplexActionInput,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::database::RedisComplexActionResult> {
    let result = crate::domain::database::redis_complex_action(&state.ssh, input.clone()).await;
    audit_outcome(
        &state,
        Some(&input.server_id),
        &format!("redis_complex_{}", input.action),
        "redis_value",
        Some(&input.key),
        &result,
        format!("Redis 复杂值操作：db{} / {}", input.database, input.action),
    )
    .await;
    result
}

/// 在远端导出或导入 Redis 复杂值快照，并写入不含键值内容的审计事件。
#[tauri::command]
pub async fn redis_transfer_action(
    input: crate::domain::database::RedisTransferInput,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::database::RedisTransferResult> {
    let result = crate::domain::database::redis_transfer(&state.ssh, input.clone()).await;
    audit_outcome(
        &state,
        Some(&input.server_id),
        &format!("redis_{}", input.action),
        "redis_transfer",
        Some(&input.path),
        &result,
        format!("Redis 远端快照{}：db{}", input.action, input.database),
    )
    .await;
    result
}

/// 在源服务器上用 Redis MIGRATE 逐键迁移到目标实例，不把数据值返回客户端。
#[tauri::command]
pub async fn redis_migration_action(
    input: crate::domain::database::RedisMigrationInput,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::database::RedisMigrationResult> {
    let source_server_id = input.source_server_id.clone();
    let target_host = input.target_host.clone();
    let result = crate::domain::database::redis_migrate(&state.ssh, input).await;
    audit_outcome(
        &state,
        Some(&source_server_id),
        "redis_migration",
        "redis",
        Some(&target_host),
        &result,
        format!("Redis 跨实例迁移到 {target_host}"),
    )
    .await;
    result
}

/// 读取远端 crontab 和 systemd timer。
#[tauri::command]
pub async fn get_cronjobs(
    server_id: String,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::cronjob::CronSnapshot> {
    crate::domain::cronjob::snapshot(&state.ssh, &server_id).await
}

/// 导出当前服务器的版本化 crontab 任务文件，并记录不含命令输出的审计元数据。
#[tauri::command]
pub async fn export_cronjobs(
    server_id: String,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::cronjob::CronJobExport> {
    let result = crate::domain::cronjob::export_jobs(&state.ssh, &server_id).await;
    write_audit(
        &state,
        Some(&server_id),
        "cronjob_export",
        "cronjob",
        None,
        if result.is_ok() { "success" } else { "failed" },
        "计划任务已导出".into(),
    )
    .await;
    result
}

/// 导入用户明确确认的版本化任务文件；每条任务都会在远端生成新的客户端 marker。
#[tauri::command]
pub async fn import_cronjobs(
    input: crate::domain::cronjob::CronJobImportInput,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::cronjob::CronJobImportResult> {
    let server_id = input.server_id.clone();
    let mut input = input;
    let accounts = crate::domain::backup_accounts::list(&state.local, &state.credentials).await?;
    let unresolved_backup_accounts =
        resolve_import_backup_accounts(&server_id, &mut input.jobs, &accounts);
    let mut result = crate::domain::cronjob::import_jobs(&state.ssh, input).await;
    if let Ok(value) = result.as_mut() {
        value.unresolved_backup_accounts = unresolved_backup_accounts;
    }
    let summary = result
        .as_ref()
        .map(|value| {
            if value.unresolved_backup_accounts > 0 {
                format!(
                    "导入 {} 条计划任务，剔除 {} 个无效备份账号引用",
                    value.imported, value.unresolved_backup_accounts
                )
            } else {
                format!("导入 {} 条计划任务", value.imported)
            }
        })
        .unwrap_or_else(|_| "计划任务导入失败".into());
    write_audit(
        &state,
        Some(&server_id),
        "cronjob_import",
        "cronjob",
        None,
        if result.is_ok() { "success" } else { "failed" },
        summary,
    )
    .await;
    result
}

/// 过滤计划任务导入中的备份账号引用，只恢复当前客户端可用且绑定范围匹配的 UUID。
///
/// 返回被移除的引用数量；重复、格式非法、跨服务器、缺少 secret 的账号均不会写入远端 marker。
fn resolve_import_backup_accounts(
    server_id: &str,
    jobs: &mut [crate::domain::cronjob::CronJobExportEntry],
    accounts: &[crate::domain::backup_accounts::BackupAccount],
) -> usize {
    use std::collections::HashSet;

    let mut unresolved = 0usize;
    for job in jobs {
        let original_ids = job.backup_account_ids.clone();
        let mut seen = HashSet::new();
        job.backup_account_ids.retain(|account_id| {
            let valid = uuid::Uuid::parse_str(account_id).is_ok()
                && seen.insert(account_id.clone())
                && accounts.iter().any(|account| {
                    account.id == *account_id
                        && account
                            .server_id
                            .as_deref()
                            .is_none_or(|bound_server| bound_server == server_id)
                        && (account.kind == "local" || account.has_secret)
                });
            if !valid {
                unresolved += 1;
            }
            valid
        });
        if let Some(default_id) = job.default_backup_account_id.as_ref() {
            if !job.backup_account_ids.iter().any(|id| id == default_id) {
                if !original_ids.iter().any(|id| id == default_id) {
                    unresolved += 1;
                }
                job.default_backup_account_id = None;
            }
        }
    }
    unresolved
}

/// 创建或更新一条带 marker 的远端 crontab 任务。
#[tauri::command]
pub async fn save_cronjob(
    input: crate::domain::cronjob::SaveCronJobInput,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::cronjob::CronJobActionResult> {
    crate::domain::backup_accounts::validate_selection(
        &state.local,
        &state.credentials,
        &input.server_id,
        &input.backup_account_ids,
    )
    .await?;
    let result = crate::domain::cronjob::save(&state.ssh, input.clone()).await;
    write_audit(
        &state,
        Some(&input.server_id),
        "cronjob_save",
        "cronjob",
        input.id.as_deref(),
        if result.is_ok() { "success" } else { "failed" },
        "计划任务已保存".into(),
    )
    .await;
    result
}

/// 删除或立即运行一条远端计划任务。
#[tauri::command]
pub async fn cronjob_action(
    input: crate::domain::cronjob::CronJobActionInput,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::cronjob::CronJobActionResult> {
    let started_at = chrono::Utc::now();
    let mut result = crate::domain::cronjob::action(&state.ssh, input.clone()).await;
    if input.action == "run" && !input.backup_account_ids.is_empty() {
        if let Ok(action_result) = result.as_mut() {
            if let Some(remote_path) =
                crate::domain::cronjob::extract_backup_artifact_path(&action_result.output)
            {
                for account_id in &input.backup_account_ids {
                    let upload = crate::domain::backup_accounts::upload(
                        &state.ssh,
                        &state.local,
                        &state.credentials,
                        crate::domain::backup_accounts::UploadBackupInput {
                            server_id: input.server_id.clone(),
                            account_id: account_id.clone(),
                            remote_path: remote_path.clone(),
                            confirmed: true,
                        },
                    )
                    .await;
                    match upload {
                        Ok(value) => action_result.output.push_str(&format!(
                            "\n[BACKUP_UPLOAD_OK]\t{}\t{} bytes",
                            value.kind, value.bytes
                        )),
                        Err(error) => action_result.output.push_str(&format!(
                            "\n[BACKUP_UPLOAD_FAILED]\t{}",
                            crate::security::redact(&error.message)
                        )),
                    }
                }
            } else {
                action_result
                    .output
                    .push_str("\n[BACKUP_UPLOAD_SKIPPED]\t未找到归档 marker");
            }
        }
    }
    if input.action == "run" {
        if let Err(error) = crate::domain::cronjob::send_execution_report(
            &state.local,
            &state.credentials,
            &input.server_id,
            &input.id,
            &result,
        )
        .await
        {
            if let Ok(action_result) = result.as_mut() {
                action_result.output.push_str(&format!(
                    "\n[CRON_REPORT_FAILED]\t{}",
                    crate::security::redact(&error.message)
                ));
            } else {
                tracing::warn!(
                    error = %error,
                    server_id = %input.server_id,
                    "计划任务报告通知失败"
                );
            }
        }
    }
    if input.action == "run" {
        if let Err(error) = crate::domain::cronjob::record_history(
            &state.local,
            &input.server_id,
            &input.id,
            &input.action,
            started_at,
            &result,
        )
        .await
        {
            tracing::warn!(error = %error, server_id = %input.server_id, "计划任务执行历史写入失败");
        }
    }
    write_audit(
        &state,
        Some(&input.server_id),
        &format!("cronjob_{}", input.action),
        "cronjob",
        Some(&input.id),
        if result.is_ok() { "success" } else { "failed" },
        format!("计划任务 {}", input.action),
    )
    .await;
    result
}

/// 读取指定服务器最近的本地计划任务执行历史，不访问远端。
#[tauri::command]
pub async fn get_cronjob_history(
    server_id: String,
    state: State<'_, AppState>,
) -> AppResult<Vec<crate::domain::cronjob::CronJobHistoryEntry>> {
    crate::domain::cronjob::history(&state.local, &server_id).await
}

/// 清除指定服务器的本地计划任务执行历史，不执行任何远端操作。
#[tauri::command]
pub async fn clear_cronjob_history(server_id: String, state: State<'_, AppState>) -> AppResult<()> {
    let result = crate::domain::cronjob::clear_history(&state.local, &server_id).await;
    write_audit(
        &state,
        Some(&server_id),
        "cronjob_history_clear",
        "cronjob",
        None,
        if result.is_ok() { "success" } else { "failed" },
        "计划任务本地执行历史已清除".into(),
    )
    .await;
    result
}

/// 读取当前服务器的计划任务报告通知设置；不会返回 webhook URL 或签名密钥。
#[tauri::command]
pub async fn get_cron_notification_settings(
    server_id: String,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::cronjob::CronNotificationSettings> {
    crate::domain::cronjob::notification_settings(&state.local, &state.credentials, &server_id)
        .await
}

/// 保存当前服务器的计划任务报告通知策略，并记录不含敏感字段的审计摘要。
#[tauri::command]
pub async fn save_cron_notification_settings(
    input: crate::domain::cronjob::SaveCronNotificationSettingsInput,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::cronjob::CronNotificationSettings> {
    let result = crate::domain::cronjob::save_notification_settings(
        &state.local,
        &state.credentials,
        input.clone(),
    )
    .await;
    write_audit(
        &state,
        Some(&input.server_id),
        "cron_notification_save",
        "cronjob",
        None,
        if result.is_ok() { "success" } else { "failed" },
        "计划任务报告通知设置已保存".into(),
    )
    .await;
    result
}

/// 读取全局离线归档补传调度器设置；不会访问任何服务器。
#[tauri::command]
pub async fn get_cron_offline_scheduler_settings(
    state: State<'_, AppState>,
) -> AppResult<crate::domain::cronjob::CronOfflineSchedulerSettings> {
    crate::domain::cronjob::offline_scheduler_settings(&state.local).await
}

/// 保存全局离线归档补传调度器设置，并记录不含凭据的本地审计摘要。
#[tauri::command]
pub async fn save_cron_offline_scheduler_settings(
    input: crate::domain::cronjob::SaveCronOfflineSchedulerSettingsInput,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::cronjob::CronOfflineSchedulerSettings> {
    let result =
        crate::domain::cronjob::save_offline_scheduler_settings(&state.local, input.clone()).await;
    write_audit(
        &state,
        None,
        "cron_offline_scheduler_save",
        "cronjob",
        None,
        if result.is_ok() { "success" } else { "failed" },
        if input.enabled {
            "离线归档补传已启用".into()
        } else {
            "离线归档补传已停用".into()
        },
    )
    .await;
    result
}

/// 读取远程防火墙和 SSH 有效安全配置。
#[tauri::command]
pub async fn get_security(
    server_id: String,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::security::SecuritySnapshot> {
    crate::domain::security::snapshot(&state.ssh, &server_id).await
}

/// 在用户确认后添加或删除一条远程防火墙规则，并审计结果。
#[tauri::command]
pub async fn firewall_rule_action(
    input: crate::domain::security::FirewallRuleInput,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::security::FirewallSnapshot> {
    let result = crate::domain::security::firewall_rule_action(&state.ssh, input.clone()).await;
    audit_outcome(
        &state,
        Some(&input.server_id),
        &format!("firewall_{}", input.action),
        "firewall_rule",
        Some(&input.port),
        &result,
        format!("防火墙规则 {} {}", input.action, input.port),
    )
    .await;
    result
}

/// 在用户确认后安全更新 sshd_config，并验证 sshd -t 和 reload。
#[tauri::command]
pub async fn save_ssh_security(
    input: crate::domain::security::SshSecurityInput,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::security::SshSecurityConfig> {
    let result = crate::domain::security::save_ssh_config(&state.ssh, input.clone()).await;
    audit_outcome(
        &state,
        Some(&input.server_id),
        "save_ssh_security",
        "ssh_config",
        Some("/etc/ssh/sshd_config"),
        &result,
        "SSH 安全配置已更新".into(),
    )
    .await;
    result
}

/// 读取远程 Nginx/OpenResty 受控站点配置和证书摘要。
#[tauri::command]
pub async fn get_websites(
    server_id: String,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::website::WebsiteSnapshot> {
    crate::domain::website::snapshot(&state.ssh, &server_id).await
}

/// 计算启用的 HTTPS 站点里需要签发或续期的证书批量规划。
#[tauri::command]
pub async fn get_certificate_renewal_plan(
    server_id: String,
    renew_before_days: u32,
    state: State<'_, AppState>,
) -> AppResult<Vec<crate::domain::website::CertificateRenewalPlan>> {
    let snapshot = crate::domain::website::snapshot(&state.ssh, &server_id).await?;
    Ok(crate::domain::website::certificate_renewal_plan(
        &snapshot,
        renew_before_days,
    ))
}

/// 创建或替换静态/反向代理站点，并在本地写入审计事件。
#[tauri::command]
pub async fn save_website(
    input: crate::domain::website::SaveWebsiteInput,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::website::WebsiteSnapshot> {
    let result = crate::domain::website::save(&state.ssh, input.clone()).await;
    audit_outcome(
        &state,
        Some(&input.server_id),
        "save_website",
        "website",
        Some(&input.domain),
        &result,
        format!("保存网站：{}", input.domain),
    )
    .await;
    result
}

/// 启用、停用或删除客户端管理的网站配置。
#[tauri::command]
pub async fn website_action(
    input: crate::domain::website::WebsiteActionInput,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::website::WebsiteSnapshot> {
    let result = crate::domain::website::action(&state.ssh, input.clone()).await;
    audit_outcome(
        &state,
        Some(&input.server_id),
        &format!("website_{}", input.action),
        "website",
        Some(&input.domain),
        &result,
        format!("网站操作：{} / {}", input.domain, input.action),
    )
    .await;
    result
}

/// 申请或续期 HTTP-01 ACME 证书，并记录不含私钥内容的审计结果。
#[tauri::command]
pub async fn website_certificate_action(
    input: crate::domain::website::CertificateActionInput,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::website::CertificateActionResult> {
    let result = crate::domain::website::certificate_action(&state.ssh, input.clone()).await;
    audit_outcome(
        &state,
        Some(&input.server_id),
        &format!("certificate_{}", input.action),
        "website_certificate",
        Some(&input.domain),
        &result,
        format!("网站证书操作：{} / {}", input.domain, input.action),
    )
    .await;
    result
}

/// 将签发后的证书绑定到同域客户端受控站点，并记录脱敏审计事件。
#[tauri::command]
pub async fn bind_website_certificate(
    input: crate::domain::website::BindWebsiteCertificateInput,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::website::WebsiteSnapshot> {
    let result = crate::domain::website::bind_certificate(&state.ssh, input.clone()).await;
    audit_outcome(
        &state,
        Some(&input.server_id),
        "bind_website_certificate",
        "website_certificate",
        Some(&input.domain),
        &result,
        format!("绑定网站证书：{}", input.domain),
    )
    .await;
    result
}

/// 返回 PHP-FPM 固定包安装计划，不执行远端写入。
#[tauri::command]
pub async fn get_php_install_plan(
    server_id: String,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::website::PhpInstallPlan> {
    crate::domain::website::php_install_plan(&state.ssh, &server_id).await
}

/// 在用户确认后安装 PHP-FPM，并写入脱敏审计事件。
#[tauri::command]
pub async fn install_php_runtime(
    input: crate::domain::website::PhpInstallInput,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::website::PhpInstallResult> {
    let result = crate::domain::website::php_install(&state.ssh, input.clone()).await;
    audit_outcome(
        &state,
        Some(&input.server_id),
        "php_runtime_install",
        "website_runtime",
        None,
        &result,
        "安装 PHP-FPM 运行时".into(),
    )
    .await;
    result
}

/// 读取真实 WAF 编译能力与远程 HTTP 探活工具状态。
#[tauri::command]
pub async fn get_advanced(
    server_id: String,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::advanced::AdvancedSnapshot> {
    crate::domain::advanced::snapshot(&state.ssh, &server_id).await
}

/// 从远端服务器发起一次受控 HTTP 探活并写入脱敏审计结果。
#[tauri::command]
pub async fn probe_http_monitor(
    input: crate::domain::advanced::HttpMonitorInput,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::advanced::HttpMonitorResult> {
    let result = crate::domain::advanced::probe_http(&state.ssh, input.clone()).await;
    audit_outcome(
        &state,
        Some(&input.server_id),
        "probe_http_monitor",
        "website_monitor",
        Some(&input.url),
        &result,
        format!("网站探活：{}", input.url),
    )
    .await;
    result
}

/// Lists persisted HTTP monitor definitions for the selected server.
#[tauri::command]
pub async fn get_http_monitors(
    server_id: String,
    state: State<'_, AppState>,
) -> AppResult<Vec<crate::domain::advanced::HttpMonitorProfile>> {
    state.local.list_http_monitors(Some(&server_id)).await
}

/// Saves a validated HTTP monitor definition; the scheduler will pick it up on its next tick.
#[tauri::command]
pub async fn save_http_monitor(
    input: crate::domain::advanced::SaveHttpMonitorInput,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::advanced::HttpMonitorProfile> {
    crate::domain::advanced::validate_http_monitor(&input)?;
    let result = state.local.save_http_monitor(&input).await;
    audit_outcome(
        &state,
        Some(&input.server_id),
        "save_http_monitor",
        "website_monitor",
        input.id.as_deref(),
        &result,
        format!("保存网站探活任务：{}", input.name.trim()),
    )
    .await;
    result
}

/// Deletes a persisted HTTP monitor and its local history without changing remote configuration.
#[tauri::command]
pub async fn delete_http_monitor(monitor_id: String, state: State<'_, AppState>) -> AppResult<()> {
    let profile = state.local.http_monitor_by_id(&monitor_id).await?;
    let result = state.local.delete_http_monitor(&monitor_id).await;
    audit_outcome(
        &state,
        Some(&profile.server_id),
        "delete_http_monitor",
        "website_monitor",
        Some(&monitor_id),
        &result,
        format!("删除网站探活任务：{}", profile.name),
    )
    .await;
    result
}

/// Runs one saved HTTP monitor immediately and records its result for the history panel.
#[tauri::command]
pub async fn run_http_monitor(
    monitor_id: String,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::advanced::HttpMonitorResult> {
    let profile = state.local.http_monitor_by_id(&monitor_id).await?;
    let result =
        crate::domain::advanced::run_saved_monitor(&state.ssh, &state.local, &monitor_id).await;
    audit_outcome(
        &state,
        Some(&profile.server_id),
        "run_http_monitor",
        "website_monitor",
        Some(&monitor_id),
        &result,
        format!("执行网站探活任务：{}", profile.name),
    )
    .await;
    result
}

/// Returns recent bounded checks for one monitor; response bodies are never persisted or returned.
#[tauri::command]
pub async fn get_http_monitor_history(
    monitor_id: String,
    state: State<'_, AppState>,
) -> AppResult<Vec<crate::domain::advanced::HttpMonitorCheck>> {
    state.local.http_monitor_checks(&monitor_id, 100).await
}

/// 读取远端 ModSecurity 规则摘要，不返回完整配置文件。
#[tauri::command]
pub async fn get_waf_rules(
    server_id: String,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::advanced::WafRulesSnapshot> {
    crate::domain::advanced::waf_rules(&state.ssh, &server_id).await
}

/// 读取固定第三方 WAF 规则源、安装版本和远端可控能力。
#[tauri::command]
pub async fn get_waf_rule_sources(
    server_id: String,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::advanced::WafRuleSourcesSnapshot> {
    crate::domain::advanced::waf_rule_sources(&state.ssh, &server_id).await
}

/// Returns fixed WAF strategy templates without contacting or modifying a server.
#[tauri::command]
pub async fn get_waf_templates() -> AppResult<Vec<crate::domain::advanced::WafTemplate>> {
    Ok(crate::domain::advanced::waf_templates())
}

/// Reads recent bounded ModSecurity denial summaries from fixed remote log paths.
#[tauri::command]
pub async fn get_waf_alerts(
    server_id: String,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::advanced::WafAlertsSnapshot> {
    crate::domain::advanced::waf_alerts(&state.ssh, &state.local, &state.credentials, &server_id)
        .await
}

/// 读取指定服务器的本地 WAF 告警阈值和应用内通知设置。
#[tauri::command]
pub async fn get_waf_alert_settings(
    server_id: String,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::advanced::WafAlertSettings> {
    crate::domain::advanced::get_waf_alert_settings(&state.local, &state.credentials, &server_id)
        .await
}

/// 保存指定服务器的本地 WAF 告警阈值和历史容量设置。
#[tauri::command]
pub async fn save_waf_alert_settings(
    server_id: String,
    input: crate::domain::advanced::SaveWafAlertSettingsInput,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::advanced::WafAlertSettings> {
    crate::domain::advanced::save_waf_alert_settings(
        &state.local,
        &state.credentials,
        &server_id,
        input,
    )
    .await
}

/// 清空指定服务器的本地 WAF 告警历史，不修改远端日志。
#[tauri::command]
pub async fn clear_waf_alert_history(
    server_id: String,
    state: State<'_, AppState>,
) -> AppResult<()> {
    crate::domain::advanced::clear_waf_alert_history(&state.local, &server_id).await
}

/// 增删 WAF 规则并记录脱敏审计结果。
#[tauri::command]
pub async fn waf_rule_action(
    input: crate::domain::advanced::WafRuleActionInput,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::advanced::WafRulesSnapshot> {
    let result = crate::domain::advanced::waf_rule_action(&state.ssh, input.clone()).await;
    audit_outcome(
        &state,
        Some(&input.server_id),
        &format!("waf_rule_{}", input.action),
        "waf",
        input.line_number.map(|line| line.to_string()).as_deref(),
        &result,
        format!("WAF 规则操作：{}", input.action),
    )
    .await;
    result
}

/// Applies one fixed WAF strategy and records only the template identifier in the audit trail.
#[tauri::command]
pub async fn waf_template_action(
    input: crate::domain::advanced::WafTemplateActionInput,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::advanced::WafRulesSnapshot> {
    let result = crate::domain::advanced::waf_template_action(&state.ssh, input.clone()).await;
    audit_outcome(
        &state,
        Some(&input.server_id),
        "waf_template_action",
        "waf",
        Some(&input.template_id),
        &result,
        "应用 WAF 内置策略模板".into(),
    )
    .await;
    result
}

/// 安装、更新或移除固定第三方 WAF 规则集，并记录脱敏审计结果。
#[tauri::command]
pub async fn waf_rule_source_action(
    input: crate::domain::advanced::WafRuleSourceActionInput,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::advanced::WafRuleSourceActionResult> {
    let result = crate::domain::advanced::waf_rule_source_action(&state.ssh, input.clone()).await;
    audit_outcome(
        &state,
        Some(&input.server_id),
        &format!("waf_source_{}", input.action),
        "waf",
        Some(&input.source_id),
        &result,
        format!("第三方 WAF 规则源操作：{}", input.action),
    )
    .await;
    result
}

/// 从选定的 1Panel 应用仓库读取应用目录，并复用本地缓存；不会触碰任何远端服务器。
#[tauri::command]
pub async fn get_app_catalog(
    state: State<'_, AppState>,
) -> AppResult<crate::domain::appstore::AppCatalogSnapshot> {
    crate::domain::appstore::catalog(&state.local, &state.credentials).await
}

/// 读取一个应用的所选来源 metadata 和可安装版本。
#[tauri::command]
pub async fn get_app_detail(
    key: String,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::appstore::AppDetail> {
    crate::domain::appstore::detail(&state.local, &state.credentials, &key).await
}

/// 读取本地应用商店来源和缓存设置，不包含远端服务器凭据。
#[tauri::command]
pub async fn get_appstore_settings(
    state: State<'_, AppState>,
) -> AppResult<crate::domain::appstore::AppStoreSettings> {
    crate::domain::appstore::settings(&state.local, &state.credentials).await
}

/// 保存应用商店来源设置，切换镜像或离线模式立即对后续请求生效。
#[tauri::command]
pub async fn save_appstore_settings(
    input: crate::domain::appstore::SaveAppStoreSettingsInput,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::appstore::AppStoreSettings> {
    crate::domain::appstore::save_settings(&state.local, &state.credentials, input).await
}

/// Generates a signed static application-store mirror on the local machine.
#[tauri::command]
pub async fn generate_appstore_mirror(
    input: crate::domain::appstore::GenerateAppStoreMirrorInput,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::appstore::AppStoreMirrorGenerationResult> {
    crate::domain::appstore::generate_mirror(&state.local, &state.credentials, input).await
}

/// 清理本地应用商店目录和详情缓存，不修改任何远端应用。
#[tauri::command]
pub async fn clear_appstore_cache(
    state: State<'_, AppState>,
) -> AppResult<crate::domain::appstore::AppStoreCacheClearResult> {
    crate::domain::appstore::clear_cache(&state.local).await
}

/// 探测远端 1Panel 应用目录和 Docker Compose 状态。
#[tauri::command]
pub async fn get_installed_apps(
    server_id: String,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::appstore::InstalledAppsSnapshot> {
    crate::domain::appstore::installed(&state.ssh, &server_id).await
}

/// 读取指定 Compose 应用的服务健康状态并写入只读审计摘要。
#[tauri::command]
pub async fn get_app_health(
    input: crate::domain::appstore::AppHealthInput,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::appstore::AppHealthSnapshot> {
    let result = crate::domain::appstore::health(&state.ssh, input.clone()).await;
    audit_outcome(
        &state,
        Some(&input.server_id),
        "app_health",
        "appstore",
        Some(&input.project),
        &result,
        format!("读取应用健康状态：{}", input.project),
    )
    .await;
    result
}

/// 读取当前应用商店来源最新 Compose 与已安装版本的差异摘要，并写入只读审计事件。
#[tauri::command]
pub async fn app_update_preview(
    input: crate::domain::appstore::AppUpdatePreviewInput,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::appstore::AppUpdatePreview> {
    let result = crate::domain::appstore::update_preview(
        &state.ssh,
        &state.local,
        &state.credentials,
        input.clone(),
    )
    .await;
    audit_outcome(
        &state,
        Some(&input.server_id),
        "app_update_preview",
        "appstore",
        Some(&input.key),
        &result,
        format!("预览应用升级差异：{}", input.key),
    )
    .await;
    result
}

/// 读取已安装应用的环境变量键摘要，不回传远端秘密值。
#[tauri::command]
pub async fn get_app_environment(
    server_id: String,
    install_path: String,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::appstore::AppEnvironmentSnapshot> {
    crate::domain::appstore::environment(&state.ssh, &server_id, &install_path).await
}

/// 合并保存应用环境变量，并写入脱敏审计事件。
#[tauri::command]
pub async fn save_app_environment(
    input: crate::domain::appstore::AppEnvironmentInput,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::appstore::AppActionResult> {
    let result = crate::domain::appstore::save_environment(&state.ssh, input.clone()).await;
    audit_outcome(
        &state,
        Some(&input.server_id),
        "app_environment",
        "appstore",
        Some(&input.install_path),
        &result,
        "合并保存应用环境变量".into(),
    )
    .await;
    result
}

/// 下载并安装当前来源的 Compose 应用，所有远端写入和启动结果都会审计。
#[tauri::command]
pub async fn install_app(
    input: crate::domain::appstore::InstallAppInput,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::appstore::AppActionResult> {
    let server_id = input.server_id.clone();
    let key = input.key.clone();
    let result =
        crate::domain::appstore::install(&state.ssh, &state.local, &state.credentials, input).await;
    audit_outcome(
        &state,
        Some(&server_id),
        "app_install",
        "app",
        Some(&key),
        &result,
        format!("安装应用：{key}"),
    )
    .await;
    result
}

/// 执行应用 Compose 生命周期操作，并要求破坏性动作显式确认。
#[tauri::command]
pub async fn app_action(
    input: crate::domain::appstore::AppActionInput,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::appstore::AppActionResult> {
    let server_id = input.server_id.clone();
    let key = input.key.clone();
    let action = input.action.clone();
    let result =
        crate::domain::appstore::action(&state.ssh, &state.local, &state.credentials, input).await;
    audit_outcome(
        &state,
        Some(&server_id),
        &format!("app_{action}"),
        "app",
        Some(&key),
        &result,
        format!("应用操作：{key} / {action}"),
    )
    .await;
    result
}

/// 读取本地 AI 供应商配置和密钥存在状态。
#[tauri::command]
pub async fn list_ai_providers(
    state: State<'_, AppState>,
) -> AppResult<Vec<crate::domain::ai::AiProvider>> {
    crate::domain::ai::list(&state.local, &state.credentials).await
}

/// 保存 AI 供应商元数据，API key 只进入操作系统密钥链。
#[tauri::command]
pub async fn save_ai_provider(
    input: crate::domain::ai::SaveAiProviderInput,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::ai::AiProvider> {
    crate::domain::ai::save(&state.local, &state.credentials, input).await
}

/// 删除本地 AI 供应商和对应密钥链条目。
#[tauri::command]
pub async fn delete_ai_provider(
    input: crate::domain::ai::DeleteAiProviderInput,
    state: State<'_, AppState>,
) -> AppResult<()> {
    crate::domain::ai::delete(&state.local, &state.credentials, input).await
}

/// 读取本地 AI 对话历史；消息正文只返回给当前本地客户端，不会写入审计摘要。
#[tauri::command]
pub async fn list_ai_conversations(
    provider_id: Option<String>,
    state: State<'_, AppState>,
) -> AppResult<Vec<crate::domain::ai::AiConversation>> {
    crate::domain::ai::list_conversations(&state.local, provider_id.as_deref()).await
}

/// 保存本地 AI 对话历史，API key 仍只存在系统密钥链。
#[tauri::command]
pub async fn save_ai_conversation(
    input: crate::domain::ai::SaveAiConversationInput,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::ai::AiConversation> {
    crate::domain::ai::save_conversation(&state.local, input).await
}

/// 删除单条本地 AI 对话历史，不影响供应商配置或密钥链。
#[tauri::command]
pub async fn delete_ai_conversation(
    input: crate::domain::ai::DeleteAiConversationInput,
    state: State<'_, AppState>,
) -> AppResult<()> {
    crate::domain::ai::delete_conversation(&state.local, input).await
}

/// 清理本地 AI 对话历史，可按供应商范围执行。
#[tauri::command]
pub async fn clear_ai_conversations(
    provider_id: Option<String>,
    state: State<'_, AppState>,
) -> AppResult<()> {
    crate::domain::ai::clear_conversations(&state.local, provider_id.as_deref()).await
}

/// 读取真实 AI 供应商的模型列表，不返回 API key 或未约定的响应字段。
#[tauri::command]
pub async fn ai_models(
    provider_id: String,
    state: State<'_, AppState>,
) -> AppResult<Vec<crate::domain::ai::AiModel>> {
    crate::domain::ai::models(&state.local, &state.credentials, &provider_id).await
}

/// 向配置的 OpenAI-compatible 模型发送一次非流式聊天请求。
#[tauri::command]
pub async fn ai_chat(
    input: crate::domain::ai::AiChatInput,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::ai::AiChatResult> {
    crate::domain::ai::chat(&state.local, &state.credentials, input).await
}

/// 向配置的 OpenAI-compatible 模型发送 SSE 流式聊天，并把增量文本转发到前端 Channel。
#[tauri::command]
pub async fn ai_chat_stream(
    input: crate::domain::ai::AiChatInput,
    on_event: Channel<crate::domain::ai::AiStreamEvent>,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::ai::AiChatResult> {
    crate::domain::ai::stream_chat(
        &state.ssh,
        &state.local,
        &state.credentials,
        input,
        &on_event,
    )
    .await
}

/// Runs a bounded read-only AI agent; tool calls are restricted to the selected server overview.
#[tauri::command]
pub async fn ai_agent(
    input: crate::domain::ai::AiAgentInput,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::ai::AiAgentResult> {
    let result =
        crate::domain::ai::agent(&state.ssh, &state.local, &state.credentials, input.clone()).await;
    audit_outcome(
        &state,
        Some(&input.server_id),
        "ai_agent",
        "ai",
        Some(&input.provider_id),
        &result,
        "AI 只读智能体执行".into(),
    )
    .await;
    result
}

/// 读取本地 MCP 服务器配置；远程认证只返回是否已配置，不返回令牌。
#[tauri::command]
pub async fn list_ai_mcp_servers(
    state: State<'_, AppState>,
) -> AppResult<Vec<crate::domain::ai::mcp::McpServerConfig>> {
    crate::domain::ai::mcp::list(&state.local, &state.credentials).await
}

/// 保存 MCP 服务器配置，stdio 命令以无 shell 的子进程方式执行。
#[tauri::command]
pub async fn save_ai_mcp_server(
    input: crate::domain::ai::mcp::SaveMcpServerInput,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::ai::mcp::McpServerConfig> {
    crate::domain::ai::mcp::save(&state.local, &state.credentials, input).await
}

/// 删除一个本地 MCP 服务器配置，不影响远端服务器。
#[tauri::command]
pub async fn delete_ai_mcp_server(
    input: crate::domain::ai::mcp::DeleteMcpServerInput,
    state: State<'_, AppState>,
) -> AppResult<()> {
    crate::domain::ai::mcp::delete(&state.local, &state.credentials, input).await
}

/// 启动指定 MCP 服务器并真实执行 initialize/tools/list 探测。
#[tauri::command]
pub async fn probe_ai_mcp_server(
    server_id: String,
    state: State<'_, AppState>,
) -> AppResult<crate::domain::ai::mcp::McpProbeResult> {
    crate::domain::ai::mcp::probe(&state.local, &state.credentials, &server_id).await
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use crate::domain::backup_accounts::BackupAccount;
    use crate::domain::cronjob::CronJobExportEntry;

    use super::{
        overview_memo_key, resolve_import_backup_accounts, validate_overview_memo_content,
        validate_overview_server_id,
    };

    #[test]
    fn bounds_overview_memo_keys_and_text() {
        validate_overview_server_id("server_01").unwrap();
        assert!(validate_overview_server_id("server/01").is_err());
        assert!(validate_overview_memo_content("line 1\nline 2\tok").is_ok());
        assert!(validate_overview_memo_content(&"x".repeat(4001)).is_err());
        assert_eq!(overview_memo_key("server_01"), "overview.memo.server_01");
    }

    /// 验证任务导入只恢复当前客户端可用账号，并统计重复或失效引用。
    #[test]
    fn filters_import_backup_account_references() {
        let valid_local = BackupAccount {
            id: "11111111-1111-4111-8111-111111111111".into(),
            name: "local".into(),
            kind: "local".into(),
            server_id: Some("server-1".into()),
            endpoint: None,
            remote_path: "/tmp".into(),
            bucket: None,
            region: None,
            username: None,
            private_key_path: None,
            host_key_fingerprint: None,
            has_secret: false,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let valid_s3 = BackupAccount {
            id: "22222222-2222-4222-8222-222222222222".into(),
            name: "s3".into(),
            kind: "s3".into(),
            server_id: None,
            endpoint: Some("https://s3.example.invalid".into()),
            remote_path: "backups".into(),
            bucket: Some("bucket".into()),
            region: Some("auto".into()),
            username: None,
            private_key_path: None,
            host_key_fingerprint: None,
            has_secret: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let missing_secret = BackupAccount {
            id: "33333333-3333-4333-8333-333333333333".into(),
            name: "missing".into(),
            kind: "webdav".into(),
            server_id: None,
            endpoint: Some("https://dav.example.invalid".into()),
            remote_path: "backups".into(),
            bucket: None,
            region: None,
            username: Some("user".into()),
            private_key_path: None,
            host_key_fingerprint: None,
            has_secret: false,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let mut jobs = vec![CronJobExportEntry {
            id: "job".into(),
            schedule: "0 2 * * *".into(),
            command: "echo ok".into(),
            kind: "directory".into(),
            user: "root".into(),
            managed: true,
            enabled: true,
            retention_count: None,
            retention_days: None,
            backup_account_ids: vec![
                valid_local.id.clone(),
                valid_s3.id.clone(),
                valid_s3.id.clone(),
                missing_secret.id.clone(),
                "not-a-uuid".into(),
            ],
            default_backup_account_id: Some(missing_secret.id.clone()),
            backup_event_path: None,
        }];

        let unresolved = resolve_import_backup_accounts(
            "server-1",
            &mut jobs,
            &[valid_local, valid_s3, missing_secret],
        );
        assert_eq!(unresolved, 3);
        assert_eq!(jobs[0].backup_account_ids.len(), 2);
        assert!(jobs[0].default_backup_account_id.is_none());
    }
}

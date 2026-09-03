use crate::domain::server::ServerRecord;
use crate::errors::{AppError, AppResult};
use crate::infra::db::{KnownHost, ServerRepository};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use russh::client;
use russh::keys::agent::client::{AgentClient, AgentStream};
use russh::keys::agent::AgentIdentity;
use russh::keys::ssh_key::Algorithm;
use russh::keys::ssh_key::{HashAlg, PublicKey};
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::ipc::Channel;
use tokio::sync::{mpsc, Notify, Semaphore};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostKeyChallenge {
    pub server_id: String,
    pub host: String,
    pub port: u16,
    pub key_type: String,
    pub fingerprint: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrustHostKeyInput {
    pub server_id: String,
    pub host: String,
    pub port: u16,
    pub key_type: String,
    pub fingerprint: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionSnapshot {
    pub server_id: String,
    pub status: ConnectionStatus,
    pub connected_at: Option<DateTime<Utc>>,
    pub error: Option<AppError>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionStatus {
    Offline,
    Connecting,
    Online,
    Error,
}

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum ConnectOutcome {
    Connected(ConnectionSnapshot),
    HostKey(HostKeyChallenge),
}

#[derive(Debug, Clone)]
struct PendingHostKey {
    server_id: String,
    host: String,
    port: u16,
    key_type: String,
    fingerprint: String,
    public_key: String,
    observed_at: Instant,
}

struct ManagedConnection {
    handle: client::Handle<HostKeyHandler>,
    connected_at: DateTime<Utc>,
    limiter: Arc<Semaphore>,
}

struct CommandTaskState {
    cancelled: AtomicBool,
    notify: Notify,
}

#[derive(Clone, Default)]
pub struct CommandTaskManager {
    tasks: Arc<DashMap<String, Arc<CommandTaskState>>>,
}

impl CommandTaskManager {
    /// 注册一个可由 UI 取消的远程流式命令任务。
    pub fn begin(&self, task_id: &str) -> AppResult<CommandTaskGuard> {
        if task_id.is_empty() || self.tasks.contains_key(task_id) {
            return Err(AppError::new(
                "TASK_ID_INVALID",
                "task",
                "任务标识无效或已存在",
            ));
        }
        let state = Arc::new(CommandTaskState {
            cancelled: AtomicBool::new(false),
            notify: Notify::new(),
        });
        self.tasks.insert(task_id.to_string(), state.clone());
        Ok(CommandTaskGuard {
            task_id: task_id.to_string(),
            state,
            manager: self.clone(),
        })
    }

    /// 标记流式远程任务取消，并唤醒正在等待 SSH 输出的执行器。
    pub fn cancel(&self, task_id: &str) -> AppResult<()> {
        let state = self
            .tasks
            .get(task_id)
            .ok_or_else(|| AppError::new("TASK_NOT_FOUND", "task", "远程任务不存在或已结束"))?;
        state.cancelled.store(true, Ordering::Release);
        state.notify.notify_one();
        Ok(())
    }
}

pub struct CommandTaskGuard {
    task_id: String,
    state: Arc<CommandTaskState>,
    manager: CommandTaskManager,
}

impl CommandTaskGuard {
    /// 返回任务取消状态，使 SSH channel 能在输出等待期间立即退出。
    fn state(&self) -> Arc<CommandTaskState> {
        self.state.clone()
    }

    /// 返回任务是否已被 UI 标记取消，供非 SSH 流式请求复用同一取消协议。
    pub fn is_cancelled(&self) -> bool {
        self.state.cancelled.load(Ordering::Acquire)
    }

    /// 等待任务取消通知；调用方应同时检查 `is_cancelled` 处理已发出的通知。
    pub async fn wait_cancelled(&self) {
        self.state.notify.notified().await;
    }
}

impl Drop for CommandTaskGuard {
    fn drop(&mut self) {
        self.manager.tasks.remove(&self.task_id);
    }
}

#[derive(Clone)]
pub struct SshConnectionManager {
    repository: ServerRepository,
    connections: Arc<DashMap<String, Arc<ManagedConnection>>>,
    pending: Arc<DashMap<String, PendingHostKey>>,
    connecting: Arc<DashMap<String, ()>>,
    errors: Arc<DashMap<String, AppError>>,
    terminals: Arc<DashMap<String, ManagedTerminal>>,
    tasks: CommandTaskManager,
}

impl SshConnectionManager {
    /// 创建进程内 SSH 会话管理器，并为每个服务器保留独立连接、终端和错误状态。
    pub fn new(repository: ServerRepository) -> Self {
        Self {
            repository,
            connections: Arc::new(DashMap::new()),
            pending: Arc::new(DashMap::new()),
            connecting: Arc::new(DashMap::new()),
            errors: Arc::new(DashMap::new()),
            terminals: Arc::new(DashMap::new()),
            tasks: CommandTaskManager::default(),
        }
    }

    /// 取消一个由流式安装、拉取或日志任务注册的远程命令。
    pub fn cancel_task(&self, task_id: &str) -> AppResult<()> {
        self.tasks.cancel(task_id)
    }

    /// 注册一个可取消的通用流式任务，供 AI 等非 SSH 请求复用任务中心。
    pub fn begin_command_task(&self, task_id: &str) -> AppResult<CommandTaskGuard> {
        self.tasks.begin(task_id)
    }

    /// 返回连接快照，并把最近一次可恢复的连接错误暴露给 UI。
    pub fn snapshot(&self, server_id: &str) -> ConnectionSnapshot {
        let closed = self
            .connections
            .get(server_id)
            .map(|connection| connection.handle.is_closed())
            .unwrap_or(false);
        if closed {
            self.connections.remove(server_id);
            self.errors
                .entry(server_id.to_string())
                .or_insert_with(|| connection_lost_error(server_id));
        }
        let (status, connected_at, error) =
            if let Some(connection) = self.connections.get(server_id) {
                (
                    ConnectionStatus::Online,
                    Some(connection.connected_at),
                    None,
                )
            } else if self.connecting.contains_key(server_id) {
                (ConnectionStatus::Connecting, None, None)
            } else if let Some(error) = self.errors.get(server_id) {
                (ConnectionStatus::Error, None, Some(error.clone()))
            } else {
                (ConnectionStatus::Offline, None, None)
            };
        ConnectionSnapshot {
            server_id: server_id.to_string(),
            status,
            connected_at,
            error,
        }
    }

    /// 建立复用 SSH 会话，并记录最后一次连接失败的结构化错误。
    pub async fn connect(&self, server_id: &str) -> AppResult<ConnectOutcome> {
        if self.connections.contains_key(server_id) {
            return Ok(ConnectOutcome::Connected(self.snapshot(server_id)));
        }
        if self.connecting.insert(server_id.to_string(), ()).is_some() {
            return Err(
                AppError::new("SSH_CONNECT_IN_PROGRESS", "ssh", "该服务器正在连接")
                    .for_server(server_id),
            );
        }
        let result = self.connect_inner(server_id).await;
        self.connecting.remove(server_id);
        match &result {
            Ok(_) => {
                self.errors.remove(server_id);
            }
            Err(error) => {
                self.errors.insert(server_id.to_string(), error.clone());
            }
        }
        result
    }

    /// 断开旧会话后以有限次数重试连接；Host Key 挑战或不可恢复错误会立即返回。
    pub async fn reconnect(&self, server_id: &str) -> AppResult<ConnectOutcome> {
        let _ = self.disconnect(server_id).await;
        let delays = [Duration::from_millis(250), Duration::from_millis(500)];
        for attempt in 0..=delays.len() {
            match self.connect(server_id).await {
                Ok(outcome) => return Ok(outcome),
                Err(error) if attempt < delays.len() && is_retryable_connect_error(error.code) => {
                    tokio::time::sleep(delays[attempt]).await;
                }
                Err(error) => return Err(error),
            }
        }
        Err(AppError::new("SSH_RECONNECT_FAILED", "ssh", "SSH 重连失败").for_server(server_id))
    }

    /// 建立直接或经多级 ProxyJump 的 SSH 会话，并保留 Host Key 挑战状态供 UI 确认。
    async fn connect_inner(&self, server_id: &str) -> AppResult<ConnectOutcome> {
        let record = self.repository.record(server_id).await?;
        let proxy_jump_id = record.proxy_jump_id();
        self.repository
            .proxy_jump_chain(server_id, proxy_jump_id.as_deref())
            .await
            .map_err(|error| error.for_server(server_id))?;
        let identity = format!("{}:{}", record.host, record.port);
        let known = self.repository.known_host(&identity).await?;
        let observation = Arc::new(Mutex::new(None));
        let handler = HostKeyHandler {
            expected: known,
            observation: observation.clone(),
        };
        let config = client::Config {
            // connect_timeout 只限制首次握手；不能拿它作为空闲超时，否则默认 10 秒会误杀长期会话。
            inactivity_timeout: None,
            keepalive_interval: Some(Duration::from_secs(record.keepalive().clamp(10, 300))),
            keepalive_max: 6,
            ..Default::default()
        };
        let connection = if let Some(proxy_jump_id) = proxy_jump_id {
            // 递归连接多级跳板时通过堆分配打断 async future 的自引用类型；每级跳板复用连接池。
            let jump_outcome = Box::pin(self.connect(&proxy_jump_id))
                .await
                .map_err(|error| {
                    AppError::new(
                        "SSH_PROXY_JUMP_FAILED",
                        "ssh",
                        "无法连接 ProxyJump 跳板服务器",
                    )
                    .details(error)
                    .for_server(server_id)
                })?;
            if let ConnectOutcome::HostKey(challenge) = jump_outcome {
                return Ok(ConnectOutcome::HostKey(challenge));
            }
            let jump_connection = self.connections.get(&proxy_jump_id).ok_or_else(|| {
                AppError::new(
                    "SSH_PROXY_JUMP_FAILED",
                    "ssh",
                    "ProxyJump 跳板连接未保持在线",
                )
                .for_server(server_id)
            })?;
            let channel = jump_connection
                .handle
                .channel_open_direct_tcpip(record.host.clone(), record.port as u32, "127.0.0.1", 0)
                .await
                .map_err(|error| {
                    AppError::new(
                        "SSH_PROXY_JUMP_FAILED",
                        "ssh",
                        "跳板服务器拒绝打开目标 TCP 通道",
                    )
                    .details(error)
                    .for_server(server_id)
                })?;
            drop(jump_connection);
            tokio::time::timeout(
                Duration::from_secs(record.connect_timeout().max(5)),
                client::connect_stream(Arc::new(config), channel.into_stream(), handler),
            )
            .await
        } else {
            let address = (record.host.as_str(), record.port as u16);
            tokio::time::timeout(
                Duration::from_secs(record.connect_timeout().max(5)),
                client::connect(Arc::new(config), address, handler),
            )
            .await
        };

        let mut handle = match connection {
            Err(_) => {
                return Err(AppError::new("NETWORK_TIMEOUT", "network", "SSH 连接超时")
                    .for_server(server_id)
                    .suggestion("检查主机地址、端口与网络连通性"))
            }
            Ok(Err(error)) => {
                let observed = observation.lock().ok().and_then(|value| value.clone());
                if let Some(HostKeyObservation::Unknown(value)) = observed {
                    let pending = PendingHostKey {
                        server_id: server_id.to_string(),
                        host: record.host.clone(),
                        port: record.port as u16,
                        key_type: value.key_type,
                        fingerprint: value.fingerprint,
                        public_key: value.public_key,
                        observed_at: Instant::now(),
                    };
                    let challenge = pending.challenge();
                    self.pending.insert(server_id.to_string(), pending);
                    return Ok(ConnectOutcome::HostKey(challenge));
                }
                if matches!(observed, Some(HostKeyObservation::Changed)) {
                    return Err(AppError::new(
                        "HOST_KEY_CHANGED",
                        "security",
                        "服务器 Host Key 已发生变化",
                    )
                    .for_server(server_id)
                    .suggestion("停止连接并通过可信渠道核对服务器身份")
                    .fatal());
                }
                return Err(map_connect_error(error, server_id));
            }
            Ok(Ok(handle)) => handle,
        };

        authenticate(&mut handle, &record, &self.repository).await?;
        let managed = Arc::new(ManagedConnection {
            handle,
            connected_at: Utc::now(),
            limiter: Arc::new(Semaphore::new(8)),
        });
        self.connections.insert(server_id.to_string(), managed);
        self.repository.mark_connected(server_id).await?;
        Ok(ConnectOutcome::Connected(self.snapshot(server_id)))
    }

    pub async fn trust(&self, input: TrustHostKeyInput) -> AppResult<ConnectionSnapshot> {
        let Some((_, pending)) = self.pending.remove(&input.server_id) else {
            return Err(AppError::new(
                "HOST_KEY_CHALLENGE_EXPIRED",
                "security",
                "Host Key 确认已过期，请重新连接",
            ));
        };
        if pending.observed_at.elapsed() > Duration::from_secs(300) {
            return Err(AppError::new(
                "HOST_KEY_CHALLENGE_EXPIRED",
                "security",
                "Host Key 确认已超过 5 分钟，请重新连接",
            ));
        }
        if pending.host != input.host
            || pending.port != input.port
            || pending.key_type != input.key_type
            || pending.fingerprint != input.fingerprint
        {
            return Err(AppError::new(
                "HOST_KEY_CHALLENGE_MISMATCH",
                "security",
                "Host Key 确认内容与握手结果不一致",
            )
            .fatal());
        }
        self.repository
            .trust_host(&KnownHost {
                server_identity: format!("{}:{}", pending.host, pending.port),
                key_type: pending.key_type,
                fingerprint: pending.fingerprint,
                public_key: pending.public_key,
            })
            .await?;
        match self.connect(&input.server_id).await? {
            ConnectOutcome::Connected(snapshot) => Ok(snapshot),
            ConnectOutcome::HostKey(_) => Err(AppError::new(
                "HOST_KEY_UNKNOWN",
                "security",
                "保存 Host Key 后仍无法验证服务器身份",
            )
            .fatal()),
        }
    }

    /// 主动关闭服务器会话、关联终端，并清除可恢复的连接错误。
    pub async fn disconnect(&self, server_id: &str) -> AppResult<()> {
        self.errors.remove(server_id);
        let terminal_ids: Vec<String> = self
            .terminals
            .iter()
            .filter(|entry| entry.server_id == server_id)
            .map(|entry| entry.key().clone())
            .collect();
        for terminal_id in terminal_ids {
            let _ = self.close_terminal(&terminal_id).await;
        }
        if let Some((_, connection)) = self.connections.remove(server_id) {
            connection
                .handle
                .disconnect(russh::Disconnect::ByApplication, "user disconnect", "en")
                .await
                .map_err(|error| {
                    AppError::new("SSH_DISCONNECT_FAILED", "ssh", "SSH 断开失败")
                        .details(error)
                        .for_server(server_id)
                })?;
        }
        Ok(())
    }

    pub async fn open_terminal(
        &self,
        server_id: &str,
        columns: u32,
        rows: u32,
        command: Option<String>,
    ) -> AppResult<(String, mpsc::Receiver<TerminalEvent>)> {
        let connection = self.connection_for_operation(server_id).await?;
        let channel = connection
            .handle
            .channel_open_session()
            .await
            .map_err(|error| {
                AppError::new("TERMINAL_OPEN_FAILED", "terminal", "无法创建终端 channel")
                    .details(error)
                    .for_server(server_id)
            })?;
        channel
            .request_pty(
                true,
                "xterm-256color",
                columns.max(20),
                rows.max(5),
                0,
                0,
                &[],
            )
            .await
            .map_err(|error| {
                AppError::new("TERMINAL_PTY_FAILED", "terminal", "远端拒绝创建 PTY")
                    .details(error)
                    .for_server(server_id)
            })?;
        match command {
            Some(command) => channel.exec(true, command).await.map_err(|error| {
                AppError::new("TERMINAL_OPEN_FAILED", "terminal", "远端拒绝执行命令")
                    .details(error)
                    .for_server(server_id)
            })?,
            None => channel.request_shell(true).await.map_err(|error| {
                AppError::new("TERMINAL_OPEN_FAILED", "terminal", "远端拒绝启动交互 Shell")
                    .details(error)
                    .for_server(server_id)
            })?,
        };

        let terminal_id = uuid::Uuid::new_v4().to_string();
        let (sender, receiver) = mpsc::channel(256);
        let (control_sender, mut control_receiver) = mpsc::channel(256);
        self.terminals.insert(
            terminal_id.clone(),
            ManagedTerminal {
                server_id: server_id.to_string(),
                control: control_sender,
            },
        );
        let terminals = self.terminals.clone();
        let spawned_id = terminal_id.clone();
        tokio::spawn(async move {
            let mut channel = channel;
            loop {
                tokio::select! {
                    message = channel.wait() => {
                        let Some(message) = message else { break; };
                        let event = match message {
                            russh::ChannelMsg::Data { data } => TerminalEvent::Data {
                                data: base64::Engine::encode(
                                    &base64::engine::general_purpose::STANDARD,
                                    data,
                                ),
                            },
                            russh::ChannelMsg::ExtendedData { data, .. } => TerminalEvent::Data {
                                data: base64::Engine::encode(
                                    &base64::engine::general_purpose::STANDARD,
                                    data,
                                ),
                            },
                            russh::ChannelMsg::ExitStatus { exit_status } => TerminalEvent::Exit { exit_status },
                            russh::ChannelMsg::Close | russh::ChannelMsg::Eof => break,
                            _ => continue,
                        };
                        if sender.send(event).await.is_err() { break; }
                    }
                    control = control_receiver.recv() => {
                        match control {
                            Some(TerminalControl::Data(data)) => {
                                if channel.data_bytes(data).await.is_err() { break; }
                            }
                            Some(TerminalControl::Resize { columns, rows }) => {
                                if channel.window_change(columns, rows, 0, 0).await.is_err() { break; }
                            }
                            Some(TerminalControl::Close) | None => {
                                let _ = channel.close().await;
                                break;
                            }
                        }
                    }
                }
            }
            terminals.remove(&spawned_id);
            let _ = sender.send(TerminalEvent::Closed).await;
        });
        Ok((terminal_id, receiver))
    }

    pub async fn write_terminal(&self, terminal_id: &str, data: &[u8]) -> AppResult<()> {
        let terminal = self
            .terminals
            .get(terminal_id)
            .map(|value| value.control.clone())
            .ok_or_else(|| AppError::new("TERMINAL_NOT_FOUND", "terminal", "终端会话已关闭"))?;
        terminal
            .send(TerminalControl::Data(data.to_vec()))
            .await
            .map_err(|_| AppError::new("TERMINAL_WRITE_FAILED", "terminal", "终端输入发送失败"))
    }

    pub async fn resize_terminal(
        &self,
        terminal_id: &str,
        columns: u32,
        rows: u32,
    ) -> AppResult<()> {
        let terminal = self
            .terminals
            .get(terminal_id)
            .map(|value| value.control.clone())
            .ok_or_else(|| AppError::new("TERMINAL_NOT_FOUND", "terminal", "终端会话已关闭"))?;
        terminal
            .send(TerminalControl::Resize {
                columns: columns.max(20),
                rows: rows.max(5),
            })
            .await
            .map_err(|_| AppError::new("TERMINAL_RESIZE_FAILED", "terminal", "终端尺寸更新失败"))
    }

    pub async fn close_terminal(&self, terminal_id: &str) -> AppResult<()> {
        if let Some((_, terminal)) = self.terminals.remove(terminal_id) {
            let _ = terminal.control.send(TerminalControl::Close).await;
        }
        Ok(())
    }

    pub async fn open_sftp(&self, server_id: &str) -> AppResult<russh_sftp::client::SftpSession> {
        let connection = self.connection_for_operation(server_id).await?;
        let channel = connection
            .handle
            .channel_open_session()
            .await
            .map_err(|error| {
                AppError::new("SFTP_FAILED", "sftp", "无法创建 SFTP channel")
                    .details(error)
                    .for_server(server_id)
            })?;
        channel
            .request_subsystem(true, "sftp")
            .await
            .map_err(|error| {
                AppError::new("SFTP_FAILED", "sftp", "远端拒绝启动 SFTP 子系统")
                    .details(error)
                    .for_server(server_id)
            })?;
        let session = russh_sftp::client::SftpSession::new(channel.into_stream())
            .await
            .map_err(|error| {
                AppError::new("SFTP_FAILED", "sftp", "SFTP 协议初始化失败")
                    .details(error)
                    .for_server(server_id)
            })?;
        session.set_timeout(30);
        Ok(session)
    }

    /// 在已认证连接上执行一次非交互远程命令并收集完整 stdout/stderr。
    pub async fn execute(
        &self,
        server_id: &str,
        command: &str,
        timeout: Duration,
    ) -> AppResult<RemoteCommandResult> {
        self.execute_with_input(server_id, command, None, timeout, None, None)
            .await
    }

    /// 在独立 SSH channel 上执行命令，并逐块转发 stdout/stderr 到 Tauri Channel。
    #[allow(dead_code)]
    pub async fn execute_stream(
        &self,
        server_id: &str,
        command: &str,
        timeout: Duration,
        events: &Channel<CommandEvent>,
    ) -> AppResult<RemoteCommandResult> {
        self.execute_with_input(server_id, command, None, timeout, Some(events), None)
            .await
    }

    /// 执行可取消的流式命令，并在 SSH channel 等待输出时响应取消请求。
    pub async fn execute_stream_task(
        &self,
        server_id: &str,
        command: &str,
        timeout: Duration,
        events: &Channel<CommandEvent>,
        task_id: &str,
    ) -> AppResult<RemoteCommandResult> {
        let guard = self.tasks.begin(task_id)?;
        self.execute_with_input(
            server_id,
            command,
            None,
            timeout,
            Some(events),
            Some(guard.state()),
        )
        .await
    }

    /// 按服务器 sudo 模式执行命令；密码只通过 SSH stdin 发送，不进入命令字符串。
    pub async fn execute_privileged(
        &self,
        server_id: &str,
        command: &str,
        timeout: Duration,
    ) -> AppResult<RemoteCommandResult> {
        let record = self.repository.record(server_id).await?;
        match record.sudo_mode.as_str() {
            "passwordless" => {
                self.execute_with_input(
                    server_id,
                    &format!(
                        "sudo -n -- sh -c {}",
                        crate::security::shell_escape(command)
                    ),
                    None,
                    timeout,
                    None,
                    None,
                )
                .await
            }
            "password" => {
                let reference = record.sudo_secret_ref.as_deref().ok_or_else(|| {
                    AppError::new("SUDO_AUTH_FAILED", "privilege", "服务器未保存 sudo 密码")
                        .for_server(server_id)
                })?;
                let secret = self.repository.credential(reference)?;
                let mut input = zeroize::Zeroizing::new(secret.expose_secret().as_bytes().to_vec());
                input.push(b'\n');
                let result = self
                    .execute_with_input(
                        server_id,
                        &format!(
                            "sudo -S -p '' -- sh -c {}",
                            crate::security::shell_escape(command)
                        ),
                        Some(input.as_slice()),
                        timeout,
                        None,
                        None,
                    )
                    .await?;
                if result.exit_code != 0 && result.stderr.to_ascii_lowercase().contains("password")
                {
                    return Err(
                        AppError::new("SUDO_AUTH_FAILED", "privilege", "sudo 认证失败")
                            .for_server(server_id),
                    );
                }
                Ok(result)
            }
            _ => Err(
                AppError::new("SUDO_REQUIRED", "privilege", "该服务器档案未启用 sudo")
                    .for_server(server_id)
                    .suggestion("编辑服务器档案并选择 sudo 模式"),
            ),
        }
    }

    /// 以当前 SSH 用户或已配置的 sudo 模式执行系统管理命令。
    /// root 连接不额外套 sudo；普通用户必须在服务器档案中配置可用的 sudo 模式。
    pub async fn execute_system(
        &self,
        server_id: &str,
        command: &str,
        timeout: Duration,
    ) -> AppResult<RemoteCommandResult> {
        let record = self.repository.record(server_id).await?;
        if record.username == "root" {
            self.execute(server_id, command, timeout).await
        } else {
            self.execute_privileged(server_id, command, timeout).await
        }
    }

    /// 以当前 SSH 用户或 sudo 模式执行可取消的系统管理命令，并复用统一任务取消协议。
    pub async fn execute_stream_system_task(
        &self,
        server_id: &str,
        command: &str,
        timeout: Duration,
        events: &Channel<CommandEvent>,
        task_id: &str,
    ) -> AppResult<RemoteCommandResult> {
        let record = self.repository.record(server_id).await?;
        if record.username == "root" {
            self.execute_stream_task(server_id, command, timeout, events, task_id)
                .await
        } else {
            self.execute_stream_privileged_task(server_id, command, timeout, events, task_id)
                .await
        }
    }

    /// 以已配置的 sudo 模式执行可取消的流式命令，并把输出转发到 Tauri Channel。
    pub async fn execute_stream_privileged_task(
        &self,
        server_id: &str,
        command: &str,
        timeout: Duration,
        events: &Channel<CommandEvent>,
        task_id: &str,
    ) -> AppResult<RemoteCommandResult> {
        let guard = self.tasks.begin(task_id)?;
        self.execute_stream_privileged_inner(
            server_id,
            command,
            timeout,
            events,
            Some(guard.state()),
        )
        .await
    }

    /// 以已配置的 sudo 模式执行流式命令；未注册 task 时保留原有不可取消调用。
    #[allow(dead_code)]
    pub async fn execute_stream_privileged(
        &self,
        server_id: &str,
        command: &str,
        timeout: Duration,
        events: &Channel<CommandEvent>,
    ) -> AppResult<RemoteCommandResult> {
        self.execute_stream_privileged_inner(server_id, command, timeout, events, None)
            .await
    }

    /// 处理 sudo 流式命令的认证分支，并把取消令牌传入 SSH channel。
    async fn execute_stream_privileged_inner(
        &self,
        server_id: &str,
        command: &str,
        timeout: Duration,
        events: &Channel<CommandEvent>,
        cancellation: Option<Arc<CommandTaskState>>,
    ) -> AppResult<RemoteCommandResult> {
        let record = self.repository.record(server_id).await?;
        match record.sudo_mode.as_str() {
            "passwordless" => {
                self.execute_with_input(
                    server_id,
                    &format!(
                        "sudo -n -- sh -c {}",
                        crate::security::shell_escape(command)
                    ),
                    None,
                    timeout,
                    Some(events),
                    cancellation.clone(),
                )
                .await
            }
            "password" => {
                let reference = record.sudo_secret_ref.as_deref().ok_or_else(|| {
                    AppError::new("SUDO_AUTH_FAILED", "privilege", "服务器未保存 sudo 密码")
                        .for_server(server_id)
                })?;
                let secret = self.repository.credential(reference)?;
                let mut input = zeroize::Zeroizing::new(secret.expose_secret().as_bytes().to_vec());
                input.push(b'\n');
                let result = self
                    .execute_with_input(
                        server_id,
                        &format!(
                            "sudo -S -p '' -- sh -c {}",
                            crate::security::shell_escape(command)
                        ),
                        Some(input.as_slice()),
                        timeout,
                        Some(events),
                        cancellation,
                    )
                    .await?;
                if result.exit_code != 0 && result.stderr.to_ascii_lowercase().contains("password")
                {
                    return Err(
                        AppError::new("SUDO_AUTH_FAILED", "privilege", "sudo 认证失败")
                            .for_server(server_id),
                    );
                }
                Ok(result)
            }
            _ => Err(
                AppError::new("SUDO_REQUIRED", "privilege", "该服务器档案未启用 sudo")
                    .for_server(server_id)
                    .suggestion("编辑服务器档案并选择 sudo 模式"),
            ),
        }
    }

    /// 在受连接并发限制的 SSH channel 上执行命令，并可选地转发原始输出块。
    /// 统一通过远端 `sh -c` 执行，以支持探测脚本中的 if/for 等复合命令；命令内容来自受控的领域层，外层参数会先做 shell 转义。
    async fn execute_with_input(
        &self,
        server_id: &str,
        command: &str,
        input: Option<&[u8]>,
        timeout: Duration,
        events: Option<&Channel<CommandEvent>>,
        cancellation: Option<Arc<CommandTaskState>>,
    ) -> AppResult<RemoteCommandResult> {
        let connection = self.connection_for_operation(server_id).await?;
        let _permit =
            connection.limiter.acquire().await.map_err(|_| {
                AppError::new("CANCELLED", "task", "命令已取消").for_server(server_id)
            })?;
        let started = Instant::now();
        let future = async {
            let mut channel = connection
                .handle
                .channel_open_session()
                .await
                .map_err(|error| {
                    AppError::new("SSH_CHANNEL_FAILED", "ssh", "无法创建 SSH channel")
                        .details(error)
                        .for_server(server_id)
                })?;
            channel
                .exec(true, wrap_remote_command(command))
                .await
                .map_err(|error| {
                    AppError::new(
                        "REMOTE_COMMAND_FAILED",
                        "remote_command",
                        "远程命令启动失败",
                    )
                    .details(error)
                    .for_server(server_id)
                })?;
            if let Some(input) = input {
                channel.data_bytes(input.to_vec()).await.map_err(|error| {
                    AppError::new(
                        "REMOTE_COMMAND_FAILED",
                        "remote_command",
                        "远程标准输入发送失败",
                    )
                    .details(error)
                    .for_server(server_id)
                })?;
                channel.eof().await.map_err(|error| {
                    AppError::new(
                        "REMOTE_COMMAND_FAILED",
                        "remote_command",
                        "远程标准输入关闭失败",
                    )
                    .details(error)
                    .for_server(server_id)
                })?;
            }
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            let mut exit_code = None;
            loop {
                let message = if let Some(cancellation) = cancellation.as_ref() {
                    if cancellation.cancelled.load(Ordering::Acquire) {
                        let _ = channel.close().await;
                        if let Some(events) = events {
                            let _ = events.send(CommandEvent::Cancelled);
                        }
                        return Err(cancelled_error(server_id));
                    }
                    tokio::select! {
                        _ = cancellation.notify.notified() => {
                            let _ = channel.close().await;
                            if let Some(events) = events {
                                let _ = events.send(CommandEvent::Cancelled);
                            }
                            return Err(cancelled_error(server_id));
                        }
                        message = channel.wait() => message,
                    }
                } else {
                    channel.wait().await
                };
                let Some(message) = message else { break };
                match message {
                    russh::ChannelMsg::Data { data } => {
                        stdout.extend_from_slice(&data);
                        if let Some(events) = events {
                            let _ = events.send(CommandEvent::Output {
                                stream: "stdout".into(),
                                data: String::from_utf8_lossy(&data).into_owned(),
                            });
                        }
                    }
                    russh::ChannelMsg::ExtendedData { data, ext: 1 } => {
                        stderr.extend_from_slice(&data);
                        if let Some(events) = events {
                            let _ = events.send(CommandEvent::Output {
                                stream: "stderr".into(),
                                data: String::from_utf8_lossy(&data).into_owned(),
                            });
                        }
                    }
                    russh::ChannelMsg::ExitStatus { exit_status } => exit_code = Some(exit_status),
                    _ => {}
                }
            }
            let result = RemoteCommandResult {
                exit_code: exit_code.unwrap_or(255),
                stdout: String::from_utf8_lossy(&stdout).into_owned(),
                stderr: String::from_utf8_lossy(&stderr).into_owned(),
                duration_ms: started.elapsed().as_millis() as u64,
            };
            if let Some(events) = events {
                let _ = events.send(CommandEvent::Completed {
                    exit_code: result.exit_code,
                });
            }
            Ok(result)
        };
        tokio::time::timeout(timeout, future).await.map_err(|_| {
            AppError::new("COMMAND_TIMEOUT", "remote_command", "远程命令执行超时")
                .for_server(server_id)
        })?
    }

    /// 返回可执行远程操作的 SSH 会话；发现断开或从未建立会时透明尝试恢复连接，
    /// 主机密钥尚未信任的服务器保持原错误，引导用户到概览页完成指纹核对。
    async fn connection_for_operation(&self, server_id: &str) -> AppResult<Arc<ManagedConnection>> {
        if let Some(connection) = self.connections.get(server_id).map(|value| value.clone()) {
            if !connection.handle.is_closed() {
                return Ok(connection);
            }
            self.connections.remove(server_id);
            self.errors
                .insert(server_id.to_string(), connection_lost_error(server_id));
        }

        let should_reconnect = self
            .errors
            .get(server_id)
            .is_some_and(|error| error.code == "SSH_CONNECTION_LOST");
        let outcome = if should_reconnect {
            self.reconnect(server_id).await
        } else {
            self.connect(server_id).await
        };
        if let Ok(ConnectOutcome::Connected(_)) = outcome {
            if let Some(connection) = self.connections.get(server_id).map(|value| value.clone()) {
                return Ok(connection);
            }
        }

        Err(AppError::new("SSH_NOT_CONNECTED", "ssh", "服务器尚未连接")
            .for_server(server_id)
            .suggestion("请返回服务器概览，点击“重新连接”后再继续操作"))
    }
}

/// 创建统一的流式命令取消错误，供 UI 将任务标记为已取消。
fn cancelled_error(server_id: &str) -> AppError {
    AppError::new("CANCELLED", "task", "远程任务已取消").for_server(server_id)
}

/// 生成统一的会话失效错误，供状态栏、错误提示和按需重连共用。
fn connection_lost_error(server_id: &str) -> AppError {
    AppError::new("SSH_CONNECTION_LOST", "ssh", "SSH 连接已断开")
        .for_server(server_id)
        .suggestion("正在尝试恢复连接；若仍失败，请检查网络和服务器 SSH 服务")
}

#[derive(Clone)]
struct ManagedTerminal {
    server_id: String,
    control: mpsc::Sender<TerminalControl>,
}

enum TerminalControl {
    Data(Vec<u8>),
    Resize { columns: u32, rows: u32 },
    Close,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "event", content = "data")]
pub enum TerminalEvent {
    Data { data: String },
    Exit { exit_status: u32 },
    Closed,
}

impl PendingHostKey {
    fn challenge(&self) -> HostKeyChallenge {
        HostKeyChallenge {
            server_id: self.server_id.clone(),
            host: self.host.clone(),
            port: self.port,
            key_type: self.key_type.clone(),
            fingerprint: self.fingerprint.clone(),
        }
    }
}

#[derive(Debug, Clone)]
enum HostKeyObservation {
    Unknown(ObservedKey),
    Changed,
}

#[derive(Debug, Clone)]
struct ObservedKey {
    key_type: String,
    fingerprint: String,
    public_key: String,
}

#[derive(Clone)]
struct HostKeyHandler {
    expected: Option<KnownHost>,
    observation: Arc<Mutex<Option<HostKeyObservation>>>,
}

impl client::Handler for HostKeyHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKey,
    ) -> Result<bool, Self::Error> {
        let public_key = server_public_key.to_openssh().map_err(russh::Error::from)?;
        if let Some(expected) = &self.expected {
            if expected.public_key == public_key {
                return Ok(true);
            }
            if let Ok(mut observation) = self.observation.lock() {
                *observation = Some(HostKeyObservation::Changed);
            }
            return Ok(false);
        }
        let value = ObservedKey {
            key_type: server_public_key.algorithm().as_str().to_string(),
            fingerprint: server_public_key.fingerprint(HashAlg::Sha256).to_string(),
            public_key,
        };
        if let Ok(mut observation) = self.observation.lock() {
            *observation = Some(HostKeyObservation::Unknown(value));
        }
        Ok(false)
    }
}

async fn authenticate(
    handle: &mut client::Handle<HostKeyHandler>,
    record: &ServerRecord,
    repository: &ServerRepository,
) -> AppResult<()> {
    let authenticated = match record.auth_type.as_str() {
        "password" => {
            let reference = record.password_secret_ref.as_deref().ok_or_else(|| {
                AppError::new("SSH_AUTH_FAILED", "authentication", "服务器未保存 SSH 密码")
            })?;
            let secret = repository.credential(reference)?;
            handle
                .authenticate_password(&record.username, secret.expose_secret())
                .await
                .map_err(|error| {
                    AppError::new("SSH_AUTH_FAILED", "authentication", "SSH 密码认证失败")
                        .details(error)
                        .for_server(&record.id)
                })?
                .success()
        }
        "private_key" => {
            let path = record.private_key_path.as_deref().ok_or_else(|| {
                AppError::new("SSH_AUTH_FAILED", "authentication", "服务器未配置私钥路径")
            })?;
            let passphrase = record
                .key_passphrase_secret_ref
                .as_deref()
                .map(|reference| repository.credential(reference))
                .transpose()?;
            let key = russh::keys::load_secret_key(
                path,
                passphrase.as_ref().map(|value| value.expose_secret()),
            )
            .map_err(|error| {
                AppError::new("SSH_AUTH_FAILED", "authentication", "无法读取 SSH 私钥")
                    .details(error)
                    .for_server(&record.id)
            })?;
            let hash_algorithm = handle
                .best_supported_rsa_hash()
                .await
                .map_err(|error| {
                    AppError::new("SSH_AUTH_FAILED", "authentication", "无法协商 RSA 签名算法")
                        .details(error)
                        .for_server(&record.id)
                })?
                .flatten();
            let key = russh::keys::PrivateKeyWithHashAlg::new(Arc::new(key), hash_algorithm);
            handle
                .authenticate_publickey(&record.username, key)
                .await
                .map_err(|error| {
                    AppError::new("SSH_AUTH_FAILED", "authentication", "SSH 私钥认证失败")
                        .details(error)
                        .for_server(&record.id)
                })?
                .success()
        }
        "ssh_agent" => authenticate_with_agent(handle, record).await?,
        _ => {
            return Err(
                AppError::new("SSH_AUTH_FAILED", "authentication", "不支持的 SSH 认证方式")
                    .for_server(&record.id),
            )
        }
    };
    if !authenticated {
        return Err(
            AppError::new("SSH_AUTH_FAILED", "authentication", "SSH 认证被服务器拒绝")
                .for_server(&record.id)
                .suggestion("检查用户名、认证方式与凭据"),
        );
    }
    Ok(())
}

type DynamicAgentClient = AgentClient<Box<dyn AgentStream + Send + Unpin>>;

/// 使用当前桌面会话的 SSH Agent 尝试所有可用身份；私钥不会进入本地数据库或远程命令参数。
async fn authenticate_with_agent(
    handle: &mut client::Handle<HostKeyHandler>,
    record: &ServerRecord,
) -> AppResult<bool> {
    let mut agent = connect_agent().await.map_err(|error| {
        AppError::new(
            "SSH_AGENT_UNAVAILABLE",
            "authentication",
            "无法连接本机 SSH Agent",
        )
        .details(error)
        .for_server(&record.id)
        .suggestion("启动 ssh-agent/Pageant 并确认 SSH_AUTH_SOCK 或 Pageant 可用")
    })?;
    let identities = agent.request_identities().await.map_err(|error| {
        AppError::new(
            "SSH_AGENT_UNAVAILABLE",
            "authentication",
            "无法读取 SSH Agent 身份",
        )
        .details(error)
        .for_server(&record.id)
    })?;
    if identities.is_empty() {
        return Err(AppError::new(
            "SSH_AGENT_EMPTY",
            "authentication",
            "SSH Agent 中没有可用身份",
        )
        .for_server(&record.id)
        .suggestion("使用 ssh-add 加载私钥，或改用私钥文件认证"));
    }

    for identity in identities {
        let public_key = identity.public_key().into_owned();
        let hash_alg = match public_key.algorithm() {
            Algorithm::Rsa { .. } => handle
                .best_supported_rsa_hash()
                .await
                .map_err(|error| {
                    AppError::new(
                        "SSH_AUTH_FAILED",
                        "authentication",
                        "无法协商 Agent RSA 签名算法",
                    )
                    .details(error)
                    .for_server(&record.id)
                })?
                .flatten(),
            _ => None,
        };
        let result = match identity {
            AgentIdentity::PublicKey { key, .. } => handle
                .authenticate_publickey_with(&record.username, key, hash_alg, &mut agent)
                .await
                .map_err(|error| {
                    AppError::new(
                        "SSH_AGENT_SIGN_FAILED",
                        "authentication",
                        "SSH Agent 签名失败",
                    )
                    .details(error)
                    .for_server(&record.id)
                })?,
            AgentIdentity::Certificate { certificate, .. } => handle
                .authenticate_certificate_with(&record.username, certificate, hash_alg, &mut agent)
                .await
                .map_err(|error| {
                    AppError::new(
                        "SSH_AGENT_SIGN_FAILED",
                        "authentication",
                        "SSH Agent 证书签名失败",
                    )
                    .details(error)
                    .for_server(&record.id)
                })?,
        };
        if result.success() {
            return Ok(true);
        }
    }

    Ok(false)
}

/// 连接 Windows Pageant/OpenSSH Agent 或 Unix SSH_AUTH_SOCK，并统一为动态 Agent 客户端类型。
async fn connect_agent() -> Result<DynamicAgentClient, String> {
    #[cfg(unix)]
    {
        AgentClient::connect_env()
            .await
            .map(AgentClient::dynamic)
            .map_err(|error| error.to_string())
    }
    #[cfg(windows)]
    {
        if let Ok(path) = std::env::var("SSH_AUTH_SOCK") {
            if path.starts_with(r"\\.\pipe\") {
                if let Ok(client) = AgentClient::connect_named_pipe(path).await {
                    return Ok(client.dynamic());
                }
            }
        }
        AgentClient::connect_pageant()
            .await
            .map(AgentClient::dynamic)
            .map_err(|error| error.to_string())
    }
    #[cfg(not(any(unix, windows)))]
    {
        Err("当前平台不支持 SSH Agent".into())
    }
}

fn map_connect_error(error: russh::Error, server_id: &str) -> AppError {
    AppError::new("SSH_CONNECT_FAILED", "ssh", "无法建立 SSH 连接")
        .details(error)
        .for_server(server_id)
        .suggestion("检查 SSH 服务、端口、防火墙与支持的密钥算法")
}

/// 判断连接错误是否适合进行短暂退避重试，不重试凭据、Host Key 和参数错误。
fn is_retryable_connect_error(code: &str) -> bool {
    matches!(code, "NETWORK_TIMEOUT" | "SSH_CONNECT_FAILED")
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "event", content = "data")]
pub enum CommandEvent {
    Output { stream: String, data: String },
    Completed { exit_code: u32 },
    Cancelled,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteCommandResult {
    pub exit_code: u32,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
}

/// 将受控远程命令包裹为可执行的 POSIX Shell 脚本，并固定远端输出语言为 C。
fn wrap_remote_command(command: &str) -> String {
    format!(
        "sh -c {}",
        crate::security::shell_escape(&format!("export LC_ALL=C; {command}"))
    )
}

#[cfg(test)]
mod tests {
    use super::wrap_remote_command;

    #[test]
    fn wraps_compound_commands_inside_shell() {
        let command = wrap_remote_command(
            "if command -v ss; then for item in ss lsof; do printf '%s' \"$item\"; done; fi",
        );
        assert!(command.starts_with("sh -c 'export LC_ALL=C; if command -v ss;"));
        assert!(command.contains("for item in ss lsof"));
    }
}

use crate::domain::ssh::{CommandEvent, RemoteCommandResult, SshConnectionManager};
use crate::errors::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerSnapshot {
    pub installed: bool,
    pub running: bool,
    pub version: Option<String>,
    pub api_version: Option<String>,
    pub os: Option<String>,
    pub architecture: Option<String>,
    pub storage_driver: Option<String>,
    pub cgroup_version: Option<String>,
    pub root_dir: Option<String>,
    pub disk_usage: Option<String>,
    pub containers: Vec<ContainerInfo>,
    pub images: Vec<ImageInfo>,
    pub volumes: Vec<VolumeInfo>,
    pub networks: Vec<NetworkInfo>,
    pub compose_projects: Vec<ComposeProject>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContainerInfo {
    pub id: String,
    pub name: String,
    pub image: String,
    pub status: String,
    pub health: Option<String>,
    pub created: String,
    pub ports: String,
    pub compose_project: Option<String>,
    pub restart_policy: Option<String>,
    pub cpu_limit_nano: Option<i64>,
    pub memory_limit_bytes: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageInfo {
    pub repository: String,
    pub tag: String,
    pub id: String,
    pub size: String,
    pub created: String,
    pub dangling: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeInfo {
    pub name: String,
    pub driver: String,
    pub mountpoint: String,
    pub labels: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkInfo {
    pub id: String,
    pub name: String,
    pub driver: String,
    pub scope: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComposeProject {
    pub name: String,
    pub status: String,
    pub config_files: String,
    pub working_dir: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerComposeService {
    pub name: String,
    pub service: String,
    pub image: String,
    pub state: String,
    pub status: String,
    pub ports: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerComposeDetails {
    pub project: String,
    pub services: Vec<DockerComposeService>,
    pub config: String,
    pub config_path: Option<String>,
    pub config_size: Option<u64>,
    pub config_modified_at: Option<u32>,
    pub volumes: Vec<String>,
    pub networks: Vec<String>,
    pub cleanup_volumes: Vec<String>,
    pub cleanup_networks: Vec<String>,
    pub orphan_containers: Vec<String>,
    pub cleanup_warnings: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerActionInput {
    pub server_id: String,
    pub container_id: String,
    pub action: String,
    #[serde(default)]
    pub force: bool,
    #[serde(default)]
    pub sudo: bool,
    #[serde(default)]
    pub confirmed: bool,
    pub new_name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerActionResult {
    pub container_id: String,
    pub action: String,
    pub verified_status: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerLogs {
    pub container_id: String,
    pub output: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerTextResult {
    pub container_id: String,
    pub output: String,
}

/// A bounded read-only Docker daemon event summary; actor attributes are deliberately omitted.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerEvent {
    pub event_type: String,
    pub action: String,
    pub actor_name: Option<String>,
    pub actor_id: Option<String>,
    pub timestamp: Option<i64>,
}

/// Returns recent Docker events without retaining the raw daemon payload.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerEventsResult {
    pub since_seconds: u32,
    pub events: Vec<DockerEvent>,
    pub fetched_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerPullInput {
    pub server_id: String,
    pub image: String,
    pub task_id: String,
    #[serde(default)]
    pub sudo: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerPullResult {
    pub image: String,
    pub output: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerBuildInput {
    pub server_id: String,
    pub context_path: String,
    pub dockerfile_path: Option<String>,
    pub image: String,
    #[serde(default)]
    pub build_args: Vec<String>,
    pub task_id: String,
    #[serde(default)]
    pub sudo: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerBuildResult {
    pub image: String,
    pub context_path: String,
    pub output: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerRunInput {
    pub server_id: String,
    pub image: String,
    pub name: Option<String>,
    #[serde(default)]
    pub ports: Vec<String>,
    #[serde(default)]
    pub environment: Vec<String>,
    pub network: Option<String>,
    pub restart_policy: Option<String>,
    #[serde(default)]
    pub auto_remove: bool,
    #[serde(default)]
    pub privileged: bool,
    #[serde(default)]
    pub sudo: bool,
    #[serde(default)]
    pub confirmed: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerRunResult {
    pub container_id: String,
    pub output: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerExecInput {
    pub server_id: String,
    pub container_id: String,
    pub command: String,
    #[serde(default)]
    pub sudo: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerResourceActionInput {
    pub server_id: String,
    pub kind: String,
    pub name: String,
    pub action: String,
    #[serde(default)]
    pub sudo: bool,
    #[serde(default)]
    pub confirmed: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerResourceActionResult {
    pub kind: String,
    pub name: String,
    pub action: String,
    pub verified: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerImageActionInput {
    pub server_id: String,
    pub image: String,
    pub action: String,
    #[serde(default)]
    pub force: bool,
    #[serde(default)]
    pub sudo: bool,
    #[serde(default)]
    pub confirmed: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerResourceInspectInput {
    pub server_id: String,
    pub kind: String,
    pub name: String,
    #[serde(default)]
    pub sudo: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerComposeActionInput {
    pub server_id: String,
    pub project: String,
    pub working_dir: Option<String>,
    pub action: String,
    #[serde(default)]
    pub sudo: bool,
    #[serde(default)]
    pub confirmed: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerComposeYamlInput {
    pub server_id: String,
    pub project: String,
    pub working_dir: Option<String>,
    pub config_path: String,
    pub content: String,
    pub expected_size: u64,
    pub expected_modified_at: Option<u32>,
    #[serde(default)]
    pub force: bool,
    #[serde(default)]
    pub sudo: bool,
    #[serde(default)]
    pub confirmed: bool,
}

/// 通过 SSH 执行远端 Docker CLI，读取真实容器、镜像和 Engine 信息。
pub async fn snapshot(
    ssh: &SshConnectionManager,
    server_id: &str,
    privileged: bool,
) -> AppResult<DockerSnapshot> {
    let result = if privileged {
        ssh.execute_privileged(server_id, snapshot_command(), Duration::from_secs(45))
            .await?
    } else {
        ssh.execute(server_id, snapshot_command(), Duration::from_secs(45))
            .await?
    };
    if result.exit_code != 0 {
        if result.stderr.contains("not found") || result.stderr.contains("command not found") {
            return Ok(empty_snapshot());
        }
        let code = if result.stderr.to_ascii_lowercase().contains("permission") {
            "DOCKER_PERMISSION_DENIED"
        } else {
            "DOCKER_DAEMON_STOPPED"
        };
        return Err(AppError::new(code, "docker", "无法读取 Docker Engine 状态")
            .details(result.stderr)
            .for_server(server_id));
    }
    parse_snapshot(&result.stdout).ok_or_else(|| {
        AppError::new("PARSE_FAILED", "docker", "Docker CLI 返回内容无法解析").for_server(server_id)
    })
}

/// 读取最近一段 Docker daemon 事件；只返回有限字段，绝不把 actor 属性原文送入前端。
pub async fn events(
    ssh: &SshConnectionManager,
    server_id: &str,
    since_seconds: u32,
    privileged: bool,
) -> AppResult<DockerEventsResult> {
    let since_seconds = since_seconds.clamp(1, 3600);
    let command =
        format!("docker events --since {since_seconds}s --until 0s --format '{{{{json .}}}}'");
    let result = if privileged {
        ssh.execute_privileged(server_id, &command, Duration::from_secs(35))
            .await?
    } else {
        ssh.execute(server_id, &command, Duration::from_secs(35))
            .await?
    };
    if result.exit_code != 0 {
        return Err(
            AppError::new("DOCKER_EVENTS_FAILED", "docker", "读取 Docker 事件失败")
                .details(result.stderr)
                .for_server(server_id),
        );
    }
    Ok(DockerEventsResult {
        since_seconds,
        events: parse_events(&result.stdout),
        fetched_at: chrono::Utc::now().to_rfc3339(),
    })
}

/// 执行已确认的容器动作，并读取 inspect 状态验证结果。
pub async fn action(
    ssh: &SshConnectionManager,
    input: DockerActionInput,
) -> AppResult<DockerActionResult> {
    validate_name(&input.container_id)?;
    let action = match input.action.as_str() {
        "start" | "stop" | "restart" | "pause" | "unpause" | "kill" | "remove" | "rename" => {
            input.action.as_str()
        }
        _ => {
            return Err(AppError::new(
                "VALIDATION_FAILED",
                "validation",
                "不支持的 Docker 容器操作",
            )
            .for_server(&input.server_id))
        }
    };
    if matches!(action, "kill" | "remove" | "rename") && !input.confirmed {
        return Err(AppError::new(
            "CONFIRMATION_REQUIRED",
            "confirmation",
            "该 Docker 操作必须经过用户确认",
        )
        .for_server(&input.server_id));
    }
    let command = if action == "remove" {
        format!(
            "docker rm {} -- {}",
            if input.force { "-f" } else { "" },
            crate::security::shell_escape(&input.container_id)
        )
    } else if action == "rename" {
        let new_name = input.new_name.as_deref().ok_or_else(|| {
            AppError::new("VALIDATION_FAILED", "validation", "重命名容器需要新名称")
                .for_server(&input.server_id)
        })?;
        validate_container_name(new_name)?;
        format!(
            "docker rename -- {} {}",
            crate::security::shell_escape(&input.container_id),
            crate::security::shell_escape(new_name)
        )
    } else {
        format!(
            "docker {action} -- {}",
            crate::security::shell_escape(&input.container_id)
        )
    };
    let result = if input.sudo {
        ssh.execute_privileged(&input.server_id, &command, Duration::from_secs(60))
            .await?
    } else {
        ssh.execute(&input.server_id, &command, Duration::from_secs(60))
            .await?
    };
    if result.exit_code != 0 {
        return Err(
            AppError::new("DOCKER_ACTION_FAILED", "docker", "Docker 容器操作失败")
                .details(result.stderr)
                .for_server(&input.server_id),
        );
    }
    let verify_command = if action == "rename" {
        let new_name = input.new_name.as_deref().ok_or_else(|| {
            AppError::new("VALIDATION_FAILED", "validation", "重命名容器需要新名称")
                .for_server(&input.server_id)
        })?;
        format!(
            "docker inspect --format '{{{{.Name}}}}' -- {}",
            crate::security::shell_escape(new_name)
        )
    } else {
        format!(
            "docker inspect --format '{{{{.State.Status}}}}' -- {}",
            crate::security::shell_escape(&input.container_id)
        )
    };
    let verify = if input.sudo {
        ssh.execute_privileged(&input.server_id, &verify_command, Duration::from_secs(20))
            .await?
    } else {
        ssh.execute(&input.server_id, &verify_command, Duration::from_secs(20))
            .await?
    };
    let status = if action == "remove" && verify.exit_code != 0 {
        "removed".into()
    } else if action == "rename" && verify.exit_code == 0 {
        verify.stdout.trim().trim_start_matches('/').to_string()
    } else if verify.exit_code == 0 {
        verify.stdout.trim().to_string()
    } else {
        return Err(
            AppError::new("DOCKER_VERIFY_FAILED", "docker", "容器操作后状态验证失败")
                .details(verify.stderr)
                .for_server(&input.server_id),
        );
    };
    Ok(DockerActionResult {
        container_id: input.container_id,
        action: action.into(),
        verified_status: status,
    })
}

/// 读取指定容器最近日志；日志内容不写入本地持久化存储。
pub async fn logs(
    ssh: &SshConnectionManager,
    server_id: &str,
    container_id: &str,
    tail: u32,
    privileged: bool,
) -> AppResult<DockerLogs> {
    validate_name(container_id)?;
    let tail = tail.clamp(1, 10_000);
    let command = format!(
        "docker logs --timestamps --tail {tail} -- {}",
        crate::security::shell_escape(container_id)
    );
    let result = if privileged {
        ssh.execute_privileged(server_id, &command, Duration::from_secs(45))
            .await?
    } else {
        ssh.execute(server_id, &command, Duration::from_secs(45))
            .await?
    };
    if result.exit_code != 0 {
        return Err(
            AppError::new("DOCKER_LOGS_FAILED", "docker", "读取容器日志失败")
                .details(result.stderr)
                .for_server(server_id),
        );
    }
    Ok(DockerLogs {
        container_id: container_id.into(),
        output: result.stdout,
    })
}

/// 返回单个容器的原始 inspect JSON，供前端格式化查看而不丢失字段。
pub async fn inspect(
    ssh: &SshConnectionManager,
    server_id: &str,
    container_id: &str,
    privileged: bool,
) -> AppResult<DockerTextResult> {
    validate_name(container_id)?;
    let command = format!(
        "docker inspect -- {}",
        crate::security::shell_escape(container_id)
    );
    let result = if privileged {
        ssh.execute_privileged(server_id, &command, Duration::from_secs(30))
            .await?
    } else {
        ssh.execute(server_id, &command, Duration::from_secs(30))
            .await?
    };
    if result.exit_code != 0 {
        return Err(
            AppError::new("DOCKER_INSPECT_FAILED", "docker", "读取容器 inspect 失败")
                .details(result.stderr)
                .for_server(server_id),
        );
    }
    Ok(DockerTextResult {
        container_id: container_id.into(),
        output: result.stdout,
    })
}

/// 读取容器的一次性资源统计，不伪造长期历史数据。
pub async fn stats(
    ssh: &SshConnectionManager,
    server_id: &str,
    container_id: &str,
    privileged: bool,
) -> AppResult<DockerTextResult> {
    validate_name(container_id)?;
    let command = format!(
        "docker stats --no-stream --format '{{{{json .}}}}' -- {}",
        crate::security::shell_escape(container_id)
    );
    let result = if privileged {
        ssh.execute_privileged(server_id, &command, Duration::from_secs(30))
            .await?
    } else {
        ssh.execute(server_id, &command, Duration::from_secs(30))
            .await?
    };
    if result.exit_code != 0 {
        return Err(
            AppError::new("DOCKER_STATS_FAILED", "docker", "读取容器资源统计失败")
                .details(result.stderr)
                .for_server(server_id),
        );
    }
    Ok(DockerTextResult {
        container_id: container_id.into(),
        output: result.stdout,
    })
}

/// 读取容器内进程列表，保留 Docker CLI 的列格式供复制和搜索。
pub async fn top(
    ssh: &SshConnectionManager,
    server_id: &str,
    container_id: &str,
    privileged: bool,
) -> AppResult<DockerTextResult> {
    validate_name(container_id)?;
    let command = format!(
        "docker top -- {}",
        crate::security::shell_escape(container_id)
    );
    let result = if privileged {
        ssh.execute_privileged(server_id, &command, Duration::from_secs(30))
            .await?
    } else {
        ssh.execute(server_id, &command, Duration::from_secs(30))
            .await?
    };
    if result.exit_code != 0 {
        return Err(
            AppError::new("DOCKER_TOP_FAILED", "docker", "读取容器进程失败")
                .details(result.stderr)
                .for_server(server_id),
        );
    }
    Ok(DockerTextResult {
        container_id: container_id.into(),
        output: result.stdout,
    })
}

/// 拉取单个镜像并通过可取消 SSH task 转发 Docker layer 输出。
pub async fn pull(
    ssh: &SshConnectionManager,
    input: DockerPullInput,
    events: &tauri::ipc::Channel<CommandEvent>,
) -> AppResult<DockerPullResult> {
    validate_image(&input.image)?;
    let command = format!("docker pull {}", input.image);
    let result = if input.sudo {
        ssh.execute_stream_privileged_task(
            &input.server_id,
            &command,
            Duration::from_secs(1800),
            events,
            &input.task_id,
        )
        .await?
    } else {
        ssh.execute_stream_task(
            &input.server_id,
            &command,
            Duration::from_secs(1800),
            events,
            &input.task_id,
        )
        .await?
    };
    if result.exit_code != 0 {
        return Err(
            AppError::new("DOCKER_PULL_FAILED", "docker", "拉取镜像失败")
                .details(result.stderr)
                .for_server(&input.server_id),
        );
    }
    Ok(DockerPullResult {
        image: input.image,
        output: format!("{}\n{}", result.stdout, result.stderr),
    })
}

/// 在远端 Docker daemon 构建镜像，并通过可取消 SSH task 转发 BuildKit/CLI 输出。
pub async fn build(
    ssh: &SshConnectionManager,
    input: DockerBuildInput,
    events: &tauri::ipc::Channel<CommandEvent>,
) -> AppResult<DockerBuildResult> {
    validate_image(&input.image)?;
    validate_build_context(&input.context_path)?;
    if input.task_id.is_empty()
        || input.task_id.len() > 128
        || input
            .task_id
            .chars()
            .any(|character| character == '\0' || character == '\r' || character == '\n')
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "validation",
            "Docker 构建任务 ID 无效",
        ));
    }
    if input.build_args.len() > 64 {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "validation",
            "Docker 构建参数过多",
        ));
    }
    for argument in &input.build_args {
        if argument.len() > 2048 {
            return Err(AppError::new(
                "VALIDATION_FAILED",
                "validation",
                "Docker 构建参数过长",
            ));
        }
        validate_environment(argument)?;
    }
    if let Some(dockerfile_path) = input.dockerfile_path.as_deref() {
        validate_build_file(dockerfile_path)?;
    }
    let mut parts = vec![
        "docker".to_string(),
        "build".to_string(),
        "--progress=plain".to_string(),
        "-t".to_string(),
        crate::security::shell_escape(&input.image),
    ];
    if let Some(dockerfile_path) = input.dockerfile_path.as_deref() {
        parts.extend(["-f".into(), crate::security::shell_escape(dockerfile_path)]);
    }
    for argument in &input.build_args {
        parts.extend([
            "--build-arg".into(),
            crate::security::shell_escape(argument),
        ]);
    }
    parts.extend([
        "--".into(),
        crate::security::shell_escape(&input.context_path),
    ]);
    let command = parts.join(" ");
    let result = if input.sudo {
        ssh.execute_stream_privileged_task(
            &input.server_id,
            &command,
            Duration::from_secs(3600),
            events,
            &input.task_id,
        )
        .await?
    } else {
        ssh.execute_stream_task(
            &input.server_id,
            &command,
            Duration::from_secs(3600),
            events,
            &input.task_id,
        )
        .await?
    };
    if result.exit_code != 0 {
        return Err(
            AppError::new("DOCKER_BUILD_FAILED", "docker", "构建 Docker 镜像失败")
                .details(result.stderr)
                .for_server(&input.server_id),
        );
    }
    Ok(DockerBuildResult {
        image: input.image,
        context_path: input.context_path,
        output: format!("{}\n{}", result.stdout, result.stderr),
    })
}

/// 按受控表单创建容器，并用 inspect 验证 Docker 返回的容器 ID。
pub async fn run(ssh: &SshConnectionManager, input: DockerRunInput) -> AppResult<DockerRunResult> {
    validate_image(&input.image)?;
    if input.privileged && !input.confirmed {
        return Err(AppError::new(
            "CONFIRMATION_REQUIRED",
            "confirmation",
            "以 privileged 运行容器必须经过用户确认",
        )
        .for_server(&input.server_id));
    }
    if let Some(name) = input.name.as_deref() {
        validate_name(name)?;
    }
    for port in &input.ports {
        validate_token(port, "端口映射")?;
    }
    for environment in &input.environment {
        validate_environment(environment)?;
    }
    if let Some(network) = input.network.as_deref() {
        validate_token(network, "网络名")?;
    }
    if let Some(policy) = input.restart_policy.as_deref() {
        validate_restart_policy(policy)?;
    }
    let mut parts = vec!["docker".into(), "run".into(), "-d".into()];
    if let Some(name) = input.name.as_deref() {
        parts.extend(["--name".into(), crate::security::shell_escape(name)]);
    }
    for port in &input.ports {
        parts.extend(["-p".into(), crate::security::shell_escape(port)]);
    }
    for environment in &input.environment {
        parts.extend(["-e".into(), crate::security::shell_escape(environment)]);
    }
    if let Some(network) = input.network.as_deref() {
        parts.extend(["--network".into(), crate::security::shell_escape(network)]);
    }
    if let Some(policy) = input.restart_policy.as_deref() {
        parts.extend(["--restart".into(), crate::security::shell_escape(policy)]);
    }
    if input.auto_remove {
        parts.push("--rm".into());
    }
    if input.privileged {
        parts.push("--privileged".into());
    }
    parts.push(input.image.clone());
    let command = parts.join(" ");
    let result = if input.sudo {
        ssh.execute_privileged(&input.server_id, &command, Duration::from_secs(120))
            .await?
    } else {
        ssh.execute(&input.server_id, &command, Duration::from_secs(120))
            .await?
    };
    if result.exit_code != 0 {
        return Err(AppError::new("DOCKER_RUN_FAILED", "docker", "创建容器失败")
            .details(result.stderr)
            .for_server(&input.server_id));
    }
    let container_id = result
        .stdout
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .ok_or_else(|| {
            AppError::new(
                "DOCKER_VERIFY_FAILED",
                "docker",
                "创建成功但没有返回容器 ID",
            )
            .for_server(&input.server_id)
        })?
        .to_string();
    validate_name(&container_id)?;
    let inspect_command = format!(
        "docker inspect --format '{{{{.Id}}}}' -- {}",
        crate::security::shell_escape(&container_id)
    );
    let verify = if input.sudo {
        ssh.execute_privileged(&input.server_id, &inspect_command, Duration::from_secs(20))
            .await?
    } else {
        ssh.execute(&input.server_id, &inspect_command, Duration::from_secs(20))
            .await?
    };
    if verify.exit_code != 0 {
        return Err(AppError::new(
            "DOCKER_VERIFY_FAILED",
            "docker",
            "容器创建后 inspect 验证失败",
        )
        .details(verify.stderr)
        .for_server(&input.server_id));
    }
    Ok(DockerRunResult {
        container_id,
        output: result.stdout,
    })
}

/// 在指定容器内执行一次用户明确输入的命令；容器名和命令边界均由 Rust 端转义。
pub async fn exec(
    ssh: &SshConnectionManager,
    input: DockerExecInput,
) -> AppResult<DockerTextResult> {
    validate_name(&input.container_id)?;
    validate_exec_command(&input.command)?;
    let command = format!(
        "docker exec -- {} sh -lc {}",
        crate::security::shell_escape(&input.container_id),
        crate::security::shell_escape(&input.command)
    );
    let result = if input.sudo {
        ssh.execute_privileged(&input.server_id, &command, Duration::from_secs(120))
            .await?
    } else {
        ssh.execute(&input.server_id, &command, Duration::from_secs(120))
            .await?
    };
    if result.exit_code != 0 {
        return Err(
            AppError::new("DOCKER_EXEC_FAILED", "docker", "容器命令执行失败")
                .details(result.stderr)
                .for_server(&input.server_id),
        );
    }
    Ok(DockerTextResult {
        container_id: input.container_id,
        output: result.stdout,
    })
}

/// 在远端执行最多 30 秒的日志 follow，并允许 UI 取消实时输出 task。
pub async fn follow_logs(
    ssh: &SshConnectionManager,
    server_id: &str,
    container_id: &str,
    tail: u32,
    sudo: bool,
    task_id: &str,
    events: &tauri::ipc::Channel<CommandEvent>,
) -> AppResult<DockerLogs> {
    validate_name(container_id)?;
    let tail = tail.clamp(1, 10_000);
    let command = format!(
        "timeout --signal=TERM 30s docker logs --timestamps --follow --tail {tail} -- {}",
        crate::security::shell_escape(container_id)
    );
    let result = if sudo {
        ssh.execute_stream_privileged_task(
            server_id,
            &command,
            Duration::from_secs(40),
            events,
            task_id,
        )
        .await?
    } else {
        ssh.execute_stream_task(
            server_id,
            &command,
            Duration::from_secs(40),
            events,
            task_id,
        )
        .await?
    };
    if result.exit_code != 0 && result.exit_code != 124 {
        return Err(
            AppError::new("DOCKER_LOGS_FAILED", "docker", "跟随容器日志失败")
                .details(result.stderr)
                .for_server(server_id),
        );
    }
    Ok(DockerLogs {
        container_id: container_id.into(),
        output: format!("{}{}", result.stdout, result.stderr),
    })
}

/// 创建或删除 Docker volume/network，并在删除前强制要求 UI 确认。
pub async fn resource_action(
    ssh: &SshConnectionManager,
    input: DockerResourceActionInput,
) -> AppResult<DockerResourceActionResult> {
    if !matches!(input.kind.as_str(), "volume" | "network")
        || !matches!(input.action.as_str(), "create" | "remove")
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "validation",
            "Docker 资源操作无效",
        ));
    }
    validate_name(&input.name)?;
    if input.action == "remove" && !input.confirmed {
        return Err(AppError::new(
            "CONFIRMATION_REQUIRED",
            "confirmation",
            "删除 Docker 资源必须经过用户确认",
        )
        .for_server(&input.server_id));
    }
    let command = format!(
        "docker {} {} {}",
        input.kind,
        input.action,
        crate::security::shell_escape(&input.name)
    );
    let result = if input.sudo {
        ssh.execute_privileged(&input.server_id, &command, Duration::from_secs(60))
            .await?
    } else {
        ssh.execute(&input.server_id, &command, Duration::from_secs(60))
            .await?
    };
    if result.exit_code != 0 {
        return Err(
            AppError::new("DOCKER_RESOURCE_FAILED", "docker", "Docker 资源操作失败")
                .details(result.stderr)
                .for_server(&input.server_id),
        );
    }
    let verify_command = format!(
        "docker {} inspect -- {}",
        input.kind,
        crate::security::shell_escape(&input.name)
    );
    let verify = if input.sudo {
        ssh.execute_privileged(&input.server_id, &verify_command, Duration::from_secs(20))
            .await?
    } else {
        ssh.execute(&input.server_id, &verify_command, Duration::from_secs(20))
            .await?
    };
    let expected = input.action == "create";
    if (verify.exit_code == 0) != expected {
        return Err(AppError::new(
            "DOCKER_VERIFY_FAILED",
            "docker",
            "Docker 资源操作后状态验证失败",
        )
        .details(verify.stderr)
        .for_server(&input.server_id));
    }
    Ok(DockerResourceActionResult {
        kind: input.kind,
        name: input.name,
        action: input.action,
        verified: true,
    })
}

/// 删除指定镜像并验证镜像引用不再可 inspect；删除始终要求用户确认。
pub async fn image_action(
    ssh: &SshConnectionManager,
    input: DockerImageActionInput,
) -> AppResult<DockerResourceActionResult> {
    validate_image(&input.image)?;
    if input.action != "remove" {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "validation",
            "不支持的 Docker 镜像操作",
        ));
    }
    if !input.confirmed {
        return Err(AppError::new(
            "CONFIRMATION_REQUIRED",
            "confirmation",
            "删除 Docker 镜像必须经过用户确认",
        )
        .for_server(&input.server_id));
    }
    let command = format!(
        "docker image rm {} {}",
        if input.force { "--force" } else { "" },
        crate::security::shell_escape(&input.image)
    );
    let result = if input.sudo {
        ssh.execute_privileged(&input.server_id, &command, Duration::from_secs(120))
            .await?
    } else {
        ssh.execute(&input.server_id, &command, Duration::from_secs(120))
            .await?
    };
    if result.exit_code != 0 {
        return Err(
            AppError::new("DOCKER_IMAGE_FAILED", "docker", "删除 Docker 镜像失败")
                .details(result.stderr)
                .for_server(&input.server_id),
        );
    }
    let verify_command = format!(
        "docker image inspect -- {}",
        crate::security::shell_escape(&input.image)
    );
    let verify = if input.sudo {
        ssh.execute_privileged(&input.server_id, &verify_command, Duration::from_secs(30))
            .await?
    } else {
        ssh.execute(&input.server_id, &verify_command, Duration::from_secs(30))
            .await?
    };
    if verify.exit_code == 0 {
        return Err(AppError::new(
            "DOCKER_VERIFY_FAILED",
            "docker",
            "镜像删除命令完成但 inspect 仍可找到该镜像",
        )
        .for_server(&input.server_id));
    }
    Ok(DockerResourceActionResult {
        kind: "image".into(),
        name: input.image,
        action: "remove".into(),
        verified: true,
    })
}

/// 读取 Docker volume 或 network 的原始 inspect JSON，不执行远端变更。
pub async fn resource_inspect(
    ssh: &SshConnectionManager,
    input: DockerResourceInspectInput,
) -> AppResult<DockerTextResult> {
    if !matches!(input.kind.as_str(), "volume" | "network") {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "validation",
            "Docker 资源类型无效",
        ));
    }
    validate_name(&input.name)?;
    let command = format!(
        "docker {} inspect -- {}",
        input.kind,
        crate::security::shell_escape(&input.name)
    );
    let result = if input.sudo {
        ssh.execute_privileged(&input.server_id, &command, Duration::from_secs(30))
            .await?
    } else {
        ssh.execute(&input.server_id, &command, Duration::from_secs(30))
            .await?
    };
    if result.exit_code != 0 {
        return Err(AppError::new(
            "DOCKER_INSPECT_FAILED",
            "docker",
            "读取 Docker 资源 inspect 失败",
        )
        .details(result.stderr)
        .for_server(&input.server_id));
    }
    Ok(DockerTextResult {
        container_id: input.name,
        output: result.stdout,
    })
}

/// 执行 Compose 项目的受控生命周期动作，并验证项目列表可再次读取。
pub async fn compose_action(
    ssh: &SshConnectionManager,
    input: DockerComposeActionInput,
) -> AppResult<DockerResourceActionResult> {
    if !matches!(
        input.action.as_str(),
        "up" | "down" | "start" | "stop" | "restart" | "pull" | "build" | "cleanup"
    ) {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "validation",
            "Compose 操作无效",
        ));
    }
    validate_name(&input.project)?;
    if let Some(working_dir) = input.working_dir.as_deref() {
        validate_working_dir(working_dir)?;
    }
    if matches!(input.action.as_str(), "down" | "stop" | "cleanup") && !input.confirmed {
        return Err(AppError::new(
            "CONFIRMATION_REQUIRED",
            "confirmation",
            "停止或清理 Compose 项目必须经过用户确认",
        )
        .for_server(&input.server_id));
    }
    let prefix = input
        .working_dir
        .as_deref()
        .map(|path| format!("cd {} && ", crate::security::shell_escape(path)))
        .unwrap_or_default();
    let command = if input.action == "up" {
        format!(
            "{prefix}docker compose --project-name {} up -d",
            crate::security::shell_escape(&input.project)
        )
    } else if input.action == "cleanup" {
        format!(
            "{prefix}docker compose --project-name {} down --remove-orphans --volumes",
            crate::security::shell_escape(&input.project)
        )
    } else {
        format!(
            "{prefix}docker compose --project-name {} {}",
            crate::security::shell_escape(&input.project),
            input.action
        )
    };
    let result = if input.sudo {
        ssh.execute_privileged(&input.server_id, &command, Duration::from_secs(300))
            .await?
    } else {
        ssh.execute(&input.server_id, &command, Duration::from_secs(300))
            .await?
    };
    if result.exit_code != 0 {
        return Err(
            AppError::new("DOCKER_COMPOSE_FAILED", "docker", "Compose 项目操作失败")
                .details(result.stderr)
                .for_server(&input.server_id),
        );
    }
    let verify = compose_execute(
        ssh,
        &input.server_id,
        input.working_dir.as_deref(),
        &input.project,
        "ls --all --format json",
        input.sudo,
        Duration::from_secs(30),
    )
    .await?;
    if verify.exit_code != 0 {
        return Err(AppError::new(
            "DOCKER_VERIFY_FAILED",
            "docker",
            "Compose 操作后项目列表验证失败",
        )
        .details(verify.stderr)
        .for_server(&input.server_id));
    }
    Ok(DockerResourceActionResult {
        kind: "compose".into(),
        name: input.project,
        action: input.action,
        verified: true,
    })
}

/// 读取 Compose 项目的服务状态、渲染配置和可清理资源候选；结果不写入本地。
pub async fn compose_details(
    ssh: &SshConnectionManager,
    server_id: &str,
    project: &str,
    working_dir: Option<&str>,
    sudo: bool,
) -> AppResult<DockerComposeDetails> {
    validate_name(project)?;
    if let Some(path) = working_dir {
        validate_working_dir(path)?;
    }
    let (
        services,
        config,
        volumes,
        networks,
        cleanup_volumes,
        cleanup_networks,
        compose_containers,
        compose_services,
    ) = tokio::join!(
        compose_execute(
            ssh,
            server_id,
            working_dir,
            project,
            "ps --all --format json",
            sudo,
            Duration::from_secs(30),
        ),
        compose_execute(
            ssh,
            server_id,
            working_dir,
            project,
            "config",
            sudo,
            Duration::from_secs(45),
        ),
        compose_execute(
            ssh,
            server_id,
            working_dir,
            project,
            "config --volumes",
            sudo,
            Duration::from_secs(30),
        ),
        compose_execute(
            ssh,
            server_id,
            working_dir,
            project,
            "config --networks",
            sudo,
            Duration::from_secs(30),
        ),
        docker_label_list(
            ssh,
            server_id,
            "volume",
            project,
            sudo,
            Duration::from_secs(30),
        ),
        docker_label_list(
            ssh,
            server_id,
            "network",
            project,
            sudo,
            Duration::from_secs(30),
        ),
        docker_label_list(
            ssh,
            server_id,
            "container",
            project,
            sudo,
            Duration::from_secs(30),
        ),
        compose_execute(
            ssh,
            server_id,
            working_dir,
            project,
            "config --services",
            sudo,
            Duration::from_secs(30),
        )
    );
    let services = services?.stdout;
    let config = config?;
    if config.exit_code != 0 {
        return Err(AppError::new(
            "DOCKER_COMPOSE_CONFIG_FAILED",
            "docker",
            "读取 Compose 渲染配置失败",
        )
        .details(config.stderr)
        .for_server(server_id));
    }
    let project_listing = compose_execute(
        ssh,
        server_id,
        working_dir,
        project,
        "ls --all --format json",
        sudo,
        Duration::from_secs(30),
    )
    .await
    .ok()
    .map(|value| parse_compose(&value.stdout))
    .unwrap_or_default();
    let config_path = project_listing
        .into_iter()
        .find(|value| value.name == project)
        .and_then(|value| {
            value
                .config_files
                .split(',')
                .next()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        });
    let (config_size, config_modified_at) = if let Some(path) = config_path.as_deref() {
        config_metadata(ssh, server_id, path).await
    } else {
        (None, None)
    };
    let declared_volumes = volumes
        .ok()
        .map(|result| parse_name_lines(&result.stdout))
        .unwrap_or_default();
    let declared_networks = networks
        .ok()
        .map(|result| parse_name_lines(&result.stdout))
        .unwrap_or_default();
    let cleanup_volumes = cleanup_volumes
        .ok()
        .map(|result| parse_name_lines(&result.stdout))
        .unwrap_or_default();
    let cleanup_networks = cleanup_networks
        .ok()
        .map(|result| parse_name_lines(&result.stdout))
        .unwrap_or_default();
    let expected_services = compose_services
        .ok()
        .map(|result| parse_name_lines(&result.stdout))
        .unwrap_or_default();
    let mut cleanup_warnings = Vec::new();
    if !declared_volumes.is_empty() && cleanup_volumes.len() < declared_volumes.len() {
        cleanup_warnings.push(
            "部分声明卷没有 com.docker.compose.project 标签，可能是 external 或尚未创建；cleanup 不会盲删它们。"
                .into(),
        );
    }
    if !declared_networks.is_empty() && cleanup_networks.len() < declared_networks.len() {
        cleanup_warnings.push(
            "部分声明网络没有 com.docker.compose.project 标签，可能是 external 或尚未创建；cleanup 不会盲删它们。"
                .into(),
        );
    }
    let orphan_containers = compose_containers
        .ok()
        .map(|result| parse_orphan_containers(&result.stdout, &expected_services))
        .unwrap_or_default();
    if !orphan_containers.is_empty() {
        cleanup_warnings
            .push("存在不属于当前 Compose 配置的同项目孤儿容器，cleanup 会移除它们。".into());
    }
    Ok(DockerComposeDetails {
        project: project.into(),
        services: parse_compose_services(&services),
        config: redact_compose_config(&config.stdout),
        config_path,
        config_size,
        config_modified_at,
        volumes: declared_volumes,
        networks: declared_networks,
        cleanup_volumes,
        cleanup_networks,
        orphan_containers,
        cleanup_warnings,
    })
}

/// 读取 Compose YAML 文件的路径和版本元数据，不读取文件内容。
async fn config_metadata(
    ssh: &SshConnectionManager,
    server_id: &str,
    path: &str,
) -> (Option<u64>, Option<u32>) {
    let Ok(sftp) = ssh.open_sftp(server_id).await else {
        return (None, None);
    };
    let result = sftp.metadata(path).await.ok();
    let _ = sftp.close().await;
    result
        .map(|value| (Some(value.len()), value.mtime))
        .unwrap_or((None, None))
}

/// 保存用户明确确认的 Compose YAML，先校验配置，失败时恢复原文件。
pub async fn save_compose_yaml(
    ssh: &SshConnectionManager,
    input: DockerComposeYamlInput,
) -> AppResult<crate::domain::files::RemoteTextFile> {
    validate_name(&input.project)?;
    if let Some(path) = input.working_dir.as_deref() {
        validate_working_dir(path)?;
    }
    validate_config_path(&input.config_path)?;
    if !input.confirmed {
        return Err(AppError::new(
            "CONFIRMATION_REQUIRED",
            "confirmation",
            "保存 Compose YAML 必须经过用户确认",
        )
        .for_server(&input.server_id));
    }
    let original =
        crate::domain::files::read_text(ssh, &input.server_id, &input.config_path).await?;
    let save_input = || crate::domain::files::SaveTextInput {
        server_id: input.server_id.clone(),
        path: input.config_path.clone(),
        content: input.content.clone(),
        expected_size: input.expected_size,
        expected_modified_at: input.expected_modified_at,
        force: input.force,
    };
    let saved = if input.sudo {
        crate::domain::files::save_text_privileged(ssh, save_input()).await?
    } else {
        crate::domain::files::save_text(ssh, save_input()).await?
    };
    let suffix = format!(
        "-f {} config -q",
        crate::security::shell_escape(&input.config_path)
    );
    let checked = compose_execute(
        ssh,
        &input.server_id,
        input.working_dir.as_deref(),
        &input.project,
        &suffix,
        input.sudo,
        Duration::from_secs(45),
    )
    .await?;
    if checked.exit_code != 0 {
        let restore_input = crate::domain::files::SaveTextInput {
            server_id: input.server_id.clone(),
            path: input.config_path.clone(),
            content: original.content,
            expected_size: saved.size,
            expected_modified_at: saved.modified_at,
            force: true,
        };
        let restored = if input.sudo {
            crate::domain::files::save_text_privileged(ssh, restore_input).await
        } else {
            crate::domain::files::save_text(ssh, restore_input).await
        };
        if let Err(error) = restored {
            return Err(AppError::new(
                "DOCKER_ROLLBACK_FAILED",
                "docker",
                "Compose 配置无效且自动恢复失败",
            )
            .details(error)
            .for_server(&input.server_id)
            .fatal());
        }
        return Err(AppError::new(
            "DOCKER_COMPOSE_CONFIG_INVALID",
            "docker",
            "Compose YAML 校验失败，已恢复原文件",
        )
        .details(checked.stderr)
        .for_server(&input.server_id));
    }
    Ok(saved)
}

/// 读取 Compose 项目或单个服务的最近日志，限制输出行数。
pub async fn compose_logs(
    ssh: &SshConnectionManager,
    server_id: &str,
    project: &str,
    working_dir: Option<&str>,
    service: Option<&str>,
    tail: u32,
    sudo: bool,
) -> AppResult<DockerLogs> {
    validate_name(project)?;
    if let Some(path) = working_dir {
        validate_working_dir(path)?;
    }
    if let Some(name) = service {
        validate_name(name)?;
    }
    let tail = tail.clamp(1, 10_000);
    let service_arg = service
        .map(|name| format!(" {}", crate::security::shell_escape(name)))
        .unwrap_or_default();
    let result = compose_execute(
        ssh,
        server_id,
        working_dir,
        project,
        &format!("logs --no-color --timestamps --tail {tail}{service_arg}"),
        sudo,
        Duration::from_secs(45),
    )
    .await?;
    if result.exit_code != 0 {
        return Err(AppError::new(
            "DOCKER_COMPOSE_LOGS_FAILED",
            "docker",
            "读取 Compose 日志失败",
        )
        .details(result.stderr)
        .for_server(server_id));
    }
    Ok(DockerLogs {
        container_id: service
            .map(|name| format!("{project}/{name}"))
            .unwrap_or_else(|| project.into()),
        output: result.stdout,
    })
}

/// 执行固定 Compose 子命令，统一处理工作目录和远程 sudo 边界。
async fn compose_execute(
    ssh: &SshConnectionManager,
    server_id: &str,
    working_dir: Option<&str>,
    project: &str,
    suffix: &str,
    sudo: bool,
    timeout: Duration,
) -> AppResult<RemoteCommandResult> {
    let prefix = working_dir
        .map(|path| format!("cd {} && ", crate::security::shell_escape(path)))
        .unwrap_or_default();
    let command = format!(
        "{prefix}docker compose --project-name {} {suffix}",
        crate::security::shell_escape(project)
    );
    if sudo {
        ssh.execute_privileged(server_id, &command, timeout).await
    } else {
        ssh.execute(server_id, &command, timeout).await
    }
}

/// 读取当前 Compose 项目带有 project label 的真实 Docker 资源；仅用于清理预览。
async fn docker_label_list(
    ssh: &SshConnectionManager,
    server_id: &str,
    kind: &str,
    project: &str,
    sudo: bool,
    timeout: Duration,
) -> AppResult<RemoteCommandResult> {
    let (list_command, format_arg) = match kind {
        "volume" => ("volume ls", "'{{.Name}}'"),
        "network" => ("network ls", "'{{.Name}}'"),
        "container" => (
            "ps -a",
            "'{{.Names}}\\t{{.Label \"com.docker.compose.service\"}}'",
        ),
        _ => {
            return Err(AppError::new(
                "VALIDATION_FAILED",
                "validation",
                "Docker label 资源类型无效",
            ));
        }
    };
    let command = format!(
        "docker {list_command} --filter label=com.docker.compose.project={} --format {format_arg}",
        crate::security::shell_escape(project)
    );
    if sudo {
        ssh.execute_privileged(server_id, &command, timeout).await
    } else {
        ssh.execute(server_id, &command, timeout).await
    }
}

/// 构造未安装 Docker 时返回的稳定空状态。
fn empty_snapshot() -> DockerSnapshot {
    DockerSnapshot {
        installed: false,
        running: false,
        version: None,
        api_version: None,
        os: None,
        architecture: None,
        storage_driver: None,
        cgroup_version: None,
        root_dir: None,
        disk_usage: None,
        containers: Vec::new(),
        images: Vec::new(),
        volumes: Vec::new(),
        networks: Vec::new(),
        compose_projects: Vec::new(),
    }
}
/// 校验来自 UI 的 Docker 对象标识，避免命令参数注入。
fn validate_name(value: &str) -> AppResult<()> {
    if value.is_empty()
        || value.len() > 128
        || !value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-' | '/')
        })
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "validation",
            "Docker 对象名称无效",
        ));
    }
    Ok(())
}
/// 校验 Docker 容器名称，拒绝路径分隔符以匹配 Docker rename 的名称语法。
fn validate_container_name(value: &str) -> AppResult<()> {
    if value.is_empty()
        || value.len() > 128
        || !value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-')
        })
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "validation",
            "Docker 容器名称无效",
        ));
    }
    Ok(())
}
/// 校验镜像引用，只允许 Docker registry 常用的非控制字符集合。
fn validate_image(value: &str) -> AppResult<()> {
    if value.is_empty()
        || value.len() > 256
        || !value.chars().all(|character| {
            character.is_ascii_alphanumeric()
                || matches!(character, '.' | '_' | '-' | '/' | ':' | '@')
        })
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "validation",
            "Docker 镜像引用无效",
        ));
    }
    Ok(())
}
/// 校验不会改变 Docker CLI 参数边界的端口、网络等 token。
fn validate_token(value: &str, label: &str) -> AppResult<()> {
    if value.is_empty()
        || value.len() > 128
        || !value.chars().all(|character| {
            character.is_ascii_alphanumeric()
                || matches!(character, '.' | '_' | '-' | '/' | ':' | '+')
        })
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "validation",
            format!("Docker {label}无效"),
        ));
    }
    Ok(())
}
/// 校验环境变量为 KEY=VALUE，并阻止控制字符进入远程命令。
fn validate_environment(value: &str) -> AppResult<()> {
    let Some((key, _)) = value.split_once('=') else {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "validation",
            "环境变量必须使用 KEY=VALUE 格式",
        ));
    };
    if key.is_empty()
        || !key.chars().enumerate().all(|(index, character)| {
            if index == 0 {
                character.is_ascii_alphabetic() || character == '_'
            } else {
                character.is_ascii_alphanumeric() || character == '_'
            }
        })
        || value
            .chars()
            .any(|character| character == '\n' || character == '\r' || character == '\0')
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "validation",
            "环境变量名称或值无效",
        ));
    }
    Ok(())
}
/// 校验容器 exec 命令长度和控制字符；命令本身仍作为显式 shell 输入执行。
fn validate_exec_command(value: &str) -> AppResult<()> {
    if value.trim().is_empty()
        || value.len() > 4096
        || value
            .chars()
            .any(|character| character == '\0' || character == '\r' || character == '\n')
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "validation",
            "容器命令不能为空或包含控制字符",
        ));
    }
    Ok(())
}

/// 校验 Compose 工作目录只包含可传给固定 shell 语句的非控制文本。
fn validate_working_dir(value: &str) -> AppResult<()> {
    if value.is_empty()
        || value.len() > 1024
        || value
            .chars()
            .any(|character| character == '\0' || character == '\r' || character == '\n')
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "validation",
            "Compose 工作目录无效",
        ));
    }
    Ok(())
}

/// 校验 Compose 配置文件路径；只允许绝对路径和无控制字符文本。
fn validate_config_path(value: &str) -> AppResult<()> {
    if value.is_empty()
        || value.len() > 2048
        || !value.starts_with('/')
        || value
            .chars()
            .any(|character| character == '\0' || character == '\r' || character == '\n')
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "validation",
            "Compose 配置文件路径无效",
        ));
    }
    Ok(())
}

/// 校验 Docker 构建上下文为远端绝对目录，并拒绝路径穿越和控制字符。
fn validate_build_context(value: &str) -> AppResult<()> {
    if value.is_empty()
        || value.len() > 2048
        || !value.starts_with('/')
        || value
            .chars()
            .any(|character| character == '\0' || character == '\r' || character == '\n')
        || value.split('/').any(|component| component == "..")
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "validation",
            "Docker 构建上下文路径无效",
        ));
    }
    Ok(())
}

/// 校验 Dockerfile 路径，允许相对上下文路径但不允许路径穿越或 option 注入。
fn validate_build_file(value: &str) -> AppResult<()> {
    if value.is_empty()
        || value.len() > 2048
        || value.starts_with('-')
        || value
            .chars()
            .any(|character| character == '\0' || character == '\r' || character == '\n')
        || value.split('/').any(|component| component == "..")
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "validation",
            "Dockerfile 路径无效",
        ));
    }
    Ok(())
}
/// 限制重启策略为 Docker 支持的固定值或 on-failure 次数。
fn validate_restart_policy(value: &str) -> AppResult<()> {
    if matches!(value, "no" | "always" | "unless-stopped")
        || value.strip_prefix("on-failure").is_some_and(|suffix| {
            suffix.is_empty() || suffix.starts_with(':') && suffix[1..].parse::<u32>().is_ok()
        })
    {
        Ok(())
    } else {
        Err(AppError::new(
            "VALIDATION_FAILED",
            "validation",
            "Docker 重启策略无效",
        ))
    }
}
/// 返回固定的 Docker CLI JSON 探测脚本。
fn snapshot_command() -> &'static str {
    "set -e; if ! command -v docker >/dev/null 2>&1; then printf 'docker: not found\\n' >&2; exit 127; fi; printf '__VERSION__\\n'; docker version --format '{{json .Server}}'; printf '__INFO__\\n'; docker info --format '{{json .}}'; printf '__CONTAINERS__\\n'; docker ps -a --format '{{json .}}'; printf '__CONTAINER_RESOURCES__\\n'; for id in $(docker ps -aq); do docker inspect --format '{{.Id}}\\t{{.HostConfig.RestartPolicy.Name}}\\t{{.HostConfig.NanoCpus}}\\t{{.HostConfig.Memory}}' \"$id\"; done; printf '__IMAGES__\\n'; docker images --format '{{json .}}'; printf '__VOLUMES__\\n'; docker volume ls --format '{{json .}}' 2>/dev/null || true; printf '__NETWORKS__\\n'; docker network ls --format '{{json .}}' 2>/dev/null || true; printf '__DISK__\\n'; docker system df --format '{{json .}}' 2>/dev/null || true; printf '__COMPOSE__\\n'; docker compose ls --all --format json 2>/dev/null || true"
}

/// 将带 section marker 的 Docker CLI 输出转换为强类型快照。
pub fn parse_snapshot(output: &str) -> Option<DockerSnapshot> {
    let sections = split_sections(output);
    let version: serde_json::Value = first_line(sections.get("VERSION")?)?.parse().ok()?;
    let info: serde_json::Value = first_line(sections.get("INFO")?)?.parse().ok()?;
    let resources = sections
        .get("CONTAINER_RESOURCES")
        .map(|value| parse_container_resources(value))
        .unwrap_or_default();
    let mut containers: Vec<ContainerInfo> = sections
        .get("CONTAINERS")
        .map(|value| value.lines().filter_map(parse_container).collect())
        .unwrap_or_default();
    for container in &mut containers {
        if let Some((_, resource)) = resources
            .iter()
            .find(|(id, _)| id.starts_with(&container.id) || container.id.starts_with(id.as_str()))
        {
            container.restart_policy = resource.restart_policy.clone();
            container.cpu_limit_nano = resource.cpu_limit_nano;
            container.memory_limit_bytes = resource.memory_limit_bytes;
        }
    }
    let images = sections
        .get("IMAGES")
        .map(|value| value.lines().filter_map(parse_image).collect())
        .unwrap_or_default();
    let volumes = sections
        .get("VOLUMES")
        .map(|value| value.lines().filter_map(parse_volume).collect())
        .unwrap_or_default();
    let networks = sections
        .get("NETWORKS")
        .map(|value| value.lines().filter_map(parse_network).collect())
        .unwrap_or_default();
    let disk_usage = sections
        .get("DISK")
        .and_then(|value| first_line(value))
        .map(str::to_string);
    let compose_projects = sections
        .get("COMPOSE")
        .map(|value| parse_compose(value))
        .unwrap_or_default();
    Some(DockerSnapshot {
        installed: true,
        running: true,
        version: version
            .get("Version")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
        api_version: version
            .get("ApiVersion")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
        os: info
            .get("OperatingSystem")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
        architecture: info
            .get("Architecture")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
        storage_driver: info
            .get("Driver")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
        cgroup_version: info
            .get("CgroupVersion")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
        root_dir: info
            .get("DockerRootDir")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
        disk_usage,
        containers,
        images,
        volumes,
        networks,
        compose_projects,
    })
}

/// 解析 Docker ps 的单行 JSON。
fn parse_container(line: &str) -> Option<ContainerInfo> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    let compose_project = value
        .get(r#"Label "com.docker.compose.project""#)
        .or_else(|| value.get("Label com.docker.compose.project"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);
    Some(ContainerInfo {
        id: value.get("ID")?.as_str()?.into(),
        name: value.get("Names")?.as_str()?.into(),
        image: value.get("Image")?.as_str()?.into(),
        status: value.get("Status")?.as_str()?.into(),
        health: value.get("Status")?.as_str().and_then(parse_health),
        created: value
            .get("CreatedAt")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .into(),
        ports: value
            .get("Ports")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .into(),
        compose_project,
        restart_policy: None,
        cpu_limit_nano: None,
        memory_limit_bytes: None,
    })
}

#[derive(Debug, Clone)]
struct ContainerResources {
    restart_policy: Option<String>,
    cpu_limit_nano: Option<i64>,
    memory_limit_bytes: Option<i64>,
}

/// 解析 docker inspect 的容器重启策略和 CPU/内存限制字段。
fn parse_container_resources(input: &str) -> std::collections::HashMap<String, ContainerResources> {
    input
        .lines()
        .filter_map(|line| {
            let fields: Vec<_> = line.split('\t').collect();
            let id = fields.first()?.trim();
            if id.is_empty() {
                return None;
            }
            Some((
                id.to_string(),
                ContainerResources {
                    restart_policy: fields
                        .get(1)
                        .map(|value| value.trim())
                        .filter(|value| !value.is_empty() && *value != "no")
                        .map(str::to_string),
                    cpu_limit_nano: fields
                        .get(2)
                        .and_then(|value| value.trim().parse().ok())
                        .filter(|value| *value > 0),
                    memory_limit_bytes: fields
                        .get(3)
                        .and_then(|value| value.trim().parse().ok())
                        .filter(|value| *value > 0),
                },
            ))
        })
        .collect()
}
/// 解析 Docker images 的单行 JSON。
fn parse_image(line: &str) -> Option<ImageInfo> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    let repository = value.get("Repository")?.as_str()?.to_string();
    let tag = value
        .get("Tag")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("<none>")
        .to_string();
    Some(ImageInfo {
        dangling: repository == "<none>" || tag == "<none>",
        repository,
        tag,
        id: value
            .get("ID")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .into(),
        size: value
            .get("Size")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .into(),
        created: value
            .get("CreatedAt")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .into(),
    })
}
/// 解析 Docker volume ls 的单行 JSON。
fn parse_volume(line: &str) -> Option<VolumeInfo> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    Some(VolumeInfo {
        name: value.get("Name")?.as_str()?.into(),
        driver: value
            .get("Driver")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .into(),
        mountpoint: value
            .get("Mountpoint")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .into(),
        labels: value
            .get("Labels")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .into(),
    })
}
/// 解析 Docker network ls 的单行 JSON。
fn parse_network(line: &str) -> Option<NetworkInfo> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    Some(NetworkInfo {
        id: value.get("ID")?.as_str()?.into(),
        name: value.get("Name")?.as_str()?.into(),
        driver: value
            .get("Driver")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .into(),
        scope: value
            .get("Scope")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .into(),
    })
}
/// 解析 Docker Compose v2 的 project JSON 数组。
fn parse_compose(value: &str) -> Vec<ComposeProject> {
    let parsed: serde_json::Value = serde_json::from_str(value).unwrap_or_default();
    parsed
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            Some(ComposeProject {
                name: entry.get("Name")?.as_str()?.into(),
                status: entry
                    .get("Status")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .into(),
                config_files: entry
                    .get("ConfigFiles")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .into(),
                working_dir: entry
                    .get("WorkingDir")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .into(),
            })
        })
        .collect()
}
/// 解析 Compose `ps --format json` 的服务数组，兼容 Docker Compose 字段缺失。
fn parse_compose_services(value: &str) -> Vec<DockerComposeService> {
    let parsed: serde_json::Value = serde_json::from_str(value).unwrap_or_default();
    parsed
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            Some(DockerComposeService {
                name: entry.get("Name")?.as_str()?.into(),
                service: entry
                    .get("Service")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .into(),
                image: entry
                    .get("Image")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .into(),
                state: entry
                    .get("State")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .into(),
                status: entry
                    .get("Status")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .into(),
                ports: entry
                    .get("Publishers")
                    .and_then(serde_json::Value::as_array)
                    .map(|publishers| {
                        publishers
                            .iter()
                            .filter_map(|publisher| {
                                let published = publisher.get("PublishedPort")?.as_u64()?;
                                let target = publisher.get("TargetPort")?.as_u64()?;
                                Some(format!("{published}:{target}"))
                            })
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_default(),
            })
        })
        .collect()
}

/// 解析 Docker events JSON 行，只保留类型、动作、actor 标识和秒级时间戳。
fn parse_events(value: &str) -> Vec<DockerEvent> {
    value
        .lines()
        .filter_map(|line| {
            let entry: serde_json::Value = serde_json::from_str(line).ok()?;
            let event_type = entry.get("Type")?.as_str()?.trim();
            let action = entry.get("Action")?.as_str()?.trim();
            let action_summary = action.split(':').next().unwrap_or(action).trim();
            if event_type.is_empty() || action_summary.is_empty() {
                return None;
            }
            let actor = entry.get("Actor").and_then(serde_json::Value::as_object);
            let attributes = actor
                .and_then(|value| value.get("Attributes"))
                .and_then(serde_json::Value::as_object);
            let actor_name = attributes
                .and_then(|value| value.get("name"))
                .and_then(serde_json::Value::as_str)
                .or_else(|| {
                    actor
                        .and_then(|value| value.get("Name"))
                        .and_then(serde_json::Value::as_str)
                })
                .map(|value| value.trim().trim_start_matches('/').to_string())
                .filter(|value| !value.is_empty())
                .map(|value| value.chars().take(160).collect::<String>());
            let actor_id = actor
                .and_then(|value| value.get("ID"))
                .and_then(serde_json::Value::as_str)
                .map(|value| value.chars().take(128).collect::<String>())
                .filter(|value| !value.is_empty());
            Some(DockerEvent {
                event_type: event_type.chars().take(40).collect(),
                action: action_summary.chars().take(40).collect(),
                actor_name,
                actor_id,
                timestamp: entry.get("time").and_then(serde_json::Value::as_i64),
            })
        })
        .take(200)
        .collect()
}

/// 解析 Compose 配置输出中的资源名称，丢弃空白行和重复项。
fn parse_name_lines(value: &str) -> Vec<String> {
    let mut names = value
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    names.sort();
    names.dedup();
    names
}

/// 根据 Compose 当前服务名过滤同项目但不再属于配置的孤儿容器。
fn parse_orphan_containers(value: &str, expected_services: &[String]) -> Vec<String> {
    if expected_services.is_empty() {
        return Vec::new();
    }
    let mut names = value
        .lines()
        .filter_map(|line| {
            let (name, service) = line.split_once('\t')?;
            let name = name.trim();
            let service = service.trim();
            if name.is_empty() || expected_services.iter().any(|item| item == service) {
                None
            } else {
                Some(name.to_string())
            }
        })
        .collect::<Vec<_>>();
    names.sort();
    names.dedup();
    names
}
/// 在把 Compose 渲染配置送到 WebView 前遮盖常见 secret-like 环境变量值。
fn redact_compose_config(value: &str) -> String {
    value
        .lines()
        .map(|line| {
            let trimmed = line.trim_start();
            if let Some((key, _)) = trimmed.split_once(':') {
                if is_secret_key(key.trim().trim_matches(['-', '"', '\''])) {
                    let prefix_len = line.len() - trimmed.len();
                    return format!("{}{}: ***REDACTED***", &line[..prefix_len], key.trim());
                }
            }
            if let Some((key, _)) = trimmed
                .strip_prefix("- ")
                .and_then(|item| item.split_once('='))
            {
                if is_secret_key(key.trim()) {
                    let prefix_len = line.len() - trimmed.len();
                    return format!("{}- {}=***REDACTED***", &line[..prefix_len], key.trim());
                }
            }
            line.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}
/// 判断配置键是否可能承载凭据或令牌。
fn is_secret_key(value: &str) -> bool {
    let key = value.to_ascii_lowercase().replace(['-', '.'], "_");
    [
        "password",
        "passwd",
        "secret",
        "token",
        "api_key",
        "private_key",
        "credential",
        "authorization",
    ]
    .iter()
    .any(|marker| key == *marker || key.contains(&format!("_{marker}")) || key.ends_with(marker))
}
/// 从 Docker status 文本中提取 health 状态。
fn parse_health(value: &str) -> Option<String> {
    value
        .split_once("(health:")
        .and_then(|(_, rest)| rest.split(')').next())
        .map(str::trim)
        .map(str::to_string)
}
/// 取得 section 中的第一条非空记录。
fn first_line(value: &str) -> Option<&str> {
    value.lines().map(str::trim).find(|line| !line.is_empty())
}
/// 按探测 marker 拆分 Docker CLI 输出。
fn split_sections(output: &str) -> std::collections::HashMap<String, String> {
    let mut result: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut current = String::new();
    for line in output.lines() {
        if line.starts_with("__") && line.ends_with("__") {
            current = line.trim_matches('_').into();
            result.entry(current.clone()).or_default();
        } else if !current.is_empty() {
            result.entry(current.clone()).or_default().push_str(line);
            result.entry(current.clone()).or_default().push('\n');
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::{
        parse_compose_services, parse_events, parse_orphan_containers, parse_snapshot,
        redact_compose_config, validate_build_context, validate_build_file, validate_image,
        validate_name, validate_restart_policy, validate_token,
    };

    #[test]
    fn parses_docker_json_sections() {
        let value =
            parse_snapshot(include_str!("../../../../fixtures/docker-snapshot.txt")).unwrap();
        assert_eq!(value.version.as_deref(), Some("27.0.1"));
        assert_eq!(value.root_dir.as_deref(), Some("/var/lib/docker"));
        assert_eq!(value.containers[0].name, "web");
        assert_eq!(
            value.containers[0].restart_policy.as_deref(),
            Some("unless-stopped")
        );
        assert_eq!(value.containers[0].cpu_limit_nano, Some(2_000_000_000));
        assert_eq!(value.containers[0].memory_limit_bytes, Some(536_870_912));
        assert_eq!(value.images[0].repository, "nginx");
        assert_eq!(value.volumes[0].name, "relay-data");
        assert_eq!(value.networks[0].name, "bridge");
        assert_eq!(value.compose_projects[0].name, "relay");
        assert!(!value.images[0].dangling);
    }

    #[test]
    fn parses_compose_service_json() {
        let services = parse_compose_services(
            r#"[{"Name":"relay-web-1","Service":"web","Image":"nginx:latest","State":"running","Status":"Up 2 minutes","Publishers":[{"PublishedPort":8080,"TargetPort":80}]}]"#,
        );
        assert_eq!(services[0].service, "web");
        assert_eq!(services[0].ports, "8080:80");
    }

    /// 确认 Docker 事件解析会舍弃未约定的 actor 属性并保留有限字段。
    #[test]
    fn parses_bounded_docker_events() {
        let events = parse_events(
            r#"{"Type":"container","Action":"start","Actor":{"ID":"abc123","Attributes":{"name":"/web","token":"secret"}},"time":1700000000}
{"Type":"network","Action":"connect","Actor":{"ID":"net-1","Attributes":{}}}"#,
        );
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event_type, "container");
        assert_eq!(events[0].actor_name.as_deref(), Some("web"));
        assert_eq!(events[0].timestamp, Some(1_700_000_000));
    }

    #[test]
    fn redacts_secret_like_compose_values() {
        let redacted =
            redact_compose_config("environment:\n  DB_PASSWORD: plain\n  - API_TOKEN=secret");
        assert!(!redacted.contains("plain"));
        assert!(!redacted.contains("secret"));
        assert!(redacted.contains("***REDACTED***"));
    }

    /// 确认清理预览只标记不属于当前 Compose service 列表的带标签容器。
    #[test]
    fn parses_compose_orphan_containers() {
        let orphans = parse_orphan_containers(
            "relay-web-1\tweb\nrelay-old-1\told\nrelay-unknown-1\t",
            &["web".into(), "db".into()],
        );
        assert_eq!(orphans, vec!["relay-old-1", "relay-unknown-1"]);
        assert!(parse_orphan_containers("relay-old-1\told", &[]).is_empty());
    }

    /// 确认 Docker UI 输入校验拒绝 shell 控制字符和不受支持的重启策略。
    #[test]
    fn rejects_unsafe_docker_identifiers() {
        assert!(validate_name("web; rm -rf /").is_err());
        assert!(super::validate_container_name("web/name").is_err());
        assert!(validate_image("nginx:latest && id").is_err());
        assert!(validate_token("8080;id", "端口").is_err());
        assert!(validate_restart_policy("always;id").is_err());
        assert!(validate_restart_policy("on-failure:3").is_ok());
        assert!(validate_build_context("/opt/app").is_ok());
        assert!(validate_build_context("/opt/../etc").is_err());
        assert!(validate_build_file("Dockerfile.prod").is_ok());
        assert!(validate_build_file("../Dockerfile").is_err());
    }
}

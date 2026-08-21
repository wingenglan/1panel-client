use crate::domain::ssh::{CommandEvent, SshConnectionManager};
use crate::errors::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::time::Duration;

const TOOL_IDS: &[&str] = &[
    "nginx", "docker", "git", "curl", "wget", "jq", "tar", "unzip", "rsync", "tmux", "htop", "lsof",
];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolStatus {
    pub id: String,
    pub name: String,
    pub description: String,
    pub installed: bool,
    pub version: Option<String>,
    pub running: Option<bool>,
    pub package_manager: Option<String>,
    pub install_package: Option<String>,
    pub requires_sudo: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolInstallPlan {
    pub tool: ToolStatus,
    pub command: String,
    pub risk: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallToolInput {
    pub server_id: String,
    pub tool_id: String,
    pub task_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolInstallResult {
    pub tool_id: String,
    pub output: String,
    pub verified: ToolStatus,
}

#[derive(Debug, Clone, Copy)]
struct ToolDefinition {
    id: &'static str,
    name: &'static str,
    description: &'static str,
    debian_package: &'static str,
    rhel_package: &'static str,
    daemon: bool,
}

const DEFINITIONS: &[ToolDefinition] = &[
    ToolDefinition {
        id: "nginx",
        name: "Nginx",
        description: "反向代理与静态文件服务",
        debian_package: "nginx",
        rhel_package: "nginx",
        daemon: true,
    },
    ToolDefinition {
        id: "docker",
        name: "Docker Engine",
        description: "容器运行时与 CLI",
        debian_package: "docker.io",
        rhel_package: "docker",
        daemon: true,
    },
    ToolDefinition {
        id: "git",
        name: "Git",
        description: "版本控制工具",
        debian_package: "git",
        rhel_package: "git",
        daemon: false,
    },
    ToolDefinition {
        id: "curl",
        name: "curl",
        description: "HTTP 与网络诊断工具",
        debian_package: "curl",
        rhel_package: "curl",
        daemon: false,
    },
    ToolDefinition {
        id: "wget",
        name: "wget",
        description: "文件下载工具",
        debian_package: "wget",
        rhel_package: "wget",
        daemon: false,
    },
    ToolDefinition {
        id: "jq",
        name: "jq",
        description: "JSON 命令行处理器",
        debian_package: "jq",
        rhel_package: "jq",
        daemon: false,
    },
    ToolDefinition {
        id: "tar",
        name: "tar",
        description: "归档与解包工具",
        debian_package: "tar",
        rhel_package: "tar",
        daemon: false,
    },
    ToolDefinition {
        id: "unzip",
        name: "unzip",
        description: "ZIP 解压工具",
        debian_package: "unzip",
        rhel_package: "unzip",
        daemon: false,
    },
    ToolDefinition {
        id: "rsync",
        name: "rsync",
        description: "增量文件同步工具",
        debian_package: "rsync",
        rhel_package: "rsync",
        daemon: false,
    },
    ToolDefinition {
        id: "tmux",
        name: "tmux",
        description: "持久化终端会话",
        debian_package: "tmux",
        rhel_package: "tmux",
        daemon: false,
    },
    ToolDefinition {
        id: "htop",
        name: "htop",
        description: "交互式进程查看器",
        debian_package: "htop",
        rhel_package: "htop",
        daemon: false,
    },
    ToolDefinition {
        id: "lsof",
        name: "lsof",
        description: "进程与文件占用诊断",
        debian_package: "lsof",
        rhel_package: "lsof",
        daemon: false,
    },
];

/// 通过远程标准命令探测工具是否存在、版本和 daemon 状态。
pub async fn list(ssh: &SshConnectionManager, server_id: &str) -> AppResult<Vec<ToolStatus>> {
    let result = ssh
        .execute(server_id, &detection_command(), Duration::from_secs(30))
        .await?;
    if result.exit_code != 0 {
        return Err(
            AppError::new("REMOTE_COMMAND_FAILED", "tools", "工具能力探测失败")
                .details(result.stderr)
                .for_server(server_id),
        );
    }
    let package_manager = detect_package_manager(&result.stdout);
    Ok(parse_detection(&result.stdout, package_manager.as_deref()))
}

/// 生成用户确认前展示的精确安装计划，不执行任何远程写入。
pub async fn install_plan(
    ssh: &SshConnectionManager,
    server_id: &str,
    tool_id: &str,
) -> AppResult<ToolInstallPlan> {
    let tool = list(ssh, server_id)
        .await?
        .into_iter()
        .find(|value| value.id == tool_id)
        .ok_or_else(|| invalid_tool(tool_id))?;
    if tool.installed {
        return Err(
            AppError::new("ALREADY_INSTALLED", "tools", "工具已经安装").for_server(server_id)
        );
    }
    let package_manager = tool.package_manager.clone().ok_or_else(|| {
        AppError::new(
            "UNSUPPORTED_PLATFORM",
            "tools",
            "未识别支持的 apt、dnf、apk 或 pacman 包管理器",
        )
        .for_server(server_id)
    })?;
    let package = tool.install_package.clone().ok_or_else(|| {
        AppError::new(
            "UNSUPPORTED_PLATFORM",
            "tools",
            "当前平台没有该工具的安装映射",
        )
        .for_server(server_id)
    })?;
    let adapter = crate::domain::platform::adapter_for(&package_manager);
    let command = adapter.install_command(&package);
    if command.is_empty() {
        return Err(
            AppError::new("UNSUPPORTED_PLATFORM", "tools", "不支持的包管理器")
                .for_server(server_id),
        );
    }
    Ok(ToolInstallPlan {
        tool,
        command,
        risk: "将通过系统包管理器安装，不会自动升级其它软件。需要 sudo 权限。".into(),
    })
}

/// 在用户明确确认后安装单个工具，流式转发输出并支持关闭远端 task，再重新探测验证二进制。
pub async fn install(
    ssh: &SshConnectionManager,
    input: InstallToolInput,
    events: &tauri::ipc::Channel<CommandEvent>,
) -> AppResult<ToolInstallResult> {
    let plan = install_plan(ssh, &input.server_id, &input.tool_id).await?;
    let result = ssh
        .execute_stream_privileged_task(
            &input.server_id,
            &plan.command,
            Duration::from_secs(300),
            events,
            &input.task_id,
        )
        .await?;
    if result.exit_code != 0 {
        return Err(
            AppError::new("TOOL_INSTALL_FAILED", "tools", "工具安装失败")
                .details(result.stderr)
                .for_server(&input.server_id),
        );
    }
    let verified = list(ssh, &input.server_id)
        .await?
        .into_iter()
        .find(|value| value.id == input.tool_id)
        .ok_or_else(|| {
            AppError::new("TOOL_VERIFY_FAILED", "tools", "安装后未找到工具")
                .for_server(&input.server_id)
        })?;
    if !verified.installed {
        return Err(
            AppError::new("TOOL_VERIFY_FAILED", "tools", "安装命令成功但工具验证失败")
                .for_server(&input.server_id),
        );
    }
    Ok(ToolInstallResult {
        tool_id: input.tool_id,
        output: format!("{}\n{}", result.stdout, result.stderr),
        verified,
    })
}

/// 将未知 registry id 转换为结构化校验错误。
fn invalid_tool(tool_id: &str) -> AppError {
    AppError::new("VALIDATION_FAILED", "validation", "未知工具").details(tool_id)
}

/// 生成只使用固定 registry 命令名的远端探测脚本。
fn detection_command() -> String {
    let commands = TOOL_IDS.join(" ");
    format!(
        "for command in {commands}; do if command -v \"$command\" >/dev/null 2>&1; then version=$(\"$command\" --version 2>&1 | head -n 1); running=na; case \"$command\" in nginx|docker) if command -v systemctl >/dev/null 2>&1 && systemctl is-active --quiet \"$command\"; then running=1; elif command -v rc-service >/dev/null 2>&1 && rc-service \"$command\" status >/dev/null 2>&1; then running=1; else running=0; fi;; esac; printf '%s\\tinstalled\\t%s\\t%s\\n' \"$command\" \"$version\" \"$running\"; else printf '%s\\tnot-installed\\t-\\tna\\n' \"$command\"; fi; done; printf '__PACKAGE_MANAGER__\\t'; if command -v apt-get >/dev/null 2>&1; then printf 'apt\\n'; elif command -v dnf >/dev/null 2>&1; then printf 'dnf\\n'; elif command -v yum >/dev/null 2>&1; then printf 'dnf\\n'; elif command -v apk >/dev/null 2>&1; then printf 'apk\\n'; elif command -v pacman >/dev/null 2>&1; then printf 'pacman\\n'; else printf 'unknown\\n'; fi"
    )
}

/// 从探测脚本的 marker 中读取支持的包管理器。
fn detect_package_manager(output: &str) -> Option<String> {
    output
        .lines()
        .find_map(|line| line.strip_prefix("__PACKAGE_MANAGER__\t"))
        .and_then(|value| match value.trim() {
            "apt" => Some("apt".into()),
            "dnf" => Some("dnf".into()),
            "apk" => Some("apk".into()),
            "pacman" => Some("pacman".into()),
            _ => None,
        })
}

/// 将 tab 分隔探测结果映射为完整 registry 状态列表。
fn parse_detection(output: &str, package_manager: Option<&str>) -> Vec<ToolStatus> {
    DEFINITIONS
        .iter()
        .map(|definition| {
            let line = output
                .lines()
                .find(|line| line.starts_with(&format!("{}\t", definition.id)));
            let (installed, version, running) = line.map(parse_line).unwrap_or((false, None, None));
            let package = package_manager.map(|manager| {
                if manager == "apt" {
                    definition.debian_package
                } else {
                    definition.rhel_package
                }
                .to_string()
            });
            ToolStatus {
                id: definition.id.into(),
                name: definition.name.into(),
                description: definition.description.into(),
                installed,
                version,
                running: definition.daemon.then_some(running.unwrap_or(false)),
                package_manager: package_manager.map(str::to_string),
                install_package: package,
                requires_sudo: true,
            }
        })
        .collect()
}

/// 解析单个工具的 installed/version/running 字段。
fn parse_line(line: &str) -> (bool, Option<String>, Option<bool>) {
    let fields: Vec<_> = line.splitn(4, '\t').collect();
    if fields.get(1) != Some(&"installed") {
        return (false, None, None);
    }
    let version = fields
        .get(2)
        .filter(|value| **value != "-")
        .map(|value| value.trim().to_string());
    let running = fields.get(3).and_then(|value| match value.trim() {
        "1" => Some(true),
        "0" => Some(false),
        _ => None,
    });
    (true, version, running)
}

#[cfg(test)]
mod tests {
    use super::{detect_package_manager, parse_detection, parse_line};

    #[test]
    fn parses_installed_and_daemon_state() {
        let (installed, version, running) =
            parse_line("nginx\tinstalled\tnginx version: nginx/1.24.0\t1");
        assert!(installed);
        assert_eq!(version.as_deref(), Some("nginx version: nginx/1.24.0"));
        assert_eq!(running, Some(true));
    }

    #[test]
    fn parses_registry_and_package_manager() {
        let output = include_str!("../../../../fixtures/tool-detection.txt");
        let values = parse_detection(output, Some("apt"));
        assert_eq!(values.len(), 12);
        assert!(values[0].installed);
        assert_eq!(values[0].install_package.as_deref(), Some("nginx"));
        assert!(!values[1].installed);
        assert_eq!(detect_package_manager(output).as_deref(), Some("apt"));
        assert_eq!(
            detect_package_manager("__PACKAGE_MANAGER__\tapk\n").as_deref(),
            Some("apk")
        );
        assert_eq!(
            detect_package_manager("__PACKAGE_MANAGER__\tpacman\n").as_deref(),
            Some("pacman")
        );
    }
}

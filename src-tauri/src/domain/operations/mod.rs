use crate::domain::ssh::SshConnectionManager;
use crate::errors::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationsSnapshot {
    pub processes: Vec<ProcessInfo>,
    pub ports: Vec<PortInfo>,
    pub ports_source: String,
    pub ports_warning: Option<String>,
    pub services: Vec<ServiceInfo>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessInfo {
    pub pid: u32,
    pub ppid: u32,
    pub user: String,
    pub state: String,
    pub cpu_percent: f64,
    pub memory_percent: f64,
    pub rss_bytes: u64,
    pub elapsed_seconds: u64,
    pub name: String,
    pub command: String,
    pub systemd_unit: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortInfo {
    pub protocol: String,
    pub local_address: String,
    pub port: u16,
    pub pid: Option<u32>,
    pub process_name: Option<String>,
    pub ipv6: bool,
    pub process_visible: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceInfo {
    pub name: String,
    pub load: String,
    pub active: String,
    pub sub: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceDetail {
    pub name: String,
    pub description: String,
    pub load: String,
    pub active: String,
    pub sub: String,
    pub main_pid: Option<u32>,
    pub fragment_path: String,
    pub unit_file_state: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceLogs {
    pub name: String,
    pub output: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminateProcessInput {
    pub server_id: String,
    pub pid: u32,
    pub port: Option<u16>,
    #[serde(default)]
    pub force: bool,
    #[serde(default)]
    pub privileged: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminationResult {
    pub pid: u32,
    pub signal: String,
    pub process_exited: bool,
    pub port_released: Option<bool>,
}

/// 读取进程、监听端口和 systemd 服务；端口探测支持 ss/lsof 与显式 sudo 重扫。
pub async fn snapshot(
    ssh: &SshConnectionManager,
    server_id: &str,
    privileged: bool,
) -> AppResult<OperationsSnapshot> {
    let (processes, ports, services) = tokio::join!(
        execute_probe(
            ssh,
            server_id,
            "ps -eo pid=,ppid=,user=,stat=,pcpu=,pmem=,rss=,etimes=,comm=,args=",
            Duration::from_secs(30),
            privileged,
        ),
        execute_probe(
            ssh,
            server_id,
            port_scan_command(),
            Duration::from_secs(30),
            privileged,
        ),
        execute_probe(
            ssh,
            server_id,
            "systemctl list-units --type=service --all --no-legend --no-pager --plain",
            Duration::from_secs(30),
            privileged,
        )
    );
    let processes = processes?;
    let ports = ports?;
    let services = services?;
    if processes.exit_code != 0 {
        return Err(remote_failure(server_id, "进程列表", processes.stderr));
    }
    if ports.exit_code != 0 {
        return Err(remote_failure(server_id, "端口扫描", ports.stderr));
    }
    let port_scan = parse_port_scan(&ports.stdout);
    Ok(OperationsSnapshot {
        processes: parse_processes(&processes.stdout),
        ports: port_scan.ports,
        ports_source: port_scan.source,
        ports_warning: port_scan.warning,
        services: if services.exit_code == 0 {
            parse_services(&services.stdout)
        } else {
            Vec::new()
        },
    })
}

/// 发送 SIGTERM/SIGKILL，并按用户选择的权限验证进程与端口是否释放。
pub async fn terminate(
    ssh: &SshConnectionManager,
    input: TerminateProcessInput,
) -> AppResult<TerminationResult> {
    if input.pid <= 1 {
        return Err(
            AppError::new("DANGEROUS_PROCESS", "security", "禁止结束 PID 0 或 PID 1")
                .for_server(&input.server_id)
                .fatal(),
        );
    }
    let signal = if input.force { "KILL" } else { "TERM" };
    let command = format!("kill -{signal} -- {}", input.pid);
    let result = if input.privileged {
        ssh.execute_privileged(&input.server_id, &command, Duration::from_secs(10))
            .await?
    } else {
        ssh.execute(&input.server_id, &command, Duration::from_secs(10))
            .await?
    };
    if result.exit_code != 0 {
        return Err(
            AppError::new("PERMISSION_DENIED", "process", "无法向目标进程发送信号")
                .details(result.stderr)
                .for_server(&input.server_id)
                .suggestion("检查进程权限，或通过已配置的 sudo 模式重试"),
        );
    }
    tokio::time::sleep(Duration::from_millis(900)).await;
    let verify = execute_probe(
        ssh,
        &input.server_id,
        &format!("kill -0 -- {} 2>/dev/null", input.pid),
        Duration::from_secs(10),
        input.privileged,
    )
    .await?;
    let process_exited = verify.exit_code != 0;
    let port_released = if let Some(port) = input.port {
        let ports = execute_probe(
            ssh,
            &input.server_id,
            port_scan_command(),
            Duration::from_secs(10),
            input.privileged,
        )
        .await?;
        let scan = parse_port_scan(&ports.stdout);
        Some(scan.source != "none" && !scan.ports.iter().any(|value| value.port == port))
    } else {
        None
    };
    Ok(TerminationResult {
        pid: input.pid,
        signal: signal.to_string(),
        process_exited,
        port_released,
    })
}

/// 执行 systemd 服务动作，并用 is-active/is-enabled 验证最终状态。
pub async fn service_action(
    ssh: &SshConnectionManager,
    server_id: &str,
    service: &str,
    action: &str,
) -> AppResult<()> {
    if !valid_unit(service)
        || !matches!(action, "start" | "stop" | "restart" | "enable" | "disable")
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "validation",
            "systemd 服务名或操作无效",
        ));
    }
    let command = format!(
        "systemctl {action} -- {}",
        crate::security::shell_escape(service)
    );
    let result = ssh
        .execute_privileged(server_id, &command, Duration::from_secs(30))
        .await?;
    if result.exit_code != 0 {
        return Err(
            AppError::new("REMOTE_COMMAND_FAILED", "systemd", "systemd 服务操作失败")
                .details(result.stderr)
                .for_server(server_id),
        );
    }
    let verification_command = if matches!(action, "enable" | "disable") {
        format!(
            "systemctl is-enabled --quiet -- {}",
            crate::security::shell_escape(service)
        )
    } else {
        format!(
            "systemctl is-active --quiet -- {}",
            crate::security::shell_escape(service)
        )
    };
    let verification = ssh
        .execute(server_id, &verification_command, Duration::from_secs(10))
        .await?;
    let should_be_active = matches!(action, "start" | "restart" | "enable");
    if (verification.exit_code == 0) != should_be_active {
        return Err(AppError::new(
            "SERVICE_VERIFY_FAILED",
            "systemd",
            "systemd 操作后状态与预期不符",
        )
        .for_server(server_id)
        .suggestion("重新扫描服务状态并检查 journal 日志"));
    }
    Ok(())
}

/// 读取 systemd 单元的状态、主进程和来源路径，不执行任何改变操作。
pub async fn service_detail(
    ssh: &SshConnectionManager,
    server_id: &str,
    service: &str,
) -> AppResult<ServiceDetail> {
    if !valid_unit(service) {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "validation",
            "systemd 服务名无效",
        ));
    }
    let result = ssh
        .execute(
            server_id,
            &format!(
                "systemctl show --no-pager -p Id,Description,LoadState,ActiveState,SubState,MainPID,FragmentPath,UnitFileState -- {}",
                crate::security::shell_escape(service)
            ),
            Duration::from_secs(20),
        )
        .await?;
    if result.exit_code != 0 {
        return Err(remote_failure(server_id, "systemctl show", result.stderr));
    }
    let values = parse_properties(&result.stdout);
    Ok(ServiceDetail {
        name: values.get("Id").cloned().unwrap_or_else(|| service.into()),
        description: values.get("Description").cloned().unwrap_or_default(),
        load: values.get("LoadState").cloned().unwrap_or_default(),
        active: values.get("ActiveState").cloned().unwrap_or_default(),
        sub: values.get("SubState").cloned().unwrap_or_default(),
        main_pid: values
            .get("MainPID")
            .and_then(|value| value.parse().ok())
            .filter(|pid| *pid > 0),
        fragment_path: values.get("FragmentPath").cloned().unwrap_or_default(),
        unit_file_state: values.get("UnitFileState").cloned().unwrap_or_default(),
    })
}

/// 读取指定 systemd 服务最近日志，限制行数并保持内容只在当前响应中存在。
pub async fn service_logs(
    ssh: &SshConnectionManager,
    server_id: &str,
    service: &str,
    lines: u32,
) -> AppResult<ServiceLogs> {
    if !valid_unit(service) {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "validation",
            "systemd 服务名无效",
        ));
    }
    let lines = lines.clamp(1, 2_000);
    let result = ssh
        .execute(
            server_id,
            &format!(
                "journalctl --no-pager -o short-iso -n {lines} -u {}",
                crate::security::shell_escape(service)
            ),
            Duration::from_secs(30),
        )
        .await?;
    if result.exit_code != 0 {
        return Err(remote_failure(server_id, "journalctl", result.stderr));
    }
    Ok(ServiceLogs {
        name: service.into(),
        output: result.stdout,
    })
}

pub fn parse_processes(output: &str) -> Vec<ProcessInfo> {
    output
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let pid = fields.next()?.parse().ok()?;
            let ppid = fields.next()?.parse().ok()?;
            let user = fields.next()?.to_string();
            let state = fields.next()?.to_string();
            let cpu_percent = fields.next()?.parse().unwrap_or(0.0);
            let memory_percent = fields.next()?.parse().unwrap_or(0.0);
            let rss_bytes = fields.next()?.parse::<u64>().unwrap_or(0) * 1024;
            let elapsed_seconds = fields.next()?.parse().unwrap_or(0);
            let name = fields.next()?.to_string();
            let command = fields.collect::<Vec<_>>().join(" ");
            Some(ProcessInfo {
                pid,
                ppid,
                user,
                state,
                cpu_percent,
                memory_percent,
                rss_bytes,
                elapsed_seconds,
                name,
                systemd_unit: infer_unit(&command),
                command,
            })
        })
        .collect()
}

pub fn parse_ports(output: &str) -> Vec<PortInfo> {
    output
        .lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() < 5 {
                return None;
            }
            let protocol = fields[0].to_string();
            let local = fields[4];
            let (local_address, port) = split_socket(local)?;
            let process_text = fields.get(6..).unwrap_or_default().join(" ");
            let pid = extract_number(&process_text, "pid=");
            let process_name = process_text
                .split("((\"")
                .nth(1)
                .and_then(|value| value.split('"').next())
                .map(ToString::to_string);
            Some(PortInfo {
                protocol,
                local_address: local_address.to_string(),
                port,
                pid,
                process_name,
                ipv6: local_address.contains(':'),
                process_visible: pid.is_some(),
            })
        })
        .collect()
}

#[derive(Debug, PartialEq, Eq)]
struct PortScan {
    ports: Vec<PortInfo>,
    source: String,
    warning: Option<String>,
}

/// 解析带来源标记的端口探测输出，并在 ss 不可用时支持 lsof 回退。
fn parse_port_scan(output: &str) -> PortScan {
    let mut source = "ss".to_string();
    let mut body = Vec::new();
    for line in output.lines() {
        if let Some(value) = line.strip_prefix("__PORT_SOURCE__=") {
            source = value.trim().to_string();
        } else {
            body.push(line);
        }
    }
    let body = body.join("\n");
    let ports = match source.as_str() {
        "lsof" => parse_lsof_ports(&body),
        "none" => Vec::new(),
        _ => parse_ports(&body),
    };
    let warning = match source.as_str() {
        "lsof" => Some("ss 不可用，已回退到 lsof；部分进程信息可能受权限限制".into()),
        "none" => Some("远端没有可用的 ss 或 lsof，无法读取监听端口".into()),
        _ => None,
    };
    PortScan {
        ports,
        source,
        warning,
    }
}

/// 解析 lsof 的网络监听行，保留协议、地址、端口和进程归属。
fn parse_lsof_ports(output: &str) -> Vec<PortInfo> {
    output
        .lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() < 9 || fields[0] == "COMMAND" {
                return None;
            }
            let pid = fields.get(1)?.parse().ok()?;
            let protocol = fields.get(7)?.to_lowercase();
            if !matches!(protocol.as_str(), "tcp" | "udp") {
                return None;
            }
            let local = fields.get(8)?.trim_end_matches("(LISTEN)");
            let (local_address, port) = split_socket(local)?;
            Some(PortInfo {
                protocol,
                local_address: local_address.to_string(),
                port,
                pid: Some(pid),
                process_name: Some(fields[0].to_string()),
                ipv6: fields.get(4).is_some_and(|value| *value == "IPv6"),
                process_visible: true,
            })
        })
        .collect()
}

/// 生成优先使用 ss、缺失时回退 lsof 的远端端口探测命令；权限不足时保留可解析的空结果，不让整个运行现场页面失败。
fn port_scan_command() -> &'static str {
    "if command -v ss >/dev/null 2>&1; then printf '__PORT_SOURCE__=ss\\n'; ss -H -lntup 2>/dev/null || ss -H -lntu 2>/dev/null || true; elif command -v lsof >/dev/null 2>&1; then printf '__PORT_SOURCE__=lsof\\n'; lsof -nP -iTCP -sTCP:LISTEN -iUDP 2>/dev/null || true; else printf '__PORT_SOURCE__=none\\n'; fi"
}

/// 根据 UI 的权限选择执行普通或 sudo 运行现场探测命令。
async fn execute_probe(
    ssh: &SshConnectionManager,
    server_id: &str,
    command: &str,
    timeout: Duration,
    privileged: bool,
) -> AppResult<crate::domain::ssh::RemoteCommandResult> {
    if privileged {
        ssh.execute_privileged(server_id, command, timeout).await
    } else {
        ssh.execute(server_id, command, timeout).await
    }
}

pub fn parse_services(output: &str) -> Vec<ServiceInfo> {
    output
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            Some(ServiceInfo {
                name: fields.next()?.trim_start_matches('●').to_string(),
                load: fields.next()?.to_string(),
                active: fields.next()?.to_string(),
                sub: fields.next()?.to_string(),
                description: fields.collect::<Vec<_>>().join(" "),
            })
        })
        .collect()
}

/// 解析 systemctl show 的 key=value 属性输出。
fn parse_properties(output: &str) -> std::collections::HashMap<String, String> {
    output
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

fn split_socket(value: &str) -> Option<(&str, u16)> {
    let index = value.rfind(':')?;
    Some((&value[..index], value[index + 1..].parse().ok()?))
}

fn extract_number(value: &str, marker: &str) -> Option<u32> {
    let start = value.find(marker)? + marker.len();
    let end = value[start..]
        .find(|character: char| !character.is_ascii_digit())
        .map(|offset| start + offset)
        .unwrap_or(value.len());
    value[start..end].parse().ok()
}

fn infer_unit(command: &str) -> Option<String> {
    command
        .split_whitespace()
        .find(|value| value.ends_with(".service"))
        .map(|value| {
            value
                .trim_matches(|character: char| {
                    !character.is_alphanumeric()
                        && character != '.'
                        && character != '@'
                        && character != '-'
                        && character != '_'
                })
                .to_string()
        })
}

fn valid_unit(value: &str) -> bool {
    value.ends_with(".service")
        && !value.is_empty()
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '@' | '-' | '_' | '\\')
        })
}

fn remote_failure(server_id: &str, command: &str, stderr: String) -> AppError {
    AppError::new(
        "REMOTE_COMMAND_FAILED",
        "remote_command",
        format!("远程命令 {command} 执行失败"),
    )
    .details(stderr)
    .for_server(server_id)
}

#[cfg(test)]
mod tests {
    use super::{parse_lsof_ports, parse_ports, parse_processes, parse_services};

    #[test]
    fn parses_process_fixture() {
        let values = parse_processes(include_str!("../../../../fixtures/ps-linux.txt"));
        assert_eq!(values[0].pid, 1);
        assert_eq!(values[1].rss_bytes, 42_000 * 1024);
        assert!(values[1].command.contains("server.js"));
    }

    #[test]
    fn parses_ss_ipv4_and_ipv6() {
        let values = parse_ports(include_str!("../../../../fixtures/ss-listen.txt"));
        assert_eq!(values[0].port, 8080);
        assert_eq!(values[0].pid, Some(1234));
        assert_eq!(values[0].process_name.as_deref(), Some("node"));
        assert!(values[1].ipv6);
        assert!(!values[1].process_visible);
    }

    #[test]
    fn parses_systemd_units() {
        let values = parse_services(include_str!("../../../../fixtures/systemd-services.txt"));
        assert_eq!(values[0].name, "nginx.service");
        assert_eq!(values[1].active, "failed");
    }

    #[test]
    fn parses_lsof_fallback() {
        let values = parse_lsof_ports(
            "COMMAND PID USER FD TYPE DEVICE SIZE/OFF NODE NAME\nnginx 88 root 6u IPv4 1 0t0 TCP *:8080 (LISTEN)",
        );
        assert_eq!(values[0].port, 8080);
        assert_eq!(values[0].process_name.as_deref(), Some("nginx"));
    }
}

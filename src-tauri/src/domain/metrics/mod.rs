use crate::domain::platform::{PermissionDiagnostic, ServerCapabilities};
use crate::domain::ssh::SshConnectionManager;
use crate::errors::{AppError, AppResult};
use serde::Serialize;
use std::collections::HashMap;
use std::time::Duration;

const PROBE_COMMAND: &str = r#"
printf '__OS__\n'; cat /etc/os-release 2>/dev/null || true
printf '__HOSTNAME__\n'; hostname 2>/dev/null || true
printf '__UNAME__\n'; uname -srm 2>/dev/null || true
printf '__ARCH__\n'; uname -m 2>/dev/null || true
printf '__UPTIME__\n'; cut -d. -f1 /proc/uptime 2>/dev/null || printf '0\n'
printf '__CPU_MODEL__\n'; awk -F: '/model name|Hardware/ {gsub(/^[ \t]+/,"",$2); print $2; exit}' /proc/cpuinfo 2>/dev/null
printf '__CORES__\n'; getconf _NPROCESSORS_ONLN 2>/dev/null || printf '1\n'
printf '__MEMINFO__\n'; cat /proc/meminfo 2>/dev/null || true
printf '__LOAD__\n'; cat /proc/loadavg 2>/dev/null || true
printf '__IDENTITY__\n'; printf '%s\n' "$(id -un 2>/dev/null)" "$(date --iso-8601=seconds 2>/dev/null)" "$(date +%Z 2>/dev/null)"
printf '__NETWORK__\n'; ip route get 1.1.1.1 2>/dev/null | awk '{for(i=1;i<=NF;i++) if($i=="src") {print $(i+1); exit}}'; ip route show default 2>/dev/null | awk '{print $3; exit}'
printf '__PLATFORM__\n'; if command -v apt-get >/dev/null; then echo apt; elif command -v dnf >/dev/null; then echo dnf; elif command -v yum >/dev/null; then echo yum; else echo unknown; fi; if command -v systemctl >/dev/null && systemctl is-system-running --quiet 2>/dev/null; then echo running; else echo degraded; fi
printf '__DF__\n'; df -B1 -P 2>/dev/null || true
printf '__CPU_A__\n'; head -n1 /proc/stat 2>/dev/null || true
printf '__NET_A__\n'; cat /proc/net/dev 2>/dev/null || true
printf '__IO_A__\n'; cat /proc/diskstats 2>/dev/null || true
sleep 0.25
printf '__CPU_B__\n'; head -n1 /proc/stat 2>/dev/null || true
printf '__NET_B__\n'; cat /proc/net/dev 2>/dev/null || true
printf '__IO_B__\n'; cat /proc/diskstats 2>/dev/null || true
printf '__COUNTS__\n'; systemctl --failed --type=service --no-legend --no-pager 2>/dev/null | wc -l; ss -H -lntu 2>/dev/null | wc -l
printf '__TOP_PROCESSES__\n'; ps -eo pid=,comm=,pcpu=,pmem=,args= --sort=-pcpu 2>/dev/null | head -n 6 || true
printf '__MOUNTS__\n'; if command -v findmnt >/dev/null 2>&1; then findmnt -rn -o TARGET,SOURCE,FSTYPE,OPTIONS 2>/dev/null; else cat /proc/mounts 2>/dev/null; fi
printf '__DOCKER__\n'; if command -v docker >/dev/null 2>&1; then printf 'installed\t'; docker version --format '{{.Server.Version}}' 2>/dev/null || docker --version 2>/dev/null || true; if command -v systemctl >/dev/null 2>&1 && systemctl is-active --quiet docker 2>/dev/null; then printf 'running\n'; else printf 'stopped\n'; fi; else printf 'missing\n'; fi
printf '__NGINX__\n'; if command -v nginx >/dev/null 2>&1; then printf 'installed\t'; nginx -v 2>&1 | sed 's#nginx version: nginx/##'; if command -v systemctl >/dev/null 2>&1 && systemctl is-active --quiet nginx 2>/dev/null; then printf 'running\n'; else printf 'stopped\n'; fi; else printf 'missing\n'; fi
printf '__CAPABILITIES__\n'; for name in systemctl sudo docker nginx ss ip journalctl lsof tar gzip ufw firewalld nft; do if command -v "$name" >/dev/null 2>&1; then printf '%s=1\n' "$name"; else printf '%s=0\n' "$name"; fi; done
printf '__COMMAND_PATHS__\n'; for name in systemctl sudo docker nginx ss ip journalctl lsof tar gzip ufw firewalld nft; do path=$(command -v "$name" 2>/dev/null || true); if [ -n "$path" ]; then printf '%s=%s\n' "$name" "$path"; fi; done
printf '__PERMISSIONS__\n'; uid=$(id -u 2>/dev/null || printf 'unknown'); if [ "$uid" = "0" ]; then printf 'identity=ok\troot\n'; else printf 'identity=warning\tnon-root uid=%s\n' "$uid"; fi; if command -v sudo >/dev/null 2>&1; then if sudo -n -v >/dev/null 2>&1; then printf 'sudo=ok\tpasswordless\n'; else printf 'sudo=warning\trequires-password-or-policy\n'; fi; else printf 'sudo=missing\tnot-installed\n'; fi; for command_name in systemctl docker nginx; do if command -v "$command_name" >/dev/null 2>&1; then printf 'command:%s=ok\tavailable\n' "$command_name"; else printf 'command:%s=missing\tnot-installed\n' "$command_name"; fi; done; for target in /etc/nginx /etc/openresty /var/lib/docker /opt/1panel; do if [ -d "$target" ]; then if [ -r "$target" ]; then printf 'path:%s=ok\treadable\n' "$target"; else printf 'path:%s=warning\tpermission-denied\n' "$target"; fi; else printf 'path:%s=missing\tnot-present\n' "$target"; fi; done
"#;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemOverview {
    pub hostname: String,
    pub os_name: String,
    pub os_version: String,
    pub kernel: String,
    pub architecture: String,
    pub current_user: String,
    pub current_time: String,
    pub timezone: String,
    pub primary_ip: String,
    pub default_gateway: String,
    pub package_manager: String,
    pub systemd_running: bool,
    pub uptime_seconds: u64,
    pub cpu_model: String,
    pub logical_cores: u32,
    pub cpu_usage_percent: Option<f64>,
    pub load: [f64; 3],
    pub memory_total_bytes: u64,
    pub memory_available_bytes: u64,
    pub swap_total_bytes: u64,
    pub swap_free_bytes: u64,
    pub network_rx_bytes_per_second: u64,
    pub network_tx_bytes_per_second: u64,
    pub network_rx_bytes_total: u64,
    pub network_tx_bytes_total: u64,
    pub io_read_bytes_per_second: u64,
    pub io_write_bytes_per_second: u64,
    pub io_count_per_second: u64,
    pub io_latency_ms: u64,
    pub failed_services: u32,
    pub listening_ports: u32,
    pub disks: Vec<DiskUsage>,
    pub top_processes: Vec<TopProcess>,
    pub mounts: Vec<MountInfo>,
    pub docker: RuntimeStatus,
    pub nginx: RuntimeStatus,
    pub capabilities: HashMap<String, bool>,
    pub server_capabilities: ServerCapabilities,
    pub permission_diagnostics: Vec<PermissionDiagnostic>,
    pub sampled_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskUsage {
    pub mount: String,
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub usage_percent: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TopProcess {
    pub pid: u32,
    pub name: String,
    pub cpu_percent: f64,
    pub memory_percent: f64,
    pub command: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MountInfo {
    pub mount: String,
    pub source: String,
    pub filesystem: String,
    pub options: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeStatus {
    pub installed: bool,
    pub running: bool,
    pub version: Option<String>,
}

pub async fn probe(manager: &SshConnectionManager, server_id: &str) -> AppResult<SystemOverview> {
    let result = manager
        .execute(server_id, PROBE_COMMAND, Duration::from_secs(15))
        .await?;
    if result.exit_code != 0 && result.stdout.is_empty() {
        return Err(
            AppError::new("REMOTE_COMMAND_FAILED", "metrics", "系统状态探测失败")
                .details(result.stderr)
                .for_server(server_id),
        );
    }
    parse_overview(&result.stdout).map_err(|error| error.for_server(server_id))
}

pub fn parse_overview(output: &str) -> AppResult<SystemOverview> {
    let sections = split_sections(output);
    let os_release = parse_key_values(section(&sections, "OS"));
    let meminfo = parse_meminfo(section(&sections, "MEMINFO"));
    let uname: Vec<_> = section(&sections, "UNAME").split_whitespace().collect();
    let load_values: Vec<f64> = section(&sections, "LOAD")
        .split_whitespace()
        .take(3)
        .filter_map(|value| value.parse().ok())
        .collect();
    let identity: Vec<_> = section(&sections, "IDENTITY").lines().collect();
    let network: Vec<_> = section(&sections, "NETWORK").lines().collect();
    let platform: Vec<_> = section(&sections, "PLATFORM").lines().collect();
    let counts: Vec<u32> = section(&sections, "COUNTS")
        .lines()
        .filter_map(|value| value.trim().parse().ok())
        .collect();
    let (
        network_rx_bytes_per_second,
        network_tx_bytes_per_second,
        network_rx_bytes_total,
        network_tx_bytes_total,
    ) = network_totals_pair(
        section(&sections, "NET_A"),
        section(&sections, "NET_B"),
        0.25,
    );
    let (io_read_bytes_per_second, io_write_bytes_per_second, io_count_per_second, io_latency_ms) =
        io_delta(section(&sections, "IO_A"), section(&sections, "IO_B"), 0.25);
    let capabilities = parse_capabilities(section(&sections, "CAPABILITIES"));
    let server_capabilities = ServerCapabilities::from_probe(
        platform.first().copied().unwrap_or("unknown"),
        &capabilities,
        parse_command_paths(section(&sections, "COMMAND_PATHS")),
        parse_permission_diagnostics(section(&sections, "PERMISSIONS")),
    );
    let permission_diagnostics = server_capabilities.permission_diagnostics.clone();
    Ok(SystemOverview {
        hostname: first_line(section(&sections, "HOSTNAME"))
            .unwrap_or("unknown")
            .to_string(),
        os_name: os_release
            .get("NAME")
            .cloned()
            .unwrap_or_else(|| "Linux".to_string()),
        os_version: os_release.get("VERSION_ID").cloned().unwrap_or_default(),
        kernel: uname.get(1).copied().unwrap_or("unknown").to_string(),
        architecture: first_line(section(&sections, "ARCH"))
            .or_else(|| uname.get(2).copied())
            .unwrap_or("unknown")
            .to_string(),
        current_user: identity.first().copied().unwrap_or("unknown").to_string(),
        current_time: identity.get(1).copied().unwrap_or_default().to_string(),
        timezone: identity.get(2).copied().unwrap_or_default().to_string(),
        primary_ip: network.first().copied().unwrap_or_default().to_string(),
        default_gateway: network.get(1).copied().unwrap_or_default().to_string(),
        package_manager: platform.first().copied().unwrap_or("unknown").to_string(),
        systemd_running: platform
            .get(1)
            .is_some_and(|value| value.trim() == "running"),
        uptime_seconds: first_line(section(&sections, "UPTIME"))
            .and_then(|value| value.parse().ok())
            .unwrap_or(0),
        cpu_model: first_line(section(&sections, "CPU_MODEL"))
            .unwrap_or("Unknown CPU")
            .to_string(),
        logical_cores: first_line(section(&sections, "CORES"))
            .and_then(|value| value.parse().ok())
            .unwrap_or(1),
        cpu_usage_percent: cpu_delta(section(&sections, "CPU_A"), section(&sections, "CPU_B")),
        load: [
            *load_values.first().unwrap_or(&0.0),
            *load_values.get(1).unwrap_or(&0.0),
            *load_values.get(2).unwrap_or(&0.0),
        ],
        memory_total_bytes: meminfo.get("MemTotal").copied().unwrap_or(0) * 1024,
        memory_available_bytes: meminfo
            .get("MemAvailable")
            .or_else(|| meminfo.get("MemFree"))
            .copied()
            .unwrap_or(0)
            * 1024,
        swap_total_bytes: meminfo.get("SwapTotal").copied().unwrap_or(0) * 1024,
        swap_free_bytes: meminfo.get("SwapFree").copied().unwrap_or(0) * 1024,
        network_rx_bytes_per_second,
        network_tx_bytes_per_second,
        network_rx_bytes_total,
        network_tx_bytes_total,
        io_read_bytes_per_second,
        io_write_bytes_per_second,
        io_count_per_second,
        io_latency_ms,
        failed_services: *counts.first().unwrap_or(&0),
        listening_ports: *counts.get(1).unwrap_or(&0),
        disks: parse_df(section(&sections, "DF")),
        top_processes: parse_top_processes(section(&sections, "TOP_PROCESSES")),
        mounts: parse_mounts(section(&sections, "MOUNTS")),
        docker: parse_runtime(section(&sections, "DOCKER")),
        nginx: parse_runtime(section(&sections, "NGINX")),
        capabilities,
        server_capabilities,
        permission_diagnostics,
        sampled_at: chrono::Utc::now().to_rfc3339(),
    })
}

fn network_totals(input: &str) -> (u64, u64) {
    input
        .lines()
        .filter_map(|line| {
            let (interface, values) = line.split_once(':')?;
            if interface.trim() == "lo" {
                return None;
            }
            let values: Vec<u64> = values
                .split_whitespace()
                .filter_map(|value| value.parse().ok())
                .collect();
            Some((*values.first()?, *values.get(8)?))
        })
        .fold((0, 0), |total, value| {
            (total.0 + value.0, total.1 + value.1)
        })
}

#[cfg(test)]
pub fn network_delta(first: &str, second: &str, seconds: f64) -> (u64, u64) {
    let (rx_second, tx_second, _, _) = network_totals_pair(first, second, seconds);
    (rx_second, tx_second)
}

fn network_totals_pair(first: &str, second: &str, seconds: f64) -> (u64, u64, u64, u64) {
    let first = network_totals(first);
    let second = network_totals(second);
    let seconds = seconds.max(0.001);
    (
        (second.0.saturating_sub(first.0) as f64 / seconds) as u64,
        (second.1.saturating_sub(first.1) as f64 / seconds) as u64,
        second.0,
        second.1,
    )
}

/// Parses two /proc/diskstats snapshots into per-second IO rates, operation count and latency.
/// diskstats fields: major minor name reads reads_merged sectors_read ms_read writes writes_merged sectors_written ms_write ...
fn io_delta(first: &str, second: &str, seconds: f64) -> (u64, u64, u64, u64) {
    fn io_totals(input: &str) -> (u64, u64, u64, u64) {
        input
            .lines()
            .filter_map(|line| {
                let fields: Vec<&str> = line.split_whitespace().collect();
                if fields.len() < 11 {
                    return None;
                }
                let name = fields[2];
                if ["loop", "ram", "zram", "sr", "fd"]
                    .iter()
                    .any(|prefix| name.starts_with(prefix))
                {
                    return None;
                }
                let read_sectors = fields[5].parse::<u64>().ok()?;
                let write_sectors = fields[9].parse::<u64>().ok()?;
                let read_ops = fields[3].parse::<u64>().ok()?;
                let write_ops = fields[7].parse::<u64>().ok()?;
                let read_ms = fields[6].parse::<u64>().ok()?;
                let write_ms = fields[10].parse::<u64>().ok()?;
                Some((
                    read_sectors,
                    write_sectors,
                    read_ops + write_ops,
                    read_ms.max(write_ms),
                ))
            })
            .fold((0, 0, 0, 0), |acc, value| {
                (
                    acc.0 + value.0,
                    acc.1 + value.1,
                    acc.2 + value.2,
                    acc.3 + value.3,
                )
            })
    }
    let first = io_totals(first);
    let second = io_totals(second);
    let seconds = seconds.max(0.001);
    let read_bytes = (second.0.saturating_sub(first.0) as f64 / seconds * 512.0) as u64;
    let write_bytes = (second.1.saturating_sub(first.1) as f64 / seconds * 512.0) as u64;
    let count = (second.2.saturating_sub(first.2) as f64 / seconds) as u64;
    let latency = (second.3.saturating_sub(first.3) as f64 / seconds) as u64;
    (read_bytes, write_bytes, count, latency)
}

fn split_sections(output: &str) -> HashMap<String, String> {
    let mut sections = HashMap::new();
    let mut current: Option<String> = None;
    for line in output.lines() {
        if line.starts_with("__") && line.ends_with("__") {
            current = Some(line.trim_matches('_').to_string());
            sections
                .entry(current.clone().unwrap_or_default())
                .or_insert_with(String::new);
        } else if let Some(name) = &current {
            let value = sections.entry(name.clone()).or_insert_with(String::new);
            value.push_str(line);
            value.push('\n');
        }
    }
    sections
}

fn section<'a>(sections: &'a HashMap<String, String>, name: &str) -> &'a str {
    sections.get(name).map(String::as_str).unwrap_or("")
}
fn first_line(input: &str) -> Option<&str> {
    input.lines().map(str::trim).find(|value| !value.is_empty())
}

pub fn parse_key_values(input: &str) -> HashMap<String, String> {
    input
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| {
            (
                key.trim().to_string(),
                value.trim().trim_matches('"').to_string(),
            )
        })
        .collect()
}

pub fn parse_meminfo(input: &str) -> HashMap<String, u64> {
    input
        .lines()
        .filter_map(|line| {
            let (key, value) = line.split_once(':')?;
            Some((
                key.to_string(),
                value.split_whitespace().next()?.parse().ok()?,
            ))
        })
        .collect()
}

pub fn cpu_delta(first: &str, second: &str) -> Option<f64> {
    fn values(line: &str) -> Option<Vec<u64>> {
        let values: Vec<u64> = line
            .split_whitespace()
            .skip(1)
            .filter_map(|value| value.parse().ok())
            .collect();
        (values.len() >= 4).then_some(values)
    }
    let first = values(first_line(first)?)?;
    let second = values(first_line(second)?)?;
    let first_idle = first.get(3).copied().unwrap_or(0) + first.get(4).copied().unwrap_or(0);
    let second_idle = second.get(3).copied().unwrap_or(0) + second.get(4).copied().unwrap_or(0);
    let first_total: u64 = first.iter().sum();
    let second_total: u64 = second.iter().sum();
    let total_delta = second_total.saturating_sub(first_total);
    if total_delta == 0 {
        return None;
    }
    let idle_delta = second_idle.saturating_sub(first_idle);
    Some(
        ((total_delta.saturating_sub(idle_delta)) as f64 / total_delta as f64 * 100.0)
            .clamp(0.0, 100.0),
    )
}

pub fn parse_df(input: &str) -> Vec<DiskUsage> {
    input
        .lines()
        .skip(1)
        .filter_map(|line| {
            let values: Vec<_> = line.split_whitespace().collect();
            if values.len() < 6 {
                return None;
            }
            Some(DiskUsage {
                total_bytes: values[1].parse().ok()?,
                used_bytes: values[2].parse().ok()?,
                usage_percent: values[4].trim_end_matches('%').parse().ok()?,
                mount: values[5..].join(" "),
            })
        })
        .collect()
}

/// 解析远程 ps 排序结果，最多保留探测脚本返回的前五个进程。
pub fn parse_top_processes(input: &str) -> Vec<TopProcess> {
    input
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            Some(TopProcess {
                pid: fields.next()?.parse().ok()?,
                name: fields.next()?.into(),
                cpu_percent: fields.next()?.parse().unwrap_or(0.0),
                memory_percent: fields.next()?.parse().unwrap_or(0.0),
                command: fields.collect::<Vec<_>>().join(" "),
            })
        })
        .collect()
}

/// 解析 findmnt 或 /proc/mounts 的四列摘要，跳过格式不完整的行。
pub fn parse_mounts(input: &str) -> Vec<MountInfo> {
    input
        .lines()
        .filter_map(|line| {
            let fields: Vec<_> = line.split_whitespace().collect();
            (fields.len() >= 4).then(|| MountInfo {
                mount: fields[0].into(),
                source: fields[1].into(),
                filesystem: fields[2].into(),
                options: fields[3].into(),
            })
        })
        .collect()
}

fn parse_runtime(input: &str) -> RuntimeStatus {
    let value = input.trim();
    if value.starts_with("missing") {
        return RuntimeStatus {
            installed: false,
            running: false,
            version: None,
        };
    }
    let running = value
        .lines()
        .last()
        .is_some_and(|line| line.trim() == "running");
    let version = value
        .strip_prefix("installed\t")
        .map(|value| value.lines().next().unwrap_or("").trim())
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    RuntimeStatus {
        installed: true,
        running,
        version,
    }
}

fn parse_capabilities(input: &str) -> HashMap<String, bool> {
    input
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(name, value)| (format!("has_{}", name), value == "1"))
        .collect()
}

/// 解析 command -v 返回的真实远端命令路径，不根据发行版猜测位置。
fn parse_command_paths(input: &str) -> HashMap<String, String> {
    input
        .lines()
        .filter_map(|line| line.split_once('='))
        .filter(|(_, path)| !path.trim().is_empty())
        .map(|(name, path)| (name.to_string(), path.trim().to_string()))
        .collect()
}

/// 解析固定权限探测 marker，限制字段长度并丢弃未知状态以保持 DTO 稳定。
pub fn parse_permission_diagnostics(input: &str) -> Vec<PermissionDiagnostic> {
    input
        .lines()
        .filter_map(|line| {
            let (marker, detail) = line.split_once('\t').unwrap_or((line, ""));
            let (scope, status) = marker.split_once('=')?;
            let status = match status.trim() {
                "ok" | "warning" | "missing" => status.trim(),
                _ => return None,
            };
            let scope = scope.trim();
            if scope.is_empty() || scope.len() > 160 {
                return None;
            }
            Some(PermissionDiagnostic {
                scope: scope.to_string(),
                status: status.to_string(),
                detail: detail.trim().chars().take(240).collect(),
            })
        })
        .take(32)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        cpu_delta, network_delta, parse_df, parse_key_values, parse_meminfo,
        parse_permission_diagnostics,
    };

    #[test]
    fn parses_os_release_quotes() {
        let values = parse_key_values(include_str!("../../../../fixtures/os-release-ubuntu.txt"));
        assert_eq!(values["NAME"], "Ubuntu");
        assert_eq!(values["VERSION_ID"], "24.04");
    }

    #[test]
    fn parses_memory_kibibytes() {
        let values = parse_meminfo(include_str!("../../../../fixtures/proc-meminfo.txt"));
        assert_eq!(values["MemTotal"], 16_384_256);
    }

    #[test]
    fn computes_cpu_usage_from_two_samples() {
        let value = cpu_delta(
            include_str!("../../../../fixtures/proc-stat-a.txt"),
            include_str!("../../../../fixtures/proc-stat-b.txt"),
        )
        .unwrap();
        assert!((value - 46.666).abs() < 0.01);
    }

    /// 验证权限 marker 能保留 root/sudo/路径状态并忽略未知状态。
    #[test]
    fn parses_permission_diagnostics() {
        let values = parse_permission_diagnostics(
            "identity=ok\troot\nsudo=warning\trequires-password\npath:/etc/nginx=missing\tnot-present\nunknown=broken\tdrop\n",
        );
        assert_eq!(values.len(), 3);
        assert_eq!(values[0].scope, "identity");
        assert_eq!(values[1].status, "warning");
        assert_eq!(values[2].detail, "not-present");
    }

    #[test]
    fn parses_posix_df() {
        let disks = parse_df(include_str!("../../../../fixtures/df-posix.txt"));
        assert_eq!(disks[0].mount, "/");
        assert_eq!(disks[0].usage_percent, 85.0);
    }

    #[test]
    fn computes_network_rate_from_two_samples() {
        let first = "eth0: 1000 0 0 0 0 0 0 0 2000 0 0 0 0 0 0 0\nlo: 9000 0 0 0 0 0 0 0 9000 0 0 0 0 0 0 0";
        let second = "eth0: 1500 0 0 0 0 0 0 0 3000 0 0 0 0 0 0 0\nlo: 12000 0 0 0 0 0 0 0 12000 0 0 0 0 0 0 0";
        assert_eq!(network_delta(first, second, 0.5), (1000, 2000));
    }
}

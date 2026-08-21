use serde::Serialize;
use std::collections::HashMap;

/// 描述远端 Linux 平台家族适配器；安装和服务命令由适配器集中生成。
pub trait PlatformAdapter: Send + Sync {
    fn id(&self) -> &'static str;
    fn package_manager(&self) -> &'static str;
    fn service_manager(&self) -> &'static str;
    fn install_command(&self, package: &str) -> String;
}

/// Debian/Ubuntu 家族的远端命令适配器。
pub struct DebianFamilyAdapter;

impl PlatformAdapter for DebianFamilyAdapter {
    fn id(&self) -> &'static str {
        "debian-family"
    }
    fn package_manager(&self) -> &'static str {
        "apt"
    }
    fn service_manager(&self) -> &'static str {
        "systemd"
    }
    fn install_command(&self, package: &str) -> String {
        format!("apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y -- {package}")
    }
}

/// RHEL/Rocky/Alma 家族的远端命令适配器。
pub struct RhelFamilyAdapter;

impl PlatformAdapter for RhelFamilyAdapter {
    fn id(&self) -> &'static str {
        "rhel-family"
    }
    fn package_manager(&self) -> &'static str {
        "dnf"
    }
    fn service_manager(&self) -> &'static str {
        "systemd"
    }
    fn install_command(&self, package: &str) -> String {
        format!("dnf install -y -- {package}")
    }
}

/// Alpine Linux 的 apk/OpenRC 适配器；命令固定为非交互式安装。
pub struct AlpineAdapter;

impl PlatformAdapter for AlpineAdapter {
    fn id(&self) -> &'static str {
        "alpine"
    }
    fn package_manager(&self) -> &'static str {
        "apk"
    }
    fn service_manager(&self) -> &'static str {
        "openrc"
    }
    fn install_command(&self, package: &str) -> String {
        format!("apk add --no-cache -- {package}")
    }
}

/// Arch Linux 家族的 pacman/systemd 适配器。
pub struct ArchFamilyAdapter;

impl PlatformAdapter for ArchFamilyAdapter {
    fn id(&self) -> &'static str {
        "arch-family"
    }
    fn package_manager(&self) -> &'static str {
        "pacman"
    }
    fn service_manager(&self) -> &'static str {
        "systemd"
    }
    fn install_command(&self, package: &str) -> String {
        format!("pacman -S --needed --noconfirm -- {package}")
    }
}

/// 未识别平台的只读适配器；禁止生成安装命令。
pub struct UnknownPlatformAdapter;

impl PlatformAdapter for UnknownPlatformAdapter {
    fn id(&self) -> &'static str {
        "unknown"
    }
    fn package_manager(&self) -> &'static str {
        "unknown"
    }
    fn service_manager(&self) -> &'static str {
        "unknown"
    }
    fn install_command(&self, _package: &str) -> String {
        String::new()
    }
}

/// 根据远端探测出的包管理器选择平台适配器。
pub fn adapter_for(package_manager: &str) -> Box<dyn PlatformAdapter> {
    match package_manager {
        "apt" => Box::new(DebianFamilyAdapter),
        "dnf" | "yum" => Box::new(RhelFamilyAdapter),
        "apk" => Box::new(AlpineAdapter),
        "pacman" => Box::new(ArchFamilyAdapter),
        _ => Box::new(UnknownPlatformAdapter),
    }
}

/// 描述一个远端管理范围的执行权限状态，不包含凭据或命令输出正文。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PermissionDiagnostic {
    pub scope: String,
    pub status: String,
    pub detail: String,
}

/// 汇总远端可用的包管理器、服务管理器、防火墙和命令路径能力。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerCapabilities {
    pub adapter: String,
    pub package_manager: String,
    pub service_manager: String,
    pub firewall: Option<String>,
    pub command_paths: HashMap<String, String>,
    pub docker_command: Option<String>,
    pub nginx_command: Option<String>,
    pub permission_diagnostics: Vec<PermissionDiagnostic>,
}

impl ServerCapabilities {
    /// 从一次远端探测结果生成稳定能力快照；缺少命令时保留 None 而不猜测路径。
    pub fn from_probe(
        package_manager: &str,
        available: &HashMap<String, bool>,
        command_paths: HashMap<String, String>,
        permission_diagnostics: Vec<PermissionDiagnostic>,
    ) -> Self {
        let adapter = adapter_for(package_manager);
        let has_command = |name: &str| {
            available
                .get(&format!("has_{name}"))
                .copied()
                .unwrap_or(false)
        };
        let firewall = ["ufw", "firewalld", "nft"]
            .into_iter()
            .find(|name| has_command(name))
            .map(str::to_string);
        Self {
            adapter: adapter.id().to_string(),
            package_manager: adapter.package_manager().to_string(),
            service_manager: if has_command("systemctl") {
                adapter.service_manager().to_string()
            } else {
                "unknown".into()
            },
            firewall,
            docker_command: command_paths.get("docker").cloned(),
            nginx_command: command_paths.get("nginx").cloned(),
            command_paths,
            permission_diagnostics,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{adapter_for, PermissionDiagnostic, ServerCapabilities};
    use std::collections::HashMap;

    /// 验证 Debian/RHEL 适配器生成的安装命令和平台标识保持集中一致。
    #[test]
    fn selects_package_adapter() {
        assert_eq!(adapter_for("apt").id(), "debian-family");
        assert!(adapter_for("dnf")
            .install_command("curl")
            .contains("dnf install"));
        assert_eq!(adapter_for("apk").service_manager(), "openrc");
        assert!(adapter_for("pacman")
            .install_command("curl")
            .contains("pacman -S --needed"));
        assert!(adapter_for("unknown").install_command("curl").is_empty());
    }

    /// 验证能力快照会保留实际命令路径并识别可用防火墙。
    #[test]
    fn builds_capability_snapshot() {
        let available = HashMap::from([("has_systemctl".into(), true), ("has_ufw".into(), true)]);
        let paths = HashMap::from([("docker".into(), "/usr/bin/docker".into())]);
        let value = ServerCapabilities::from_probe(
            "apt",
            &available,
            paths,
            vec![PermissionDiagnostic {
                scope: "sudo".into(),
                status: "ok".into(),
                detail: "passwordless".into(),
            }],
        );
        assert_eq!(value.adapter, "debian-family");
        assert_eq!(value.firewall.as_deref(), Some("ufw"));
        assert_eq!(value.docker_command.as_deref(), Some("/usr/bin/docker"));
        assert_eq!(value.permission_diagnostics[0].scope, "sudo");
    }
}

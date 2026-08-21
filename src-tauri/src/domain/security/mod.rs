use crate::domain::ssh::SshConnectionManager;
use crate::errors::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// 远程防火墙的一条可展示规则；`raw` 保留原始文本，便于用户核对 CLI 输出。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FirewallRule {
    pub id: String,
    pub source: String,
    pub destination: String,
    pub protocol: String,
    pub port: String,
    pub action: String,
    pub raw: String,
}

/// 防火墙探测结果，支持 UFW、firewalld 和 nftables 只读回退。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FirewallSnapshot {
    pub backend: String,
    pub installed: bool,
    pub enabled: bool,
    pub default_incoming: Option<String>,
    pub default_outgoing: Option<String>,
    pub rules: Vec<FirewallRule>,
    pub warnings: Vec<String>,
}

/// SSH 服务的有效安全配置，不返回密码、私钥或授权密钥内容。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SshSecurityConfig {
    pub config_path: String,
    pub port: u16,
    pub password_authentication: Option<bool>,
    pub pubkey_authentication: Option<bool>,
    pub permit_root_login: Option<String>,
    pub effective_lines: Vec<String>,
}

/// 系统安全页面需要的完整远程快照。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecuritySnapshot {
    pub firewall: FirewallSnapshot,
    pub ssh: SshSecurityConfig,
    pub warnings: Vec<String>,
}

/// 添加或删除防火墙规则的输入；破坏性操作必须由 UI 显式确认。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FirewallRuleInput {
    pub server_id: String,
    pub action: String,
    pub protocol: String,
    pub port: String,
    pub source: Option<String>,
    pub comment: Option<String>,
    pub confirmed: bool,
}

/// 修改 SSH 有效配置的输入；每个字段均为可选，只更新用户明确填写的值。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshSecurityInput {
    pub server_id: String,
    pub port: Option<u16>,
    pub password_authentication: Option<bool>,
    pub pubkey_authentication: Option<bool>,
    pub permit_root_login: Option<String>,
    pub confirmed: bool,
}

/// 读取远端防火墙和 sshd 有效配置，并统一解析为安全页面 DTO。
pub async fn snapshot(ssh: &SshConnectionManager, server_id: &str) -> AppResult<SecuritySnapshot> {
    let result = ssh
        .execute_system(server_id, security_dump_command(), Duration::from_secs(30))
        .await?;
    if !result.stdout.contains("__SECURITY_FIREWALL__") {
        return Err(
            AppError::new("SECURITY_PROBE_FAILED", "security", "无法读取远程安全配置")
                .details(result.stderr)
                .for_server(server_id),
        );
    }
    Ok(parse_security_dump(&result.stdout))
}

/// 添加或删除一条受支持的 UFW/firewalld 规则，并重新读取快照验证结果。
pub async fn firewall_rule_action(
    ssh: &SshConnectionManager,
    input: FirewallRuleInput,
) -> AppResult<FirewallSnapshot> {
    validate_rule_input(&input)?;
    if !input.confirmed {
        return Err(AppError::new(
            "CONFIRMATION_REQUIRED",
            "security",
            "防火墙变更需要显式确认",
        )
        .for_server(&input.server_id));
    }
    let current = snapshot(ssh, &input.server_id).await?;
    // UFW displays `Anywhere`/`Anywhere (v6)` while its command syntax uses
    // the canonical `any` source; normalize both forms before building delete
    // commands so a rule read from the table can be removed again.
    let source_value = input.source.as_deref().unwrap_or("any");
    let source = if source_value.eq_ignore_ascii_case("any")
        || source_value.eq_ignore_ascii_case("anywhere")
        || source_value.eq_ignore_ascii_case("anywhere (v6)")
    {
        "any"
    } else {
        source_value
    };
    let protocol = if input.protocol == "any" {
        None
    } else {
        Some(input.protocol.as_str())
    };
    let command = match current.firewall.backend.as_str() {
        "ufw" => ufw_rule_command(&input, source, protocol),
        "firewalld" => firewalld_rule_command(&input, source, protocol)?,
        "nftables" => {
            return Err(AppError::new(
                "FIREWALL_READ_ONLY",
                "capability",
                "已检测到 nftables，但当前版本不会直接改写 nftables 规则集",
            )
            .for_server(&input.server_id)
            .suggestion("请在发行版防火墙工具中管理 nftables，或先安装并启用 UFW/firewalld"));
        }
        _ => {
            return Err(AppError::new(
                "FIREWALL_UNAVAILABLE",
                "capability",
                "远端没有可管理的 UFW 或 firewalld",
            )
            .for_server(&input.server_id));
        }
    };
    let result = ssh
        .execute_system(&input.server_id, &command, Duration::from_secs(30))
        .await?;
    if result.exit_code != 0 {
        return Err(
            AppError::new("FIREWALL_ACTION_FAILED", "security", "防火墙规则变更失败")
                .details(result.stderr)
                .for_server(&input.server_id),
        );
    }
    let refreshed = snapshot(ssh, &input.server_id).await?;
    Ok(refreshed.firewall)
}

/// 更新 sshd_config 中用户指定的安全字段，失败时恢复备份并保持连接服务可用。
pub async fn save_ssh_config(
    ssh: &SshConnectionManager,
    input: SshSecurityInput,
) -> AppResult<SshSecurityConfig> {
    validate_ssh_input(&input)?;
    if !input.confirmed {
        return Err(AppError::new(
            "CONFIRMATION_REQUIRED",
            "security",
            "SSH 配置变更需要显式确认",
        )
        .for_server(&input.server_id));
    }
    let mut updates = Vec::new();
    if let Some(port) = input.port {
        updates.push(format!("update ssh_port {port}"));
    }
    if let Some(value) = input.password_authentication {
        updates.push(format!(
            "update password_authentication {}",
            if value { "yes" } else { "no" }
        ));
    }
    if let Some(value) = input.pubkey_authentication {
        updates.push(format!(
            "update pubkey_authentication {}",
            if value { "yes" } else { "no" }
        ));
    }
    if let Some(value) = input.permit_root_login.as_deref() {
        updates.push(format!("update permit_root_login {value}"));
    }
    if updates.is_empty() {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "validation",
            "至少需要修改一个 SSH 配置字段",
        )
        .for_server(&input.server_id));
    }
    let backup = format!(
        "/etc/ssh/sshd_config.1panel-client-backup-{}",
        uuid::Uuid::new_v4()
    );
    let mut command = format!(
        "set -eu; file=/etc/ssh/sshd_config; backup={}; cp -a -- \"$file\" \"$backup\"; ",
        crate::security::shell_escape(&backup)
    );
    for update in updates {
        let mut parts = update.split_whitespace();
        let _ = parts.next();
        let key = parts.next().unwrap_or_default();
        let value = parts.next().unwrap_or_default();
        let directive = match key {
            "ssh_port" => "Port",
            "password_authentication" => "PasswordAuthentication",
            "pubkey_authentication" => "PubkeyAuthentication",
            "permit_root_login" => "PermitRootLogin",
            _ => continue,
        };
        command.push_str(&format!(
            "if grep -Eq '^[#[:space:]]*{directive}[[:space:]]+' \"$file\"; then sed -i -E 's|^[#[:space:]]*{directive}[[:space:]].*$|{directive} {value}|' \"$file\"; else printf '\\n{directive} {value}\\n' >> \"$file\"; fi; ",
        ));
    }
    command.push_str(
        "if ! sshd -t -f /etc/ssh/sshd_config 2>/tmp/1panel-client-sshd-test.err; then cp -a -- \"$backup\" \"$file\"; cat /tmp/1panel-client-sshd-test.err; rm -f -- /tmp/1panel-client-sshd-test.err; exit 41; fi; if ! (systemctl reload ssh 2>/dev/null || systemctl reload sshd 2>/dev/null || service ssh reload 2>/dev/null || service sshd reload 2>/dev/null); then cp -a -- \"$backup\" \"$file\"; systemctl reload ssh 2>/dev/null || true; exit 42; fi; rm -f -- /tmp/1panel-client-sshd-test.err; rm -f -- \"$backup\"",
    );
    let result = ssh
        .execute_system(&input.server_id, &command, Duration::from_secs(45))
        .await?;
    if result.exit_code != 0 {
        return Err(AppError::new(
            "SSH_CONFIG_INVALID",
            "security",
            "SSH 配置检查或 reload 失败，已恢复备份",
        )
        .details(result.stderr)
        .for_server(&input.server_id));
    }
    Ok(snapshot(ssh, &input.server_id).await?.ssh)
}

/// 返回固定的安全探测脚本，脚本不包含任何用户输入。
fn security_dump_command() -> &'static str {
    "set +e; printf '__SECURITY_FIREWALL__\\n'; if command -v ufw >/dev/null 2>&1; then printf '__FIREWALL_BACKEND__ufw\\n'; if ufw status 2>/dev/null | grep -qi '^Status: active'; then printf '__FIREWALL_ENABLED__yes\\n'; else printf '__FIREWALL_ENABLED__no\\n'; fi; ufw status verbose 2>&1; ufw status numbered 2>&1; elif command -v firewall-cmd >/dev/null 2>&1; then printf '__FIREWALL_BACKEND__firewalld\\n'; if firewall-cmd --state 2>/dev/null | grep -qi running; then printf '__FIREWALL_ENABLED__yes\\n'; else printf '__FIREWALL_ENABLED__no\\n'; fi; firewall-cmd --list-all 2>&1; elif command -v nft >/dev/null 2>&1; then printf '__FIREWALL_BACKEND__nftables\\n'; printf '__FIREWALL_ENABLED__yes\\n'; nft list ruleset 2>&1; else printf '__FIREWALL_BACKEND__none\\n'; printf '__FIREWALL_ENABLED__no\\n'; fi; printf '__SSH_EFFECTIVE__\\n'; (sshd -T 2>/dev/null || /usr/sbin/sshd -T 2>/dev/null); printf '__SSH_CONFIG__\\n'; sed -n '1,240p' /etc/ssh/sshd_config 2>/dev/null"
}

/// 将防火墙和 SSH 探测 marker 解析为稳定 DTO；无法识别的原始行只进入 warnings。
pub fn parse_security_dump(input: &str) -> SecuritySnapshot {
    let mut firewall = FirewallSnapshot {
        backend: "none".into(),
        installed: false,
        enabled: false,
        default_incoming: None,
        default_outgoing: None,
        rules: Vec::new(),
        warnings: Vec::new(),
    };
    let mut ssh = SshSecurityConfig {
        config_path: "/etc/ssh/sshd_config".into(),
        port: 22,
        password_authentication: None,
        pubkey_authentication: None,
        permit_root_login: None,
        effective_lines: Vec::new(),
    };
    let mut section = "firewall";
    for raw in input.lines() {
        let line = raw.trim();
        if line == "__SSH_EFFECTIVE__" {
            section = "ssh-effective";
            continue;
        }
        if line == "__SSH_CONFIG__" {
            section = "ssh-config";
            continue;
        }
        if let Some(value) = line.strip_prefix("__FIREWALL_BACKEND__") {
            firewall.backend = value.to_string();
            firewall.installed = value != "none";
            continue;
        }
        if let Some(value) = line.strip_prefix("__FIREWALL_ENABLED__") {
            firewall.enabled = value == "yes";
            continue;
        }
        if section == "ssh-effective" {
            if line.is_empty() || line.starts_with("__") {
                continue;
            }
            ssh.effective_lines.push(line.to_string());
            let fields: Vec<_> = line.split_whitespace().collect();
            match fields.as_slice() {
                ["port", value, ..] => {
                    if let Ok(port) = value.parse() {
                        ssh.port = port;
                    }
                }
                ["passwordauthentication", value, ..] => {
                    ssh.password_authentication = parse_yes_no(value)
                }
                ["pubkeyauthentication", value, ..] => {
                    ssh.pubkey_authentication = parse_yes_no(value)
                }
                ["permitrootlogin", value, ..] => ssh.permit_root_login = Some((*value).into()),
                _ => {}
            }
            continue;
        }
        if section == "ssh-config" || line.starts_with("Status:") {
            continue;
        }
        if firewall.backend == "ufw" {
            if let Some(value) = line.strip_prefix("Default:") {
                firewall.default_incoming = parse_ufw_default(value, "incoming");
                firewall.default_outgoing = parse_ufw_default(value, "outgoing");
            }
            if let Some(rule) = parse_ufw_rule(line) {
                firewall.rules.push(rule);
            }
        } else if firewall.backend == "firewalld" {
            parse_firewalld_rule(line, &mut firewall.rules);
        }
    }
    if firewall.backend == "nftables" {
        firewall
            .warnings
            .push("nftables 当前只提供只读规则摘要，客户端不会直接改写规则集".into());
    }
    let mut warnings = firewall.warnings.clone();
    if !firewall.installed {
        warnings.push("未检测到 UFW、firewalld 或 nftables".into());
    }
    SecuritySnapshot {
        firewall,
        ssh,
        warnings,
    }
}

/// 解析 UFW `Default: deny (incoming), allow (outgoing)` 的策略词。
fn parse_ufw_default(value: &str, direction: &str) -> Option<String> {
    value.split(',').find_map(|part| {
        let (policy, scope) = part.trim().split_once('(')?;
        if scope.trim_end_matches(')').trim() != direction {
            return None;
        }
        let policy = policy
            .trim()
            .trim_matches(|character: char| !character.is_ascii_alphabetic());
        (!policy.is_empty()).then(|| policy.to_string())
    })
}

/// 解析 UFW 的 numbered status 行，兼容 IPv4/IPv6 和 `Anywhere` 来源。
fn parse_ufw_rule(line: &str) -> Option<FirewallRule> {
    let trimmed = line.trim();
    if !trimmed.starts_with('[') {
        return None;
    }
    let close = trimmed.find(']')?;
    let id = trimmed[1..close].trim().to_string();
    let fields: Vec<_> = trimmed[close + 1..].split_whitespace().collect();
    let target = fields.first()?.to_string();
    let action_index = fields.iter().position(|field| {
        *field == "ALLOW" || *field == "DENY" || *field == "REJECT" || *field == "LIMIT"
    })?;
    let action = fields[action_index].to_string();
    let source = fields
        .get(action_index + 2..)
        .map(|values| values.join(" "))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "any".into());
    let (port, protocol) = target
        .split_once('/')
        .map(|(port, protocol)| (port.to_string(), protocol.to_string()))
        .unwrap_or_else(|| (target.clone(), "any".into()));
    Some(FirewallRule {
        id,
        source,
        destination: "any".into(),
        protocol,
        port,
        action,
        raw: trimmed.into(),
    })
}

/// 将 firewalld 的 `ports:` 项解析成同一套规则 DTO。
fn parse_firewalld_rule(line: &str, rules: &mut Vec<FirewallRule>) {
    let Some(value) = line.strip_prefix("ports:") else {
        return;
    };
    for (index, item) in value.split_whitespace().enumerate() {
        let Some((port, protocol)) = item.split_once('/') else {
            continue;
        };
        rules.push(FirewallRule {
            id: (index + 1).to_string(),
            source: "any".into(),
            destination: "public".into(),
            protocol: protocol.into(),
            port: port.into(),
            action: "ALLOW".into(),
            raw: item.into(),
        });
    }
}

/// 构造已校验的 UFW 命令；协议为 any 时不添加 protocol 参数。
fn ufw_rule_command(input: &FirewallRuleInput, source: &str, protocol: Option<&str>) -> String {
    let verb = if input.action == "add" {
        "allow"
    } else {
        "delete allow"
    };
    let port = crate::security::shell_escape(&input.port);
    let comment = input
        .comment
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(|value| format!(" comment {}", crate::security::shell_escape(value)))
        .unwrap_or_default();
    if source == "any" {
        match protocol {
            Some(protocol) => format!("ufw {verb} {port}/{protocol}{comment}"),
            None => format!("ufw {verb} {port}{comment}"),
        }
    } else {
        let source = crate::security::shell_escape(source);
        match protocol {
            Some(protocol) => {
                format!("ufw {verb} from {source} to any port {port} proto {protocol}{comment}")
            }
            None => format!("ufw {verb} from {source} to any port {port}{comment}"),
        }
    }
}

/// 构造 firewalld 永久端口变更命令；来源限制暂由后端拒绝，避免生成错误 rich rule。
fn firewalld_rule_command(
    input: &FirewallRuleInput,
    source: &str,
    protocol: Option<&str>,
) -> AppResult<String> {
    if source != "any" {
        return Err(AppError::new(
            "FIREWALL_SOURCE_UNSUPPORTED",
            "capability",
            "firewalld 当前只支持任意来源端口规则",
        ));
    }
    let protocol = protocol.ok_or_else(|| {
        AppError::new(
            "VALIDATION_FAILED",
            "validation",
            "firewalld 规则必须指定 tcp 或 udp 协议",
        )
    })?;
    let operation = if input.action == "add" {
        "--add-port"
    } else {
        "--remove-port"
    };
    Ok(format!(
        "firewall-cmd --permanent {operation}={}/{} && firewall-cmd --reload",
        crate::security::shell_escape(&input.port),
        protocol
    ))
}

/// 校验防火墙输入，阻止 shell 元字符、非法端口范围和未知动作。
fn validate_rule_input(input: &FirewallRuleInput) -> AppResult<()> {
    if !matches!(input.action.as_str(), "add" | "delete")
        || !matches!(input.protocol.as_str(), "tcp" | "udp" | "any")
        || !valid_port_expression(&input.port)
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "validation",
            "防火墙动作、协议或端口无效",
        )
        .for_server(&input.server_id));
    }
    for value in [
        input.source.as_deref().unwrap_or("any"),
        input.comment.as_deref().unwrap_or_default(),
    ] {
        if value.chars().any(|character| {
            character == '\0'
                || character == '\n'
                || character == '\r'
                || character == ';'
                || character == '`'
        }) {
            return Err(AppError::new(
                "VALIDATION_FAILED",
                "validation",
                "防火墙来源和备注不能包含控制字符或 shell 分隔符",
            ));
        }
    }
    Ok(())
}

/// 校验防火墙允许的单端口、端口范围和逗号列表表达式。
fn valid_port_expression(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.split(',').all(|part| {
            let mut values = part.split('-');
            let first = values.next().unwrap_or_default();
            let second = values.next();
            values.next().is_none()
                && first.parse::<u16>().is_ok_and(|port| port > 0)
                && second
                    .map(|value| {
                        value.parse::<u16>().is_ok_and(|port| {
                            port > 0 && first.parse::<u16>().is_ok_and(|start| start <= port)
                        })
                    })
                    .unwrap_or(true)
        })
}

/// 校验 SSH 配置字段，并明确限制 root 登录策略到 sshd 支持的固定值。
fn validate_ssh_input(input: &SshSecurityInput) -> AppResult<()> {
    if input.port.is_some_and(|port| port == 0)
        || input.permit_root_login.as_deref().is_some_and(|value| {
            !matches!(
                value,
                "yes" | "no" | "prohibit-password" | "forced-commands-only"
            )
        })
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "validation",
            "SSH 端口或 root 登录策略无效",
        )
        .for_server(&input.server_id));
    }
    Ok(())
}

/// 将 sshd 的 yes/no 有效值转换为可选布尔值。
fn parse_yes_no(value: &str) -> Option<bool> {
    match value.to_ascii_lowercase().as_str() {
        "yes" => Some(true),
        "no" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_security_dump, valid_port_expression, FirewallRuleInput, SshSecurityInput};

    #[test]
    fn parses_ufw_and_sshd_sections() {
        let snapshot = parse_security_dump(
            "__SECURITY_FIREWALL__\n__FIREWALL_BACKEND__ufw\n__FIREWALL_ENABLED__yes\nDefault: deny (incoming), allow (outgoing), disabled (routed)\n[ 1] 22/tcp ALLOW IN Anywhere\n__SSH_EFFECTIVE__\nport 2222\npasswordauthentication no\npubkeyauthentication yes\npermitrootlogin prohibit-password\n__SSH_CONFIG__\n",
        );
        assert_eq!(snapshot.firewall.backend, "ufw");
        assert!(snapshot.firewall.enabled);
        assert_eq!(snapshot.firewall.default_incoming.as_deref(), Some("deny"));
        assert_eq!(snapshot.firewall.default_outgoing.as_deref(), Some("allow"));
        assert_eq!(snapshot.firewall.rules[0].port, "22");
        assert_eq!(snapshot.ssh.port, 2222);
        assert_eq!(snapshot.ssh.password_authentication, Some(false));
        assert_eq!(
            snapshot.ssh.permit_root_login.as_deref(),
            Some("prohibit-password")
        );
    }

    #[test]
    fn parses_firewalld_ports() {
        let snapshot = parse_security_dump(
            "__SECURITY_FIREWALL__\n__FIREWALL_BACKEND__firewalld\n__FIREWALL_ENABLED__yes\npublic (active)\nports: 80/tcp 443/tcp\n__SSH_EFFECTIVE__\nport 22\n",
        );
        assert_eq!(snapshot.firewall.rules.len(), 2);
        assert_eq!(snapshot.firewall.rules[1].port, "443");
    }

    #[test]
    fn rejects_shell_like_rule_values() {
        let input = FirewallRuleInput {
            server_id: "server".into(),
            action: "add".into(),
            protocol: "tcp".into(),
            port: "22".into(),
            source: Some("any; touch /tmp/pwned".into()),
            comment: None,
            confirmed: true,
        };
        assert!(super::validate_rule_input(&input).is_err());
        assert!(valid_port_expression("80,443,8000-8100"));
        assert!(!valid_port_expression("0"));
        assert!(!valid_port_expression("8100-8000"));
    }

    #[test]
    fn validates_ssh_policy_values() {
        let input = SshSecurityInput {
            server_id: "server".into(),
            port: Some(22),
            password_authentication: Some(false),
            pubkey_authentication: Some(true),
            permit_root_login: Some("no".into()),
            confirmed: true,
        };
        assert!(super::validate_ssh_input(&input).is_ok());
    }
}

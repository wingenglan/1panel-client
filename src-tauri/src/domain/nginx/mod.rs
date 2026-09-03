use crate::domain::ssh::SshConnectionManager;
use crate::errors::{AppError, AppResult};
use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::io::AsyncWriteExt;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NginxSnapshot {
    pub installed: bool,
    pub running: bool,
    pub flavor: String,
    pub binary: Option<String>,
    pub container_id: Option<String>,
    pub container_site_root: Option<String>,
    pub site_host_root: Option<String>,
    pub version: Option<String>,
    pub image: Option<String>,
    pub config_path: Option<String>,
    pub config_test: Option<bool>,
    pub managed_conf_supported: bool,
    pub managed_conf_dir: Option<String>,
    pub proxies: Vec<ReverseProxy>,
    pub certificates: Vec<CertificateMetadata>,
    pub config_sources: Vec<String>,
    pub servers: usize,
    pub upstreams: usize,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CertificateMetadata {
    pub certificate_path: String,
    pub private_key_path: Option<String>,
    pub source_file: String,
    pub source_line: usize,
    pub expires_at: Option<String>,
    pub days_remaining: Option<i64>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReverseProxy {
    pub server_names: Vec<String>,
    pub listen: Vec<String>,
    pub location: String,
    pub upstream: String,
    pub target_host: String,
    pub target_port: Option<u16>,
    pub ssl: bool,
    pub source_file: String,
    pub source_line: usize,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NginxProxyInput {
    pub server_id: String,
    pub name: String,
    pub server_name: String,
    pub listen_port: u16,
    pub enable_https: bool,
    pub https_port: u16,
    pub certificate_path: Option<String>,
    pub certificate_key_path: Option<String>,
    pub location: String,
    pub upstream_scheme: String,
    pub target_host: String,
    pub target_port: u16,
    pub websocket: bool,
    pub preserve_host: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NginxBackendProbeInput {
    pub server_id: String,
    pub scheme: String,
    pub target_host: String,
    pub target_port: u16,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NginxBackendProbeResult {
    pub reachable: bool,
    pub status_code: Option<u16>,
    pub latency_ms: Option<u64>,
    pub detail: String,
}

/// 读取 Nginx 版本、运行状态和 `nginx -T` 的真实配置摘要。
pub async fn snapshot(ssh: &SshConnectionManager, server_id: &str) -> AppResult<NginxSnapshot> {
    let result = ssh
        .execute(server_id, nginx_dump_command(), Duration::from_secs(30))
        .await?;
    if result.exit_code != 0 && !result.stdout.contains("configuration file") {
        if result.stderr.contains("not found") || result.stderr.contains("command not found") {
            return Ok(NginxSnapshot {
                installed: false,
                running: false,
                flavor: "nginx".into(),
                binary: None,
                container_id: None,
                container_site_root: None,
                site_host_root: None,
                version: None,
                image: None,
                config_path: None,
                config_test: None,
                managed_conf_supported: false,
                managed_conf_dir: None,
                proxies: Vec::new(),
                certificates: Vec::new(),
                config_sources: Vec::new(),
                servers: 0,
                upstreams: 0,
                warnings: Vec::new(),
            });
        }
        return Err(
            AppError::new("NGINX_CONFIG_INVALID", "nginx", "无法读取 Nginx 配置")
                .details(format!("{}\n{}", result.stdout, result.stderr))
                .for_server(server_id),
        );
    }
    let mut parsed = parse_nginx_dump(&result.stdout);
    enrich_certificate_expiry(ssh, server_id, &mut parsed).await?;
    parsed.config_test = Some(result.exit_code == 0 || result.stderr.contains("syntax is ok"));
    Ok(parsed)
}

/// 在不修改远端文件的情况下运行 Nginx 配置检查。
pub async fn test_config(ssh: &SshConnectionManager, server_id: &str) -> AppResult<bool> {
    let snapshot = snapshot(ssh, server_id).await?;
    let command = if let Some(container_id) = snapshot.container_id {
        format!(
            "docker exec {} openresty -t",
            crate::security::shell_escape(&container_id)
        )
    } else {
        format!(
            "{} -t",
            crate::security::shell_escape(snapshot.binary.as_deref().unwrap_or("nginx"))
        )
    };
    let result = ssh
        .execute(server_id, &command, Duration::from_secs(30))
        .await?;
    Ok(result.exit_code == 0)
}

/// 从远端服务器主动探测一个代理上游，只返回 HTTP 状态和耗时摘要。
pub async fn probe_backend(
    ssh: &SshConnectionManager,
    input: NginxBackendProbeInput,
) -> AppResult<NginxBackendProbeResult> {
    if !matches!(input.scheme.as_str(), "http" | "https")
        || input.target_host.trim().is_empty()
        || input.target_port == 0
        || input
            .target_host
            .chars()
            .any(|value| value == '\n' || value == '\r' || value == '\0')
    {
        return Err(
            AppError::new("VALIDATION_FAILED", "validation", "后端探活目标无效")
                .for_server(&input.server_id),
        );
    }
    let host = if input.target_host.contains(':') {
        format!(
            "[{}]",
            input
                .target_host
                .trim()
                .trim_start_matches('[')
                .trim_end_matches(']')
        )
    } else {
        input.target_host.trim().to_string()
    };
    let url = format!("{}://{}:{}/", input.scheme, host, input.target_port);
    let command = format!(
        "curl --silent --show-error --output /dev/null --connect-timeout 5 --max-time 10 --write-out '%{{http_code}} %{{time_total}}' -- {}",
        crate::security::shell_escape(&url)
    );
    let result = ssh
        .execute(&input.server_id, &command, Duration::from_secs(15))
        .await?;
    if result.stderr.contains("command not found") || result.stderr.contains("not found") {
        return Err(AppError::new(
            "COMMAND_UNAVAILABLE",
            "capability",
            "远端没有 curl，无法执行后端探活",
        )
        .for_server(&input.server_id));
    }
    let fields: Vec<_> = result.stdout.split_whitespace().collect();
    let status_code = fields
        .first()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0);
    let latency_ms = fields
        .get(1)
        .and_then(|value| value.parse::<f64>().ok())
        .map(|value| (value * 1000.0).round() as u64);
    Ok(NginxBackendProbeResult {
        reachable: result.exit_code == 0 && status_code.is_some_and(|value| value < 600),
        status_code,
        latency_ms,
        detail: if result.exit_code == 0 {
            "curl 已收到后端响应".into()
        } else {
            result.stderr.trim().to_string()
        },
    })
}

/// 写入独立代理配置，测试失败自动恢复备份，成功后 reload 并验证服务状态。
pub async fn save_proxy(
    ssh: &SshConnectionManager,
    input: NginxProxyInput,
) -> AppResult<NginxSnapshot> {
    validate_proxy_input(&input)?;
    let existing = snapshot(ssh, &input.server_id).await?;
    if !existing.installed {
        return Err(
            AppError::new("NGINX_NOT_INSTALLED", "nginx", "远端未安装 Nginx")
                .for_server(&input.server_id),
        );
    }
    if !existing.managed_conf_supported {
        return Err(AppError::new(
            "NGINX_INCLUDE_UNSUPPORTED",
            "nginx",
            "当前 Nginx 配置未包含 /etc/nginx/conf.d/*.conf，已阻止写入不会生效的配置",
        )
        .for_server(&input.server_id)
        .suggestion(
            "请先在 Nginx 配置中明确加入受控 include，或使用文件编辑器选择已有 include 目录",
        ));
    }
    if input.enable_https {
        let sftp = ssh.open_sftp(&input.server_id).await?;
        for path in [&input.certificate_path, &input.certificate_key_path]
            .into_iter()
            .flatten()
        {
            if !sftp.try_exists(path).await.map_err(|error| {
                AppError::new("SFTP_FAILED", "nginx", "无法检查 TLS 文件")
                    .details(error)
                    .for_server(&input.server_id)
            })? {
                let _ = sftp.close().await;
                return Err(AppError::new(
                    "TLS_FILE_NOT_FOUND",
                    "nginx",
                    "HTTPS 配置要求证书和私钥文件已存在",
                )
                .for_server(&input.server_id));
            }
        }
        let _ = sftp.close().await;
    }
    let config_path = managed_path(
        existing
            .managed_conf_dir
            .as_deref()
            .unwrap_or("/etc/nginx/conf.d"),
        &input.name,
    )?;
    let content = render_proxy(&input);
    let temporary = format!("/tmp/.relay-nginx-{}.conf", uuid::Uuid::new_v4());
    let sftp = ssh.open_sftp(&input.server_id).await?;
    let mut file = sftp.create(&temporary).await.map_err(|error| {
        AppError::new("SFTP_FAILED", "nginx", "无法创建 Nginx 临时配置")
            .details(error)
            .for_server(&input.server_id)
    })?;
    file.write_all(content.as_bytes()).await.map_err(|error| {
        AppError::new("SFTP_FAILED", "nginx", "无法写入 Nginx 临时配置")
            .details(error)
            .for_server(&input.server_id)
    })?;
    file.flush().await.map_err(|error| {
        AppError::new("SFTP_FAILED", "nginx", "无法刷新 Nginx 临时配置")
            .details(error)
            .for_server(&input.server_id)
    })?;
    file.sync_all().await.map_err(|error| {
        AppError::new("SFTP_FAILED", "nginx", "无法同步 Nginx 临时配置")
            .details(error)
            .for_server(&input.server_id)
    })?;
    drop(file);
    let backup = format!("{config_path}.relay-backup-{}", uuid::Uuid::new_v4());
    let control = existing
        .container_id
        .as_deref()
        .map(|id| {
            format!(
                "docker exec {} openresty",
                crate::security::shell_escape(id)
            )
        })
        .unwrap_or_else(|| {
            crate::security::shell_escape(existing.binary.as_deref().unwrap_or("nginx"))
        });
    let command = format!(
        "set -u; target={target}; backup={backup}; temporary={temporary}; restore() {{ if [ -f \"$backup\" ]; then cp -a -- \"$backup\" \"$target\"; else rm -f -- \"$target\"; fi; }}; if [ -f \"$target\" ] && ! cp -a -- \"$target\" \"$backup\"; then rm -f -- \"$temporary\"; exit 41; fi; if ! install -m 0644 -- \"$temporary\" \"$target\"; then restore; rm -f -- \"$temporary\"; exit 42; fi; rm -f -- \"$temporary\"; if ! {control} -t; then restore; {control} -t >/dev/null 2>&1 || true; exit 43; fi; if ! {control} -s reload; then restore; {control} -t >/dev/null 2>&1 || true; exit 44; fi",
        target = crate::security::shell_escape(&config_path),
        backup = crate::security::shell_escape(&backup),
        temporary = crate::security::shell_escape(&temporary),
        control = control
    );
    let result = ssh
        .execute_privileged(&input.server_id, &command, Duration::from_secs(60))
        .await?;
    let _ = sftp.close().await;
    if result.exit_code != 0 {
        return Err(AppError::new(
            "NGINX_CONFIG_INVALID",
            "nginx",
            "Nginx 配置检查或 reload 失败，已恢复备份",
        )
        .details(result.stderr)
        .for_server(&input.server_id));
    }
    snapshot(ssh, &input.server_id).await
}

/// 返回固定的 Nginx 探测脚本，不把用户输入拼入命令。
fn nginx_dump_command() -> &'static str {
    "set +e; container=''; if command -v docker >/dev/null 2>&1; then container=$(docker ps --format '{{.ID}}\\t{{.Image}}' | awk -F '\\t' 'tolower($2) ~ /(openresty|nginx)/ {print $1; exit}'); fi; if [ -n \"$container\" ]; then printf '__CONTAINER__%s\\n' \"$container\"; printf '__BINARY__openresty\\n'; printf '__FLAVOR__openresty\\n'; printf '__RUNNING__\\n'; image=$(docker inspect --format '{{.Config.Image}}' \"$container\" 2>/dev/null); [ -n \"$image\" ] && printf '__IMAGE__%s\\n' \"$image\"; host_dir=$(docker inspect --format '{{range .Mounts}}{{if eq .Destination \"/usr/local/openresty/nginx/conf/conf.d\"}}{{.Source}}{{end}}{{end}}' \"$container\" 2>/dev/null); [ -n \"$host_dir\" ] && printf '__MANAGED_DIR__%s\\n' \"$host_dir\"; site_host_root=$(docker inspect --format '{{range .Mounts}}{{if eq .Destination \"/www\"}}{{.Source}}{{end}}{{end}}' \"$container\" 2>/dev/null); [ -n \"$site_host_root\" ] && printf '__SITE_HOST_ROOT__%s\\n' \"$site_host_root\"; printf '__CONTAINER_SITE_ROOT__/www\\n'; docker exec \"$container\" openresty -v 2>&1; docker exec \"$container\" openresty -T 2>&1; exit $?; fi; binary=''; for candidate in nginx openresty /usr/local/openresty/nginx/sbin/nginx /usr/local/openresty/bin/openresty /usr/local/openresty/bin/nginx /opt/1panel/apps/openresty/openresty/bin/openresty /opt/1panel/apps/openresty/openresty/bin/nginx; do if command -v \"$candidate\" >/dev/null 2>&1; then binary=$(command -v \"$candidate\"); break; fi; if [ -x \"$candidate\" ]; then binary=\"$candidate\"; break; fi; done; if [ -z \"$binary\" ]; then printf 'nginx/openresty: not found\\n' >&2; exit 127; fi; printf '__BINARY__%s\\n' \"$binary\"; case \"$binary\" in *openresty*) printf '__FLAVOR__openresty\\n';; *) printf '__FLAVOR__nginx\\n';; esac; if (command -v systemctl >/dev/null 2>&1 && (systemctl is-active --quiet nginx || systemctl is-active --quiet openresty)) || (command -v pgrep >/dev/null 2>&1 && pgrep -f -- \"$binary\" >/dev/null 2>&1); then printf '__RUNNING__\\n'; else printf '__STOPPED__\\n'; fi; \"$binary\" -v 2>&1; \"$binary\" -T 2>&1"
}

/// 校验会被写入 Nginx 配置的字段，阻止控制字符和路径穿越。
fn validate_proxy_input(input: &NginxProxyInput) -> AppResult<()> {
    if input.name.trim().is_empty()
        || input.server_name.trim().is_empty()
        || input.target_host.trim().is_empty()
        || input.target_port == 0
        || input.listen_port == 0
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "validation",
            "Nginx 代理字段不完整",
        ));
    }
    if !matches!(input.upstream_scheme.as_str(), "http" | "https")
        || !input.location.starts_with('/')
        || input.name.contains('/')
        || input.name.contains('.')
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "validation",
            "Nginx 代理字段格式无效",
        ));
    }
    if input.enable_https
        && (input.https_port == 0
            || input
                .certificate_path
                .as_deref()
                .unwrap_or_default()
                .is_empty()
            || input
                .certificate_key_path
                .as_deref()
                .unwrap_or_default()
                .is_empty())
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "validation",
            "启用 HTTPS 时必须填写 HTTPS 端口、证书路径和私钥路径",
        ));
    }
    if [
        input.server_name.as_str(),
        input.location.as_str(),
        input.target_host.as_str(),
        input.certificate_path.as_deref().unwrap_or_default(),
        input.certificate_key_path.as_deref().unwrap_or_default(),
    ]
    .iter()
    .any(|value| {
        value
            .chars()
            .any(|character| character == '\n' || character == '\r' || character == '\0')
    }) {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "validation",
            "Nginx 配置字段不能包含控制字符",
        ));
    }
    Ok(())
}

/// 将用户配置名映射到固定的 managed conf 路径。
fn managed_path(directory: &str, name: &str) -> AppResult<String> {
    if directory.is_empty()
        || !directory.starts_with('/')
        || directory.contains(' ')
        || directory.contains('\n')
        || directory.contains('\r')
        || directory.contains("..")
        || name.is_empty()
        || !name
            .chars()
            .all(|value| value.is_ascii_alphanumeric() || value == '-' || value == '_')
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "validation",
            "Nginx 配置名称只能包含字母、数字、下划线和连字符",
        ));
    }
    Ok(format!("{directory}/relay-{name}.conf"))
}

/// 根据已校验的表单生成独立的 Nginx server block。
fn render_proxy(input: &NginxProxyInput) -> String {
    let websocket = if input.websocket {
        "        proxy_set_header Upgrade $http_upgrade;\n        proxy_set_header Connection \"upgrade\";\n"
    } else {
        ""
    };
    let host = if input.preserve_host {
        "        proxy_set_header Host $host;\n"
    } else {
        ""
    };
    let tls = if input.enable_https {
        format!(
            "    listen {} ssl;\n    ssl_certificate {};\n    ssl_certificate_key {};\n",
            input.https_port,
            input.certificate_path.as_deref().unwrap_or_default(),
            input.certificate_key_path.as_deref().unwrap_or_default()
        )
    } else {
        String::new()
    };
    format!("# Managed by 1Panel Client; edit through the desktop client.\nserver {{\n    listen {};\n{}    server_name {};\n    location {} {{\n        proxy_pass {}://{}:{};\n{}{}        proxy_set_header X-Real-IP $remote_addr;\n        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;\n    }}\n}}\n", input.listen_port, tls, input.server_name.trim(), input.location.trim(), input.upstream_scheme, input.target_host.trim(), input.target_port, websocket, host)
}

/// 通过远端 openssl 读取证书到期元数据；只读取证书公钥文件，不读取私钥内容。
async fn enrich_certificate_expiry(
    ssh: &SshConnectionManager,
    server_id: &str,
    snapshot: &mut NginxSnapshot,
) -> AppResult<()> {
    for certificate in &mut snapshot.certificates {
        let certificate_path = crate::security::shell_escape(&certificate.certificate_path);
        let command = if let Some(container_id) = snapshot.container_id.as_deref() {
            format!(
                "docker exec {} openssl x509 -in {} -noout -enddate 2>/dev/null",
                crate::security::shell_escape(container_id),
                certificate_path
            )
        } else {
            format!(
                "openssl x509 -in {} -noout -enddate 2>/dev/null",
                certificate_path
            )
        };
        let result = ssh
            .execute(server_id, &command, Duration::from_secs(10))
            .await?;
        if result.exit_code == 0 {
            if let Some((expires_at, days_remaining)) = parse_certificate_end_date(&result.stdout) {
                certificate.expires_at = Some(expires_at);
                certificate.days_remaining = Some(days_remaining);
            }
        } else {
            snapshot.warnings.push(format!(
                "无法读取证书到期时间：{}",
                certificate.certificate_path
            ));
        }
    }
    Ok(())
}

/// 解析 openssl 的 notAfter 输出并计算相对当前时间的剩余天数。
fn parse_certificate_end_date(input: &str) -> Option<(String, i64)> {
    let value = input
        .lines()
        .find_map(|line| line.trim().strip_prefix("notAfter="))?
        .trim();
    let date_value = value.strip_suffix(" GMT").unwrap_or(value);
    let parsed = NaiveDateTime::parse_from_str(date_value, "%b %e %H:%M:%S %Y")
        .or_else(|_| NaiveDateTime::parse_from_str(date_value, "%b %d %H:%M:%S %Y"))
        .ok()?;
    let parsed = DateTime::<Utc>::from_naive_utc_and_offset(parsed, Utc);
    Some((value.to_string(), (parsed - Utc::now()).num_days()))
}

/// 解析 `nginx -T` 中的 include source marker、server/location/upstream 和 proxy_pass。
pub fn parse_nginx_dump(input: &str) -> NginxSnapshot {
    let mut parser = Parser::default();
    parser.parse(input);
    parser.finish()
}

#[derive(Default)]
struct Parser {
    source_file: String,
    stack: Vec<Frame>,
    servers: Vec<ServerBuilder>,
    upstreams: Vec<String>,
    proxies: Vec<ReverseProxy>,
    certificates: Vec<CertificateMetadata>,
    config_sources: Vec<String>,
    version: Option<String>,
    image: Option<String>,
    flavor: String,
    binary: Option<String>,
    container_id: Option<String>,
    config_path: Option<String>,
    warnings: Vec<String>,
    running: Option<bool>,
    source_line: usize,
    managed_conf_supported: bool,
    managed_conf_dir: Option<String>,
    container_site_root: Option<String>,
    site_host_root: Option<String>,
}

#[derive(Clone)]
struct Frame {
    server: Option<usize>,
    location: String,
}
#[derive(Default, Clone)]
struct ServerBuilder {
    names: Vec<String>,
    listen: Vec<String>,
    ssl: bool,
}

impl Parser {
    /// 扫描 nginx -T 的 source marker、block 和 directive。
    fn parse(&mut self, input: &str) {
        for raw in input.lines() {
            let line = raw.trim();
            if line == "__RUNNING__" {
                self.running = Some(true);
                continue;
            }
            if line == "__STOPPED__" {
                self.running = Some(false);
                continue;
            }
            if let Some(binary) = line.strip_prefix("__BINARY__") {
                self.binary = Some(binary.to_string());
                continue;
            }
            if let Some(container_id) = line.strip_prefix("__CONTAINER__") {
                if container_id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                    self.container_id = Some(container_id.to_string());
                }
                continue;
            }
            if let Some(image) = line.strip_prefix("__IMAGE__") {
                self.image = Some(image.to_string());
                continue;
            }
            if let Some(directory) = line.strip_prefix("__MANAGED_DIR__") {
                self.managed_conf_dir = Some(directory.to_string());
                continue;
            }
            if let Some(directory) = line.strip_prefix("__SITE_HOST_ROOT__") {
                self.site_host_root = Some(directory.to_string());
                continue;
            }
            if let Some(directory) = line.strip_prefix("__CONTAINER_SITE_ROOT__") {
                self.container_site_root = Some(directory.to_string());
                continue;
            }
            if let Some(flavor) = line.strip_prefix("__FLAVOR__") {
                self.flavor = flavor.to_string();
                continue;
            }
            if let Some(path) = line
                .strip_prefix("# configuration file ")
                .and_then(|value| value.strip_suffix(':'))
            {
                self.source_file = path.to_string();
                if !self.config_sources.iter().any(|value| value == path) {
                    self.config_sources.push(path.to_string());
                }
                self.config_path.get_or_insert_with(|| path.to_string());
                self.source_line = 0;
                continue;
            }
            self.source_line += 1;
            if let Some(tail) = line.strip_prefix("nginx version:") {
                self.version = tail
                    .split_once("nginx/")
                    .or_else(|| tail.split_once("openresty/"))
                    .map(|(_, version)| version.split_whitespace().next().unwrap_or(version))
                    .map(str::to_string);
            }
            let (code, _) = strip_comment(raw);
            let mut tokens = tokenize(&code);
            if tokens.is_empty() {
                continue;
            }
            let has_block = tokens.last().map(|value| value == "{").unwrap_or(false);
            if has_block {
                tokens.pop();
                self.enter_block(&tokens);
                continue;
            }
            let closing_blocks = tokens.iter().filter(|value| value.as_str() == "}").count();
            tokens.retain(|value| value != "}");
            if tokens.last().map(|value| value == ";").unwrap_or(false) {
                tokens.pop();
            }
            if !tokens.is_empty() {
                self.directive(&tokens);
            }
            for _ in 0..closing_blocks {
                self.leave_block();
            }
        }
    }

    /// 进入一个配置 block，并继承外层 server/location 上下文。
    fn enter_block(&mut self, tokens: &[String]) {
        let kind = tokens.first().cloned().unwrap_or_default();
        let server = if kind == "server" {
            self.servers.push(ServerBuilder::default());
            Some(self.servers.len() - 1)
        } else {
            self.stack.iter().rev().find_map(|frame| frame.server)
        };
        if kind == "upstream" {
            if let Some(name) = tokens.get(1) {
                self.upstreams.push(name.clone());
            }
        }
        let location = if kind == "location" {
            tokens.get(1).cloned().unwrap_or_else(|| "/".into())
        } else {
            self.stack
                .iter()
                .rev()
                .find(|frame| frame.server == server)
                .map(|frame| frame.location.clone())
                .unwrap_or_else(|| "/".into())
        };
        self.stack.push(Frame { server, location });
    }

    /// 处理当前上下文中影响代理列表的 directive。
    fn directive(&mut self, tokens: &[String]) {
        let name = tokens.first().map(String::as_str).unwrap_or_default();
        let args = &tokens[1..];
        let server_index = self.stack.iter().rev().find_map(|frame| frame.server);
        match name {
            "server_name" => {
                if let Some(index) = server_index {
                    self.servers[index].names.extend(args.iter().cloned());
                }
            }
            "listen" => {
                if let Some(index) = server_index {
                    self.servers[index]
                        .listen
                        .push(args.first().cloned().unwrap_or_default());
                    self.servers[index].ssl |= args.iter().any(|arg| arg == "ssl");
                }
            }
            "proxy_pass" => {
                if let Some(index) = server_index {
                    if let Some(value) = args.first() {
                        let frame = self
                            .stack
                            .iter()
                            .rev()
                            .find(|frame| frame.server == Some(index));
                        let location = frame
                            .map(|value| value.location.clone())
                            .unwrap_or_else(|| "/".into());
                        let (upstream, host, port) = parse_target(value);
                        let names = self.servers[index].names.clone();
                        let listen = self.servers[index].listen.clone();
                        let ssl = self.servers[index].ssl;
                        self.proxies.push(ReverseProxy {
                            server_names: names,
                            listen,
                            location,
                            upstream,
                            target_host: host,
                            target_port: port,
                            ssl,
                            source_file: self.source_file.clone(),
                            source_line: self.source_line,
                        });
                    }
                }
            }
            "include" => {
                if let Some(value) = args.first() {
                    if let Some(directory) = value
                        .strip_suffix("/*.conf")
                        .or_else(|| value.strip_suffix("/*"))
                    {
                        self.managed_conf_supported = true;
                        self.managed_conf_dir
                            .get_or_insert_with(|| directory.to_string());
                    }
                }
            }
            "ssl_certificate" => {
                if let Some(path) = args.first() {
                    self.certificates.push(CertificateMetadata {
                        certificate_path: path.clone(),
                        private_key_path: None,
                        source_file: self.source_file.clone(),
                        source_line: self.source_line,
                        expires_at: None,
                        days_remaining: None,
                    });
                }
            }
            "ssl_certificate_key" => {
                if let Some(path) = args.first() {
                    let source_file = self.source_file.clone();
                    if let Some(certificate) = self
                        .certificates
                        .iter_mut()
                        .rev()
                        .find(|value| value.source_file == source_file)
                    {
                        certificate.private_key_path = Some(path.clone());
                    }
                }
            }
            "root" | "proxy_set_header" | "return" | "default_type" | "access_log"
            | "error_log" => {}
            _ => {
                if !name.starts_with("#") {
                    self.warnings
                        .push(format!("未专门解析的 directive: {name}"));
                }
            }
        }
    }

    /// 离开最近的配置 block。
    fn leave_block(&mut self) {
        self.stack.pop();
    }

    /// 将 parser 内部状态转换为 IPC DTO。
    fn finish(self) -> NginxSnapshot {
        let running = self.running.unwrap_or(false);
        NginxSnapshot {
            installed: true,
            running,
            flavor: if self.flavor.is_empty() {
                "nginx".into()
            } else {
                self.flavor
            },
            binary: self.binary,
            container_id: self.container_id,
            container_site_root: self.container_site_root,
            site_host_root: self.site_host_root,
            version: self
                .image
                .as_deref()
                .and_then(|raw| {
                    let tag = raw.rsplit(':').next().unwrap_or(raw);
                    (tag != raw && tag != "latest" && tag.contains('.')).then(|| tag.to_string())
                })
                .or_else(|| self.version.clone()),
            image: self.image,
            config_path: self.config_path,
            config_test: None,
            managed_conf_supported: self.managed_conf_supported,
            managed_conf_dir: self.managed_conf_dir,
            servers: self.servers.len(),
            upstreams: self.upstreams.len(),
            proxies: self.proxies,
            certificates: self.certificates,
            config_sources: self.config_sources,
            warnings: self.warnings,
        }
    }
}

/// 去掉不在引号内的 Nginx 注释，并保留配置字符串。
fn strip_comment(input: &str) -> (String, bool) {
    let mut quoted = false;
    for (index, value) in input.char_indices() {
        if value == '"' {
            quoted = !quoted;
        }
        if value == '#' && !quoted {
            return (input[..index].to_string(), true);
        }
    }
    (input.to_string(), false)
}

/// 将 Nginx 配置切分为 directive、参数、分号和大括号 token。
fn tokenize(input: &str) -> Vec<String> {
    let mut output = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    for value in input.chars() {
        match value {
            '"' => quoted = !quoted,
            '{' | '}' | ';' if !quoted => {
                if !current.is_empty() {
                    output.push(std::mem::take(&mut current));
                }
                output.push(value.to_string());
            }
            value if value.is_whitespace() && !quoted => {
                if !current.is_empty() {
                    output.push(std::mem::take(&mut current));
                }
            }
            value => current.push(value),
        }
    }
    if !current.is_empty() {
        output.push(current);
    }
    output
}

/// 从 proxy_pass URI 提取原始 upstream、目标 host 和显式端口。
fn parse_target(value: &str) -> (String, String, Option<u16>) {
    let without_scheme = value
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(value);
    let host_port = without_scheme.split('/').next().unwrap_or(without_scheme);
    if let Some((host, port)) = host_port.rsplit_once(':') {
        if let Ok(port) = port.parse() {
            return (
                value.to_string(),
                host.trim_matches('[').trim_matches(']').into(),
                Some(port),
            );
        }
    }
    (value.to_string(), host_port.into(), None)
}

#[cfg(test)]
mod tests {
    use super::{parse_certificate_end_date, parse_nginx_dump, ReverseProxy};

    #[test]
    fn parses_include_source_and_proxy() {
        let value = parse_nginx_dump(include_str!("../../../../fixtures/nginx-dump.txt"));
        assert_eq!(value.version.as_deref(), Some("1.24.0"));
        assert_eq!(value.config_path.as_deref(), Some("/etc/nginx/nginx.conf"));
        assert!(value.running);
        assert!(value.managed_conf_supported);
        assert_eq!(value.servers, 1);
        assert_eq!(
            value.certificates[0].certificate_path,
            "/etc/letsencrypt/live/demo/fullchain.pem"
        );
        assert_eq!(
            value.certificates[0].private_key_path.as_deref(),
            Some("/etc/letsencrypt/live/demo/privkey.pem")
        );
        assert_eq!(
            value.proxies[0],
            ReverseProxy {
                server_names: vec!["demo.example.com".into(), "api.example.com".into()],
                listen: vec!["443".into()],
                location: "/".into(),
                upstream: "http://127.0.0.1:3000".into(),
                target_host: "127.0.0.1".into(),
                target_port: Some(3000),
                ssl: true,
                source_file: "/etc/nginx/conf.d/demo.conf".into(),
                source_line: 7
            }
        );
    }

    #[test]
    fn preserves_named_upstream_target() {
        let value = parse_nginx_dump("# configuration file /etc/nginx/site.conf:\nupstream backend {\n    server 127.0.0.1:8080;\n}\nserver {\n    listen 80;\n    server_name site.test;\n    location /api {\n        proxy_pass http://backend;\n    }\n}\n");
        assert_eq!(value.upstreams, 1);
        assert_eq!(value.proxies[0].target_host, "backend");
        assert_eq!(value.proxies[0].target_port, None);
    }

    /// 验证 openssl 的证书到期行可以转换为可展示的日期和剩余天数。
    #[test]
    fn parses_certificate_expiry() {
        let value = parse_certificate_end_date("notAfter=Jan  1 00:00:00 2099 GMT\n").unwrap();
        assert!(value.0.starts_with("Jan"));
        assert!(value.1 > 0);
    }
}

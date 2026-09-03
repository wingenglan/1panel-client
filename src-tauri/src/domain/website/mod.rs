use crate::domain::nginx::NginxSnapshot;
use crate::domain::ssh::SshConnectionManager;
use crate::errors::{AppError, AppResult};
use crate::security::shell_escape;
use chrono::{DateTime, NaiveDateTime, Utc};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// 网站类型；静态站点使用 root/try_files，反向代理使用 proxy_pass。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum WebsiteKind {
    Static,
    Proxy,
}

/// 一个受控站点的配置摘要，不包含证书私钥内容。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WebsiteRecord {
    pub domain: String,
    pub kind: WebsiteKind,
    pub enabled: bool,
    pub listen_port: u16,
    pub root_path: Option<String>,
    pub upstream: Option<String>,
    /// FastCGI Unix socket used by the optional PHP-FPM binding.
    pub php_runtime: Option<String>,
    pub ssl: bool,
    pub certificate_path: Option<String>,
    pub expires_at: Option<String>,
    pub config_path: String,
}

/// 网站页面需要的远端配置目录、运行时根目录和站点列表。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebsiteSnapshot {
    pub supported: bool,
    pub managed_conf_dir: Option<String>,
    /// OpenResty/Nginx 版本（如 1.31.1.1-0-noble），来自 `nginx -v`。
    pub nginx_version: Option<String>,
    pub runtime_root: Option<String>,
    pub host_root: Option<String>,
    pub websites: Vec<WebsiteRecord>,
    pub php_runtimes: Vec<PhpRuntime>,
    pub certificate_tools: CertificateTools,
    pub warnings: Vec<String>,
    pub fetched_at: String,
}

/// 描述远端 PHP CLI/FPM 运行时及其 systemd 状态。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PhpRuntime {
    pub id: String,
    pub version: Option<String>,
    pub binary: Option<String>,
    pub service: Option<String>,
    pub socket_path: Option<String>,
    pub installed: bool,
    pub running: bool,
}

/// 描述 PHP-FPM 安装计划，供用户确认后执行。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhpInstallPlan {
    pub package_manager: String,
    pub packages: Vec<String>,
    pub services: Vec<String>,
    pub command: String,
    pub risk: String,
}

/// PHP-FPM 安装请求；安装前必须由用户明确确认。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhpInstallInput {
    pub server_id: String,
    pub confirmed: bool,
}

/// PHP-FPM 安装结果及安装后运行时探测。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhpInstallResult {
    pub package_manager: String,
    pub runtimes: Vec<PhpRuntime>,
    pub output: String,
}

/// 描述远端可用于 ACME 证书申请或续期的工具，不代表当前已签发证书。
#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CertificateTools {
    pub certbot: bool,
    pub acme_sh: bool,
}

/// 一条证书续期规划；用于批量策略展示，实际签发仍走单域名 ACME 流程。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CertificateRenewalPlan {
    pub domain: String,
    /// issue 或 renew。
    pub action: String,
    /// missing 或 expiring。
    pub reason: String,
    pub expires_at: Option<String>,
    pub certificate_path: Option<String>,
    pub renew_before_days: u32,
}

/// 申请或续期一个 HTTP-01 ACME 证书；所有写入都要求用户明确确认。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CertificateActionInput {
    pub server_id: String,
    pub domain: String,
    pub email: String,
    pub webroot: String,
    pub action: String,
    /// ACME challenge type; HTTP-01 remains the default for compatibility.
    #[serde(default = "default_certificate_challenge")]
    pub challenge: String,
    /// Supported DNS provider for DNS-01; credentials are used only for this request.
    pub dns_provider: Option<String>,
    /// DNS provider token; used only for the current remote command and never persisted.
    pub dns_api_token: Option<SecretString>,
    pub confirmed: bool,
}

/// 返回证书工具、证书路径和脱敏后的命令输出，私钥内容永不返回。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CertificateActionResult {
    pub domain: String,
    pub action: String,
    pub challenge: String,
    pub dns_provider: Option<String>,
    pub tool: String,
    pub certificate_path: String,
    pub certificate_key_path: String,
    pub output: String,
}

/// 将已签发证书绑定到同域的客户端受控站点；不会修改非客户端生成的配置。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BindWebsiteCertificateInput {
    pub server_id: String,
    pub domain: String,
    pub certificate_path: String,
    pub certificate_key_path: String,
    pub confirmed: bool,
}

/// Keeps existing callers on the safe HTTP-01 path unless DNS-01 is explicitly selected.
fn default_certificate_challenge() -> String {
    "http01".into()
}

/// 创建或替换一个静态/反向代理站点。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveWebsiteInput {
    pub server_id: String,
    pub domain: String,
    pub kind: String,
    pub listen_port: u16,
    pub root_path: Option<String>,
    /// Optional PHP-FPM runtime id; only static sites can bind a runtime.
    pub php_runtime: Option<String>,
    /// Optional FastCGI socket path visible inside the Nginx/OpenResty container.
    /// When set, this overrides runtime resolution so a containerized PHP-FPM
    /// socket reachable from the Nginx container can be used directly.
    pub php_socket: Option<String>,
    pub upstream_scheme: Option<String>,
    pub upstream_host: Option<String>,
    pub upstream_port: Option<u16>,
    pub enable_https: bool,
    pub https_port: u16,
    pub certificate_path: Option<String>,
    pub certificate_key_path: Option<String>,
    pub confirmed: bool,
}

/// 启用、停用或删除一个已由客户端管理的站点。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebsiteActionInput {
    pub server_id: String,
    pub domain: String,
    pub action: String,
    pub confirmed: bool,
}

/// 停止/启动/重启/重载 OpenResty 服务的用户输入；必须由用户显式确认。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NginxServiceInput {
    pub server_id: String,
    /// stop | start | restart | reload
    pub action: String,
    pub confirmed: bool,
}

/// 探测 PHP CLI 和固定候选 PHP-FPM service，不执行安装或配置写入。
async fn probe_php_runtimes(
    ssh: &SshConnectionManager,
    server_id: &str,
) -> AppResult<Vec<PhpRuntime>> {
    let result = ssh
        .execute_system(
            server_id,
            "set +e; if command -v php >/dev/null 2>&1; then version=$(php -r 'echo PHP_VERSION;' 2>/dev/null); printf '__PHP__\\tcli\\t%s\\t%s\\t%s\\t%s\\t%s\\n' \"$version\" \"$(command -v php)\" \"\" \"1\" \"\"; else printf '__PHP_MISSING__\\n'; fi; if command -v systemctl >/dev/null 2>&1; then systemctl list-unit-files --type=service --no-legend 'php*-fpm.service' 2>/dev/null | awk '{print $1}' | sed 's/\\.service$//' | while IFS= read -r service; do [ -n \"$service\" ] || continue; version=$(\"$service\" -v 2>/dev/null | sed -n '1s/.*PHP \\([^ ]*\\).*/\\1/p'); running=0; systemctl is-active --quiet \"$service\" && running=1; socket=$(find /run/php /var/run/php -maxdepth 1 -type s -name \"$service.sock\" -print -quit 2>/dev/null); printf '__PHP__\\t%s\\t%s\\t%s\\t%s\\t%s\\t%s\\n' \"$service\" \"$version\" \"$(command -v \"$service\" 2>/dev/null || true)\" \"$service\" \"$running\" \"$socket\"; done; fi",
            Duration::from_secs(30),
        )
        .await?;
    if result.exit_code != 0 && result.stdout.is_empty() {
        return Err(
            AppError::new("PHP_PROBE_FAILED", "website", "无法探测 PHP-FPM 运行时")
                .details(result.stderr)
                .for_server(server_id),
        );
    }
    Ok(parse_php_runtimes(&result.stdout))
}

/// 解析 PHP marker，去重同一 service 的重复探测结果。
fn parse_php_runtimes(output: &str) -> Vec<PhpRuntime> {
    let mut runtimes = Vec::new();
    for line in output.lines() {
        let fields = line.split('\t').collect::<Vec<_>>();
        if fields.first().copied() != Some("__PHP__") || fields.len() < 6 {
            continue;
        }
        let runtime = PhpRuntime {
            id: fields[1].to_string(),
            version: non_empty(fields[2]),
            binary: non_empty(fields[3]),
            service: non_empty(fields[4]),
            socket_path: fields.get(6).and_then(|value| non_empty(value)),
            installed: true,
            running: fields[5] == "1",
        };
        if !runtimes
            .iter()
            .any(|item: &PhpRuntime| item.id == runtime.id)
        {
            runtimes.push(runtime);
        }
    }
    runtimes
}

/// 将探测输出中的空字段转换为可选值。
fn non_empty(value: &str) -> Option<String> {
    (!value.trim().is_empty()).then(|| value.trim().to_string())
}

/// 生成 PHP-FPM 固定包安装计划，不执行远端写入。
pub async fn php_install_plan(
    ssh: &SshConnectionManager,
    server_id: &str,
) -> AppResult<PhpInstallPlan> {
    let current = probe_php_runtimes(ssh, server_id).await?;
    if !current.is_empty() {
        return Err(
            AppError::new("ALREADY_INSTALLED", "website", "远端已存在 PHP 运行时")
                .for_server(server_id),
        );
    }
    let package_manager = detect_php_package_manager(ssh, server_id).await?;
    let packages = vec![
        "php-fpm".into(),
        "php-cli".into(),
        "php-mysql".into(),
        "php-curl".into(),
        "php-gd".into(),
        "php-mbstring".into(),
        "php-xml".into(),
        "php-zip".into(),
    ];
    let package_command =
        crate::domain::platform::adapter_for(&package_manager).install_command(&packages.join(" "));
    if package_command.is_empty() {
        return Err(AppError::new(
            "UNSUPPORTED_PLATFORM",
            "website",
            "远端没有 apt 或 dnf 包管理器",
        )
        .for_server(server_id));
    }
    let services = ["php-fpm", "php8.3-fpm", "php8.2-fpm", "php8.1-fpm"];
    Ok(PhpInstallPlan {
        package_manager,
        packages,
        services: services.iter().map(|value| (*value).into()).collect(),
        command: format!(
            "set -e; {package_command}; if command -v systemctl >/dev/null 2>&1; then for service in {services}; do if systemctl list-unit-files \"$service.service\" >/dev/null 2>&1; then systemctl enable --now \"$service\"; break; fi; done; fi",
            services = services.join(" "),
        ),
        risk: "将通过远端系统包管理器安装 PHP-FPM 及常用扩展，并尝试启用服务；不会自动修改任何网站配置。"
            .into(),
    })
}

/// 在用户确认后安装 PHP-FPM，并重新探测运行时。
pub async fn php_install(
    ssh: &SshConnectionManager,
    input: PhpInstallInput,
) -> AppResult<PhpInstallResult> {
    if !input.confirmed {
        return Err(AppError::new(
            "CONFIRMATION_REQUIRED",
            "website",
            "安装 PHP-FPM 需要明确确认",
        )
        .for_server(&input.server_id));
    }
    let plan = php_install_plan(ssh, &input.server_id).await?;
    let result = ssh
        .execute_system(&input.server_id, &plan.command, Duration::from_secs(900))
        .await?;
    if result.exit_code != 0 {
        return Err(
            AppError::new("PHP_INSTALL_FAILED", "website", "PHP-FPM 安装失败")
                .details(result.stderr)
                .for_server(input.server_id),
        );
    }
    let runtimes = probe_php_runtimes(ssh, &input.server_id).await?;
    if runtimes.is_empty() {
        return Err(AppError::new(
            "PHP_VERIFY_FAILED",
            "website",
            "安装后没有发现 PHP-FPM 运行时",
        )
        .for_server(input.server_id));
    }
    Ok(PhpInstallResult {
        package_manager: plan.package_manager,
        runtimes,
        output: format!("{}\\n{}", result.stdout, result.stderr),
    })
}

/// 探测远端 PHP 安装所需的 apt/dnf 包管理器。
async fn detect_php_package_manager(
    ssh: &SshConnectionManager,
    server_id: &str,
) -> AppResult<String> {
    let result = ssh
        .execute_system(
            server_id,
            "if command -v apt-get >/dev/null 2>&1; then printf '__PACKAGE_MANAGER__\\tapt\\n'; elif command -v dnf >/dev/null 2>&1; then printf '__PACKAGE_MANAGER__\\tdnf\\n'; elif command -v yum >/dev/null 2>&1; then printf '__PACKAGE_MANAGER__\\tdnf\\n'; else printf '__PACKAGE_MANAGER__\\tunknown\\n'; fi",
            Duration::from_secs(30),
        )
        .await?;
    for line in result.stdout.lines() {
        let mut fields = line.split('\t');
        if fields.next() != Some("__PACKAGE_MANAGER__") {
            continue;
        }
        match fields.next() {
            Some("apt") => return Ok("apt".into()),
            Some("dnf" | "yum") => return Ok("dnf".into()),
            _ => {}
        }
    }
    Err(
        AppError::new("UNSUPPORTED_PLATFORM", "website", "远端没有支持的包管理器")
            .for_server(server_id),
    )
}

/// 将 Cloudflare certbot 凭据以 0600 临时文件写到远端，避免出现在 shell 命令参数或日志中。
async fn write_dns_credentials(
    ssh: &SshConnectionManager,
    server_id: &str,
    provider: &str,
    token: &SecretString,
) -> AppResult<String> {
    if provider != "cloudflare" {
        return Err(AppError::new(
            "UNSUPPORTED_DNS_PROVIDER",
            "website",
            "certbot DNS-01 仅支持 Cloudflare",
        )
        .for_server(server_id));
    }
    let content = format!("dns_cloudflare_api_token = {}\n", token.expose_secret());
    write_temporary_secret_file(ssh, server_id, "ini", &content).await
}

/// 将 acme.sh DNS provider 环境变量写入 0600 临时文件，操作结束由调用方删除。
async fn write_acme_dns_environment(
    ssh: &SshConnectionManager,
    server_id: &str,
    provider: &str,
    token: &SecretString,
) -> AppResult<String> {
    let content = match provider {
        "cloudflare" => format!("CF_Token={}\n", shell_escape(token.expose_secret())),
        "aliyun" => {
            let (key, secret) = token.expose_secret().split_once(':').ok_or_else(|| {
                AppError::new(
                    "VALIDATION_FAILED",
                    "website",
                    "阿里云 DNS token 必须是 AccessKeyId:AccessKeySecret",
                )
                .for_server(server_id)
            })?;
            format!(
                "Ali_Key={}\nAli_Secret={}\n",
                shell_escape(key),
                shell_escape(secret)
            )
        }
        "dnspod" => {
            let (id, key) = token.expose_secret().split_once(':').ok_or_else(|| {
                AppError::new(
                    "VALIDATION_FAILED",
                    "website",
                    "DNSPod token 必须是 ID:Token",
                )
                .for_server(server_id)
            })?;
            format!("DP_Id={}\nDP_Key={}\n", shell_escape(id), shell_escape(key))
        }
        "tencent" => {
            let (id, key) = token.expose_secret().split_once(':').ok_or_else(|| {
                AppError::new(
                    "VALIDATION_FAILED",
                    "website",
                    "腾讯云 DNS token 必须是 SecretId:SecretKey",
                )
                .for_server(server_id)
            })?;
            format!(
                "Tencent_SecretId={}\nTencent_SecretKey={}\n",
                shell_escape(id),
                shell_escape(key)
            )
        }
        "aws" => {
            let (id, key) = token.expose_secret().split_once(':').ok_or_else(|| {
                AppError::new(
                    "VALIDATION_FAILED",
                    "website",
                    "AWS Route 53 token 必须是 AccessKeyId:SecretAccessKey",
                )
                .for_server(server_id)
            })?;
            format!(
                "AWS_ACCESS_KEY_ID={}\nAWS_SECRET_ACCESS_KEY={}\n",
                shell_escape(id),
                shell_escape(key)
            )
        }
        _ => {
            return Err(AppError::new(
                "UNSUPPORTED_DNS_PROVIDER",
                "website",
                "acme.sh 当前支持 Cloudflare、阿里云、DNSPod、腾讯云和 AWS Route 53 DNS-01",
            )
            .for_server(server_id));
        }
    };
    write_temporary_secret_file(ssh, server_id, "env", &content).await
}

/// 通过 SFTP 写入并 chmod 600 一个远端临时秘密文件，失败时尽力清理。
async fn write_temporary_secret_file(
    ssh: &SshConnectionManager,
    server_id: &str,
    suffix: &str,
    content: &str,
) -> AppResult<String> {
    let path = format!(
        "/tmp/.1panel-client-dns-{}.{}",
        uuid::Uuid::new_v4(),
        suffix
    );
    let sftp = ssh.open_sftp(server_id).await?;
    let mut file = sftp.create(&path).await.map_err(|error| {
        AppError::new("SFTP_FAILED", "website", "无法创建 DNS 临时凭据文件")
            .details(error)
            .for_server(server_id)
    })?;
    file.write_all(content.as_bytes()).await.map_err(|error| {
        AppError::new("SFTP_FAILED", "website", "无法写入 DNS 临时凭据文件")
            .details(error)
            .for_server(server_id)
    })?;
    file.flush().await.map_err(|error| {
        AppError::new("SFTP_FAILED", "website", "无法刷新 DNS 临时凭据文件")
            .details(error)
            .for_server(server_id)
    })?;
    drop(file);
    let _ = sftp.close().await;
    let permission = format!("chmod 600 -- {}", shell_escape(&path));
    let result = ssh
        .execute_system(server_id, &permission, Duration::from_secs(15))
        .await?;
    if result.exit_code != 0 {
        let _ = ssh
            .execute_system(
                server_id,
                &format!("rm -f -- {}", shell_escape(&path)),
                Duration::from_secs(15),
            )
            .await;
        return Err(AppError::new(
            "DNS_CREDENTIALS_FAILED",
            "website",
            "无法保护 DNS 临时凭据文件",
        )
        .details(result.stderr)
        .for_server(server_id));
    }
    Ok(path)
}

/// 读取远端 certbot/acme.sh 是否可用；只执行命令存在性探测。
async fn probe_certificate_tools(
    ssh: &SshConnectionManager,
    server_id: &str,
) -> AppResult<CertificateTools> {
    let result = ssh
        .execute_system(
            server_id,
            "if command -v certbot >/dev/null 2>&1; then printf '__CERTBOT__yes\\n'; else printf '__CERTBOT__no\\n'; fi; if command -v acme.sh >/dev/null 2>&1; then printf '__ACME_SH__yes\\n'; else printf '__ACME_SH__no\\n'; fi",
            Duration::from_secs(15),
        )
        .await?;
    if result.exit_code != 0 {
        return Err(AppError::new(
            "CERTIFICATE_PROBE_FAILED",
            "website",
            "无法探测 ACME 证书工具",
        )
        .details(result.stderr)
        .for_server(server_id));
    }
    Ok(parse_certificate_tools(&result.stdout))
}

/// 构造 acme.sh 的固定证书安装命令，把证书复制到客户端回填/绑定使用的稳定路径。
fn acme_install_command(domain: &str, certificate_path: &str, key_path: &str) -> String {
    let directory = format!("/etc/letsencrypt/live/{domain}");
    format!(
        "mkdir -p -- {} && acme.sh --install-cert -d {} --key-file {} --fullchain-file {}",
        shell_escape(&directory),
        shell_escape(domain),
        shell_escape(key_path),
        shell_escape(certificate_path)
    )
}

/// 申请或续期 HTTP-01/DNS-01 ACME 证书；不会自动修改任何网站配置。
pub async fn certificate_action(
    ssh: &SshConnectionManager,
    input: CertificateActionInput,
) -> AppResult<CertificateActionResult> {
    validate_certificate_action(&input)?;
    if !input.confirmed {
        return Err(
            AppError::new("CONFIRMATION_REQUIRED", "website", "证书操作需要显式确认")
                .for_server(&input.server_id),
        );
    }
    let tools = probe_certificate_tools(ssh, &input.server_id).await?;
    let cert_path = format!("/etc/letsencrypt/live/{}/fullchain.pem", input.domain);
    let key_path = format!("/etc/letsencrypt/live/{}/privkey.pem", input.domain);
    let domain = shell_escape(&input.domain);
    let email = shell_escape(&input.email);
    let webroot = shell_escape(&input.webroot);
    let (tool, command, credential_path) = if input.challenge == "dns01" {
        let provider = input.dns_provider.as_deref().unwrap_or_default();
        let token = input.dns_api_token.as_ref().ok_or_else(|| {
            AppError::new("VALIDATION_FAILED", "website", "DNS-01 需要 DNS API token")
                .for_server(&input.server_id)
        })?;
        if provider == "cloudflare" && tools.certbot {
            let path = write_dns_credentials(ssh, &input.server_id, provider, token).await?;
            let credentials = shell_escape(&path);
            let command = match input.action.as_str() {
                "issue" => format!(
                    "certbot certonly --dns-cloudflare --dns-cloudflare-credentials {credentials} --domain {domain} --non-interactive --agree-tos --email {email} --keep-until-expiring"
                ),
                "renew" => format!(
                    "certbot renew --cert-name {domain} --dns-cloudflare --dns-cloudflare-credentials {credentials} --non-interactive"
                ),
                _ => unreachable!(),
            };
            ("certbot", command, Some(path))
        } else if tools.acme_sh {
            let path = write_acme_dns_environment(ssh, &input.server_id, provider, token).await?;
            let dns_plugin = match provider {
                "cloudflare" => "dns_cf",
                "aliyun" => "dns_ali",
                "dnspod" => "dns_dp",
                "tencent" => "dns_tencent",
                "aws" => "dns_aws",
                _ => unreachable!(),
            };
            let operation = match input.action.as_str() {
                "issue" => format!(
                    "acme.sh --issue --server letsencrypt -d {domain} --dns {dns_plugin} --accountemail {email}"
                ),
                "renew" => format!("acme.sh --renew -d {domain} --dns {dns_plugin}"),
                _ => unreachable!(),
            };
            let source = format!("set -a; . {}; set +a", shell_escape(&path));
            let install = acme_install_command(&input.domain, &cert_path, &key_path);
            (
                "acme.sh",
                format!("{source} && {operation} && {install}"),
                Some(path),
            )
        } else {
            return Err(AppError::new(
                "TOOL_MISSING",
                "website",
                match provider {
                    "aliyun" => "阿里云 DNS-01 需要远端 acme.sh",
                    "dnspod" => "DNSPod DNS-01 需要远端 acme.sh",
                    "tencent" => "腾讯云 DNS-01 需要远端 acme.sh",
                    "aws" => "AWS Route 53 DNS-01 需要远端 acme.sh",
                    _ => "Cloudflare DNS-01 需要远端 certbot（含插件）或 acme.sh",
                },
            )
            .for_server(&input.server_id));
        }
    } else if tools.certbot {
        let command = match input.action.as_str() {
            "issue" => format!(
                "certbot certonly --webroot --webroot-path {webroot} --domain {domain} --non-interactive --agree-tos --email {email} --keep-until-expiring"
            ),
            "renew" => format!("certbot renew --cert-name {domain} --non-interactive"),
            _ => unreachable!(),
        };
        ("certbot", command, None)
    } else if tools.acme_sh {
        let operation = match input.action.as_str() {
            "issue" => format!(
                "acme.sh --issue --server letsencrypt -d {domain} -w {webroot} --accountemail {email}"
            ),
            "renew" => format!("acme.sh --renew -d {domain}"),
            _ => unreachable!(),
        };
        let install = acme_install_command(&input.domain, &cert_path, &key_path);
        ("acme.sh", format!("{operation} && {install}"), None)
    } else {
        return Err(AppError::new(
            "TOOL_MISSING",
            "website",
            "远端没有 certbot 或 acme.sh，请先在工具箱安装证书工具",
        )
        .for_server(&input.server_id));
    };
    let result = ssh
        .execute_system(&input.server_id, &command, Duration::from_secs(900))
        .await;
    if let Some(path) = credential_path {
        let cleanup = format!("rm -f -- {}", shell_escape(&path));
        let _ = ssh
            .execute_system(&input.server_id, &cleanup, Duration::from_secs(15))
            .await;
    }
    let result = result?;
    if result.exit_code != 0 {
        return Err(
            AppError::new("CERTIFICATE_ACTION_FAILED", "website", "ACME 证书操作失败")
                .details(crate::security::redact(&result.stderr))
                .for_server(&input.server_id),
        );
    }
    Ok(CertificateActionResult {
        domain: input.domain,
        action: input.action,
        challenge: input.challenge,
        dns_provider: input.dns_provider,
        tool: tool.into(),
        certificate_path: cert_path,
        certificate_key_path: key_path,
        output: crate::security::redact(&result.stdout),
    })
}

/// 读取 Nginx/OpenResty 受控目录中的 `site-*.conf` 文件并解析站点摘要。
pub async fn snapshot(ssh: &SshConnectionManager, server_id: &str) -> AppResult<WebsiteSnapshot> {
    let nginx = crate::domain::nginx::snapshot(ssh, server_id).await?;
    let certificate_tools = probe_certificate_tools(ssh, server_id).await?;
    let php_runtimes = probe_php_runtimes(ssh, server_id).await?;
    if !nginx.installed {
        return Ok(WebsiteSnapshot {
            supported: false,
            managed_conf_dir: None,
            nginx_version: nginx.version,
            runtime_root: nginx.container_site_root,
            host_root: nginx.site_host_root,
            websites: Vec::new(),
            php_runtimes,
            certificate_tools,
            warnings: vec!["远端没有 Nginx/OpenResty".into()],
            fetched_at: chrono::Utc::now().to_rfc3339(),
        });
    }
    let Some(directory) = nginx.managed_conf_dir.clone() else {
        return Ok(WebsiteSnapshot {
            supported: false,
            managed_conf_dir: None,
            nginx_version: nginx.version,
            runtime_root: nginx.container_site_root,
            host_root: nginx.site_host_root,
            websites: Vec::new(),
            php_runtimes,
            certificate_tools,
            warnings: vec!["Nginx 配置未包含受控 conf.d include".into()],
            fetched_at: chrono::Utc::now().to_rfc3339(),
        });
    };
    let sftp = ssh.open_sftp(server_id).await?;
    let entries = sftp.read_dir(&directory).await.map_err(|error| {
        AppError::new("SFTP_FAILED", "website", "无法读取网站配置目录")
            .details(error)
            .for_server(server_id)
    })?;
    let mut pending: Vec<(u64, String, WebsiteRecord, Vec<String>)> = Vec::new();
    let mut warnings = Vec::new();
    for entry in entries {
        let path = entry.path();
        let name = entry.file_name();
        if !(name.ends_with(".conf") || name.ends_with(".conf.disabled")) {
            continue;
        }
        let mut file = match sftp.open(&path).await {
            Ok(file) => file,
            Err(error) => {
                warnings.push(format!("无法读取网站配置：{path}（{error}）"));
                continue;
            }
        };
        let mut bytes = Vec::new();
        if let Err(error) = file.read_to_end(&mut bytes).await {
            warnings.push(format!("无法读取网站配置：{path}（{error}）"));
            continue;
        }
        let content = String::from_utf8_lossy(&bytes);
        match parse_website_config(&content, &path, !name.ends_with(".disabled")) {
            Some(website) => {
                // SFTP 连接宿主机，managed_conf_dir 为宿主机路径；网站根目录在其同级
                // sites/{domain} 下。web 面板默认按创建时间降序（最新创建的网站在前），
                // 目录 mtime 可作为创建顺序基准；取不到则退回配置文件 mtime。
                let mut sort_key = u64::from(entry.metadata().mtime.unwrap_or(0));
                if let Some(root) = nginx.site_host_root.as_deref() {
                    let site_dir = format!("{root}/sites/{}", website.domain);
                    if let Ok(meta) = sftp.metadata(&site_dir).await {
                        if let Some(mtime) = meta.mtime {
                            sort_key = u64::from(mtime);
                        }
                    }
                }
                pending.push((sort_key, name, website, included_rule_dirs(&content)));
            }
            None => warnings.push(format!("无法解析网站配置：{path}")),
        }
    }
    let _ = sftp.close().await;
    // 与 web 面板一致：按创建时间降序（最新创建的网站排最前）展示，
    // 同时间戳再按域名升序保证顺序稳定。
    let mut order: Vec<usize> = (0..pending.len()).collect();
    order.sort_by(|&a, &b| {
        pending[b]
            .0
            .cmp(&pending[a].0)
            .then_with(|| pending[a].1.cmp(&pending[b].1))
    });
    let mut websites = Vec::with_capacity(pending.len());
    let mut include_dirs = Vec::new();
    for index in order {
        let (_, _, website, dirs) = &pending[index];
        websites.push(website.clone());
        if !dirs.is_empty() {
            include_dirs.push((websites.len() - 1, dirs.clone()));
        }
    }
    enrich_included_rules(ssh, server_id, &nginx, &mut websites, include_dirs).await?;
    enrich_website_expiry(ssh, server_id, &nginx, &mut websites).await?;
    Ok(WebsiteSnapshot {
        supported: true,
        managed_conf_dir: Some(directory),
        nginx_version: nginx.version,
        runtime_root: nginx.container_site_root,
        host_root: nginx.site_host_root,
        websites,
        php_runtimes,
        certificate_tools,
        warnings,
        fetched_at: chrono::Utc::now().to_rfc3339(),
    })
}

/// 创建一个网站根目录和受控配置，验证配置后 reload，失败自动恢复旧配置。
pub async fn save(
    ssh: &SshConnectionManager,
    input: SaveWebsiteInput,
) -> AppResult<WebsiteSnapshot> {
    validate_save_input(&input)?;
    if !input.confirmed {
        return Err(
            AppError::new("CONFIRMATION_REQUIRED", "website", "网站写入需要显式确认")
                .for_server(&input.server_id),
        );
    }
    let nginx = crate::domain::nginx::snapshot(ssh, &input.server_id).await?;
    ensure_supported(&nginx, &input.server_id)?;
    let managed_dir = nginx.managed_conf_dir.as_deref().unwrap_or_default();
    let config_path = website_config_path(managed_dir, &input.domain)?;
    let runtime_root = website_runtime_root(&nginx, &input)?;
    let host_root = website_host_root(&nginx, &runtime_root)?;
    let php_socket = resolve_php_socket(ssh, &input).await?;
    let content = render_website(&input, &runtime_root, php_socket.as_deref());
    install_website_config(
        ssh,
        &input.server_id,
        &nginx,
        &config_path,
        &host_root,
        &content,
    )
    .await?;
    snapshot(ssh, &input.server_id).await
}

/// 将已签发证书绑定到同域受控站点，复用配置备份、测试、reload 和失败回滚流程。
pub async fn bind_certificate(
    ssh: &SshConnectionManager,
    input: BindWebsiteCertificateInput,
) -> AppResult<WebsiteSnapshot> {
    validate_certificate_binding_input(&input)?;
    if !input.confirmed {
        return Err(
            AppError::new("CONFIRMATION_REQUIRED", "website", "证书绑定需要显式确认")
                .for_server(&input.server_id),
        );
    }
    let nginx = crate::domain::nginx::snapshot(ssh, &input.server_id).await?;
    ensure_supported(&nginx, &input.server_id)?;
    let config_path = website_config_path(
        nginx.managed_conf_dir.as_deref().unwrap_or_default(),
        &input.domain,
    )?;
    let sftp = ssh.open_sftp(&input.server_id).await?;
    let mut file = sftp.open(&config_path).await.map_err(|error| {
        AppError::new("WEBSITE_NOT_FOUND", "website", "找不到同域受控网站配置")
            .details(error)
            .for_server(&input.server_id)
    })?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).await.map_err(|error| {
        AppError::new("SFTP_FAILED", "website", "无法读取同域网站配置")
            .details(error)
            .for_server(&input.server_id)
    })?;
    drop(file);
    let _ = sftp.close().await;
    let source = String::from_utf8(bytes).map_err(|error| {
        AppError::new("WEBSITE_CONFIG_INVALID", "website", "网站配置不是有效文本")
            .details(error)
            .for_server(&input.server_id)
    })?;
    if !source.contains("# Managed by 1Panel Client;") {
        return Err(AppError::new(
            "WEBSITE_NOT_MANAGED",
            "website",
            "只允许自动绑定客户端生成的受控网站配置",
        )
        .for_server(&input.server_id));
    }
    let website = parse_website_config(&source, &config_path, true).ok_or_else(|| {
        AppError::new("WEBSITE_CONFIG_INVALID", "website", "无法解析同域网站配置")
            .for_server(&input.server_id)
    })?;
    let upstream = website
        .upstream
        .as_deref()
        .map(parse_upstream_target)
        .transpose()
        .map_err(|error| error.for_server(&input.server_id))?;
    let bind_input = SaveWebsiteInput {
        server_id: input.server_id.clone(),
        domain: website.domain.clone(),
        kind: match website.kind {
            WebsiteKind::Static => "static".into(),
            WebsiteKind::Proxy => "proxy".into(),
        },
        listen_port: website.listen_port,
        root_path: website.root_path.clone(),
        php_runtime: None,
        php_socket: None,
        upstream_scheme: upstream.as_ref().map(|value| value.0.clone()),
        upstream_host: upstream.as_ref().map(|value| value.1.clone()),
        upstream_port: upstream.as_ref().map(|value| value.2),
        enable_https: true,
        https_port: if website.ssl {
            website.listen_port
        } else {
            443
        },
        certificate_path: Some(input.certificate_path.clone()),
        certificate_key_path: Some(input.certificate_key_path.clone()),
        confirmed: true,
    };
    let runtime_root = website_runtime_root(&nginx, &bind_input)?;
    let host_root = website_host_root(&nginx, &runtime_root)?;
    let content = render_website(&bind_input, &runtime_root, website.php_runtime.as_deref());
    install_website_config(
        ssh,
        &input.server_id,
        &nginx,
        &config_path,
        &host_root,
        &content,
    )
    .await?;
    snapshot(ssh, &input.server_id).await
}

/// 复用 SFTP 临时文件、备份、Nginx 配置测试/reload 和失败恢复流程。
async fn install_website_config(
    ssh: &SshConnectionManager,
    server_id: &str,
    nginx: &NginxSnapshot,
    config_path: &str,
    host_root: &str,
    content: &str,
) -> AppResult<()> {
    let control = nginx_control(nginx);
    let temporary = format!("/tmp/.1panel-client-site-{}.conf", uuid::Uuid::new_v4());
    let sftp = ssh.open_sftp(server_id).await?;
    let mut file = sftp.create(&temporary).await.map_err(|error| {
        AppError::new("SFTP_FAILED", "website", "无法创建网站临时配置")
            .details(error)
            .for_server(server_id)
    })?;
    file.write_all(content.as_bytes()).await.map_err(|error| {
        AppError::new("SFTP_FAILED", "website", "无法写入网站临时配置")
            .details(error)
            .for_server(server_id)
    })?;
    file.flush().await.map_err(|error| {
        AppError::new("SFTP_FAILED", "website", "无法刷新网站临时配置")
            .details(error)
            .for_server(server_id)
    })?;
    file.sync_all().await.map_err(|error| {
        AppError::new("SFTP_FAILED", "website", "无法同步网站临时配置")
            .details(error)
            .for_server(server_id)
    })?;
    drop(file);
    let backup = format!(
        "{config_path}.1panel-client-backup-{}",
        uuid::Uuid::new_v4()
    );
    let command = format!(
        "set -u; target={target}; backup={backup}; temporary={temporary}; root={root}; restore() {{ if [ -f \"$backup\" ]; then cp -a -- \"$backup\" \"$target\"; else rm -f -- \"$target\"; fi; }}; if ! mkdir -p -- \"$root\"; then rm -f -- \"$temporary\"; exit 41; fi; if [ -f \"$target\" ] && ! cp -a -- \"$target\" \"$backup\"; then rm -f -- \"$temporary\"; exit 42; fi; if ! install -m 0644 -- \"$temporary\" \"$target\"; then restore; rm -f -- \"$temporary\"; exit 43; fi; rm -f -- \"$temporary\"; if ! {control} -t; then restore; {control} -t >/dev/null 2>&1 || true; exit 44; fi; if ! {control} -s reload; then restore; {control} -t >/dev/null 2>&1 || true; exit 45; fi",
        target = crate::security::shell_escape(config_path),
        backup = crate::security::shell_escape(&backup),
        temporary = crate::security::shell_escape(&temporary),
        root = crate::security::shell_escape(host_root),
        control = control,
    );
    let result = ssh
        .execute_system(server_id, &command, Duration::from_secs(60))
        .await?;
    let _ = sftp.close().await;
    if result.exit_code != 0 {
        return Err(AppError::new(
            "WEBSITE_CONFIG_INVALID",
            "website",
            "网站配置检查或 reload 失败，已恢复备份",
        )
        .details(result.stderr)
        .for_server(server_id));
    }
    Ok(())
}

/// 启用、停用或删除由客户端生成的站点配置，并重新验证 Nginx/OpenResty。
pub async fn action(
    ssh: &SshConnectionManager,
    input: WebsiteActionInput,
) -> AppResult<WebsiteSnapshot> {
    validate_action_input(&input)?;
    if !input.confirmed {
        return Err(AppError::new(
            "CONFIRMATION_REQUIRED",
            "website",
            "网站生命周期操作需要显式确认",
        )
        .for_server(&input.server_id));
    }
    let nginx = crate::domain::nginx::snapshot(ssh, &input.server_id).await?;
    ensure_supported(&nginx, &input.server_id)?;
    let active_path = website_config_path(
        nginx.managed_conf_dir.as_deref().unwrap_or_default(),
        &input.domain,
    )?;
    let disabled_path = format!("{active_path}.disabled");
    // 旧版客户端生成的配置是 site-{slug}.conf，一并处理保证历史站点可操作。
    let legacy_path = format!(
        "{}/site-{}.conf",
        nginx.managed_conf_dir.as_deref().unwrap_or_default(),
        domain_slug(&input.domain)
    );
    let legacy_disabled = format!("{legacy_path}.disabled");
    let control = nginx_control(&nginx);
    let command = match input.action.as_str() {
        "enable" => format!(
            "test -f {disabled} && mv -- {disabled} {active}; test -f {legacy_disabled} && mv -- {legacy_disabled} {legacy}; {control} -t && {control} -s reload",
            disabled = crate::security::shell_escape(&disabled_path),
            active = crate::security::shell_escape(&active_path),
            legacy_disabled = crate::security::shell_escape(&legacy_disabled),
            legacy = crate::security::shell_escape(&legacy_path),
            control = control,
        ),
        "disable" => format!(
            "test -f {active} && mv -- {active} {disabled}; test -f {legacy} && mv -- {legacy} {legacy_disabled}; {control} -t && {control} -s reload",
            active = crate::security::shell_escape(&active_path),
            disabled = crate::security::shell_escape(&disabled_path),
            legacy = crate::security::shell_escape(&legacy_path),
            legacy_disabled = crate::security::shell_escape(&legacy_disabled),
            control = control,
        ),
        "delete" => format!(
            "rm -f -- {active} {disabled} {legacy} {legacy_disabled}; {control} -t && {control} -s reload",
            active = crate::security::shell_escape(&active_path),
            disabled = crate::security::shell_escape(&disabled_path),
            legacy = crate::security::shell_escape(&legacy_path),
            legacy_disabled = crate::security::shell_escape(&legacy_disabled),
            control = control,
        ),
        _ => unreachable!(),
    };
    let result = ssh
        .execute_system(&input.server_id, &command, Duration::from_secs(60))
        .await?;
    if result.exit_code != 0 {
        return Err(
            AppError::new("WEBSITE_ACTION_FAILED", "website", "网站生命周期操作失败")
                .details(result.stderr)
                .for_server(&input.server_id),
        );
    }
    snapshot(ssh, &input.server_id).await
}

/// 停止、启动、重启或重载 OpenResty（含容器场景），完成后重新探测状态。
pub async fn nginx_service(
    ssh: &SshConnectionManager,
    input: NginxServiceInput,
) -> AppResult<crate::domain::nginx::NginxSnapshot> {
    if input.server_id.trim().is_empty() || input.action.trim().is_empty() {
        return Err(AppError::new(
            "INVALID_INPUT",
            "website",
            "缺少服务操作参数",
        ));
    }
    if !matches!(
        input.action.as_str(),
        "stop" | "start" | "restart" | "reload"
    ) {
        return Err(AppError::new(
            "INVALID_INPUT",
            "website",
            "不支持的服务操作",
        ));
    }
    if !input.confirmed {
        return Err(AppError::new(
            "CONFIRMATION_REQUIRED",
            "website",
            "OpenResty 服务操作需要显式确认",
        )
        .for_server(&input.server_id));
    }
    let nginx = crate::domain::nginx::snapshot(ssh, &input.server_id).await?;
    if !nginx.installed {
        return Err(AppError::new(
            "NGINX_NOT_INSTALLED",
            "website",
            "远端未安装 Nginx/OpenResty",
        )
        .for_server(&input.server_id));
    }
    let command = match nginx.container_id.as_deref() {
        Some(container) => {
            let id = crate::security::shell_escape(container);
            match input.action.as_str() {
                "stop" => format!("docker stop {id}"),
                "start" => format!("docker start {id}"),
                "restart" => format!("docker restart {id}"),
                "reload" => format!("docker exec {id} openresty -s reload"),
                _ => unreachable!(),
            }
        }
        None => {
            let flavor = crate::security::shell_escape(if nginx.flavor == "openresty" {
                "openresty"
            } else {
                "nginx"
            });
            let binary = crate::security::shell_escape(nginx.binary.as_deref().unwrap_or("nginx"));
            match input.action.as_str() {
                "stop" => format!("systemctl stop {flavor} 2>/dev/null || {binary} -s quit"),
                "start" => format!("systemctl start {flavor} 2>/dev/null || {binary}"),
                "restart" => {
                    format!("systemctl restart {flavor} 2>/dev/null || (test -f /run/nginx.pid && {binary} -s quit; {binary})")
                }
                "reload" => format!("{binary} -s reload"),
                _ => unreachable!(),
            }
        }
    };
    let result = ssh
        .execute_system(&input.server_id, &command, Duration::from_secs(90))
        .await?;
    if result.exit_code != 0 {
        return Err(
            AppError::new("NGINX_SERVICE_FAILED", "website", "OpenResty 服务操作失败")
                .details(result.stderr)
                .for_server(&input.server_id),
        );
    }
    crate::domain::nginx::snapshot(ssh, &input.server_id).await
}

/// 确认 Nginx/OpenResty 已安装并拥有可写的受控 conf.d 目录。
fn ensure_supported(nginx: &NginxSnapshot, server_id: &str) -> AppResult<()> {
    if !nginx.installed {
        return Err(AppError::new(
            "NGINX_NOT_INSTALLED",
            "website",
            "远端未安装 Nginx/OpenResty",
        )
        .for_server(server_id));
    }
    if !nginx.managed_conf_supported || nginx.managed_conf_dir.is_none() {
        return Err(AppError::new(
            "NGINX_INCLUDE_UNSUPPORTED",
            "website",
            "Nginx 配置未包含受控 conf.d include",
        )
        .for_server(server_id));
    }
    Ok(())
}

/// 构造固定控制命令；容器场景在容器内执行 openresty，宿主机场景执行探测到的二进制。
fn nginx_control(nginx: &NginxSnapshot) -> String {
    nginx
        .container_id
        .as_deref()
        .map(|id| {
            format!(
                "docker exec {} openresty",
                crate::security::shell_escape(id)
            )
        })
        .unwrap_or_else(|| {
            crate::security::shell_escape(nginx.binary.as_deref().unwrap_or("nginx"))
        })
}

/// 计算受控网站配置文件路径，只允许由域名派生的安全文件名。
/// web 面板使用 `{domain}.conf` 命名；旧版本客户端曾用 `site-{slug}.conf`，
/// action 命令中对旧命名也做兼容。
fn website_config_path(directory: &str, domain: &str) -> AppResult<String> {
    if directory.is_empty()
        || !directory.starts_with('/')
        || directory.contains("..")
        || directory.contains('\n')
        || directory.contains('\r')
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "validation",
            "网站配置目录无效",
        ));
    }
    if !valid_domain(domain) {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "validation",
            "网站域名无效",
        ));
    }
    Ok(format!("{directory}/{domain}.conf"))
}

/// 为静态网站选择容器内运行时路径；宿主机 Nginx 默认使用 /var/www/sites。
fn website_runtime_root(nginx: &NginxSnapshot, input: &SaveWebsiteInput) -> AppResult<String> {
    if let Some(path) = input.root_path.as_deref() {
        validate_root_path(path)?;
        return Ok(path.to_string());
    }
    let base = nginx.container_site_root.as_deref().unwrap_or("/var/www");
    Ok(format!("{base}/sites/{}", input.domain))
}

/// 将运行时网站路径映射到 SFTP 可写的宿主机路径；没有映射时直接使用运行时路径。
fn website_host_root(nginx: &NginxSnapshot, runtime_root: &str) -> AppResult<String> {
    if let (Some(container_root), Some(host_root)) = (
        nginx.container_site_root.as_deref(),
        nginx.site_host_root.as_deref(),
    ) {
        if runtime_root == container_root {
            return Ok(host_root.to_string());
        }
        if let Some(relative) = runtime_root.strip_prefix(&format!("{container_root}/")) {
            if !relative.is_empty() {
                return Ok(format!("{host_root}/{relative}"));
            }
        }
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "website",
            "网站根目录不在容器已挂载的网站目录内",
        ));
    }
    Ok(runtime_root.to_string())
}

/// 将用户选择的 PHP-FPM runtime id 解析为已探测的 Unix socket，并拒绝非 FPM 运行时。
async fn resolve_php_socket(
    ssh: &SshConnectionManager,
    input: &SaveWebsiteInput,
) -> AppResult<Option<String>> {
    if let Some(socket) = override_php_socket(input)? {
        return Ok(Some(socket));
    }
    let Some(runtime_id) = input.php_runtime.as_deref() else {
        return Ok(None);
    };
    if input.kind != "static" {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "website",
            "PHP-FPM 运行时只能绑定到静态站点",
        )
        .for_server(&input.server_id));
    }
    let runtimes = probe_php_runtimes(ssh, &input.server_id).await?;
    let runtime = runtimes
        .iter()
        .find(|value| value.id == runtime_id && value.service.is_some())
        .ok_or_else(|| {
            AppError::new(
                "PHP_RUNTIME_NOT_FOUND",
                "website",
                "所选 PHP-FPM 运行时不存在或不是 FPM 服务",
            )
            .for_server(&input.server_id)
        })?;
    let socket = runtime
        .socket_path
        .clone()
        .or_else(|| {
            runtime
                .service
                .as_ref()
                .map(|service| format!("/run/php/{service}.sock"))
        })
        .ok_or_else(|| {
            AppError::new(
                "PHP_SOCKET_NOT_FOUND",
                "website",
                "无法确定所选 PHP-FPM 的 Unix socket",
            )
            .for_server(&input.server_id)
        })?;
    validate_php_socket_path(&socket).map_err(|error| error.for_server(&input.server_id))?;
    Ok(Some(socket))
}

/// 处理显式指定的容器内可见 FastCGI socket；仅允许静态站点并校验路径安全。
fn override_php_socket(input: &SaveWebsiteInput) -> AppResult<Option<String>> {
    let Some(socket) = input.php_socket.as_deref() else {
        return Ok(None);
    };
    if input.kind != "static" {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "website",
            "PHP-FPM socket 只能绑定到静态站点",
        )
        .for_server(&input.server_id));
    }
    validate_php_socket_path(socket).map_err(|error| error.for_server(&input.server_id))?;
    Ok(Some(socket.to_string()))
}

/// 渲染静态或反向代理 server block；用户输入已在调用前完成验证。
fn render_website(
    input: &SaveWebsiteInput,
    runtime_root: &str,
    php_socket: Option<&str>,
) -> String {
    let server_name = input.domain.as_str();
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
    let body = if input.kind == "static" {
        let php = php_socket
            .map(|socket| format!(
                "    location ~ \\.php$ {{\n        try_files $uri =404;\n        include fastcgi_params;\n        fastcgi_param SCRIPT_FILENAME $document_root$fastcgi_script_name;\n        fastcgi_pass unix:{socket};\n    }}\n"
            ))
            .unwrap_or_default();
        format!(
            "    root {runtime_root};\n    index index.html index.htm index.php;\n    location / {{\n        try_files $uri $uri/ =404;\n    }}\n{php}"
        )
    } else {
        let scheme = input.upstream_scheme.as_deref().unwrap_or("http");
        let host = input.upstream_host.as_deref().unwrap_or("127.0.0.1");
        let port = input.upstream_port.unwrap_or(8080);
        format!(
            "    location / {{\n        proxy_pass {scheme}://{host}:{port};\n        proxy_set_header Host $host;\n        proxy_set_header X-Real-IP $remote_addr;\n        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;\n    }}\n"
        )
    };
    format!(
        "# Managed by 1Panel Client; edit through the desktop client.\nserver {{\n    listen {};\n{}    server_name {};\n{} }}\n",
        input.listen_port, tls, server_name, body
    )
}

/// 读取容器或宿主机证书到期时间，并更新站点摘要；只读取证书公钥文件。
async fn enrich_website_expiry(
    ssh: &SshConnectionManager,
    server_id: &str,
    nginx: &NginxSnapshot,
    websites: &mut [WebsiteRecord],
) -> AppResult<()> {
    for website in websites.iter_mut() {
        let Some(path) = website.certificate_path.as_deref() else {
            continue;
        };
        if let Some(metadata) = nginx
            .certificates
            .iter()
            .find(|item| item.certificate_path == path)
        {
            website.expires_at = metadata.expires_at.clone();
            continue;
        }
        let command = nginx
            .container_id
            .as_deref()
            .map(|id| {
                format!(
                    "docker exec {} openssl x509 -in {} -noout -enddate 2>/dev/null",
                    crate::security::shell_escape(id),
                    crate::security::shell_escape(path)
                )
            })
            .unwrap_or_else(|| {
                format!(
                    "openssl x509 -in {} -noout -enddate 2>/dev/null",
                    crate::security::shell_escape(path)
                )
            });
        let result = ssh
            .execute(server_id, &command, Duration::from_secs(10))
            .await?;
        if result.exit_code == 0 {
            website.expires_at = result
                .stdout
                .lines()
                .find_map(|line| line.trim().strip_prefix("notAfter="))
                .map(str::to_string);
        }
    }
    Ok(())
}

/// 提取主配置 include 的规则目录（1Panel 将反向代理/SSL 规则拆分到
/// /www/sites/{domain}/proxy|ssl/*.conf，主 server block 仅保留 include 引用）。
fn included_rule_dirs(input: &str) -> Vec<String> {
    let mut dirs = Vec::new();
    for raw in input.lines() {
        let line = raw.trim().trim_end_matches(';');
        let fields: Vec<_> = line.split_whitespace().collect();
        if let ["include", value, ..] = fields.as_slice() {
            if let Some(dir) = value.strip_suffix("/*.conf") {
                dirs.push((*dir).to_string());
            }
        }
    }
    dirs
}

/// 补全 include 的规则文件信息：proxy 文件中的 proxy_pass 决定站点类型，
/// ssl 文件中的 ssl_certificate 补齐证书路径，避免代理站点被误判为静态网站。
async fn enrich_included_rules(
    ssh: &SshConnectionManager,
    server_id: &str,
    nginx: &NginxSnapshot,
    websites: &mut [WebsiteRecord],
    include_dirs: Vec<(usize, Vec<String>)>,
) -> AppResult<()> {
    for (index, dirs) in include_dirs {
        let Some(website) = websites.get_mut(index) else {
            continue;
        };
        let sftp = ssh.open_sftp(server_id).await?;
        for dir in dirs {
            // 容器版 OpenResty 的 include 目标（如 /www/sites/...）对宿主机 SFTP
            // 不可见，需按挂载映射换算成宿主机路径；失败时回退原路径再试一次。
            let candidates: Vec<String> = {
                let container_root = nginx.container_site_root.as_deref().unwrap_or_default();
                let host_root = nginx.site_host_root.as_deref().unwrap_or_default();
                if !container_root.is_empty()
                    && !host_root.is_empty()
                    && dir.starts_with(container_root)
                {
                    vec![
                        format!("{host_root}{}", &dir[container_root.len()..]),
                        dir.clone(),
                    ]
                } else {
                    vec![dir.clone()]
                }
            };
            let mut rules = Vec::new();
            for candidate in &candidates {
                let Ok(entries) = sftp.read_dir(candidate).await else {
                    continue;
                };
                for entry in entries {
                    let path = entry.path();
                    let entry_name = path.rsplit('/').next().unwrap_or(path.as_str());
                    if !entry_name.ends_with(".conf") {
                        continue;
                    }
                    let Ok(mut file) = sftp.open(&path).await else {
                        continue;
                    };
                    let mut bytes = Vec::new();
                    if file.read_to_end(&mut bytes).await.is_err() {
                        continue;
                    }
                    rules.push(String::from_utf8_lossy(&bytes).into_owned());
                }
                if !rules.is_empty() {
                    break;
                }
            }
            for text in rules {
                for raw in text.lines() {
                    let line = raw.trim().trim_end_matches(';');
                    let fields: Vec<_> = line.split_whitespace().collect();
                    match fields.as_slice() {
                        ["proxy_pass", value, ..] => {
                            website.kind = WebsiteKind::Proxy;
                            website.upstream = Some((*value).to_string());
                        }
                        ["ssl_certificate", value, ..] => {
                            website.ssl = true;
                            website.certificate_path = Some((*value).to_string());
                        }
                        _ => {}
                    }
                }
            }
        }
        let _ = sftp.close().await;
    }
    Ok(())
}

/// 解析受控 server block 的最小字段，兼容启用和 `.disabled` 状态。
fn parse_website_config(input: &str, config_path: &str, enabled: bool) -> Option<WebsiteRecord> {
    let mut domain = None;
    let mut kind = WebsiteKind::Static;
    let mut listen_port = 80;
    let mut root_path = None;
    let mut upstream = None;
    let mut php_runtime = None;
    let mut ssl = false;
    let mut certificate_path = None;
    for raw in input.lines() {
        let line = raw.trim().trim_end_matches(';');
        let fields: Vec<_> = line.split_whitespace().collect();
        match fields.as_slice() {
            ["server_name", value, ..] => domain = Some((*value).to_string()),
            ["listen", value, rest @ ..] => {
                listen_port = value.parse().unwrap_or(80);
                // Nginx commonly renders this as `listen 443 ssl;`; the
                // optional flags must not make the port parser fall back to 80.
                ssl |= value == &"443" || rest.contains(&"ssl");
            }
            ["root", value, ..] => root_path = Some((*value).to_string()),
            ["proxy_pass", value, ..] => {
                kind = WebsiteKind::Proxy;
                upstream = Some((*value).to_string());
            }
            ["fastcgi_pass", value, ..] => {
                php_runtime = Some(value.strip_prefix("unix:").unwrap_or(value).to_string());
            }
            ["ssl_certificate", value, ..] => {
                ssl = true;
                certificate_path = Some((*value).to_string());
            }
            _ => {}
        }
    }
    Some(WebsiteRecord {
        domain: domain?,
        kind,
        enabled,
        listen_port,
        root_path,
        upstream,
        php_runtime,
        ssl,
        certificate_path,
        expires_at: None,
        config_path: config_path.to_string(),
    })
}

/// 解析客户端生成的 proxy_pass 地址，绑定证书时保留原有上游而不信任任意配置文本。
fn parse_upstream_target(value: &str) -> AppResult<(String, String, u16)> {
    let parsed = reqwest::Url::parse(value).map_err(|_| {
        AppError::new(
            "WEBSITE_CONFIG_INVALID",
            "website",
            "反向代理上游地址无法解析",
        )
    })?;
    let scheme = parsed.scheme();
    if !matches!(scheme, "http" | "https") || parsed.path() != "/" || parsed.query().is_some() {
        return Err(AppError::new(
            "WEBSITE_CONFIG_INVALID",
            "website",
            "反向代理上游地址不是客户端支持的简单 URL",
        ));
    }
    let host = parsed
        .host_str()
        .filter(|value| valid_upstream_host(value))
        .ok_or_else(|| {
            AppError::new("WEBSITE_CONFIG_INVALID", "website", "反向代理上游主机无效")
        })?;
    let port = parsed.port_or_known_default().ok_or_else(|| {
        AppError::new("WEBSITE_CONFIG_INVALID", "website", "反向代理上游端口无效")
    })?;
    Ok((scheme.to_string(), host.to_string(), port))
}

/// 校验站点输入，阻止路径穿越、控制字符和未经验证的 proxy 上游。
fn validate_save_input(input: &SaveWebsiteInput) -> AppResult<()> {
    if !valid_domain(&input.domain)
        || !matches!(input.kind.as_str(), "static" | "proxy")
        || input.listen_port == 0
        || (input.enable_https && input.https_port == 0)
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "validation",
            "网站域名、类型或监听端口无效",
        )
        .for_server(&input.server_id));
    }
    if input.kind == "static" {
        if let Some(path) = input.root_path.as_deref() {
            validate_root_path(path)?;
        }
        if let Some(runtime) = input.php_runtime.as_deref() {
            validate_php_runtime_id(runtime)?;
        }
        if let Some(socket) = input.php_socket.as_deref() {
            validate_php_socket_path(socket)?;
        }
    } else if input.php_runtime.is_some() || input.php_socket.is_some() {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "website",
            "PHP-FPM 运行时只能绑定到静态站点",
        )
        .for_server(&input.server_id));
    } else if !matches!(
        input.upstream_scheme.as_deref(),
        Some("http") | Some("https")
    ) || input
        .upstream_host
        .as_deref()
        .unwrap_or_default()
        .is_empty()
        || input.upstream_port.is_none_or(|port| port == 0)
        || !valid_upstream_host(input.upstream_host.as_deref().unwrap_or_default())
    {
        return Err(
            AppError::new("VALIDATION_FAILED", "validation", "反向代理上游地址无效")
                .for_server(&input.server_id),
        );
    }
    if input.enable_https
        && (input
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
            "启用 HTTPS 时必须填写证书和私钥路径",
        )
        .for_server(&input.server_id));
    }
    for value in [
        input.root_path.as_deref().unwrap_or_default(),
        input.upstream_host.as_deref().unwrap_or_default(),
    ] {
        if value.chars().any(|character| {
            character == '\0'
                || character == '\n'
                || character == '\r'
                || character == ';'
                || character == '`'
                || character.is_whitespace()
        }) {
            return Err(AppError::new(
                "VALIDATION_FAILED",
                "validation",
                "网站字段包含非法控制字符",
            )
            .for_server(&input.server_id));
        }
    }
    if input.enable_https {
        validate_certificate_path(input.certificate_path.as_deref().unwrap_or_default())?;
        validate_certificate_path(input.certificate_key_path.as_deref().unwrap_or_default())?;
    }
    Ok(())
}

/// 校验网站动作和域名，动作集合保持在受控文件名范围内。
fn validate_action_input(input: &WebsiteActionInput) -> AppResult<()> {
    if !matches!(input.action.as_str(), "enable" | "disable" | "delete")
        || !valid_domain(&input.domain)
    {
        return Err(
            AppError::new("VALIDATION_FAILED", "validation", "网站动作或域名无效")
                .for_server(&input.server_id),
        );
    }
    Ok(())
}

/// 仅允许普通 DNS 域名、通配符前缀和 localhost，避免路径或 shell 注入。
fn valid_domain(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 253
        && !value.starts_with('.')
        && !value.ends_with('.')
        && value.split('.').all(|part| {
            !part.is_empty()
                && part.len() <= 63
                && (part == "*"
                    || part
                        .chars()
                        .all(|character| character.is_ascii_alphanumeric() || character == '-'))
                && !part.starts_with('-')
                && !part.ends_with('-')
        })
}

/// 将通配符域名映射为安全的配置文件名，避免 `*` 被 shell 或文件系统特殊处理。
fn domain_slug(domain: &str) -> String {
    domain.replace('*', "wildcard").replace('.', "_")
}

/// 校验站点根目录必须是绝对路径且不能穿越父目录。
fn validate_root_path(value: &str) -> AppResult<()> {
    if !value.starts_with('/')
        || value.contains("..")
        || value.contains('\n')
        || value.contains('\r')
        || value.contains('\0')
        || value.contains(';')
        || value.contains('`')
        || value.chars().any(char::is_whitespace)
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "validation",
            "网站根目录必须是安全的绝对路径",
        ));
    }
    Ok(())
}

/// 校验 PHP runtime id，只允许来自远端探测的简单服务标识。
fn validate_php_runtime_id(value: &str) -> AppResult<()> {
    if value.is_empty()
        || value.len() > 128
        || value.chars().any(|character| {
            !character.is_ascii_alphanumeric() && !matches!(character, '.' | '-' | '_')
        })
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "validation",
            "PHP-FPM 运行时标识无效",
        ));
    }
    Ok(())
}

/// 校验 FastCGI socket 路径，防止把 shell 控制字符写入 Nginx 配置。
fn validate_php_socket_path(value: &str) -> AppResult<()> {
    if !value.starts_with('/')
        || value.contains("..")
        || value.contains('\n')
        || value.contains('\r')
        || value.contains('\0')
        || value.contains(';')
        || value.contains('`')
        || value.chars().any(char::is_whitespace)
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "validation",
            "PHP-FPM socket 路径无效",
        ));
    }
    Ok(())
}

/// 校验证书和私钥路径为绝对路径，拒绝路径穿越与 shell 控制字符。
fn validate_certificate_path(value: &str) -> AppResult<()> {
    if !value.starts_with('/')
        || value.contains("..")
        || value.contains('\n')
        || value.contains('\r')
        || value.contains('\0')
        || value.contains(';')
        || value.contains('`')
        || value.chars().any(char::is_whitespace)
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "validation",
            "证书路径必须是安全的绝对路径",
        ));
    }
    Ok(())
}

/// 解析 certbot/acme.sh 探测 marker，缺失 marker 默认视为不可用。
fn parse_certificate_tools(output: &str) -> CertificateTools {
    CertificateTools {
        certbot: output.lines().any(|line| line.trim() == "__CERTBOT__yes"),
        acme_sh: output.lines().any(|line| line.trim() == "__ACME_SH__yes"),
    }
}

/// 按证书到期情况生成批量续期/签发规划；只读取已有快照，不触发远端写入。
pub fn certificate_renewal_plan(
    snapshot: &WebsiteSnapshot,
    renew_before_days: u32,
) -> Vec<CertificateRenewalPlan> {
    let mut plans = Vec::new();
    for website in &snapshot.websites {
        if !website.enabled || !website.ssl {
            continue;
        }
        if website.certificate_path.is_none() || website.expires_at.is_none() {
            plans.push(CertificateRenewalPlan {
                domain: website.domain.clone(),
                action: "issue".into(),
                reason: "missing".into(),
                expires_at: None,
                certificate_path: website.certificate_path.clone(),
                renew_before_days,
            });
            continue;
        }
        let days = certificate_expiry_days(website.expires_at.as_deref().unwrap_or_default());
        if days.is_some_and(|remaining| remaining <= renew_before_days as i64) {
            plans.push(CertificateRenewalPlan {
                domain: website.domain.clone(),
                action: "renew".into(),
                reason: "expiring".into(),
                expires_at: website.expires_at.clone(),
                certificate_path: website.certificate_path.clone(),
                renew_before_days,
            });
        }
    }
    plans
}

/// 计算证书 notAfter 值相对当前时间的剩余天数，无法解析时返回 None。
fn certificate_expiry_days(value: &str) -> Option<i64> {
    let date_value = value.strip_suffix(" GMT").unwrap_or(value);
    let parsed = NaiveDateTime::parse_from_str(date_value, "%b %e %H:%M:%S %Y")
        .or_else(|_| NaiveDateTime::parse_from_str(date_value, "%b %d %H:%M:%S %Y"))
        .ok()?;
    let parsed = DateTime::<Utc>::from_naive_utc_and_offset(parsed, Utc);
    Some((parsed - Utc::now()).num_days())
}

/// 校验证书操作的域名、邮箱、webroot 和动作，阻止 shell 注入与路径穿越。
fn validate_certificate_action(input: &CertificateActionInput) -> AppResult<()> {
    if !matches!(input.action.as_str(), "issue" | "renew")
        || !matches!(input.challenge.as_str(), "http01" | "dns01")
        || !valid_domain(&input.domain)
        || !valid_email(&input.email)
    {
        return Err(
            AppError::new("VALIDATION_FAILED", "website", "证书域名、邮箱或动作无效")
                .for_server(&input.server_id),
        );
    }
    if input.challenge == "http01" {
        return validate_root_path(&input.webroot)
            .map_err(|error| error.for_server(&input.server_id));
    }
    let provider = input.dns_provider.as_deref().unwrap_or_default();
    let valid_token = input
        .dns_api_token
        .as_ref()
        .is_some_and(|token| match provider {
            "cloudflare" => valid_dns_token(token.expose_secret()),
            "aliyun" => valid_aliyun_dns_token(token.expose_secret()),
            "dnspod" => valid_dnspod_dns_token(token.expose_secret()),
            "tencent" => valid_tencent_dns_token(token.expose_secret()),
            "aws" => valid_aws_dns_token(token.expose_secret()),
            _ => false,
        });
    if !matches!(
        provider,
        "cloudflare" | "aliyun" | "dnspod" | "tencent" | "aws"
    ) || !valid_token
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "website",
            "DNS-01 需要受支持的 provider，以及有效 API token",
        )
        .for_server(&input.server_id));
    }
    Ok(())
}

/// 校验证书自动绑定请求，限制为绝对证书路径和合法域名。
fn validate_certificate_binding_input(input: &BindWebsiteCertificateInput) -> AppResult<()> {
    if input.server_id.trim().is_empty() || !valid_domain(&input.domain) {
        return Err(
            AppError::new("VALIDATION_FAILED", "website", "证书绑定服务器或域名无效")
                .for_server(&input.server_id),
        );
    }
    validate_certificate_path(&input.certificate_path)
        .map_err(|error| error.for_server(&input.server_id))?;
    validate_certificate_path(&input.certificate_key_path)
        .map_err(|error| error.for_server(&input.server_id))?;
    Ok(())
}

/// 校验 DNS API token 的大小和控制字符；具体权限由远端 DNS 服务端验证。
fn valid_dns_token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 512
        && !value
            .chars()
            .any(|character| character == '\0' || character == '\n' || character == '\r')
}

/// 校验阿里云 DNS-01 所需的 AccessKeyId:AccessKeySecret 组合，不记录具体密钥内容。
fn valid_aliyun_dns_token(value: &str) -> bool {
    let Some((key, secret)) = value.split_once(':') else {
        return false;
    };
    valid_dns_token(key) && valid_dns_token(secret) && key.len() <= 128 && secret.len() <= 512
}

/// 校验 DNSPod DNS-01 所需的 ID:Token 组合，不记录具体密钥内容。
fn valid_dnspod_dns_token(value: &str) -> bool {
    let Some((id, token)) = value.split_once(':') else {
        return false;
    };
    valid_dns_token(id) && valid_dns_token(token) && id.len() <= 128 && token.len() <= 512
}

/// 校验腾讯云 DNS-01 所需的 SecretId:SecretKey 组合，不记录具体密钥内容。
fn valid_tencent_dns_token(value: &str) -> bool {
    let Some((id, key)) = value.split_once(':') else {
        return false;
    };
    valid_dns_token(id) && valid_dns_token(key) && id.len() <= 128 && key.len() <= 512
}

/// 校验 AWS Route 53 DNS-01 所需的 AccessKeyId:SecretAccessKey 组合。
fn valid_aws_dns_token(value: &str) -> bool {
    let Some((id, key)) = value.split_once(':') else {
        return false;
    };
    valid_dns_token(id) && valid_dns_token(key) && id.len() <= 128 && key.len() <= 512
}

/// 校验 ACME 联系邮箱为单一地址，并拒绝 shell 元字符以避免命令注入。
fn valid_email(value: &str) -> bool {
    let Some((local, domain)) = value.split_once('@') else {
        return false;
    };
    value.len() <= 254
        && !local.is_empty()
        && !domain.is_empty()
        && local.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '%' | '+' | '-')
        })
        && domain
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '.' | '-'))
}

/// 校验 proxy_pass 主机只包含地址、域名和 IPv6 所需字符。
fn valid_upstream_host(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 253
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | ':' | '[' | ']')
        })
}

#[cfg(test)]
mod tests {
    use super::{
        acme_install_command, certificate_renewal_plan, override_php_socket,
        parse_certificate_tools, parse_php_runtimes, parse_upstream_target, parse_website_config,
        render_website, valid_aliyun_dns_token, valid_aws_dns_token, valid_dnspod_dns_token,
        valid_domain, valid_email, valid_tencent_dns_token, validate_certificate_path,
        validate_php_socket_path, BindWebsiteCertificateInput, CertificateActionInput,
        CertificateTools, SaveWebsiteInput, WebsiteKind, WebsiteRecord, WebsiteSnapshot,
    };
    use secrecy::SecretString;

    #[test]
    fn parses_static_and_proxy_configs() {
        let static_site = parse_website_config("server {\n listen 80;\n server_name demo.example.com;\n root /www/sites/demo.example.com;\n fastcgi_pass unix:/run/php/php8.3-fpm.sock;\n}", "/etc/nginx/conf.d/site-demo.example.com.conf", true).unwrap();
        assert_eq!(static_site.kind, WebsiteKind::Static);
        assert_eq!(
            static_site.root_path.as_deref(),
            Some("/www/sites/demo.example.com")
        );
        assert_eq!(
            static_site.php_runtime.as_deref(),
            Some("/run/php/php8.3-fpm.sock")
        );
        let proxy_site = parse_website_config("server {\n listen 443 ssl;\n server_name api.example.com;\n proxy_pass http://127.0.0.1:8080;\n}", "/etc/nginx/conf.d/site-api.example.com.conf", true).unwrap();
        assert_eq!(proxy_site.kind, WebsiteKind::Proxy);
        assert_eq!(proxy_site.listen_port, 443);
        assert!(proxy_site.ssl);
    }

    #[test]
    fn validates_domain_shape() {
        assert!(valid_domain("demo.example.com"));
        assert!(valid_domain("*.example.com"));
        assert!(!valid_domain("../etc/passwd"));
        assert!(!valid_domain("demo.example.com;rm"));
    }

    #[test]
    fn validates_certificate_paths() {
        assert!(validate_certificate_path("/etc/letsencrypt/live/demo/fullchain.pem").is_ok());
        assert!(validate_certificate_path("../secret.pem").is_err());
        assert!(validate_certificate_path("/tmp/key.pem;rm").is_err());
    }

    #[test]
    fn preserves_supported_proxy_target_for_certificate_binding() {
        let target = parse_upstream_target("http://127.0.0.1:8080").unwrap();
        assert_eq!(target, ("http".into(), "127.0.0.1".into(), 8080));
        assert!(parse_upstream_target("https://api.example.com:8443").is_ok());
        assert!(parse_upstream_target("http://api.example.com/v1").is_err());
    }

    #[test]
    fn validates_certificate_binding_request() {
        let input = BindWebsiteCertificateInput {
            server_id: "server".into(),
            domain: "demo.example.com".into(),
            certificate_path: "/etc/letsencrypt/live/demo/fullchain.pem".into(),
            certificate_key_path: "/etc/letsencrypt/live/demo/privkey.pem".into(),
            confirmed: true,
        };
        assert!(super::validate_certificate_binding_input(&input).is_ok());
        assert!(
            super::validate_certificate_binding_input(&BindWebsiteCertificateInput {
                certificate_path: "/tmp/key;rm".into(),
                ..input
            })
            .is_err()
        );
    }

    #[test]
    fn parses_certificate_tool_markers() {
        let tools = parse_certificate_tools("__CERTBOT__yes\n__ACME_SH__no\n");
        assert!(tools.certbot);
        assert!(!tools.acme_sh);
    }

    #[test]
    fn validates_certificate_contact_email() {
        assert!(valid_email("ops@example.com"));
        assert!(!valid_email("ops@example.com;rm"));
        assert!(!valid_email("ops example.com"));
        let input = CertificateActionInput {
            server_id: "server".into(),
            domain: "demo.example.com".into(),
            email: "ops@example.com".into(),
            webroot: "/www/sites/demo.example.com".into(),
            action: "issue".into(),
            challenge: "http01".into(),
            dns_provider: None,
            dns_api_token: None,
            confirmed: true,
        };
        assert!(super::validate_certificate_action(&input).is_ok());
    }

    #[test]
    fn validates_cloudflare_dns_challenge_without_exposing_token() {
        let input = CertificateActionInput {
            server_id: "server".into(),
            domain: "demo.example.com".into(),
            email: "ops@example.com".into(),
            webroot: "".into(),
            action: "issue".into(),
            challenge: "dns01".into(),
            dns_provider: Some("cloudflare".into()),
            dns_api_token: Some(SecretString::from("token-value")),
            confirmed: true,
        };
        assert!(super::validate_certificate_action(&input).is_ok());
        assert!(!super::valid_dns_token("token\nvalue"));
    }

    #[test]
    fn validates_aliyun_dns_credentials_and_acme_install_paths() {
        assert!(valid_aliyun_dns_token("LTAIkey:secret-value"));
        assert!(!valid_aliyun_dns_token("missing-separator"));
        assert!(!valid_aliyun_dns_token("key:secret\nvalue"));
        let command = acme_install_command(
            "demo.example.com",
            "/etc/letsencrypt/live/demo.example.com/fullchain.pem",
            "/etc/letsencrypt/live/demo.example.com/privkey.pem",
        );
        assert!(command.contains("acme.sh --install-cert"));
        assert!(command.contains("fullchain.pem"));
    }

    /// 验证 DNSPod DNS-01 凭据格式只允许 ID:Token 配对，并拒绝控制字符。
    #[test]
    fn validates_dnspod_dns_credentials() {
        assert!(valid_dnspod_dns_token("123456:token-value"));
        assert!(!valid_dnspod_dns_token("missing-separator"));
        assert!(!valid_dnspod_dns_token("123456:token\nvalue"));
    }

    /// 验证腾讯云和 AWS Route 53 DNS-01 凭据都要求 ID:Secret 配对。
    #[test]
    fn validates_tencent_and_aws_dns_credentials() {
        assert!(valid_tencent_dns_token("AKID:secret-value"));
        assert!(!valid_tencent_dns_token("missing-separator"));
        assert!(valid_aws_dns_token("AKIA:secret-value"));
        assert!(!valid_aws_dns_token("AKIA:secret\nvalue"));
    }

    #[test]
    fn parses_php_cli_and_fpm_markers() {
        let runtimes = parse_php_runtimes(
            "__PHP__\tcli\t8.3.4\t/usr/bin/php\t\t1\t\n__PHP__\tphp8.3-fpm\t8.3.4\t/usr/sbin/php-fpm8.3\tphp8.3-fpm\t1\t/run/php/php8.3-fpm.sock\n",
        );
        assert_eq!(runtimes.len(), 2);
        assert_eq!(runtimes[1].service.as_deref(), Some("php8.3-fpm"));
        assert_eq!(
            runtimes[1].socket_path.as_deref(),
            Some("/run/php/php8.3-fpm.sock")
        );
        assert!(runtimes[1].running);
    }

    #[test]
    fn renders_php_fastcgi_binding_for_static_sites() {
        let input = SaveWebsiteInput {
            server_id: "server".into(),
            domain: "demo.example.com".into(),
            kind: "static".into(),
            listen_port: 80,
            root_path: None,
            php_runtime: Some("php8.3-fpm".into()),
            php_socket: None,
            upstream_scheme: None,
            upstream_host: None,
            upstream_port: None,
            enable_https: false,
            https_port: 443,
            certificate_path: None,
            certificate_key_path: None,
            confirmed: true,
        };
        let rendered = render_website(
            &input,
            "/www/sites/demo.example.com",
            Some("/run/php/php8.3-fpm.sock"),
        );
        assert!(rendered.contains("fastcgi_pass unix:/run/php/php8.3-fpm.sock;"));
        assert!(rendered.contains("index index.html index.htm index.php;"));
    }

    /// 验证容器内显式 socket 会覆盖 runtime 解析，并渲染为 FastCGI 路径。
    #[test]
    fn uses_explicit_container_socket_for_php_binding() {
        let input = SaveWebsiteInput {
            server_id: "server".into(),
            domain: "demo.example.com".into(),
            kind: "static".into(),
            listen_port: 80,
            root_path: None,
            php_runtime: None,
            php_socket: Some("/tmp/php-cgi/app.sock".into()),
            upstream_scheme: None,
            upstream_host: None,
            upstream_port: None,
            enable_https: false,
            https_port: 443,
            certificate_path: None,
            certificate_key_path: None,
            confirmed: true,
        };
        assert_eq!(
            override_php_socket(&input).unwrap(),
            Some("/tmp/php-cgi/app.sock".into())
        );
        let rendered = render_website(
            &input,
            "/www/sites/demo.example.com",
            input.php_socket.as_deref(),
        );
        assert!(rendered.contains("fastcgi_pass unix:/tmp/php-cgi/app.sock;"));
    }

    /// 验证非静态站点显式指定 socket 会被拒绝，且不安全路径无法通过校验。
    #[test]
    fn rejects_unsafe_explicit_php_socket() {
        let mut input = SaveWebsiteInput {
            server_id: "server".into(),
            domain: "demo.example.com".into(),
            kind: "proxy".into(),
            listen_port: 80,
            root_path: None,
            php_runtime: None,
            php_socket: Some("/tmp/php-cgi/app.sock".into()),
            upstream_scheme: Some("http".into()),
            upstream_host: Some("127.0.0.1".into()),
            upstream_port: Some(8080),
            enable_https: false,
            https_port: 443,
            certificate_path: None,
            certificate_key_path: None,
            confirmed: true,
        };
        assert!(override_php_socket(&input).is_err());
        input.kind = "static".into();
        input.php_socket = Some("/tmp/../etc/passwd".into());
        assert!(validate_php_socket_path(input.php_socket.as_deref().unwrap()).is_err());
    }

    /// 验证证书续期规划只覆盖启用的 HTTPS 站点，区分缺失证书与即将到期。
    #[test]
    fn plans_certificate_renewals_for_enabled_sites() {
        let snapshot = WebsiteSnapshot {
            supported: true,
            managed_conf_dir: Some("/etc/nginx/conf.d".into()),
            nginx_version: Some("1.24.0".into()),
            runtime_root: Some("/www/sites".into()),
            host_root: Some("/opt/1panel/apps/openresty/www/sites".into()),
            websites: vec![
                WebsiteRecord {
                    domain: "healthy.example.com".into(),
                    kind: WebsiteKind::Static,
                    enabled: true,
                    listen_port: 80,
                    root_path: Some("/www/sites/healthy".into()),
                    upstream: None,
                    php_runtime: None,
                    ssl: true,
                    certificate_path: Some("/www/certs/healthy/fullchain.pem".into()),
                    expires_at: Some("Jan  1 00:00:00 2099 GMT".into()),
                    config_path: "/etc/nginx/conf.d/healthy.conf".into(),
                },
                WebsiteRecord {
                    domain: "expiring.example.com".into(),
                    kind: WebsiteKind::Static,
                    enabled: true,
                    listen_port: 80,
                    root_path: Some("/www/sites/expiring".into()),
                    upstream: None,
                    php_runtime: None,
                    ssl: true,
                    certificate_path: Some("/www/certs/expiring/fullchain.pem".into()),
                    expires_at: Some("Jan  1 00:00:00 2000 GMT".into()),
                    config_path: "/etc/nginx/conf.d/expiring.conf".into(),
                },
                WebsiteRecord {
                    domain: "missing.example.com".into(),
                    kind: WebsiteKind::Proxy,
                    enabled: true,
                    listen_port: 443,
                    root_path: None,
                    upstream: Some("http://127.0.0.1:8080".into()),
                    php_runtime: None,
                    ssl: true,
                    certificate_path: None,
                    expires_at: None,
                    config_path: "/etc/nginx/conf.d/missing.conf".into(),
                },
                WebsiteRecord {
                    domain: "disabled.example.com".into(),
                    kind: WebsiteKind::Static,
                    enabled: false,
                    listen_port: 80,
                    root_path: Some("/www/sites/disabled".into()),
                    upstream: None,
                    php_runtime: None,
                    ssl: true,
                    certificate_path: None,
                    expires_at: None,
                    config_path: "/etc/nginx/conf.d/disabled.conf".into(),
                },
            ],
            php_runtimes: vec![],
            certificate_tools: CertificateTools {
                certbot: true,
                acme_sh: false,
            },
            warnings: vec![],
            fetched_at: "1970-01-01T00:00:00Z".into(),
        };
        let plans = certificate_renewal_plan(&snapshot, 30);
        let domains: Vec<_> = plans.iter().map(|plan| plan.domain.as_str()).collect();
        assert!(domains.contains(&"expiring.example.com"));
        assert!(domains.contains(&"missing.example.com"));
        assert!(!domains.contains(&"healthy.example.com"));
        assert!(!domains.contains(&"disabled.example.com"));
        assert!(plans
            .iter()
            .any(|plan| plan.domain == "expiring.example.com" && plan.action == "renew"));
        assert!(plans
            .iter()
            .any(|plan| plan.domain == "missing.example.com" && plan.action == "issue"));
    }
}

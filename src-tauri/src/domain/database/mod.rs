use crate::domain::ssh::SshConnectionManager;
use crate::errors::{AppError, AppResult};
use crate::security::shell_escape;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// 描述远端数据库引擎、版本和当前运行状态。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseEngine {
    pub id: String,
    pub name: String,
    pub installed: bool,
    pub running: bool,
    pub version: Option<String>,
    pub port: Option<u16>,
}

/// 描述一个可由本地客户端管理的数据库实例。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseRecord {
    pub engine: String,
    pub name: String,
    pub owner: Option<String>,
    pub charset: Option<String>,
    pub collation: Option<String>,
}

/// 描述一个远端数据库登录账号或 PostgreSQL role，不包含密码内容。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseUser {
    pub engine: String,
    pub username: String,
    pub host: Option<String>,
    pub privileges: Option<String>,
    pub can_login: Option<bool>,
}

/// 描述一个账号在单个数据库上的真实权限摘要，不包含密码或完整授权 SQL。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DatabasePrivilegeEntry {
    pub database: String,
    pub privileges: String,
}

/// 返回指定账号的数据库级权限矩阵；Redis 以 ACL 全局范围表示。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabasePrivilegeSnapshot {
    pub engine: String,
    pub username: String,
    pub host: Option<String>,
    pub entries: Vec<DatabasePrivilegeEntry>,
    pub fetched_at: chrono::DateTime<chrono::Utc>,
}

/// 一条权限模型诊断建议；severity 为 info 或 warning。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PrivilegeDiagnostic {
    pub severity: String,
    pub category: String,
    pub message: String,
}

/// 数据库权限矩阵及其诊断建议。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabasePrivilegeDiagnostic {
    pub snapshot: DatabasePrivilegeSnapshot,
    pub diagnostics: Vec<PrivilegeDiagnostic>,
}

/// 一次数据库探测返回的引擎和实例列表。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseSnapshot {
    pub engines: Vec<DatabaseEngine>,
    pub databases: Vec<DatabaseRecord>,
    pub users: Vec<DatabaseUser>,
    pub fetched_at: chrono::DateTime<chrono::Utc>,
}

/// 数据库创建或删除请求；所有破坏性操作必须显式确认。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseActionInput {
    pub server_id: String,
    pub engine: String,
    pub name: String,
    pub action: String,
    pub confirmed: bool,
}

/// 数据库备份请求；目标文件由用户明确指定在远端服务器上。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseBackupInput {
    pub server_id: String,
    pub engine: String,
    pub name: String,
    pub destination: String,
    pub confirmed: bool,
}

/// 数据库恢复请求；恢复会覆盖目标数据库中的同名对象。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseRestoreInput {
    pub server_id: String,
    pub engine: String,
    pub name: String,
    pub source: String,
    pub confirmed: bool,
}

/// 数据库账号和权限变更请求；password 只通过系统 IPC 进入 Rust，不落本地数据库。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseUserActionInput {
    pub server_id: String,
    pub engine: String,
    pub username: String,
    pub host: Option<String>,
    pub database: Option<String>,
    pub privileges: Option<String>,
    pub password: Option<SecretString>,
    pub action: String,
    pub confirmed: bool,
}

/// 查询指定账号的实际数据库级权限；Redis 凭据只用于本次远端命令。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabasePrivilegeInput {
    pub server_id: String,
    pub engine: String,
    pub username: String,
    pub host: Option<String>,
    #[serde(default)]
    pub redis_username: Option<String>,
    #[serde(default)]
    pub redis_password: Option<SecretString>,
}

/// 数据库服务启停请求；只允许固定引擎和 systemd 生命周期动作。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseEngineActionInput {
    pub server_id: String,
    pub engine: String,
    pub action: String,
    pub confirmed: bool,
}

/// 描述一次数据库引擎安装的固定包、服务和远端命令，供确认弹窗展示。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseInstallPlan {
    pub engine: DatabaseEngine,
    pub package_manager: String,
    pub packages: Vec<String>,
    pub services: Vec<String>,
    pub command: String,
    pub risk: String,
}

/// 数据库引擎安装请求；安装包和命令只能由 Rust 端根据固定引擎映射生成。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseInstallInput {
    pub server_id: String,
    pub engine: String,
    pub confirmed: bool,
    pub task_id: String,
}

/// 数据库引擎安装完成后的验证结果，不包含任何凭据或密码。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseInstallResult {
    pub engine: DatabaseEngine,
    pub package_manager: String,
    pub output: String,
}

/// 返回数据库变更或备份操作的可审计结果。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseActionResult {
    pub engine: String,
    pub name: String,
    pub action: String,
    pub output: String,
}

/// 描述 Redis 数据库中的一个键；值本身不会在列表接口中回传。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RedisKeyEntry {
    pub key: String,
    pub kind: String,
    pub ttl_seconds: i64,
    pub size_bytes: Option<u64>,
}

/// 返回 Redis 键空间摘要和受限的键列表。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RedisSnapshot {
    pub available: bool,
    pub database: u8,
    pub total_keys: u64,
    pub keys: Vec<RedisKeyEntry>,
    pub fetched_at: chrono::DateTime<chrono::Utc>,
}

/// Redis 连接诊断结果，不含任何键值或凭据。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RedisDiagnostic {
    pub available: bool,
    pub database: u8,
    pub latency_ms: Option<u64>,
    pub status: Option<String>,
    pub version: Option<String>,
    pub role: Option<String>,
    pub mode: Option<String>,
    pub uptime_seconds: Option<u64>,
    pub connected_clients: Option<u64>,
    pub used_memory_bytes: Option<u64>,
    pub fetched_at: chrono::DateTime<chrono::Utc>,
}

/// Redis 连接诊断请求；凭据只在本次远端命令中使用，不写入本地。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RedisDiagnosticInput {
    pub server_id: String,
    pub database: u8,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<SecretString>,
}

/// Redis 键查询请求；pattern 和 limit 只影响只读扫描范围。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RedisQueryInput {
    pub server_id: String,
    pub database: u8,
    pub pattern: Option<String>,
    pub limit: Option<u16>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<SecretString>,
}

/// Redis 数据变更请求；删除单键或 FLUSHDB 必须显式确认。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RedisActionInput {
    pub server_id: String,
    pub database: u8,
    pub action: String,
    pub key: Option<String>,
    pub confirmed: bool,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<SecretString>,
}

/// 返回 Redis 数据变更结果，不回传被删除键的值。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RedisActionResult {
    pub database: u8,
    pub action: String,
    pub key: Option<String>,
    pub output: String,
}

/// Redis 键值读写请求；get 为只读，set 必须确认并可设置 TTL。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RedisValueInput {
    pub server_id: String,
    pub database: u8,
    pub key: String,
    pub action: String,
    pub value: Option<String>,
    pub ttl_seconds: Option<i64>,
    pub confirmed: bool,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<SecretString>,
}

/// 返回 Redis 键值摘要；超过 64 KiB 的值会被截断并标记。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RedisValueResult {
    pub database: u8,
    pub key: String,
    pub kind: String,
    pub value: String,
    pub truncated: bool,
}

/// Redis complex-value mutation request; each operation is type-checked remotely before it runs.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RedisComplexActionInput {
    pub server_id: String,
    pub database: u8,
    pub key: String,
    pub kind: String,
    pub action: String,
    pub field: Option<String>,
    pub value: Option<String>,
    pub score: Option<f64>,
    pub confirmed: bool,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<SecretString>,
}

/// Returns a numeric Redis complex mutation result without returning the stored value.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RedisComplexActionResult {
    pub database: u8,
    pub key: String,
    pub kind: String,
    pub action: String,
    pub output: String,
}

/// Redis 复杂值迁移请求；快照文件始终保存在远端，避免把业务值传回客户端。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RedisTransferInput {
    pub server_id: String,
    pub database: u8,
    pub action: String,
    pub path: String,
    pub max_keys: Option<u32>,
    pub confirmed: bool,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<SecretString>,
}

/// 返回 Redis 远端快照迁移结果，不包含任何键值内容。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RedisTransferResult {
    pub database: u8,
    pub action: String,
    pub path: String,
    pub keys: u64,
    pub output: String,
}

/// Redis 跨实例/跨版本迁移请求；迁移由源服务器上的 redis-cli 发起，数据不回传客户端。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RedisMigrationInput {
    pub source_server_id: String,
    pub source_database: u8,
    pub target_host: String,
    pub target_port: u16,
    pub target_database: u8,
    #[serde(default)]
    pub source_username: Option<String>,
    #[serde(default)]
    pub source_password: Option<SecretString>,
    #[serde(default)]
    pub target_username: Option<String>,
    #[serde(default)]
    pub target_password: Option<SecretString>,
    pub max_keys: Option<u32>,
    pub confirmed: bool,
}

/// 返回 Redis 跨实例迁移的有限统计，不包含键名或值。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RedisMigrationResult {
    pub source_database: u8,
    pub target_host: String,
    pub target_port: u16,
    pub target_database: u8,
    pub keys: u64,
    pub output: String,
}

/// 探测 MySQL、MariaDB、PostgreSQL 和 Redis，并读取当前可见的数据库列表。
pub async fn snapshot(ssh: &SshConnectionManager, server_id: &str) -> AppResult<DatabaseSnapshot> {
    let result = ssh
        .execute_system(server_id, &probe_command(), Duration::from_secs(45))
        .await?;
    if result.exit_code != 0 {
        return Err(
            AppError::new("DATABASE_PROBE_FAILED", "database", "数据库能力探测失败")
                .details(result.stderr)
                .for_server(server_id),
        );
    }
    parse_snapshot(&result.stdout).ok_or_else(|| {
        AppError::new(
            "DATABASE_PARSE_FAILED",
            "database",
            "数据库探测结果无法解析",
        )
        .for_server(server_id)
    })
}

/// 创建或删除一个本地数据库，并在命令完成后返回标准输出用于审计。
pub async fn action(
    ssh: &SshConnectionManager,
    input: DatabaseActionInput,
) -> AppResult<DatabaseActionResult> {
    validate_action(&input)?;
    let command = match (input.engine.as_str(), input.action.as_str()) {
        ("mysql" | "mariadb", "create") => format!(
            "mysql --batch --skip-column-names -e {}",
            shell_escape(&format!("CREATE DATABASE IF NOT EXISTS `{}` CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;", input.name))
        ),
        ("mysql" | "mariadb", "drop") => format!(
            "mysql --batch --skip-column-names -e {}",
            shell_escape(&format!("DROP DATABASE IF EXISTS `{}`;", input.name))
        ),
        ("postgresql", "create") => format!(
            "psql --tuples-only --no-align -c {}",
            shell_escape(&format!("CREATE DATABASE \"{}\";", input.name))
        ),
        ("postgresql", "drop") => format!(
            "psql --tuples-only --no-align -c {}",
            shell_escape(&format!("DROP DATABASE IF EXISTS \"{}\";", input.name))
        ),
        ("redis", _) => return Err(unsupported("Redis 不需要创建或删除数据库")),
        _ => return Err(unsupported("不支持的数据库引擎或操作")),
    };
    let result = ssh
        .execute_system(
            &input.server_id,
            &postgres_command(&input.engine, &command),
            Duration::from_secs(60),
        )
        .await?;
    if result.exit_code != 0 {
        return Err(
            AppError::new("DATABASE_ACTION_FAILED", "database", "数据库操作失败")
                .details(result.stderr)
                .for_server(input.server_id),
        );
    }
    Ok(DatabaseActionResult {
        engine: input.engine,
        name: input.name,
        action: input.action,
        output: result.stdout,
    })
}

/// 创建、删除或调整数据库账号权限，并在返回值中省略任何密码和 SQL 秘密。
pub async fn user_action(
    ssh: &SshConnectionManager,
    input: DatabaseUserActionInput,
) -> AppResult<DatabaseActionResult> {
    validate_user_action(&input)?;
    let command = database_user_command(&input)?;
    let result = ssh
        .execute_system(
            &input.server_id,
            &postgres_command(&input.engine, &command),
            Duration::from_secs(90),
        )
        .await?;
    if result.exit_code != 0 {
        return Err(AppError::new(
            "DATABASE_USER_ACTION_FAILED",
            "database",
            "数据库账号操作失败",
        )
        .details(result.stderr)
        .for_server(input.server_id));
    }
    Ok(DatabaseActionResult {
        engine: input.engine,
        name: input.username,
        action: format!("user_{}", input.action),
        output: result.stdout,
    })
}

/// 查询 MySQL/MariaDB schema 权限、PostgreSQL database ACL 或 Redis ACL 全局范围。
pub async fn privilege_matrix(
    ssh: &SshConnectionManager,
    input: DatabasePrivilegeInput,
) -> AppResult<DatabasePrivilegeSnapshot> {
    validate_privilege_input(&input)?;
    let username = sql_string_literal(&input.username);
    let host = sql_string_literal(input.host.as_deref().unwrap_or("%"));
    let command = match input.engine.as_str() {
        "mysql" | "mariadb" => {
            let query = format!(
                "SELECT CONCAT('__DB_PRIV__', CHAR(9), TABLE_SCHEMA, CHAR(9), GROUP_CONCAT(PRIVILEGE_TYPE ORDER BY PRIVILEGE_TYPE SEPARATOR ',')) FROM information_schema.SCHEMA_PRIVILEGES WHERE GRANTEE=CONCAT(CHAR(39), {username}, CHAR(39), '@', CHAR(39), {host}, CHAR(39)) GROUP BY TABLE_SCHEMA"
            );
            format!(
                "{} --batch --skip-column-names -e {}",
                input.engine,
                shell_escape(&query)
            )
        }
        "postgresql" => {
            let query = format!(
                "SELECT '__DB_PRIV__' || chr(9) || datname || chr(9) || concat_ws(',', CASE WHEN has_database_privilege({username}, datname, 'CONNECT') THEN 'CONNECT' END, CASE WHEN has_database_privilege({username}, datname, 'CREATE') THEN 'CREATE' END, CASE WHEN has_database_privilege({username}, datname, 'TEMPORARY') THEN 'TEMPORARY' END) FROM pg_database WHERE NOT datistemplate AND concat_ws(',', CASE WHEN has_database_privilege({username}, datname, 'CONNECT') THEN 'CONNECT' END, CASE WHEN has_database_privilege({username}, datname, 'CREATE') THEN 'CREATE' END, CASE WHEN has_database_privilege({username}, datname, 'TEMPORARY') THEN 'TEMPORARY' END) <> ''"
            );
            format!("psql --tuples-only --no-align -c {}", shell_escape(&query))
        }
        "redis" => {
            let redis_user = input.redis_username.as_deref().unwrap_or(&input.username);
            let options = redis_cli_options(
                input.redis_username.as_deref(),
                input.redis_password.as_ref(),
            )?;
            format!(
                "if redis-cli{options} --raw ACL GETUSER {user} >/dev/null 2>&1; then redis-cli{options} --raw ACL GETUSER {user} | awk 'BEGIN{{section=\"\"; output=\"\"}} $0==\"flags\"{{section=\"flags\"; next}} $0==\"passwords\"{{section=\"passwords\"; next}} $0==\"commands\"{{section=\"commands\"; next}} $0==\"keys\"{{section=\"keys\"; next}} $0==\"channels\"{{section=\"channels\"; next}} $0==\"selectors\"{{section=\"selectors\"; next}} section==\"flags\" || section==\"commands\" || section==\"keys\" || section==\"channels\" || section==\"selectors\" {{ if ($0 != \"\") {{ if (output != \"\") output=output \",\"; output=output $0 }} }} END{{ printf \"__DB_PRIV__\\t*\\t%s\\n\", output }}'; else exit 1; fi",
                user = shell_escape(redis_user),
            )
        }
        _ => return Err(unsupported("不支持的数据库引擎")),
    };
    let result = ssh
        .execute_system(
            &input.server_id,
            &postgres_command(&input.engine, &command),
            Duration::from_secs(60),
        )
        .await?;
    if result.exit_code != 0 {
        return Err(AppError::new(
            "DATABASE_PRIVILEGE_FAILED",
            "database",
            "读取数据库权限矩阵失败",
        )
        .details(result.stderr)
        .for_server(input.server_id));
    }
    let entries = parse_privilege_matrix(&result.stdout);
    Ok(DatabasePrivilegeSnapshot {
        engine: input.engine,
        username: input.username,
        host: input.host,
        entries,
        fetched_at: chrono::Utc::now(),
    })
}

/// 读取数据库权限矩阵并附带安全诊断建议，便于识别过宽授权与通配主机。
pub async fn database_privilege_diagnostic(
    ssh: &SshConnectionManager,
    input: DatabasePrivilegeInput,
) -> AppResult<DatabasePrivilegeDiagnostic> {
    let snapshot = privilege_matrix(ssh, input).await?;
    let diagnostics = diagnose_privilege_matrix(&snapshot);
    Ok(DatabasePrivilegeDiagnostic {
        snapshot,
        diagnostics,
    })
}

/// 对权限矩阵做只读安全诊断：通配主机、全库授权、ALL 权限与 Redis 过宽 ACL。
fn diagnose_privilege_matrix(snapshot: &DatabasePrivilegeSnapshot) -> Vec<PrivilegeDiagnostic> {
    let mut diagnostics = Vec::new();
    if snapshot.engine == "redis" {
        // Redis ACL 规则："~*" 允许所有键，"＋@all" 代表全部命令。
        for entry in &snapshot.entries {
            let scope = entry.database.to_uppercase();
            let rules = entry.privileges.to_uppercase();
            if scope.contains("~*") {
                diagnostics.push(PrivilegeDiagnostic {
                    severity: "warning".into(),
                    category: "scope".into(),
                    message: format!(
                        "Redis 用户 {} 的 ACL 允许访问所有键（~*）",
                        snapshot.username
                    ),
                });
            }
            if rules.contains("+@ALL") || rules.contains("ALLCOMMANDS") {
                diagnostics.push(PrivilegeDiagnostic {
                    severity: "warning".into(),
                    category: "privilege".into(),
                    message: format!(
                        "Redis 用户 {} 的 ACL 允许全部命令（+@all）",
                        snapshot.username
                    ),
                });
            }
        }
        return diagnostics;
    }
    if snapshot.host.as_deref().map(str::trim) == Some("%") {
        diagnostics.push(PrivilegeDiagnostic {
            severity: "warning".into(),
            category: "host".into(),
            message: format!(
                "账号 {} 允许从任意主机（%）连接，建议限定来源",
                snapshot.username
            ),
        });
    }
    for entry in &snapshot.entries {
        let database = entry.database.trim();
        let privileges = entry.privileges.to_uppercase();
        if database == "*.*" || database == "*" {
            diagnostics.push(PrivilegeDiagnostic {
                severity: "warning".into(),
                category: "scope".into(),
                message: format!(
                    "账号 {} 被授予全部数据库（{}）",
                    snapshot.username, database
                ),
            });
        }
        if privileges
            .split(',')
            .any(|part| part.trim() == "ALL" || part.trim() == "ALL PRIVILEGES")
        {
            diagnostics.push(PrivilegeDiagnostic {
                severity: "info".into(),
                category: "privilege".into(),
                message: format!(
                    "账号 {} 在 {} 上拥有 ALL PRIVILEGES",
                    snapshot.username, database
                ),
            });
        }
    }
    if matches!(snapshot.engine.as_str(), "mysql" | "mariadb") {
        // MySQL/MariaDB 的 Grant Option 属于可继续授权他人，应单独标注。
        for entry in &snapshot.entries {
            let privileges = entry.privileges.to_uppercase();
            if privileges
                .split(',')
                .any(|part| part.trim() == "GRANT OPTION")
            {
                diagnostics.push(PrivilegeDiagnostic {
                    severity: "warning".into(),
                    category: "delegation".into(),
                    message: format!(
                        "账号 {} 在 {} 上拥有 GRANT OPTION，可把自身权限继续授予其他用户",
                        snapshot.username, entry.database
                    ),
                });
            }
        }
    } else if snapshot.engine == "postgresql" {
        // PostgreSQL 中具备 CREATE 的库越多，能新建对象的范围就越广。
        let create_count = snapshot
            .entries
            .iter()
            .filter(|entry| {
                entry
                    .privileges
                    .to_uppercase()
                    .split(',')
                    .any(|part| part.trim() == "CREATE")
            })
            .count();
        if create_count >= 2 {
            diagnostics.push(PrivilegeDiagnostic {
                severity: "info".into(),
                category: "scope".into(),
                message: format!(
                    "账号 {} 在 {} 个数据库上拥有 CREATE 权限，可在较多目标中新建对象",
                    snapshot.username, create_count
                ),
            });
        }
    }
    diagnostics
}

/// 启动、停止或重启数据库服务，并返回刷新前的命令输出。
pub async fn engine_action(
    ssh: &SshConnectionManager,
    input: DatabaseEngineActionInput,
) -> AppResult<DatabaseActionResult> {
    if !input.confirmed
        || !matches!(
            input.engine.as_str(),
            "mysql" | "mariadb" | "postgresql" | "redis"
        )
        || !matches!(input.action.as_str(), "start" | "stop" | "restart")
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "database",
            "数据库服务引擎、动作或确认状态无效",
        ));
    }
    let services = match input.engine.as_str() {
        "mysql" => "mysql mysqld",
        "mariadb" => "mariadb mysql",
        "postgresql" => "postgresql",
        "redis" => "redis-server redis",
        _ => unreachable!(),
    };
    let command = format!(
        "set -e; if command -v systemctl >/dev/null 2>&1; then for service in {services}; do if systemctl list-unit-files \"$service.service\" >/dev/null 2>&1; then systemctl {} \"$service\" && exit 0; fi; done; elif command -v rc-service >/dev/null 2>&1; then for service in {services}; do if [ -x \"/etc/init.d/$service\" ]; then rc-service \"$service\" {} && exit 0; fi; done; fi; echo 'database service not found' >&2; exit 127",
        input.action, input.action
    );
    let result = ssh
        .execute_system(&input.server_id, &command, Duration::from_secs(60))
        .await?;
    if result.exit_code != 0 {
        return Err(AppError::new(
            "DATABASE_SERVICE_ACTION_FAILED",
            "database",
            "数据库服务操作失败",
        )
        .details(result.stderr)
        .for_server(input.server_id));
    }
    Ok(DatabaseActionResult {
        engine: input.engine,
        name: "service".into(),
        action: input.action,
        output: result.stdout,
    })
}

/// 生成数据库引擎安装计划；只探测平台和当前状态，不执行远端写入。
pub async fn install_plan(
    ssh: &SshConnectionManager,
    server_id: &str,
    engine: &str,
) -> AppResult<DatabaseInstallPlan> {
    let current = snapshot(ssh, server_id)
        .await?
        .engines
        .into_iter()
        .find(|value| value.id == engine)
        .ok_or_else(|| invalid_engine(engine, server_id))?;
    if current.installed {
        return Err(
            AppError::new("ALREADY_INSTALLED", "database", "数据库引擎已经安装")
                .for_server(server_id),
        );
    }
    let package_manager = detect_package_manager(ssh, server_id).await?;
    let (packages, services) = install_definition(engine, &package_manager)?;
    let adapter = crate::domain::platform::adapter_for(&package_manager);
    let package_command = adapter.install_command(&packages.join(" "));
    if package_command.is_empty() {
        return Err(AppError::new(
            "UNSUPPORTED_PLATFORM",
            "database",
            "未识别支持的 apt、dnf、apk 或 pacman 包管理器",
        )
        .for_server(server_id));
    }
    Ok(DatabaseInstallPlan {
        engine: current,
        package_manager,
        packages,
        services: services.iter().map(|value| (*value).to_string()).collect(),
        command: install_command(engine, &package_command, &services),
        risk: "将通过系统包管理器安装数据库服务，并尝试启用和启动对应 systemd 服务。不会创建数据库或用户。"
            .into(),
    })
}

/// 在用户明确确认后安装数据库引擎，并重新探测验证安装结果。
pub async fn install(
    ssh: &SshConnectionManager,
    input: DatabaseInstallInput,
    events: &tauri::ipc::Channel<crate::domain::ssh::CommandEvent>,
) -> AppResult<DatabaseInstallResult> {
    if !input.confirmed {
        return Err(AppError::new(
            "CONFIRMATION_REQUIRED",
            "database",
            "安装数据库引擎需要明确确认",
        )
        .for_server(&input.server_id));
    }
    if input.task_id.is_empty()
        || input.task_id.len() > 128
        || input
            .task_id
            .chars()
            .any(|character| character == '\0' || character == '\r' || character == '\n')
    {
        return Err(
            AppError::new("VALIDATION_FAILED", "task", "数据库安装任务 ID 无效")
                .for_server(&input.server_id),
        );
    }
    let plan = install_plan(ssh, &input.server_id, &input.engine).await?;
    let result = ssh
        .execute_stream_system_task(
            &input.server_id,
            &plan.command,
            Duration::from_secs(900),
            events,
            &input.task_id,
        )
        .await?;
    if result.exit_code != 0 {
        return Err(
            AppError::new("DATABASE_INSTALL_FAILED", "database", "数据库引擎安装失败")
                .details(result.stderr)
                .for_server(&input.server_id),
        );
    }
    let verified = snapshot(ssh, &input.server_id)
        .await?
        .engines
        .into_iter()
        .find(|value| value.id == input.engine)
        .ok_or_else(|| {
            AppError::new(
                "DATABASE_VERIFY_FAILED",
                "database",
                "安装后未找到数据库引擎",
            )
            .for_server(&input.server_id)
        })?;
    if !verified.installed {
        return Err(AppError::new(
            "DATABASE_VERIFY_FAILED",
            "database",
            "安装命令成功但数据库引擎验证失败",
        )
        .for_server(&input.server_id));
    }
    Ok(DatabaseInstallResult {
        engine: verified,
        package_manager: plan.package_manager,
        output: format!("{}\n{}", result.stdout, result.stderr),
    })
}

/// 扫描 Redis 键的类型、TTL 和内存摘要；最多返回 500 个键，避免大库拖垮客户端。
pub async fn redis_snapshot(
    ssh: &SshConnectionManager,
    input: RedisQueryInput,
) -> AppResult<RedisSnapshot> {
    validate_redis_query(&input)?;
    let cli_options = redis_cli_options(input.username.as_deref(), input.password.as_ref())?;
    let pattern = input.pattern.as_deref().unwrap_or("*").trim();
    let limit = input.limit.unwrap_or(100).clamp(1, 500);
    let pattern_option = if pattern == "*" {
        String::new()
    } else {
        format!(" --pattern {}", shell_escape(pattern))
    };
    let command = format!(
        "set +e; if ! command -v redis-cli >/dev/null 2>&1; then printf '__REDIS_MISSING__\\n'; exit 0; fi; total=$(redis-cli{cli} --raw -n {database} DBSIZE 2>/dev/null); printf '__REDIS_DB__\\t%s\\n' \"$total\"; redis-cli{cli} --raw -n {database} --scan{pattern} 2>/dev/null | head -n {limit} | while IFS= read -r key; do [ -n \"$key\" ] || continue; encoded=$(printf '%s' \"$key\" | base64 -w0 2>/dev/null || printf '%s' \"$key\" | base64 | tr -d '\\n'); kind=$(redis-cli{cli} --raw -n {database} TYPE \"$key\" 2>/dev/null); ttl=$(redis-cli{cli} --raw -n {database} TTL \"$key\" 2>/dev/null); size=$(redis-cli{cli} --raw -n {database} MEMORY USAGE \"$key\" 2>/dev/null); printf '__REDIS_KEY__\\t%s\\t%s\\t%s\\t%s\\n' \"$encoded\" \"$kind\" \"$ttl\" \"$size\"; done",
        cli = cli_options,
        database = input.database,
        pattern = pattern_option,
        limit = limit,
    );
    let result = ssh
        .execute_system(&input.server_id, &command, Duration::from_secs(60))
        .await?;
    if result.exit_code != 0 {
        return Err(
            AppError::new("REDIS_PROBE_FAILED", "database", "Redis 数据扫描失败")
                .details(result.stderr)
                .for_server(input.server_id),
        );
    }
    if result.stdout.contains("__REDIS_MISSING__") {
        return Ok(RedisSnapshot {
            available: false,
            database: input.database,
            total_keys: 0,
            keys: Vec::new(),
            fetched_at: chrono::Utc::now(),
        });
    }
    parse_redis_snapshot(&result.stdout, input.database).ok_or_else(|| {
        AppError::new(
            "REDIS_PARSE_FAILED",
            "database",
            "Redis 数据扫描结果无法解析",
        )
        .for_server(input.server_id)
    })
}

/// 对单个 Redis 逻辑库做只读连接诊断：PING 延迟、版本、角色、客户端与内存摘要。
pub async fn redis_diagnostic(
    ssh: &SshConnectionManager,
    input: RedisDiagnosticInput,
) -> AppResult<RedisDiagnostic> {
    validate_redis_diagnostic(&input)?;
    let cli_options = redis_cli_options(input.username.as_deref(), input.password.as_ref())?;
    let command = format!(
        "set +e; if ! command -v redis-cli >/dev/null 2>&1; then printf '__REDIS_MISSING__\\n'; exit 0; fi; \
         pong=$(redis-cli{cli} -n {db} PING 2>/dev/null); start=$(date +%s%N); redis-cli{cli} -n {db} PING >/dev/null 2>&1; end=$(date +%s%N); \
         printf '__REDIS_PING__\\t%s\\n' \"$pong\"; \
         printf '__REDIS_LATENCY_MS__\\t%s\\n' \"$(( (end-start)/1000000 ))\"; \
         redis-cli{cli} -n {db} INFO server 2>/dev/null | grep -E '^(redis_version|role|redis_mode|uptime_in_seconds):'; \
         redis-cli{cli} -n {db} INFO clients 2>/dev/null | grep -E '^connected_clients:'; \
         redis-cli{cli} -n {db} INFO memory 2>/dev/null | grep -E '^used_memory:'",
        cli = cli_options,
        db = input.database,
    );
    let result = ssh
        .execute_system(&input.server_id, &command, Duration::from_secs(30))
        .await?;
    if result.exit_code != 0 {
        return Err(
            AppError::new("REDIS_DIAGNOSTIC_FAILED", "database", "Redis 连接诊断失败")
                .details(result.stderr)
                .for_server(&input.server_id),
        );
    }
    if result.stdout.contains("__REDIS_MISSING__") {
        return Ok(RedisDiagnostic {
            available: false,
            database: input.database,
            latency_ms: None,
            status: None,
            version: None,
            role: None,
            mode: None,
            uptime_seconds: None,
            connected_clients: None,
            used_memory_bytes: None,
            fetched_at: chrono::Utc::now(),
        });
    }
    parse_redis_diagnostic(&result.stdout, input.database).ok_or_else(|| {
        AppError::new(
            "REDIS_DIAGNOSTIC_PARSE_FAILED",
            "database",
            "Redis 连接诊断结果无法解析",
        )
        .for_server(&input.server_id)
    })
}

/// 校验 Redis 连接诊断请求：服务器标识与逻辑库范围必须合法。
fn validate_redis_diagnostic(input: &RedisDiagnosticInput) -> AppResult<()> {
    if input.server_id.trim().is_empty() || input.server_id.len() > 128 || input.database > 15 {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "database",
            "Redis 诊断参数无效",
        ));
    }
    Ok(())
}

/// 解析 redis-cli PING/INFO 输出为连接诊断结构；缺少 PING 或版本时视为不可解析。
fn parse_redis_diagnostic(output: &str, database: u8) -> Option<RedisDiagnostic> {
    let mut latency_ms = None;
    let mut status = None;
    let mut version = None;
    let mut role = None;
    let mut mode = None;
    let mut uptime_seconds = None;
    let mut connected_clients = None;
    let mut used_memory_bytes = None;
    for line in output.lines() {
        if let Some(value) = line.strip_prefix("__REDIS_PING__\t") {
            status = Some(value.trim().to_string());
        } else if let Some(value) = line.strip_prefix("__REDIS_LATENCY_MS__\t") {
            latency_ms = value.trim().parse().ok();
        } else if let Some(value) = line.strip_prefix("redis_version:") {
            version = Some(value.trim().to_string());
        } else if let Some(value) = line.strip_prefix("role:") {
            role = Some(value.trim().to_string());
        } else if let Some(value) = line.strip_prefix("redis_mode:") {
            mode = Some(value.trim().to_string());
        } else if let Some(value) = line.strip_prefix("uptime_in_seconds:") {
            uptime_seconds = value.trim().parse().ok();
        } else if let Some(value) = line.strip_prefix("connected_clients:") {
            connected_clients = value.trim().parse().ok();
        } else if let Some(value) = line.strip_prefix("used_memory:") {
            used_memory_bytes = value.trim().parse().ok();
        }
    }
    if status.is_none() && version.is_none() {
        return None;
    }
    Some(RedisDiagnostic {
        available: true,
        database,
        latency_ms,
        status,
        version,
        role,
        mode,
        uptime_seconds,
        connected_clients,
        used_memory_bytes,
        fetched_at: chrono::Utc::now(),
    })
}

/// 删除一个 Redis 键或清空指定逻辑库；清空操作永远不会隐式执行。
pub async fn redis_action(
    ssh: &SshConnectionManager,
    input: RedisActionInput,
) -> AppResult<RedisActionResult> {
    validate_redis_action(&input)?;
    let cli_options = redis_cli_options(input.username.as_deref(), input.password.as_ref())?;
    let command = match input.action.as_str() {
        "delete" => format!(
            "redis-cli{} --raw -n {} DEL {}",
            cli_options,
            input.database,
            shell_escape(input.key.as_deref().unwrap_or_default())
        ),
        "flushdb" => format!(
            "redis-cli{} --raw -n {} FLUSHDB ASYNC",
            cli_options, input.database
        ),
        _ => {
            return Err(
                AppError::new("VALIDATION_FAILED", "database", "不支持的 Redis 数据操作")
                    .for_server(input.server_id),
            )
        }
    };
    let result = ssh
        .execute_system(&input.server_id, &command, Duration::from_secs(60))
        .await?;
    if result.exit_code != 0 {
        return Err(
            AppError::new("REDIS_ACTION_FAILED", "database", "Redis 数据操作失败")
                .details(result.stderr)
                .for_server(input.server_id),
        );
    }
    Ok(RedisActionResult {
        database: input.database,
        action: input.action,
        key: input.key,
        output: result.stdout,
    })
}

/// 读取 Redis 键值摘要或写入字符串键；复杂类型只读，不执行危险的隐式转换。
pub async fn redis_value(
    ssh: &SshConnectionManager,
    input: RedisValueInput,
) -> AppResult<RedisValueResult> {
    validate_redis_value(&input)?;
    let cli_options = redis_cli_options(input.username.as_deref(), input.password.as_ref())?;
    let key = shell_escape(&input.key);
    if input.action == "get" {
        let command = format!(
            "set +e; kind=$(redis-cli{cli} --raw -n {database} TYPE {key} 2>/dev/null); printf '__REDIS_KIND__%s\\n' \"$kind\"; case \"$kind\" in string) redis-cli{cli} --raw -n {database} GET {key} | head -c 65536 ;; hash) redis-cli{cli} --raw -n {database} HGETALL {key} | head -c 65536 ;; list) redis-cli{cli} --raw -n {database} LRANGE {key} 0 99 | head -c 65536 ;; set) redis-cli{cli} --raw -n {database} SSCAN {key} 0 COUNT 100 | head -c 65536 ;; zset) redis-cli{cli} --raw -n {database} ZRANGE {key} 0 99 WITHSCORES | head -c 65536 ;; esac",
            cli = cli_options,
            database = input.database,
            key = key,
        );
        let result = ssh
            .execute_system(&input.server_id, &command, Duration::from_secs(60))
            .await?;
        if result.exit_code != 0 {
            return Err(AppError::new(
                "REDIS_VALUE_READ_FAILED",
                "database",
                "读取 Redis 键值失败",
            )
            .details(result.stderr)
            .for_server(input.server_id));
        }
        let mut lines = result.stdout.splitn(2, '\n');
        let kind = lines
            .next()
            .and_then(|line| line.strip_prefix("__REDIS_KIND__"))
            .unwrap_or("none")
            .to_string();
        let value = lines.next().unwrap_or_default().to_string();
        return Ok(RedisValueResult {
            database: input.database,
            key: input.key,
            kind,
            truncated: value.len() >= 65_536,
            value,
        });
    }
    let value = input.value.as_deref().unwrap_or_default();
    let ttl = input.ttl_seconds.unwrap_or(0);
    let ttl_option = if ttl > 0 {
        format!(" EX {ttl}")
    } else {
        String::new()
    };
    let command = format!(
        "redis-cli{} --raw -n {} SET {} {}{}",
        cli_options,
        input.database,
        key,
        shell_escape(value),
        ttl_option
    );
    let result = ssh
        .execute_system(&input.server_id, &command, Duration::from_secs(60))
        .await?;
    if result.exit_code != 0 {
        return Err(AppError::new(
            "REDIS_VALUE_WRITE_FAILED",
            "database",
            "写入 Redis 字符串失败",
        )
        .details(result.stderr)
        .for_server(input.server_id));
    }
    Ok(RedisValueResult {
        database: input.database,
        key: input.key,
        kind: "string".into(),
        value: "OK".into(),
        truncated: false,
    })
}

/// Applies one bounded hash/list/set/zset operation and verifies the remote Redis type first.
pub async fn redis_complex_action(
    ssh: &SshConnectionManager,
    input: RedisComplexActionInput,
) -> AppResult<RedisComplexActionResult> {
    validate_redis_complex_action(&input)?;
    let cli_options = redis_cli_options(input.username.as_deref(), input.password.as_ref())?;
    let key = shell_escape(&input.key);
    let expected_kind = shell_escape(&input.kind);
    let field = shell_escape(input.field.as_deref().unwrap_or_default());
    let value = shell_escape(input.value.as_deref().unwrap_or_default());
    let score = shell_escape(&input.score.unwrap_or_default().to_string());
    let operation = match input.action.as_str() {
        "hash_set" => format!("HSET {key} {field} {value}"),
        "hash_delete" => format!("HDEL {key} {field}"),
        "list_push_left" => format!("LPUSH {key} {value}"),
        "list_push_right" => format!("RPUSH {key} {value}"),
        "list_pop_left" => format!("LPOP {key}"),
        "list_pop_right" => format!("RPOP {key}"),
        "set_add" => format!("SADD {key} {value}"),
        "set_remove" => format!("SREM {key} {value}"),
        "zset_add" => format!("ZADD {key} {score} {value}"),
        "zset_remove" => format!("ZREM {key} {value}"),
        _ => unreachable!(),
    };
    let command = format!(
        "set -e; kind=$(redis-cli{cli} --raw -n {database} TYPE {key} 2>/dev/null); [ \"$kind\" = {expected_kind} ] || {{ printf 'redis type mismatch: %s\\n' \"$kind\" >&2; exit 42; }}; redis-cli{cli} --raw -n {database} {operation}",
        cli = cli_options,
        database = input.database,
        key = key,
        expected_kind = expected_kind,
        operation = operation,
    );
    let result = ssh
        .execute_system(&input.server_id, &command, Duration::from_secs(60))
        .await?;
    if result.exit_code != 0 {
        return Err(AppError::new(
            "REDIS_COMPLEX_ACTION_FAILED",
            "database",
            "Redis 复杂值操作失败",
        )
        .details(result.stderr)
        .for_server(input.server_id));
    }
    Ok(RedisComplexActionResult {
        database: input.database,
        key: input.key,
        kind: input.kind,
        action: input.action,
        output: result.stdout,
    })
}

/// 使用 Redis DUMP/RESTORE 在远端导出或导入复杂类型、TTL 和键名，不把值带回本机。
pub async fn redis_transfer(
    ssh: &SshConnectionManager,
    input: RedisTransferInput,
) -> AppResult<RedisTransferResult> {
    validate_redis_transfer(&input)?;
    let cli_options = redis_cli_options(input.username.as_deref(), input.password.as_ref())?;
    let path = shell_escape(&input.path);
    let max_keys = input.max_keys.unwrap_or(10_000).clamp(1, 100_000);
    let command = match input.action.as_str() {
        "export" => format!(
            concat!(
                "set -eu\n",
                "if ! command -v redis-cli >/dev/null 2>&1; then printf '%s\\n' 'redis-cli not found' >&2; exit 127; fi\n",
                "mkdir -p -- $(dirname -- {path})\n",
                "tmp=$(mktemp)\n",
                "trap 'rm -f -- \"$tmp\"' EXIT\n",
                "redis-cli{cli} --raw -n {database} --scan > \"$tmp\"\n",
                ": > {path}\n",
                "encode() {{ base64 -w0 2>/dev/null || base64 | tr -d '\\n'; }}\n",
                "count=0\n",
                "while IFS= read -r key; do\n",
                "  [ \"$count\" -ge {max_keys} ] && break\n",
                "  [ -n \"$key\" ] || continue\n",
                "  encoded_key=$(printf '%s' \"$key\" | encode)\n",
                "  ttl=$(redis-cli{cli} --raw -n {database} PTTL \"$key\" 2>/dev/null || printf '0')\n",
                "  case \"$ttl\" in ''|*[!0-9-]*) ttl=0;; esac\n",
                "  [ \"$ttl\" -ge 0 ] || ttl=0\n",
                "  dump=$(redis-cli{cli} --raw -n {database} DUMP \"$key\" 2>/dev/null | head -c -1 | encode)\n",
                "  [ -n \"$dump\" ] || continue\n",
                "  printf '%s\\t%s\\t%s\\n' \"$encoded_key\" \"$ttl\" \"$dump\" >> {path}\n",
                "  count=$((count + 1))\n",
                "done < \"$tmp\"\n",
                "chmod 600 -- {path}\n",
                "printf '__REDIS_TRANSFER__\\texport\\t%s\\n' \"$count\"\n"
            ),
            path = path,
            database = input.database,
            max_keys = max_keys,
            cli = cli_options,
        ),
        "import" => format!(
            concat!(
                "set -eu\n",
                "if ! command -v redis-cli >/dev/null 2>&1; then printf '%s\\n' 'redis-cli not found' >&2; exit 127; fi\n",
                "test -f {path}\n",
                "decode() {{ base64 -d 2>/dev/null || base64 -D 2>/dev/null; }}\n",
                "count=0\n",
                "while IFS=\"$(printf '\\t')\" read -r encoded_key ttl dump; do\n",
                "  [ \"$count\" -ge {max_keys} ] && break\n",
                "  [ -n \"$encoded_key\" ] || continue\n",
                "  key=$(printf '%s' \"$encoded_key\" | decode)\n",
                "  [ -n \"$key\" ] || continue\n",
                "  case \"$ttl\" in ''|*[!0-9]*) ttl=0;; esac\n",
                "  redis-cli{cli} --raw -n {database} DEL \"$key\" >/dev/null\n",
                "  printf '%s' \"$dump\" | decode | redis-cli{cli} --raw -n {database} -x RESTORE \"$key\" \"$ttl\" >/dev/null\n",
                "  count=$((count + 1))\n",
                "done < {path}\n",
                "printf '__REDIS_TRANSFER__\\timport\\t%s\\n' \"$count\"\n"
            ),
            path = path,
            database = input.database,
            max_keys = max_keys,
            cli = cli_options,
        ),
        _ => unreachable!(),
    };
    let result = ssh
        .execute_system(&input.server_id, &command, Duration::from_secs(900))
        .await?;
    if result.exit_code != 0 {
        return Err(AppError::new(
            "REDIS_TRANSFER_FAILED",
            "database",
            "Redis 迁移文件操作失败",
        )
        .details(result.stderr)
        .for_server(input.server_id));
    }
    Ok(RedisTransferResult {
        database: input.database,
        action: input.action,
        path: input.path,
        keys: parse_redis_transfer_count(&result.stdout),
        output: result.stdout,
    })
}

/// 使用 Redis MIGRATE 逐键迁移到目标实例，保留类型和 TTL，适用于跨版本升级且不经过客户端明文缓存。
pub async fn redis_migrate(
    ssh: &SshConnectionManager,
    input: RedisMigrationInput,
) -> AppResult<RedisMigrationResult> {
    validate_redis_migration(&input)?;
    let source_cli_options = redis_cli_options(
        input.source_username.as_deref(),
        input.source_password.as_ref(),
    )?;
    let max_keys = input.max_keys.unwrap_or(10_000).clamp(1, 100_000);
    let target_host = shell_escape(&input.target_host);
    let target_auth = match (&input.target_username, &input.target_password) {
        (Some(username), Some(password)) => format!(
            " AUTH2 {} {}",
            shell_escape(username),
            shell_escape(password.expose_secret())
        ),
        (None, Some(password)) => format!(" AUTH {}", shell_escape(password.expose_secret())),
        (Some(_), None) => {
            return Err(AppError::new(
                "VALIDATION_FAILED",
                "database",
                "Redis 目标用户名必须同时提供密码",
            )
            .for_server(&input.source_server_id))
        }
        (None, None) => String::new(),
    };
    let command = format!(
        concat!(
            "set -eu\n",
            "if ! command -v redis-cli >/dev/null 2>&1; then printf '%s\\n' 'redis-cli not found' >&2; exit 127; fi\n",
            "tmp=$(mktemp)\n",
            "trap 'rm -f -- \"$tmp\"' EXIT\n",
            "redis-cli{source_cli} --raw -n {source_database} --scan > \"$tmp\"\n",
            "count=0\n",
            "while IFS= read -r key; do\n",
            "  [ \"$count\" -ge {max_keys} ] && break\n",
            "  [ -n \"$key\" ] || continue\n",
            "  redis-cli{source_cli} --raw -n {source_database} MIGRATE {target_host} {target_port} \"\" {target_database} 5000 REPLACE{target_auth} KEYS \"$key\" >/dev/null\n",
            "  count=$((count + 1))\n",
            "done < \"$tmp\"\n",
            "printf '__REDIS_MIGRATION__\\t%s\\t%s\\n' \"$count\" {target_database}\n"
        ),
        source_database = input.source_database,
        target_host = target_host,
        target_port = input.target_port,
        target_database = input.target_database,
        target_auth = target_auth,
        max_keys = max_keys,
        source_cli = source_cli_options,
    );
    let result = ssh
        .execute_system(&input.source_server_id, &command, Duration::from_secs(1800))
        .await?;
    if result.exit_code != 0 {
        return Err(
            AppError::new("REDIS_MIGRATION_FAILED", "database", "Redis 跨版本迁移失败")
                .details(result.stderr)
                .for_server(input.source_server_id),
        );
    }
    Ok(RedisMigrationResult {
        source_database: input.source_database,
        target_host: input.target_host,
        target_port: input.target_port,
        target_database: input.target_database,
        keys: parse_redis_migration_count(&result.stdout),
        output: result.stdout,
    })
}

/// 使用 mysqldump 或 pg_dump 将数据库备份到远端指定文件。
pub async fn backup(
    ssh: &SshConnectionManager,
    input: DatabaseBackupInput,
) -> AppResult<DatabaseActionResult> {
    validate_path_input(
        &input.engine,
        &input.name,
        &input.destination,
        input.confirmed,
    )?;
    let destination = shell_escape(&input.destination);
    let name = shell_escape(&input.name);
    let command = match input.engine.as_str() {
        "mysql" | "mariadb" => format!("mkdir -p -- $(dirname -- {destination}) && mysqldump --single-transaction --routines --events --triggers --databases {name} > {destination}"),
        "postgresql" => format!("mkdir -p -- $(dirname -- {destination}) && pg_dump {name} > {destination}"),
        _ => return Err(unsupported("当前引擎不支持 SQL 备份")),
    };
    let result = ssh
        .execute_system(
            &input.server_id,
            &postgres_command(&input.engine, &command),
            Duration::from_secs(300),
        )
        .await?;
    if result.exit_code != 0 {
        return Err(
            AppError::new("DATABASE_BACKUP_FAILED", "database", "数据库备份失败")
                .details(result.stderr)
                .for_server(input.server_id),
        );
    }
    Ok(DatabaseActionResult {
        engine: input.engine,
        name: input.name,
        action: "backup".into(),
        output: format!("备份已写入 {}\n{}", input.destination, result.stdout),
    })
}

/// 使用 mysql 或 psql 从远端 SQL 文件恢复数据库。
pub async fn restore(
    ssh: &SshConnectionManager,
    input: DatabaseRestoreInput,
) -> AppResult<DatabaseActionResult> {
    validate_path_input(&input.engine, &input.name, &input.source, input.confirmed)?;
    let source = shell_escape(&input.source);
    let name = shell_escape(&input.name);
    let command = match input.engine.as_str() {
        "mysql" | "mariadb" => format!("test -f {source} && mysql {name} < {source}"),
        "postgresql" => format!("test -f {source} && psql {name} < {source}"),
        _ => return Err(unsupported("当前引擎不支持 SQL 恢复")),
    };
    let result = ssh
        .execute_system(
            &input.server_id,
            &postgres_command(&input.engine, &command),
            Duration::from_secs(600),
        )
        .await?;
    if result.exit_code != 0 {
        return Err(
            AppError::new("DATABASE_RESTORE_FAILED", "database", "数据库恢复失败")
                .details(result.stderr)
                .for_server(input.server_id),
        );
    }
    Ok(DatabaseActionResult {
        engine: input.engine,
        name: input.name,
        action: "restore".into(),
        output: result.stdout,
    })
}

/// 生成只包含固定数据库客户端的远端探测脚本。
fn probe_command() -> String {
    r#"set +e
engine() { id="$1"; name="$2"; command="$3"; port="$4"; services="$5"; if command -v "$command" >/dev/null 2>&1; then version=$($command --version 2>&1 | head -n 1); running=0; if command -v systemctl >/dev/null 2>&1; then for service in $services; do if systemctl is-active --quiet "$service" || systemctl is-active --quiet "$service.service"; then running=1; break; fi; done; if [ "$id" = "postgresql" ] && systemctl list-units --type=service --state=active --no-legend 'postgresql*' 2>/dev/null | grep -q .; then running=1; fi; elif command -v rc-service >/dev/null 2>&1; then for service in $services; do if rc-service "$service" status >/dev/null 2>&1; then running=1; break; fi; done; fi; printf '__ENGINE__\t%s\t%s\t1\t%s\t%s\t%s\n' "$id" "$name" "$version" "$running" "$port"; else printf '__ENGINE__\t%s\t%s\t0\t\t0\t%s\n' "$id" "$name" "$port"; fi; }
engine mysql MySQL mysql 3306 "mysql mysqld"
engine mariadb MariaDB mariadb 3306 "mariadb mysql"
engine postgresql PostgreSQL psql 5432 "postgresql"
engine redis Redis redis-cli 6379 "redis-server redis"
engine mongodb MongoDB mongod 27017 "mongod"
printf '__DATABASES__\n'
if command -v mysql >/dev/null 2>&1; then mysql --batch --skip-column-names -e 'SELECT SCHEMA_NAME,DEFAULT_CHARACTER_SET_NAME,DEFAULT_COLLATION_NAME FROM information_schema.SCHEMATA' 2>/dev/null | while IFS='	' read -r name charset collation; do printf '__DB__\tmysql\t%s\t%s\t%s\t\n' "$name" "$charset" "$collation"; done; fi
if command -v mariadb >/dev/null 2>&1; then mariadb --batch --skip-column-names -e 'SELECT SCHEMA_NAME,DEFAULT_CHARACTER_SET_NAME,DEFAULT_COLLATION_NAME FROM information_schema.SCHEMATA' 2>/dev/null | while IFS='	' read -r name charset collation; do printf '__DB__\tmariadb\t%s\t%s\t%s\t\n' "$name" "$charset" "$collation"; done; fi
if command -v psql >/dev/null 2>&1; then PSQL='psql'; if [ "$(id -u)" = 0 ] && command -v runuser >/dev/null 2>&1; then PSQL='runuser -u postgres -- psql'; fi; $PSQL --tuples-only --no-align --field-separator '	' -c 'SELECT datname,pg_catalog.pg_get_userbyid(datdba),pg_encoding_to_char(encoding),datcollate FROM pg_database WHERE datistemplate = false' 2>/dev/null | while IFS='	' read -r name owner charset collation; do name=$(printf '%s' "$name" | sed 's/^ *//;s/ *$//'); owner=$(printf '%s' "$owner" | sed 's/^ *//;s/ *$//'); [ -n "$name" ] && printf '__DB__\tpostgresql\t%s\t%s\t%s\t%s\n' "$name" "$charset" "$collation" "$owner"; done; fi
printf '__USERS__\n'
if command -v mysql >/dev/null 2>&1; then mysql --batch --skip-column-names -e "SELECT User,Host,IFNULL(plugin,'') FROM mysql.user WHERE User NOT IN ('mysql.infoschema','mysql.session','mysql.sys')" 2>/dev/null | while IFS='	' read -r username host plugin; do [ -n "$username" ] && printf '__DB_USER__\tmysql\t%s\t%s\t%s\t\n' "$username" "$host" "$plugin"; done; fi
if command -v mariadb >/dev/null 2>&1; then mariadb --batch --skip-column-names -e "SELECT User,Host,IFNULL(plugin,'') FROM mysql.user WHERE User NOT IN ('mysql.infoschema','mysql.session','mysql.sys')" 2>/dev/null | while IFS='	' read -r username host plugin; do [ -n "$username" ] && printf '__DB_USER__\tmariadb\t%s\t%s\t%s\t\n' "$username" "$host" "$plugin"; done; fi
if command -v psql >/dev/null 2>&1; then PSQL='psql'; if [ "$(id -u)" = 0 ] && command -v runuser >/dev/null 2>&1; then PSQL='runuser -u postgres -- psql'; fi; $PSQL --tuples-only --no-align --field-separator '	' -c 'SELECT rolname,rolcanlogin,CASE WHEN rolsuper THEN 1 WHEN rolcreatedb THEN 2 ELSE 0 END FROM pg_roles WHERE rolname NOT LIKE '''pg_%'''' 2>/dev/null | while IFS='	' read -r username can_login privilege; do username=$(printf '%s' "$username" | sed 's/^ *//;s/ *$//'); can_login=$(printf '%s' "$can_login" | sed 's/^ *//;s/ *$//'); privilege=$(printf '%s' "$privilege" | sed 's/^ *//;s/ *$//'); [ -n "$username" ] && printf '__DB_USER__\tpostgresql\t%s\t\t%s\t%s\n' "$username" "$can_login" "$privilege"; done; fi
if command -v redis-cli >/dev/null 2>&1; then redis-cli ACL LIST 2>/dev/null | sed -n 's/^user \([^ ]*\) \([^ ]*\).*$/__DB_USER__\tredis\t\1\t\t\2\t/p'; fi
"#.into()
}

/// 探测远端可用包管理器，避免根据本地平台猜测安装方式。
async fn detect_package_manager(ssh: &SshConnectionManager, server_id: &str) -> AppResult<String> {
    let result = ssh
        .execute_system(
            server_id,
            "if command -v apt-get >/dev/null 2>&1; then printf '__PACKAGE_MANAGER__\\tapt\\n'; elif command -v dnf >/dev/null 2>&1; then printf '__PACKAGE_MANAGER__\\tdnf\\n'; elif command -v yum >/dev/null 2>&1; then printf '__PACKAGE_MANAGER__\\tdnf\\n'; elif command -v apk >/dev/null 2>&1; then printf '__PACKAGE_MANAGER__\\tapk\\n'; elif command -v pacman >/dev/null 2>&1; then printf '__PACKAGE_MANAGER__\\tpacman\\n'; else printf '__PACKAGE_MANAGER__\\tunknown\\n'; fi",
            Duration::from_secs(30),
        )
        .await?;
    if result.exit_code != 0 {
        return Err(AppError::new(
            "PACKAGE_MANAGER_PROBE_FAILED",
            "database",
            "远端包管理器探测失败",
        )
        .details(result.stderr)
        .for_server(server_id));
    }
    parse_package_manager(&result.stdout).ok_or_else(|| {
        AppError::new("UNSUPPORTED_PLATFORM", "database", "远端没有支持的包管理器")
            .for_server(server_id)
    })
}

/// 将固定 marker 解析为规范化的 apt、dnf、apk 或 pacman 标识。
fn parse_package_manager(output: &str) -> Option<String> {
    output.lines().find_map(|line| {
        let mut fields = line.trim().split('\t');
        if fields.next() != Some("__PACKAGE_MANAGER__") {
            return None;
        }
        match fields.next()? {
            "apt" => Some("apt".into()),
            "dnf" | "yum" => Some("dnf".into()),
            "apk" => Some("apk".into()),
            "pacman" => Some("pacman".into()),
            _ => None,
        }
    })
}

/// 返回引擎对应的固定包名和 systemd 服务候选，不接受用户自定义值。
fn install_definition(
    engine: &str,
    package_manager: &str,
) -> AppResult<(Vec<String>, Vec<&'static str>)> {
    let definition = match (engine, package_manager) {
        ("mysql", "apt" | "dnf") => (vec!["mysql-server".into()], vec!["mysql", "mysqld"]),
        ("mysql", "pacman") => (vec!["mariadb".into()], vec!["mariadb"]),
        ("mariadb", "apt" | "dnf") => (vec!["mariadb-server".into()], vec!["mariadb"]),
        ("mariadb", "apk") => (vec!["mariadb".into()], vec!["mariadb"]),
        ("mariadb", "pacman") => (vec!["mariadb".into()], vec!["mariadb"]),
        ("postgresql", "apt") => (vec!["postgresql".into()], vec!["postgresql"]),
        ("postgresql", "dnf") => (vec!["postgresql-server".into()], vec!["postgresql"]),
        ("postgresql", "apk" | "pacman") => (vec!["postgresql".into()], vec!["postgresql"]),
        ("redis", "apt") => (vec!["redis-server".into()], vec!["redis-server", "redis"]),
        ("redis", "dnf" | "apk" | "pacman") => {
            (vec!["redis".into()], vec!["redis", "redis-server"])
        }
        _ => {
            return Err(AppError::new(
                "UNSUPPORTED_ENGINE",
                "database",
                "不支持的数据库引擎或平台",
            ))
        }
    };
    Ok(definition)
}

/// 组合固定包安装、PostgreSQL 初始化和 systemd 启动步骤。
fn install_command(engine: &str, package_command: &str, services: &[&str]) -> String {
    let service_list = services.join(" ");
    let initialize = if engine == "postgresql" {
        "if command -v postgresql-setup >/dev/null 2>&1 && [ ! -f /var/lib/pgsql/data/PG_VERSION ]; then postgresql-setup --initdb; elif command -v initdb >/dev/null 2>&1 && id postgres >/dev/null 2>&1 && [ ! -f /var/lib/postgresql/data/PG_VERSION ]; then install -d -o postgres -g postgres /var/lib/postgresql/data; su -s /bin/sh postgres -c 'initdb -D /var/lib/postgresql/data'; fi;"
    } else {
        ""
    };
    let versioned_postgres = if engine == "postgresql" {
        "if command -v systemctl >/dev/null 2>&1 && ! systemctl is-active --quiet postgresql; then for service in $(systemctl list-unit-files --no-legend 'postgresql*.service' 2>/dev/null | awk '{print $1}'); do systemctl enable --now \"$service\" && break; done; fi;"
    } else {
        ""
    };
    format!(
        "set -e; {package_command}; {initialize} if command -v systemctl >/dev/null 2>&1; then for service in {service_list}; do if systemctl list-unit-files \"$service.service\" >/dev/null 2>&1; then systemctl enable --now \"$service\"; break; fi; done; elif command -v rc-update >/dev/null 2>&1; then for service in {service_list}; do if [ -x \"/etc/init.d/$service\" ]; then rc-update add \"$service\" default; rc-service \"$service\" start; break; fi; done; fi; {versioned_postgres}"
    )
}

/// PostgreSQL 在 root 连接下优先切换为 postgres 系统用户，避免 root 角色不存在。
fn postgres_command(engine: &str, command: &str) -> String {
    if engine == "postgresql" {
        format!("if [ \"$(id -u)\" = 0 ] && command -v runuser >/dev/null 2>&1; then runuser -u postgres -- sh -c {}; else sh -c {}; fi", shell_escape(command), shell_escape(command))
    } else {
        command.to_string()
    }
}

/// 解析固定 marker 格式的数据库探测输出。
fn parse_snapshot(output: &str) -> Option<DatabaseSnapshot> {
    let mut engines = Vec::new();
    let mut databases = Vec::new();
    let mut users = Vec::new();
    for line in output.lines() {
        let fields = line.split('\t').collect::<Vec<_>>();
        match fields.first().copied() {
            Some("__ENGINE__") if fields.len() >= 7 => engines.push(DatabaseEngine {
                id: fields[1].into(),
                name: fields[2].into(),
                installed: fields[3] == "1",
                running: fields[5] == "1",
                version: non_empty(fields[4]),
                port: fields[6].parse().ok(),
            }),
            Some("__DB__") if fields.len() >= 6 => databases.push(DatabaseRecord {
                engine: fields[1].into(),
                name: fields[2].into(),
                charset: non_empty(fields[3]),
                collation: non_empty(fields[4]),
                owner: non_empty(fields[5]),
            }),
            Some("__DB_USER__") if fields.len() >= 6 => users.push(DatabaseUser {
                engine: fields[1].into(),
                username: fields[2].into(),
                host: non_empty(fields[3]),
                privileges: non_empty(fields[4]),
                can_login: fields.get(5).and_then(|value| match value.trim() {
                    "true" | "on" | "yes" => Some(true),
                    "false" | "off" | "no" => Some(false),
                    _ => None,
                }),
            }),
            _ => {}
        }
    }
    if engines.is_empty() {
        return None;
    }
    Some(DatabaseSnapshot {
        engines,
        databases,
        users,
        fetched_at: chrono::Utc::now(),
    })
}

/// 解析数据库权限 marker，丢弃客户端无法证明来源的非 marker 输出。
fn parse_privilege_matrix(output: &str) -> Vec<DatabasePrivilegeEntry> {
    output
        .lines()
        .filter_map(|line| {
            let fields = line.splitn(3, '\t').collect::<Vec<_>>();
            (fields.first().copied() == Some("__DB_PRIV__")
                && fields.get(1).is_some_and(|value| !value.trim().is_empty())
                && fields.get(2).is_some_and(|value| !value.trim().is_empty()))
            .then(|| DatabasePrivilegeEntry {
                database: fields[1].trim().to_string(),
                privileges: fields[2].trim().to_string(),
            })
        })
        .take(500)
        .collect()
}

/// 解析 Redis 扫描 marker，并从 base64 键名恢复可显示的 UTF-8 文本。
fn parse_redis_snapshot(output: &str, database: u8) -> Option<RedisSnapshot> {
    let mut total_keys = None;
    let mut keys = Vec::new();
    for line in output.lines() {
        let fields = line.split('\t').collect::<Vec<_>>();
        match fields.first().copied() {
            Some("__REDIS_DB__") => {
                total_keys = fields.get(1).and_then(|value| value.parse().ok());
            }
            Some("__REDIS_KEY__") if fields.len() >= 5 => {
                let decoded = BASE64.decode(fields[1]).ok()?;
                let key = String::from_utf8(decoded).ok()?;
                if key.contains('\n') || key.contains('\r') || key.contains('\0') {
                    continue;
                }
                keys.push(RedisKeyEntry {
                    key,
                    kind: fields[2].to_string(),
                    ttl_seconds: fields[3].parse().unwrap_or(-1),
                    size_bytes: fields[4].parse().ok(),
                });
            }
            _ => {}
        }
    }
    Some(RedisSnapshot {
        available: total_keys.is_some(),
        database,
        total_keys: total_keys.unwrap_or(0),
        keys,
        fetched_at: chrono::Utc::now(),
    })
}

/// 将本次请求的 Redis 密码转换为受控 CLI 参数；凭据只存在于当前远端命令生命周期内。
fn redis_cli_options(username: Option<&str>, password: Option<&SecretString>) -> AppResult<String> {
    let valid_username = username.is_none_or(|value| {
        !value.is_empty()
            && value.len() <= 128
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    });
    let valid_password = password.is_none_or(|value| {
        let password = value.expose_secret();
        !password.is_empty() && password.len() <= 512 && !password.chars().any(char::is_control)
    });
    if !valid_username || !valid_password || username.is_some() && password.is_none() {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "database",
            "Redis 用户名或密码格式无效；填写用户名时必须同时填写密码",
        ));
    }
    Ok(match (username, password) {
        (Some(username), Some(password)) => format!(
            " --user {} --pass {}",
            shell_escape(username),
            shell_escape(password.expose_secret())
        ),
        (None, Some(password)) => format!(" --pass {}", shell_escape(password.expose_secret())),
        (None, None) | (Some(_), None) => String::new(),
    })
}

/// 校验 Redis 查询范围，限制逻辑库和扫描模式避免无界远程命令。
fn validate_redis_query(input: &RedisQueryInput) -> AppResult<()> {
    if input.database > 15 {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "database",
            "Redis 逻辑库必须在 0-15 范围内",
        )
        .for_server(&input.server_id));
    }
    if let Some(pattern) = input.pattern.as_deref() {
        let pattern = pattern.trim();
        if pattern.is_empty()
            || pattern.len() > 256
            || pattern
                .chars()
                .any(|value| value == '\0' || value == '\n' || value == '\r')
        {
            return Err(
                AppError::new("VALIDATION_FAILED", "database", "Redis 键匹配模式无效")
                    .for_server(&input.server_id),
            );
        }
    }
    Ok(())
}

/// 校验 Redis 删除/清空操作和键名，所有破坏性路径必须显式确认。
fn validate_redis_action(input: &RedisActionInput) -> AppResult<()> {
    if !input.confirmed
        || input.database > 15
        || !matches!(input.action.as_str(), "delete" | "flushdb")
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "database",
            "Redis 数据操作或确认状态无效",
        )
        .for_server(&input.server_id));
    }
    if input.action == "delete" {
        let key = input.key.as_deref().unwrap_or_default();
        if key.is_empty()
            || key.len() > 1024
            || key
                .chars()
                .any(|value| value == '\0' || value == '\n' || value == '\r')
        {
            return Err(
                AppError::new("VALIDATION_FAILED", "database", "Redis 键名无效")
                    .for_server(&input.server_id),
            );
        }
    }
    Ok(())
}

/// 校验 Redis 键值读写边界，字符串值限制在 64 KiB 内并禁止控制字符。
fn validate_redis_value(input: &RedisValueInput) -> AppResult<()> {
    if input.database > 15
        || !matches!(input.action.as_str(), "get" | "set")
        || input.key.is_empty()
        || input.key.len() > 1024
        || input
            .key
            .chars()
            .any(|value| value == '\0' || value == '\n' || value == '\r')
    {
        return Err(
            AppError::new("VALIDATION_FAILED", "database", "Redis 键值请求无效")
                .for_server(&input.server_id),
        );
    }
    if input.action == "set" {
        if !input.confirmed {
            return Err(AppError::new(
                "CONFIRMATION_REQUIRED",
                "database",
                "写入 Redis 值需要明确确认",
            )
            .for_server(&input.server_id));
        }
        let value = input.value.as_deref().unwrap_or_default();
        if value.len() > 65_536 || value.contains('\0') {
            return Err(AppError::new(
                "VALIDATION_FAILED",
                "database",
                "Redis 字符串值过大或包含无效字符",
            )
            .for_server(&input.server_id));
        }
        if input
            .ttl_seconds
            .is_some_and(|ttl| !(0..=31_536_000).contains(&ttl))
        {
            return Err(
                AppError::new("VALIDATION_FAILED", "database", "Redis TTL 超出允许范围")
                    .for_server(&input.server_id),
            );
        }
    }
    Ok(())
}

/// Validates complex Redis mutation scope, field/value bounds, and type-specific operation pairs.
fn validate_redis_complex_action(input: &RedisComplexActionInput) -> AppResult<()> {
    let valid_kind = matches!(input.kind.as_str(), "hash" | "list" | "set" | "zset");
    let valid_action = matches!(
        input.action.as_str(),
        "hash_set"
            | "hash_delete"
            | "list_push_left"
            | "list_push_right"
            | "list_pop_left"
            | "list_pop_right"
            | "set_add"
            | "set_remove"
            | "zset_add"
            | "zset_remove"
    );
    if !input.confirmed
        || input.database > 15
        || input.key.is_empty()
        || input.key.len() > 1024
        || !valid_kind
        || !valid_action
        || input
            .key
            .chars()
            .any(|value| value == '\0' || value == '\n' || value == '\r')
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "database",
            "Redis 复杂值操作范围或确认状态无效",
        )
        .for_server(&input.server_id));
    }
    let expected_prefix = input.kind.as_str();
    if !input.action.starts_with(expected_prefix) {
        return Err(
            AppError::new("VALIDATION_FAILED", "database", "Redis 键类型与操作不匹配")
                .for_server(&input.server_id),
        );
    }
    let text_values = [input.field.as_deref(), input.value.as_deref()];
    if text_values.iter().flatten().any(|value| {
        value.len() > 65_536
            || value
                .chars()
                .any(|character| character == '\0' || character == '\n' || character == '\r')
    }) {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "database",
            "Redis 复杂值字段或成员无效",
        )
        .for_server(&input.server_id));
    }
    if matches!(input.action.as_str(), "hash_set" | "hash_delete")
        && input.field.as_deref().unwrap_or_default().is_empty()
    {
        return Err(
            AppError::new("VALIDATION_FAILED", "database", "Redis hash 字段不能为空")
                .for_server(&input.server_id),
        );
    }
    if matches!(
        input.action.as_str(),
        "list_push_left"
            | "list_push_right"
            | "set_add"
            | "set_remove"
            | "zset_add"
            | "zset_remove"
    ) && input.value.as_deref().unwrap_or_default().is_empty()
    {
        return Err(
            AppError::new("VALIDATION_FAILED", "database", "Redis 成员不能为空")
                .for_server(&input.server_id),
        );
    }
    if input.action == "hash_set" && input.value.is_none() {
        return Err(
            AppError::new("VALIDATION_FAILED", "database", "Redis hash 值不能为空")
                .for_server(&input.server_id),
        );
    }
    if input.action == "zset_add" && input.score.is_none_or(|score| !score.is_finite()) {
        return Err(
            AppError::new("VALIDATION_FAILED", "database", "Redis zset 分数无效")
                .for_server(&input.server_id),
        );
    }
    Ok(())
}

/// 校验 Redis 远端迁移路径、动作和规模，避免无界读取或把路径解释为命令片段。
fn validate_redis_transfer(input: &RedisTransferInput) -> AppResult<()> {
    if !input.confirmed
        || input.database > 15
        || !matches!(input.action.as_str(), "export" | "import")
        || input.path.trim().is_empty()
        || input.path.len() > 4096
        || !input.path.starts_with('/')
        || input.path.contains("..")
        || input
            .path
            .chars()
            .any(|value| value == '\0' || value == '\n' || value == '\r')
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "database",
            "Redis 迁移动作或远端路径无效",
        )
        .for_server(&input.server_id));
    }
    if input
        .max_keys
        .is_some_and(|value| value == 0 || value > 100_000)
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "database",
            "Redis 迁移键数量必须在 1-100000 范围内",
        )
        .for_server(&input.server_id));
    }
    Ok(())
}

/// 校验 Redis MIGRATE 目标和规模，拒绝控制字符、非法端口和未确认的跨实例写入。
fn validate_redis_migration(input: &RedisMigrationInput) -> AppResult<()> {
    let valid_username = input.target_username.as_deref().is_none_or(|value| {
        !value.is_empty()
            && value.len() <= 128
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    });
    let valid_password = input.target_password.as_ref().is_none_or(|value| {
        let password = value.expose_secret();
        !password.is_empty() && password.len() <= 512 && !password.contains(['\0', '\n', '\r'])
    });
    if !input.confirmed
        || input.source_server_id.trim().is_empty()
        || input.source_database > 15
        || input.target_database > 15
        || input.target_port == 0
        || input.target_host.is_empty()
        || input.target_host.len() > 255
        || !input.target_host.chars().all(|value| {
            value.is_ascii_alphanumeric() || matches!(value, '.' | ':' | '-' | '_' | '%')
        })
        || !valid_username
        || !valid_password
        || input.target_username.is_some() != input.target_password.is_some()
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "database",
            "Redis 跨版本迁移目标无效或未确认",
        )
        .for_server(&input.source_server_id));
    }
    if input
        .max_keys
        .is_some_and(|value| value == 0 || value > 100_000)
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "database",
            "Redis 迁移键数量必须在 1-100000 范围内",
        )
        .for_server(&input.source_server_id));
    }
    Ok(())
}

/// 从远端 marker 输出解析本次 Redis 迁移的键数量。
fn parse_redis_transfer_count(output: &str) -> u64 {
    output
        .lines()
        .find_map(|line| {
            let mut fields = line.split('\t');
            (fields.next() == Some("__REDIS_TRANSFER__"))
                .then(|| fields.nth(1).and_then(|value| value.parse().ok()))
                .flatten()
        })
        .unwrap_or(0)
}

/// 从 Redis MIGRATE marker 中解析迁移键数量，忽略远端 CLI 的其他输出。
fn parse_redis_migration_count(output: &str) -> u64 {
    output
        .lines()
        .find_map(|line| {
            let mut fields = line.split('\t');
            (fields.next() == Some("__REDIS_MIGRATION__"))
                .then(|| fields.next().and_then(|value| value.parse().ok()))
                .flatten()
        })
        .unwrap_or(0)
}

/// 丢弃空字段，保持前端对未知版本和属主的显示一致。
fn non_empty(value: &str) -> Option<String> {
    (!value.trim().is_empty()).then(|| value.trim().to_string())
}

/// 将未知数据库引擎转换为带服务器上下文的结构化校验错误。
fn invalid_engine(engine: &str, server_id: &str) -> AppError {
    AppError::new("VALIDATION_FAILED", "database", "未知数据库引擎")
        .details(engine)
        .for_server(server_id)
}

/// 构造数据库账号 SQL；所有标识符和密码在进入命令前分别完成 SQL 字面量校验。
fn database_user_command(input: &DatabaseUserActionInput) -> AppResult<String> {
    let username = sql_string_literal(&input.username);
    let host = sql_string_literal(input.host.as_deref().unwrap_or("%"));
    let password = input
        .password
        .as_ref()
        .map(|value| sql_string_literal(value.expose_secret()));
    match (input.engine.as_str(), input.action.as_str()) {
        ("mysql" | "mariadb", "create") => {
            let mut command = format!("CREATE USER IF NOT EXISTS {username}@{host}");
            if let Some(password) = password {
                command.push_str(&format!(" IDENTIFIED BY {password}"));
            }
            command.push(';');
            if let Some(grant) = mysql_grant_clause(input)? {
                command.push_str(&grant);
            }
            Ok(format!(
                "mysql --batch --skip-column-names -e {}",
                shell_escape(&command)
            ))
        }
        ("mysql" | "mariadb", "reset_password") => {
            let password = password.ok_or_else(|| {
                AppError::new("VALIDATION_FAILED", "database", "重置密码必须填写新密码")
            })?;
            Ok(format!(
                "mysql --batch --skip-column-names -e {}",
                shell_escape(&format!(
                    "ALTER USER {username}@{host} IDENTIFIED BY {password};"
                ))
            ))
        }
        ("mysql" | "mariadb", "drop") => Ok(format!(
            "mysql --batch --skip-column-names -e {}",
            shell_escape(&format!("DROP USER IF EXISTS {username}@{host};"))
        )),
        ("mysql" | "mariadb", "grant" | "revoke") => {
            let grant = mysql_grant_clause(input)?.ok_or_else(|| {
                AppError::new(
                    "VALIDATION_FAILED",
                    "database",
                    "授权操作必须填写数据库和权限",
                )
            })?;
            let statement = if input.action == "grant" {
                grant
            } else {
                grant
                    .replacen("GRANT", "REVOKE", 1)
                    .replace(" TO ", " FROM ")
            };
            Ok(format!(
                "mysql --batch --skip-column-names -e {}",
                shell_escape(&statement)
            ))
        }
        ("postgresql", "create") => {
            let name = sql_identifier(&input.username)?;
            let mut statement = format!("CREATE ROLE {name} LOGIN");
            if let Some(password) = password {
                statement.push_str(&format!(" PASSWORD {password}"));
            }
            statement.push(';');
            Ok(format!(
                "psql --tuples-only --no-align -c {}",
                shell_escape(&statement)
            ))
        }
        ("postgresql", "reset_password") => {
            let name = sql_identifier(&input.username)?;
            let password = password.ok_or_else(|| {
                AppError::new("VALIDATION_FAILED", "database", "重置密码必须填写新密码")
            })?;
            Ok(format!(
                "psql --tuples-only --no-align -c {}",
                shell_escape(&format!("ALTER ROLE {name} PASSWORD {password};"))
            ))
        }
        ("postgresql", "drop") => {
            let name = sql_identifier(&input.username)?;
            Ok(format!(
                "psql --tuples-only --no-align -c {}",
                shell_escape(&format!("DROP ROLE IF EXISTS {name};"))
            ))
        }
        ("postgresql", "grant" | "revoke") => {
            let name = sql_identifier(&input.username)?;
            let database = sql_identifier(input.database.as_deref().unwrap_or_default())?;
            let privileges = postgres_privileges(input.privileges.as_deref().unwrap_or_default())?;
            let verb = if input.action == "grant" {
                "GRANT"
            } else {
                "REVOKE"
            };
            Ok(format!(
                "psql --tuples-only --no-align -c {}",
                shell_escape(&format!(
                    "{verb} {privileges} ON DATABASE {database} TO {name};"
                ))
            ))
        }
        ("redis", "create") => {
            let password = input.password.as_ref().ok_or_else(|| {
                AppError::new("VALIDATION_FAILED", "database", "Redis 用户必须填写密码")
            })?;
            let rules = redis_acl_rules("create", input.privileges.as_deref())?;
            Ok(format!(
                "redis-cli ACL SETUSER {} on resetpass {} {}",
                shell_escape(&input.username),
                shell_escape(&format!(">{}", password.expose_secret())),
                rules
                    .iter()
                    .map(|rule| shell_escape(rule))
                    .collect::<Vec<_>>()
                    .join(" ")
            ))
        }
        ("redis", "drop") => Ok(format!(
            "redis-cli ACL DELUSER {}",
            shell_escape(&input.username)
        )),
        ("redis", "reset_password") => {
            let password = input.password.as_ref().ok_or_else(|| {
                AppError::new("VALIDATION_FAILED", "database", "Redis 用户必须填写新密码")
            })?;
            Ok(format!(
                "redis-cli ACL SETUSER {} resetpass {}",
                shell_escape(&input.username),
                shell_escape(&format!(">{}", password.expose_secret()))
            ))
        }
        ("redis", "grant" | "revoke") => {
            let rules = redis_acl_rules(&input.action, input.privileges.as_deref())?;
            Ok(format!(
                "redis-cli ACL SETUSER {} {}",
                shell_escape(&input.username),
                rules
                    .iter()
                    .map(|rule| shell_escape(rule))
                    .collect::<Vec<_>>()
                    .join(" ")
            ))
        }
        ("redis", _) => Err(unsupported("不支持的 Redis ACL 操作")),
        _ => Err(unsupported("不支持的数据库引擎或账号操作")),
    }
}

/// 将界面中的 Redis ACL 权限短语转换为安全的 ACL SETUSER 规则。
///
/// 规则只允许命令、命令分类、键/频道模式和官方聚合规则；禁止控制字符与任意
/// shell 片段。grant/create 默认把无前缀命令视为允许，revoke 则视为拒绝。
fn redis_acl_rules(action: &str, privileges: Option<&str>) -> AppResult<Vec<String>> {
    let tokens = privileges
        .unwrap_or_default()
        .split(|character: char| character == ',' || character.is_whitespace())
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    if tokens.len() > 64 {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "database",
            "Redis ACL 规则数量超出限制",
        ));
    }
    if action == "create" && tokens.is_empty() {
        return Ok(vec!["~*".into(), "+@all".into()]);
    }
    if !matches!(action, "create" | "grant" | "revoke") || tokens.is_empty() {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "database",
            "Redis ACL 规则不能为空或动作无效",
        ));
    }
    let mut rules = Vec::with_capacity(tokens.len() + 1);
    let mut has_key_rule = false;
    let mut has_command_rule = false;
    for token in tokens {
        if token.len() > 128
            || token.chars().any(|character| {
                character.is_control() || character == ',' || character.is_whitespace()
            })
        {
            return Err(AppError::new(
                "VALIDATION_FAILED",
                "database",
                "Redis ACL 规则包含无效字符",
            ));
        }
        let lower = token.to_ascii_lowercase();
        if matches!(
            lower.as_str(),
            "allkeys" | "resetkeys" | "allchannels" | "resetchannels"
        ) {
            if action == "revoke" && lower == "allkeys" {
                rules.push("resetkeys".into());
            } else if action == "revoke" && lower == "allchannels" {
                rules.push("resetchannels".into());
            } else {
                rules.push(lower.clone());
            }
            has_key_rule |= matches!(lower.as_str(), "allkeys" | "resetkeys");
            continue;
        }
        if matches!(lower.as_str(), "allcommands" | "nocommands") {
            if action == "revoke" && lower == "allcommands" {
                rules.push("nocommands".into());
            } else {
                rules.push(lower);
            }
            has_command_rule = true;
            continue;
        }
        if let Some(pattern) = token.strip_prefix('~') {
            if pattern.is_empty() || action == "revoke" {
                return Err(AppError::new(
                    "VALIDATION_FAILED",
                    "database",
                    "Redis ACL 键模式只能用于创建或授予权限",
                ));
            }
            rules.push(token.to_string());
            has_key_rule = true;
            continue;
        }
        if let Some(pattern) = token.strip_prefix('&') {
            if pattern.is_empty() || action == "revoke" {
                return Err(AppError::new(
                    "VALIDATION_FAILED",
                    "database",
                    "Redis ACL 频道模式只能用于创建或授予权限",
                ));
            }
            rules.push(token.to_string());
            continue;
        }
        let (prefix, body) = match token.as_bytes().first().copied() {
            Some(b'+') | Some(b'-') => (token[..1].to_string(), &token[1..]),
            _ => (String::new(), token),
        };
        if body.is_empty()
            || !body
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'@' | b'_' | b'-'))
            || body == "@"
        {
            return Err(AppError::new(
                "VALIDATION_FAILED",
                "database",
                "Redis ACL 命令或分类无效",
            ));
        }
        let is_category = body.starts_with('@');
        let explicit_negative = prefix == "-";
        let normalized = if action == "revoke" {
            if explicit_negative {
                token.to_string()
            } else {
                format!("-{body}")
            }
        } else if prefix.is_empty() {
            format!("+{body}")
        } else {
            format!("{prefix}{body}")
        };
        has_command_rule = true;
        if is_category || !body.is_empty() {
            rules.push(normalized);
        }
    }
    if action == "create" {
        if !has_key_rule {
            rules.push("~*".into());
        }
        if !has_command_rule {
            rules.push("+@all".into());
        }
    }
    Ok(rules)
}

/// 构造 MySQL/MariaDB 的 GRANT 语句，仅开放常用权限集合和数据库级授权范围。
fn mysql_grant_clause(input: &DatabaseUserActionInput) -> AppResult<Option<String>> {
    let Some(database) = input.database.as_deref() else {
        return Ok(None);
    };
    let database = mysql_identifier(database)?;
    let privileges = mysql_privileges(input.privileges.as_deref().unwrap_or_default())?;
    let username = sql_string_literal(&input.username);
    let host = sql_string_literal(input.host.as_deref().unwrap_or("%"));
    Ok(Some(format!(
        "GRANT {privileges} ON {database}.* TO {username}@{host};"
    )))
}

/// 校验数据库账号请求，阻止系统账号、危险 SQL 片段和未确认的远端变更。
fn validate_user_action(input: &DatabaseUserActionInput) -> AppResult<()> {
    if !input.confirmed {
        return Err(AppError::new(
            "CONFIRMATION_REQUIRED",
            "database",
            "请先确认数据库账号变更",
        ));
    }
    if !matches!(
        input.engine.as_str(),
        "mysql" | "mariadb" | "postgresql" | "redis"
    ) || !matches!(
        input.action.as_str(),
        "create" | "drop" | "grant" | "revoke" | "reset_password"
    ) || !valid_identifier(&input.username)
        || matches!(input.username.as_str(), "root" | "postgres" | "redis")
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "database",
            "数据库账号引擎、动作或名称无效",
        ));
    }
    if let Some(host) = input.host.as_deref() {
        if host.is_empty() || host.len() > 253 || !valid_host(host) {
            return Err(AppError::new(
                "VALIDATION_FAILED",
                "database",
                "数据库账号主机范围无效",
            ));
        }
    }
    if input.engine == "redis" && input.host.is_some() {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "database",
            "Redis ACL 不使用来源主机字段",
        ));
    }
    if (input.action == "create" || input.action == "reset_password")
        && input.password.as_ref().is_some_and(|value| {
            value.expose_secret().is_empty() || value.expose_secret().len() > 256
        })
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "database",
            "数据库密码长度无效",
        ));
    }
    if input.password.as_ref().is_some_and(|value| {
        value
            .expose_secret()
            .chars()
            .any(|character| character.is_control())
    }) {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "database",
            "数据库密码不能包含控制字符",
        ));
    }
    if input.engine == "redis" {
        if matches!(input.action.as_str(), "create" | "reset_password") && input.password.is_none()
        {
            return Err(AppError::new(
                "VALIDATION_FAILED",
                "database",
                "Redis 创建或重置密码必须填写密码",
            ));
        }
        if matches!(input.action.as_str(), "grant" | "revoke") {
            redis_acl_rules(&input.action, input.privileges.as_deref())?;
        }
    }
    Ok(())
}

/// 校验权限矩阵查询的引擎、账号和 Redis 会话凭据。
fn validate_privilege_input(input: &DatabasePrivilegeInput) -> AppResult<()> {
    if !matches!(
        input.engine.as_str(),
        "mysql" | "mariadb" | "postgresql" | "redis"
    ) || !valid_identifier(&input.username)
        || matches!(input.username.as_str(), "root" | "postgres" | "redis")
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "database",
            "数据库权限查询账号或引擎无效",
        )
        .for_server(&input.server_id));
    }
    if input
        .host
        .as_deref()
        .is_some_and(|host| host.is_empty() || host.len() > 253 || !valid_host(host))
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "database",
            "数据库权限查询主机范围无效",
        )
        .for_server(&input.server_id));
    }
    if input.engine == "redis" {
        let redis_user = input.redis_username.as_deref().unwrap_or(&input.username);
        if !valid_identifier(redis_user) {
            return Err(
                AppError::new("VALIDATION_FAILED", "database", "Redis ACL 用户名无效")
                    .for_server(&input.server_id),
            );
        }
        redis_cli_options(
            input.redis_username.as_deref(),
            input.redis_password.as_ref(),
        )
        .map(|_| ())
        .map_err(|error| error.for_server(&input.server_id))?;
    }
    Ok(())
}

/// 校验并标准化 MySQL/MariaDB 数据库标识符。
fn mysql_identifier(value: &str) -> AppResult<String> {
    if valid_identifier(value) {
        Ok(format!("`{value}`"))
    } else {
        Err(AppError::new(
            "VALIDATION_FAILED",
            "database",
            "数据库标识符无效",
        ))
    }
}

/// 校验并双引号包裹 PostgreSQL 标识符。
fn sql_identifier(value: &str) -> AppResult<String> {
    if valid_identifier(value) {
        Ok(format!("\"{value}\""))
    } else {
        Err(AppError::new(
            "VALIDATION_FAILED",
            "database",
            "数据库标识符无效",
        ))
    }
}

/// 将 SQL 字符串转换为单引号字面量；控制字符已在入口校验。
fn sql_string_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

/// 仅允许常见 MySQL 权限名称，避免把任意 SQL 片段写入 GRANT。
fn mysql_privileges(value: &str) -> AppResult<String> {
    let allowed = [
        "SELECT",
        "INSERT",
        "UPDATE",
        "DELETE",
        "CREATE",
        "ALTER",
        "INDEX",
        "DROP",
        "EXECUTE",
        "REFERENCES",
        "ALL PRIVILEGES",
    ];
    let values = value
        .split(',')
        .map(|item| item.trim().to_ascii_uppercase())
        .filter(|item| !item.is_empty())
        .collect::<Vec<_>>();
    if values.is_empty() || values.iter().any(|item| !allowed.contains(&item.as_str())) {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "database",
            "MySQL 权限名称无效",
        ));
    }
    Ok(values.join(", "))
}

/// 仅允许 PostgreSQL 数据库级常用权限。
fn postgres_privileges(value: &str) -> AppResult<String> {
    let allowed = ["CONNECT", "CREATE", "TEMPORARY", "TEMP"];
    let values = value
        .split(',')
        .map(|item| item.trim().to_ascii_uppercase())
        .filter(|item| !item.is_empty())
        .collect::<Vec<_>>();
    if values.is_empty() || values.iter().any(|item| !allowed.contains(&item.as_str())) {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "database",
            "PostgreSQL 权限名称无效",
        ));
    }
    Ok(values.join(", "))
}

/// 校验数据库操作只允许固定引擎、动作和安全标识符。
fn validate_action(input: &DatabaseActionInput) -> AppResult<()> {
    if !input.confirmed {
        return Err(AppError::new(
            "CONFIRMATION_REQUIRED",
            "database",
            "请先确认数据库变更",
        ));
    }
    if !matches!(
        input.engine.as_str(),
        "mysql" | "mariadb" | "postgresql" | "redis"
    ) || !matches!(input.action.as_str(), "create" | "drop")
        || !valid_identifier(&input.name)
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "database",
            "数据库引擎、操作或名称无效",
        ));
    }
    if input.action == "drop"
        && matches!(
            input.name.as_str(),
            "information_schema"
                | "performance_schema"
                | "mysql"
                | "sys"
                | "postgres"
                | "template0"
                | "template1"
        )
    {
        return Err(AppError::new(
            "PROTECTED_DATABASE",
            "database",
            "系统数据库不可删除",
        ));
    }
    Ok(())
}

/// 校验备份/恢复路径为绝对路径，避免把文件写到客户端工作目录。
fn validate_path_input(engine: &str, name: &str, path: &str, confirmed: bool) -> AppResult<()> {
    if !confirmed {
        return Err(AppError::new(
            "CONFIRMATION_REQUIRED",
            "database",
            "请先确认数据库文件操作",
        ));
    }
    if !matches!(engine, "mysql" | "mariadb" | "postgresql")
        || !valid_identifier(name)
        || !path.starts_with('/')
        || path.contains('\n')
        || path.contains('\r')
        || path.contains("..")
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "database",
            "数据库名称或远端文件路径无效",
        ));
    }
    Ok(())
}

/// 复用 SQL 标识符的严格字符集，确保名称不会进入 shell 或 SQL 控制语法。
fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .as_bytes()
            .first()
            .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
}

/// 校验 MySQL 账号来源主机的有限字符集。
fn valid_host(value: &str) -> bool {
    value.bytes().all(|byte| {
        byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b':' | b'%' | b'_')
    })
}

/// 创建统一的不支持能力错误。
fn unsupported(message: &str) -> AppError {
    AppError::new("UNSUPPORTED_DATABASE", "database", message)
}

#[cfg(test)]
mod tests {
    use base64::Engine as _;
    use secrecy::SecretString;

    use super::{
        database_user_command, diagnose_privilege_matrix, install_command, install_definition,
        parse_package_manager, parse_privilege_matrix, parse_redis_diagnostic,
        parse_redis_migration_count, parse_redis_snapshot, parse_redis_transfer_count,
        parse_snapshot, redis_acl_rules, redis_cli_options, valid_identifier,
        validate_privilege_input, validate_redis_action, validate_redis_complex_action,
        validate_redis_migration, validate_redis_query, validate_redis_transfer,
        validate_redis_value, validate_user_action, DatabasePrivilegeInput,
        DatabasePrivilegeSnapshot, DatabaseUserActionInput, RedisActionInput,
        RedisComplexActionInput, RedisMigrationInput, RedisQueryInput, RedisTransferInput,
        RedisValueInput,
    };

    #[test]
    fn parses_engine_and_database_markers() {
        let snapshot = parse_snapshot("__ENGINE__\tmysql\tMySQL\t1\t8.0\t1\t3306\n__ENGINE__\tredis\tRedis\t0\t\t0\t6379\n__DB__\tmysql\tapp\tutf8mb4\tutf8mb4_general_ci\t\n__DB_USER__\tmysql\tapp_user\tlocalhost\tSELECT\t\n").unwrap();
        assert!(snapshot.engines[0].installed);
        assert!(!snapshot.engines[1].installed);
        assert_eq!(snapshot.databases[0].name, "app");
        assert_eq!(snapshot.users[0].username, "app_user");
    }

    /// 解析权限矩阵 marker，并忽略数据库 CLI 的非结构化噪声。
    #[test]
    fn parses_database_privilege_matrix_markers() {
        let entries = parse_privilege_matrix(
            "notice\n__DB_PRIV__\tapp\tSELECT,INSERT\n__DB_PRIV__\tpostgres\tCONNECT,CREATE\n__DB_PRIV__\t\tignored\n__DB_PRIV__\tcache\t\n",
        );
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].database, "app");
        assert_eq!(entries[0].privileges, "SELECT,INSERT");
        assert_eq!(entries[1].database, "postgres");
    }

    /// 验证权限矩阵诊断能识别通配主机、全库授权和 ALL 权限。
    #[test]
    fn diagnose_privilege_matrix_flags_broad_grants() {
        let snapshot = DatabasePrivilegeSnapshot {
            engine: "mysql".into(),
            username: "app_user".into(),
            host: Some("%".into()),
            entries: vec![
                super::DatabasePrivilegeEntry {
                    database: "app".into(),
                    privileges: "SELECT,UPDATE".into(),
                },
                super::DatabasePrivilegeEntry {
                    database: "*.*".into(),
                    privileges: "ALL PRIVILEGES".into(),
                },
            ],
            fetched_at: chrono::Utc::now(),
        };
        let diagnostics = diagnose_privilege_matrix(&snapshot);
        let categories: Vec<_> = diagnostics
            .iter()
            .map(|item| item.category.as_str())
            .collect();
        assert!(categories.contains(&"host"));
        assert!(categories.contains(&"scope"));
        assert!(categories.contains(&"privilege"));
    }

    /// 验证 Redis ACL 过宽规则（~*、+@all）会被标记为告警。
    #[test]
    fn diagnose_privilege_matrix_flags_broad_redis_acl() {
        let snapshot = DatabasePrivilegeSnapshot {
            engine: "redis".into(),
            username: "ai".into(),
            host: None,
            entries: vec![super::DatabasePrivilegeEntry {
                database: "~*".into(),
                privileges: "+@all".into(),
            }],
            fetched_at: chrono::Utc::now(),
        };
        let diagnostics = diagnose_privilege_matrix(&snapshot);
        assert!(diagnostics.iter().any(|item| item.category == "privilege"));
        assert!(diagnostics.iter().any(|item| item.category == "scope"));
    }

    /// 验证 MySQL/MariaDB 的 GRANT OPTION 会被标记为可继续授权的告警。
    #[test]
    fn diagnose_privilege_matrix_flags_mysql_grant_option() {
        let snapshot = DatabasePrivilegeSnapshot {
            engine: "mysql".into(),
            username: "app_writer".into(),
            host: Some("10.0.0.5".into()),
            entries: vec![super::DatabasePrivilegeEntry {
                database: "app".into(),
                privileges: "SELECT,INSERT,GRANT OPTION".into(),
            }],
            fetched_at: chrono::Utc::now(),
        };
        let diagnostics = diagnose_privilege_matrix(&snapshot);
        assert!(diagnostics
            .iter()
            .any(|item| item.category == "delegation" && item.severity == "warning"));
    }

    /// 验证 PostgreSQL 在多个数据库上拥有 CREATE 会被标记为范围较广。
    #[test]
    fn diagnose_privilege_matrix_flags_postgres_wide_create() {
        let snapshot = DatabasePrivilegeSnapshot {
            engine: "postgresql".into(),
            username: "etl".into(),
            host: None,
            entries: vec![
                super::DatabasePrivilegeEntry {
                    database: "warehouse".into(),
                    privileges: "CONNECT,CREATE".into(),
                },
                super::DatabasePrivilegeEntry {
                    database: "staging".into(),
                    privileges: "CONNECT,CREATE".into(),
                },
            ],
            fetched_at: chrono::Utc::now(),
        };
        let diagnostics = diagnose_privilege_matrix(&snapshot);
        assert!(diagnostics
            .iter()
            .any(|item| item.category == "scope" && item.message.contains("2 个数据库")));
    }

    /// 校验不同数据库引擎的权限查询边界，确保系统账号和 Redis 缺失凭据会被拒绝。
    #[test]
    fn validates_database_privilege_scope() {
        let mysql = DatabasePrivilegeInput {
            server_id: "server".into(),
            engine: "mysql".into(),
            username: "app_user".into(),
            host: Some("%".into()),
            redis_username: None,
            redis_password: None,
        };
        assert!(validate_privilege_input(&mysql).is_ok());

        let mut system_user = mysql.clone();
        system_user.username = "root".into();
        assert!(validate_privilege_input(&system_user).is_err());

        let mut unsafe_host = mysql;
        unsafe_host.host = Some("%'; DROP DATABASE app;--".into());
        assert!(validate_privilege_input(&unsafe_host).is_err());

        let redis = DatabasePrivilegeInput {
            server_id: "server".into(),
            engine: "redis".into(),
            username: "app_acl".into(),
            host: None,
            redis_username: Some("default".into()),
            redis_password: Some(SecretString::from("secret")),
        };
        assert!(validate_privilege_input(&redis).is_ok());
        let mut missing_password = redis;
        missing_password.redis_password = None;
        assert!(validate_privilege_input(&missing_password).is_err());
    }

    #[test]
    fn rejects_shell_control_identifiers() {
        assert!(valid_identifier("demo_db"));
        assert!(!valid_identifier("demo; rm -rf /"));
        assert!(!valid_identifier(""));
    }

    #[test]
    fn builds_safe_user_commands() {
        let input = DatabaseUserActionInput {
            server_id: "server".into(),
            engine: "mysql".into(),
            username: "app_user".into(),
            host: Some("localhost".into()),
            database: Some("app".into()),
            privileges: Some("SELECT,INSERT".into()),
            password: Some(SecretString::from("p'a$$".to_string())),
            action: "create".into(),
            confirmed: true,
        };
        assert!(validate_user_action(&input).is_ok());
        let command = database_user_command(&input).unwrap();
        assert!(command.contains("CREATE USER"));
        assert!(command.contains("p'\"'\"'"));
        assert!(command.contains("a$$"));
        assert!(!command.contains("DROP USER"));
    }

    /// 验证 Redis ACL 规则会按动作转换为允许/拒绝规则，并拒绝未受控的键模式撤销。
    #[test]
    fn builds_safe_redis_acl_commands() {
        let create = DatabaseUserActionInput {
            server_id: "server".into(),
            engine: "redis".into(),
            username: "app_user".into(),
            host: None,
            database: None,
            privileges: Some("GET,SET,~app:*".into()),
            password: Some(SecretString::from("secret")),
            action: "create".into(),
            confirmed: true,
        };
        assert!(validate_user_action(&create).is_ok());
        let command = database_user_command(&create).unwrap();
        assert!(command.contains("ACL SETUSER"));
        assert!(command.contains("+GET"));
        assert!(command.contains("+SET"));
        assert!(command.contains("~app:*"));
        assert!(!command.contains("resetkeys"));

        assert_eq!(
            redis_acl_rules("revoke", Some("GET,@dangerous")).unwrap(),
            vec!["-GET", "-@dangerous"]
        );
        assert!(redis_acl_rules("revoke", Some("~app:*")).is_err());
        assert!(redis_acl_rules("grant", Some("GET;FLUSHALL")).is_err());
    }

    #[test]
    fn parses_package_manager_markers() {
        assert_eq!(
            parse_package_manager("noise\n__PACKAGE_MANAGER__\tapt\n").as_deref(),
            Some("apt")
        );
        assert_eq!(
            parse_package_manager("__PACKAGE_MANAGER__\tyum\n").as_deref(),
            Some("dnf")
        );
        assert_eq!(
            parse_package_manager("__PACKAGE_MANAGER__\tapk\n").as_deref(),
            Some("apk")
        );
        assert_eq!(
            parse_package_manager("__PACKAGE_MANAGER__\tpacman\n").as_deref(),
            Some("pacman")
        );
        assert!(parse_package_manager("__PACKAGE_MANAGER__\tunknown\n").is_none());
    }

    #[test]
    fn builds_fixed_database_install_commands() {
        let (packages, services) = install_definition("postgresql", "apt").unwrap();
        assert_eq!(packages, vec!["postgresql"]);
        assert_eq!(services, vec!["postgresql"]);
        let command = install_command("postgresql", "apt-get install -y -- postgresql", &services);
        assert!(command.contains("postgresql-setup"));
        assert!(command.contains("postgresql*.service"));
        assert!(command.contains("systemctl enable --now"));
        let (_, alpine_services) = install_definition("redis", "apk").unwrap();
        let alpine_command =
            install_command("redis", "apk add --no-cache -- redis", &alpine_services);
        assert!(alpine_command.contains("rc-update add"));
        assert!(alpine_command.contains("rc-service"));
    }

    /// 验证 Debian/RHEL/Alpine/Arch 包管理器都映射到固定数据库包和服务候选。
    #[test]
    fn supports_database_install_definitions_on_supported_managers() {
        for package_manager in ["apt", "dnf", "apk", "pacman"] {
            for engine in ["mysql", "mariadb", "postgresql", "redis"] {
                let definition = install_definition(engine, package_manager);
                if package_manager == "apk" && engine == "mysql" {
                    assert!(definition.is_err());
                } else {
                    let (packages, services) = definition.unwrap();
                    assert!(!packages.is_empty());
                    assert!(!services.is_empty());
                }
            }
        }
    }

    #[test]
    fn parses_redis_key_markers_without_values() {
        let key = base64::engine::general_purpose::STANDARD.encode("cache:user:1");
        let snapshot = parse_redis_snapshot(
            &format!("__REDIS_DB__\t2\n__REDIS_KEY__\t{key}\tstring\t120\t64\n"),
            2,
        )
        .unwrap();
        assert!(snapshot.available);
        assert_eq!(snapshot.total_keys, 2);
        assert_eq!(snapshot.keys[0].key, "cache:user:1");
        assert_eq!(snapshot.keys[0].ttl_seconds, 120);
    }

    #[test]
    fn validates_redis_scope_and_destructive_actions() {
        let query = RedisQueryInput {
            server_id: "server".into(),
            database: 16,
            pattern: None,
            limit: None,
            username: None,
            password: None,
        };
        assert!(validate_redis_query(&query).is_err());
        let action = RedisActionInput {
            server_id: "server".into(),
            database: 0,
            action: "flushdb".into(),
            key: None,
            confirmed: false,
            username: None,
            password: None,
        };
        assert!(validate_redis_action(&action).is_err());
    }

    /// 验证 Redis 默认用户密码和 ACL 用户密码都会生成安全参数，缺失配对字段会拒绝。
    #[test]
    fn validates_redis_authentication() {
        let password = SecretString::from("p'a$$".to_string());
        let options = redis_cli_options(Some("app_user"), Some(&password)).unwrap();
        assert!(options.contains("--user"));
        assert!(options.contains("app_user"));
        assert!(options.contains("--pass"));
        assert!(redis_cli_options(Some("app_user"), None).is_err());
        assert!(redis_cli_options(None, Some(&SecretString::from("line\nfeed"))).is_err());
    }

    #[test]
    fn validates_redis_string_value_write_limits() {
        let input = RedisValueInput {
            server_id: "server".into(),
            database: 0,
            key: "cache:key".into(),
            action: "set".into(),
            value: Some("value".into()),
            ttl_seconds: Some(60),
            confirmed: true,
            username: None,
            password: None,
        };
        assert!(validate_redis_value(&input).is_ok());
        let mut unsafe_input = input;
        unsafe_input.confirmed = false;
        assert!(validate_redis_value(&unsafe_input).is_err());
    }

    #[test]
    fn validates_redis_complex_type_operations() {
        let input = RedisComplexActionInput {
            server_id: "server".into(),
            database: 0,
            key: "profile".into(),
            kind: "hash".into(),
            action: "hash_set".into(),
            field: Some("name".into()),
            value: Some("panel".into()),
            score: None,
            confirmed: true,
            username: None,
            password: None,
        };
        assert!(validate_redis_complex_action(&input).is_ok());
        let mut mismatch = input.clone();
        mismatch.action = "list_push_left".into();
        assert!(validate_redis_complex_action(&mismatch).is_err());
    }

    #[test]
    fn validates_redis_transfer_scope_and_marker_count() {
        let input = RedisTransferInput {
            server_id: "server".into(),
            database: 0,
            action: "export".into(),
            path: "/var/backups/cache.1pc".into(),
            max_keys: Some(100),
            confirmed: true,
            username: None,
            password: None,
        };
        assert!(validate_redis_transfer(&input).is_ok());
        assert_eq!(
            parse_redis_transfer_count("noise\n__REDIS_TRANSFER__\texport\t42\n"),
            42
        );
        let mut unsafe_input = input;
        unsafe_input.path = "/tmp/../cache".into();
        assert!(validate_redis_transfer(&unsafe_input).is_err());
    }

    #[test]
    fn validates_redis_migration_target_and_marker_count() {
        let input = RedisMigrationInput {
            source_server_id: "source".into(),
            source_database: 0,
            target_host: "127.0.0.1".into(),
            target_port: 6379,
            target_database: 1,
            source_username: None,
            source_password: None,
            target_username: None,
            target_password: None,
            max_keys: Some(100),
            confirmed: true,
        };
        assert!(validate_redis_migration(&input).is_ok());
        assert_eq!(
            parse_redis_migration_count("noise\n__REDIS_MIGRATION__\t42\t1\n"),
            42
        );
        let mut unsafe_input = input;
        unsafe_input.target_host = "bad;host".into();
        assert!(validate_redis_migration(&unsafe_input).is_err());

        let mut authenticated = unsafe_input;
        authenticated.target_host = "127.0.0.1".into();
        authenticated.target_username = Some("default".into());
        authenticated.target_password = Some(SecretString::from("secret"));
        assert!(validate_redis_migration(&authenticated).is_ok());
    }

    /// 验证 Redis 连接诊断能解析 PING 延迟、版本、角色、客户端与内存摘要。
    #[test]
    fn parses_redis_diagnostic_basics() {
        let output = "__REDIS_PING__\tPONG\n__REDIS_LATENCY_MS__\t3\nredis_version:6.2.0\nrole:master\nredis_mode:standalone\nuptime_in_seconds:120\nconnected_clients:4\nused_memory:1048576\n";
        let diagnostic = parse_redis_diagnostic(output, 0).expect("应能解析");
        assert!(diagnostic.available);
        assert_eq!(diagnostic.database, 0);
        assert_eq!(diagnostic.status.as_deref(), Some("PONG"));
        assert_eq!(diagnostic.latency_ms, Some(3));
        assert_eq!(diagnostic.version.as_deref(), Some("6.2.0"));
        assert_eq!(diagnostic.role.as_deref(), Some("master"));
        assert_eq!(diagnostic.connected_clients, Some(4));
        assert_eq!(diagnostic.used_memory_bytes, Some(1_048_576));
    }

    /// 验证缺少 PING 与版本（例如认证失败或异常返回）时诊断不可解析。
    #[test]
    fn redis_diagnostic_rejects_unparseable_output() {
        assert!(parse_redis_diagnostic("__REDIS_LATENCY_MS__\t3\n", 0).is_none());
        assert!(parse_redis_diagnostic("", 0).is_none());
    }
}

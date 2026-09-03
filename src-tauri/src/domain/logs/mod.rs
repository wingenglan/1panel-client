use crate::domain::docker;
use crate::domain::ssh::{CommandEvent, RemoteCommandResult, SshConnectionManager};
use crate::errors::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Selects one of the supported remote log providers; arbitrary remote files are intentionally excluded.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum LogSource {
    System,
    Systemd,
    NginxAccess,
    NginxError,
    Docker,
    DockerCompose,
}

/// Carries the bounded, typed inputs used by both one-shot and follow log queries.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogQuery {
    pub server_id: String,
    pub source: LogSource,
    pub target: Option<String>,
    pub working_dir: Option<String>,
    pub service: Option<String>,
    pub tail: u32,
    pub privileged: bool,
}

/// Returns one bounded log response without persisting its content locally.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogSnapshot {
    pub source: LogSource,
    pub target: Option<String>,
    pub output: String,
    pub fetched_at: String,
    pub truncated: bool,
}

/// Reads a supported remote log source with a fixed command shape and bounded tail.
pub async fn read(ssh: &SshConnectionManager, query: &LogQuery) -> AppResult<LogSnapshot> {
    let tail = query.tail.clamp(1, 5_000);
    let result = match &query.source {
        LogSource::Docker => {
            let target = required_target(query, "Docker 容器")?;
            let value = docker::logs(ssh, &query.server_id, target, tail, query.privileged).await?;
            RemoteCommandResult {
                exit_code: 0,
                stdout: value.output,
                stderr: String::new(),
                duration_ms: 0,
            }
        }
        LogSource::DockerCompose => {
            let project = required_target(query, "Compose 项目")?;
            let value = docker::compose_logs(
                ssh,
                &query.server_id,
                project,
                query.working_dir.as_deref(),
                query.service.as_deref(),
                tail,
                query.privileged,
            )
            .await?;
            RemoteCommandResult {
                exit_code: 0,
                stdout: value.output,
                stderr: String::new(),
                duration_ms: 0,
            }
        }
        _ => {
            let command = build_command(query, tail, false)?;
            if query.privileged {
                ssh.execute_privileged(&query.server_id, &command, Duration::from_secs(45))
                    .await?
            } else {
                ssh.execute(&query.server_id, &command, Duration::from_secs(45))
                    .await?
            }
        }
    };
    if result.exit_code != 0 {
        return Err(
            AppError::new("LOG_QUERY_FAILED", "logs", "读取远程日志失败")
                .details(result.stderr)
                .for_server(&query.server_id),
        );
    }
    Ok(snapshot(query, result.stdout))
}

/// Follows a supported log source for a bounded SSH task and emits chunks to the UI channel.
pub async fn follow(
    ssh: &SshConnectionManager,
    query: &LogQuery,
    task_id: &str,
    events: &tauri::ipc::Channel<CommandEvent>,
) -> AppResult<LogSnapshot> {
    let tail = query.tail.clamp(1, 5_000);
    let command = build_command(query, tail, true)?;
    let command = format!("timeout --signal=TERM 30s {command}");
    let result = if query.privileged {
        ssh.execute_stream_privileged_task(
            &query.server_id,
            &command,
            Duration::from_secs(40),
            events,
            task_id,
        )
        .await?
    } else {
        ssh.execute_stream_task(
            &query.server_id,
            &command,
            Duration::from_secs(40),
            events,
            task_id,
        )
        .await?
    };
    if result.exit_code != 0 && result.exit_code != 124 {
        return Err(
            AppError::new("LOG_FOLLOW_FAILED", "logs", "跟随远程日志失败")
                .details(result.stderr)
                .for_server(&query.server_id),
        );
    }
    Ok(snapshot(
        query,
        format!("{}{}", result.stdout, result.stderr),
    ))
}

/// Builds a safe one-shot or follow command for non-Docker sources.
fn build_command(query: &LogQuery, tail: u32, follow: bool) -> AppResult<String> {
    let follow_arg = if follow { " -f" } else { "" };
    match &query.source {
        LogSource::System => Ok(format!(
            "journalctl --no-pager -o short-iso -n {tail}{follow_arg}"
        )),
        LogSource::Systemd => {
            let service = required_target(query, "systemd 服务")?;
            if !valid_unit(service) {
                return Err(AppError::new(
                    "VALIDATION_FAILED",
                    "logs",
                    "systemd 服务名无效",
                ));
            }
            Ok(format!(
                "journalctl --no-pager -o short-iso -n {tail} -u {}{}",
                crate::security::shell_escape(service),
                follow_arg
            ))
        }
        LogSource::NginxAccess | LogSource::NginxError => {
            let path = match query.source {
                LogSource::NginxAccess => "/var/log/nginx/access.log",
                LogSource::NginxError => "/var/log/nginx/error.log",
                _ => unreachable!(),
            };
            Ok(if follow {
                format!("tail -n {tail} -F -- {path}")
            } else {
                format!("tail -n {tail} -- {path}")
            })
        }
        LogSource::Docker => {
            let target = required_target(query, "Docker 容器")?;
            if !valid_object(target) {
                return Err(AppError::new(
                    "VALIDATION_FAILED",
                    "logs",
                    "Docker 容器名称无效",
                ));
            }
            Ok(format!(
                "docker logs --timestamps --tail {tail}{} -- {}",
                if follow { " --follow" } else { "" },
                crate::security::shell_escape(target)
            ))
        }
        LogSource::DockerCompose => {
            let project = required_target(query, "Compose 项目")?;
            if !valid_object(project)
                || query
                    .working_dir
                    .as_deref()
                    .is_some_and(|path| !valid_working_dir(path))
            {
                return Err(AppError::new(
                    "VALIDATION_FAILED",
                    "logs",
                    "Compose 项目或工作目录无效",
                ));
            }
            if let Some(service) = query.service.as_deref() {
                if !valid_object(service) {
                    return Err(AppError::new(
                        "VALIDATION_FAILED",
                        "logs",
                        "Compose 服务名无效",
                    ));
                }
            }
            let prefix = query
                .working_dir
                .as_deref()
                .map(|path| format!("cd {} && ", crate::security::shell_escape(path)))
                .unwrap_or_default();
            let follow_arg = if follow { " --follow" } else { "" };
            let service_arg = query
                .service
                .as_deref()
                .map(|value| format!(" {}", crate::security::shell_escape(value)))
                .unwrap_or_default();
            Ok(format!("{prefix}docker compose --project-name {} logs --no-color --timestamps{follow_arg} --tail {tail}{service_arg}", crate::security::shell_escape(project)))
        }
    }
}

/// Converts remote output into a bounded typed response and marks oversized output for the UI.
fn snapshot(query: &LogQuery, output: String) -> LogSnapshot {
    const MAX_BYTES: usize = 1_000_000;
    let truncated = output.len() > MAX_BYTES;
    let output = if truncated {
        String::from_utf8_lossy(&output.as_bytes()[output.len() - MAX_BYTES..]).to_string()
    } else {
        output
    };
    LogSnapshot {
        source: query.source.clone(),
        target: query.target.clone().or_else(|| query.service.clone()),
        output,
        fetched_at: chrono::Utc::now().to_rfc3339(),
        truncated,
    }
}

/// Requires a non-empty target for log sources that address a service, container, or project.
fn required_target<'a>(query: &'a LogQuery, label: &str) -> AppResult<&'a str> {
    query
        .target
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| AppError::new("VALIDATION_FAILED", "logs", format!("请选择{}", label)))
}

/// Validates the subset of systemd unit characters accepted by the existing service module.
fn valid_unit(value: &str) -> bool {
    value.ends_with(".service")
        && !value.is_empty()
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '@' | '-' | '_' | '\\')
        })
}

/// Validates Docker and Compose identifiers before they are shell-escaped into a fixed command.
fn valid_object(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-' | '/')
        })
}

/// Validates an explicitly supplied Compose working directory without allowing shell metacharacters.
fn valid_working_dir(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 512
        && value.starts_with('/')
        && !value.chars().any(|character| {
            character.is_control() || matches!(character, ';' | '|' | '&' | '`' | '$')
        })
}

#[cfg(test)]
mod tests {
    use super::{build_command, LogQuery, LogSource};

    /// Creates a bounded log query fixture for command validation tests.
    fn query(source: LogSource, target: Option<&str>) -> LogQuery {
        LogQuery {
            server_id: "server".into(),
            source,
            target: target.map(str::to_string),
            working_dir: None,
            service: None,
            tail: 200,
            privileged: false,
        }
    }

    #[test]
    /// Verifies that Nginx queries use the fixed access log path.
    fn builds_fixed_nginx_paths() {
        let command = build_command(&query(LogSource::NginxAccess, None), 200, false).unwrap();
        assert_eq!(command, "tail -n 200 -- /var/log/nginx/access.log");
    }

    #[test]
    /// Rejects shell metacharacters in a systemd unit name.
    fn rejects_invalid_systemd_unit() {
        assert!(build_command(
            &query(LogSource::Systemd, Some("bad;rm.service")),
            20,
            false
        )
        .is_err());
    }
}

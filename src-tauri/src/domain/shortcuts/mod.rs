use crate::errors::{AppError, AppResult};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Defines whether a shortcut is shared by every server or overrides one server only.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ShortcutScope {
    Global,
    Server,
}

impl ShortcutScope {
    /// Returns the stable database representation of the shortcut scope.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Server => "server",
        }
    }
}

/// Describes a user-editable terminal shortcut exposed through typed IPC.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutRecord {
    pub id: String,
    pub scope: ShortcutScope,
    pub server_id: Option<String>,
    pub name: String,
    pub group_name: String,
    pub command_template: String,
    pub description: String,
    pub tags: Vec<String>,
    pub enabled: bool,
    pub builtin: bool,
    pub usage_count: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Carries the validated fields needed to create or update a terminal shortcut.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveShortcutInput {
    pub id: Option<String>,
    pub scope: ShortcutScope,
    pub server_id: Option<String>,
    pub name: String,
    pub group_name: String,
    pub command_template: String,
    pub description: String,
    pub tags: Vec<String>,
    pub enabled: bool,
}

impl SaveShortcutInput {
    /// Validates shortcut scope, text lengths, and placeholder shape before SQLite writes.
    pub fn validate(&self) -> AppResult<()> {
        let name = self.name.trim();
        let command = self.command_template.trim();
        if name.is_empty() || name.chars().count() > 80 {
            return Err(AppError::new(
                "VALIDATION_FAILED",
                "shortcut",
                "快捷指令名称不能为空且不能超过 80 个字符",
            ));
        }
        if command.is_empty() || command.chars().count() > 4_000 || command.contains('\0') {
            return Err(AppError::new(
                "VALIDATION_FAILED",
                "shortcut",
                "快捷指令内容不能为空、不能包含空字符且不能超过 4000 个字符",
            ));
        }
        if self.group_name.chars().count() > 60
            || self.description.chars().count() > 240
            || self.tags.len() > 12
        {
            return Err(AppError::new(
                "VALIDATION_FAILED",
                "shortcut",
                "快捷指令分组、说明或标签数量超出限制",
            ));
        }
        if self.group_name.contains('\0') {
            return Err(AppError::new(
                "VALIDATION_FAILED",
                "shortcut",
                "快捷指令分组不能包含空字符",
            ));
        }
        match self.scope {
            ShortcutScope::Global if self.server_id.is_some() => Err(AppError::new(
                "VALIDATION_FAILED",
                "shortcut",
                "全局快捷指令不能绑定服务器",
            )),
            ShortcutScope::Server if self.server_id.as_deref().unwrap_or_default().is_empty() => {
                Err(AppError::new(
                    "VALIDATION_FAILED",
                    "shortcut",
                    "服务器快捷指令必须绑定服务器",
                ))
            }
            _ => Ok(()),
        }
    }
}

/// Defines a built-in shortcut seeded into a new local database or restored on demand.
#[derive(Debug, Clone)]
pub struct DefaultShortcut {
    pub id: &'static str,
    pub name: &'static str,
    pub group_name: &'static str,
    pub command_template: &'static str,
    pub description: &'static str,
    pub tags: &'static [&'static str],
}

/// Returns the safe, read-only-oriented starter shortcuts shipped with 1Panel Client.
pub fn default_shortcuts() -> &'static [DefaultShortcut] {
    &[
        DefaultShortcut {
            id: "builtin-docker-ps",
            name: "docker ps",
            group_name: "Docker",
            command_template: "docker ps -a",
            description: "查看全部容器",
            tags: &["docker", "查看"],
        },
        DefaultShortcut {
            id: "builtin-docker-run",
            name: "docker run",
            group_name: "Docker",
            command_template: "docker run --name {{name}} -d {{image}}",
            description: "启动一个容器",
            tags: &["docker", "容器"],
        },
        DefaultShortcut {
            id: "builtin-docker-logs",
            name: "docker logs",
            group_name: "Docker",
            command_template: "docker logs -f --tail 100 {{container}}",
            description: "跟踪容器日志",
            tags: &["docker", "日志"],
        },
        DefaultShortcut {
            id: "builtin-docker-exec",
            name: "docker exec",
            group_name: "Docker",
            command_template: "docker exec -it {{container}} sh",
            description: "进入容器终端",
            tags: &["docker", "终端"],
        },
        DefaultShortcut {
            id: "builtin-compose-ps",
            name: "compose ps",
            group_name: "Docker",
            command_template: "docker compose ps",
            description: "查看 Compose 服务",
            tags: &["docker", "compose"],
        },
        DefaultShortcut {
            id: "builtin-systemctl-status",
            name: "systemctl status",
            group_name: "Systemd",
            command_template: "systemctl status {{service}} --no-pager",
            description: "查看服务状态",
            tags: &["systemd", "服务"],
        },
        DefaultShortcut {
            id: "builtin-journalctl-service",
            name: "journalctl service",
            group_name: "Systemd",
            command_template: "journalctl -u {{service}} -n 100 --no-pager",
            description: "查看服务日志",
            tags: &["systemd", "日志"],
        },
        DefaultShortcut {
            id: "builtin-ss-listen",
            name: "ss listen",
            group_name: "网络",
            command_template: "ss -lntup",
            description: "查看监听端口",
            tags: &["网络", "端口"],
        },
        DefaultShortcut {
            id: "builtin-df",
            name: "disk usage",
            group_name: "系统",
            command_template: "df -h",
            description: "查看磁盘使用情况",
            tags: &["系统", "磁盘"],
        },
        DefaultShortcut {
            id: "builtin-free",
            name: "memory usage",
            group_name: "系统",
            command_template: "free -h",
            description: "查看内存使用情况",
            tags: &["系统", "内存"],
        },
    ]
}

use crate::domain::server::ServerProfile;
use crate::domain::ssh::{ConnectionSnapshot, ConnectionStatus};
use crate::infra::db::AuditEvent;
use chrono::{DateTime, Utc};
use serde::Serialize;

/// 描述诊断导出中的非敏感服务器档案；不包含任何凭据引用或私钥内容。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticServer {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: i64,
    pub username: String,
    pub auth_type: String,
    pub sudo_mode: String,
    pub favorite: bool,
    pub tags: Vec<String>,
}

/// 描述诊断导出中的连接状态；错误只保留稳定错误码，不导出远端输出或详情。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticConnection {
    pub server_id: String,
    pub status: String,
    pub connected_at: Option<DateTime<Utc>>,
    pub error_code: Option<String>,
}

/// 生成可下载的本地诊断快照；所有字段都来自已脱敏的档案、状态和审计元数据。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsExport {
    pub generated_at: DateTime<Utc>,
    pub app_version: String,
    pub platform: String,
    pub architecture: String,
    pub servers: Vec<DiagnosticServer>,
    pub connections: Vec<DiagnosticConnection>,
    pub recent_audit: Vec<AuditEvent>,
}

impl DiagnosticsExport {
    /// 将本地服务器档案、内存连接快照和最近审计记录组装成脱敏导出。
    pub fn build(
        profiles: Vec<ServerProfile>,
        connections: Vec<ConnectionSnapshot>,
        recent_audit: Vec<AuditEvent>,
    ) -> Self {
        Self {
            generated_at: Utc::now(),
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            platform: std::env::consts::OS.to_string(),
            architecture: std::env::consts::ARCH.to_string(),
            servers: profiles
                .into_iter()
                .map(|profile| DiagnosticServer {
                    id: profile.id,
                    name: profile.name,
                    host: profile.host,
                    port: profile.port,
                    username: profile.username,
                    auth_type: profile.auth_type,
                    sudo_mode: profile.sudo_mode,
                    favorite: profile.favorite,
                    tags: profile.tags,
                })
                .collect(),
            connections: connections
                .into_iter()
                .map(|snapshot| DiagnosticConnection {
                    server_id: snapshot.server_id,
                    status: connection_status(snapshot.status).to_string(),
                    connected_at: snapshot.connected_at,
                    error_code: snapshot.error.map(|error| error.code.to_string()),
                })
                .collect(),
            recent_audit,
        }
    }
}

/// 将内部连接状态映射为稳定的小写诊断字段。
fn connection_status(status: ConnectionStatus) -> &'static str {
    match status {
        ConnectionStatus::Offline => "offline",
        ConnectionStatus::Connecting => "connecting",
        ConnectionStatus::Online => "online",
        ConnectionStatus::Error => "error",
    }
}

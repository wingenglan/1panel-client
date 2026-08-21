use serde::Serialize;
use std::fmt;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppError {
    pub code: &'static str,
    pub category: &'static str,
    pub message: String,
    pub details: Option<String>,
    pub server_id: Option<String>,
    pub recoverable: bool,
    pub suggested_action: Option<String>,
}

impl AppError {
    pub fn new(code: &'static str, category: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            category,
            message: message.into(),
            details: None,
            server_id: None,
            recoverable: true,
            suggested_action: None,
        }
    }

    pub fn details(mut self, details: impl fmt::Display) -> Self {
        self.details = Some(crate::security::redact(&details.to_string()));
        self
    }

    pub fn for_server(mut self, server_id: impl Into<String>) -> Self {
        self.server_id = Some(server_id.into());
        self
    }

    pub fn suggestion(mut self, action: impl Into<String>) -> Self {
        self.suggested_action = Some(action.into());
        self
    }

    pub fn fatal(mut self) -> Self {
        self.recoverable = false;
        self
    }

    pub fn database(error: impl fmt::Display) -> Self {
        Self::new("DATABASE_FAILED", "local_storage", "本地数据库操作失败")
            .details(error.to_string())
    }

    pub fn credential(error: impl fmt::Display) -> Self {
        Self::new("CREDENTIAL_STORE_FAILED", "security", "系统安全存储不可用")
            .details(error.to_string())
            .suggestion("检查系统凭据管理器或 Secret Service 是否可用")
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for AppError {}

pub type AppResult<T> = Result<T, AppError>;

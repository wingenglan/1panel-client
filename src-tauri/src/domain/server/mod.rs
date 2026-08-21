use chrono::{DateTime, Utc};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// 限制 ProxyJump 链路节点数，避免错误配置导致无限递归或过长连接链。
pub const MAX_PROXY_JUMP_DEPTH: usize = 16;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ServerProfile {
    pub id: String,
    pub name: String,
    pub description: String,
    pub host: String,
    pub port: i64,
    pub username: String,
    pub auth_type: String,
    pub private_key_path: Option<String>,
    pub sudo_mode: String,
    pub group_id: Option<String>,
    #[sqlx(skip)]
    pub tags: Vec<String>,
    pub favorite: bool,
    pub connect_timeout: i64,
    pub keepalive: i64,
    pub encoding: String,
    pub proxy_jump_id: Option<String>,
    pub last_connected_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ServerGroup {
    pub id: String,
    pub name: String,
    pub sort_order: i64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicServerProfile {
    pub name: String,
    pub description: String,
    pub host: String,
    pub port: i64,
    pub username: String,
    pub auth_type: String,
    pub private_key_path: Option<String>,
    pub sudo_mode: String,
    pub group_id: Option<String>,
    pub tags: Vec<String>,
    pub favorite: bool,
    pub connect_timeout: i64,
    pub keepalive: i64,
    pub encoding: String,
    #[serde(default)]
    pub proxy_jump_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicServerImport {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_type: String,
    pub private_key_path: Option<String>,
    #[serde(default = "default_sudo_mode")]
    pub sudo_mode: String,
    pub group_id: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub favorite: bool,
    #[serde(default = "default_connect_timeout")]
    pub connect_timeout: i64,
    #[serde(default = "default_keepalive")]
    pub keepalive: i64,
    #[serde(default = "default_encoding")]
    pub encoding: String,
    #[serde(default)]
    pub proxy_jump_id: Option<String>,
}

/// 为旧版公共配置导入提供稳定的连接超时默认值。
fn default_connect_timeout() -> i64 {
    10
}

/// 为旧版公共配置导入提供稳定的 keepalive 默认值。
fn default_keepalive() -> i64 {
    30
}

/// 为旧版公共配置导入提供 UTF-8 默认编码。
fn default_encoding() -> String {
    "UTF-8".into()
}

/// 为不含 sudo 配置的旧版公共导入提供安全默认值。
fn default_sudo_mode() -> String {
    "none".into()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicServerExport {
    pub format: String,
    pub version: u32,
    pub encrypted: bool,
    pub servers: Vec<PublicServerProfile>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveServerInput {
    pub id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_type: String,
    pub password: Option<SecretString>,
    pub private_key_path: Option<String>,
    pub private_key_passphrase: Option<SecretString>,
    pub sudo_mode: String,
    pub sudo_password: Option<SecretString>,
    #[serde(default)]
    pub group_id: Option<String>,
    #[serde(default)]
    pub connect_timeout: Option<u64>,
    #[serde(default)]
    pub keepalive: Option<u64>,
    #[serde(default)]
    pub encoding: Option<String>,
    #[serde(default)]
    pub proxy_jump_id: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub favorite: bool,
}

/// 解析多级 ProxyJump 关系并拒绝循环、孤儿引用和超长链路。
///
/// `links` 只包含本地已知服务器及其下一跳；`first_proxy_jump_id` 用于校验
/// 当前正在保存的档案，因此新建档案也能在落库前参与循环检测。
pub fn resolve_proxy_jump_chain(
    server_id: &str,
    first_proxy_jump_id: Option<&str>,
    links: &HashMap<String, Option<String>>,
) -> crate::errors::AppResult<Vec<String>> {
    let mut chain = vec![server_id.to_string()];
    let mut visited = HashSet::from([server_id.to_string()]);
    let mut next = first_proxy_jump_id.map(str::to_string);
    while let Some(proxy_id) = next {
        if chain.len() >= MAX_PROXY_JUMP_DEPTH {
            return Err(crate::errors::AppError::new(
                "PROXY_JUMP_DEPTH_EXCEEDED",
                "validation",
                format!("ProxyJump 链路不能超过 {} 个节点", MAX_PROXY_JUMP_DEPTH),
            )
            .details(chain.join(" -> ")));
        }
        if !visited.insert(proxy_id.clone()) {
            return Err(crate::errors::AppError::new(
                "PROXY_JUMP_CYCLE",
                "validation",
                "ProxyJump 链路包含循环引用",
            )
            .details(
                chain
                    .iter()
                    .chain(std::iter::once(&proxy_id))
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(" -> "),
            ));
        }
        let Some(next_proxy) = links.get(&proxy_id) else {
            return Err(crate::errors::AppError::new(
                "PROXY_JUMP_NOT_FOUND",
                "validation",
                "ProxyJump 链路引用了不存在的服务器",
            )
            .details(proxy_id));
        };
        chain.push(proxy_id);
        next = next_proxy.clone();
    }
    Ok(chain)
}

impl SaveServerInput {
    /// 校验服务器档案及可选 ProxyJump 跳板引用，拒绝无法建立安全 SSH 会话的配置。
    pub fn validate(&self) -> crate::errors::AppResult<()> {
        if self.name.trim().is_empty()
            || self.host.trim().is_empty()
            || self.username.trim().is_empty()
        {
            return Err(crate::errors::AppError::new(
                "VALIDATION_FAILED",
                "validation",
                "名称、主机和用户名不能为空",
            ));
        }
        if !matches!(
            self.auth_type.as_str(),
            "password" | "private_key" | "ssh_agent"
        ) {
            return Err(crate::errors::AppError::new(
                "VALIDATION_FAILED",
                "validation",
                "不支持的 SSH 认证方式",
            ));
        }
        if self.proxy_jump_id.as_deref().is_some_and(|value| {
            value.is_empty()
                || value.len() > 80
                || self.id.as_deref() == Some(value)
                || !value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        }) {
            return Err(crate::errors::AppError::new(
                "VALIDATION_FAILED",
                "validation",
                "ProxyJump 服务器标识无效或不能指向自身",
            ));
        }
        if !matches!(
            self.sudo_mode.as_str(),
            "none" | "passwordless" | "password"
        ) {
            return Err(crate::errors::AppError::new(
                "VALIDATION_FAILED",
                "validation",
                "不支持的 sudo 模式",
            ));
        }
        if self.auth_type == "password"
            && self.id.is_none()
            && self
                .password
                .as_ref()
                .map(|value| value.expose_secret().is_empty())
                .unwrap_or(true)
        {
            return Err(crate::errors::AppError::new(
                "VALIDATION_FAILED",
                "validation",
                "密码认证需要 SSH 密码",
            ));
        }
        if self.auth_type == "private_key"
            && self
                .private_key_path
                .as_deref()
                .unwrap_or_default()
                .is_empty()
        {
            return Err(crate::errors::AppError::new(
                "VALIDATION_FAILED",
                "validation",
                "私钥认证需要私钥路径",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
#[allow(dead_code)]
pub struct ServerRecord {
    pub id: String,
    pub name: String,
    pub description: String,
    pub host: String,
    pub port: i64,
    pub username: String,
    pub auth_type: String,
    pub password_secret_ref: Option<String>,
    pub private_key_path: Option<String>,
    pub key_passphrase_secret_ref: Option<String>,
    pub sudo_mode: String,
    pub sudo_secret_ref: Option<String>,
    pub group_id: Option<String>,
    pub favorite: bool,
    pub settings_json: String,
    pub last_connected_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ServerRecord {
    pub fn connect_timeout(&self) -> u64 {
        serde_json::from_str::<serde_json::Value>(&self.settings_json)
            .ok()
            .and_then(|value| value.get("connectTimeout").and_then(|value| value.as_u64()))
            .unwrap_or(10)
    }

    pub fn keepalive(&self) -> u64 {
        serde_json::from_str::<serde_json::Value>(&self.settings_json)
            .ok()
            .and_then(|value| value.get("keepalive").and_then(|value| value.as_u64()))
            .unwrap_or(30)
    }

    /// 从服务器设置中读取可选的 ProxyJump 跳板服务器 ID。
    pub fn proxy_jump_id(&self) -> Option<String> {
        serde_json::from_str::<serde_json::Value>(&self.settings_json)
            .ok()
            .and_then(|value| {
                value
                    .get("proxyJumpId")
                    .and_then(|value| value.as_str())
                    .filter(|value| !value.trim().is_empty())
                    .map(str::to_string)
            })
    }
}

#[cfg(test)]
mod tests {
    use super::{resolve_proxy_jump_chain, SaveServerInput};
    use std::collections::HashMap;

    /// 构造一份可通过基础校验的服务器输入，供字段级验证测试复用。
    fn valid_input() -> SaveServerInput {
        SaveServerInput {
            id: Some("target".into()),
            name: "Target".into(),
            description: String::new(),
            host: "target.example".into(),
            port: 22,
            username: "root".into(),
            auth_type: "ssh_agent".into(),
            password: None,
            private_key_path: None,
            private_key_passphrase: None,
            sudo_mode: "none".into(),
            sudo_password: None,
            group_id: None,
            connect_timeout: Some(10),
            keepalive: Some(30),
            encoding: Some("UTF-8".into()),
            proxy_jump_id: Some("jump".into()),
            tags: Vec::new(),
            favorite: false,
        }
    }

    #[test]
    fn validates_proxy_jump_reference() {
        assert!(valid_input().validate().is_ok());

        let mut self_reference = valid_input();
        self_reference.proxy_jump_id = Some("target".into());
        assert!(self_reference.validate().is_err());

        let mut unsafe_reference = valid_input();
        unsafe_reference.proxy_jump_id = Some("jump;rm".into());
        assert!(unsafe_reference.validate().is_err());
    }

    /// 验证多级跳板能解析为有序链路，并能拒绝循环与不存在的下一跳。
    #[test]
    fn resolves_multihop_proxy_jump_chain_and_rejects_invalid_links() {
        let links = HashMap::from([
            ("jump-a".to_string(), Some("jump-b".to_string())),
            ("jump-b".to_string(), None),
            ("loop-a".to_string(), Some("loop-b".to_string())),
            ("loop-b".to_string(), Some("loop-a".to_string())),
        ]);
        assert_eq!(
            resolve_proxy_jump_chain("target", Some("jump-a"), &links).unwrap(),
            vec!["target", "jump-a", "jump-b"]
        );
        assert_eq!(
            resolve_proxy_jump_chain("target", Some("loop-a"), &links)
                .unwrap_err()
                .code,
            "PROXY_JUMP_CYCLE"
        );
        assert_eq!(
            resolve_proxy_jump_chain("target", Some("missing"), &links)
                .unwrap_err()
                .code,
            "PROXY_JUMP_NOT_FOUND"
        );
    }

    /// 验证 ProxyJump 深度上限，避免异常配置消耗无限递归栈。
    #[test]
    fn rejects_proxy_jump_depth_over_limit() {
        let links = (0..super::MAX_PROXY_JUMP_DEPTH)
            .map(|index| (format!("jump-{index}"), Some(format!("jump-{}", index + 1))))
            .collect::<HashMap<_, _>>();
        assert_eq!(
            resolve_proxy_jump_chain("target", Some("jump-0"), &links)
                .unwrap_err()
                .code,
            "PROXY_JUMP_DEPTH_EXCEEDED"
        );
    }
}

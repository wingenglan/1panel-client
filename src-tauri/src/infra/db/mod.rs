use crate::domain::backup::{BackupPayload, BackupServer};
use crate::domain::server::{
    PublicServerExport, PublicServerImport, PublicServerProfile, SaveServerInput, ServerGroup,
    ServerProfile, ServerRecord,
};
use crate::errors::{AppError, AppResult};
use chrono::Utc;
use secrecy::{ExposeSecret, SecretString};
use serde::Serialize;
use sqlx::{SqlitePool, Transaction};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

/// 表示本地审计表中的一条非敏感操作元数据。
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct AuditEvent {
    pub id: String,
    pub server_id: Option<String>,
    pub action: String,
    pub resource_type: String,
    pub resource_id: Option<String>,
    pub result: String,
    pub summary: String,
    pub created_at: chrono::DateTime<Utc>,
}

#[derive(Clone)]
pub struct ServerRepository {
    pool: SqlitePool,
    credentials: Arc<dyn crate::security::CredentialStore>,
}

impl ServerRepository {
    pub fn new(pool: SqlitePool, credentials: Arc<dyn crate::security::CredentialStore>) -> Self {
        Self { pool, credentials }
    }

    pub async fn list(&self) -> AppResult<Vec<ServerProfile>> {
        let records = sqlx::query_as::<_, ServerProfile>(
            r#"SELECT id, name, description, host, port, username, auth_type, private_key_path,
               sudo_mode, group_id, favorite,
               COALESCE(json_extract(settings_json, '$.connectTimeout'), 10) AS connect_timeout,
               COALESCE(json_extract(settings_json, '$.keepalive'), 30) AS keepalive,
               COALESCE(json_extract(settings_json, '$.encoding'), 'UTF-8') AS encoding,
               json_extract(settings_json, '$.proxyJumpId') AS proxy_jump_id,
               last_connected_at, created_at, updated_at
               FROM servers ORDER BY favorite DESC, COALESCE(last_connected_at, created_at) DESC"#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::database)?;
        self.attach_tags(records).await
    }

    /// 读取本地服务器分组，供侧栏和服务器编辑表单复用。
    pub async fn list_groups(&self) -> AppResult<Vec<ServerGroup>> {
        sqlx::query_as::<_, ServerGroup>(
            "SELECT id, name, sort_order, created_at FROM server_groups ORDER BY sort_order, name",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::database)
    }

    /// 写入一条不含命令输出、凭据或私钥内容的本地审计事件。
    pub async fn record_audit(
        &self,
        server_id: Option<&str>,
        action: &str,
        resource_type: &str,
        resource_id: Option<&str>,
        result: &str,
        summary: &str,
    ) -> AppResult<()> {
        sqlx::query(
            "INSERT INTO audit_events (id,server_id,action,resource_type,resource_id,result,summary,created_at) VALUES (?,?,?,?,?,?,?,?)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(server_id)
        .bind(action)
        .bind(resource_type)
        .bind(resource_id)
        .bind(result)
        .bind(summary)
        .bind(Utc::now())
        .execute(&self.pool)
        .await
        .map_err(AppError::database)?;
        Ok(())
    }

    /// 读取最近的审计元数据，供设置页和诊断导出查看。
    pub async fn list_audit(&self, limit: u32) -> AppResult<Vec<AuditEvent>> {
        sqlx::query_as::<_, AuditEvent>(
            "SELECT id,server_id,action,resource_type,resource_id,result,summary,created_at FROM audit_events ORDER BY created_at DESC LIMIT ?",
        )
        .bind(i64::from(limit.clamp(1, 200)))
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::database)
    }

    /// 创建一个本地服务器分组；名称必须唯一且不能为空。
    pub async fn create_group(&self, name: String) -> AppResult<ServerGroup> {
        let name = name.trim().to_string();
        if name.is_empty() || name.len() > 80 {
            return Err(AppError::new(
                "VALIDATION_FAILED",
                "validation",
                "分组名称不能为空且不能超过 80 个字符",
            ));
        }
        let group = ServerGroup {
            id: Uuid::new_v4().to_string(),
            name,
            sort_order: 0,
            created_at: Utc::now(),
        };
        sqlx::query("INSERT INTO server_groups (id,name,sort_order,created_at) VALUES (?,?,?,?)")
            .bind(&group.id)
            .bind(&group.name)
            .bind(group.sort_order)
            .bind(group.created_at)
            .execute(&self.pool)
            .await
            .map_err(AppError::database)?;
        Ok(group)
    }

    pub async fn get(&self, id: &str) -> AppResult<ServerProfile> {
        let profile = sqlx::query_as::<_, ServerProfile>(
            r#"SELECT id, name, description, host, port, username, auth_type, private_key_path,
               sudo_mode, group_id, favorite,
               COALESCE(json_extract(settings_json, '$.connectTimeout'), 10) AS connect_timeout,
               COALESCE(json_extract(settings_json, '$.keepalive'), 30) AS keepalive,
               COALESCE(json_extract(settings_json, '$.encoding'), 'UTF-8') AS encoding,
               json_extract(settings_json, '$.proxyJumpId') AS proxy_jump_id,
               last_connected_at, created_at, updated_at FROM servers WHERE id = ?"#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::database)?
        .ok_or_else(|| AppError::new("SERVER_NOT_FOUND", "server", "服务器配置不存在"))?;
        self.attach_tags(vec![profile])
            .await
            .map(|mut values| values.remove(0))
    }

    /// 导出不含凭据内容的服务器配置，供用户保存为版本化 JSON。
    pub async fn export_public(&self) -> AppResult<PublicServerExport> {
        let servers = self
            .list()
            .await?
            .into_iter()
            .map(|value| PublicServerProfile {
                name: value.name,
                description: value.description,
                host: value.host,
                port: value.port,
                username: value.username,
                auth_type: value.auth_type,
                private_key_path: value.private_key_path,
                sudo_mode: value.sudo_mode,
                group_id: value.group_id,
                tags: value.tags,
                favorite: value.favorite,
                connect_timeout: value.connect_timeout,
                keepalive: value.keepalive,
                encoding: value.encoding,
                proxy_jump_id: value.proxy_jump_id,
            })
            .collect();
        Ok(PublicServerExport {
            format: "1panel-client-backup".into(),
            version: 1,
            encrypted: false,
            servers,
        })
    }

    /// 导入公共配置并始终生成新 ID，避免覆盖已有服务器；secret 需由用户重新录入。
    pub async fn import_public(
        &self,
        values: Vec<PublicServerImport>,
    ) -> AppResult<Vec<ServerProfile>> {
        let mut imported = Vec::with_capacity(values.len());
        for value in values {
            // 仅复用当前数据库中确实存在的跳板档案；导入文件里的旧 ID 不会被盲目绑定。
            let proxy_jump_id = match value.proxy_jump_id {
                Some(id) if self.record(&id).await.is_ok() => Some(id),
                _ => None,
            };
            imported.push(
                self.save(SaveServerInput {
                    id: Some(Uuid::new_v4().to_string()),
                    name: value.name,
                    description: value.description,
                    host: value.host,
                    port: value.port,
                    username: value.username,
                    auth_type: value.auth_type,
                    password: None,
                    private_key_path: value.private_key_path,
                    private_key_passphrase: None,
                    sudo_mode: value.sudo_mode,
                    sudo_password: None,
                    group_id: value.group_id,
                    connect_timeout: Some(value.connect_timeout as u64),
                    keepalive: Some(value.keepalive as u64),
                    encoding: Some(value.encoding),
                    proxy_jump_id,
                    tags: value.tags,
                    favorite: value.favorite,
                })
                .await?,
            );
        }
        Ok(imported)
    }

    /// 复制服务器公共配置并生成新 ID；凭据不会复制，用户需在新档案中重新录入。
    pub async fn duplicate(&self, id: &str) -> AppResult<ServerProfile> {
        let profile = self.get(id).await?;
        self.save(SaveServerInput {
            id: Some(Uuid::new_v4().to_string()),
            name: format!("{} (副本)", profile.name),
            description: profile.description,
            host: profile.host,
            port: u16::try_from(profile.port)
                .map_err(|_| AppError::new("SERVER_INVALID", "server", "服务器端口无效"))?,
            username: profile.username,
            auth_type: profile.auth_type,
            password: None,
            private_key_path: profile.private_key_path,
            private_key_passphrase: None,
            sudo_mode: profile.sudo_mode,
            sudo_password: None,
            group_id: profile.group_id,
            connect_timeout: Some(profile.connect_timeout as u64),
            keepalive: Some(profile.keepalive as u64),
            encoding: Some(profile.encoding),
            proxy_jump_id: profile.proxy_jump_id,
            tags: profile.tags,
            favorite: false,
        })
        .await
    }

    /// 读取服务器档案及其系统凭据，供上层加密为完整备份；secret 不进入数据库或日志。
    pub async fn export_backup(&self) -> AppResult<BackupPayload> {
        let profiles = self.list().await?;
        let mut servers = Vec::with_capacity(profiles.len());
        for profile in profiles {
            let record = self.record(&profile.id).await?;
            servers.push(BackupServer {
                profile: PublicServerProfile {
                    name: profile.name,
                    description: profile.description,
                    host: profile.host,
                    port: profile.port,
                    username: profile.username,
                    auth_type: profile.auth_type,
                    private_key_path: profile.private_key_path,
                    sudo_mode: profile.sudo_mode,
                    group_id: profile.group_id,
                    tags: profile.tags,
                    favorite: profile.favorite,
                    connect_timeout: profile.connect_timeout,
                    keepalive: profile.keepalive,
                    encoding: profile.encoding,
                    proxy_jump_id: profile.proxy_jump_id,
                },
                password: self.read_optional_secret(record.password_secret_ref.as_deref())?,
                private_key_passphrase: self
                    .read_optional_secret(record.key_passphrase_secret_ref.as_deref())?,
                sudo_password: self.read_optional_secret(record.sudo_secret_ref.as_deref())?,
            });
        }
        Ok(BackupPayload { servers })
    }

    /// 解密后的完整备份以新 ID 导入，并将凭据重新写入操作系统安全存储。
    pub async fn import_backup(&self, payload: BackupPayload) -> AppResult<Vec<ServerProfile>> {
        let mut imported = Vec::with_capacity(payload.servers.len());
        for value in payload.servers {
            let profile = value.profile;
            imported.push(
                self.save(SaveServerInput {
                    id: Some(Uuid::new_v4().to_string()),
                    name: profile.name,
                    description: profile.description,
                    host: profile.host,
                    port: u16::try_from(profile.port).map_err(|_| {
                        AppError::new("BACKUP_INVALID", "backup", "备份中的 SSH 端口无效")
                    })?,
                    username: profile.username,
                    auth_type: profile.auth_type,
                    password: value.password.map(SecretString::from),
                    private_key_path: profile.private_key_path,
                    private_key_passphrase: value.private_key_passphrase.map(SecretString::from),
                    sudo_mode: profile.sudo_mode,
                    sudo_password: value.sudo_password.map(SecretString::from),
                    group_id: None,
                    connect_timeout: Some(profile.connect_timeout as u64),
                    keepalive: Some(profile.keepalive as u64),
                    encoding: Some(profile.encoding),
                    // 完整备份导入会生成新 ID，避免引用不存在的旧跳板档案。
                    proxy_jump_id: None,
                    tags: profile.tags,
                    favorite: profile.favorite,
                })
                .await?,
            );
        }
        Ok(imported)
    }

    pub async fn record(&self, id: &str) -> AppResult<ServerRecord> {
        sqlx::query_as::<_, ServerRecord>("SELECT * FROM servers WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(AppError::database)?
            .ok_or_else(|| AppError::new("SERVER_NOT_FOUND", "server", "服务器配置不存在"))
    }

    /// 读取本地服务器关系并解析当前档案的完整多级 ProxyJump 链路。
    ///
    /// 关系在落库前也会调用同一解析器，因此这里主要供 SSH 连接前再次防御
    /// 被旧版本或外部导入写入的循环配置。
    pub async fn proxy_jump_chain(
        &self,
        server_id: &str,
        first_proxy_jump_id: Option<&str>,
    ) -> AppResult<Vec<String>> {
        let records =
            sqlx::query_as::<_, (String, String)>("SELECT id, settings_json FROM servers")
                .fetch_all(&self.pool)
                .await
                .map_err(AppError::database)?;
        let links = records
            .into_iter()
            .map(|(id, settings)| {
                let next = serde_json::from_str::<serde_json::Value>(&settings)
                    .ok()
                    .and_then(|value| {
                        value
                            .get("proxyJumpId")
                            .and_then(|value| value.as_str())
                            .filter(|value| !value.trim().is_empty())
                            .map(str::to_string)
                    });
                (id, next)
            })
            .collect::<HashMap<_, _>>();
        crate::domain::server::resolve_proxy_jump_chain(server_id, first_proxy_jump_id, &links)
    }

    async fn attach_tags(&self, mut profiles: Vec<ServerProfile>) -> AppResult<Vec<ServerProfile>> {
        for profile in &mut profiles {
            profile.tags = sqlx::query_scalar::<_, String>(
                "SELECT t.name FROM tags t JOIN server_tags st ON st.tag_id=t.id WHERE st.server_id=? ORDER BY t.name",
            )
            .bind(&profile.id)
            .fetch_all(&self.pool)
            .await
            .map_err(AppError::database)?;
        }
        Ok(profiles)
    }

    /// 校验并保存服务器档案；凭据只写入 CredentialStore，数据库仅保留引用。
    pub async fn save(&self, input: SaveServerInput) -> AppResult<ServerProfile> {
        input.validate()?;
        let id = input
            .id
            .clone()
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let existing = sqlx::query_as::<_, ServerRecord>("SELECT * FROM servers WHERE id=?")
            .bind(&id)
            .fetch_optional(&self.pool)
            .await
            .map_err(AppError::database)?;
        self.proxy_jump_chain(&id, input.proxy_jump_id.as_deref())
            .await?;
        let previous_password_ref = existing
            .as_ref()
            .and_then(|value| value.password_secret_ref.clone());
        let previous_passphrase_ref = existing
            .as_ref()
            .and_then(|value| value.key_passphrase_secret_ref.clone());
        let previous_sudo_ref = existing
            .as_ref()
            .and_then(|value| value.sudo_secret_ref.clone());
        let password_ref = if input.auth_type == "password" {
            self.persist_optional_secret(
                previous_password_ref.clone(),
                input.password.as_ref().map(|value| value.expose_secret()),
                "ssh",
            )?
        } else {
            None
        };
        let passphrase_ref = if input.auth_type == "private_key" {
            self.persist_optional_secret(
                previous_passphrase_ref.clone(),
                input
                    .private_key_passphrase
                    .as_ref()
                    .map(|value| value.expose_secret()),
                "key-passphrase",
            )?
        } else {
            None
        };
        let sudo_ref = if input.sudo_mode == "password" {
            self.persist_optional_secret(
                previous_sudo_ref.clone(),
                input
                    .sudo_password
                    .as_ref()
                    .map(|value| value.expose_secret()),
                "sudo",
            )?
        } else {
            None
        };
        let now = Utc::now();
        let created_at = existing
            .as_ref()
            .map(|value| value.created_at)
            .unwrap_or(now);
        let settings = serde_json::json!({
            "connectTimeout": input.connect_timeout.unwrap_or(10).clamp(5, 120),
            "keepalive": input.keepalive.unwrap_or(30).clamp(5, 300),
            "encoding": input.encoding.as_deref().unwrap_or("UTF-8"),
            "proxyJumpId": input.proxy_jump_id.as_deref(),
        })
        .to_string();
        let mut transaction = self.pool.begin().await.map_err(AppError::database)?;
        sqlx::query(
            r#"INSERT INTO servers (id,name,description,host,port,username,auth_type,password_secret_ref,
               private_key_path,key_passphrase_secret_ref,sudo_mode,sudo_secret_ref,group_id,favorite,settings_json,created_at,updated_at)
               VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)
               ON CONFLICT(id) DO UPDATE SET name=excluded.name,description=excluded.description,host=excluded.host,
               port=excluded.port,username=excluded.username,auth_type=excluded.auth_type,password_secret_ref=excluded.password_secret_ref,
               private_key_path=excluded.private_key_path,key_passphrase_secret_ref=excluded.key_passphrase_secret_ref,
               sudo_mode=excluded.sudo_mode,sudo_secret_ref=excluded.sudo_secret_ref,group_id=excluded.group_id,
               favorite=excluded.favorite,settings_json=excluded.settings_json,updated_at=excluded.updated_at"#,
        )
        .bind(&id).bind(input.name.trim()).bind(input.description.trim()).bind(input.host.trim())
        .bind(i64::from(input.port)).bind(input.username.trim()).bind(&input.auth_type).bind(password_ref.as_deref())
        .bind(input.private_key_path.as_deref().map(str::trim)).bind(passphrase_ref.as_deref()).bind(&input.sudo_mode)
        .bind(sudo_ref.as_deref()).bind(input.group_id.as_deref()).bind(input.favorite).bind(settings)
        .bind(created_at).bind(now)
        .execute(&mut *transaction).await.map_err(AppError::database)?;
        self.replace_tags(&mut transaction, &id, input.tags).await?;
        transaction.commit().await.map_err(AppError::database)?;
        for (previous, current) in [
            (previous_password_ref, password_ref),
            (previous_passphrase_ref, passphrase_ref),
            (previous_sudo_ref, sudo_ref),
        ] {
            if previous.is_some() && previous != current {
                if let Some(reference) = previous {
                    self.credentials.delete(&reference)?;
                }
            }
        }
        self.get(&id).await
    }

    fn persist_optional_secret(
        &self,
        existing: Option<String>,
        value: Option<&str>,
        kind: &str,
    ) -> AppResult<Option<String>> {
        let Some(value) = value.filter(|value| !value.is_empty()) else {
            return Ok(existing);
        };
        let reference = existing.unwrap_or_else(|| format!("{}-{}", kind, Uuid::new_v4()));
        self.credentials
            .put(&reference, SecretString::from(value.to_owned()))?;
        Ok(Some(reference))
    }

    async fn replace_tags(
        &self,
        transaction: &mut Transaction<'_, sqlx::Sqlite>,
        server_id: &str,
        tags: Vec<String>,
    ) -> AppResult<()> {
        sqlx::query("DELETE FROM server_tags WHERE server_id=?")
            .bind(server_id)
            .execute(&mut **transaction)
            .await
            .map_err(AppError::database)?;
        for name in tags
            .into_iter()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
        {
            let tag_id = Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT INTO tags (id,name,created_at) VALUES (?,?,?) ON CONFLICT(name) DO NOTHING",
            )
            .bind(&tag_id)
            .bind(&name)
            .bind(Utc::now())
            .execute(&mut **transaction)
            .await
            .map_err(AppError::database)?;
            sqlx::query(
                "INSERT INTO server_tags (server_id,tag_id) SELECT ?,id FROM tags WHERE name=?",
            )
            .bind(server_id)
            .bind(&name)
            .execute(&mut **transaction)
            .await
            .map_err(AppError::database)?;
        }
        Ok(())
    }

    pub async fn delete(&self, id: &str) -> AppResult<()> {
        let record = self.record(id).await?;
        sqlx::query("DELETE FROM servers WHERE id=?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(AppError::database)?;
        for reference in [
            record.password_secret_ref,
            record.key_passphrase_secret_ref,
            record.sudo_secret_ref,
        ]
        .into_iter()
        .flatten()
        {
            self.credentials.delete(&reference)?;
        }
        Ok(())
    }

    pub async fn known_host(&self, identity: &str) -> AppResult<Option<KnownHost>> {
        sqlx::query_as::<_, KnownHost>("SELECT server_identity,key_type,fingerprint,public_key FROM known_hosts WHERE server_identity=?")
            .bind(identity).fetch_optional(&self.pool).await.map_err(AppError::database)
    }

    pub async fn trust_host(&self, value: &KnownHost) -> AppResult<()> {
        let now = Utc::now();
        sqlx::query("INSERT INTO known_hosts (server_identity,key_type,fingerprint,public_key,first_seen_at,last_seen_at) VALUES (?,?,?,?,?,?)")
            .bind(&value.server_identity).bind(&value.key_type).bind(&value.fingerprint).bind(&value.public_key)
            .bind(now).bind(now).execute(&self.pool).await.map_err(AppError::database)?;
        Ok(())
    }

    pub async fn mark_connected(&self, id: &str) -> AppResult<()> {
        sqlx::query("UPDATE servers SET last_connected_at=?,updated_at=? WHERE id=?")
            .bind(Utc::now())
            .bind(Utc::now())
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(AppError::database)?;
        Ok(())
    }

    pub fn credential(&self, reference: &str) -> AppResult<SecretString> {
        self.credentials.get(reference)
    }

    /// 从系统凭据库读取可选 secret，并立即复制成仅供加密流程使用的内存字符串。
    fn read_optional_secret(&self, reference: Option<&str>) -> AppResult<Option<String>> {
        reference
            .map(|value| {
                self.credential(value)
                    .map(|secret| secret.expose_secret().to_owned())
            })
            .transpose()
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct KnownHost {
    pub server_identity: String,
    pub key_type: String,
    pub fingerprint: String,
    pub public_key: String,
}

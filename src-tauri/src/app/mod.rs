use crate::domain::ssh::SshConnectionManager;
use crate::errors::{AppError, AppResult};
use crate::infra::db::ServerRepository;
use crate::infra::local::LocalRepository;
use crate::security::{CredentialStore, OsCredentialStore};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::sync::Arc;
use tauri::{AppHandle, Manager};

pub struct AppState {
    pub servers: ServerRepository,
    pub ssh: SshConnectionManager,
    pub transfers: crate::domain::transfer::TransferManager,
    pub local: LocalRepository,
    pub credentials: Arc<dyn CredentialStore>,
}

impl AppState {
    pub async fn initialize(app: &AppHandle) -> AppResult<Self> {
        let app_data = app.path().app_data_dir().map_err(|error| {
            AppError::new(
                "APP_DATA_UNAVAILABLE",
                "local_storage",
                "无法确定应用数据目录",
            )
            .details(error)
        })?;
        std::fs::create_dir_all(&app_data).map_err(AppError::database)?;
        let options = SqliteConnectOptions::new()
            .filename(app_data.join("1panel-client.sqlite3"))
            .create_if_missing(true)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await
            .map_err(AppError::database)?;
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .map_err(AppError::database)?;
        let local = LocalRepository::new(pool.clone());
        local.initialize_workspace().await?;
        let credentials: Arc<dyn CredentialStore> =
            Arc::new(OsCredentialStore::new("com.agentless.servermanager"));
        let servers = ServerRepository::new(pool, credentials.clone());
        let ssh = SshConnectionManager::new(servers.clone());
        let state = Self {
            servers,
            ssh,
            transfers: crate::domain::transfer::TransferManager::default(),
            local,
            credentials,
        };
        crate::domain::advanced::spawn_monitor_scheduler(state.ssh.clone(), state.local.clone());
        crate::domain::cronjob::spawn_offline_scheduler(
            state.servers.clone(),
            state.ssh.clone(),
            state.local.clone(),
            state.credentials.clone(),
        );
        Ok(state)
    }
}

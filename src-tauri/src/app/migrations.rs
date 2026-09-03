use sqlx::migrate::{Migrate, MigrateError, Migration, Migrator};
use sqlx::SqlitePool;
use std::borrow::Cow;

static MIGRATIONS: Migrator = sqlx::migrate!("./migrations");

/// 兼容前五个历史脚本的 LF/CRLF 校验和；保留历史记录与 SQLx 的内容校验，新增迁移统一使用 LF。
pub(super) async fn run(pool: &SqlitePool) -> Result<(), MigrateError> {
    let mut connection = pool.acquire().await?;
    connection.ensure_migrations_table().await?;
    let applied = connection.list_applied_migrations().await?;
    let migrations = MIGRATIONS
        .iter()
        .map(|migration| {
            let lf = migration.sql.replace("\r\n", "\n");
            let canonical = Migration::new(
                migration.version,
                migration.description.clone(),
                migration.migration_type,
                Cow::Owned(lf.clone()),
                migration.no_tx,
            );
            // 旧版 Windows 构建曾受 Git autocrlf 影响；只接受同一份 SQL 的精确 CRLF 变体。
            if migration.version <= 5 {
                if let Some(previous) = applied
                    .iter()
                    .find(|entry| entry.version == migration.version)
                {
                    if previous.checksum != canonical.checksum {
                        let windows = Migration::new(
                            migration.version,
                            migration.description.clone(),
                            migration.migration_type,
                            Cow::Owned(lf.replace('\n', "\r\n")),
                            migration.no_tx,
                        );
                        if previous.checksum == windows.checksum {
                            return windows;
                        }
                    }
                }
            }
            canonical
        })
        .collect();
    Migrator {
        migrations: Cow::Owned(migrations),
        ..Migrator::DEFAULT
    }
    .run_direct(&mut *connection)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    /// 构造早期开发版、Windows CI 或 Unix 打包使用的换行组合。
    fn legacy_migrations(mode: &str) -> Migrator {
        Migrator {
            migrations: Cow::Owned(
                MIGRATIONS
                    .iter()
                    .take(4)
                    .map(|migration| {
                        let lf = migration.sql.replace("\r\n", "\n");
                        let crlf = mode == "windows" || (mode == "mixed" && migration.version == 1);
                        Migration::new(
                            migration.version,
                            migration.description.clone(),
                            migration.migration_type,
                            Cow::Owned(if crlf { lf.replace('\n', "\r\n") } else { lf }),
                            migration.no_tx,
                        )
                    })
                    .collect(),
            ),
            ..Migrator::DEFAULT
        }
    }

    /// 使用独立内存数据库验证全新安装、重复启动与初始化后的工作区可用性。
    #[tokio::test]
    async fn initializes_fresh_database_and_reopens() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        run(&pool).await.unwrap();
        run(&pool).await.unwrap();
        crate::infra::local::LocalRepository::new(pool.clone())
            .initialize_workspace()
            .await
            .unwrap();
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, MIGRATIONS.iter().count() as i64);
    }

    /// 复现不同平台换行导致的升级闪退，确认数据及历史校验和保持原样。
    #[tokio::test]
    async fn upgrades_legacy_line_endings_without_rewriting_history() {
        for mode in ["unix", "windows", "mixed"] {
            let pool = SqlitePoolOptions::new()
                .max_connections(1)
                .connect("sqlite::memory:")
                .await
                .unwrap();
            legacy_migrations(mode).run(&pool).await.unwrap();
            sqlx::query("INSERT INTO app_settings VALUES ('startup-test', '42', '2026-09-03')")
                .execute(&pool)
                .await
                .unwrap();
            let before: Vec<(i64, Vec<u8>)> =
                sqlx::query_as("SELECT version, checksum FROM _sqlx_migrations ORDER BY version")
                    .fetch_all(&pool)
                    .await
                    .unwrap();
            run(&pool)
                .await
                .unwrap_or_else(|error| panic!("{mode}: {error}"));
            run(&pool).await.unwrap();
            let after: Vec<(i64, Vec<u8>)> = sqlx::query_as("SELECT version, checksum FROM _sqlx_migrations WHERE version <= 4 ORDER BY version").fetch_all(&pool).await.unwrap();
            assert_eq!(before, after);
            let value: String = sqlx::query_scalar(
                "SELECT value_json FROM app_settings WHERE key = 'startup-test'",
            )
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(value, "42");
            sqlx::query(
                "SELECT io_read_bytes_per_second, io_write_bytes_per_second FROM metric_samples",
            )
            .fetch_all(&pool)
            .await
            .unwrap();
        }
    }

    /// 真正修改过 SQL 的历史校验和仍必须阻止启动，不能被换行兼容逻辑掩盖。
    #[tokio::test]
    async fn rejects_actual_migration_changes() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        run(&pool).await.unwrap();
        sqlx::query("UPDATE _sqlx_migrations SET checksum = zeroblob(48) WHERE version = 2")
            .execute(&pool)
            .await
            .unwrap();
        assert!(matches!(
            run(&pool).await,
            Err(MigrateError::VersionMismatch(2))
        ));
    }
}

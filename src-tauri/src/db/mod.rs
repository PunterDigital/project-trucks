use std::path::PathBuf;

use anyhow::{Context, Result};
use sha2::{Digest, Sha384};
use sqlx::migrate::Migrator;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use tauri::{AppHandle, Manager};

pub mod models;
pub mod rollups;

pub struct AppState {
    pub pool: SqlitePool,
}

pub async fn init_pool(app: &AppHandle) -> Result<SqlitePool> {
    let db_path = resolve_db_path(app)?;

    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating app data dir {}", parent.display()))?;
    }

    let options = SqliteConnectOptions::new()
        .filename(&db_path)
        .create_if_missing(true)
        .foreign_keys(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await
        .with_context(|| format!("opening sqlite pool at {}", db_path.display()))?;

    migrate(&pool).await?;

    tracing::info!(path = %db_path.display(), "database ready");
    Ok(pool)
}

/// Runs the embedded migrations, first repairing any stored checksum that only
/// differs by line endings (see [`repair_line_ending_checksums`]).
pub async fn migrate(pool: &SqlitePool) -> Result<()> {
    let migrator = sqlx::migrate!("./migrations");
    let repaired = repair_line_ending_checksums(pool, &migrator)
        .await
        .context("checking migration checksums")?;
    if repaired > 0 {
        tracing::warn!(repaired, "repaired migration checksums (line endings)");
    }
    migrator
        .run(pool)
        .await
        .context("running database migrations")?;
    Ok(())
}

/// sqlx stores a SHA-384 of each applied migration's text and refuses to start
/// when the embedded file hashes differently. Git can check the same file out
/// with LF or CRLF line endings, so a database created by one build would not
/// open in another (#4601: an LF-built database vs the CRLF release build).
/// When the stored checksum matches the embedded migration with the other line
/// endings, rewrite it to the embedded one. Any other mismatch is left alone,
/// so a genuinely edited migration still fails loudly. Returns the rows fixed.
pub async fn repair_line_ending_checksums(pool: &SqlitePool, migrator: &Migrator) -> Result<u64> {
    let table: Option<(String,)> = sqlx::query_as(
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name = '_sqlx_migrations'",
    )
    .fetch_optional(pool)
    .await?;
    if table.is_none() {
        return Ok(0);
    }

    let mut fixed = 0;
    for migration in migrator.iter() {
        let embedded: &[u8] = migration.checksum.as_ref();
        let lf = migration.sql.as_str().replace("\r\n", "\n");
        let crlf = lf.replace('\n', "\r\n");
        for variant in [lf, crlf] {
            let variant_sum = Sha384::digest(variant.as_bytes()).to_vec();
            if variant_sum == embedded {
                continue;
            }
            let result = sqlx::query(
                "UPDATE _sqlx_migrations SET checksum = ? WHERE version = ? AND checksum = ?",
            )
            .bind(embedded.to_vec())
            .bind(migration.version)
            .bind(variant_sum)
            .execute(pool)
            .await?;
            fixed += result.rows_affected();
        }
    }
    Ok(fixed)
}

/// Env var that points the app at a different data directory. The E2E suite
/// sets it so tests run against a throwaway database, never the real one.
pub const DATA_DIR_ENV: &str = "PROJECTTRUCKS_DATA_DIR";

/// The data directory override from [`DATA_DIR_ENV`], if set. It only moves the data; the test
/// hooks also need [`TEST_HOOKS`], see [`test_profile_dir`].
pub fn data_dir_override() -> Option<PathBuf> {
    std::env::var_os(DATA_DIR_ENV)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// Whether this build has the E2E test hooks: debug builds, or a release built with the `e2e`
/// feature. A released installer has none, so no environment variable can open a CDP port on
/// its webview, replay fake jobs into the real database or point it at another update feed or
/// Hub (#5614).
pub const TEST_HOOKS: bool = cfg!(any(debug_assertions, feature = "e2e"));

/// The test profile's data dir: [`DATA_DIR_ENV`] set in a build with [`TEST_HOOKS`]. Every
/// test-only behaviour is keyed on this.
pub fn test_profile_dir() -> Option<PathBuf> {
    test_profile_from(TEST_HOOKS, data_dir_override())
}

pub fn test_profile_from(hooks: bool, data_dir: Option<PathBuf>) -> Option<PathBuf> {
    data_dir.filter(|_| hooks)
}

fn resolve_db_path(app: &AppHandle) -> Result<PathBuf> {
    let mut dir = match data_dir_override() {
        Some(custom) => custom,
        None => app
            .path()
            .app_data_dir()
            .context("resolving app data dir")?,
    };
    dir.push("projecttrucks.db");
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_hooks_need_the_hooks_and_a_data_dir() {
        let dir = PathBuf::from("C:/tmp/pt-e2e");
        assert_eq!(
            test_profile_from(true, Some(dir.clone())),
            Some(dir.clone())
        );
        assert_eq!(
            test_profile_from(false, Some(dir)),
            None,
            "a released build ignores the hooks even with the data dir moved"
        );
        assert_eq!(test_profile_from(true, None), None);
    }

    async fn memory_pool() -> SqlitePool {
        let opts = SqliteConnectOptions::from_str("sqlite::memory:")
            .unwrap()
            .foreign_keys(true);
        SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await
            .unwrap()
    }

    /// Checksum of the first migration with the line endings this build did *not* embed.
    fn other_line_ending_checksum() -> (i64, Vec<u8>) {
        let migrator = sqlx::migrate!("./migrations");
        let first = migrator.iter().next().unwrap();
        let sql = first.sql.as_str();
        let other = if sql.contains("\r\n") {
            sql.replace("\r\n", "\n")
        } else {
            sql.replace('\n', "\r\n")
        };
        (first.version, Sha384::digest(other.as_bytes()).to_vec())
    }

    async fn set_checksum(pool: &SqlitePool, version: i64, checksum: &[u8]) {
        sqlx::query("UPDATE _sqlx_migrations SET checksum = ? WHERE version = ?")
            .bind(checksum.to_vec())
            .bind(version)
            .execute(pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn database_from_other_line_endings_opens() {
        let pool = memory_pool().await;
        migrate(&pool).await.unwrap();
        let (version, other) = other_line_ending_checksum();
        set_checksum(&pool, version, &other).await;

        // Without the repair sqlx refuses the database (the #4601 crash).
        assert!(sqlx::migrate!("./migrations").run(&pool).await.is_err());

        migrate(&pool).await.unwrap();
        let (stored,): (Vec<u8>,) =
            sqlx::query_as("SELECT checksum FROM _sqlx_migrations WHERE version = ?")
                .bind(version)
                .fetch_one(&pool)
                .await
                .unwrap();
        let migrator = sqlx::migrate!("./migrations");
        assert_eq!(stored, migrator.iter().next().unwrap().checksum.to_vec());
        // Running again is a no-op.
        assert_eq!(
            repair_line_ending_checksums(&pool, &migrator)
                .await
                .unwrap(),
            0
        );
    }

    #[tokio::test]
    async fn genuinely_changed_migration_still_fails() {
        let pool = memory_pool().await;
        migrate(&pool).await.unwrap();
        let (version, _) = other_line_ending_checksum();
        set_checksum(&pool, version, &[0u8; 48]).await;
        assert!(migrate(&pool).await.is_err());
    }

    #[tokio::test]
    async fn fresh_database_needs_no_repair() {
        let pool = memory_pool().await;
        let migrator = sqlx::migrate!("./migrations");
        assert_eq!(
            repair_line_ending_checksums(&pool, &migrator)
                .await
                .unwrap(),
            0
        );
        migrate(&pool).await.unwrap();
    }
}

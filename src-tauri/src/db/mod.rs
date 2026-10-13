use std::path::PathBuf;

use anyhow::{Context, Result};
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

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .context("running database migrations")?;

    tracing::info!(path = %db_path.display(), "database ready");
    Ok(pool)
}

/// Env var that points the app at a different data directory. The E2E suite
/// sets it so tests run against a throwaway database, never the real one.
pub const DATA_DIR_ENV: &str = "PROJECTTRUCKS_DATA_DIR";

/// The data directory override from [`DATA_DIR_ENV`], if set.
pub fn data_dir_override() -> Option<PathBuf> {
    std::env::var_os(DATA_DIR_ENV)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
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

use serde::Serialize;
use tauri::State;

use crate::db::AppState;

#[derive(Serialize)]
pub struct DbPing {
    pub ok: bool,
    pub message: String,
}

#[tauri::command]
pub async fn db_ping(state: State<'_, AppState>) -> Result<DbPing, String> {
    let row: (i64,) = sqlx::query_as("SELECT 1")
        .fetch_one(&state.pool)
        .await
        .map_err(|e| e.to_string())?;

    Ok(DbPing {
        ok: row.0 == 1,
        message: "DB ok".to_string(),
    })
}

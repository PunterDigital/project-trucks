mod commands;
mod db;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let handle = app.handle().clone();
            let pool = tauri::async_runtime::block_on(db::init_pool(&handle))?;
            app.manage(db::AppState { pool });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::db::db_ping,
            commands::sessions::create_session,
            commands::sessions::list_sessions,
            commands::sessions::delete_session,
            commands::jobs::create_job,
            commands::jobs::list_jobs,
            commands::jobs::get_job,
            commands::jobs::update_job,
            commands::jobs::delete_job,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

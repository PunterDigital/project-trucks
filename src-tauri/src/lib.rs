pub mod commands;
pub mod db;
pub mod telemetry;

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
            create_main_window(app)?;
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
            commands::trucks::list_trucks,
            commands::trucks::get_truck,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Env var (Windows only) that opens a Chrome DevTools Protocol port on the
/// WebView2 window so the Playwright E2E suite can drive the real app.
#[cfg(windows)]
const E2E_CDP_PORT_ENV: &str = "PROJECTTRUCKS_E2E_CDP_PORT";

/// wry's default WebView2 flags; `additional_browser_args` replaces them.
#[cfg(windows)]
const WEBVIEW2_DEFAULT_ARGS: &str =
    "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection";

/// Builds the main window from `tauri.conf.json` (where it is `create: false`)
/// so test-only overrides can be applied. WebView2 ignores its `WEBVIEW2_*`
/// env vars on some machines, so we pass them through the builder instead.
fn create_main_window(app: &tauri::App) -> tauri::Result<()> {
    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|w| w.label == "main")
        .cloned()
        .expect("tauri.conf.json defines the main window");
    let mut builder = tauri::WebviewWindowBuilder::from_config(app.handle(), &config)?;

    if let Some(dir) = db::data_dir_override() {
        builder = builder.data_directory(dir.join("webview"));
    }

    #[cfg(windows)]
    if let Some(port) = std::env::var(E2E_CDP_PORT_ENV)
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
    {
        builder = builder.additional_browser_args(&format!(
            "{WEBVIEW2_DEFAULT_ARGS} --remote-debugging-port={port}"
        ));
    }
    builder.build()?;
    Ok(())
}

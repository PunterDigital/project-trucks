pub mod commands;
pub mod db;
pub mod plugin_installer;
pub mod settings;
pub mod telemetry;
pub mod updater;

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
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| setup_app(app).inspect_err(|err| show_startup_error(err.as_ref())))
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
            commands::stats::get_dashboard_stats,
            commands::stats::get_truck_stats,
            commands::export::export_jobs_csv,
            commands::export::backup_database,
            telemetry::runner::get_telemetry_status,
            telemetry::runner::get_current_job,
            settings::get_settings,
            settings::update_settings,
            settings::detect_ets2,
            plugin_installer::telemetry_plugin_status,
            plugin_installer::install_telemetry_plugin,
            updater::get_update_status,
            updater::check_for_updates,
            updater::install_update,
            updater::open_release_page,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn setup_app(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let handle = app.handle().clone();
    let pool = tauri::async_runtime::block_on(db::init_pool(&handle))?;
    let telemetry_source = match telemetry::replay::from_env()? {
        Some((replay, every)) => telemetry::runner::Source::Replay(replay, every),
        None => telemetry::runner::Source::Live,
    };
    app.manage(telemetry::runner::TelemetryStatus::default());
    app.manage(telemetry::runner::CurrentJobState::default());
    tauri::async_runtime::block_on(settings::detect_on_first_run(&pool))?;
    let saved = tauri::async_runtime::block_on(settings::load(&pool))?;
    let auto_capture = saved.auto_capture;
    let (capture_tx, capture_rx) = tokio::sync::watch::channel(auto_capture);
    app.manage(settings::AutoCapture(capture_tx));
    tauri::async_runtime::spawn(telemetry::runner::run(
        handle.clone(),
        pool.clone(),
        telemetry_source,
        capture_rx,
    ));
    app.manage(db::AppState { pool });
    let updater = updater::Updater::new(&handle);
    updater.clean_old_downloads();
    app.manage(updater);
    if saved.check_updates {
        updater::spawn_launch_check(handle.clone());
    }
    create_main_window(app)?;
    Ok(())
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
        // E2E runs on Shay's PC while he games: the test window must not take focus or sit in
        // the taskbar, and must keep rendering while the game covers it (Chromium otherwise
        // throttles occluded windows, which stalls the tests).
        builder = builder
            .focused(false)
            .skip_taskbar(true)
            .additional_browser_args(&format!(
                "{WEBVIEW2_DEFAULT_ARGS},CalculateNativeWinOcclusion \
                 --disable-backgrounding-occluded-windows --disable-renderer-backgrounding \
                 --remote-debugging-port={port}"
            ));
    }
    builder.build()?;
    Ok(())
}

/// The error and its causes on one line ("a: b: c"), for the startup dialog.
fn error_chain(err: &(dyn std::error::Error + 'static)) -> String {
    let mut parts = vec![err.to_string()];
    let mut source = err.source();
    while let Some(cause) = source {
        parts.push(cause.to_string());
        source = cause.source();
    }
    parts.join(": ")
}

/// A failed setup used to end the process before any window existed, so the app
/// just never appeared (#4601). Tell the user why before Tauri exits. The E2E
/// suite (CDP port set) gets no dialog, since nobody would close it.
fn show_startup_error(err: &(dyn std::error::Error + 'static)) {
    let message = error_chain(err);
    tracing::error!(%message, "startup failed");
    #[cfg(windows)]
    if std::env::var_os(E2E_CDP_PORT_ENV).is_none() {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};
        let wide = |s: &str| -> Vec<u16> {
            std::ffi::OsStr::new(s)
                .encode_wide()
                .chain(std::iter::once(0))
                .collect()
        };
        let text = wide(&format!(
            "ProjectTrucks couldn't start.\n\n{message}\n\nPlease report this at \
             https://github.com/PunterDigital/project-trucks/issues"
        ));
        let title = wide("ProjectTrucks");
        // SAFETY: both strings are NUL-terminated UTF-16 buffers that outlive the call.
        unsafe {
            MessageBoxW(
                std::ptr::null_mut(),
                text.as_ptr(),
                title.as_ptr(),
                MB_OK | MB_ICONERROR,
            );
        }
    }
}

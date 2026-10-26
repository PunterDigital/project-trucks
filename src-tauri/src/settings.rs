//! App settings (the `settings` key/value table) and the Steam lookup that finds ETS2.
//!
//! Keys: `auto_capture` ("true"/"false", default off), `ets2_install_path`, `units` ("km"/"mi") and
//! `check_updates` ("true"/"false", default on: check GitHub for a newer release on launch).
//! Changing `auto_capture` flips the [`AutoCapture`] channel the telemetry task listens on, so
//! polling starts or stops within a tick.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tauri::State;
use tokio::sync::watch;

use crate::db::AppState;

const KEY_AUTO_CAPTURE: &str = "auto_capture";
const KEY_ETS2_PATH: &str = "ets2_install_path";
const KEY_UNITS: &str = "units";
const KEY_PLUGIN_PROMPT: &str = "plugin_prompt_dismissed";
const KEY_CHECK_UPDATES: &str = "check_updates";

/// Steam app id of Euro Truck Simulator 2.
const ETS2_APP_ID: &str = "227300";

/// Sender half of the "is auto-capture on" channel, as Tauri managed state.
pub struct AutoCapture(pub watch::Sender<bool>);

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub auto_capture: bool,
    pub ets2_install_path: Option<String>,
    /// `"km"` or `"mi"`.
    pub units: String,
    /// SHA-256 of the bundled plugin the user last said "Not now" to; the first-launch prompt
    /// stays quiet until the app ships a different plugin.
    pub plugin_prompt_dismissed: Option<String>,
    /// Check for a newer release when the app starts (default on).
    pub check_updates: bool,
}

/// Fields to change; absent ones are left alone. An empty `ets2_install_path` clears it.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsPatch {
    pub auto_capture: Option<bool>,
    pub ets2_install_path: Option<String>,
    pub units: Option<String>,
    pub plugin_prompt_dismissed: Option<String>,
    pub check_updates: Option<bool>,
}

async fn get(pool: &SqlitePool, key: &str) -> Result<Option<String>, sqlx::Error> {
    let row: Option<(String,)> = sqlx::query_as("SELECT value FROM settings WHERE key = ?")
        .bind(key)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|r| r.0))
}

async fn put(pool: &SqlitePool, key: &str, value: &str) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO settings (key, value) VALUES (?, ?) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind(key)
    .bind(value)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn load(pool: &SqlitePool) -> Result<Settings, sqlx::Error> {
    let units = get(pool, KEY_UNITS).await?;
    Ok(Settings {
        auto_capture: get(pool, KEY_AUTO_CAPTURE).await?.as_deref() == Some("true"),
        ets2_install_path: get(pool, KEY_ETS2_PATH).await?.filter(|p| !p.is_empty()),
        units: match units.as_deref() {
            Some("mi") => "mi".to_string(),
            _ => "km".to_string(),
        },
        plugin_prompt_dismissed: get(pool, KEY_PLUGIN_PROMPT)
            .await?
            .filter(|p| !p.is_empty()),
        check_updates: get(pool, KEY_CHECK_UPDATES).await?.as_deref() != Some("false"),
    })
}

/// On startup: if no path is saved yet, look for the game and save what turns up.
pub async fn detect_on_first_run(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    if load(pool).await?.ets2_install_path.is_none() {
        if let Some(found) = detect_ets2_path() {
            put(pool, KEY_ETS2_PATH, &found.to_string_lossy()).await?;
        }
    }
    Ok(())
}

pub async fn apply(pool: &SqlitePool, patch: SettingsPatch) -> Result<Settings, String> {
    if let Some(units) = &patch.units {
        if units != "km" && units != "mi" {
            return Err(format!("units must be \"km\" or \"mi\", not {units:?}"));
        }
    }
    let io = |e: sqlx::Error| e.to_string();
    if let Some(on) = patch.auto_capture {
        put(pool, KEY_AUTO_CAPTURE, if on { "true" } else { "false" })
            .await
            .map_err(io)?;
    }
    if let Some(path) = &patch.ets2_install_path {
        put(pool, KEY_ETS2_PATH, path.trim()).await.map_err(io)?;
    }
    if let Some(units) = &patch.units {
        put(pool, KEY_UNITS, units).await.map_err(io)?;
    }
    if let Some(on) = patch.check_updates {
        put(pool, KEY_CHECK_UPDATES, if on { "true" } else { "false" })
            .await
            .map_err(io)?;
    }
    if let Some(sha) = &patch.plugin_prompt_dismissed {
        put(pool, KEY_PLUGIN_PROMPT, sha.trim()).await.map_err(io)?;
    }
    load(pool).await.map_err(io)
}

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<Settings, String> {
    load(&state.pool).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn update_settings(
    state: State<'_, AppState>,
    capture: State<'_, AutoCapture>,
    patch: SettingsPatch,
) -> Result<Settings, String> {
    let settings = apply(&state.pool, patch).await?;
    capture.0.send_replace(settings.auto_capture);
    Ok(settings)
}

/// Looks for ETS2 in Steam's libraries; `None` if Steam or the game is not found.
#[tauri::command]
pub async fn detect_ets2() -> Option<String> {
    detect_ets2_path().map(|p| p.to_string_lossy().into_owned())
}

pub fn detect_ets2_path() -> Option<PathBuf> {
    for steam in steam_roots() {
        let libraries = std::fs::read_to_string(steam.join("steamapps").join("libraryfolders.vdf"))
            .map(|text| library_paths(&text))
            .unwrap_or_default();
        // The Steam folder is itself a library, whether or not the file lists it.
        let candidates =
            std::iter::once(steam.clone()).chain(libraries.into_iter().map(PathBuf::from));
        for library in candidates {
            if let Some(dir) = ets2_dir_in_library(&library) {
                return Some(dir);
            }
        }
    }
    None
}

fn ets2_dir_in_library(library: &Path) -> Option<PathBuf> {
    let apps = library.join("steamapps");
    let manifest =
        std::fs::read_to_string(apps.join(format!("appmanifest_{ETS2_APP_ID}.acf"))).ok()?;
    let install_dir = vdf_values(&manifest, "installdir").into_iter().next()?;
    let dir = apps.join("common").join(install_dir);
    dir.is_dir().then_some(dir)
}

fn steam_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    #[cfg(windows)]
    if let Some(p) = steam_path_from_registry() {
        roots.push(p);
    }
    for var in ["ProgramFiles(x86)", "ProgramFiles"] {
        if let Some(base) = std::env::var_os(var) {
            roots.push(PathBuf::from(base).join("Steam"));
        }
    }
    roots
}

#[cfg(windows)]
fn steam_path_from_registry() -> Option<PathBuf> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let out = std::process::Command::new("reg")
        .args(["query", r"HKCU\Software\Valve\Steam", "/v", "SteamPath"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    parse_reg_steam_path(&String::from_utf8_lossy(&out.stdout))
}

/// `reg query` prints `    SteamPath    REG_SZ    c:/program files (x86)/steam`.
#[cfg_attr(not(windows), allow(dead_code))]
fn parse_reg_steam_path(output: &str) -> Option<PathBuf> {
    output.lines().find_map(|line| {
        let rest = line.trim().strip_prefix("SteamPath")?;
        let (_, value) = rest.split_once("REG_SZ")?;
        let value = value.trim();
        (!value.is_empty()).then(|| PathBuf::from(value))
    })
}

/// Library folders listed in Steam's `libraryfolders.vdf` (the `"path"` entries).
pub fn library_paths(vdf: &str) -> Vec<String> {
    vdf_values(vdf, "path")
}

/// Every value of `"key" "value"` pairs in Valve's KeyValues text format, with `\\` unescaped.
fn vdf_values(vdf: &str, key: &str) -> Vec<String> {
    vdf.lines()
        .filter_map(|line| {
            let mut quoted = line.trim().split('"').skip(1).step_by(2);
            let (k, v) = (quoted.next()?, quoted.next()?);
            k.eq_ignore_ascii_case(key)
                .then(|| v.replace("\\\\", "\\").replace("\\\"", "\""))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIBRARY_FOLDERS: &str = r#"
"libraryfolders"
{
	"0"
	{
		"path"		"C:\\Program Files (x86)\\Steam"
		"label"		""
		"apps"
		{
			"228980"		"123"
		}
	}
	"1"
	{
		"path"		"D:\\SteamLibrary"
		"apps"
		{
			"227300"		"4567890"
		}
	}
}
"#;

    #[test]
    fn library_paths_are_unescaped() {
        assert_eq!(
            library_paths(LIBRARY_FOLDERS),
            vec![r"C:\Program Files (x86)\Steam", r"D:\SteamLibrary"]
        );
    }

    #[test]
    fn manifest_install_dir() {
        let acf = "\"AppState\"\n{\n\t\"appid\"\t\t\"227300\"\n\t\"installdir\"\t\t\"Euro Truck Simulator 2\"\n}\n";
        assert_eq!(
            vdf_values(acf, "installdir"),
            vec!["Euro Truck Simulator 2"]
        );
        assert_eq!(vdf_values(acf, "missing"), Vec::<String>::new());
    }

    #[test]
    fn registry_output() {
        let out = "\r\nHKEY_CURRENT_USER\\Software\\Valve\\Steam\r\n    SteamPath    REG_SZ    c:/program files (x86)/steam\r\n\r\n";
        assert_eq!(
            parse_reg_steam_path(out),
            Some(PathBuf::from("c:/program files (x86)/steam"))
        );
        assert_eq!(parse_reg_steam_path("ERROR: not found"), None);
    }

    #[test]
    fn finds_the_game_in_a_library() {
        let root = std::env::temp_dir().join(format!("pt-steam-{}", std::process::id()));
        let apps = root.join("steamapps");
        std::fs::create_dir_all(apps.join("common").join("Euro Truck Simulator 2")).unwrap();
        std::fs::write(
            apps.join("appmanifest_227300.acf"),
            "\"AppState\"\n{\n\t\"installdir\"\t\t\"Euro Truck Simulator 2\"\n}\n",
        )
        .unwrap();
        assert_eq!(
            ets2_dir_in_library(&root),
            Some(apps.join("common").join("Euro Truck Simulator 2"))
        );
        assert_eq!(ets2_dir_in_library(&root.join("nope")), None);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[tokio::test]
    async fn settings_round_trip_with_defaults() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();

        let defaults = load(&pool).await.unwrap();
        assert!(!defaults.auto_capture);
        assert_eq!(defaults.ets2_install_path, None);
        assert_eq!(defaults.units, "km");
        assert_eq!(defaults.plugin_prompt_dismissed, None);
        assert!(defaults.check_updates);

        let saved = apply(
            &pool,
            SettingsPatch {
                auto_capture: Some(true),
                ets2_install_path: Some(r" D:\Games\ETS2 ".into()),
                units: Some("mi".into()),
                plugin_prompt_dismissed: Some("abc123".into()),
                check_updates: Some(false),
            },
        )
        .await
        .unwrap();
        assert_eq!(saved.plugin_prompt_dismissed.as_deref(), Some("abc123"));
        assert!(!saved.check_updates);
        assert!(saved.auto_capture);
        assert_eq!(saved.ets2_install_path.as_deref(), Some(r"D:\Games\ETS2"));
        assert_eq!(saved.units, "mi");

        // Partial patches leave the rest alone; an empty path clears it.
        let cleared = apply(
            &pool,
            SettingsPatch {
                ets2_install_path: Some(String::new()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert!(cleared.auto_capture);
        assert_eq!(cleared.ets2_install_path, None);
        assert_eq!(cleared.plugin_prompt_dismissed.as_deref(), Some("abc123"));
        assert!(!cleared.check_updates);

        assert!(apply(
            &pool,
            SettingsPatch {
                units: Some("furlongs".into()),
                ..Default::default()
            }
        )
        .await
        .is_err());
    }
}

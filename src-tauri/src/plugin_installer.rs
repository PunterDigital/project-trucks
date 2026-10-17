//! Installs the bundled ETS2 telemetry plugin DLL into the game folder.
//!
//! The DLL ships as a Tauri resource (`bundle.resources` in `tauri.conf.json`). Installing copies
//! it to `<ets2>/bin/win_x64/plugins/ets2-telemetry-server.dll`, keeping any other DLL that was
//! there as `ets2-telemetry-server.dll.bak`. The status compares SHA-256 hashes, so a plugin
//! that came with an older app version shows as outdated.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager, State};

use crate::db::AppState;
use crate::settings;

const DLL_NAME: &str = "ets2-telemetry-server.dll";
const RESOURCE_PATH: &str = "telemetry-plugin/ets2-telemetry-server.dll";
/// The game's 64-bit executable; its presence is how a folder "looks like ETS2".
const GAME_EXE: &str = "eurotrucks2.exe";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PluginState {
    /// No ETS2 folder saved.
    NoFolder,
    /// The saved folder does not look like ETS2.
    BadFolder,
    NotInstalled,
    /// A DLL is there, but it is not the one this app ships.
    Outdated,
    Installed,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginStatus {
    pub state: PluginState,
    /// Where the DLL goes (or is), when the folder is usable.
    pub plugin_path: Option<String>,
}

fn plugins_dir(game: &Path) -> PathBuf {
    game.join("bin").join("win_x64").join("plugins")
}

fn looks_like_ets2(game: &Path) -> bool {
    game.join("bin").join("win_x64").join(GAME_EXE).is_file()
}

fn sha256_file(path: &Path) -> std::io::Result<[u8; 32]> {
    Ok(Sha256::digest(std::fs::read(path)?).into())
}

pub fn status_of(game: Option<&str>, bundled: &Path) -> Result<PluginStatus, String> {
    let Some(game) = game.map(str::trim).filter(|g| !g.is_empty()) else {
        return Ok(PluginStatus {
            state: PluginState::NoFolder,
            plugin_path: None,
        });
    };
    let game = Path::new(game);
    if !looks_like_ets2(game) {
        return Ok(PluginStatus {
            state: PluginState::BadFolder,
            plugin_path: None,
        });
    }
    let target = plugins_dir(game).join(DLL_NAME);
    let state = match sha256_file(&target) {
        Ok(have) => {
            let want = sha256_file(bundled).map_err(|e| {
                format!(
                    "Cannot read the bundled plugin ({}): {e}",
                    bundled.display()
                )
            })?;
            if have == want {
                PluginState::Installed
            } else {
                PluginState::Outdated
            }
        }
        Err(e) if e.kind() == ErrorKind::NotFound => PluginState::NotInstalled,
        Err(e) => return Err(io_message("read", &target, &e)),
    };
    Ok(PluginStatus {
        state,
        plugin_path: Some(target.to_string_lossy().into_owned()),
    })
}

fn io_message(action: &str, path: &Path, e: &std::io::Error) -> String {
    if e.kind() == ErrorKind::PermissionDenied {
        format!(
            "Windows denied access to {} (while trying to {action} it). \
             The game folder needs administrator rights: run ProjectTrucks as administrator, \
             or move ETS2 out of Program Files.",
            path.display()
        )
    } else {
        format!("Could not {action} {}: {e}", path.display())
    }
}

/// Copies the bundled DLL into the game folder, backing up a different DLL already there.
pub fn install(game: &Path, bundled: &Path) -> Result<PluginStatus, String> {
    if !looks_like_ets2(game) {
        return Err(format!(
            "{} does not look like an ETS2 folder (bin/win_x64/{GAME_EXE} is missing).",
            game.display()
        ));
    }
    let dir = plugins_dir(game);
    std::fs::create_dir_all(&dir).map_err(|e| io_message("create", &dir, &e))?;
    let target = dir.join(DLL_NAME);
    let game_str = game.to_string_lossy();
    match status_of(Some(&game_str), bundled)?.state {
        PluginState::Installed => {}
        state => {
            if state == PluginState::Outdated {
                let backup = dir.join(format!("{DLL_NAME}.bak"));
                std::fs::copy(&target, &backup)
                    .map_err(|e| io_message("back up to", &backup, &e))?;
            }
            std::fs::copy(bundled, &target).map_err(|e| io_message("write", &target, &e))?;
        }
    }
    status_of(Some(&game_str), bundled)
}

fn bundled_dll(app: &AppHandle) -> Result<PathBuf, String> {
    let path = app
        .path()
        .resolve(RESOURCE_PATH, tauri::path::BaseDirectory::Resource)
        .map_err(|e| e.to_string())?;
    if path.is_file() {
        Ok(path)
    } else {
        Err(format!(
            "The bundled plugin is missing ({}).",
            path.display()
        ))
    }
}

async fn saved_folder(state: &State<'_, AppState>) -> Result<Option<String>, String> {
    Ok(settings::load(&state.pool)
        .await
        .map_err(|e| e.to_string())?
        .ets2_install_path)
}

#[tauri::command]
pub async fn telemetry_plugin_status(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<PluginStatus, String> {
    let folder = saved_folder(&state).await?;
    let bundled = bundled_dll(&app)?;
    status_of(folder.as_deref(), &bundled)
}

#[tauri::command]
pub async fn install_telemetry_plugin(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<PluginStatus, String> {
    let folder = saved_folder(&state)
        .await?
        .ok_or("Set the ETS2 folder first.")?;
    let bundled = bundled_dll(&app)?;
    install(Path::new(folder.trim()), &bundled)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake {
        root: PathBuf,
        game: PathBuf,
        bundled: PathBuf,
    }

    impl Fake {
        fn new(name: &str) -> Self {
            let root =
                std::env::temp_dir().join(format!("pt-plugin-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            let game = root.join("ets2");
            std::fs::create_dir_all(game.join("bin").join("win_x64")).unwrap();
            std::fs::write(game.join("bin").join("win_x64").join(GAME_EXE), b"exe").unwrap();
            let bundled = root.join("bundled.dll");
            std::fs::write(&bundled, b"new plugin").unwrap();
            Fake {
                root,
                game,
                bundled,
            }
        }
        fn target(&self) -> PathBuf {
            plugins_dir(&self.game).join(DLL_NAME)
        }
        fn backup(&self) -> PathBuf {
            plugins_dir(&self.game).join(format!("{DLL_NAME}.bak"))
        }
        fn state(&self) -> PluginState {
            status_of(Some(&self.game.to_string_lossy()), &self.bundled)
                .unwrap()
                .state
        }
    }

    impl Drop for Fake {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn fresh_install_creates_the_plugins_folder() {
        let f = Fake::new("fresh");
        assert_eq!(f.state(), PluginState::NotInstalled);
        assert_eq!(
            install(&f.game, &f.bundled).unwrap().state,
            PluginState::Installed
        );
        assert_eq!(std::fs::read(f.target()).unwrap(), b"new plugin");
        assert!(!f.backup().exists());
        assert_eq!(f.state(), PluginState::Installed);
    }

    #[test]
    fn a_different_dll_is_outdated_and_gets_backed_up() {
        let f = Fake::new("outdated");
        std::fs::create_dir_all(plugins_dir(&f.game)).unwrap();
        std::fs::write(f.target(), b"old plugin").unwrap();
        assert_eq!(f.state(), PluginState::Outdated);

        assert_eq!(
            install(&f.game, &f.bundled).unwrap().state,
            PluginState::Installed
        );
        assert_eq!(std::fs::read(f.target()).unwrap(), b"new plugin");
        assert_eq!(std::fs::read(f.backup()).unwrap(), b"old plugin");
    }

    #[test]
    fn installing_twice_changes_nothing() {
        let f = Fake::new("twice");
        install(&f.game, &f.bundled).unwrap();
        install(&f.game, &f.bundled).unwrap();
        assert!(!f.backup().exists());
    }

    #[test]
    fn bad_and_missing_folders() {
        let f = Fake::new("bad");
        let other = f.root.join("not-a-game");
        std::fs::create_dir_all(&other).unwrap();
        let bad = status_of(Some(&other.to_string_lossy()), &f.bundled).unwrap();
        assert_eq!(bad.state, PluginState::BadFolder);
        assert!(install(&other, &f.bundled)
            .unwrap_err()
            .contains("does not look like an ETS2 folder"));
        assert!(!other.join("bin").exists());

        assert_eq!(
            status_of(None, &f.bundled).unwrap().state,
            PluginState::NoFolder
        );
        assert_eq!(
            status_of(Some("  "), &f.bundled).unwrap().state,
            PluginState::NoFolder
        );
    }

    #[test]
    fn permission_errors_explain_administrator_rights() {
        let e = std::io::Error::from(ErrorKind::PermissionDenied);
        let msg = io_message("write", Path::new(r"C:\Program Files\ETS2\x.dll"), &e);
        assert!(msg.contains("administrator"), "{msg}");
    }
}

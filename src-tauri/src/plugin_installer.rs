//! Installs the bundled ETS2 telemetry plugin DLL into the game folder.
//!
//! The DLL (RenCloud's scs-telemetry since #4619) ships as a Tauri resource (`bundle.resources`
//! in `tauri.conf.json`). Installing copies it to `<ets2>/bin/win_x64/plugins/scs-telemetry.dll`,
//! keeping a different `scs-telemetry.dll` that was there as `scs-telemetry.dll.bak` (the DLL has
//! no version resource, so an older and a newer build can't be told apart). It also removes the
//! plugin earlier versions installed (Funbit's `ets2-telemetry-server.dll`, only when it is the
//! exact file we shipped) and the `.bak` copy that installer made, so the game loads one
//! ProjectTrucks plugin. Another app's DLL is never touched. The status compares SHA-256 hashes,
//! so a plugin that came with an older app version (or the old plugin left behind) shows as
//! outdated.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager, State};

use crate::db::AppState;
use crate::settings;

const DLL_NAME: &str = "scs-telemetry.dll";
const RESOURCE_PATH: &str = "telemetry-plugin/scs-telemetry.dll";
/// The plugin ProjectTrucks installed before #4619.
const OLD_DLL_NAME: &str = "ets2-telemetry-server.dll";
/// SHA-256 of that DLL as we shipped it (Funbit/ets2-telemetry-server 3.2.5, win_x64), so only our
/// own copy is ever removed.
const OLD_DLL_SHA: [u8; 32] =
    hex32("9923e368d5e1abb19f67fbc560f6213433ebc51d664b6227785486f6bc36c160");
/// The game's 64-bit executable; its presence is how a folder "looks like ETS2".
pub(crate) const GAME_EXE: &str = "eurotrucks2.exe";

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
    /// SHA-256 (hex) of the plugin this app ships; the first-launch prompt is dismissed per value.
    pub bundled_sha: Option<String>,
}

fn plugins_dir(game: &Path) -> PathBuf {
    game.join("bin").join("win_x64").join("plugins")
}

fn looks_like_ets2(game: &Path) -> bool {
    game.join("bin").join("win_x64").join(GAME_EXE).is_file()
}

const fn hex32(s: &str) -> [u8; 32] {
    const fn nibble(c: u8) -> u8 {
        match c {
            b'0'..=b'9' => c - b'0',
            b'a'..=b'f' => c - b'a' + 10,
            _ => panic!("not lowercase hex"),
        }
    }
    let b = s.as_bytes();
    assert!(b.len() == 64);
    let mut out = [0u8; 32];
    let mut i = 0;
    while i < 32 {
        out[i] = (nibble(b[2 * i]) << 4) | nibble(b[2 * i + 1]);
        i += 1;
    }
    out
}

/// Files an older ProjectTrucks left in the plugins folder: our Funbit DLL (only if unchanged)
/// and the `.bak` our old installer made when it replaced a different one.
fn leftovers(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let old = dir.join(OLD_DLL_NAME);
    if sha256_file(&old).is_ok_and(|h| h == OLD_DLL_SHA) {
        found.push(old);
    }
    let old_backup = dir.join(format!("{OLD_DLL_NAME}.bak"));
    if old_backup.is_file() {
        found.push(old_backup);
    }
    found
}

fn sha256_file(path: &Path) -> std::io::Result<[u8; 32]> {
    Ok(Sha256::digest(std::fs::read(path)?).into())
}

fn sha_hex(bundled: &Path) -> Option<String> {
    let hash = sha256_file(bundled).ok()?;
    Some(hash.iter().map(|b| format!("{b:02x}")).collect())
}

pub fn status_of(game: Option<&str>, bundled: &Path) -> Result<PluginStatus, String> {
    let bundled_sha = sha_hex(bundled);
    let Some(game) = game.map(str::trim).filter(|g| !g.is_empty()) else {
        return Ok(PluginStatus {
            state: PluginState::NoFolder,
            plugin_path: None,
            bundled_sha,
        });
    };
    let game = Path::new(game);
    if !looks_like_ets2(game) {
        return Ok(PluginStatus {
            state: PluginState::BadFolder,
            plugin_path: None,
            bundled_sha,
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
    // The old plugin still there: "Update" removes it.
    let state = if leftovers(&plugins_dir(game)).is_empty() {
        state
    } else {
        PluginState::Outdated
    };
    Ok(PluginStatus {
        state,
        plugin_path: Some(target.to_string_lossy().into_owned()),
        bundled_sha,
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
    let want = sha256_file(bundled).map_err(|e| {
        format!(
            "Cannot read the bundled plugin ({}): {e}",
            bundled.display()
        )
    })?;
    match sha256_file(&target) {
        Ok(have) if have == want => {}
        Ok(_) => {
            let backup = dir.join(format!("{DLL_NAME}.bak"));
            std::fs::copy(&target, &backup).map_err(|e| io_message("back up to", &backup, &e))?;
            std::fs::copy(bundled, &target).map_err(|e| io_message("write", &target, &e))?;
        }
        Err(e) if e.kind() == ErrorKind::NotFound => {
            std::fs::copy(bundled, &target).map_err(|e| io_message("write", &target, &e))?;
        }
        Err(e) => return Err(io_message("read", &target, &e)),
    }
    for old in leftovers(&dir) {
        std::fs::remove_file(&old).map_err(|e| io_message("remove", &old, &e))?;
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
    fn hex32_matches_sha_hex() {
        let f = Fake::new("hex");
        let hex = sha_hex(&f.bundled).unwrap();
        assert_eq!(hex32(&hex), sha256_file(&f.bundled).unwrap());
    }

    fn put(path: PathBuf, bytes: &[u8]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    /// Copies the Funbit DLL we used to ship (kept in assets for provenance) into the fake game.
    fn put_our_old_dll(f: &Fake) {
        let real = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../assets/telemetry-plugin/win_x64")
            .join(OLD_DLL_NAME);
        assert_eq!(
            sha256_file(&real).unwrap(),
            OLD_DLL_SHA,
            "OLD_DLL_SHA drifted"
        );
        put(
            plugins_dir(&f.game).join(OLD_DLL_NAME),
            &std::fs::read(&real).unwrap(),
        );
    }

    #[test]
    fn updating_removes_our_old_plugin_and_its_backup() {
        let f = Fake::new("old-ours");
        put_our_old_dll(&f);
        put(
            plugins_dir(&f.game).join(format!("{OLD_DLL_NAME}.bak")),
            b"what was there first",
        );
        // Only the old plugin: an update, not a fresh install.
        assert_eq!(f.state(), PluginState::Outdated);

        assert_eq!(
            install(&f.game, &f.bundled).unwrap().state,
            PluginState::Installed
        );
        assert_eq!(std::fs::read(f.target()).unwrap(), b"new plugin");
        assert!(!plugins_dir(&f.game).join(OLD_DLL_NAME).exists());
        assert!(!plugins_dir(&f.game)
            .join(format!("{OLD_DLL_NAME}.bak"))
            .exists());
    }

    #[test]
    fn the_new_plugin_with_our_old_one_still_there_is_outdated() {
        let f = Fake::new("both");
        install(&f.game, &f.bundled).unwrap();
        put_our_old_dll(&f);
        assert_eq!(f.state(), PluginState::Outdated);
        install(&f.game, &f.bundled).unwrap();
        assert_eq!(f.state(), PluginState::Installed);
        assert!(
            !f.backup().exists(),
            "the right scs-telemetry.dll is not backed up"
        );
    }

    #[test]
    fn other_apps_plugins_are_left_alone() {
        let f = Fake::new("others");
        // Someone else's Funbit build, and another tracker's DLL.
        put(
            plugins_dir(&f.game).join(OLD_DLL_NAME),
            b"another app's funbit build",
        );
        put(
            plugins_dir(&f.game).join("ETSSharedMemoryMapPlugin64v2.dll"),
            b"other tracker",
        );
        assert_eq!(f.state(), PluginState::NotInstalled);
        install(&f.game, &f.bundled).unwrap();
        assert_eq!(f.state(), PluginState::Installed);
        assert_eq!(
            std::fs::read(plugins_dir(&f.game).join(OLD_DLL_NAME)).unwrap(),
            b"another app's funbit build"
        );
        assert!(plugins_dir(&f.game)
            .join("ETSSharedMemoryMapPlugin64v2.dll")
            .exists());
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

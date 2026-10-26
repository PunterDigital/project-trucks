//! One-click app updates (card #3354).
//!
//! On launch (when the `check_updates` setting is on) and from Settings → "Check now", the app
//! asks GitHub for the latest release. When it is newer than this build, the banner offers
//! "Update to vX.Y.Z": the app downloads the release's `_x64-setup.exe` into its own temp folder,
//! checks the size and the SHA-256 GitHub publishes for the asset, starts the NSIS installer in
//! passive mode (`/P /R /UPDATE`: progress bar only, relaunch when done, keep shortcuts) and exits
//! so the installer can replace the files. No Tauri updater plugin, so no update-signing key.
//!
//! Test profiles (`PROJECTTRUCKS_DATA_DIR` set) never talk to GitHub: they check only when
//! `PROJECTTRUCKS_UPDATE_FEED` points at a fake feed (the E2E suite serves one from Node), and the
//! download goes under the data dir. Debug builds outside a test profile don't check at all.
//! Builds for other platforms can't replace themselves (unsigned), so they only link to the
//! release page.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::io::AsyncWriteExt;

use crate::db;

/// Test-profile-only override for the release feed (a GitHub "latest release" JSON document).
pub const FEED_ENV: &str = "PROJECTTRUCKS_UPDATE_FEED";
const GITHUB_FEED: &str =
    "https://api.github.com/repos/PunterDigital/project-trucks/releases/latest";
/// Suffix of the NSIS installer `tauri-action` attaches to each release.
const INSTALLER_SUFFIX: &str = "_x64-setup.exe";
/// Event carrying the new [`UpdateStatus`] whenever it changes.
pub const STATUS_EVENT: &str = "update:status";
/// Flags understood by Tauri's NSIS template (`installer.nsi`): `/P` passive (progress bar, no
/// questions), `/R` start the app again when done, `/UPDATE` update in place (skips the reinstall
/// page and doesn't recreate shortcuts the user removed).
pub const INSTALLER_ARGS: [&str; 3] = ["/P", "/R", "/UPDATE"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum UpdateState {
    /// No checks in this build or profile (`message` says why).
    Disabled,
    /// Nothing checked yet (the setting is off, or the launch check hasn't run).
    Idle,
    Checking,
    UpToDate,
    Available,
    Downloading,
    /// The installer has been started; the app is about to exit.
    Installing,
    Error,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LatestRelease {
    /// Without the leading `v`.
    pub version: String,
    pub page_url: String,
    /// `true` when this build can download and run the installer itself (Windows with an
    /// `_x64-setup.exe` asset); otherwise the banner links to the release page.
    pub can_install: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    pub current_version: String,
    pub state: UpdateState,
    /// The newer release, while there is one (kept through download errors so the banner can
    /// offer "Try again").
    pub latest: Option<LatestRelease>,
    /// Download progress in bytes (`total` is 0 when unknown).
    pub received: u64,
    pub total: u64,
    /// Error text for `error`, the reason for `disabled`.
    pub message: Option<String>,
}

/// The parts of GitHub's release JSON we use.
#[derive(Debug, Clone, Deserialize)]
pub struct GhRelease {
    pub tag_name: String,
    pub html_url: String,
    #[serde(default)]
    pub assets: Vec<GhAsset>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct GhAsset {
    pub name: String,
    pub browser_download_url: String,
    #[serde(default)]
    pub size: u64,
    /// `sha256:<hex>`; GitHub fills it in for assets uploaded since mid 2025.
    #[serde(default)]
    pub digest: Option<String>,
}

/// Where update checks go.
#[derive(Debug, Clone, PartialEq)]
pub enum Feed {
    Off(&'static str),
    Url { url: String, allow_http: bool },
}

/// Test profiles only use an explicit fake feed; debug builds don't check; release builds ask
/// GitHub. The env var is ignored outside a test profile, so it can't redirect a real install.
pub fn resolve_feed(test_profile: bool, debug_build: bool, env_feed: Option<String>) -> Feed {
    let env_feed = env_feed
        .map(|u| u.trim().to_string())
        .filter(|u| !u.is_empty());
    if test_profile {
        match env_feed {
            Some(url) => Feed::Url {
                url,
                allow_http: true,
            },
            None => Feed::Off("Update checks are off in test profiles."),
        }
    } else if debug_build {
        Feed::Off("Update checks are off in development builds.")
    } else {
        Feed::Url {
            url: GITHUB_FEED.to_string(),
            allow_http: false,
        }
    }
}

/// `v1.2.3` / `1.2.3` / `1.2.3-beta.1` → a semver version.
pub fn parse_version(tag: &str) -> Option<semver::Version> {
    semver::Version::parse(tag.trim().trim_start_matches(['v', 'V'])).ok()
}

/// Is the release tagged `tag` newer than `current`? `None` when the tag isn't a version.
pub fn is_newer(current: &semver::Version, tag: &str) -> Option<bool> {
    parse_version(tag).map(|latest| latest > *current)
}

/// A plain file name (no path separators, no leading dot), safe to join onto our temp folder.
fn safe_file_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// The release's NSIS installer (`*_x64-setup.exe`), if it has one.
pub fn pick_installer(assets: &[GhAsset]) -> Option<&GhAsset> {
    assets.iter().find(|a| {
        safe_file_name(&a.name) && a.name.to_ascii_lowercase().ends_with(INSTALLER_SUFFIX)
    })
}

/// The lowercase hex SHA-256 from a GitHub `digest` (`sha256:<64 hex>`), if there is one.
pub fn expected_sha256(digest: Option<&str>) -> Option<String> {
    let hex = digest?.trim().strip_prefix("sha256:")?;
    (hex.len() == 64 && hex.chars().all(|c| c.is_ascii_hexdigit()))
        .then(|| hex.to_ascii_lowercase())
}

/// Checks a finished download against the asset's published size and digest.
pub fn verify_download(received: u64, sha256_hex: &str, asset: &GhAsset) -> Result<(), String> {
    if asset.size > 0 && received != asset.size {
        return Err(format!(
            "The download was {received} bytes, expected {}. Try again.",
            asset.size
        ));
    }
    if let Some(want) = expected_sha256(asset.digest.as_deref()) {
        if !want.eq_ignore_ascii_case(sha256_hex) {
            return Err("The download didn't match the release's checksum. Try again.".into());
        }
    }
    Ok(())
}

fn check_scheme(url: &str, allow_http: bool) -> Result<(), String> {
    if url.starts_with("https://") || (allow_http && url.starts_with("http://")) {
        Ok(())
    } else {
        Err(format!("Refusing to download from {url}"))
    }
}

struct Inner {
    status: UpdateStatus,
    asset: Option<GhAsset>,
}

/// Managed state behind the update commands.
pub struct Updater {
    feed: Feed,
    current: semver::Version,
    download_dir: PathBuf,
    client: reqwest::Client,
    inner: Mutex<Inner>,
    /// Held while a check or a download runs, so they never overlap.
    busy: tokio::sync::Mutex<()>,
}

impl Updater {
    pub fn new(app: &AppHandle) -> Self {
        let test_dir = db::data_dir_override();
        let feed = resolve_feed(
            test_dir.is_some(),
            cfg!(debug_assertions),
            std::env::var(FEED_ENV).ok(),
        );
        let current = app.package_info().version.clone();
        let download_dir = match test_dir {
            Some(dir) => dir.join("update"),
            None => std::env::temp_dir().join("ProjectTrucks-update"),
        };
        let client = reqwest::Client::builder()
            .user_agent(format!("ProjectTrucks/{current}"))
            .connect_timeout(Duration::from_secs(15))
            .read_timeout(Duration::from_secs(60))
            .build()
            .expect("the HTTP client builds");
        let (state, message) = match &feed {
            Feed::Off(why) => (UpdateState::Disabled, Some(why.to_string())),
            Feed::Url { .. } => (UpdateState::Idle, None),
        };
        Self {
            feed,
            download_dir,
            client,
            inner: Mutex::new(Inner {
                status: UpdateStatus {
                    current_version: current.to_string(),
                    state,
                    latest: None,
                    received: 0,
                    total: 0,
                    message,
                },
                asset: None,
            }),
            current,
            busy: tokio::sync::Mutex::new(()),
        }
    }

    pub fn status(&self) -> UpdateStatus {
        self.inner.lock().unwrap().status.clone()
    }

    fn update(&self, app: &AppHandle, change: impl FnOnce(&mut Inner)) -> UpdateStatus {
        let status = {
            let mut inner = self.inner.lock().unwrap();
            change(&mut inner);
            inner.status.clone()
        };
        let _ = app.emit(STATUS_EVENT, &status);
        status
    }

    /// Asks the feed for the latest release. Returns the resulting status; if a check or a
    /// download is already running, returns the current status without starting another.
    pub async fn check(&self, app: &AppHandle) -> UpdateStatus {
        let Feed::Url { url, allow_http } = &self.feed else {
            return self.status();
        };
        let Ok(_busy) = self.busy.try_lock() else {
            return self.status();
        };
        self.update(app, |i| {
            i.status.state = UpdateState::Checking;
            i.status.message = None;
        });
        let result = self.fetch_release(url, *allow_http).await;
        self.update(app, |i| {
            i.status.received = 0;
            i.status.total = 0;
            match result {
                Ok(release) => match is_newer(&self.current, &release.tag_name) {
                    Some(true) => {
                        let asset = pick_installer(&release.assets).cloned();
                        i.status.state = UpdateState::Available;
                        i.status.latest = Some(LatestRelease {
                            version: parse_version(&release.tag_name)
                                .map(|v| v.to_string())
                                .unwrap_or(release.tag_name),
                            page_url: release.html_url,
                            can_install: cfg!(windows) && asset.is_some(),
                        });
                        i.asset = asset;
                    }
                    Some(false) => {
                        i.status.state = UpdateState::UpToDate;
                        i.status.latest = None;
                        i.asset = None;
                    }
                    None => {
                        i.status.state = UpdateState::Error;
                        i.status.message = Some(format!(
                            "The latest release has an unexpected tag ({}).",
                            release.tag_name
                        ));
                        i.status.latest = None;
                        i.asset = None;
                    }
                },
                Err(message) => {
                    i.status.state = UpdateState::Error;
                    i.status.message = Some(message);
                    i.status.latest = None;
                    i.asset = None;
                }
            }
        })
    }

    async fn fetch_release(&self, url: &str, allow_http: bool) -> Result<GhRelease, String> {
        check_scheme(url, allow_http)?;
        let res = self
            .client
            .get(url)
            .header(reqwest::header::ACCEPT, "application/vnd.github+json")
            .timeout(Duration::from_secs(30))
            .send()
            .await
            .map_err(|e| format!("Could not reach the update server: {e}"))?;
        if !res.status().is_success() {
            return Err(format!("The update server answered {}.", res.status()));
        }
        res.json::<GhRelease>()
            .await
            .map_err(|e| format!("Unexpected answer from the update server: {e}"))
    }

    /// Downloads and verifies the installer, starts it and exits the app.
    pub async fn install(&self, app: &AppHandle) -> Result<UpdateStatus, String> {
        let Feed::Url { allow_http, .. } = &self.feed else {
            return Err("Updates are off in this build.".into());
        };
        let Ok(_busy) = self.busy.try_lock() else {
            return Err("An update check or download is already running.".into());
        };
        let (asset, latest) = {
            let inner = self.inner.lock().unwrap();
            (inner.asset.clone(), inner.status.latest.clone())
        };
        let (Some(asset), Some(latest)) = (asset, latest) else {
            return Err("There is no update to install. Check for updates first.".into());
        };
        if !latest.can_install {
            return Err("This build can't install updates itself. Open the release page.".into());
        }

        self.update(app, |i| {
            i.status.state = UpdateState::Downloading;
            i.status.received = 0;
            i.status.total = asset.size;
            i.status.message = None;
        });
        let result = match self.download(app, &asset, *allow_http).await {
            Ok(path) => launch_installer(&path),
            Err(e) => Err(e),
        };
        if let Err(message) = result {
            self.update(app, |i| {
                i.status.state = UpdateState::Error;
                i.status.message = Some(message.clone());
            });
            return Err(message);
        }

        let status = self.update(app, |i| i.status.state = UpdateState::Installing);
        tracing::info!(version = %latest.version, "installer started, exiting");
        // Give the UI a moment to show "Installing", then get out of the installer's way.
        let handle = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_millis(500)).await;
            handle.exit(0);
        });
        Ok(status)
    }

    async fn download(
        &self,
        app: &AppHandle,
        asset: &GhAsset,
        allow_http: bool,
    ) -> Result<PathBuf, String> {
        check_scheme(&asset.browser_download_url, allow_http)?;
        let io = |e: std::io::Error| format!("Could not save the update: {e}");
        let dir = &self.download_dir;
        // Our own folder: whatever a previous update left there can go.
        let _ = tokio::fs::remove_dir_all(dir).await;
        tokio::fs::create_dir_all(dir).await.map_err(io)?;
        let target = dir.join(&asset.name);
        let partial = dir.join(format!("{}.part", asset.name));

        let mut res = self
            .client
            .get(&asset.browser_download_url)
            .header(reqwest::header::ACCEPT, "application/octet-stream")
            .send()
            .await
            .map_err(|e| format!("Could not download the update: {e}"))?;
        if !res.status().is_success() {
            return Err(format!(
                "The download server answered {}. Try again later.",
                res.status()
            ));
        }
        let total = if asset.size > 0 {
            asset.size
        } else {
            res.content_length().unwrap_or(0)
        };
        let mut file = tokio::fs::File::create(&partial).await.map_err(io)?;
        let mut hasher = Sha256::new();
        let mut received = 0u64;
        let mut last_emit = Instant::now();
        while let Some(chunk) = res
            .chunk()
            .await
            .map_err(|e| format!("The download was interrupted: {e}"))?
        {
            received += chunk.len() as u64;
            if asset.size > 0 && received > asset.size {
                return Err("The download is bigger than the release says. Try again.".into());
            }
            hasher.update(&chunk);
            file.write_all(&chunk).await.map_err(io)?;
            if last_emit.elapsed() >= Duration::from_millis(150) {
                last_emit = Instant::now();
                self.update(app, |i| {
                    i.status.received = received;
                    i.status.total = total;
                });
            }
        }
        file.flush().await.map_err(io)?;
        file.sync_all().await.map_err(io)?;
        drop(file);
        self.update(app, |i| {
            i.status.received = received;
            i.status.total = total.max(received);
        });

        let digest: [u8; 32] = hasher.finalize().into();
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        verify_download(received, &hex, asset)?;
        tokio::fs::rename(&partial, &target).await.map_err(io)?;
        tracing::info!(path = %target.display(), bytes = received, "update downloaded and verified");
        Ok(target)
    }

    /// Best-effort removal of an installer a previous update left behind.
    pub fn clean_old_downloads(&self) {
        if self.download_dir.exists() {
            let _ = std::fs::remove_dir_all(&self.download_dir);
        }
    }
}

/// Starts the installer through the shell (like Explorer would), so a per-machine install can
/// still ask for elevation; ProjectTrucks' own installer is per-user and doesn't.
#[cfg(windows)]
fn launch_installer(path: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    fn wide(s: &std::ffi::OsStr) -> Vec<u16> {
        s.encode_wide().chain(std::iter::once(0)).collect()
    }
    let verb = wide("open".as_ref());
    let file = wide(path.as_os_str());
    let params = wide(INSTALLER_ARGS.join(" ").as_ref());
    // SAFETY: every pointer is a NUL-terminated UTF-16 buffer that outlives the call.
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            params.as_ptr(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    // ShellExecuteW returns a value greater than 32 on success.
    if result as isize > 32 {
        tracing::info!(path = %path.display(), args = ?INSTALLER_ARGS, "started the installer");
        Ok(())
    } else {
        Err(format!(
            "Could not start the installer (error {}). It is saved at {}.",
            result as isize,
            path.display()
        ))
    }
}

#[cfg(not(windows))]
fn launch_installer(_path: &Path) -> Result<(), String> {
    Err("This build can't install updates itself. Open the release page.".into())
}

/// Launch check, run from `setup()` when the `check_updates` setting is on.
pub fn spawn_launch_check(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let updater = app.state::<Updater>();
        updater.check(&app).await;
    });
}

#[tauri::command]
pub fn get_update_status(updater: State<'_, Updater>) -> UpdateStatus {
    updater.status()
}

#[tauri::command]
pub async fn check_for_updates(
    app: AppHandle,
    updater: State<'_, Updater>,
) -> Result<UpdateStatus, String> {
    Ok(updater.check(&app).await)
}

#[tauri::command]
pub async fn install_update(
    app: AppHandle,
    updater: State<'_, Updater>,
) -> Result<UpdateStatus, String> {
    updater.install(&app).await
}

/// Opens the latest release's page in the browser (the fallback where the app can't install).
#[tauri::command]
pub fn open_release_page(app: AppHandle, updater: State<'_, Updater>) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let url = updater
        .status()
        .latest
        .map(|l| l.page_url)
        .ok_or("There is no newer release to open.")?;
    check_scheme(&url, false)?;
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> semver::Version {
        semver::Version::parse(s).unwrap()
    }

    fn asset(name: &str) -> GhAsset {
        GhAsset {
            name: name.into(),
            browser_download_url: format!("https://example.com/{name}"),
            size: 0,
            digest: None,
        }
    }

    #[test]
    fn version_compare() {
        let current = v("0.5.0");
        assert_eq!(is_newer(&current, "v0.6.0"), Some(true));
        assert_eq!(is_newer(&current, "0.5.1"), Some(true));
        assert_eq!(is_newer(&current, "v1.0.0"), Some(true));
        assert_eq!(is_newer(&current, "v0.5.0"), Some(false));
        assert_eq!(is_newer(&current, "v0.4.9"), Some(false));
        // Numeric, not string, comparison.
        assert_eq!(is_newer(&v("0.9.0"), "v0.10.0"), Some(true));
        assert_eq!(is_newer(&v("0.10.0"), "v0.9.0"), Some(false));
        // A pre-release sorts before its release.
        assert_eq!(is_newer(&current, "v0.6.0-beta.1"), Some(true));
        assert_eq!(is_newer(&v("0.6.0"), "v0.6.0-beta.1"), Some(false));
        assert_eq!(is_newer(&v("0.6.0-beta.1"), "v0.6.0"), Some(true));
        // Whitespace and an upper-case V are tolerated; junk isn't a version.
        assert_eq!(is_newer(&current, " V0.6.0 "), Some(true));
        assert_eq!(is_newer(&current, "latest"), None);
        assert_eq!(is_newer(&current, "v0.6"), None);
    }

    #[test]
    fn picks_the_nsis_installer() {
        let assets = vec![
            asset("ProjectTrucks_0.6.0_x64_en-US.msi"),
            asset("ProjectTrucks_0.6.0_x64-setup.exe.sig"),
            asset("ProjectTrucks_0.6.0_x64-setup.exe"),
            asset("ProjectTrucks_0.6.0_aarch64.dmg"),
        ];
        assert_eq!(
            pick_installer(&assets).map(|a| a.name.as_str()),
            Some("ProjectTrucks_0.6.0_x64-setup.exe")
        );
        assert_eq!(
            pick_installer(&[asset("ProjectTrucks_0.6.0_X64-SETUP.EXE")]).map(|a| a.name.as_str()),
            Some("ProjectTrucks_0.6.0_X64-SETUP.EXE")
        );
        assert_eq!(
            pick_installer(&[asset("ProjectTrucks_0.6.0_x64_en-US.msi")]),
            None
        );
        assert_eq!(pick_installer(&[]), None);
        // Names that could escape the download folder are never picked.
        assert_eq!(pick_installer(&[asset("..\\evil_x64-setup.exe")]), None);
        assert_eq!(pick_installer(&[asset("sub/evil_x64-setup.exe")]), None);
        assert_eq!(pick_installer(&[asset(".hidden_x64-setup.exe")]), None);
    }

    #[test]
    fn digest_parsing() {
        let hex = "561c6b252d8e985118967402ffc6ef6c0b62aae076949abd07a4dc143ca58ce0";
        assert_eq!(
            expected_sha256(Some(&format!("sha256:{hex}"))),
            Some(hex.to_string())
        );
        assert_eq!(
            expected_sha256(Some(&format!("sha256:{}", hex.to_uppercase()))),
            Some(hex.to_string())
        );
        assert_eq!(expected_sha256(Some("sha512:abcd")), None);
        assert_eq!(expected_sha256(Some("sha256:abcd")), None);
        assert_eq!(expected_sha256(None), None);
    }

    #[test]
    fn download_verification() {
        let body = b"fake installer";
        let hex: String = Sha256::digest(body)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        let mut a = asset("ProjectTrucks_0.6.0_x64-setup.exe");
        a.size = body.len() as u64;
        a.digest = Some(format!("sha256:{hex}"));
        assert_eq!(verify_download(body.len() as u64, &hex, &a), Ok(()));
        assert!(verify_download(3, &hex, &a)
            .unwrap_err()
            .contains("expected 14"));
        let other: String = "0".repeat(64);
        assert!(verify_download(body.len() as u64, &other, &a)
            .unwrap_err()
            .contains("checksum"));
        // No size or digest published: nothing to compare against.
        let bare = asset("ProjectTrucks_0.6.0_x64-setup.exe");
        assert_eq!(verify_download(99, &other, &bare), Ok(()));
    }

    #[test]
    fn feed_selection() {
        let fake = Some("http://127.0.0.1:9999/latest".to_string());
        assert_eq!(
            resolve_feed(true, false, fake.clone()),
            Feed::Url {
                url: "http://127.0.0.1:9999/latest".into(),
                allow_http: true
            }
        );
        assert!(matches!(resolve_feed(true, true, None), Feed::Off(_)));
        assert!(matches!(
            resolve_feed(true, false, Some("  ".into())),
            Feed::Off(_)
        ));
        // Outside a test profile the env var is ignored.
        assert_eq!(
            resolve_feed(false, false, fake.clone()),
            Feed::Url {
                url: GITHUB_FEED.into(),
                allow_http: false
            }
        );
        assert!(matches!(resolve_feed(false, true, fake), Feed::Off(_)));
    }

    #[test]
    fn schemes() {
        assert!(check_scheme("https://github.com/x", false).is_ok());
        assert!(check_scheme("http://127.0.0.1/x", false).is_err());
        assert!(check_scheme("http://127.0.0.1/x", true).is_ok());
        assert!(check_scheme("file:///C:/x.exe", true).is_err());
    }

    #[test]
    fn parses_github_release_json() {
        let json = r#"{
            "tag_name": "v0.5.0",
            "html_url": "https://github.com/PunterDigital/project-trucks/releases/tag/v0.5.0",
            "draft": false,
            "assets": [{
                "name": "ProjectTrucks_0.5.0_x64-setup.exe",
                "size": 4081732,
                "digest": "sha256:561c6b252d8e985118967402ffc6ef6c0b62aae076949abd07a4dc143ca58ce0",
                "browser_download_url": "https://github.com/PunterDigital/project-trucks/releases/download/v0.5.0/ProjectTrucks_0.5.0_x64-setup.exe"
            }, {
                "name": "ProjectTrucks_0.5.0_x64_en-US.msi",
                "size": 5816320,
                "browser_download_url": "https://github.com/PunterDigital/project-trucks/releases/download/v0.5.0/ProjectTrucks_0.5.0_x64_en-US.msi"
            }]
        }"#;
        let release: GhRelease = serde_json::from_str(json).unwrap();
        assert_eq!(release.tag_name, "v0.5.0");
        let installer = pick_installer(&release.assets).unwrap();
        assert_eq!(installer.size, 4081732);
        assert!(expected_sha256(installer.digest.as_deref()).is_some());
        assert_eq!(release.assets[1].digest, None);
    }

    #[test]
    fn status_serialises_for_the_frontend() {
        let status = UpdateStatus {
            current_version: "0.5.0".into(),
            state: UpdateState::UpToDate,
            latest: None,
            received: 0,
            total: 0,
            message: None,
        };
        let json = serde_json::to_value(&status).unwrap();
        assert_eq!(json["state"], "upToDate");
        assert_eq!(json["currentVersion"], "0.5.0");
    }
}

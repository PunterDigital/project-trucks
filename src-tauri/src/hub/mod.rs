//! ProjectTrucks Hub sync (#4634): link this app to a Hub account and keep the Hub's copy of the
//! driver's jobs up to date. The live "on the road" status is in [`live`] (#4635).
//!
//! - Linking is the device-code flow from the Hub's `docs/api.md`: ask for a code, show it, open
//!   the approval page, poll until the driver approves on the website.
//! - The token is kept by [`token::TokenStore`] (Windows Credential Manager), never in SQLite.
//! - The `hub_linked` setting switches the SQLite triggers (migration 0002) that queue job
//!   changes in `hub_outbox`. A background worker sends the queue: deletes, then upserts in
//!   batches of up to [`BATCH`], backing off while the Hub can't be reached.
//! - A 401 means the app was unlinked on the website: the token is dropped and nothing more is
//!   queued.
//!
//! Test profiles (`PROJECTTRUCKS_DATA_DIR` set) talk only to `PROJECTTRUCKS_HUB_URL` (the E2E
//! stub) and never open a browser; without that variable the Hub is off in them.

pub mod api;
pub mod live;
pub mod outbox;
pub mod token;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::Utc;
use serde::Serialize;
use sqlx::SqlitePool;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::Notify;

use crate::db::{self, AppState};
use crate::settings;
use api::{HubAccount, HubClient, HubError, JobPayload, TokenPoll};

/// The production Hub.
pub const DEFAULT_HUB_URL: &str = "https://project-trucks.shaypunter.co.uk";
/// Test-profile-only Hub address (the E2E stub).
pub const HUB_URL_ENV: &str = "PROJECTTRUCKS_HUB_URL";
pub const STATUS_EVENT: &str = "hub:status";
/// Jobs per `POST /jobs` (the Hub's limit).
pub const BATCH: i64 = 100;

const KEY_LINKED: &str = "hub_linked";
const KEY_ACCOUNT: &str = "hub_account";
const KEY_LAST_SYNC: &str = "hub_last_sync";

/// How often the worker looks at the outbox while linked (the triggers fill it; this is a cheap
/// local query).
const POLL: Duration = Duration::from_secs(3);
const BACKOFF_START: Duration = Duration::from_secs(10);
const BACKOFF_MAX: Duration = Duration::from_secs(300);

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Linking {
    /// `ABCD-1234`, shown to the driver.
    pub user_code: String,
    /// Opens the approval page with the code filled in.
    pub verification_uri: String,
    pub expires_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HubStatus {
    /// `None` when the Hub is off (test profile without a stub).
    pub hub_url: Option<String>,
    pub linked: bool,
    pub account: Option<HubAccount>,
    /// A link in progress: show the code.
    pub linking: Option<Linking>,
    pub pending: i64,
    pub failed: i64,
    pub synced: i64,
    /// Jobs from before linking, sent only by "Sync history".
    pub history: i64,
    pub last_sync_at: Option<String>,
    /// The last thing that went wrong (link declined, Hub unreachable, a job refused…).
    pub last_error: Option<String>,
}

pub struct Hub {
    client: Option<HubClient>,
    tokens: token::TokenStore,
    /// The token while linked (cached from the store).
    token: Mutex<Option<String>>,
    status: Mutex<HubStatus>,
    linking: Mutex<Option<Linking>>,
    /// Bumped by every link start, cancel and unlink, so a stale polling task stops.
    generation: AtomicU64,
    last_error: Mutex<Option<String>>,
    pub wake: Notify,
    test_profile: bool,
}

/// Which Hub to talk to: a test profile only uses an explicit stub (http allowed); everything
/// else uses production. The variable is ignored outside a test profile.
pub fn resolve_url(test_profile: bool, env_url: Option<String>) -> Option<String> {
    let env_url = env_url
        .map(|u| u.trim().to_string())
        .filter(|u| !u.is_empty());
    if test_profile {
        env_url
    } else {
        Some(DEFAULT_HUB_URL.to_string())
    }
}

/// "ProjectTrucks on DESKTOP-1234": what the driver sees on the approval page and in Linked apps.
pub fn client_name() -> String {
    match std::env::var("COMPUTERNAME")
        .ok()
        .filter(|n| !n.trim().is_empty())
    {
        Some(pc) => format!("ProjectTrucks on {}", pc.trim()),
        None => "ProjectTrucks".to_string(),
    }
}

impl Hub {
    pub fn new(fallback_dir: std::path::PathBuf) -> Self {
        let test_dir = db::data_dir_override();
        let test_profile = test_dir.is_some();
        let url = resolve_url(test_profile, std::env::var(HUB_URL_ENV).ok());
        Hub {
            client: url.as_deref().map(HubClient::new),
            tokens: token::TokenStore::new(test_dir, fallback_dir),
            token: Mutex::new(None),
            status: Mutex::new(HubStatus {
                hub_url: url,
                linked: false,
                account: None,
                linking: None,
                pending: 0,
                failed: 0,
                synced: 0,
                history: 0,
                last_sync_at: None,
                last_error: None,
            }),
            linking: Mutex::new(None),
            generation: AtomicU64::new(0),
            last_error: Mutex::new(None),
            wake: Notify::new(),
            test_profile,
        }
    }

    /// The token, only while linked.
    pub fn token(&self) -> Option<String> {
        self.token.lock().expect("hub token lock").clone()
    }

    pub fn client(&self) -> Option<&HubClient> {
        self.client.as_ref()
    }

    fn set_error(&self, error: Option<String>) {
        *self.last_error.lock().expect("hub error lock") = error;
    }

    /// Startup: a saved link is only real if the token is still in the store.
    pub async fn load(&self, pool: &SqlitePool) -> Result<(), sqlx::Error> {
        let linked = settings::get(pool, KEY_LINKED).await?.as_deref() == Some("true");
        if !linked || self.client.is_none() {
            return Ok(());
        }
        match self.tokens.get() {
            Some(token) => *self.token.lock().expect("hub token lock") = Some(token),
            None => {
                // The credential was removed outside the app: unlinked.
                settings::put(pool, KEY_LINKED, "false").await?;
                outbox::clear(pool).await?;
            }
        }
        Ok(())
    }
}

/// Recomputes the status and emits [`STATUS_EVENT`] when it changed.
pub async fn refresh(app: &AppHandle, pool: &SqlitePool, hub: &Hub) -> HubStatus {
    let counts = outbox::counts(pool).await.unwrap_or_default();
    let linked = hub.token().is_some();
    let account = if linked {
        settings::get(pool, KEY_ACCOUNT)
            .await
            .ok()
            .flatten()
            .and_then(|json| serde_json::from_str::<HubAccount>(&json).ok())
    } else {
        None
    };
    let mut last_error = hub.last_error.lock().expect("hub error lock").clone();
    if last_error.is_none() && counts.failed > 0 {
        last_error = outbox::first_failure(pool)
            .await
            .ok()
            .flatten()
            .map(|e| format!("The Hub refused {} job(s): {e}", counts.failed));
    }
    let last_sync_at = if linked {
        settings::get(pool, KEY_LAST_SYNC)
            .await
            .ok()
            .flatten()
            .filter(|s| !s.is_empty())
    } else {
        None
    };
    let linking = hub.linking.lock().expect("hub linking lock").clone();
    let next = {
        let mut status = hub.status.lock().expect("hub status lock");
        let next = HubStatus {
            hub_url: status.hub_url.clone(),
            linked,
            account,
            linking,
            pending: counts.pending,
            failed: counts.failed,
            synced: counts.synced,
            history: counts.history,
            last_sync_at,
            last_error,
        };
        if *status == next {
            return next;
        }
        *status = next.clone();
        next
    };
    if let Err(e) = app.emit(STATUS_EVENT, &next) {
        tracing::warn!(error = %e, "could not emit hub:status");
    }
    next
}

/// Drops the link locally: token, setting, queue. `reason` is shown on the Settings page.
async fn forget_link(pool: &SqlitePool, hub: &Hub, reason: Option<String>) -> Result<(), String> {
    hub.generation.fetch_add(1, Ordering::SeqCst);
    *hub.linking.lock().expect("hub linking lock") = None;
    *hub.token.lock().expect("hub token lock") = None;
    hub.tokens.delete();
    let io = |e: sqlx::Error| e.to_string();
    settings::put(pool, KEY_LINKED, "false").await.map_err(io)?;
    settings::put(pool, KEY_ACCOUNT, "").await.map_err(io)?;
    settings::put(pool, KEY_LAST_SYNC, "").await.map_err(io)?;
    outbox::clear(pool).await.map_err(io)?;
    hub.set_error(reason);
    Ok(())
}

/// A 401 from any call: unlinked on the website.
pub async fn handle_unauthorized(app: &AppHandle, pool: &SqlitePool, hub: &Hub) {
    if let Err(e) = forget_link(
        pool,
        hub,
        Some("This app was unlinked on the website. Link it again to keep syncing.".into()),
    )
    .await
    {
        tracing::error!(error = %e, "could not clear the Hub link");
    }
    refresh(app, pool, hub).await;
}

async fn start_link(
    app: &AppHandle,
    pool: &SqlitePool,
    hub: &Arc<Hub>,
) -> Result<HubStatus, String> {
    let client = hub
        .client
        .clone()
        .ok_or("The Hub is off in this test profile.")?;
    if hub.token().is_some() {
        return Err("Already linked. Unlink first to link another account.".into());
    }
    let existing = hub.linking.lock().expect("hub linking lock").clone();
    if existing.is_some() {
        return Ok(refresh(app, pool, hub).await);
    }
    let code = client
        .device_code(&client_name())
        .await
        .map_err(|e| e.to_string())?;
    let generation = hub.generation.fetch_add(1, Ordering::SeqCst) + 1;
    let expires_at = Utc::now() + chrono::Duration::seconds(code.expires_in as i64);
    *hub.linking.lock().expect("hub linking lock") = Some(Linking {
        user_code: code.user_code.clone(),
        verification_uri: code.verification_uri_complete.clone(),
        expires_at: expires_at.to_rfc3339(),
    });
    hub.set_error(None);
    if !hub.test_profile {
        open_url(app, &code.verification_uri_complete);
    }
    tauri::async_runtime::spawn(poll_link(
        app.clone(),
        pool.clone(),
        hub.clone(),
        client,
        code,
        generation,
    ));
    Ok(refresh(app, pool, hub).await)
}

async fn poll_link(
    app: AppHandle,
    pool: SqlitePool,
    hub: Arc<Hub>,
    client: HubClient,
    code: api::DeviceCode,
    generation: u64,
) {
    let mut interval = Duration::from_secs(code.interval.max(1));
    let deadline = tokio::time::Instant::now() + Duration::from_secs(code.expires_in);
    let end = |error: Option<String>| {
        let hub = hub.clone();
        let app = app.clone();
        let pool = pool.clone();
        async move {
            if hub.generation.load(Ordering::SeqCst) == generation {
                *hub.linking.lock().expect("hub linking lock") = None;
                hub.set_error(error);
                refresh(&app, &pool, &hub).await;
            }
        }
    };
    loop {
        tokio::time::sleep(interval).await;
        if hub.generation.load(Ordering::SeqCst) != generation {
            return;
        }
        if tokio::time::Instant::now() >= deadline {
            end(Some(
                "The code expired. Click Link account to try again.".into(),
            ))
            .await;
            return;
        }
        match client.device_token(&code.device_code).await {
            Ok(TokenPoll::Pending) => {}
            Ok(TokenPoll::SlowDown) => interval += Duration::from_secs(5),
            Ok(TokenPoll::Denied) => {
                end(Some("Linking was declined on the website.".into())).await;
                return;
            }
            Ok(TokenPoll::Expired) => {
                end(Some(
                    "The code expired. Click Link account to try again.".into(),
                ))
                .await;
                return;
            }
            Ok(TokenPoll::Linked { token, account }) => {
                if hub.generation.load(Ordering::SeqCst) != generation {
                    // Cancelled while the answer was on its way: give the token back.
                    let _ = client.revoke_token(&token).await;
                    return;
                }
                let result = finish_link(&pool, &hub, &token, &account).await;
                end(result.err()).await;
                hub.wake.notify_one();
                return;
            }
            // Offline for a moment: keep trying until the code expires.
            Err(HubError::Network(e)) => tracing::debug!(error = %e, "hub link poll failed"),
            Err(e) => {
                end(Some(e.to_string())).await;
                return;
            }
        }
    }
}

async fn finish_link(
    pool: &SqlitePool,
    hub: &Hub,
    token: &str,
    account: &HubAccount,
) -> Result<(), String> {
    hub.tokens.set(token)?;
    let io = |e: sqlx::Error| e.to_string();
    // A fresh account starts from an empty queue: only jobs from now on sync by themselves.
    outbox::clear(pool).await.map_err(io)?;
    settings::put(
        pool,
        KEY_ACCOUNT,
        &serde_json::to_string(account).unwrap_or_default(),
    )
    .await
    .map_err(io)?;
    settings::put(pool, KEY_LINKED, "true").await.map_err(io)?;
    *hub.token.lock().expect("hub token lock") = Some(token.to_string());
    tracing::info!(username = ?account.username, "linked to the Hub");
    Ok(())
}

fn open_url(app: &AppHandle, url: &str) {
    use tauri_plugin_opener::OpenerExt;
    if let Err(e) = app.opener().open_url(url, None::<&str>) {
        tracing::warn!(error = %e, "could not open the browser");
    }
}

/// Sends what's queued until the queue is empty or something fails.
async fn drain(
    pool: &SqlitePool,
    hub: &Hub,
    client: &HubClient,
    token: &str,
) -> Result<bool, HubError> {
    let db = |e: &dyn std::fmt::Display| HubError::Network(format!("local database: {e}"));
    let mut sent_any = false;

    for queued in outbox::next_deletes(pool, BATCH)
        .await
        .map_err(|e| db(&e))?
    {
        client.delete_job(token, &queued.job_id).await?;
        outbox::mark_deleted(pool, &queued)
            .await
            .map_err(|e| db(&e))?;
        sent_any = true;
    }

    loop {
        let batch = outbox::next_upserts(pool, BATCH)
            .await
            .map_err(|e| db(&e))?;
        if batch.is_empty() {
            break;
        }
        let payload: Vec<JobPayload> = batch.iter().map(|(_, job)| JobPayload::from(job)).collect();
        match client.post_jobs(token, &payload).await {
            Ok(synced) => {
                for (queued, _) in &batch {
                    let url = synced
                        .iter()
                        .find(|s| s.id == queued.job_id)
                        .map(|s| s.url.as_str());
                    outbox::mark_synced(pool, queued, url)
                        .await
                        .map_err(|e| db(&e))?;
                }
                sent_any = true;
            }
            // One bad job must not block the rest: retry them one at a time and count each
            // refusal against that job only.
            Err(HubError::Rejected { status: 422, .. }) => {
                for (i, (queued, _)) in batch.iter().enumerate() {
                    match client
                        .post_jobs(token, std::slice::from_ref(&payload[i]))
                        .await
                    {
                        Ok(synced) => {
                            let url = synced.first().map(|s| s.url.as_str());
                            outbox::mark_synced(pool, queued, url)
                                .await
                                .map_err(|e| db(&e))?;
                            sent_any = true;
                        }
                        Err(HubError::Rejected {
                            status: 422,
                            message,
                        }) => {
                            outbox::mark_failed(pool, queued, &message)
                                .await
                                .map_err(|e| db(&e))?;
                        }
                        Err(e) => return Err(e),
                    }
                }
            }
            Err(e) => return Err(e),
        }
        if hub.token().is_none() {
            break;
        }
    }
    Ok(sent_any)
}

/// The background sync loop. Runs for the life of the app.
pub async fn run_sync(app: AppHandle, pool: SqlitePool, hub: Arc<Hub>) {
    let mut delay = POLL;
    let mut backoff = BACKOFF_START;
    refresh(&app, &pool, &hub).await;
    loop {
        tokio::select! {
            _ = hub.wake.notified() => {}
            _ = tokio::time::sleep(delay) => {}
        }
        delay = POLL;
        let (Some(client), Some(token)) = (hub.client.clone(), hub.token()) else {
            refresh(&app, &pool, &hub).await;
            continue;
        };
        match drain(&pool, &hub, &client, &token).await {
            Ok(sent) => {
                backoff = BACKOFF_START;
                if sent {
                    let _ = settings::put(&pool, KEY_LAST_SYNC, &Utc::now().to_rfc3339()).await;
                }
                // A transient error from an earlier round is over.
                let was_transient = hub
                    .last_error
                    .lock()
                    .expect("hub error lock")
                    .as_deref()
                    .is_some_and(|e| e.starts_with("Couldn't sync"));
                if was_transient {
                    hub.set_error(None);
                }
            }
            Err(HubError::Unauthorized) => {
                handle_unauthorized(&app, &pool, &hub).await;
                continue;
            }
            Err(e) => {
                tracing::warn!(error = %e, retry_in = ?backoff, "hub sync failed");
                hub.set_error(Some(format!("Couldn't sync: {e}. Retrying automatically.")));
                delay = backoff;
                backoff = (backoff * 2).min(BACKOFF_MAX);
            }
        }
        refresh(&app, &pool, &hub).await;
    }
}

type HubState<'a> = State<'a, Arc<Hub>>;

#[tauri::command]
pub async fn get_hub_status(
    app: AppHandle,
    state: State<'_, AppState>,
    hub: HubState<'_>,
) -> Result<HubStatus, String> {
    Ok(refresh(&app, &state.pool, &hub).await)
}

#[tauri::command]
pub async fn hub_link_start(
    app: AppHandle,
    state: State<'_, AppState>,
    hub: HubState<'_>,
) -> Result<HubStatus, String> {
    start_link(&app, &state.pool, &hub).await
}

#[tauri::command]
pub async fn hub_link_cancel(
    app: AppHandle,
    state: State<'_, AppState>,
    hub: HubState<'_>,
) -> Result<HubStatus, String> {
    hub.generation.fetch_add(1, Ordering::SeqCst);
    *hub.linking.lock().expect("hub linking lock") = None;
    Ok(refresh(&app, &state.pool, &hub).await)
}

/// "Unlink": revoke the token on the Hub (best effort, the app may be offline), clear the live
/// status there, and forget everything locally.
#[tauri::command]
pub async fn hub_unlink(
    app: AppHandle,
    state: State<'_, AppState>,
    hub: HubState<'_>,
) -> Result<HubStatus, String> {
    if let (Some(client), Some(token)) = (hub.client.clone(), hub.token()) {
        let revoke = async {
            let _ = client.delete_status(&token).await;
            let _ = client.revoke_token(&token).await;
        };
        let _ = tokio::time::timeout(Duration::from_secs(5), revoke).await;
    }
    forget_link(&state.pool, &hub, None).await?;
    Ok(refresh(&app, &state.pool, &hub).await)
}

/// "Sync history": queue every older job the Hub doesn't have.
#[tauri::command]
pub async fn hub_sync_history(
    app: AppHandle,
    state: State<'_, AppState>,
    hub: HubState<'_>,
) -> Result<HubStatus, String> {
    if hub.token().is_none() {
        return Err("Link an account first.".into());
    }
    outbox::queue_history(&state.pool)
        .await
        .map_err(|e| e.to_string())?;
    hub.wake.notify_one();
    Ok(refresh(&app, &state.pool, &hub).await)
}

/// "Sync now" / "Retry": try failed jobs again and wake the worker.
#[tauri::command]
pub async fn hub_sync_now(
    app: AppHandle,
    state: State<'_, AppState>,
    hub: HubState<'_>,
) -> Result<HubStatus, String> {
    outbox::retry_failed(&state.pool)
        .await
        .map_err(|e| e.to_string())?;
    hub.set_error(None);
    hub.wake.notify_one();
    Ok(refresh(&app, &state.pool, &hub).await)
}

/// Opens the approval page (while linking) or the driver's profile (while linked) in the browser.
#[tauri::command]
pub async fn hub_open(
    app: AppHandle,
    state: State<'_, AppState>,
    hub: HubState<'_>,
    target: String,
) -> Result<(), String> {
    let status = refresh(&app, &state.pool, &hub).await;
    let url = match target.as_str() {
        "approve" => status.linking.map(|l| l.verification_uri),
        "profile" => status.account.map(|a| a.profile_url),
        _ => None,
    }
    .ok_or("Nothing to open.")?;
    let allowed = url.starts_with("https://") || (hub.test_profile && url.starts_with("http://"));
    if !allowed {
        return Err("Refusing to open a non-https address.".into());
    }
    if hub.test_profile {
        // Never pop a browser on the machine running the tests.
        return Ok(());
    }
    open_url(&app, &url);
    Ok(())
}

/// Called from `lib.rs` setup.
pub fn setup(app: &tauri::App, pool: &SqlitePool) -> Result<Arc<Hub>, Box<dyn std::error::Error>> {
    let fallback = app.path().app_data_dir()?;
    let hub = Arc::new(Hub::new(fallback));
    tauri::async_runtime::block_on(hub.load(pool))?;
    Ok(hub)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hub_url_resolution() {
        assert_eq!(resolve_url(false, None).as_deref(), Some(DEFAULT_HUB_URL));
        assert_eq!(
            resolve_url(false, Some("http://evil.example".into())).as_deref(),
            Some(DEFAULT_HUB_URL),
            "the variable is ignored outside a test profile"
        );
        assert_eq!(
            resolve_url(true, None),
            None,
            "test profiles never reach the real Hub"
        );
        assert_eq!(
            resolve_url(true, Some(" http://127.0.0.1:9999 ".into())).as_deref(),
            Some("http://127.0.0.1:9999")
        );
    }

    #[test]
    fn client_name_mentions_the_pc() {
        assert!(client_name().starts_with("ProjectTrucks"));
    }
}

//! HTTP calls to the Hub (`/api/v1`, contract in the Hub repo's `docs/api.md`).

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::db::models::Job;

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum HubError {
    /// 401: the token was revoked (unlinked on the website) or never valid.
    #[error("this app is no longer linked to the Hub")]
    Unauthorized,
    /// Any other non-2xx answer.
    #[error("the Hub refused the request ({status}): {message}")]
    Rejected { status: u16, message: String },
    /// Offline, DNS, TLS, timeout…: worth retrying later.
    #[error("couldn't reach the Hub: {0}")]
    Network(String),
}

impl HubError {
    pub fn is_transient(&self) -> bool {
        match self {
            HubError::Network(_) => true,
            HubError::Rejected { status, .. } => *status == 429 || *status >= 500,
            HubError::Unauthorized => false,
        }
    }
}

/// The ids the app makes (`commands::jobs`): `job_` + 32 lowercase hex digits. Ids that go into
/// a Hub URL path are checked against it, so a crafted one can't reach another endpoint (#5614).
pub fn is_job_id(id: &str) -> bool {
    id.strip_prefix("job_").is_some_and(|hex| {
        hex.len() == 32
            && hex
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeviceCode {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub verification_uri_complete: String,
    pub expires_in: u64,
    pub interval: u64,
}

/// Who the token belongs to. Read from the Hub (snake_case), sent to the frontend (camelCase).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HubAccount {
    pub name: String,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(alias = "profile_url")]
    pub profile_url: String,
    /// The driver's company, from `GET /me` (#4654). The link response doesn't carry it.
    #[serde(default)]
    pub company: Option<HubCompany>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HubCompany {
    pub name: String,
    #[serde(default)]
    pub tag: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    user: HubAccount,
}

#[derive(Debug, PartialEq)]
pub enum TokenPoll {
    Pending,
    SlowDown,
    Denied,
    Expired,
    Linked { token: String, account: HubAccount },
}

/// A job as the Hub takes it: the app's own column names, minus `notes` and `session_id`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct JobPayload {
    pub id: String,
    pub truck_make: Option<String>,
    pub truck_model: Option<String>,
    pub source_city: String,
    pub source_company: String,
    pub dest_city: String,
    pub dest_company: String,
    pub cargo_id: String,
    pub cargo_name: String,
    pub cargo_mass_kg: f64,
    pub declared_income: i64,
    pub distance_km: f64,
    pub duration_sec: i64,
    /// In-game minutes (#4771): the Hub checks the average speed against these. `null` for jobs
    /// recorded before 0.11.
    pub game_duration_min: Option<i64>,
    pub avg_speed_kmh: f64,
    pub fuel_used_l: Option<f64>,
    pub damage_delta: Option<f64>,
    pub started_at: String,
    pub completed_at: String,
    pub status: crate::db::models::JobStatus,
    pub source: crate::db::models::JobSource,
    pub flagged: bool,
    pub flag_reason: Option<String>,
    /// What the game reported (#5227, scs-telemetry only); `null` otherwise. Hubs before #5227
    /// ignore them.
    pub game_revenue: Option<i64>,
    pub earned_xp: Option<i64>,
    pub cargo_damage: Option<f64>,
    /// The job's tolls, fines and ferry/train fares in total: set with [`JobPayload::with_costs`].
    pub costs: Option<i64>,
    pub cancel_penalty: Option<i64>,
}

impl JobPayload {
    pub fn with_costs(mut self, costs: Option<i64>) -> Self {
        self.costs = costs.map(|c| c.max(0));
        self
    }
}

/// The Hub requires these text fields and caps their length; manual entries can leave them blank.
fn text(value: &str, max: usize) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        "Unknown".to_string()
    } else {
        trimmed.chars().take(max).collect()
    }
}

fn opt_text(value: &Option<String>, max: usize) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(|v| v.chars().take(max).collect())
}

impl From<&Job> for JobPayload {
    fn from(job: &Job) -> Self {
        JobPayload {
            id: job.id.clone(),
            truck_make: opt_text(&job.truck_make, 60),
            truck_model: opt_text(&job.truck_model, 60),
            source_city: text(&job.source_city, 80),
            source_company: text(&job.source_company, 80),
            dest_city: text(&job.dest_city, 80),
            dest_company: text(&job.dest_company, 80),
            cargo_id: text(&job.cargo_id, 80),
            cargo_name: text(&job.cargo_name, 120),
            cargo_mass_kg: job.cargo_mass_kg.max(0.0),
            declared_income: job.declared_income,
            distance_km: job.distance_km.max(0.0),
            duration_sec: job.duration_sec.max(0),
            game_duration_min: job.game_duration_min.map(|m| m.max(0)),
            avg_speed_kmh: job.avg_speed_kmh.max(0.0),
            fuel_used_l: job.fuel_used_l.map(|v| v.max(0.0)),
            damage_delta: job.damage_delta.map(|v| v.clamp(0.0, 1.0)),
            started_at: job.started_at.to_rfc3339(),
            completed_at: job.completed_at.to_rfc3339(),
            status: job.status,
            source: job.source,
            flagged: job.flagged,
            flag_reason: opt_text(&job.flag_reason, 255),
            game_revenue: job.game.game_revenue,
            earned_xp: job.game.earned_xp.map(|xp| xp.max(0)),
            cargo_damage: job.game.cargo_damage.map(|d| d.clamp(0.0, 1.0)),
            costs: None,
            cancel_penalty: job.game.cancel_penalty.map(|p| p.max(0)),
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct SyncedJob {
    pub id: String,
    pub url: String,
    /// Hidden on the Hub by the driver (Hub #30; `None` from older Hubs).
    #[serde(default)]
    pub hidden: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct HiddenResponse {
    hidden: bool,
}

#[derive(Debug, Deserialize)]
struct SyncResponse {
    jobs: Vec<SyncedJob>,
}

/// `PUT /status`: the truck and the job in progress (see `live.rs`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LivePayload {
    pub truck: Option<LiveTruck>,
    pub job: Option<LiveJob>,
    /// Where the truck is on the game map (#4686), on a job or free-roaming. The Hub drops it
    /// when the driver turned "Show me on the live map" off.
    pub position: Option<LivePosition>,
}

/// SCS world coordinates (metres; x grows east, z grows south) and the heading in degrees,
/// SCS convention: 0 = north, counter-clockwise.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LivePosition {
    pub x: f64,
    pub z: f64,
    pub heading_deg: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LiveTruck {
    pub make: Option<String>,
    pub model: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LiveJob {
    pub cargo_name: Option<String>,
    pub cargo_mass_kg: f64,
    pub source_city: Option<String>,
    pub source_company: Option<String>,
    pub destination_city: Option<String>,
    pub destination_company: Option<String>,
    pub income: i64,
    pub driven_km: f64,
    pub distance_left_km: Option<f64>,
    pub progress_pct: Option<f64>,
    pub parked: bool,
}

#[derive(Clone)]
pub struct HubClient {
    http: reqwest::Client,
    base: String,
}

impl HubClient {
    pub fn new(base: &str) -> Self {
        let http = reqwest::Client::builder()
            .user_agent(concat!("ProjectTrucks/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(20))
            .build()
            .expect("reqwest client");
        HubClient {
            http,
            base: base.trim_end_matches('/').to_string(),
        }
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    fn url(&self, path: &str) -> String {
        format!("{}/api/v1/{path}", self.base)
    }

    async fn send(&self, request: reqwest::RequestBuilder) -> Result<reqwest::Response, HubError> {
        let response = request
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| HubError::Network(e.to_string()))?;
        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }
        if status.as_u16() == 401 {
            return Err(HubError::Unauthorized);
        }
        let body = response.text().await.unwrap_or_default();
        Err(HubError::Rejected {
            status: status.as_u16(),
            message: error_message(&body),
        })
    }

    pub async fn device_code(&self, client_name: &str) -> Result<DeviceCode, HubError> {
        let response = self
            .send(
                self.http
                    .post(self.url("device/code"))
                    .json(&serde_json::json!({ "client_name": client_name })),
            )
            .await?;
        response
            .json()
            .await
            .map_err(|e| HubError::Network(e.to_string()))
    }

    pub async fn device_token(&self, device_code: &str) -> Result<TokenPoll, HubError> {
        let response = self
            .http
            .post(self.url("device/token"))
            .header("Accept", "application/json")
            .json(&serde_json::json!({ "device_code": device_code }))
            .send()
            .await
            .map_err(|e| HubError::Network(e.to_string()))?;
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        if status == 200 {
            let parsed: TokenResponse =
                serde_json::from_str(&body).map_err(|e| HubError::Network(e.to_string()))?;
            return Ok(TokenPoll::Linked {
                token: parsed.access_token,
                account: parsed.user,
            });
        }
        let code = serde_json::from_str::<serde_json::Value>(&body)
            .ok()
            .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(str::to_string));
        match (status, code.as_deref()) {
            (400, Some("authorization_pending")) => Ok(TokenPoll::Pending),
            (400, Some("slow_down")) => Ok(TokenPoll::SlowDown),
            (400, Some("access_denied")) => Ok(TokenPoll::Denied),
            (400, Some("expired_token")) => Ok(TokenPoll::Expired),
            _ => Err(HubError::Rejected {
                status,
                message: error_message(&body),
            }),
        }
    }

    pub async fn post_jobs(
        &self,
        token: &str,
        jobs: &[JobPayload],
    ) -> Result<Vec<SyncedJob>, HubError> {
        let response = self
            .send(
                self.http
                    .post(self.url("jobs"))
                    .bearer_auth(token)
                    .json(&serde_json::json!({ "jobs": jobs })),
            )
            .await?;
        let parsed: SyncResponse = response
            .json()
            .await
            .map_err(|e| HubError::Network(e.to_string()))?;
        Ok(parsed.jobs)
    }

    /// 404 counts as done: the Hub never had it. So does an id the app never makes (nothing is
    /// sent: it can't name a Hub job, and it never goes into a URL path).
    pub async fn delete_job(&self, token: &str, id: &str) -> Result<(), HubError> {
        if !is_job_id(id) {
            tracing::warn!(id, "not deleting an odd job id on the Hub");
            return Ok(());
        }
        match self
            .send(
                self.http
                    .delete(self.url(&format!("jobs/{id}")))
                    .bearer_auth(token),
            )
            .await
        {
            Ok(_) | Err(HubError::Rejected { status: 404, .. }) => Ok(()),
            Err(e) => Err(e),
        }
    }

    /// "Hide on Hub": returns what the Hub stored. 404 = the Hub doesn't have the job.
    pub async fn set_hidden(&self, token: &str, id: &str, hidden: bool) -> Result<bool, HubError> {
        if !is_job_id(id) {
            return Err(HubError::Rejected {
                status: 400,
                message: "that isn't a job id".into(),
            });
        }
        let response = self
            .send(
                self.http
                    .patch(self.url(&format!("jobs/{id}")))
                    .bearer_auth(token)
                    .json(&serde_json::json!({ "hidden": hidden })),
            )
            .await?;
        let parsed: HiddenResponse = response
            .json()
            .await
            .map_err(|e| HubError::Network(e.to_string()))?;
        Ok(parsed.hidden)
    }

    /// Who the token belongs to, with the company.
    pub async fn me(&self, token: &str) -> Result<HubAccount, HubError> {
        let response = self
            .send(self.http.get(self.url("me")).bearer_auth(token))
            .await?;
        response
            .json()
            .await
            .map_err(|e| HubError::Network(e.to_string()))
    }

    pub async fn put_status(&self, token: &str, payload: &LivePayload) -> Result<(), HubError> {
        self.send(
            self.http
                .put(self.url("status"))
                .bearer_auth(token)
                .json(payload),
        )
        .await
        .map(|_| ())
    }

    pub async fn delete_status(&self, token: &str) -> Result<(), HubError> {
        self.send(self.http.delete(self.url("status")).bearer_auth(token))
            .await
            .map(|_| ())
    }

    pub async fn revoke_token(&self, token: &str) -> Result<(), HubError> {
        self.send(self.http.delete(self.url("token")).bearer_auth(token))
            .await
            .map(|_| ())
    }
}

/// A short, readable reason from a JSON error body (Laravel's `message`), else the raw text.
fn error_message(body: &str) -> String {
    let message = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| {
            v.get("message")
                .or_else(|| v.get("error_description"))
                .and_then(|m| m.as_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| body.trim().to_string());
    message.chars().take(300).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::models::{GameResult, JobSource, JobStatus};
    use chrono::{TimeZone, Utc};

    #[test]
    fn only_real_job_ids_go_into_urls() {
        let made = format!("job_{}", uuid::Uuid::new_v4().simple());
        assert!(is_job_id(&made));
        assert!(is_job_id("job_0123456789abcdef0123456789abcdef"));
        for bad in [
            "job_0123456789ABCDEF0123456789ABCDEF",
            "job_0123456789abcdef0123456789abcde",
            "job_0123456789abcdef0123456789abcdef0",
            "job_../../me",
            "job_0123456789abcdef0123456789abcd?x",
            "../tokens/1",
            "job_1",
            "",
        ] {
            assert!(!is_job_id(bad), "{bad}");
        }
    }

    fn job() -> Job {
        Job {
            id: "7f1c6a8e".into(),
            session_id: Some("s1".into()),
            truck_make: Some("Scania".into()),
            truck_model: Some(" ".into()),
            source_city: "Calais".into(),
            source_company: "".into(),
            dest_city: "Paris".into(),
            dest_company: "Euroacres".into(),
            cargo_id: "apples".into(),
            cargo_name: "Apples".into(),
            cargo_mass_kg: 18_000.0,
            declared_income: 12_450,
            distance_km: 291.4,
            duration_sec: 14_400,
            game_duration_min: Some(240),
            avg_speed_kmh: 72.8,
            fuel_used_l: Some(95.2),
            damage_delta: Some(1.5),
            started_at: Utc.with_ymd_and_hms(2026, 11, 14, 10, 0, 0).unwrap(),
            completed_at: Utc.with_ymd_and_hms(2026, 11, 14, 14, 0, 0).unwrap(),
            status: JobStatus::Delivered,
            source: JobSource::Telemetry,
            flagged: false,
            flag_reason: None,
            notes: Some("private".into()),
            game: Default::default(),
        }
    }

    #[test]
    fn payload_has_the_hub_fields_and_never_the_notes() {
        let value = serde_json::to_value(JobPayload::from(&job())).unwrap();
        assert!(value.get("notes").is_none());
        assert!(value.get("session_id").is_none());
        assert_eq!(value["status"], "DELIVERED");
        assert_eq!(value["source"], "TELEMETRY");
        assert_eq!(value["completed_at"], "2026-11-14T14:00:00+00:00");
        assert_eq!(value["declared_income"], 12_450);
        // Blank text the Hub would refuse, and out-of-range numbers, are tidied up.
        assert_eq!(value["source_company"], "Unknown");
        assert_eq!(value["truck_model"], serde_json::Value::Null);
        assert_eq!(value["damage_delta"], 1.0);
        // No game figures (old plugin or a job from before 0.12): sent as null.
        for key in [
            "game_revenue",
            "earned_xp",
            "cargo_damage",
            "costs",
            "cancel_penalty",
        ] {
            assert_eq!(value[key], serde_json::Value::Null, "{key}");
        }
    }

    #[test]
    fn payload_carries_the_games_figures_and_costs() {
        let mut j = job();
        j.game = GameResult {
            game_revenue: Some(4_478),
            earned_xp: Some(320),
            cargo_damage: Some(1.3),
            cancel_penalty: Some(-5),
            ..Default::default()
        };
        let value = serde_json::to_value(JobPayload::from(&j).with_costs(Some(319))).unwrap();
        assert_eq!(value["game_revenue"], 4_478);
        assert_eq!(value["earned_xp"], 320);
        assert_eq!(value["cargo_damage"], 1.0);
        assert_eq!(value["costs"], 319);
        assert_eq!(value["cancel_penalty"], 0);
        assert_eq!(value["declared_income"], 12_450);
    }

    #[test]
    fn error_messages_prefer_the_json_message() {
        assert_eq!(error_message(r#"{"message":"Too many."}"#), "Too many.");
        assert_eq!(error_message("plain"), "plain");
    }

    #[test]
    fn transient_errors() {
        assert!(HubError::Network("x".into()).is_transient());
        assert!(HubError::Rejected {
            status: 429,
            message: String::new()
        }
        .is_transient());
        assert!(HubError::Rejected {
            status: 503,
            message: String::new()
        }
        .is_transient());
        assert!(!HubError::Rejected {
            status: 422,
            message: String::new()
        }
        .is_transient());
        assert!(!HubError::Unauthorized.is_transient());
    }

    #[test]
    fn me_and_sync_answers_parse() {
        // `GET /me` as the Hub sends it (extra fields ignored).
        let me: HubAccount = serde_json::from_str(
            r#"{"id":3,"name":"Erin","username":"erin","avatar":null,
                "company":{"name":"Chart Haulage","slug":"chart","tag":"CHT"},
                "profile_url":"https://hub/drivers/erin","profile_private":false,"jobs_synced":4}"#,
        )
        .unwrap();
        assert_eq!(
            me.company,
            Some(HubCompany {
                name: "Chart Haulage".into(),
                tag: Some("CHT".into())
            })
        );
        let solo: HubAccount =
            serde_json::from_str(r#"{"name":"Erin","profile_url":"x","company":null}"#).unwrap();
        assert_eq!(solo.company, None);
        // Saved accounts from before #4654 (camelCase, no company) still load.
        let saved: HubAccount =
            serde_json::from_str(r#"{"name":"Erin","username":"erin","profileUrl":"x"}"#).unwrap();
        assert_eq!(saved.company, None);

        let old: SyncedJob = serde_json::from_str(r#"{"id":"a","url":"u"}"#).unwrap();
        assert_eq!(old.hidden, None);
        let new: SyncedJob =
            serde_json::from_str(r#"{"id":"a","url":"u","hidden":true,"hub_flag_reason":null}"#)
                .unwrap();
        assert_eq!(new.hidden, Some(true));
    }
}

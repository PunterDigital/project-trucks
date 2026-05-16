//! Domain types for the local database. Mirrors PLAN.md §5.
//!
//! Rust field names stay snake_case to line up with SQL column names so that
//! `sqlx::FromRow` works without per-field overrides. Serde renames them to
//! camelCase on the wire so the TypeScript side reads idiomatically.

// These types are the consumer-facing surface for commands::sessions /
// commands::jobs (added in Task 1.2). Suppress dead-code lints until those
// land and start exercising the types.
#![allow(dead_code)]

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Kind of driving session: free-roam (no job attached) or job (delivery).
#[derive(sqlx::Type, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[sqlx(type_name = "TEXT", rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SessionKind {
    FreeRoam,
    Job,
}

/// How a session ended. `Cancelled` is only meaningful for `JOB` sessions —
/// free-roam sessions are always `Completed` when they close.
#[derive(sqlx::Type, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[sqlx(type_name = "TEXT", rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SessionStatus {
    Completed,
    Cancelled,
}

#[derive(sqlx::Type, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[sqlx(type_name = "TEXT", rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum JobStatus {
    Delivered,
    Late,
    Cancelled,
}

#[derive(sqlx::Type, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[sqlx(type_name = "TEXT", rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum JobSource {
    Telemetry,
    Manual,
}

/// One stretch of driving, either free-roam or a single delivery. The source
/// of truth for km driven — every truck rollup is computed from these rows.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct DrivingSession {
    pub id: String,
    pub kind: SessionKind,
    pub truck_make: Option<String>,
    pub truck_model: Option<String>,
    pub start_odometer: f64,
    pub end_odometer: f64,
    pub distance_km: f64,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    pub status: SessionStatus,
}

/// A delivery. Telemetry jobs have a `session_id` pointing at the driving
/// session that contains them; manually-entered jobs leave it `None`.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: String,
    pub session_id: Option<String>,
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
    pub avg_speed_kmh: f64,
    pub fuel_used_l: Option<f64>,
    pub damage_delta: Option<f64>,
    pub started_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
    pub status: JobStatus,
    pub source: JobSource,
    pub flagged: bool,
    pub flag_reason: Option<String>,
    pub notes: Option<String>,
}

/// Per-truck rollup. Maintained by the write paths in commands::sessions /
/// commands::jobs, never by hand.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Truck {
    pub id: String,
    pub make: String,
    pub model: String,
    pub first_seen_at: DateTime<Utc>,
    pub total_distance_km: f64,
    pub total_delivery_distance: f64,
    pub total_free_roam_distance: f64,
    pub total_jobs: i64,
}

/// Input shape for inserting a new driving session. The `id` is generated
/// server-side; everything else comes from the caller (telemetry pipeline or
/// future test fixtures).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewSession {
    pub kind: SessionKind,
    pub truck_make: Option<String>,
    pub truck_model: Option<String>,
    pub start_odometer: f64,
    pub end_odometer: f64,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    pub status: SessionStatus,
}

/// Filter for `list_sessions`. All fields optional.
///
/// `limit` defaults to 25 and is capped at 200 server-side.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionFilter {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub kind: Option<SessionKind>,
    /// `ended_at >= from` (inclusive).
    pub from: Option<DateTime<Utc>>,
    /// `ended_at < to` (exclusive).
    pub to: Option<DateTime<Utc>>,
}

/// Filter for `list_jobs`. All fields optional.
///
/// `search` matches case-insensitively against source/dest city and company,
/// and against cargo name. `from`/`to` bound `completed_at`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobFilter {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub status: Option<JobStatus>,
    pub search: Option<String>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionsPage {
    pub items: Vec<DrivingSession>,
    pub total: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobsPage {
    pub items: Vec<Job>,
    pub total: i64,
}

/// Mutable subset of a Job. Each `Some(x)` overwrites; `None` leaves the
/// column alone. To clear `notes` or `flag_reason`, send `Some("")`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobPatch {
    pub notes: Option<String>,
    pub flagged: Option<bool>,
    pub flag_reason: Option<String>,
}

/// Input shape for inserting a new job. `id` and `avg_speed_kmh` are computed
/// server-side; `session_id` is set by the telemetry pipeline and left `None`
/// by the manual-entry form.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewJob {
    pub session_id: Option<String>,
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
    pub fuel_used_l: Option<f64>,
    pub damage_delta: Option<f64>,
    pub started_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
    pub status: JobStatus,
    pub source: JobSource,
    pub notes: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn sample_session() -> DrivingSession {
        DrivingSession {
            id: "ses_1".into(),
            kind: SessionKind::Job,
            truck_make: Some("Scania".into()),
            truck_model: Some("R 730".into()),
            start_odometer: 100.5,
            end_odometer: 250.75,
            distance_km: 150.25,
            started_at: Utc.with_ymd_and_hms(2026, 5, 1, 10, 0, 0).unwrap(),
            ended_at: Utc.with_ymd_and_hms(2026, 5, 1, 12, 30, 0).unwrap(),
            status: SessionStatus::Completed,
        }
    }

    fn sample_job() -> Job {
        Job {
            id: "job_1".into(),
            session_id: Some("ses_1".into()),
            truck_make: Some("Scania".into()),
            truck_model: Some("R 730".into()),
            source_city: "Hamburg".into(),
            source_company: "DHL".into(),
            dest_city: "Berlin".into(),
            dest_company: "Tradeaux".into(),
            cargo_id: "container".into(),
            cargo_name: "Containers".into(),
            cargo_mass_kg: 20000.0,
            declared_income: 4250,
            distance_km: 290.7,
            duration_sec: 14_400,
            avg_speed_kmh: 72.675,
            fuel_used_l: Some(95.5),
            damage_delta: Some(0.02),
            started_at: Utc.with_ymd_and_hms(2026, 5, 1, 10, 0, 0).unwrap(),
            completed_at: Utc.with_ymd_and_hms(2026, 5, 1, 14, 0, 0).unwrap(),
            status: JobStatus::Delivered,
            source: JobSource::Telemetry,
            flagged: false,
            flag_reason: None,
            notes: None,
        }
    }

    fn sample_truck() -> Truck {
        Truck {
            id: "truck_abcd1234".into(),
            make: "Scania".into(),
            model: "R 730".into(),
            first_seen_at: Utc.with_ymd_and_hms(2026, 4, 1, 9, 0, 0).unwrap(),
            total_distance_km: 1234.5,
            total_delivery_distance: 1000.0,
            total_free_roam_distance: 234.5,
            total_jobs: 7,
        }
    }

    #[test]
    fn enums_serialize_as_screaming_snake_case() {
        assert_eq!(
            serde_json::to_string(&SessionKind::FreeRoam).unwrap(),
            "\"FREE_ROAM\""
        );
        assert_eq!(serde_json::to_string(&SessionKind::Job).unwrap(), "\"JOB\"");
        assert_eq!(
            serde_json::to_string(&SessionStatus::Cancelled).unwrap(),
            "\"CANCELLED\""
        );
        assert_eq!(
            serde_json::to_string(&JobStatus::Delivered).unwrap(),
            "\"DELIVERED\""
        );
        assert_eq!(
            serde_json::to_string(&JobSource::Telemetry).unwrap(),
            "\"TELEMETRY\""
        );
        assert_eq!(
            serde_json::to_string(&JobSource::Manual).unwrap(),
            "\"MANUAL\""
        );
    }

    #[test]
    fn driving_session_json_round_trip() {
        let s = sample_session();
        let json = serde_json::to_string(&s).unwrap();
        let parsed: DrivingSession = serde_json::from_str(&json).unwrap();
        assert_eq!(s.id, parsed.id);
        assert_eq!(s.kind, parsed.kind);
        assert_eq!(s.truck_make, parsed.truck_make);
        assert_eq!(s.distance_km, parsed.distance_km);
        assert_eq!(s.started_at, parsed.started_at);
        assert_eq!(s.ended_at, parsed.ended_at);
        assert_eq!(s.status, parsed.status);
    }

    #[test]
    fn driving_session_uses_camel_case_keys() {
        let json = serde_json::to_value(sample_session()).unwrap();
        let keys: Vec<&str> = json.as_object().unwrap().keys().map(String::as_str).collect();
        for key in [
            "id",
            "kind",
            "truckMake",
            "truckModel",
            "startOdometer",
            "endOdometer",
            "distanceKm",
            "startedAt",
            "endedAt",
            "status",
        ] {
            assert!(keys.contains(&key), "missing key {key}; got {keys:?}");
        }
    }

    #[test]
    fn job_json_round_trip() {
        let j = sample_job();
        let json = serde_json::to_string(&j).unwrap();
        let parsed: Job = serde_json::from_str(&json).unwrap();
        assert_eq!(j.id, parsed.id);
        assert_eq!(j.session_id, parsed.session_id);
        assert_eq!(j.cargo_mass_kg, parsed.cargo_mass_kg);
        assert_eq!(j.declared_income, parsed.declared_income);
        assert_eq!(j.duration_sec, parsed.duration_sec);
        assert_eq!(j.status, parsed.status);
        assert_eq!(j.source, parsed.source);
        assert_eq!(j.flagged, parsed.flagged);
        assert_eq!(j.fuel_used_l, parsed.fuel_used_l);
    }

    #[test]
    fn job_with_null_optionals_round_trips() {
        let mut j = sample_job();
        j.session_id = None;
        j.fuel_used_l = None;
        j.damage_delta = None;
        j.flag_reason = None;
        j.notes = None;
        let json = serde_json::to_string(&j).unwrap();
        let parsed: Job = serde_json::from_str(&json).unwrap();
        assert!(parsed.session_id.is_none());
        assert!(parsed.fuel_used_l.is_none());
        assert!(parsed.damage_delta.is_none());
        assert!(parsed.flag_reason.is_none());
        assert!(parsed.notes.is_none());
    }

    #[test]
    fn truck_json_round_trip() {
        let t = sample_truck();
        let json = serde_json::to_string(&t).unwrap();
        let parsed: Truck = serde_json::from_str(&json).unwrap();
        assert_eq!(t.id, parsed.id);
        assert_eq!(t.make, parsed.make);
        assert_eq!(t.total_distance_km, parsed.total_distance_km);
        assert_eq!(t.total_jobs, parsed.total_jobs);
    }

    #[test]
    fn new_session_round_trip() {
        let n = NewSession {
            kind: SessionKind::FreeRoam,
            truck_make: Some("Volvo".into()),
            truck_model: Some("FH 540".into()),
            start_odometer: 0.0,
            end_odometer: 42.0,
            started_at: Utc.with_ymd_and_hms(2026, 5, 2, 8, 0, 0).unwrap(),
            ended_at: Utc.with_ymd_and_hms(2026, 5, 2, 8, 30, 0).unwrap(),
            status: SessionStatus::Completed,
        };
        let json = serde_json::to_string(&n).unwrap();
        let parsed: NewSession = serde_json::from_str(&json).unwrap();
        assert_eq!(n.kind, parsed.kind);
        assert_eq!(n.truck_make, parsed.truck_make);
        assert_eq!(n.end_odometer, parsed.end_odometer);
    }

    #[test]
    fn new_job_round_trip() {
        let n = NewJob {
            session_id: None,
            truck_make: None,
            truck_model: None,
            source_city: "Paris".into(),
            source_company: "Acme".into(),
            dest_city: "Lyon".into(),
            dest_company: "Beta".into(),
            cargo_id: "logs".into(),
            cargo_name: "Logs".into(),
            cargo_mass_kg: 18000.0,
            declared_income: 3100,
            distance_km: 465.0,
            duration_sec: 18_000,
            fuel_used_l: None,
            damage_delta: None,
            started_at: Utc.with_ymd_and_hms(2026, 5, 2, 10, 0, 0).unwrap(),
            completed_at: Utc.with_ymd_and_hms(2026, 5, 2, 15, 0, 0).unwrap(),
            status: JobStatus::Delivered,
            source: JobSource::Manual,
            notes: Some("first manual entry".into()),
        };
        let json = serde_json::to_string(&n).unwrap();
        let parsed: NewJob = serde_json::from_str(&json).unwrap();
        assert_eq!(n.source_city, parsed.source_city);
        assert_eq!(n.declared_income, parsed.declared_income);
        assert_eq!(n.source, parsed.source);
        assert_eq!(n.notes, parsed.notes);
    }
}

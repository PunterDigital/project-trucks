//! Commands and helpers for `jobs`.
//!
//! Like sessions.rs, the helper layer (`insert_job`) is exposed so M2's
//! telemetry pipeline can call it without going through the Tauri wrapper.

#![allow(dead_code)]

use anyhow::{Context, Result};
use sqlx::{QueryBuilder, Sqlite, SqlitePool};
use tauri::State;
use uuid::Uuid;

use crate::db::models::{Job, JobFilter, JobPatch, JobsPage, NewJob};
use crate::db::rollups::recompute_truck;
use crate::db::AppState;

const DEFAULT_LIMIT: i64 = 25;
const MAX_LIMIT: i64 = 200;

/// Insert a job row. Generates the id, computes `avg_speed_kmh` from
/// distance/duration, then refreshes the truck rollup. Returns the inserted
/// row.
pub async fn insert_job(pool: &SqlitePool, input: NewJob) -> Result<Job> {
    let id = format!("job_{}", Uuid::new_v4().simple());
    let avg_speed_kmh = compute_avg_speed(input.distance_km, input.duration_sec);
    let flagged = false;
    let flag_reason: Option<String> = None;

    sqlx::query(
        "INSERT INTO jobs
           (id, session_id, truck_make, truck_model,
            source_city, source_company, dest_city, dest_company,
            cargo_id, cargo_name, cargo_mass_kg, declared_income,
            distance_km, duration_sec, avg_speed_kmh,
            fuel_used_l, damage_delta,
            started_at, completed_at, status, source,
            flagged, flag_reason, notes)
         VALUES (?, ?, ?, ?,
                 ?, ?, ?, ?,
                 ?, ?, ?, ?,
                 ?, ?, ?,
                 ?, ?,
                 ?, ?, ?, ?,
                 ?, ?, ?)",
    )
    .bind(&id)
    .bind(&input.session_id)
    .bind(&input.truck_make)
    .bind(&input.truck_model)
    .bind(&input.source_city)
    .bind(&input.source_company)
    .bind(&input.dest_city)
    .bind(&input.dest_company)
    .bind(&input.cargo_id)
    .bind(&input.cargo_name)
    .bind(input.cargo_mass_kg)
    .bind(input.declared_income)
    .bind(input.distance_km)
    .bind(input.duration_sec)
    .bind(avg_speed_kmh)
    .bind(input.fuel_used_l)
    .bind(input.damage_delta)
    .bind(input.started_at)
    .bind(input.completed_at)
    .bind(input.status)
    .bind(input.source)
    .bind(flagged)
    .bind(&flag_reason)
    .bind(&input.notes)
    .execute(pool)
    .await
    .context("inserting job")?;

    if let (Some(make), Some(model)) = (&input.truck_make, &input.truck_model) {
        recompute_truck(pool, make, model).await?;
    }

    Ok(Job {
        id,
        session_id: input.session_id,
        truck_make: input.truck_make,
        truck_model: input.truck_model,
        source_city: input.source_city,
        source_company: input.source_company,
        dest_city: input.dest_city,
        dest_company: input.dest_company,
        cargo_id: input.cargo_id,
        cargo_name: input.cargo_name,
        cargo_mass_kg: input.cargo_mass_kg,
        declared_income: input.declared_income,
        distance_km: input.distance_km,
        duration_sec: input.duration_sec,
        avg_speed_kmh,
        fuel_used_l: input.fuel_used_l,
        damage_delta: input.damage_delta,
        started_at: input.started_at,
        completed_at: input.completed_at,
        status: input.status,
        source: input.source,
        flagged,
        flag_reason,
        notes: input.notes,
    })
}

fn compute_avg_speed(distance_km: f64, duration_sec: i64) -> f64 {
    if duration_sec <= 0 {
        return 0.0;
    }
    distance_km / (duration_sec as f64 / 3600.0)
}

/// Read a paginated, filtered slice of `jobs`. Ordered by `completed_at DESC`.
pub async fn fetch_jobs(pool: &SqlitePool, filter: JobFilter) -> Result<JobsPage> {
    let limit = filter.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let offset = filter.offset.unwrap_or(0).max(0);
    let search_pattern = filter.search.as_deref().map(|s| format!("%{}%", s));

    let mut qb: QueryBuilder<Sqlite> = QueryBuilder::new("SELECT * FROM jobs WHERE 1=1");
    apply_job_filters(&mut qb, &filter, search_pattern.as_deref());
    qb.push(" ORDER BY completed_at DESC LIMIT ");
    qb.push_bind(limit);
    qb.push(" OFFSET ");
    qb.push_bind(offset);

    let items: Vec<Job> = qb
        .build_query_as::<Job>()
        .fetch_all(pool)
        .await
        .context("listing jobs")?;

    let mut cb: QueryBuilder<Sqlite> = QueryBuilder::new("SELECT COUNT(*) FROM jobs WHERE 1=1");
    apply_job_filters(&mut cb, &filter, search_pattern.as_deref());
    let total: (i64,) = cb
        .build_query_as::<(i64,)>()
        .fetch_one(pool)
        .await
        .context("counting jobs")?;

    Ok(JobsPage {
        items,
        total: total.0,
    })
}

fn apply_job_filters<'q>(
    qb: &mut QueryBuilder<'q, Sqlite>,
    filter: &'q JobFilter,
    search_pattern: Option<&'q str>,
) {
    if let Some(status) = filter.status {
        qb.push(" AND status = ");
        qb.push_bind(status);
    }
    if let Some(from) = filter.from {
        qb.push(" AND completed_at >= ");
        qb.push_bind(from);
    }
    if let Some(to) = filter.to {
        qb.push(" AND completed_at < ");
        qb.push_bind(to);
    }
    if let Some(pattern) = search_pattern {
        qb.push(" AND (source_city LIKE ");
        qb.push_bind(pattern);
        qb.push(" OR dest_city LIKE ");
        qb.push_bind(pattern);
        qb.push(" OR source_company LIKE ");
        qb.push_bind(pattern);
        qb.push(" OR dest_company LIKE ");
        qb.push_bind(pattern);
        qb.push(" OR cargo_name LIKE ");
        qb.push_bind(pattern);
        qb.push(")");
    }
}

pub async fn fetch_job(pool: &SqlitePool, id: &str) -> Result<Option<Job>> {
    sqlx::query_as::<_, Job>("SELECT * FROM jobs WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
        .context("loading job")
}

/// Update notes / flagged / flag_reason on a job. Returns the row after the
/// update. Truck make/model are not editable here, so no rollup recompute.
pub async fn patch_job(pool: &SqlitePool, id: &str, patch: JobPatch) -> Result<Option<Job>> {
    if patch.notes.is_none() && patch.flagged.is_none() && patch.flag_reason.is_none() {
        // Nothing to do — return the row as-is.
        return fetch_job(pool, id).await;
    }

    let mut qb: QueryBuilder<Sqlite> = QueryBuilder::new("UPDATE jobs SET ");
    let mut sep = qb.separated(", ");
    if let Some(notes) = &patch.notes {
        sep.push("notes = ");
        sep.push_bind_unseparated(notes);
    }
    if let Some(flagged) = patch.flagged {
        sep.push("flagged = ");
        sep.push_bind_unseparated(flagged);
    }
    if let Some(flag_reason) = &patch.flag_reason {
        sep.push("flag_reason = ");
        sep.push_bind_unseparated(flag_reason);
    }
    qb.push(" WHERE id = ");
    qb.push_bind(id);

    qb.build().execute(pool).await.context("updating job")?;

    fetch_job(pool, id).await
}

/// Delete a job row. Refreshes the affected truck rollup if the job had
/// make/model set. Does not touch the linked `driving_session` — the km still
/// happened, even if the user wants to forget about the delivery.
pub async fn remove_job(pool: &SqlitePool, id: &str) -> Result<()> {
    let row: Option<(Option<String>, Option<String>)> =
        sqlx::query_as("SELECT truck_make, truck_model FROM jobs WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await
            .context("loading job for delete")?;

    sqlx::query("DELETE FROM jobs WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await
        .context("deleting job")?;

    if let Some((Some(make), Some(model))) = row {
        recompute_truck(pool, &make, &model).await?;
    }
    Ok(())
}

// --- Tauri command wrappers ---

#[tauri::command]
pub async fn create_job(state: State<'_, AppState>, input: NewJob) -> Result<Job, String> {
    insert_job(&state.pool, input)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_jobs(
    state: State<'_, AppState>,
    filter: Option<JobFilter>,
) -> Result<JobsPage, String> {
    fetch_jobs(&state.pool, filter.unwrap_or_default())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_job(state: State<'_, AppState>, id: String) -> Result<Option<Job>, String> {
    fetch_job(&state.pool, &id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn update_job(
    state: State<'_, AppState>,
    id: String,
    patch: JobPatch,
) -> Result<Option<Job>, String> {
    patch_job(&state.pool, &id, patch)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_job(state: State<'_, AppState>, id: String) -> Result<(), String> {
    remove_job(&state.pool, &id)
        .await
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::sessions::insert_session;
    use crate::db::models::{JobSource, JobStatus, NewSession, SessionKind, SessionStatus};
    use crate::db::rollups::truck_id;
    use chrono::{TimeZone, Utc};
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use std::str::FromStr;

    async fn test_pool() -> SqlitePool {
        let opts = SqliteConnectOptions::from_str("sqlite::memory:")
            .unwrap()
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        pool
    }

    fn sample_job(session_id: Option<String>) -> NewJob {
        NewJob {
            session_id,
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
            distance_km: 290.0,
            duration_sec: 14_400, // 4h
            fuel_used_l: Some(95.5),
            damage_delta: Some(0.02),
            started_at: Utc.with_ymd_and_hms(2026, 5, 1, 10, 0, 0).unwrap(),
            completed_at: Utc.with_ymd_and_hms(2026, 5, 1, 14, 0, 0).unwrap(),
            status: JobStatus::Delivered,
            source: JobSource::Manual,
            notes: Some("first trip".into()),
        }
    }

    #[test]
    fn avg_speed_handles_zero_duration() {
        assert_eq!(compute_avg_speed(100.0, 0), 0.0);
        assert_eq!(compute_avg_speed(100.0, -1), 0.0);
    }

    #[test]
    fn avg_speed_basic_math() {
        assert!((compute_avg_speed(290.0, 14_400) - 72.5).abs() < 1e-9);
    }

    #[tokio::test]
    async fn inserting_manual_job_computes_avg_speed_and_updates_truck() {
        let pool = test_pool().await;
        let job = insert_job(&pool, sample_job(None)).await.unwrap();
        assert!(job.id.starts_with("job_"));
        assert!((job.avg_speed_kmh - 72.5).abs() < 1e-9);
        assert_eq!(job.source, JobSource::Manual);
        assert!(!job.flagged);

        let (total_jobs, total_distance): (i64, f64) =
            sqlx::query_as("SELECT total_jobs, total_distance_km FROM trucks WHERE id = ?")
                .bind(truck_id("Scania", "R 730"))
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(total_jobs, 1);
        assert_eq!(total_distance, 0.0);
    }

    #[tokio::test]
    async fn telemetry_job_links_to_session_and_both_contribute_to_truck() {
        let pool = test_pool().await;
        let session = insert_session(
            &pool,
            NewSession {
                kind: SessionKind::Job,
                truck_make: Some("Scania".into()),
                truck_model: Some("R 730".into()),
                start_odometer: 100.0,
                end_odometer: 390.0,
                started_at: Utc.with_ymd_and_hms(2026, 5, 1, 10, 0, 0).unwrap(),
                ended_at: Utc.with_ymd_and_hms(2026, 5, 1, 14, 0, 0).unwrap(),
                status: SessionStatus::Completed,
            },
        )
        .await
        .unwrap();

        let mut input = sample_job(Some(session.id.clone()));
        input.source = JobSource::Telemetry;
        let job = insert_job(&pool, input).await.unwrap();
        assert_eq!(job.session_id.as_deref(), Some(session.id.as_str()));

        let (total, delivery, free_roam, jobs): (f64, f64, f64, i64) = sqlx::query_as(
            "SELECT total_distance_km, total_delivery_distance, total_free_roam_distance, total_jobs
               FROM trucks WHERE id = ?",
        )
        .bind(truck_id("Scania", "R 730"))
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(total, 290.0);
        assert_eq!(delivery, 290.0);
        assert_eq!(free_roam, 0.0);
        assert_eq!(jobs, 1);
    }

    #[tokio::test]
    async fn fetch_jobs_paginates_and_orders_newest_first() {
        let pool = test_pool().await;
        for hour in [8u32, 10, 12] {
            let mut input = sample_job(None);
            input.completed_at = Utc.with_ymd_and_hms(2026, 5, 1, hour, 0, 0).unwrap();
            insert_job(&pool, input).await.unwrap();
        }

        let page = fetch_jobs(
            &pool,
            JobFilter {
                limit: Some(2),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(page.total, 3);
        assert_eq!(page.items.len(), 2);
        assert_eq!(
            page.items[0].completed_at,
            Utc.with_ymd_and_hms(2026, 5, 1, 12, 0, 0).unwrap()
        );
    }

    #[tokio::test]
    async fn fetch_jobs_search_matches_city_and_cargo() {
        let pool = test_pool().await;
        let mut a = sample_job(None);
        a.source_city = "Hamburg".into();
        a.cargo_name = "Containers".into();
        insert_job(&pool, a).await.unwrap();

        let mut b = sample_job(None);
        b.source_city = "Paris".into();
        b.cargo_name = "Logs".into();
        insert_job(&pool, b).await.unwrap();

        let only_paris = fetch_jobs(
            &pool,
            JobFilter {
                search: Some("paris".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(only_paris.total, 1);
        assert_eq!(only_paris.items[0].source_city, "Paris");

        let by_cargo = fetch_jobs(
            &pool,
            JobFilter {
                search: Some("contain".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(by_cargo.total, 1);
        assert_eq!(by_cargo.items[0].cargo_name, "Containers");
    }

    #[tokio::test]
    async fn fetch_jobs_filters_by_status_and_date_range() {
        let pool = test_pool().await;
        let mut delivered = sample_job(None);
        delivered.status = JobStatus::Delivered;
        delivered.completed_at = Utc.with_ymd_and_hms(2026, 5, 1, 10, 0, 0).unwrap();
        insert_job(&pool, delivered).await.unwrap();

        let mut late = sample_job(None);
        late.status = JobStatus::Late;
        late.completed_at = Utc.with_ymd_and_hms(2026, 5, 2, 10, 0, 0).unwrap();
        insert_job(&pool, late).await.unwrap();

        let only_late = fetch_jobs(
            &pool,
            JobFilter {
                status: Some(JobStatus::Late),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(only_late.total, 1);
        assert_eq!(only_late.items[0].status, JobStatus::Late);

        let only_first_day = fetch_jobs(
            &pool,
            JobFilter {
                from: Some(Utc.with_ymd_and_hms(2026, 5, 1, 0, 0, 0).unwrap()),
                to: Some(Utc.with_ymd_and_hms(2026, 5, 2, 0, 0, 0).unwrap()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(only_first_day.total, 1);
        assert_eq!(only_first_day.items[0].status, JobStatus::Delivered);
    }

    #[tokio::test]
    async fn fetch_job_returns_none_for_unknown_id() {
        let pool = test_pool().await;
        let found = fetch_job(&pool, "job_nope").await.unwrap();
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn patch_job_updates_only_provided_fields() {
        let pool = test_pool().await;
        let job = insert_job(&pool, sample_job(None)).await.unwrap();

        let patched = patch_job(
            &pool,
            &job.id,
            JobPatch {
                notes: Some("updated note".into()),
                flagged: None,
                flag_reason: None,
            },
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(patched.notes.as_deref(), Some("updated note"));
        assert!(!patched.flagged);

        let patched2 = patch_job(
            &pool,
            &job.id,
            JobPatch {
                notes: None,
                flagged: Some(true),
                flag_reason: Some("avg speed > 150".into()),
            },
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(patched2.notes.as_deref(), Some("updated note")); // unchanged
        assert!(patched2.flagged);
        assert_eq!(patched2.flag_reason.as_deref(), Some("avg speed > 150"));
    }

    #[tokio::test]
    async fn patch_job_empty_patch_is_a_noop() {
        let pool = test_pool().await;
        let job = insert_job(&pool, sample_job(None)).await.unwrap();
        let patched = patch_job(&pool, &job.id, JobPatch::default())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(patched.notes, job.notes);
        assert_eq!(patched.flagged, job.flagged);
    }

    #[tokio::test]
    async fn delete_job_drops_count_keeps_session_distance() {
        let pool = test_pool().await;
        let session = insert_session(
            &pool,
            NewSession {
                kind: SessionKind::Job,
                truck_make: Some("Scania".into()),
                truck_model: Some("R 730".into()),
                start_odometer: 0.0,
                end_odometer: 200.0,
                started_at: Utc.with_ymd_and_hms(2026, 5, 1, 10, 0, 0).unwrap(),
                ended_at: Utc.with_ymd_and_hms(2026, 5, 1, 13, 0, 0).unwrap(),
                status: SessionStatus::Completed,
            },
        )
        .await
        .unwrap();
        let mut input = sample_job(Some(session.id.clone()));
        input.source = JobSource::Telemetry;
        let job = insert_job(&pool, input).await.unwrap();

        remove_job(&pool, &job.id).await.unwrap();

        // Session row survives — the km still happened.
        let (total, delivery, jobs): (f64, f64, i64) = sqlx::query_as(
            "SELECT total_distance_km, total_delivery_distance, total_jobs
               FROM trucks WHERE id = ?",
        )
        .bind(truck_id("Scania", "R 730"))
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(total, 200.0);
        assert_eq!(delivery, 200.0);
        assert_eq!(jobs, 0);
    }

    #[tokio::test]
    async fn delete_unknown_job_is_noop() {
        let pool = test_pool().await;
        remove_job(&pool, "job_nope").await.unwrap();
    }
}

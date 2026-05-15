//! Commands and helpers for `jobs`.
//!
//! Like sessions.rs, the helper layer (`insert_job`) is exposed so M2's
//! telemetry pipeline can call it without going through the Tauri wrapper.

#![allow(dead_code)]

use anyhow::{Context, Result};
use sqlx::SqlitePool;
use tauri::State;
use uuid::Uuid;

use crate::db::models::{Job, NewJob};
use crate::db::rollups::recompute_truck;
use crate::db::AppState;

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

#[tauri::command]
pub async fn create_job(state: State<'_, AppState>, input: NewJob) -> Result<Job, String> {
    insert_job(&state.pool, input)
        .await
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::sessions::insert_session;
    use crate::db::models::{
        JobSource, JobStatus, NewSession, SessionKind, SessionStatus,
    };
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
        // 290 km in 4 hours == 72.5 km/h
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

        // Truck rollup: jobs += 1, but distance is 0 since no driving_session
        // row exists for this manual job.
        let (total_jobs, total_distance): (i64, f64) = sqlx::query_as(
            "SELECT total_jobs, total_distance_km FROM trucks WHERE id = ?",
        )
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
        // Telemetry path: first the session, then the job tied to it.
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
        assert_eq!(session.distance_km, 290.0);

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
}

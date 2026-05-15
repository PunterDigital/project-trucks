//! Commands and helpers for `driving_sessions`.
//!
//! The helpers (insert_session, etc.) are exposed at module level so the
//! telemetry pipeline in M2 can call them directly without going through the
//! Tauri command wrapper. That keeps the manual-entry path and the auto-
//! capture path on a single write code path.

#![allow(dead_code)]

use anyhow::{Context, Result};
use sqlx::SqlitePool;
use tauri::State;
use uuid::Uuid;

use crate::db::models::{DrivingSession, NewSession};
use crate::db::rollups::recompute_truck;
use crate::db::AppState;

/// Insert a driving session and refresh the truck rollup. Returns the inserted
/// row. The id is generated server-side as `ses_<uuid-simple>`.
pub async fn insert_session(pool: &SqlitePool, input: NewSession) -> Result<DrivingSession> {
    let id = format!("ses_{}", Uuid::new_v4().simple());
    let distance_km = input.end_odometer - input.start_odometer;

    sqlx::query(
        "INSERT INTO driving_sessions
           (id, kind, truck_make, truck_model, start_odometer, end_odometer,
            distance_km, started_at, ended_at, status)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(input.kind)
    .bind(&input.truck_make)
    .bind(&input.truck_model)
    .bind(input.start_odometer)
    .bind(input.end_odometer)
    .bind(distance_km)
    .bind(input.started_at)
    .bind(input.ended_at)
    .bind(input.status)
    .execute(pool)
    .await
    .context("inserting driving_session")?;

    if let (Some(make), Some(model)) = (&input.truck_make, &input.truck_model) {
        recompute_truck(pool, make, model).await?;
    }

    Ok(DrivingSession {
        id,
        kind: input.kind,
        truck_make: input.truck_make,
        truck_model: input.truck_model,
        start_odometer: input.start_odometer,
        end_odometer: input.end_odometer,
        distance_km,
        started_at: input.started_at,
        ended_at: input.ended_at,
        status: input.status,
    })
}

#[tauri::command]
pub async fn create_session(
    state: State<'_, AppState>,
    input: NewSession,
) -> Result<DrivingSession, String> {
    insert_session(&state.pool, input)
        .await
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::models::{SessionKind, SessionStatus};
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

    fn sample_input(kind: SessionKind, distance: f64) -> NewSession {
        NewSession {
            kind,
            truck_make: Some("Scania".into()),
            truck_model: Some("R 730".into()),
            start_odometer: 0.0,
            end_odometer: distance,
            started_at: Utc.with_ymd_and_hms(2026, 5, 1, 10, 0, 0).unwrap(),
            ended_at: Utc.with_ymd_and_hms(2026, 5, 1, 11, 0, 0).unwrap(),
            status: SessionStatus::Completed,
        }
    }

    #[tokio::test]
    async fn inserting_free_roam_updates_truck_rollup() {
        let pool = test_pool().await;
        let session = insert_session(&pool, sample_input(SessionKind::FreeRoam, 42.0))
            .await
            .unwrap();
        assert_eq!(session.distance_km, 42.0);
        assert_eq!(session.kind, SessionKind::FreeRoam);
        assert!(session.id.starts_with("ses_"));

        let (total, delivery, free_roam): (f64, f64, f64) = sqlx::query_as(
            "SELECT total_distance_km, total_delivery_distance, total_free_roam_distance
               FROM trucks WHERE id = ?",
        )
        .bind(truck_id("Scania", "R 730"))
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(total, 42.0);
        assert_eq!(delivery, 0.0);
        assert_eq!(free_roam, 42.0);
    }

    #[tokio::test]
    async fn inserting_job_session_records_delivery_km() {
        let pool = test_pool().await;
        insert_session(&pool, sample_input(SessionKind::Job, 300.0))
            .await
            .unwrap();
        let (delivery,): (f64,) = sqlx::query_as(
            "SELECT total_delivery_distance FROM trucks WHERE id = ?",
        )
        .bind(truck_id("Scania", "R 730"))
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(delivery, 300.0);
    }

    #[tokio::test]
    async fn session_without_truck_make_skips_rollup() {
        let pool = test_pool().await;
        let mut input = sample_input(SessionKind::FreeRoam, 10.0);
        input.truck_make = None;
        input.truck_model = None;
        let session = insert_session(&pool, input).await.unwrap();
        assert_eq!(session.distance_km, 10.0);

        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM trucks")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count.0, 0);
    }

    #[tokio::test]
    async fn two_sessions_accumulate_into_one_truck_row() {
        let pool = test_pool().await;
        insert_session(&pool, sample_input(SessionKind::FreeRoam, 30.0))
            .await
            .unwrap();
        insert_session(&pool, sample_input(SessionKind::Job, 120.0))
            .await
            .unwrap();
        let (total, delivery, free_roam): (f64, f64, f64) = sqlx::query_as(
            "SELECT total_distance_km, total_delivery_distance, total_free_roam_distance
               FROM trucks WHERE id = ?",
        )
        .bind(truck_id("Scania", "R 730"))
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(total, 150.0);
        assert_eq!(delivery, 120.0);
        assert_eq!(free_roam, 30.0);
    }
}

//! Read commands for `trucks`.
//!
//! No write commands here — the trucks table is purely derived from sessions
//! and jobs, maintained by `db::rollups::recompute_truck` inside every write
//! path. The list/get commands just expose the cached rollup to the UI.

#![allow(dead_code)]

use anyhow::{Context, Result};
use sqlx::SqlitePool;
use tauri::State;

use crate::db::models::Truck;
use crate::db::AppState;

pub async fn fetch_trucks(pool: &SqlitePool) -> Result<Vec<Truck>> {
    sqlx::query_as::<_, Truck>(
        "SELECT * FROM trucks ORDER BY total_distance_km DESC, first_seen_at DESC",
    )
    .fetch_all(pool)
    .await
    .context("listing trucks")
}

pub async fn fetch_truck(pool: &SqlitePool, id: &str) -> Result<Option<Truck>> {
    sqlx::query_as::<_, Truck>("SELECT * FROM trucks WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
        .context("loading truck")
}

// --- Tauri command wrappers ---

#[tauri::command]
pub async fn list_trucks(state: State<'_, AppState>) -> Result<Vec<Truck>, String> {
    fetch_trucks(&state.pool).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_truck(state: State<'_, AppState>, id: String) -> Result<Option<Truck>, String> {
    fetch_truck(&state.pool, &id)
        .await
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::jobs::insert_job;
    use crate::commands::sessions::insert_session;
    use crate::db::models::{JobSource, JobStatus, NewJob, NewSession, SessionKind, SessionStatus};
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

    fn session(kind: SessionKind, make: &str, model: &str, distance: f64) -> NewSession {
        NewSession {
            kind,
            truck_make: Some(make.into()),
            truck_model: Some(model.into()),
            start_odometer: 0.0,
            end_odometer: distance,
            started_at: Utc.with_ymd_and_hms(2026, 5, 1, 10, 0, 0).unwrap(),
            ended_at: Utc.with_ymd_and_hms(2026, 5, 1, 11, 0, 0).unwrap(),
            status: SessionStatus::Completed,
        }
    }

    fn manual_job(make: &str, model: &str) -> NewJob {
        NewJob {
            session_id: None,
            truck_make: Some(make.into()),
            truck_model: Some(model.into()),
            source_city: "A".into(),
            source_company: "a".into(),
            dest_city: "B".into(),
            dest_company: "b".into(),
            cargo_id: "c".into(),
            cargo_name: "C".into(),
            cargo_mass_kg: 1000.0,
            declared_income: 1000,
            distance_km: 100.0,
            duration_sec: 3600,
            fuel_used_l: None,
            damage_delta: None,
            started_at: Utc.with_ymd_and_hms(2026, 5, 1, 10, 0, 0).unwrap(),
            completed_at: Utc.with_ymd_and_hms(2026, 5, 1, 11, 0, 0).unwrap(),
            status: JobStatus::Delivered,
            source: JobSource::Manual,
            notes: None,
        }
    }

    #[tokio::test]
    async fn fetch_trucks_returns_empty_on_fresh_db() {
        let pool = test_pool().await;
        let trucks = fetch_trucks(&pool).await.unwrap();
        assert!(trucks.is_empty());
    }

    #[tokio::test]
    async fn fetch_trucks_orders_by_total_distance_desc() {
        let pool = test_pool().await;
        // Volvo: 25 km free-roam
        insert_session(&pool, session(SessionKind::FreeRoam, "Volvo", "FH", 25.0))
            .await
            .unwrap();
        // Scania: 200 km job
        insert_session(&pool, session(SessionKind::Job, "Scania", "R 730", 200.0))
            .await
            .unwrap();
        // Mercedes: 100 km job + 30 km free-roam = 130
        insert_session(
            &pool,
            session(SessionKind::Job, "Mercedes", "Actros", 100.0),
        )
        .await
        .unwrap();
        insert_session(
            &pool,
            session(SessionKind::FreeRoam, "Mercedes", "Actros", 30.0),
        )
        .await
        .unwrap();

        let trucks = fetch_trucks(&pool).await.unwrap();
        assert_eq!(trucks.len(), 3);
        assert_eq!(trucks[0].make, "Scania");
        assert_eq!(trucks[1].make, "Mercedes");
        assert_eq!(trucks[2].make, "Volvo");
        assert_eq!(trucks[0].total_distance_km, 200.0);
        assert_eq!(trucks[1].total_distance_km, 130.0);
        assert_eq!(trucks[2].total_distance_km, 25.0);
    }

    #[tokio::test]
    async fn manual_jobs_count_their_own_distance_as_delivery() {
        let pool = test_pool().await;
        insert_job(&pool, manual_job("Iveco", "S-Way"))
            .await
            .unwrap();
        insert_job(&pool, manual_job("Iveco", "S-Way"))
            .await
            .unwrap();
        let trucks = fetch_trucks(&pool).await.unwrap();
        assert_eq!(trucks.len(), 1);
        let t = &trucks[0];
        assert_eq!(t.make, "Iveco");
        assert_eq!(t.total_jobs, 2);
        // Manual entries have no driving session, so each job's own km
        // counts as delivery distance.
        assert_eq!(t.total_distance_km, 200.0);
        assert_eq!(t.total_delivery_distance, 200.0);
        assert_eq!(t.total_free_roam_distance, 0.0);
    }

    #[tokio::test]
    async fn fetch_truck_returns_some_for_known_id_and_none_otherwise() {
        let pool = test_pool().await;
        insert_session(&pool, session(SessionKind::Job, "Scania", "R 730", 200.0))
            .await
            .unwrap();
        let id = truck_id("Scania", "R 730");
        let found = fetch_truck(&pool, &id).await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().make, "Scania");

        let missing = fetch_truck(&pool, "nope").await.unwrap();
        assert!(missing.is_none());
    }
}

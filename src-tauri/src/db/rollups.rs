//! Truck-rollup maintenance.
//!
//! The `trucks` table is purely derived state — it's the cached sum of every
//! row in `driving_sessions` and `jobs` for a given `(make, model)` pair.
//! Distance comes from sessions, plus the `distance_km` of manual jobs (which
//! have no session), counted as delivery distance.
//! Every write path that touches sessions or jobs must call
//! [`recompute_truck`] in the same transaction so the rollup never drifts.
//!
//! Truck id = first 16 hex chars of `sha256(make + "|" + model)`. Stable across
//! runs and trivially derivable from any (make, model) pair, so we don't need
//! a separate lookup table.

#![allow(dead_code)]

use anyhow::Result;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

/// Stable id for a `(make, model)` pair. 64-bit's worth of hex.
pub fn truck_id(make: &str, model: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(make.as_bytes());
    hasher.update(b"|");
    hasher.update(model.as_bytes());
    let bytes = hasher.finalize();
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    hex[..16].to_string()
}

/// Rebuild the `trucks` row for `(make, model)` from scratch by summing the
/// underlying sessions and jobs tables. Inserts a fresh row if none existed.
///
/// No-op when `make` or `model` is empty — sessions without truck info shouldn't
/// create phantom trucks.
pub async fn recompute_truck(pool: &SqlitePool, make: &str, model: &str) -> Result<()> {
    if make.is_empty() || model.is_empty() {
        return Ok(());
    }

    let id = truck_id(make, model);

    // Note the `ELSE 0.0` literals: SQLite's type affinity will demote SUM to
    // INTEGER if every CASE arm evaluates to an integer, which then fails to
    // decode into f64.
    let (total_distance, delivery_distance, free_roam_distance): (f64, f64, f64) =
        sqlx::query_as(
            "SELECT
               COALESCE(SUM(distance_km), 0.0) AS total,
               COALESCE(SUM(CASE WHEN kind = 'JOB' THEN distance_km ELSE 0.0 END), 0.0) AS delivery,
               COALESCE(SUM(CASE WHEN kind = 'FREE_ROAM' THEN distance_km ELSE 0.0 END), 0.0) AS free_roam
             FROM driving_sessions
             WHERE truck_make = ? AND truck_model = ?",
        )
        .bind(make)
        .bind(model)
        .fetch_one(pool)
        .await?;

    // Manual jobs have no driving_sessions row to carry their km, so their own
    // distance counts as delivery distance. Session-linked jobs are already in
    // the session sum above and must not be added twice. A telemetry job whose
    // session was deleted keeps source = 'TELEMETRY', so deleting a session
    // still drops its km.
    let (total_jobs, manual_job_distance): (i64, f64) = sqlx::query_as(
        "SELECT
           COUNT(*),
           COALESCE(SUM(CASE WHEN source = 'MANUAL' AND session_id IS NULL
                             THEN distance_km ELSE 0.0 END), 0.0)
         FROM jobs
         WHERE truck_make = ? AND truck_model = ?",
    )
    .bind(make)
    .bind(model)
    .fetch_one(pool)
    .await?;
    let total_distance = total_distance + manual_job_distance;
    let delivery_distance = delivery_distance + manual_job_distance;

    let now = chrono::Utc::now().to_rfc3339();

    // Upsert: first session/job for this truck inserts the row; subsequent
    // writes overwrite the totals. `first_seen_at` is set once and never
    // updated, which keeps the "first seen" semantic honest.
    sqlx::query(
        "INSERT INTO trucks
           (id, make, model, first_seen_at, total_distance_km,
            total_delivery_distance, total_free_roam_distance, total_jobs)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET
           total_distance_km        = excluded.total_distance_km,
           total_delivery_distance  = excluded.total_delivery_distance,
           total_free_roam_distance = excluded.total_free_roam_distance,
           total_jobs               = excluded.total_jobs",
    )
    .bind(&id)
    .bind(make)
    .bind(model)
    .bind(&now)
    .bind(total_distance)
    .bind(delivery_distance)
    .bind(free_roam_distance)
    .bind(total_jobs)
    .execute(pool)
    .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use std::str::FromStr;

    async fn test_pool() -> SqlitePool {
        // Shared in-memory database across pool connections, so the migrations
        // we apply once are visible to subsequent queries.
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

    async fn insert_session(
        pool: &SqlitePool,
        id: &str,
        kind: &str,
        make: &str,
        model: &str,
        distance: f64,
    ) {
        sqlx::query(
            "INSERT INTO driving_sessions
               (id, kind, truck_make, truck_model, start_odometer, end_odometer,
                distance_km, started_at, ended_at, status)
             VALUES (?, ?, ?, ?, 0.0, ?, ?, '2026-05-01T10:00:00Z', '2026-05-01T11:00:00Z', 'COMPLETED')",
        )
        .bind(id)
        .bind(kind)
        .bind(make)
        .bind(model)
        .bind(distance)
        .bind(distance)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn insert_job(pool: &SqlitePool, id: &str, session_id: &str, make: &str, model: &str) {
        insert_job_with(pool, id, Some(session_id), make, model, 100.0, "TELEMETRY").await;
    }

    async fn insert_manual_job(
        pool: &SqlitePool,
        id: &str,
        make: &str,
        model: &str,
        distance: f64,
    ) {
        insert_job_with(pool, id, None, make, model, distance, "MANUAL").await;
    }

    async fn insert_job_with(
        pool: &SqlitePool,
        id: &str,
        session_id: Option<&str>,
        make: &str,
        model: &str,
        distance: f64,
        source: &str,
    ) {
        sqlx::query(
            "INSERT INTO jobs
               (id, session_id, truck_make, truck_model,
                source_city, source_company, dest_city, dest_company,
                cargo_id, cargo_name, cargo_mass_kg, declared_income,
                distance_km, duration_sec, avg_speed_kmh,
                started_at, completed_at, status, source)
             VALUES (?, ?, ?, ?, 'A', 'a', 'B', 'b', 'c', 'C', 1000, 1000,
                     ?, 3600, 100.0,
                     '2026-05-01T10:00:00Z', '2026-05-01T11:00:00Z',
                     'DELIVERED', ?)",
        )
        .bind(id)
        .bind(session_id)
        .bind(make)
        .bind(model)
        .bind(distance)
        .bind(source)
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn truck_id_is_stable_and_distinct() {
        let scania = truck_id("Scania", "R 730");
        let volvo = truck_id("Volvo", "FH 540");
        assert_eq!(scania.len(), 16);
        assert_eq!(volvo.len(), 16);
        assert_ne!(scania, volvo);
        // Idempotent
        assert_eq!(scania, truck_id("Scania", "R 730"));
    }

    #[tokio::test]
    async fn recompute_with_no_rows_is_noop() {
        let pool = test_pool().await;
        recompute_truck(&pool, "Scania", "R 730").await.unwrap();
        // No rows in driving_sessions/jobs => trucks gets a row with zeros.
        let (total, delivery, free_roam, jobs): (f64, f64, f64, i64) = sqlx::query_as(
            "SELECT total_distance_km, total_delivery_distance, total_free_roam_distance, total_jobs
               FROM trucks WHERE id = ?",
        )
        .bind(truck_id("Scania", "R 730"))
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(total, 0.0);
        assert_eq!(delivery, 0.0);
        assert_eq!(free_roam, 0.0);
        assert_eq!(jobs, 0);
    }

    #[tokio::test]
    async fn empty_make_or_model_is_noop() {
        let pool = test_pool().await;
        recompute_truck(&pool, "", "R 730").await.unwrap();
        recompute_truck(&pool, "Scania", "").await.unwrap();
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM trucks")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count.0, 0, "trucks table should stay empty");
    }

    #[tokio::test]
    async fn free_roam_session_only_contributes_to_total_and_free_roam() {
        let pool = test_pool().await;
        insert_session(&pool, "ses_1", "FREE_ROAM", "Scania", "R 730", 50.0).await;
        recompute_truck(&pool, "Scania", "R 730").await.unwrap();

        let (total, delivery, free_roam, jobs): (f64, f64, f64, i64) = sqlx::query_as(
            "SELECT total_distance_km, total_delivery_distance, total_free_roam_distance, total_jobs
               FROM trucks WHERE id = ?",
        )
        .bind(truck_id("Scania", "R 730"))
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(total, 50.0);
        assert_eq!(delivery, 0.0);
        assert_eq!(free_roam, 50.0);
        assert_eq!(jobs, 0);
    }

    #[tokio::test]
    async fn job_session_plus_job_row_counts_both() {
        let pool = test_pool().await;
        insert_session(&pool, "ses_1", "JOB", "Scania", "R 730", 200.0).await;
        insert_job(&pool, "job_1", "ses_1", "Scania", "R 730").await;
        recompute_truck(&pool, "Scania", "R 730").await.unwrap();

        let (total, delivery, free_roam, jobs): (f64, f64, f64, i64) = sqlx::query_as(
            "SELECT total_distance_km, total_delivery_distance, total_free_roam_distance, total_jobs
               FROM trucks WHERE id = ?",
        )
        .bind(truck_id("Scania", "R 730"))
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(total, 200.0);
        assert_eq!(delivery, 200.0);
        assert_eq!(free_roam, 0.0);
        assert_eq!(jobs, 1);
    }

    #[tokio::test]
    async fn deleting_job_drops_job_count_but_keeps_session_km() {
        let pool = test_pool().await;
        insert_session(&pool, "ses_1", "JOB", "Scania", "R 730", 200.0).await;
        insert_job(&pool, "job_1", "ses_1", "Scania", "R 730").await;
        recompute_truck(&pool, "Scania", "R 730").await.unwrap();

        // Now delete the job row only.
        sqlx::query("DELETE FROM jobs WHERE id = 'job_1'")
            .execute(&pool)
            .await
            .unwrap();
        recompute_truck(&pool, "Scania", "R 730").await.unwrap();

        let (total, delivery, jobs): (f64, f64, i64) = sqlx::query_as(
            "SELECT total_distance_km, total_delivery_distance, total_jobs
               FROM trucks WHERE id = ?",
        )
        .bind(truck_id("Scania", "R 730"))
        .fetch_one(&pool)
        .await
        .unwrap();
        // The session row still exists, so delivery km is unchanged.
        assert_eq!(total, 200.0);
        assert_eq!(delivery, 200.0);
        assert_eq!(jobs, 0);
    }

    #[tokio::test]
    async fn deleting_session_drops_distance() {
        let pool = test_pool().await;
        insert_session(&pool, "ses_1", "JOB", "Scania", "R 730", 200.0).await;
        insert_job(&pool, "job_1", "ses_1", "Scania", "R 730").await;
        recompute_truck(&pool, "Scania", "R 730").await.unwrap();

        sqlx::query("DELETE FROM driving_sessions WHERE id = 'ses_1'")
            .execute(&pool)
            .await
            .unwrap();
        // FK is ON DELETE SET NULL, so the job row survives with session_id = NULL.
        recompute_truck(&pool, "Scania", "R 730").await.unwrap();

        let (total, delivery, jobs): (f64, f64, i64) = sqlx::query_as(
            "SELECT total_distance_km, total_delivery_distance, total_jobs
               FROM trucks WHERE id = ?",
        )
        .bind(truck_id("Scania", "R 730"))
        .fetch_one(&pool)
        .await
        .unwrap();
        // The orphaned job is still a telemetry job, so its own km don't count.
        assert_eq!(total, 0.0);
        assert_eq!(delivery, 0.0);
        // Job row is still there and matches truck make/model.
        assert_eq!(jobs, 1);
    }

    async fn truck_totals(pool: &SqlitePool, make: &str, model: &str) -> (f64, f64, f64, i64) {
        sqlx::query_as(
            "SELECT total_distance_km, total_delivery_distance, total_free_roam_distance, total_jobs
               FROM trucks WHERE id = ?",
        )
        .bind(truck_id(make, model))
        .fetch_one(pool)
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn manual_job_counts_as_delivery_distance() {
        let pool = test_pool().await;
        insert_manual_job(&pool, "job_1", "Scania", "S 730", 412.0).await;
        recompute_truck(&pool, "Scania", "S 730").await.unwrap();

        assert_eq!(
            truck_totals(&pool, "Scania", "S 730").await,
            (412.0, 412.0, 0.0, 1)
        );
    }

    #[tokio::test]
    async fn session_linked_job_is_not_double_counted() {
        let pool = test_pool().await;
        // The job row says 100 km, the session says 200: only the session counts.
        insert_session(&pool, "ses_1", "JOB", "Scania", "R 730", 200.0).await;
        insert_job(&pool, "job_1", "ses_1", "Scania", "R 730").await;
        recompute_truck(&pool, "Scania", "R 730").await.unwrap();

        assert_eq!(
            truck_totals(&pool, "Scania", "R 730").await,
            (200.0, 200.0, 0.0, 1)
        );
    }

    #[tokio::test]
    async fn mixed_sessions_and_manual_jobs_add_up() {
        let pool = test_pool().await;
        insert_session(&pool, "ses_1", "JOB", "Scania", "R 730", 200.0).await;
        insert_job(&pool, "job_1", "ses_1", "Scania", "R 730").await;
        insert_session(&pool, "ses_2", "FREE_ROAM", "Scania", "R 730", 30.0).await;
        insert_manual_job(&pool, "job_2", "Scania", "R 730", 412.0).await;
        insert_manual_job(&pool, "job_3", "Scania", "R 730", 88.0).await;
        // Another truck's manual job stays on that truck.
        insert_manual_job(&pool, "job_4", "Volvo", "FH 540", 999.0).await;
        recompute_truck(&pool, "Scania", "R 730").await.unwrap();

        assert_eq!(
            truck_totals(&pool, "Scania", "R 730").await,
            (730.0, 700.0, 30.0, 3)
        );
    }

    #[tokio::test]
    async fn two_trucks_track_independently() {
        let pool = test_pool().await;
        insert_session(&pool, "ses_1", "JOB", "Scania", "R 730", 100.0).await;
        insert_session(&pool, "ses_2", "FREE_ROAM", "Volvo", "FH 540", 25.0).await;
        recompute_truck(&pool, "Scania", "R 730").await.unwrap();
        recompute_truck(&pool, "Volvo", "FH 540").await.unwrap();

        let scania_total: (f64,) =
            sqlx::query_as("SELECT total_distance_km FROM trucks WHERE id = ?")
                .bind(truck_id("Scania", "R 730"))
                .fetch_one(&pool)
                .await
                .unwrap();
        let volvo_total: (f64,) =
            sqlx::query_as("SELECT total_distance_km FROM trucks WHERE id = ?")
                .bind(truck_id("Volvo", "FH 540"))
                .fetch_one(&pool)
                .await
                .unwrap();

        assert_eq!(scania_total.0, 100.0);
        assert_eq!(volvo_total.0, 25.0);
    }
}

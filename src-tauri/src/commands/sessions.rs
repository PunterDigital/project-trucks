//! Commands and helpers for `driving_sessions`.
//!
//! The helpers (insert_session, etc.) are exposed at module level so the
//! telemetry pipeline in M2 can call them directly without going through the
//! Tauri command wrapper. That keeps the manual-entry path and the auto-
//! capture path on a single write code path.

#![allow(dead_code)]

use anyhow::{Context, Result};
use sqlx::{QueryBuilder, Sqlite, SqliteConnection, SqlitePool};
use tauri::State;
use uuid::Uuid;

use crate::db::models::{DrivingSession, NewSession, SessionFilter, SessionsPage};
use crate::db::rollups::recompute_truck;
use crate::db::AppState;

const DEFAULT_LIMIT: i64 = 25;
const MAX_LIMIT: i64 = 200;

/// Insert a driving session and refresh the truck rollup. Returns the inserted
/// row. The id is generated server-side as `ses_<uuid-simple>`.
pub async fn insert_session(pool: &SqlitePool, input: NewSession) -> Result<DrivingSession> {
    let session = {
        // Release the connection before the rollup asks for one (small pools would deadlock).
        let mut conn = pool.acquire().await.context("acquiring connection")?;
        insert_session_row(&mut conn, input).await?
    };
    if let (Some(make), Some(model)) = (&session.truck_make, &session.truck_model) {
        recompute_truck(pool, make, model).await?;
    }
    Ok(session)
}

/// Insert the session row only, on a caller-supplied connection (or transaction), so the telemetry
/// writer can add a session and its job atomically. The caller refreshes the truck rollup after
/// committing.
pub async fn insert_session_row(
    conn: &mut SqliteConnection,
    input: NewSession,
) -> Result<DrivingSession> {
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
    .execute(&mut *conn)
    .await
    .context("inserting driving_session")?;

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

/// Read a paginated, filtered slice of `driving_sessions`. Ordered by
/// `ended_at DESC` (newest first).
pub async fn fetch_sessions(pool: &SqlitePool, filter: SessionFilter) -> Result<SessionsPage> {
    let limit = filter.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let offset = filter.offset.unwrap_or(0).max(0);

    // Items query.
    let mut qb: QueryBuilder<Sqlite> =
        QueryBuilder::new("SELECT * FROM driving_sessions WHERE 1=1");
    apply_session_filters(&mut qb, &filter);
    qb.push(" ORDER BY ended_at DESC LIMIT ");
    qb.push_bind(limit);
    qb.push(" OFFSET ");
    qb.push_bind(offset);
    let items: Vec<DrivingSession> = qb
        .build_query_as::<DrivingSession>()
        .fetch_all(pool)
        .await
        .context("listing driving_sessions")?;

    // Count query.
    let mut cb: QueryBuilder<Sqlite> =
        QueryBuilder::new("SELECT COUNT(*) FROM driving_sessions WHERE 1=1");
    apply_session_filters(&mut cb, &filter);
    let total: (i64,) = cb
        .build_query_as::<(i64,)>()
        .fetch_one(pool)
        .await
        .context("counting driving_sessions")?;

    Ok(SessionsPage {
        items,
        total: total.0,
    })
}

fn apply_session_filters(qb: &mut QueryBuilder<Sqlite>, filter: &SessionFilter) {
    if let Some(kind) = filter.kind {
        qb.push(" AND kind = ");
        qb.push_bind(kind);
    }
    if let Some(make) = &filter.truck_make {
        qb.push(" AND truck_make = ");
        qb.push_bind(make.clone());
    }
    if let Some(model) = &filter.truck_model {
        qb.push(" AND truck_model = ");
        qb.push_bind(model.clone());
    }
    if let Some(from) = filter.from {
        qb.push(" AND ended_at >= ");
        qb.push_bind(from);
    }
    if let Some(to) = filter.to {
        qb.push(" AND ended_at < ");
        qb.push_bind(to);
    }
}

/// Delete a session by id. Any linked jobs survive (their `session_id`
/// becomes NULL via the FK). Truck rollup is refreshed afterwards.
pub async fn remove_session(pool: &SqlitePool, id: &str) -> Result<()> {
    let row: Option<(Option<String>, Option<String>)> =
        sqlx::query_as("SELECT truck_make, truck_model FROM driving_sessions WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await
            .context("loading session for delete")?;

    sqlx::query("DELETE FROM driving_sessions WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await
        .context("deleting driving_session")?;

    if let Some((Some(make), Some(model))) = row {
        recompute_truck(pool, &make, &model).await?;
    }
    Ok(())
}

// --- Tauri command wrappers ---

#[tauri::command]
pub async fn create_session(
    state: State<'_, AppState>,
    input: NewSession,
) -> Result<DrivingSession, String> {
    insert_session(&state.pool, input)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_sessions(
    state: State<'_, AppState>,
    filter: Option<SessionFilter>,
) -> Result<SessionsPage, String> {
    fetch_sessions(&state.pool, filter.unwrap_or_default())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_session(state: State<'_, AppState>, id: String) -> Result<(), String> {
    remove_session(&state.pool, &id)
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
        let (delivery,): (f64,) =
            sqlx::query_as("SELECT total_delivery_distance FROM trucks WHERE id = ?")
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

    #[tokio::test]
    async fn list_sessions_paginates_and_orders_newest_first() {
        let pool = test_pool().await;
        // Three sessions on staggered hours.
        for hour in [9u32, 10, 11] {
            let mut input = sample_input(SessionKind::FreeRoam, 10.0);
            input.started_at = Utc.with_ymd_and_hms(2026, 5, 1, hour, 0, 0).unwrap();
            input.ended_at = Utc.with_ymd_and_hms(2026, 5, 1, hour, 30, 0).unwrap();
            insert_session(&pool, input).await.unwrap();
        }

        let page = fetch_sessions(
            &pool,
            SessionFilter {
                limit: Some(2),
                offset: Some(0),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(page.total, 3);
        assert_eq!(page.items.len(), 2);
        // Newest first: 11:30 then 10:30.
        assert_eq!(
            page.items[0].ended_at,
            Utc.with_ymd_and_hms(2026, 5, 1, 11, 30, 0).unwrap()
        );
        assert_eq!(
            page.items[1].ended_at,
            Utc.with_ymd_and_hms(2026, 5, 1, 10, 30, 0).unwrap()
        );

        let page2 = fetch_sessions(
            &pool,
            SessionFilter {
                limit: Some(2),
                offset: Some(2),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(page2.items.len(), 1);
        assert_eq!(
            page2.items[0].ended_at,
            Utc.with_ymd_and_hms(2026, 5, 1, 9, 30, 0).unwrap()
        );
    }

    #[tokio::test]
    async fn list_sessions_filters_by_kind() {
        let pool = test_pool().await;
        insert_session(&pool, sample_input(SessionKind::FreeRoam, 10.0))
            .await
            .unwrap();
        insert_session(&pool, sample_input(SessionKind::Job, 200.0))
            .await
            .unwrap();
        insert_session(&pool, sample_input(SessionKind::Job, 50.0))
            .await
            .unwrap();

        let page = fetch_sessions(
            &pool,
            SessionFilter {
                kind: Some(SessionKind::Job),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(page.total, 2);
        assert!(page.items.iter().all(|s| s.kind == SessionKind::Job));
    }

    #[tokio::test]
    async fn list_sessions_filters_by_truck() {
        let pool = test_pool().await;
        insert_session(&pool, sample_input(SessionKind::FreeRoam, 10.0))
            .await
            .unwrap();
        let mut volvo = sample_input(SessionKind::Job, 200.0);
        volvo.truck_make = Some("Volvo".into());
        volvo.truck_model = Some("FH16".into());
        insert_session(&pool, volvo).await.unwrap();
        // Same make, another model: must not match.
        let mut other_scania = sample_input(SessionKind::Job, 50.0);
        other_scania.truck_model = Some("S 730".into());
        insert_session(&pool, other_scania).await.unwrap();

        let page = fetch_sessions(
            &pool,
            SessionFilter {
                truck_make: Some("Scania".into()),
                truck_model: Some("R 730".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.items[0].distance_km, 10.0);
    }

    #[tokio::test]
    async fn deleting_session_drops_distance_keeps_jobs() {
        let pool = test_pool().await;
        let session = insert_session(&pool, sample_input(SessionKind::Job, 200.0))
            .await
            .unwrap();

        // Insert a job linked to this session via raw SQL so this test stays
        // independent of insert_job.
        sqlx::query(
            "INSERT INTO jobs
               (id, session_id, truck_make, truck_model,
                source_city, source_company, dest_city, dest_company,
                cargo_id, cargo_name, cargo_mass_kg, declared_income,
                distance_km, duration_sec, avg_speed_kmh,
                started_at, completed_at, status, source)
             VALUES ('job_1', ?, 'Scania', 'R 730',
                     'A', 'a', 'B', 'b', 'c', 'C', 1.0, 1, 200.0, 3600, 200.0,
                     '2026-05-01T10:00:00Z', '2026-05-01T11:00:00Z',
                     'DELIVERED', 'TELEMETRY')",
        )
        .bind(&session.id)
        .execute(&pool)
        .await
        .unwrap();
        recompute_truck(&pool, "Scania", "R 730").await.unwrap();

        remove_session(&pool, &session.id).await.unwrap();

        // Session is gone; job survives with session_id=NULL via ON DELETE SET NULL.
        let session_count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM driving_sessions WHERE id = ?")
                .bind(&session.id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(session_count.0, 0);
        let (orphaned_session_id,): (Option<String>,) =
            sqlx::query_as("SELECT session_id FROM jobs WHERE id = 'job_1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(orphaned_session_id.is_none());

        // Truck delivery km is now 0 (session gone); total_jobs still 1.
        let (total, delivery, jobs): (f64, f64, i64) = sqlx::query_as(
            "SELECT total_distance_km, total_delivery_distance, total_jobs
               FROM trucks WHERE id = ?",
        )
        .bind(truck_id("Scania", "R 730"))
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(total, 0.0);
        assert_eq!(delivery, 0.0);
        assert_eq!(jobs, 1);
    }

    #[tokio::test]
    async fn deleting_unknown_session_is_noop() {
        let pool = test_pool().await;
        remove_session(&pool, "ses_nope").await.unwrap();
    }
}

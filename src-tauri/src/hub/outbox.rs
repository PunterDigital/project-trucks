//! The `hub_outbox` / `hub_jobs` tables (migration 0002): what still has to reach the Hub and
//! what it already has. The triggers in the migration fill the outbox; this file reads it and
//! records the results.

use serde::Serialize;
use sqlx::SqlitePool;

use crate::commands::jobs::fetch_job;
use crate::db::models::Job;

/// A row that failed this many times (the Hub refused it) is left alone and shown as failed.
pub const MAX_ATTEMPTS: i64 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Counts {
    /// Waiting to be sent.
    pub pending: i64,
    /// Refused by the Hub too often; not retried.
    pub failed: i64,
    /// On the Hub.
    pub synced: i64,
    /// Recorded jobs neither on the Hub nor queued (from before linking): "Sync history".
    /// Manual jobs never sync (#4751), so they never count here.
    pub history: i64,
}

pub async fn counts(pool: &SqlitePool) -> Result<Counts, sqlx::Error> {
    let (pending, failed, synced, history): (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT
           (SELECT COUNT(*) FROM hub_outbox WHERE attempts < ?1),
           (SELECT COUNT(*) FROM hub_outbox WHERE attempts >= ?1),
           (SELECT COUNT(*) FROM hub_jobs),
           (SELECT COUNT(*) FROM jobs
              WHERE source <> 'MANUAL'
                AND id NOT IN (SELECT job_id FROM hub_jobs)
                AND id NOT IN (SELECT job_id FROM hub_outbox))",
    )
    .bind(MAX_ATTEMPTS)
    .fetch_one(pool)
    .await?;
    Ok(Counts {
        pending,
        failed,
        synced,
        history,
    })
}

/// One queued row, with its `queued_at` so a result only clears the row it was sent for (an edit
/// made while the request was in flight re-queues the job with a newer stamp).
#[derive(Debug, Clone, PartialEq)]
pub struct Queued {
    pub job_id: String,
    pub queued_at: String,
}

async fn next(pool: &SqlitePool, op: &str, limit: i64) -> Result<Vec<Queued>, sqlx::Error> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT job_id, queued_at FROM hub_outbox
         WHERE op = ? AND attempts < ?
         ORDER BY queued_at LIMIT ?",
    )
    .bind(op)
    .bind(MAX_ATTEMPTS)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(job_id, queued_at)| Queued { job_id, queued_at })
        .collect())
}

pub async fn next_deletes(pool: &SqlitePool, limit: i64) -> Result<Vec<Queued>, sqlx::Error> {
    next(pool, "delete", limit).await
}

/// Up to `limit` jobs to send. Rows whose job has gone (deleted between queueing and now) are
/// dropped here.
pub async fn next_upserts(pool: &SqlitePool, limit: i64) -> anyhow::Result<Vec<(Queued, Job)>> {
    let mut out = Vec::new();
    for queued in next(pool, "upsert", limit).await? {
        match fetch_job(pool, &queued.job_id).await? {
            Some(job) => out.push((queued, job)),
            None => {
                sqlx::query("DELETE FROM hub_outbox WHERE job_id = ? AND queued_at = ?")
                    .bind(&queued.job_id)
                    .bind(&queued.queued_at)
                    .execute(pool)
                    .await?;
            }
        }
    }
    Ok(out)
}

/// The Hub has the job. `hidden` is whether the driver hid it there (`None` from Hubs before
/// #4654, which keeps what was stored).
pub async fn mark_synced(
    pool: &SqlitePool,
    queued: &Queued,
    url: Option<&str>,
    hidden: Option<bool>,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query(
        "INSERT INTO hub_jobs (job_id, synced_at, hub_url, hidden)
         VALUES (?1, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), ?2, COALESCE(?3, 0))
         ON CONFLICT(job_id) DO UPDATE SET
           synced_at = excluded.synced_at,
           hub_url = COALESCE(excluded.hub_url, hub_jobs.hub_url),
           hidden = COALESCE(?3, hub_jobs.hidden)",
    )
    .bind(&queued.job_id)
    .bind(url)
    .bind(hidden)
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM hub_outbox WHERE job_id = ? AND queued_at = ?")
        .bind(&queued.job_id)
        .bind(&queued.queued_at)
        .execute(&mut *tx)
        .await?;
    tx.commit().await
}

pub async fn mark_deleted(pool: &SqlitePool, queued: &Queued) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM hub_jobs WHERE job_id = ?")
        .bind(&queued.job_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM hub_outbox WHERE job_id = ? AND queued_at = ?")
        .bind(&queued.job_id)
        .bind(&queued.queued_at)
        .execute(&mut *tx)
        .await?;
    tx.commit().await
}

/// The Hub refused this row: count the attempt and keep the reason for the Settings page.
pub async fn mark_failed(
    pool: &SqlitePool,
    queued: &Queued,
    error: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE hub_outbox SET attempts = attempts + 1, last_error = ? WHERE job_id = ? AND queued_at = ?")
        .bind(error)
        .bind(&queued.job_id)
        .bind(&queued.queued_at)
        .execute(pool)
        .await
        .map(|_| ())
}

/// The first failed row's reason, if any.
pub async fn first_failure(pool: &SqlitePool) -> Result<Option<String>, sqlx::Error> {
    let row: Option<(Option<String>,)> = sqlx::query_as(
        "SELECT last_error FROM hub_outbox WHERE attempts >= ? ORDER BY queued_at LIMIT 1",
    )
    .bind(MAX_ATTEMPTS)
    .fetch_optional(pool)
    .await?;
    Ok(row.and_then(|r| r.0))
}

/// "Sync history": queue every recorded job the Hub doesn't have yet (manual jobs stay on this
/// PC). Returns how many were queued.
pub async fn queue_history(pool: &SqlitePool) -> Result<u64, sqlx::Error> {
    let done = sqlx::query(
        "INSERT OR IGNORE INTO hub_outbox (job_id, op)
         SELECT id, 'upsert' FROM jobs
         WHERE source <> 'MANUAL' AND id NOT IN (SELECT job_id FROM hub_jobs)",
    )
    .execute(pool)
    .await?;
    Ok(done.rows_affected())
}

/// Failed rows get another five tries ("Retry" on the Settings page).
pub async fn retry_failed(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE hub_outbox SET attempts = 0, last_error = NULL WHERE attempts >= ?")
        .bind(MAX_ATTEMPTS)
        .execute(pool)
        .await
        .map(|_| ())
}

/// Where one job stands with the Hub, for the Jobs list and the job page (#4654).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobHubState {
    pub job_id: String,
    pub state: SyncState,
    /// The job's page on the Hub, once synced.
    pub url: Option<String>,
    /// Hidden on the Hub (only the driver sees it there).
    pub hidden: bool,
    /// Why the Hub refused it (`Refused` only).
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SyncState {
    /// On the Hub, nothing waiting.
    Synced,
    /// Queued: new, edited or being deleted.
    Pending,
    /// Refused [`MAX_ATTEMPTS`] times; waits for "Retry".
    Refused,
    /// Typed in by hand: never leaves this PC (#4751).
    Local,
    /// Recorded before linking: only "Sync history" sends it.
    NotSynced,
}

fn sync_state(
    source: &str,
    synced: bool,
    queued_attempts: Option<i64>,
    queued_op: Option<&str>,
) -> SyncState {
    match (queued_attempts, queued_op) {
        (Some(n), Some("upsert")) if n >= MAX_ATTEMPTS => SyncState::Refused,
        (Some(_), _) => SyncState::Pending,
        _ if synced => SyncState::Synced,
        _ if source == "MANUAL" => SyncState::Local,
        _ => SyncState::NotSynced,
    }
}

/// The Hub state of each of `ids` that exists, in no particular order.
pub async fn job_states(
    pool: &SqlitePool,
    ids: &[String],
) -> Result<Vec<JobHubState>, sqlx::Error> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let ids_json = serde_json::to_string(ids).unwrap_or_else(|_| "[]".into());
    type Row = (
        String,
        String,
        Option<String>,
        Option<String>,
        Option<bool>,
        Option<i64>,
        Option<String>,
        Option<String>,
    );
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT j.id, j.source, h.synced_at, h.hub_url, h.hidden, o.attempts, o.op, o.last_error
         FROM jobs j
         LEFT JOIN hub_jobs h ON h.job_id = j.id
         LEFT JOIN hub_outbox o ON o.job_id = j.id
         WHERE j.id IN (SELECT value FROM json_each(?))",
    )
    .bind(ids_json)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(
            |(id, source, synced_at, url, hidden, attempts, op, error)| {
                let state = sync_state(&source, synced_at.is_some(), attempts, op.as_deref());
                JobHubState {
                    job_id: id,
                    state,
                    url,
                    hidden: hidden.unwrap_or(false),
                    error: if state == SyncState::Refused {
                        error
                    } else {
                        None
                    },
                }
            },
        )
        .collect())
}

/// The driver hid or showed the job on the Hub from the app.
pub async fn set_hidden(pool: &SqlitePool, job_id: &str, hidden: bool) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE hub_jobs SET hidden = ? WHERE job_id = ?")
        .bind(hidden)
        .bind(job_id)
        .execute(pool)
        .await
        .map(|_| ())
}

/// Linking or unlinking: forget what was queued for, and synced to, the previous account.
pub async fn clear(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM hub_outbox")
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM hub_jobs")
        .execute(&mut *tx)
        .await?;
    tx.commit().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::jobs::{insert_job, patch_job, remove_job};
    use crate::db::models::{JobPatch, JobSource, JobStatus, NewJob};
    use chrono::{TimeZone, Utc};
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use std::str::FromStr;

    async fn pool() -> SqlitePool {
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

    async fn set_linked(pool: &SqlitePool, on: bool) {
        crate::settings::put(pool, "hub_linked", if on { "true" } else { "false" })
            .await
            .unwrap();
    }

    fn new_job() -> NewJob {
        NewJob {
            session_id: None,
            truck_make: Some("Scania".into()),
            truck_model: Some("S".into()),
            source_city: "Calais".into(),
            source_company: "Tradeaux".into(),
            dest_city: "Paris".into(),
            dest_company: "Euroacres".into(),
            cargo_id: "apples".into(),
            cargo_name: "Apples".into(),
            cargo_mass_kg: 18_000.0,
            declared_income: 12_000,
            distance_km: 290.0,
            duration_sec: 14_400,
            game_duration_min: None,
            fuel_used_l: None,
            damage_delta: None,
            started_at: Utc.with_ymd_and_hms(2026, 11, 14, 10, 0, 0).unwrap(),
            completed_at: Utc.with_ymd_and_hms(2026, 11, 14, 14, 0, 0).unwrap(),
            status: JobStatus::Delivered,
            source: JobSource::Telemetry,
            notes: None,
        }
    }

    fn manual_job() -> NewJob {
        NewJob {
            source: JobSource::Manual,
            dest_city: "Lyon".into(),
            ..new_job()
        }
    }

    async fn queued(pool: &SqlitePool) -> Vec<(String, String)> {
        sqlx::query_as("SELECT job_id, op FROM hub_outbox ORDER BY queued_at")
            .fetch_all(pool)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn the_games_figures_requeue_a_synced_job_and_its_costs_add_up() {
        use crate::commands::expenses::{job_costs, set_game_result};
        use crate::db::models::GameResult;

        let pool = pool().await;
        set_linked(&pool, true).await;
        let job = insert_job(&pool, new_job()).await.unwrap();
        // No figures and no costs recorded (old plugin): nothing to say about costs.
        assert_eq!(job_costs(&pool, &job).await.unwrap(), None);

        let batch = next_upserts(&pool, 100).await.unwrap();
        mark_synced(&pool, &batch[0].0, Some("https://hub/jobs/1"), None)
            .await
            .unwrap();
        assert!(queued(&pool).await.is_empty());

        // #5227: the Hub gets the game's figures too, so a change to them goes up again.
        let game = GameResult {
            game_revenue: Some(4_478),
            earned_xp: Some(320),
            ..Default::default()
        };
        let mut conn = pool.acquire().await.unwrap();
        set_game_result(&mut conn, &job.id, &game).await.unwrap();
        drop(conn);
        assert_eq!(queued(&pool).await, vec![(job.id.clone(), "upsert".into())]);

        let with_game = fetch_job(&pool, &job.id).await.unwrap().unwrap();
        // A scs-telemetry job that paid nothing on the way: 0, not unknown.
        assert_eq!(job_costs(&pool, &with_game).await.unwrap(), Some(0));
        for (id, job_id, amount) in [
            ("e1", Some(job.id.as_str()), 19),
            ("e2", Some(job.id.as_str()), 300),
            ("e3", None, 7),
        ] {
            sqlx::query(
                "INSERT INTO expenses (id, job_id, kind, amount, occurred_at)
                 VALUES (?, ?, 'TOLL', ?, '2026-11-14T12:00:00Z')",
            )
            .bind(id)
            .bind(job_id)
            .bind(amount)
            .execute(&pool)
            .await
            .unwrap();
        }
        // The free-roam toll (no job) isn't this job's.
        assert_eq!(job_costs(&pool, &with_game).await.unwrap(), Some(319));
    }

    #[tokio::test]
    async fn nothing_is_queued_while_unlinked() {
        let pool = pool().await;
        insert_job(&pool, new_job()).await.unwrap();
        assert!(queued(&pool).await.is_empty());
        assert_eq!(counts(&pool).await.unwrap().history, 1);
    }

    #[tokio::test]
    async fn linked_jobs_follow_inserts_edits_and_deletes() {
        let pool = pool().await;
        set_linked(&pool, true).await;

        let job = insert_job(&pool, new_job()).await.unwrap();
        assert_eq!(queued(&pool).await, vec![(job.id.clone(), "upsert".into())]);

        let batch = next_upserts(&pool, 100).await.unwrap();
        assert_eq!(batch.len(), 1);
        mark_synced(&pool, &batch[0].0, Some("https://hub/jobs/1"), None)
            .await
            .unwrap();
        assert!(queued(&pool).await.is_empty());
        assert_eq!(counts(&pool).await.unwrap().synced, 1);

        // Notes never leave the PC, so editing them queues nothing.
        let notes = JobPatch {
            notes: Some("private".into()),
            flagged: None,
            flag_reason: None,
        };
        patch_job(&pool, &job.id, notes).await.unwrap();
        assert!(queued(&pool).await.is_empty());

        let flag = JobPatch {
            notes: None,
            flagged: Some(true),
            flag_reason: Some("odd".into()),
        };
        patch_job(&pool, &job.id, flag).await.unwrap();
        assert_eq!(queued(&pool).await, vec![(job.id.clone(), "upsert".into())]);

        remove_job(&pool, &job.id).await.unwrap();
        assert_eq!(queued(&pool).await, vec![(job.id.clone(), "delete".into())]);
        let deletes = next_deletes(&pool, 100).await.unwrap();
        mark_deleted(&pool, &deletes[0]).await.unwrap();
        assert!(queued(&pool).await.is_empty());
        assert_eq!(counts(&pool).await.unwrap().synced, 0);
    }

    #[tokio::test]
    async fn a_job_deleted_before_it_was_sent_just_leaves_the_queue() {
        let pool = pool().await;
        set_linked(&pool, true).await;
        let job = insert_job(&pool, new_job()).await.unwrap();
        remove_job(&pool, &job.id).await.unwrap();
        assert!(queued(&pool).await.is_empty());
    }

    #[tokio::test]
    async fn manual_jobs_never_sync() {
        let pool = pool().await;
        let old = insert_job(&pool, manual_job()).await.unwrap();
        assert_eq!(counts(&pool).await.unwrap().history, 0);
        set_linked(&pool, true).await;

        let fresh = insert_job(&pool, manual_job()).await.unwrap();
        assert!(queued(&pool).await.is_empty());
        let flag = JobPatch {
            notes: None,
            flagged: Some(true),
            flag_reason: None,
        };
        patch_job(&pool, &fresh.id, flag).await.unwrap();
        assert!(queued(&pool).await.is_empty());

        assert_eq!(queue_history(&pool).await.unwrap(), 0);
        assert!(queued(&pool).await.is_empty());
        assert_eq!(counts(&pool).await.unwrap().history, 0);
        remove_job(&pool, &old.id).await.unwrap();
        assert!(queued(&pool).await.is_empty());
    }

    /// Migration 0003 on a database that synced manual jobs under 0002.
    #[tokio::test]
    async fn the_migration_takes_manual_jobs_back_off_the_hub() {
        let pool = pool().await;
        set_linked(&pool, true).await;
        let recorded = insert_job(&pool, new_job()).await.unwrap();
        let on_hub = insert_job(&pool, manual_job()).await.unwrap();
        let waiting = insert_job(&pool, manual_job()).await.unwrap();
        sqlx::query(
            "INSERT INTO hub_jobs (job_id, synced_at) VALUES (?, 'x'), (?, 'x');
             INSERT INTO hub_outbox (job_id, op) VALUES (?, 'upsert');",
        )
        .bind(&recorded.id)
        .bind(&on_hub.id)
        .bind(&waiting.id)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("DELETE FROM hub_outbox WHERE job_id = ?")
            .bind(&recorded.id)
            .execute(&pool)
            .await
            .unwrap();

        sqlx::raw_sql(include_str!("../../migrations/0003_hub_skip_manual.sql"))
            .execute(&pool)
            .await
            .unwrap();

        assert_eq!(
            queued(&pool).await,
            vec![(on_hub.id.clone(), "delete".into())]
        );
        assert_eq!(counts(&pool).await.unwrap().synced, 2);
        let deletes = next_deletes(&pool, 100).await.unwrap();
        mark_deleted(&pool, &deletes[0]).await.unwrap();
        let synced: Vec<(String,)> = sqlx::query_as("SELECT job_id FROM hub_jobs")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(synced, vec![(recorded.id.clone(),)]);
    }

    #[tokio::test]
    async fn old_jobs_wait_for_sync_history() {
        let pool = pool().await;
        let old = insert_job(&pool, new_job()).await.unwrap();
        set_linked(&pool, true).await;

        // Editing an old, never-synced job doesn't upload it.
        let flag = JobPatch {
            notes: None,
            flagged: Some(true),
            flag_reason: None,
        };
        patch_job(&pool, &old.id, flag).await.unwrap();
        assert!(queued(&pool).await.is_empty());

        assert_eq!(queue_history(&pool).await.unwrap(), 1);
        assert_eq!(queued(&pool).await, vec![(old.id.clone(), "upsert".into())]);
        // Twice is harmless.
        assert_eq!(queue_history(&pool).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn refused_rows_stop_after_max_attempts_and_can_be_retried() {
        let pool = pool().await;
        set_linked(&pool, true).await;
        insert_job(&pool, new_job()).await.unwrap();
        let row = next_upserts(&pool, 100).await.unwrap().remove(0).0;
        for _ in 0..MAX_ATTEMPTS {
            mark_failed(&pool, &row, "The status field is invalid.")
                .await
                .unwrap();
        }
        assert!(next_upserts(&pool, 100).await.unwrap().is_empty());
        let c = counts(&pool).await.unwrap();
        assert_eq!((c.pending, c.failed), (0, 1));
        assert_eq!(
            first_failure(&pool).await.unwrap().as_deref(),
            Some("The status field is invalid.")
        );

        retry_failed(&pool).await.unwrap();
        assert_eq!(next_upserts(&pool, 100).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn an_edit_during_a_send_stays_queued() {
        let pool = pool().await;
        set_linked(&pool, true).await;
        let job = insert_job(&pool, new_job()).await.unwrap();
        let sent = next_upserts(&pool, 100).await.unwrap().remove(0).0;
        // The job changes while the request is in flight: a newer queue stamp.
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        let flag = JobPatch {
            notes: None,
            flagged: Some(true),
            flag_reason: None,
        };
        patch_job(&pool, &job.id, flag).await.unwrap();
        mark_synced(&pool, &sent, None, None).await.unwrap();
        assert_eq!(queued(&pool).await, vec![(job.id, "upsert".into())]);
    }

    async fn state_of(pool: &SqlitePool, id: &str) -> JobHubState {
        job_states(pool, &[id.to_string()])
            .await
            .unwrap()
            .pop()
            .unwrap()
    }

    #[tokio::test]
    async fn each_job_reports_where_it_stands_with_the_hub() {
        let pool = pool().await;
        let old = insert_job(&pool, new_job()).await.unwrap();
        let manual = insert_job(&pool, manual_job()).await.unwrap();
        set_linked(&pool, true).await;
        let fresh = insert_job(&pool, new_job()).await.unwrap();

        assert_eq!(state_of(&pool, &old.id).await.state, SyncState::NotSynced);
        assert_eq!(state_of(&pool, &manual.id).await.state, SyncState::Local);
        assert_eq!(state_of(&pool, &fresh.id).await.state, SyncState::Pending);
        assert!(job_states(&pool, &[]).await.unwrap().is_empty());
        assert!(job_states(&pool, &["nope".into()])
            .await
            .unwrap()
            .is_empty());

        let row = next_upserts(&pool, 100).await.unwrap().remove(0).0;
        mark_synced(&pool, &row, Some("https://hub/jobs/7"), Some(true))
            .await
            .unwrap();
        let synced = state_of(&pool, &fresh.id).await;
        assert_eq!(synced.state, SyncState::Synced);
        assert_eq!(synced.url.as_deref(), Some("https://hub/jobs/7"));
        assert!(synced.hidden);

        // An edit queues it again. An older Hub says nothing about `hidden`: what was stored stays.
        let flag = |on: bool| JobPatch {
            notes: None,
            flagged: Some(on),
            flag_reason: None,
        };
        patch_job(&pool, &fresh.id, flag(true)).await.unwrap();
        assert_eq!(state_of(&pool, &fresh.id).await.state, SyncState::Pending);
        let row = next_upserts(&pool, 100).await.unwrap().remove(0).0;
        mark_synced(&pool, &row, None, None).await.unwrap();
        assert!(state_of(&pool, &fresh.id).await.hidden);
        set_hidden(&pool, &fresh.id, false).await.unwrap();
        assert!(!state_of(&pool, &fresh.id).await.hidden);

        // Refusals show with their reason.
        patch_job(&pool, &fresh.id, flag(false)).await.unwrap();
        let row = next_upserts(&pool, 100).await.unwrap().remove(0).0;
        for _ in 0..MAX_ATTEMPTS {
            mark_failed(&pool, &row, "Too fast.").await.unwrap();
        }
        let refused = state_of(&pool, &fresh.id).await;
        assert_eq!(refused.state, SyncState::Refused);
        assert_eq!(refused.error.as_deref(), Some("Too fast."));
        assert_eq!(refused.url.as_deref(), Some("https://hub/jobs/7"));
    }
}

//! The `hub_outbox` / `hub_jobs` tables (migration 0002): what still has to reach the Hub and
//! what it already has. The triggers in the migration fill the outbox; this file reads it and
//! records the results.

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

pub async fn mark_synced(
    pool: &SqlitePool,
    queued: &Queued,
    url: Option<&str>,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query(
        "INSERT INTO hub_jobs (job_id, synced_at, hub_url) VALUES (?, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), ?)
         ON CONFLICT(job_id) DO UPDATE SET synced_at = excluded.synced_at, hub_url = COALESCE(excluded.hub_url, hub_jobs.hub_url)",
    )
    .bind(&queued.job_id)
    .bind(url)
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
        mark_synced(&pool, &batch[0].0, Some("https://hub/jobs/1"))
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
        mark_synced(&pool, &sent, None).await.unwrap();
        assert_eq!(queued(&pool).await, vec![(job.id, "upsert".into())]);
    }
}

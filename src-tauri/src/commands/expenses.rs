//! Tolls, fines and ferry/train fares the game charged (#4756), and the game's own figures for a
//! recorded job. Written by the telemetry pipeline only; the job page reads them.

use anyhow::{Context, Result};
use sqlx::{SqliteConnection, SqlitePool};
use tauri::State;
use uuid::Uuid;

use crate::db::models::{GameResult, Job, JobExpense};
use crate::db::AppState;
use crate::telemetry::Expense;

/// Insert one cost, linked to `job_id` when it was paid on a job.
pub async fn insert_expense_row(
    conn: &mut SqliteConnection,
    job_id: Option<&str>,
    expense: &Expense,
) -> Result<JobExpense> {
    let row = JobExpense {
        id: format!("exp_{}", Uuid::new_v4().simple()),
        job_id: job_id.map(str::to_owned),
        kind: expense.kind,
        amount: expense.amount,
        detail: expense.detail.clone(),
        truck_make: expense.truck_make.clone(),
        truck_model: expense.truck_model.clone(),
        occurred_at: expense.at.and_utc(),
    };
    sqlx::query(
        "INSERT INTO expenses
           (id, job_id, kind, amount, detail, truck_make, truck_model, occurred_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&row.id)
    .bind(&row.job_id)
    .bind(row.kind)
    .bind(row.amount)
    .bind(&row.detail)
    .bind(&row.truck_make)
    .bind(&row.truck_model)
    .bind(row.occurred_at)
    .execute(&mut *conn)
    .await
    .context("inserting expense")?;
    Ok(row)
}

/// Store what the game reported for a job when it ended.
pub async fn set_game_result(
    conn: &mut SqliteConnection,
    job_id: &str,
    game: &GameResult,
) -> Result<()> {
    sqlx::query(
        "UPDATE jobs SET game_revenue = ?, earned_xp = ?, cargo_damage = ?, game_distance_km = ?,
                         auto_park = ?, auto_load = ?, cancel_penalty = ?
         WHERE id = ?",
    )
    .bind(game.game_revenue)
    .bind(game.earned_xp)
    .bind(game.cargo_damage)
    .bind(game.game_distance_km)
    .bind(game.auto_park)
    .bind(game.auto_load)
    .bind(game.cancel_penalty)
    .bind(job_id)
    .execute(&mut *conn)
    .await
    .context("saving the game's figures for the job")?;
    Ok(())
}

/// The total the Hub gets as a job's `costs` (#5227): its tolls, fines and fares. `None` for a
/// job without the game's figures and no costs (old plugin, before 0.12), whose costs nobody
/// recorded; `Some(0)` for a scs-telemetry job that paid nothing.
pub async fn job_costs(pool: &SqlitePool, job: &Job) -> Result<Option<i64>> {
    let (count, total): (i64, i64) =
        sqlx::query_as("SELECT COUNT(*), COALESCE(SUM(amount), 0) FROM expenses WHERE job_id = ?")
            .bind(&job.id)
            .fetch_one(pool)
            .await
            .context("adding up the job's costs")?;
    if count == 0 && job.game == GameResult::default() {
        return Ok(None);
    }
    Ok(Some(total))
}

/// A job's costs, oldest first.
pub async fn fetch_job_expenses(pool: &SqlitePool, job_id: &str) -> Result<Vec<JobExpense>> {
    sqlx::query_as("SELECT * FROM expenses WHERE job_id = ? ORDER BY occurred_at, id")
        .bind(job_id)
        .fetch_all(pool)
        .await
        .context("loading the job's costs")
}

#[tauri::command]
pub async fn get_job_expenses(
    state: State<'_, AppState>,
    job_id: String,
) -> Result<Vec<JobExpense>, String> {
    fetch_job_expenses(&state.pool, &job_id)
        .await
        .map_err(|e| e.to_string())
}

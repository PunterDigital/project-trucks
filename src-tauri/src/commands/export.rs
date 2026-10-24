//! Data export and backup (M3.3).
//!
//! `export_jobs_csv` writes every job to a CSV file (RFC 4180: header row, CRLF,
//! fields quoted when they hold a comma, quote or line break). `backup_database`
//! snapshots the live database with `VACUUM INTO`, which produces a clean,
//! consistent copy without closing the pool. Both take the path the user picked
//! in the native save dialog; the frontend owns the dialog, these commands only
//! write.

use std::fmt::Write as _;
use std::path::Path;

use anyhow::{Context, Result};
use sqlx::SqlitePool;
use tauri::State;

use crate::db::models::Job;
use crate::db::AppState;

/// Column order of the CSV. Keep it stable: people build spreadsheets on it.
pub const CSV_COLUMNS: [&str; 24] = [
    "id",
    "source",
    "status",
    "completed_at",
    "started_at",
    "duration_sec",
    "source_city",
    "source_company",
    "dest_city",
    "dest_company",
    "cargo_id",
    "cargo_name",
    "cargo_mass_kg",
    "declared_income",
    "distance_km",
    "avg_speed_kmh",
    "fuel_used_l",
    "damage_delta",
    "truck_make",
    "truck_model",
    "session_id",
    "flagged",
    "flag_reason",
    "notes",
];

/// Quote a field when RFC 4180 requires it (comma, quote, CR or LF), doubling inner quotes.
pub fn csv_field(value: &str) -> String {
    if value.contains([',', '"', '\r', '\n']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}

fn opt<T: ToString>(v: &Option<T>) -> String {
    v.as_ref().map(ToString::to_string).unwrap_or_default()
}

/// One CSV line (with CRLF) for a job, in `CSV_COLUMNS` order.
fn csv_row(job: &Job) -> String {
    let fields: [String; 24] = [
        job.id.clone(),
        format!("{:?}", job.source).to_uppercase(),
        format!("{:?}", job.status).to_uppercase(),
        job.completed_at.to_rfc3339(),
        job.started_at.to_rfc3339(),
        job.duration_sec.to_string(),
        job.source_city.clone(),
        job.source_company.clone(),
        job.dest_city.clone(),
        job.dest_company.clone(),
        job.cargo_id.clone(),
        job.cargo_name.clone(),
        job.cargo_mass_kg.to_string(),
        job.declared_income.to_string(),
        job.distance_km.to_string(),
        job.avg_speed_kmh.to_string(),
        opt(&job.fuel_used_l),
        opt(&job.damage_delta),
        opt(&job.truck_make),
        opt(&job.truck_model),
        opt(&job.session_id),
        job.flagged.to_string(),
        opt(&job.flag_reason),
        opt(&job.notes),
    ];
    let mut line = String::new();
    for (i, field) in fields.iter().enumerate() {
        if i > 0 {
            line.push(',');
        }
        line.push_str(&csv_field(field));
    }
    line.push_str("\r\n");
    line
}

/// Every job as CSV text, newest first.
pub async fn jobs_csv(pool: &SqlitePool) -> Result<(String, usize)> {
    let jobs: Vec<Job> = sqlx::query_as("SELECT * FROM jobs ORDER BY completed_at DESC, id")
        .fetch_all(pool)
        .await
        .context("loading jobs for export")?;
    let mut out = String::new();
    writeln!(out, "{}\r", CSV_COLUMNS.join(",")).expect("writing to a String");
    for job in &jobs {
        out.push_str(&csv_row(job));
    }
    Ok((out, jobs.len()))
}

/// Write every job to `path` as CSV. Returns the number of jobs written (header excluded).
pub async fn write_jobs_csv(pool: &SqlitePool, path: &Path) -> Result<usize> {
    let (csv, count) = jobs_csv(pool).await?;
    std::fs::write(path, csv).with_context(|| format!("writing {}", path.display()))?;
    Ok(count)
}

/// Snapshot the database to `path` with `VACUUM INTO`. An existing file is replaced (the save
/// dialog has already asked about overwriting); `VACUUM INTO` itself refuses a non-empty target.
pub async fn backup_to(pool: &SqlitePool, path: &Path) -> Result<()> {
    if path.exists() {
        std::fs::remove_file(path)
            .with_context(|| format!("replacing the old backup {}", path.display()))?;
    }
    let target = path.to_string_lossy().into_owned();
    sqlx::query("VACUUM INTO ?")
        .bind(target)
        .execute(pool)
        .await
        .with_context(|| format!("backing up to {}", path.display()))?;
    Ok(())
}

// --- Tauri command wrappers ---

#[tauri::command]
pub async fn export_jobs_csv(state: State<'_, AppState>, path: String) -> Result<usize, String> {
    write_jobs_csv(&state.pool, Path::new(&path))
        .await
        .map_err(|e| format!("{e:#}"))
}

#[tauri::command]
pub async fn backup_database(state: State<'_, AppState>, path: String) -> Result<(), String> {
    backup_to(&state.pool, Path::new(&path))
        .await
        .map_err(|e| format!("{e:#}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::jobs::{fetch_jobs, insert_job};
    use crate::db::models::{JobFilter, JobSource, JobStatus, NewJob};
    use chrono::{TimeZone, Utc};
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use std::path::PathBuf;

    async fn open_pool(path: &Path) -> SqlitePool {
        let opts = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        pool
    }

    /// A fresh scratch folder per test, under the OS temp dir.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "projecttrucks-export-{name}-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn job(dest: &str, notes: Option<&str>) -> NewJob {
        NewJob {
            session_id: None,
            truck_make: Some("Scania".into()),
            truck_model: Some("R 730".into()),
            source_city: "Calais".into(),
            source_company: "Tradeaux".into(),
            dest_city: dest.into(),
            dest_company: "Kaarfor".into(),
            cargo_id: "apples".into(),
            cargo_name: "Apples".into(),
            cargo_mass_kg: 18_000.0,
            declared_income: 5_000,
            distance_km: 100.5,
            duration_sec: 3_600,
            fuel_used_l: None,
            damage_delta: Some(0.5),
            started_at: Utc.with_ymd_and_hms(2026, 5, 1, 10, 0, 0).unwrap(),
            completed_at: Utc.with_ymd_and_hms(2026, 5, 1, 11, 0, 0).unwrap(),
            status: JobStatus::Delivered,
            source: JobSource::Manual,
            notes: notes.map(Into::into),
        }
    }

    #[test]
    fn fields_are_quoted_only_when_needed() {
        assert_eq!(csv_field("plain"), "plain");
        assert_eq!(csv_field(""), "");
        assert_eq!(csv_field("a,b"), "\"a,b\"");
        assert_eq!(csv_field("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(csv_field("two\nlines"), "\"two\nlines\"");
    }

    #[tokio::test]
    async fn export_writes_a_header_and_one_quoted_row_per_job() {
        let dir = scratch("csv");
        let pool = open_pool(&dir.join("data.db")).await;
        insert_job(&pool, job("Rotterdam", Some("fragile, keep \"cool\"")))
            .await
            .unwrap();
        insert_job(&pool, job("Berlin", None)).await.unwrap();

        let out = dir.join("jobs.csv");
        let count = write_jobs_csv(&pool, &out).await.unwrap();
        assert_eq!(count, 2);

        let text = std::fs::read_to_string(&out).unwrap();
        let lines: Vec<&str> = text.split("\r\n").filter(|l| !l.is_empty()).collect();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], CSV_COLUMNS.join(","));
        assert!(lines[0].starts_with("id,source,status,completed_at,"));
        let rotterdam = lines.iter().find(|l| l.contains("Rotterdam")).unwrap();
        assert!(rotterdam.contains(",MANUAL,DELIVERED,2026-05-01T11:00:00+00:00,"));
        assert!(rotterdam.contains(",100.5,"));
        assert!(rotterdam.ends_with(",false,,\"fragile, keep \"\"cool\"\"\""));
        // Every data row has the same number of unquoted commas as the header: 23.
        let berlin = lines.iter().find(|l| l.contains("Berlin")).unwrap();
        assert_eq!(berlin.matches(',').count(), CSV_COLUMNS.len() - 1);

        let empty_pool = open_pool(&dir.join("empty.db")).await;
        let out = dir.join("empty.csv");
        assert_eq!(write_jobs_csv(&empty_pool, &out).await.unwrap(), 0);
        assert_eq!(
            std::fs::read_to_string(&out).unwrap(),
            format!("{}\r\n", CSV_COLUMNS.join(","))
        );
    }

    #[tokio::test]
    async fn a_backup_copied_over_the_data_file_restores_the_same_jobs() {
        let dir = scratch("backup");
        let data = dir.join("data.db");
        let pool = open_pool(&data).await;
        insert_job(&pool, job("Rotterdam", None)).await.unwrap();
        insert_job(&pool, job("Berlin", Some("note")))
            .await
            .unwrap();

        let backup = dir.join("backup.db");
        // A stale file at the target is replaced.
        std::fs::write(&backup, b"not a database").unwrap();
        backup_to(&pool, &backup).await.unwrap();
        let header = std::fs::read(&backup).unwrap();
        assert!(header.starts_with(b"SQLite format 3\0"));

        // Keep writing to the live database after the snapshot.
        insert_job(&pool, job("Oslo", None)).await.unwrap();
        assert_eq!(
            fetch_jobs(&pool, JobFilter::default()).await.unwrap().total,
            3
        );
        pool.close().await;

        // Restore: the backup replaces the data file, and the app opens it as usual.
        std::fs::copy(&backup, &data).unwrap();
        let restored = open_pool(&data).await;
        let page = fetch_jobs(&restored, JobFilter::default()).await.unwrap();
        assert_eq!(page.total, 2);
        let mut cities: Vec<&str> = page.items.iter().map(|j| j.dest_city.as_str()).collect();
        cities.sort();
        assert_eq!(cities, ["Berlin", "Rotterdam"]);
        assert_eq!(
            page.items
                .iter()
                .find(|j| j.dest_city == "Berlin")
                .unwrap()
                .notes
                .as_deref(),
            Some("note")
        );
        // The rollup came with it.
        let trucks = crate::commands::trucks::fetch_trucks(&restored)
            .await
            .unwrap();
        assert_eq!(trucks.len(), 1);
        assert_eq!(trucks[0].total_jobs, 2);
    }

    #[tokio::test]
    async fn errors_name_the_path() {
        let pool = open_pool(&scratch("err").join("data.db")).await;
        let bad = Path::new("Z:/definitely/missing/folder/out.csv");
        let err = write_jobs_csv(&pool, bad).await.unwrap_err();
        assert!(format!("{err:#}").contains("out.csv"));
        let err = backup_to(&pool, bad).await.unwrap_err();
        assert!(format!("{err:#}").contains("out.csv"));
    }
}

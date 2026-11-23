//! Data export and backup (M3.3).
//!
//! `export_jobs_csv` writes every job to a CSV file (RFC 4180: header row, CRLF,
//! fields quoted when they hold a comma, quote or line break). `backup_database`
//! snapshots the live database with `VACUUM INTO`, which produces a clean,
//! consistent copy without closing the pool.
//!
//! Both commands open the native save dialog themselves and never take a path from the
//! webview (#5613): a path argument would let any script in the webview write or delete files
//! anywhere. Test profiles have no dialog; [`pick_save_path`] reads the answer from
//! `<data dir>/e2e-save-as` instead (written by the E2E spec from Node).

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::Serialize;
use sqlx::SqlitePool;
use tauri::{AppHandle, Manager, Runtime, State};
use tauri_plugin_dialog::DialogExt;

use crate::db::models::Job;
use crate::db::AppState;

/// Column order of the CSV. Keep it stable: people build spreadsheets on it. New columns go at
/// the end (the game's figures and the job's costs, #4756).
pub const CSV_COLUMNS: [&str; 32] = [
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
    "game_revenue",
    "earned_xp",
    "cargo_damage",
    "game_distance_km",
    "auto_park",
    "auto_load",
    "cancel_penalty",
    "costs",
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

/// One CSV line (with CRLF) for a job, in `CSV_COLUMNS` order. `costs` is the sum of the job's
/// tolls, fines and fares.
fn csv_row(job: &Job, costs: i64) -> String {
    let fields: [String; 32] = [
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
        opt(&job.game.game_revenue),
        opt(&job.game.earned_xp),
        opt(&job.game.cargo_damage),
        opt(&job.game.game_distance_km),
        opt(&job.game.auto_park),
        opt(&job.game.auto_load),
        opt(&job.game.cancel_penalty),
        costs.to_string(),
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
    let costs: std::collections::HashMap<String, i64> = sqlx::query_as(
        "SELECT job_id, SUM(amount) FROM expenses WHERE job_id IS NOT NULL GROUP BY job_id",
    )
    .fetch_all(pool)
    .await
    .context("loading costs for export")?
    .into_iter()
    .collect();
    let mut out = String::new();
    writeln!(out, "{}\r", CSV_COLUMNS.join(",")).expect("writing to a String");
    for job in &jobs {
        out.push_str(&csv_row(job, costs.get(&job.id).copied().unwrap_or(0)));
    }
    Ok((out, jobs.len()))
}

/// Write every job to `path` as CSV. Returns the number of jobs written (header excluded).
pub async fn write_jobs_csv(pool: &SqlitePool, path: &Path) -> Result<usize> {
    let (csv, count) = jobs_csv(pool).await?;
    std::fs::write(path, csv).with_context(|| format!("writing {}", path.display()))?;
    Ok(count)
}

const SQLITE_HEADER: &[u8; 16] = b"SQLite format 3\0";

/// Snapshot the database to `path` with `VACUUM INTO`. `VACUUM INTO` refuses a non-empty target,
/// so the snapshot goes to a temporary file next to it and is then moved over the target. An
/// existing target is replaced only when it is empty or a SQLite database (an older backup):
/// anything else is refused, whatever the dialog said.
pub async fn backup_to(pool: &SqlitePool, path: &Path) -> Result<()> {
    let (_, _, live): (i64, String, String) = sqlx::query_as("PRAGMA database_list")
        .fetch_one(pool)
        .await
        .context("finding the database file")?;
    if !live.is_empty() && same_file(Path::new(&live), path) {
        bail!("That is the app's own database. Choose another file name.");
    }
    if path.exists() && !is_replaceable_backup(path)? {
        bail!(
            "{} exists and isn't a database. Choose another file name.",
            path.display()
        );
    }
    let name = path
        .file_name()
        .with_context(|| format!("{} has no file name", path.display()))?;
    let mut temp_name = name.to_os_string();
    temp_name.push(format!(".{}.tmp", uuid::Uuid::new_v4().simple()));
    let temp = path.with_file_name(temp_name);
    let target = temp.to_string_lossy().into_owned();
    let result = async {
        sqlx::query("VACUUM INTO ?")
            .bind(target)
            .execute(pool)
            .await
            .with_context(|| format!("backing up to {}", path.display()))?;
        std::fs::rename(&temp, path).with_context(|| format!("replacing {}", path.display()))
    }
    .await;
    if result.is_err() && temp.exists() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

fn is_replaceable_backup(path: &Path) -> Result<bool> {
    use std::io::Read as _;
    let mut header = Vec::with_capacity(SQLITE_HEADER.len());
    std::fs::File::open(path)
        .with_context(|| format!("reading {}", path.display()))?
        .take(SQLITE_HEADER.len() as u64)
        .read_to_end(&mut header)
        .with_context(|| format!("reading {}", path.display()))?;
    Ok(header.is_empty() || header == SQLITE_HEADER)
}

/// Refuses a chosen file whose extension isn't `ext` (any case).
pub fn require_extension(path: &Path, ext: &str) -> Result<()> {
    let ok = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case(ext));
    if !ok {
        bail!("Choose a file name ending in .{ext}.");
    }
    Ok(())
}

/// Where to save, chosen by the user in the native save dialog; `None` when they cancel.
/// In a test profile there is no dialog: the answer is the first line of
/// `<data dir>/e2e-save-as`, and a missing file means "cancelled".
async fn pick_save_path<R: Runtime>(
    app: &AppHandle<R>,
    file_name: String,
    filter: (&str, &str),
) -> Result<Option<PathBuf>> {
    if let Some(dir) = crate::db::data_dir_override() {
        return Ok(std::fs::read_to_string(dir.join("e2e-save-as"))
            .ok()
            .and_then(|s| s.lines().next().map(|l| PathBuf::from(l.trim())))
            .filter(|p| !p.as_os_str().is_empty()));
    }
    let (tx, rx) = tokio::sync::oneshot::channel();
    let mut dialog = app
        .dialog()
        .file()
        .set_file_name(file_name)
        .add_filter(filter.0, &[filter.1]);
    if let Some(window) = app.get_webview_window("main") {
        dialog = dialog.set_parent(&window);
    }
    dialog.save_file(move |chosen| {
        let _ = tx.send(chosen);
    });
    match rx.await.context("the save dialog closed unexpectedly")? {
        None => Ok(None),
        Some(chosen) => Ok(Some(
            chosen
                .into_path()
                .context("the save dialog returned no file path")?,
        )),
    }
}

fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

// --- Tauri command wrappers ---

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Exported {
    pub path: String,
    pub count: usize,
}

/// "Export jobs to CSV": asks where, then writes. `None` when the dialog was cancelled.
#[tauri::command]
pub async fn export_jobs_csv(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<Exported>, String> {
    let run = async {
        let name = format!("projecttrucks-jobs-{}.csv", today());
        let Some(path) = pick_save_path(&app, name, ("CSV", "csv")).await? else {
            return Ok(None);
        };
        require_extension(&path, "csv")?;
        let count = write_jobs_csv(&state.pool, &path).await?;
        Ok(Some(Exported {
            path: path.display().to_string(),
            count,
        }))
    };
    run.await.map_err(|e: anyhow::Error| format!("{e:#}"))
}

/// "Back up database": asks where, then snapshots. The saved path, or `None` when cancelled.
#[tauri::command]
pub async fn backup_database(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<String>, String> {
    let run = async {
        let name = format!("projecttrucks-backup-{}.db", today());
        let Some(path) = pick_save_path(&app, name, ("SQLite database", "db")).await? else {
            return Ok(None);
        };
        require_extension(&path, "db")?;
        backup_to(&state.pool, &path).await?;
        Ok(Some(path.display().to_string()))
    };
    run.await.map_err(|e: anyhow::Error| format!("{e:#}"))
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
            game_duration_min: None,
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
    fn the_chosen_file_needs_the_right_extension() {
        assert!(require_extension(Path::new("C:/x/jobs.csv"), "csv").is_ok());
        assert!(require_extension(Path::new("C:/x/JOBS.CSV"), "csv").is_ok());
        assert!(require_extension(Path::new("C:/x/backup.db"), "db").is_ok());
        for bad in [
            "C:/x/jobs",
            "C:/x/run.bat",
            "C:/x/jobs.csv.bat",
            "C:/x/backup.db.lnk",
        ] {
            assert!(require_extension(Path::new(bad), "csv").is_err(), "{bad}");
        }
        assert!(require_extension(Path::new("C:/x/jobs.csv"), "db").is_err());
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
        // Notes, then the game's figures (none for a manual job) and its costs.
        assert!(rotterdam.ends_with(",false,,\"fragile, keep \"\"cool\"\"\",,,,,,,,0"));
        // Every data row has the same number of unquoted commas as the header.
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
        // A file that isn't a database is never replaced (#5613)...
        std::fs::write(&backup, b"not a database").unwrap();
        let refused = backup_to(&pool, &backup).await.unwrap_err();
        assert!(format!("{refused:#}").contains("isn't a database"));
        assert_eq!(std::fs::read(&backup).unwrap(), b"not a database");
        // ...an empty one is, and so is an older backup.
        std::fs::write(&backup, b"").unwrap();
        backup_to(&pool, &backup).await.unwrap();
        backup_to(&pool, &backup).await.unwrap();
        let header = std::fs::read(&backup).unwrap();
        assert!(header.starts_with(b"SQLite format 3\0"));
        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "no temporary files left behind");
        // Never over the live database itself.
        let own = backup_to(&pool, &data).await.unwrap_err();
        assert!(format!("{own:#}").contains("own database"));

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

//! Dashboard statistics (M3.1).
//!
//! Distances follow the same rules as the truck rollups (`db::rollups`):
//! `driving_sessions` is the source of truth for km driven (JOB sessions are
//! delivery km, FREE_ROAM sessions are free-roam km), and a manual job, which
//! has no session row, counts its own `distance_km` as delivery km.
//! Deliveries and income count jobs that were delivered (on time or late);
//! cancelled jobs are left out of both.
//!
//! "This month", "last month" and the per-day series follow the viewer's local
//! time: the frontend sends its UTC offset, since the Rust side has no idea
//! what time zone the window is in.

use anyhow::{Context, Result};
use chrono::{DateTime, Datelike, Duration, FixedOffset, NaiveDate, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{QueryBuilder, Sqlite, SqlitePool};
use tauri::State;

use crate::commands::jobs::fetch_jobs;
use crate::commands::trucks::fetch_truck;
use crate::db::models::{Job, JobFilter, Truck};
use crate::db::AppState;

/// How many days the distance series covers (today included).
pub const SERIES_DAYS: i64 = 30;
/// How many jobs the "recent jobs" strip shows.
pub const RECENT_JOBS: i64 = 5;

/// Totals for one period (lifetime, this month, last month).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeriodStats {
    pub total_km: f64,
    pub delivery_km: f64,
    pub free_roam_km: f64,
    /// Jobs delivered (on time or late). Cancelled jobs are not deliveries.
    pub deliveries: i64,
    /// Declared income of the deliveries, in game currency.
    pub income: i64,
    pub on_time: i64,
    pub late: i64,
}

/// One day of the distance series. `day` is a local `YYYY-MM-DD`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DistanceDay {
    pub day: String,
    pub delivery_km: f64,
    pub free_roam_km: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardStats {
    pub lifetime: PeriodStats,
    pub this_month: PeriodStats,
    pub last_month: PeriodStats,
    /// Exactly `SERIES_DAYS` entries, oldest first, ending today (local).
    pub last_30_days: Vec<DistanceDay>,
    /// Latest `RECENT_JOBS` jobs by `completed_at`, any status.
    pub recent_jobs: Vec<Job>,
}

/// One truck's figures (M3.2). The period totals follow the same rules as the
/// dashboard, scoped to rows whose `truck_make`/`truck_model` match the truck.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TruckStats {
    pub truck: Truck,
    /// Everything ever driven on this truck. `total_km` equals the Trucks list.
    pub lifetime: PeriodStats,
    /// Jobs of any status on this truck, the same count as the Trucks list.
    pub total_jobs: i64,
    /// Mean `distance_km` of those jobs (what the Jobs table shows per row); `None` with no jobs.
    pub avg_job_km: Option<f64>,
    /// Exactly `SERIES_DAYS` entries, oldest first, ending today (local).
    pub last_30_days: Vec<DistanceDay>,
}

/// Restrict the stats queries to one truck. `None` means the whole database.
#[derive(Debug, Clone, Copy)]
struct TruckScope<'a> {
    make: &'a str,
    model: &'a str,
}

/// Push ` AND truck_make = ? AND truck_model = ?` when scoped to a truck.
fn push_truck(qb: &mut QueryBuilder<Sqlite>, truck: Option<TruckScope<'_>>) {
    if let Some(truck) = truck {
        qb.push(" AND truck_make = ");
        qb.push_bind(truck.make.to_owned());
        qb.push(" AND truck_model = ");
        qb.push_bind(truck.model.to_owned());
    }
}

/// Optional inputs for `get_dashboard_stats`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardQuery {
    /// Minutes east of UTC for the viewer's local time (JS: `-getTimezoneOffset()`).
    /// Month and day boundaries follow it. Defaults to UTC.
    pub utc_offset_minutes: Option<i32>,
    /// "Now". Defaults to the current time; tests pin it.
    pub now: Option<DateTime<Utc>>,
}

/// Local-time calendar boundaries for a query.
struct Calendar {
    offset: FixedOffset,
    today: NaiveDate,
    month_start: DateTime<Utc>,
    last_month_start: DateTime<Utc>,
    series_start: DateTime<Utc>,
}

impl Calendar {
    fn new(now: DateTime<Utc>, utc_offset_minutes: i32) -> Self {
        // An offset outside ±24h can't come from a real clock; fall back to UTC.
        let offset = FixedOffset::east_opt(utc_offset_minutes * 60)
            .unwrap_or_else(|| FixedOffset::east_opt(0).expect("UTC is a valid offset"));
        let local = now.with_timezone(&offset);
        let today = local.date_naive();
        let month_start_date = NaiveDate::from_ymd_opt(today.year(), today.month(), 1)
            .expect("the first of the current month exists");
        let last_month_start_date = if today.month() == 1 {
            NaiveDate::from_ymd_opt(today.year() - 1, 12, 1)
        } else {
            NaiveDate::from_ymd_opt(today.year(), today.month() - 1, 1)
        }
        .expect("the first of last month exists");
        let series_start_date = today - Duration::days(SERIES_DAYS - 1);
        Self {
            offset,
            today,
            month_start: local_midnight(offset, month_start_date),
            last_month_start: local_midnight(offset, last_month_start_date),
            series_start: local_midnight(offset, series_start_date),
        }
    }

    /// The local calendar day a UTC timestamp falls on.
    fn day_of(&self, at: DateTime<Utc>) -> NaiveDate {
        at.with_timezone(&self.offset).date_naive()
    }
}

/// 00:00 local on `date`, as UTC. A fixed offset has no gaps, so this always resolves.
fn local_midnight(offset: FixedOffset, date: NaiveDate) -> DateTime<Utc> {
    let midnight = date.and_hms_opt(0, 0, 0).expect("midnight exists");
    offset
        .from_local_datetime(&midnight)
        .single()
        .expect("fixed offsets have no DST gaps")
        .with_timezone(&Utc)
}

/// Push ` AND <column> >= from AND <column> < to` for the bounds that are set.
fn push_range(
    qb: &mut QueryBuilder<Sqlite>,
    column: &str,
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
) {
    if let Some(from) = from {
        qb.push(format!(" AND {column} >= "));
        qb.push_bind(from);
    }
    if let Some(to) = to {
        qb.push(format!(" AND {column} < "));
        qb.push_bind(to);
    }
}

/// Totals for sessions ended and jobs completed in `[from, to)` (either bound optional).
pub async fn period_stats(
    pool: &SqlitePool,
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
) -> Result<PeriodStats> {
    scoped_period_stats(pool, None, from, to).await
}

async fn scoped_period_stats(
    pool: &SqlitePool,
    truck: Option<TruckScope<'_>>,
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
) -> Result<PeriodStats> {
    // The `ELSE 0.0` literals keep SUM a REAL (see db::rollups).
    let mut sessions: QueryBuilder<Sqlite> = QueryBuilder::new(
        "SELECT
           COALESCE(SUM(CASE WHEN kind = 'JOB' THEN distance_km ELSE 0.0 END), 0.0),
           COALESCE(SUM(CASE WHEN kind = 'FREE_ROAM' THEN distance_km ELSE 0.0 END), 0.0)
         FROM driving_sessions WHERE 1=1",
    );
    push_truck(&mut sessions, truck);
    push_range(&mut sessions, "ended_at", from, to);
    let (session_delivery_km, free_roam_km): (f64, f64) = sessions
        .build_query_as()
        .fetch_one(pool)
        .await
        .context("summing sessions")?;

    let mut jobs: QueryBuilder<Sqlite> = QueryBuilder::new(
        "SELECT
           COUNT(CASE WHEN status IN ('DELIVERED', 'LATE') THEN 1 END),
           COALESCE(SUM(CASE WHEN status IN ('DELIVERED', 'LATE') THEN declared_income ELSE 0 END), 0),
           COUNT(CASE WHEN status = 'DELIVERED' THEN 1 END),
           COUNT(CASE WHEN status = 'LATE' THEN 1 END),
           COALESCE(SUM(CASE WHEN source = 'MANUAL' AND session_id IS NULL
                             THEN distance_km ELSE 0.0 END), 0.0)
         FROM jobs WHERE 1=1",
    );
    push_truck(&mut jobs, truck);
    push_range(&mut jobs, "completed_at", from, to);
    let (deliveries, income, on_time, late, manual_km): (i64, i64, i64, i64, f64) = jobs
        .build_query_as()
        .fetch_one(pool)
        .await
        .context("summing jobs")?;

    let delivery_km = session_delivery_km + manual_km;
    Ok(PeriodStats {
        total_km: delivery_km + free_roam_km,
        delivery_km,
        free_roam_km,
        deliveries,
        income,
        on_time,
        late,
    })
}

/// Per-day distances for the last `SERIES_DAYS` local days, zero-filled.
async fn distance_series(
    pool: &SqlitePool,
    cal: &Calendar,
    truck: Option<TruckScope<'_>>,
) -> Result<Vec<DistanceDay>> {
    let mut days: Vec<DistanceDay> = (0..SERIES_DAYS)
        .map(|i| DistanceDay {
            day: (cal.today - Duration::days(SERIES_DAYS - 1 - i)).to_string(),
            delivery_km: 0.0,
            free_roam_km: 0.0,
        })
        .collect();
    let first = cal.today - Duration::days(SERIES_DAYS - 1);
    let mut add = |at: DateTime<Utc>, delivery: bool, km: f64| {
        let day = cal.day_of(at);
        let index = (day - first).num_days();
        if (0..SERIES_DAYS).contains(&index) {
            let entry = &mut days[index as usize];
            if delivery {
                entry.delivery_km += km;
            } else {
                entry.free_roam_km += km;
            }
        }
    };

    let mut sessions: QueryBuilder<Sqlite> =
        QueryBuilder::new("SELECT kind, distance_km, ended_at FROM driving_sessions WHERE 1=1");
    push_truck(&mut sessions, truck);
    push_range(&mut sessions, "ended_at", Some(cal.series_start), None);
    let sessions: Vec<(String, f64, DateTime<Utc>)> = sessions
        .build_query_as()
        .fetch_all(pool)
        .await
        .context("loading sessions for the series")?;
    for (kind, km, ended_at) in sessions {
        add(ended_at, kind == "JOB", km);
    }

    let mut manual: QueryBuilder<Sqlite> = QueryBuilder::new(
        "SELECT distance_km, completed_at FROM jobs
         WHERE source = 'MANUAL' AND session_id IS NULL",
    );
    push_truck(&mut manual, truck);
    push_range(&mut manual, "completed_at", Some(cal.series_start), None);
    let manual: Vec<(f64, DateTime<Utc>)> = manual
        .build_query_as()
        .fetch_all(pool)
        .await
        .context("loading manual jobs for the series")?;
    for (km, completed_at) in manual {
        add(completed_at, true, km);
    }

    Ok(days)
}

/// Stats for one truck, or `None` when the id is unknown.
pub async fn fetch_truck_stats(
    pool: &SqlitePool,
    id: &str,
    query: DashboardQuery,
) -> Result<Option<TruckStats>> {
    let Some(truck) = fetch_truck(pool, id).await? else {
        return Ok(None);
    };
    let scope = Some(TruckScope {
        make: &truck.make,
        model: &truck.model,
    });
    let now = query.now.unwrap_or_else(Utc::now);
    let cal = Calendar::new(now, query.utc_offset_minutes.unwrap_or(0));

    let lifetime = scoped_period_stats(pool, scope, None, None).await?;
    let last_30_days = distance_series(pool, &cal, scope).await?;

    let mut jobs: QueryBuilder<Sqlite> =
        QueryBuilder::new("SELECT COUNT(*), AVG(distance_km) FROM jobs WHERE 1=1");
    push_truck(&mut jobs, scope);
    let (total_jobs, avg_job_km): (i64, Option<f64>) = jobs
        .build_query_as()
        .fetch_one(pool)
        .await
        .context("averaging the truck's jobs")?;

    Ok(Some(TruckStats {
        truck,
        lifetime,
        total_jobs,
        avg_job_km,
        last_30_days,
    }))
}

pub async fn fetch_dashboard_stats(
    pool: &SqlitePool,
    query: DashboardQuery,
) -> Result<DashboardStats> {
    let now = query.now.unwrap_or_else(Utc::now);
    let cal = Calendar::new(now, query.utc_offset_minutes.unwrap_or(0));

    let lifetime = period_stats(pool, None, None).await?;
    let this_month = period_stats(pool, Some(cal.month_start), None).await?;
    let last_month = period_stats(pool, Some(cal.last_month_start), Some(cal.month_start)).await?;
    let last_30_days = distance_series(pool, &cal, None).await?;
    let recent_jobs = fetch_jobs(
        pool,
        JobFilter {
            limit: Some(RECENT_JOBS),
            ..JobFilter::default()
        },
    )
    .await?
    .items;

    Ok(DashboardStats {
        lifetime,
        this_month,
        last_month,
        last_30_days,
        recent_jobs,
    })
}

// --- Tauri command wrapper ---

#[tauri::command]
pub async fn get_dashboard_stats(
    state: State<'_, AppState>,
    query: Option<DashboardQuery>,
) -> Result<DashboardStats, String> {
    fetch_dashboard_stats(&state.pool, query.unwrap_or_default())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_truck_stats(
    state: State<'_, AppState>,
    id: String,
    query: Option<DashboardQuery>,
) -> Result<Option<TruckStats>, String> {
    fetch_truck_stats(&state.pool, &id, query.unwrap_or_default())
        .await
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::jobs::insert_job;
    use crate::commands::sessions::insert_session;
    use crate::db::models::{JobSource, JobStatus, NewJob, NewSession, SessionKind, SessionStatus};
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

    /// 2026-05-15 12:00 UTC, a Friday mid-month.
    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 5, 15, 12, 0, 0).unwrap()
    }

    fn query(now: DateTime<Utc>, offset: i32) -> DashboardQuery {
        DashboardQuery {
            utc_offset_minutes: Some(offset),
            now: Some(now),
        }
    }

    fn session(kind: SessionKind, km: f64, ended_at: DateTime<Utc>) -> NewSession {
        NewSession {
            kind,
            truck_make: Some("Scania".into()),
            truck_model: Some("R 730".into()),
            start_odometer: 0.0,
            end_odometer: km,
            started_at: ended_at - Duration::hours(1),
            ended_at,
            status: SessionStatus::Completed,
        }
    }

    fn job(
        session_id: Option<String>,
        km: f64,
        income: i64,
        status: JobStatus,
        completed_at: DateTime<Utc>,
    ) -> NewJob {
        NewJob {
            source: if session_id.is_some() {
                JobSource::Telemetry
            } else {
                JobSource::Manual
            },
            session_id,
            truck_make: Some("Scania".into()),
            truck_model: Some("R 730".into()),
            source_city: "Calais".into(),
            source_company: "Tradeaux".into(),
            dest_city: "Rotterdam".into(),
            dest_company: "Kaarfor".into(),
            cargo_id: "apples".into(),
            cargo_name: "Apples".into(),
            cargo_mass_kg: 18_000.0,
            declared_income: income,
            distance_km: km,
            duration_sec: 3_600 * 4,
            fuel_used_l: None,
            damage_delta: None,
            started_at: completed_at - Duration::hours(4),
            completed_at,
            status,
            notes: None,
        }
    }

    #[tokio::test]
    async fn fresh_database_is_all_zeros_with_a_full_series() {
        let pool = test_pool().await;
        let stats = fetch_dashboard_stats(&pool, query(now(), 0)).await.unwrap();
        assert_eq!(stats.lifetime, PeriodStats::default());
        assert_eq!(stats.this_month, PeriodStats::default());
        assert_eq!(stats.last_month, PeriodStats::default());
        assert_eq!(stats.last_30_days.len(), SERIES_DAYS as usize);
        assert_eq!(stats.last_30_days[0].day, "2026-04-16");
        assert_eq!(stats.last_30_days[29].day, "2026-05-15");
        assert!(stats
            .last_30_days
            .iter()
            .all(|d| d.delivery_km == 0.0 && d.free_roam_km == 0.0));
        assert!(stats.recent_jobs.is_empty());
    }

    #[tokio::test]
    async fn sessions_and_manual_jobs_add_up_like_the_truck_rollup() {
        let pool = test_pool().await;
        let today = now() - Duration::hours(2);
        // A recorded delivery: the session carries the km, the job row says less (ignored).
        let ses = insert_session(&pool, session(SessionKind::Job, 200.0, today))
            .await
            .unwrap();
        insert_job(
            &pool,
            job(Some(ses.id), 100.0, 5_000, JobStatus::Delivered, today),
        )
        .await
        .unwrap();
        // Free roam.
        insert_session(&pool, session(SessionKind::FreeRoam, 30.0, today))
            .await
            .unwrap();
        // A manual late job: its own km count as delivery km.
        insert_job(&pool, job(None, 412.0, 8_500, JobStatus::Late, today))
            .await
            .unwrap();
        // A cancelled manual job: km still count (as in the rollup), no delivery or income.
        insert_job(&pool, job(None, 50.0, 9_999, JobStatus::Cancelled, today))
            .await
            .unwrap();

        let stats = fetch_dashboard_stats(&pool, query(now(), 0)).await.unwrap();
        assert_eq!(
            stats.lifetime,
            PeriodStats {
                total_km: 692.0,
                delivery_km: 662.0,
                free_roam_km: 30.0,
                deliveries: 2,
                income: 13_500,
                on_time: 1,
                late: 1,
            }
        );
        assert_eq!(stats.this_month, stats.lifetime);
        assert_eq!(stats.last_month, PeriodStats::default());

        let today_entry = stats.last_30_days.last().unwrap();
        assert_eq!(today_entry.day, "2026-05-15");
        assert_eq!(today_entry.delivery_km, 662.0);
        assert_eq!(today_entry.free_roam_km, 30.0);

        assert_eq!(stats.recent_jobs.len(), 3);
    }

    #[tokio::test]
    async fn this_month_and_last_month_split_on_the_local_first() {
        let pool = test_pool().await;
        // April 30th 23:30 UTC is still April in UTC, but already May 1st at UTC+2.
        let boundary = Utc.with_ymd_and_hms(2026, 4, 30, 23, 30, 0).unwrap();
        insert_session(&pool, session(SessionKind::FreeRoam, 10.0, boundary))
            .await
            .unwrap();
        // Clearly April, and clearly March (outside both months).
        let april = Utc.with_ymd_and_hms(2026, 4, 10, 12, 0, 0).unwrap();
        let march = Utc.with_ymd_and_hms(2026, 3, 10, 12, 0, 0).unwrap();
        insert_session(&pool, session(SessionKind::FreeRoam, 100.0, april))
            .await
            .unwrap();
        insert_session(&pool, session(SessionKind::FreeRoam, 1000.0, march))
            .await
            .unwrap();

        let utc = fetch_dashboard_stats(&pool, query(now(), 0)).await.unwrap();
        assert_eq!(utc.this_month.total_km, 0.0);
        assert_eq!(utc.last_month.total_km, 110.0);
        assert_eq!(utc.lifetime.total_km, 1110.0);

        let cest = fetch_dashboard_stats(&pool, query(now(), 120))
            .await
            .unwrap();
        assert_eq!(cest.this_month.total_km, 10.0);
        assert_eq!(cest.last_month.total_km, 100.0);
    }

    #[tokio::test]
    async fn last_month_wraps_into_the_previous_year() {
        let pool = test_pool().await;
        let january = Utc.with_ymd_and_hms(2027, 1, 20, 12, 0, 0).unwrap();
        let december = Utc.with_ymd_and_hms(2026, 12, 24, 12, 0, 0).unwrap();
        insert_session(&pool, session(SessionKind::FreeRoam, 5.0, december))
            .await
            .unwrap();
        let stats = fetch_dashboard_stats(&pool, query(january, 0))
            .await
            .unwrap();
        assert_eq!(stats.last_month.total_km, 5.0);
        assert_eq!(stats.this_month.total_km, 0.0);
    }

    #[tokio::test]
    async fn series_buckets_by_local_day_and_drops_older_rows() {
        let pool = test_pool().await;
        // 23:30 UTC on the 14th is the 15th at UTC+2.
        let late_evening = Utc.with_ymd_and_hms(2026, 5, 14, 23, 30, 0).unwrap();
        insert_session(&pool, session(SessionKind::Job, 40.0, late_evening))
            .await
            .unwrap();
        // 31 days ago: outside the series, still in lifetime.
        let old = now() - Duration::days(31);
        insert_session(&pool, session(SessionKind::FreeRoam, 7.0, old))
            .await
            .unwrap();
        // Exactly the first day of the series.
        let first_day = Utc.with_ymd_and_hms(2026, 4, 16, 8, 0, 0).unwrap();
        insert_job(&pool, job(None, 3.0, 100, JobStatus::Delivered, first_day))
            .await
            .unwrap();

        let utc = fetch_dashboard_stats(&pool, query(now(), 0)).await.unwrap();
        let find = |s: &DashboardStats, day: &str| {
            s.last_30_days
                .iter()
                .find(|d| d.day == day)
                .cloned()
                .unwrap_or_else(|| panic!("{day} missing"))
        };
        assert_eq!(find(&utc, "2026-05-14").delivery_km, 40.0);
        assert_eq!(find(&utc, "2026-05-15").delivery_km, 0.0);
        assert_eq!(find(&utc, "2026-04-16").delivery_km, 3.0);
        assert_eq!(utc.lifetime.free_roam_km, 7.0);
        assert!(utc.last_30_days.iter().all(|d| d.free_roam_km == 0.0));

        let cest = fetch_dashboard_stats(&pool, query(now(), 120))
            .await
            .unwrap();
        assert_eq!(find(&cest, "2026-05-14").delivery_km, 0.0);
        assert_eq!(find(&cest, "2026-05-15").delivery_km, 40.0);
    }

    #[tokio::test]
    async fn recent_jobs_are_the_latest_five_by_completion() {
        let pool = test_pool().await;
        for i in 0..7 {
            let at = now() - Duration::days(i);
            insert_job(&pool, job(None, 10.0, 100, JobStatus::Delivered, at))
                .await
                .unwrap();
        }
        let stats = fetch_dashboard_stats(&pool, query(now(), 0)).await.unwrap();
        assert_eq!(stats.recent_jobs.len(), RECENT_JOBS as usize);
        assert_eq!(stats.recent_jobs[0].completed_at, now());
        assert_eq!(stats.recent_jobs[4].completed_at, now() - Duration::days(4));
    }

    #[tokio::test]
    async fn truck_stats_only_count_that_truck_and_match_its_rollup() {
        use crate::commands::trucks::fetch_truck;
        use crate::db::rollups::truck_id;

        let pool = test_pool().await;
        let today = now() - Duration::hours(2);
        // Scania R 730 (the test helpers' default truck): 200 km recorded delivery (job row says
        // 100), 30 km free roam, a 412 km manual late job and a 50 km cancelled manual job.
        let ses = insert_session(&pool, session(SessionKind::Job, 200.0, today))
            .await
            .unwrap();
        insert_job(
            &pool,
            job(Some(ses.id), 100.0, 5_000, JobStatus::Delivered, today),
        )
        .await
        .unwrap();
        insert_session(&pool, session(SessionKind::FreeRoam, 30.0, today))
            .await
            .unwrap();
        insert_job(&pool, job(None, 412.0, 8_500, JobStatus::Late, today))
            .await
            .unwrap();
        insert_job(&pool, job(None, 50.0, 9_999, JobStatus::Cancelled, today))
            .await
            .unwrap();
        // A Volvo with its own rows, which must stay out of the Scania's numbers.
        let mut volvo_session = session(SessionKind::FreeRoam, 1_000.0, today);
        volvo_session.truck_make = Some("Volvo".into());
        volvo_session.truck_model = Some("FH16".into());
        insert_session(&pool, volvo_session).await.unwrap();
        let mut volvo_job = job(None, 999.0, 1, JobStatus::Delivered, today);
        volvo_job.truck_make = Some("Volvo".into());
        volvo_job.truck_model = Some("FH16".into());
        insert_job(&pool, volvo_job).await.unwrap();

        let id = truck_id("Scania", "R 730");
        let stats = fetch_truck_stats(&pool, &id, query(now(), 0))
            .await
            .unwrap()
            .expect("the Scania exists");
        assert_eq!(stats.truck.id, id);
        assert_eq!(
            stats.lifetime,
            PeriodStats {
                total_km: 692.0,
                delivery_km: 662.0,
                free_roam_km: 30.0,
                deliveries: 2,
                income: 13_500,
                on_time: 1,
                late: 1,
            }
        );
        // The Trucks list shows the rollup row: the page must agree with it.
        let rollup = fetch_truck(&pool, &id).await.unwrap().unwrap();
        assert_eq!(stats.lifetime.total_km, rollup.total_distance_km);
        assert_eq!(stats.lifetime.delivery_km, rollup.total_delivery_distance);
        assert_eq!(stats.lifetime.free_roam_km, rollup.total_free_roam_distance);
        assert_eq!(stats.total_jobs, rollup.total_jobs);
        assert_eq!(stats.total_jobs, 3);
        // Mean of the job rows' own distances: (100 + 412 + 50) / 3.
        let avg = stats.avg_job_km.unwrap();
        assert!((avg - 562.0 / 3.0).abs() < 1e-9, "avg was {avg}");

        let today_entry = stats.last_30_days.last().unwrap();
        assert_eq!(stats.last_30_days.len(), SERIES_DAYS as usize);
        assert_eq!(today_entry.delivery_km, 662.0);
        assert_eq!(today_entry.free_roam_km, 30.0);

        // The Volvo sees only its own rows.
        let volvo = fetch_truck_stats(&pool, &truck_id("Volvo", "FH16"), query(now(), 0))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(volvo.lifetime.total_km, 1_999.0);
        assert_eq!(volvo.lifetime.free_roam_km, 1_000.0);
        assert_eq!(volvo.total_jobs, 1);
        assert_eq!(volvo.avg_job_km, Some(999.0));

        // The dashboard still sums both trucks.
        let all = fetch_dashboard_stats(&pool, query(now(), 0)).await.unwrap();
        assert_eq!(all.lifetime.total_km, 692.0 + 1_999.0);
    }

    #[tokio::test]
    async fn truck_stats_for_an_unknown_truck_are_none_and_a_truck_without_jobs_has_no_average() {
        use crate::db::rollups::truck_id;

        let pool = test_pool().await;
        assert!(fetch_truck_stats(&pool, "nope", query(now(), 0))
            .await
            .unwrap()
            .is_none());

        insert_session(&pool, session(SessionKind::FreeRoam, 12.0, now()))
            .await
            .unwrap();
        let stats = fetch_truck_stats(&pool, &truck_id("Scania", "R 730"), query(now(), 0))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stats.total_jobs, 0);
        assert_eq!(stats.avg_job_km, None);
        assert_eq!(stats.lifetime.free_roam_km, 12.0);
        assert_eq!(stats.lifetime.deliveries, 0);
    }

    #[tokio::test]
    async fn an_absurd_offset_falls_back_to_utc() {
        let pool = test_pool().await;
        let stats = fetch_dashboard_stats(&pool, query(now(), 100_000))
            .await
            .unwrap();
        assert_eq!(stats.last_30_days[29].day, "2026-05-15");
    }
}

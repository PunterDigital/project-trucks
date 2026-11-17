//! Telemetry pipeline: feeds samples to the state machine and writes what comes out to SQLite.
//!
//! Sessions and jobs go through the same `insert_*_row` helpers as the manual-entry commands. A
//! completed delivery writes its session and job in one transaction; the truck rollup is
//! recomputed once that commits. A cancelled delivery is written the same way, with a job row of
//! status `CANCELLED` (#2513: a cancel has to show up in Jobs); the stats count only delivered and
//! late jobs as deliveries and income, and the kilometres stay in the session.
//!
//! Gameplay events (#4756): a finished job also gets the game's own figures (revenue, XP, ...,
//! or the cancel penalty) and its tolls, fines and fares in the same transaction; a cost paid with
//! no job running is written straight away.

use anyhow::{Context, Result};
use chrono::NaiveDateTime;
use sqlx::SqlitePool;

use super::state_machine::{Config, DrivingState, Event, JobInProgress, StateMachine};
use super::{DeliveryReport, JobSnapshot, TelemetrySnapshot};
use crate::commands::expenses::{insert_expense_row, set_game_result};
use crate::commands::jobs::insert_job_row;
use crate::commands::sessions::insert_session_row;
use crate::db::models::{
    DrivingSession, GameResult, Job, JobExpense, JobSource, JobStatus, NewJob, NewSession,
    SessionKind, SessionStatus,
};
use crate::db::rollups::recompute_truck;

/// Sessions shorter than this are dropped (a free-roam session opened right after a delivery and
/// parked has no distance worth keeping). A session that belongs to a completed job is always kept.
pub const MIN_SESSION_KM: f64 = 0.05;

/// Settings keys holding the job in progress and the delivered job still in the frame (#4730), as
/// JSON. Written whenever they change, so a restart mid-delivery carries on.
pub const KEY_ACTIVE_JOB: &str = "telemetry_active_job";
pub const KEY_DELIVERED_JOB: &str = "telemetry_delivered_job";

/// What is saved between runs.
type Saved = (Option<JobInProgress>, Option<JobSnapshot>);

/// A row the pipeline wrote.
#[derive(Debug, Clone)]
pub enum Created {
    Session(DrivingSession),
    Job(Box<Job>),
    /// A cost paid with no job running (a job's own costs are written with the job).
    Expense(JobExpense),
}

pub struct Pipeline {
    machine: StateMachine,
    /// Session row of a delivery that was parked (not finished), so a `JobCompleted` that arrives
    /// without its own `SessionEnded` can still point at it.
    parked_job_session: Option<String>,
    /// What was last saved (or loaded), so unchanged state isn't written every tick.
    saved: Saved,
    /// When the job was last saved, to save fuel/damage-only changes less often.
    wear_saved_at: Option<NaiveDateTime>,
}

/// How often a job whose fuel or damage changed (and nothing else) is saved.
const WEAR_SAVE_EVERY: chrono::Duration = chrono::Duration::seconds(30);

impl Default for Pipeline {
    fn default() -> Self {
        Self::new(Config::default())
    }
}

/// A finished job: what was hauled, the odometer and time it ended, whether it was cancelled,
/// and what the game reported about it.
type FinishedJob = (JobInProgress, f32, NaiveDateTime, bool, GameResult);

/// The game's figures for a job that its "delivered" or "cancelled" event ended.
fn game_result(report: Option<DeliveryReport>, penalty: Option<i64>) -> GameResult {
    let mut game = GameResult {
        cancel_penalty: penalty,
        ..Default::default()
    };
    if let Some(r) = report {
        game.game_revenue = Some(r.revenue);
        game.earned_xp = Some(i64::from(r.earned_xp));
        game.cargo_damage = Some((f64::from(r.cargo_damage) * 1000.0).round() / 1000.0);
        game.game_distance_km = Some((f64::from(r.distance_km) * 10.0).round() / 10.0);
        game.auto_park = Some(r.auto_park);
        game.auto_load = Some(r.auto_load);
    }
    game
}

impl Pipeline {
    pub fn new(cfg: Config) -> Self {
        Self {
            machine: StateMachine::new(cfg),
            parked_job_session: None,
            saved: (None, None),
            wear_saved_at: None,
        }
    }

    /// A pipeline that carries on from the job saved by the last run, if any. A saved value that
    /// can't be read is ignored (the next tick overwrites it).
    pub async fn restored(pool: &SqlitePool) -> Self {
        let mut p = Self::default();
        let job = load::<JobInProgress>(pool, KEY_ACTIVE_JOB).await;
        let delivered = load::<JobSnapshot>(pool, KEY_DELIVERED_JOB).await;
        if job.is_some() {
            tracing::info!("resuming the job in progress from the last run");
        }
        p.machine.restore(job.clone(), delivered.clone());
        p.saved = (job, delivered);
        p
    }

    /// Forget the saved job (auto-capture switched off drops the drive in progress).
    pub async fn forget(pool: &SqlitePool) -> Result<()> {
        sqlx::query("DELETE FROM settings WHERE key IN (?, ?)")
            .bind(KEY_ACTIVE_JOB)
            .bind(KEY_DELIVERED_JOB)
            .execute(pool)
            .await
            .context("clear the saved job")?;
        Ok(())
    }

    pub fn state(&self) -> DrivingState {
        self.machine.state()
    }

    /// The job the machine is tracking (running or parked), if any.
    pub fn active_job(&self) -> Option<&super::state_machine::JobInProgress> {
        self.machine.active_job()
    }

    /// Feed one sample (wall-clock `at`; `None` = no frame) and write the resulting rows.
    pub async fn step(
        &mut self,
        pool: &SqlitePool,
        at: NaiveDateTime,
        frame: Option<&TelemetrySnapshot>,
    ) -> Result<Vec<Created>> {
        if let Some(f) = frame {
            self.machine.track_wear(f);
        }
        let events = self.machine.step(at, frame);
        let game_time = frame.and_then(|f| f.game_time);
        let created = self.apply(pool, events, game_time).await?;
        self.save(pool, at).await?;
        Ok(created)
    }

    /// Save the job in progress and the delivered job when either changed. Fuel and damage move
    /// on almost every frame, so a change in those alone is saved at most every
    /// [`WEAR_SAVE_EVERY`] (a restart loses at most that much of them).
    async fn save(&mut self, pool: &SqlitePool, at: NaiveDateTime) -> Result<()> {
        let now: Saved = (
            self.machine.active_job().cloned(),
            self.machine.delivered_job().cloned(),
        );
        let totals = |s: &Saved| s.0.as_ref().map(|j| j.wear.totals());
        let without_wear = |s: &Saved| {
            let mut s = s.clone();
            if let Some(j) = s.0.as_mut() {
                j.wear = Default::default();
            }
            s
        };
        if without_wear(&now) == without_wear(&self.saved) {
            let wear_moved = totals(&now) != totals(&self.saved);
            let due = self.wear_saved_at.is_none_or(|t| at - t >= WEAR_SAVE_EVERY);
            if !wear_moved || !due {
                return Ok(());
            }
        }
        store(pool, KEY_ACTIVE_JOB, now.0.as_ref()).await?;
        store(pool, KEY_DELIVERED_JOB, now.1.as_ref()).await?;
        self.saved = now;
        self.wear_saved_at = Some(at);
        Ok(())
    }

    /// Write the rows for `events`. `game_time` is the in-game clock of the sample that produced
    /// them, used to tell a late delivery from an on-time one.
    pub async fn apply(
        &mut self,
        pool: &SqlitePool,
        events: Vec<Event>,
        game_time: Option<NaiveDateTime>,
    ) -> Result<Vec<Created>> {
        let mut created = Vec::new();
        let mut events = events.into_iter().peekable();
        while let Some(event) = events.next() {
            match event {
                Event::SessionStarted { .. } => {}
                Event::JobStarted { .. } => {
                    self.parked_job_session = None;
                }
                Event::SessionEnded {
                    kind,
                    status,
                    start_odometer_km,
                    end_odometer_km,
                    started_at,
                    ended_at,
                    truck_make,
                    truck_model,
                } => {
                    let session = NewSession {
                        kind,
                        truck_make,
                        truck_model,
                        start_odometer: f64::from(start_odometer_km),
                        end_odometer: f64::from(end_odometer_km),
                        started_at: started_at.and_utc(),
                        ended_at: ended_at.and_utc(),
                        status,
                    };
                    // A delivery's session is followed by its JobCompleted (or JobCancelled):
                    // write them together.
                    let job: Option<FinishedJob> = match (kind, status, events.peek()) {
                        (
                            SessionKind::Job,
                            SessionStatus::Completed,
                            Some(Event::JobCompleted { .. }),
                        )
                        | (
                            SessionKind::Job,
                            SessionStatus::Cancelled,
                            Some(Event::JobCancelled { .. }),
                        ) => match events.next() {
                            Some(Event::JobCompleted {
                                job,
                                end_odometer_km,
                                ended_at,
                                report,
                            }) => Some((
                                job,
                                end_odometer_km,
                                ended_at,
                                false,
                                game_result(report, None),
                            )),
                            Some(Event::JobCancelled {
                                job,
                                end_odometer_km,
                                ended_at,
                                penalty,
                            }) => Some((
                                job,
                                end_odometer_km,
                                ended_at,
                                true,
                                game_result(None, penalty),
                            )),
                            _ => None,
                        },
                        _ => None,
                    };
                    self.write(pool, Some(session), job, game_time, &mut created)
                        .await?;
                }
                Event::JobCompleted {
                    job,
                    end_odometer_km,
                    ended_at,
                    report,
                } => {
                    // Finished while parked: the session ended earlier.
                    let game = game_result(report, None);
                    self.write(
                        pool,
                        None,
                        Some((job, end_odometer_km, ended_at, false, game)),
                        game_time,
                        &mut created,
                    )
                    .await?;
                }
                Event::JobCancelled {
                    job,
                    end_odometer_km,
                    ended_at,
                    penalty,
                } => {
                    // Cancelled while parked: the session ended earlier.
                    let game = game_result(None, penalty);
                    self.write(
                        pool,
                        None,
                        Some((job, end_odometer_km, ended_at, true, game)),
                        game_time,
                        &mut created,
                    )
                    .await?;
                }
                Event::Expense(expense) => {
                    let mut conn = pool.acquire().await.context("acquiring connection")?;
                    let row = insert_expense_row(&mut conn, None, &expense).await?;
                    created.push(Created::Expense(row));
                }
            }
        }
        Ok(created)
    }

    async fn write(
        &mut self,
        pool: &SqlitePool,
        session: Option<NewSession>,
        job: Option<FinishedJob>,
        game_time: Option<NaiveDateTime>,
        created: &mut Vec<Created>,
    ) -> Result<()> {
        let mut tx = pool
            .begin()
            .await
            .context("starting telemetry transaction")?;
        let mut trucks = Vec::new();

        let session_row = match session {
            Some(s) if job.is_some() || s.end_odometer - s.start_odometer >= MIN_SESSION_KM => {
                trucks.push((s.truck_make.clone(), s.truck_model.clone()));
                Some(insert_session_row(&mut tx, s).await?)
            }
            _ => None,
        };
        let job_row = match job {
            Some((j, end_odometer_km, ended_at, cancelled, game)) => {
                let session_id = session_row
                    .as_ref()
                    .map(|s| s.id.clone())
                    .or_else(|| self.parked_job_session.take());
                let new_job = new_job(
                    &j,
                    end_odometer_km,
                    ended_at,
                    game_time,
                    session_id,
                    cancelled,
                );
                trucks.push((new_job.truck_make.clone(), new_job.truck_model.clone()));
                let mut row = insert_job_row(&mut tx, new_job).await?;
                if game != GameResult::default() {
                    set_game_result(&mut tx, &row.id, &game).await?;
                    row.game = game;
                }
                for expense in &j.expenses {
                    insert_expense_row(&mut tx, Some(&row.id), expense).await?;
                }
                Some(row)
            }
            None => None,
        };
        tx.commit().await.context("committing telemetry rows")?;

        trucks.sort();
        trucks.dedup();
        for (make, model) in trucks {
            if let (Some(make), Some(model)) = (make, model) {
                recompute_truck(pool, &make, &model).await?;
            }
        }

        if let Some(s) = session_row {
            if job_row.is_none()
                && s.kind == SessionKind::Job
                && s.status == SessionStatus::Completed
            {
                self.parked_job_session = Some(s.id.clone());
            }
            created.push(Created::Session(s));
        }
        if let Some(j) = job_row {
            created.push(Created::Job(Box::new(j)));
        }
        Ok(())
    }
}

async fn load<T: serde::de::DeserializeOwned>(pool: &SqlitePool, key: &str) -> Option<T> {
    let raw = match crate::settings::get(pool, key).await {
        Ok(v) => v?,
        Err(e) => {
            tracing::warn!(error = %e, key, "could not read the saved job");
            return None;
        }
    };
    serde_json::from_str(&raw)
        .inspect_err(|e| tracing::warn!(error = %e, key, "ignoring an unreadable saved job"))
        .ok()
}

async fn store<T: serde::Serialize>(pool: &SqlitePool, key: &str, value: Option<&T>) -> Result<()> {
    match value {
        Some(v) => {
            let json = serde_json::to_string(v).context("serialise the job")?;
            crate::settings::put(pool, key, &json).await
        }
        None => sqlx::query("DELETE FROM settings WHERE key = ?")
            .bind(key)
            .execute(pool)
            .await
            .map(|_| ()),
    }
    .context("save the job in progress")
}

fn new_job(
    j: &JobInProgress,
    end_odometer_km: f32,
    ended_at: NaiveDateTime,
    game_time: Option<NaiveDateTime>,
    session_id: Option<String>,
    cancelled: bool,
) -> NewJob {
    let late = matches!((j.job.deadline, game_time), (Some(d), Some(now)) if now > d);
    let status = if cancelled {
        JobStatus::Cancelled
    } else if late {
        JobStatus::Late
    } else {
        JobStatus::Delivered
    };
    NewJob {
        session_id,
        truck_make: j.truck_make.clone(),
        truck_model: j.truck_model.clone(),
        source_city: j.job.source_city.clone().unwrap_or_default(),
        source_company: j.job.source_company.clone().unwrap_or_default(),
        dest_city: j.job.destination_city.clone().unwrap_or_default(),
        dest_company: j.job.destination_company.clone().unwrap_or_default(),
        cargo_id: j.job.cargo_id.clone().unwrap_or_default(),
        cargo_name: j.job.cargo_name.clone().unwrap_or_default(),
        cargo_mass_kg: f64::from(j.job.cargo_mass_kg),
        declared_income: i64::from(j.job.income),
        distance_km: f64::from((end_odometer_km - j.start_odometer_km).max(0.0)),
        duration_sec: (ended_at - j.started_at).num_seconds().max(0),
        // The odometer counts game kilometres, so the average speed needs game time (#4771).
        game_duration_min: match (j.started_game_time, game_time) {
            (Some(start), Some(end)) => Some((end - start).num_minutes().max(0)),
            _ => None,
        },
        // Added up frame by frame (#4771); none for a job restored from an older version's save.
        fuel_used_l: j
            .wear
            .tracked
            .then(|| (f64::from(j.wear.fuel_used_l) * 10.0).round() / 10.0),
        damage_delta: j
            .wear
            .tracked
            .then(|| (f64::from(j.wear.damage).min(1.0) * 1000.0).round() / 1000.0),
        started_at: j.started_at.and_utc(),
        completed_at: ended_at.and_utc(),
        status,
        source: JobSource::Telemetry,
        notes: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::rollups::truck_id;
    use crate::telemetry::{JobSnapshot, TruckSnapshot};
    use chrono::{Duration, NaiveDate};
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

    fn t(secs: i64) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 1, 1)
            .unwrap()
            .and_hms_opt(12, 0, 0)
            .unwrap()
            + Duration::seconds(secs)
    }

    fn job() -> JobSnapshot {
        JobSnapshot {
            income: 4_000,
            deadline: None,
            source_city: Some("Calais".into()),
            destination_city: Some("Paris".into()),
            source_company: Some("Tradeaux".into()),
            destination_company: Some("Kaarfor".into()),
            cargo_id: Some("apples".into()),
            cargo_name: Some("Apples".into()),
            cargo_mass_kg: 18_000.0,
            route_distance_km: 0.0,
            route_time_minutes: 0.0,
        }
    }

    fn snap(odo: f32, speed: f32, job: Option<JobSnapshot>) -> TelemetrySnapshot {
        TelemetrySnapshot {
            tick: 0,
            paused: false,
            game_time: None,
            plugin_revision: 10,
            game_version: (1, 55),
            truck: TruckSnapshot {
                make: Some("Scania".into()),
                make_id: Some("scania".into()),
                model: Some("S 730".into()),
                speed_kmh: speed,
                odometer_km: odo,
                engine_enabled: true,
                trailer_attached: job.is_some(),
                position: (odo * 1000.0, 0.0, 0.0),
                heading: 0.0,
                fuel_litres: 300.0,
                fuel_capacity_litres: 600.0,
                wear_chassis: 0.0,
                wear_trailer: 0.0,
            },
            job,
            events: Vec::new(),
        }
    }

    type Samples = Vec<(i64, Option<TelemetrySnapshot>)>;

    /// Feeds `(seconds, snapshot)` samples and collects what was written.
    async fn feed(pipeline: &mut Pipeline, pool: &SqlitePool, samples: Samples) -> Vec<Created> {
        let mut out = Vec::new();
        for (secs, frame) in samples {
            out.extend(pipeline.step(pool, t(secs), frame.as_ref()).await.unwrap());
        }
        out
    }

    fn sessions(created: &[Created]) -> Vec<&DrivingSession> {
        created
            .iter()
            .filter_map(|c| match c {
                Created::Session(s) => Some(s),
                _ => None,
            })
            .collect()
    }

    fn jobs(created: &[Created]) -> Vec<&Job> {
        created
            .iter()
            .filter_map(|c| match c {
                Created::Job(j) => Some(j.as_ref()),
                _ => None,
            })
            .collect()
    }

    /// Park, drive 10 km, haul 21 km, deliver, drive 5 km, then sit still past the idle limit.
    /// With `late`, the job's deadline is already behind the in-game clock when it is delivered.
    fn delivery_run(late: bool) -> Samples {
        let mut hauled = job();
        if late {
            hauled.deadline = Some(t(0));
        }
        let mut delivered = snap(136.0, 40.0, None);
        if late {
            delivered.game_time = Some(t(7_200));
        }
        vec![
            (0, Some(snap(100.0, 0.0, None))),
            (1, Some(snap(105.0, 80.0, None))),
            (2, Some(snap(110.0, 80.0, None))),
            (3, Some(snap(115.0, 80.0, Some(hauled.clone())))),
            (4, Some(snap(125.0, 80.0, Some(hauled.clone())))),
            (5, Some(snap(135.0, 80.0, Some(hauled)))),
            (6, Some(delivered)),
            (7, Some(snap(141.0, 80.0, None))),
            (8, Some(snap(141.0, 0.0, None))),
            (400, Some(snap(141.0, 0.0, None))),
        ]
    }

    #[tokio::test]
    async fn free_roam_then_delivery_then_free_roam_are_written() {
        let pool = test_pool().await;
        let mut p = Pipeline::default();
        let created = feed(&mut p, &pool, delivery_run(false)).await;

        let s = sessions(&created);
        let kinds: Vec<_> = s.iter().map(|s| (s.kind, s.status)).collect();
        assert_eq!(
            kinds,
            vec![
                (SessionKind::FreeRoam, SessionStatus::Completed),
                (SessionKind::Job, SessionStatus::Completed),
                (SessionKind::FreeRoam, SessionStatus::Completed),
            ]
        );

        let j = jobs(&created);
        assert_eq!(j.len(), 1);
        assert_eq!(j[0].session_id.as_deref(), Some(s[1].id.as_str()));
        assert_eq!(j[0].dest_city, "Paris");
        assert_eq!(j[0].declared_income, 4_000);
        assert_eq!(j[0].source, JobSource::Telemetry);
        assert_eq!(j[0].status, JobStatus::Delivered);

        // Every kilometre lands in exactly one session, and the rollup splits them by kind.
        let km: f64 = s.iter().map(|s| s.distance_km).sum();
        assert_eq!(km, 141.0 - 105.0);
        let (total, delivery, jobs_n): (f64, f64, i64) = sqlx::query_as(
            "SELECT total_distance_km, total_delivery_distance, total_jobs FROM trucks WHERE id = ?",
        )
        .bind(truck_id("Scania", "S 730"))
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(total, km);
        assert_eq!(delivery, s[1].distance_km);
        assert_eq!(jobs_n, 1);
    }

    #[tokio::test]
    async fn late_delivery_is_marked_late() {
        let pool = test_pool().await;
        let mut p = Pipeline::default();
        let created = feed(&mut p, &pool, delivery_run(true)).await;
        assert_eq!(jobs(&created)[0].status, JobStatus::Late);
    }

    #[tokio::test]
    async fn cancelled_job_gets_a_cancelled_row_linked_to_its_session() {
        let pool = test_pool().await;
        let mut p = Pipeline::default();
        let mut paused = snap(120.0, 0.0, Some(job()));
        paused.paused = true;
        let created = feed(
            &mut p,
            &pool,
            vec![
                (0, Some(snap(100.0, 0.0, None))),
                (1, Some(snap(105.0, 80.0, Some(job())))),
                (2, Some(snap(120.0, 80.0, Some(job())))),
                // Cancelled from the pause menu: paused on the frame before the job vanished.
                (3, Some(paused)),
                (4, Some(snap(120.0, 0.0, None))),
            ],
        )
        .await;
        let s = sessions(&created);
        assert_eq!(s.len(), 1);
        assert_eq!(
            (s[0].kind, s[0].status),
            (SessionKind::Job, SessionStatus::Cancelled)
        );
        let j = jobs(&created);
        assert_eq!(j.len(), 1);
        assert_eq!(j[0].status, JobStatus::Cancelled);
        assert_eq!(j[0].session_id.as_deref(), Some(s[0].id.as_str()));
        assert_eq!(j[0].dest_city, "Paris");
        assert_eq!(j[0].distance_km, 15.0);
        // A cancel is not a suspicious delivery.
        assert!(!j[0].flagged);

        // Not a delivery: the truck's km come from the session, and income stays out of the stats.
        let (delivery, jobs_n): (f64, i64) =
            sqlx::query_as("SELECT total_delivery_distance, total_jobs FROM trucks WHERE id = ?")
                .bind(truck_id("Scania", "S 730"))
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!((delivery, jobs_n), (15.0, 1));
    }

    #[tokio::test]
    async fn job_dropped_right_after_pickup_is_a_cancelled_row_without_a_flag() {
        let pool = test_pool().await;
        let mut p = Pipeline::default();
        // Hitched up, rolled 200 m, then the job was gone: cancelled, under 1 km, but not flagged.
        let created = feed(
            &mut p,
            &pool,
            vec![
                (0, Some(snap(100.0, 0.0, Some(job())))),
                (1, Some(snap(100.1, 10.0, Some(job())))),
                (2, Some(snap(100.2, 10.0, Some(job())))),
                (3, Some(snap(100.2, 0.0, None))),
            ],
        )
        .await;
        let j = jobs(&created);
        assert_eq!(j.len(), 1);
        assert_eq!(j[0].status, JobStatus::Cancelled);
        assert!(!j[0].flagged);
        assert_eq!(j[0].flag_reason, None);
    }

    #[tokio::test]
    async fn job_cancelled_while_parked_links_to_its_last_session() {
        let pool = test_pool().await;
        let mut p = Pipeline::default();
        // Park with the job past the idle limit (the session ends, the job stays), then cancel it
        // from the pause menu.
        let mut paused = snap(130.0, 0.0, Some(job()));
        paused.paused = true;
        let created = feed(
            &mut p,
            &pool,
            vec![
                (0, Some(snap(100.0, 0.0, Some(job())))),
                (1, Some(snap(105.0, 80.0, Some(job())))),
                (2, Some(snap(130.0, 80.0, Some(job())))),
                (3, Some(snap(130.0, 0.0, Some(job())))),
                (700, Some(paused)),
                (701, Some(snap(130.0, 0.0, None))),
            ],
        )
        .await;
        let s = sessions(&created);
        let j = jobs(&created);
        assert_eq!((s.len(), j.len()), (1, 1));
        assert_eq!(j[0].status, JobStatus::Cancelled);
        assert_eq!(j[0].session_id.as_deref(), Some(s[0].id.as_str()));
    }

    #[tokio::test]
    async fn zero_distance_sessions_are_not_written() {
        let pool = test_pool().await;
        let mut p = Pipeline::default();
        // Moves a metre, then sits for ten minutes: nothing worth keeping.
        let created = feed(
            &mut p,
            &pool,
            vec![
                (0, Some(snap(100.0, 0.0, None))),
                (1, Some(snap(100.001, 3.0, None))),
                (2, Some(snap(100.001, 0.0, None))),
                (600, Some(snap(100.001, 0.0, None))),
            ],
        )
        .await;
        assert!(created.is_empty());
    }

    #[tokio::test]
    async fn job_finished_while_parked_links_to_its_last_session() {
        let pool = test_pool().await;
        let mut p = Pipeline::default();
        // Park with the job for ten minutes (the session ends, the job stays), then it vanishes.
        let created = feed(
            &mut p,
            &pool,
            vec![
                (0, Some(snap(100.0, 0.0, Some(job())))),
                (1, Some(snap(105.0, 80.0, Some(job())))),
                (2, Some(snap(130.0, 80.0, Some(job())))),
                (3, Some(snap(130.0, 0.0, Some(job())))),
                (700, Some(snap(130.0, 0.0, Some(job())))),
                (701, Some(snap(130.0, 0.0, None))),
            ],
        )
        .await;
        let s = sessions(&created);
        let j = jobs(&created);
        assert_eq!((s.len(), j.len()), (1, 1));
        assert_eq!(j[0].session_id.as_deref(), Some(s[0].id.as_str()));
    }

    // ---- restart mid-delivery (#4730) ---------------------------------------------------------

    #[tokio::test]
    async fn a_restart_mid_delivery_keeps_the_whole_job() {
        let pool = test_pool().await;
        let mut p = Pipeline::restored(&pool).await;
        feed(
            &mut p,
            &pool,
            vec![
                (0, Some(snap(100.0, 0.0, None))),
                (1, Some(snap(101.0, 70.0, None))),
                (2, Some(snap(102.0, 70.0, Some(job())))), // picked up at 102 km
                (3, Some(snap(110.0, 70.0, Some(job())))),
            ],
        )
        .await;
        assert!(crate::settings::get(&pool, KEY_ACTIVE_JOB)
            .await
            .unwrap()
            .is_some());
        drop(p); // the app closes (an update, say) mid-delivery

        let mut p = Pipeline::restored(&pool).await;
        assert_eq!(p.active_job().unwrap().start_odometer_km, 102.0);
        let created = feed(
            &mut p,
            &pool,
            vec![
                (600, Some(snap(110.0, 0.0, Some(job())))),
                (601, Some(snap(111.0, 70.0, Some(job())))),
                (602, Some(snap(130.0, 70.0, Some(job())))),
                (603, Some(snap(130.0, 10.0, None))), // delivered
            ],
        )
        .await;
        let j = jobs(&created);
        assert_eq!(j.len(), 1);
        assert_eq!(j[0].distance_km, 28.0); // 102 -> 130, the km before the restart included
        assert_eq!(j[0].status, JobStatus::Delivered);
        assert!(crate::settings::get(&pool, KEY_ACTIVE_JOB)
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn a_saved_job_the_game_no_longer_shows_is_dropped_without_a_row() {
        let pool = test_pool().await;
        let mut p = Pipeline::restored(&pool).await;
        feed(
            &mut p,
            &pool,
            vec![
                (0, Some(snap(100.0, 0.0, None))),
                (1, Some(snap(101.0, 70.0, Some(job())))),
                (2, Some(snap(110.0, 70.0, Some(job())))),
            ],
        )
        .await;
        drop(p);

        let mut other = job();
        other.destination_city = Some("Lyon".into());
        other.income = 9_000;
        let mut p = Pipeline::restored(&pool).await;
        let created = feed(
            &mut p,
            &pool,
            vec![
                (600, Some(snap(500.0, 0.0, Some(other.clone())))),
                (601, Some(snap(501.0, 70.0, Some(other.clone())))),
            ],
        )
        .await;
        assert!(jobs(&created).is_empty());
        assert_eq!(
            p.active_job().unwrap().job.destination_city.as_deref(),
            Some("Lyon")
        );
        assert_eq!(p.active_job().unwrap().start_odometer_km, 501.0);
    }

    #[tokio::test]
    async fn the_delivered_owned_trailer_job_is_remembered_across_a_restart() {
        let pool = test_pool().await;
        let routed = |route: f32| {
            let mut j = job();
            j.route_distance_km = route;
            j
        };
        let mut p = Pipeline::restored(&pool).await;
        let created = feed(
            &mut p,
            &pool,
            vec![
                (0, Some(snap(100.0, 0.0, None))),
                (1, Some(snap(101.0, 70.0, Some(routed(20.0))))),
                (2, Some(snap(120.0, 20.0, Some(routed(0.4))))),
                (3, Some(snap(120.0, 0.0, Some(routed(0.0))))),
                (4, Some(snap(120.0, 0.0, Some(routed(0.0))))), // handed in, trailer still hitched
            ],
        )
        .await;
        assert_eq!(jobs(&created).len(), 1);
        drop(p);

        let mut p = Pipeline::restored(&pool).await;
        let created = feed(
            &mut p,
            &pool,
            vec![
                (600, Some(snap(120.0, 0.0, Some(routed(0.0))))),
                (601, Some(snap(121.0, 60.0, Some(routed(0.0))))),
            ],
        )
        .await;
        assert!(jobs(&created).is_empty());
        assert!(p.active_job().is_none());
        assert_eq!(p.state(), DrivingState::FreeRoam);
    }

    #[tokio::test]
    async fn an_unreadable_saved_job_is_ignored() {
        let pool = test_pool().await;
        crate::settings::put(&pool, KEY_ACTIVE_JOB, "{not json")
            .await
            .unwrap();
        let p = Pipeline::restored(&pool).await;
        assert!(p.active_job().is_none());
    }

    #[tokio::test]
    async fn forget_clears_the_saved_job() {
        let pool = test_pool().await;
        crate::settings::put(&pool, KEY_ACTIVE_JOB, "{}")
            .await
            .unwrap();
        crate::settings::put(&pool, KEY_DELIVERED_JOB, "{}")
            .await
            .unwrap();
        Pipeline::forget(&pool).await.unwrap();
        assert!(crate::settings::get(&pool, KEY_ACTIVE_JOB)
            .await
            .unwrap()
            .is_none());
        assert!(crate::settings::get(&pool, KEY_DELIVERED_JOB)
            .await
            .unwrap()
            .is_none());
    }

    /// #4771: the delivery Shay drove on Nov 15 (123.8 km on the odometer in ~11 real minutes,
    /// 104 in-game minutes). Per real hour that's ~660 km/h and it was flagged; per in-game
    /// hour it's ~71 km/h. Fuel and damage come from the frames too. #4756: the game's
    /// "delivered" event records it while the results screen is still up, with the game's
    /// figures and the toll paid on the way.
    #[tokio::test]
    async fn the_recorded_delivery_gets_game_time_speed_fuel_and_damage() {
        let pool = test_pool().await;
        let replay: crate::telemetry::replay::Replay =
            serde_json::from_str(include_str!("testdata/scs-recorded-delivery.json")).unwrap();
        let mut p = Pipeline::default();
        let mut created = Vec::new();
        for s in &replay.samples {
            let at = t((s.t * 1000.0) as i64 / 1000);
            created.extend(p.step(&pool, at, s.snapshot.as_ref()).await.unwrap());
        }
        // The recording ends on the results screen: no unpaused frame needed.
        assert!(
            replay
                .samples
                .last()
                .unwrap()
                .snapshot
                .as_ref()
                .unwrap()
                .paused
        );

        let j = jobs(&created);
        assert_eq!(j.len(), 1);
        let job = j[0];
        assert_eq!(job.status, JobStatus::Delivered);
        assert_eq!(job.game.game_revenue, Some(4478));
        assert_eq!(job.game.earned_xp, Some(320));
        assert_eq!(job.game.game_distance_km, Some(229.0));
        assert_eq!(job.game.cargo_damage, Some(0.0));
        assert_eq!(job.game.auto_park, Some(true));
        assert_eq!(job.game.auto_load, Some(true));
        assert_eq!(job.game.cancel_penalty, None);
        let stored = crate::commands::jobs::fetch_job(&pool, &job.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored.game, job.game);
        let costs = crate::commands::expenses::fetch_job_expenses(&pool, &job.id)
            .await
            .unwrap();
        assert_eq!(costs.len(), 1, "{costs:?}");
        assert_eq!(costs[0].kind, crate::db::models::ExpenseKind::Toll);
        assert_eq!(costs[0].amount, 19);
        assert!(p.active_job().is_none());
        assert!((job.distance_km - 123.8).abs() < 0.2, "{}", job.distance_km);
        let game_min = job.game_duration_min.expect("game time recorded");
        assert!((100..=106).contains(&game_min), "{game_min} game minutes");
        assert!(
            (60.0..80.0).contains(&job.avg_speed_kmh),
            "{} km/h",
            job.avg_speed_kmh
        );
        assert!(!job.flagged, "{:?}", job.flag_reason);
        let fuel = job.fuel_used_l.expect("fuel counted");
        assert!(fuel > 10.0 && fuel < 100.0, "{fuel} L");
        let damage = job.damage_delta.expect("damage counted");
        assert!((0.0..=1.0).contains(&damage), "{damage}");
    }

    fn with_events(
        mut s: TelemetrySnapshot,
        events: Vec<crate::telemetry::GameplayEvent>,
    ) -> TelemetrySnapshot {
        s.events = events;
        s
    }

    /// #4756: the game's events end the job (paused, on the results screen) and carry its
    /// figures; a toll on the way is stored with the job, one in free roam on its own.
    #[tokio::test]
    async fn gameplay_events_write_the_game_figures_and_costs() {
        use crate::telemetry::{DeliveryReport, GameplayEvent};
        let pool = test_pool().await;
        let mut p = Pipeline::default();
        let mut paused = snap(150.0, 0.0, None);
        paused.paused = true;
        let created = feed(
            &mut p,
            &pool,
            vec![
                (0, Some(snap(100.0, 0.0, None))),
                (
                    1,
                    Some(with_events(
                        snap(101.0, 60.0, None),
                        vec![GameplayEvent::Tollgate { amount: 7 }],
                    )),
                ),
                (2, Some(snap(102.0, 60.0, Some(job())))),
                (
                    3,
                    Some(with_events(
                        snap(120.0, 60.0, Some(job())),
                        vec![GameplayEvent::Fined {
                            amount: 300,
                            offence: Some("speeding_camera".into()),
                        }],
                    )),
                ),
                (4, Some(snap(150.0, 60.0, Some(job())))),
                (
                    5,
                    Some(with_events(
                        paused,
                        vec![GameplayEvent::JobDelivered(DeliveryReport {
                            revenue: 4_000,
                            earned_xp: 250,
                            cargo_damage: 0.0123,
                            distance_km: 48.04,
                            auto_park: false,
                            auto_load: true,
                        })],
                    )),
                ),
            ],
        )
        .await;
        let free: Vec<_> = created
            .iter()
            .filter_map(|c| match c {
                Created::Expense(e) => Some(e),
                _ => None,
            })
            .collect();
        assert_eq!(free.len(), 1);
        assert_eq!(free[0].amount, 7);
        assert_eq!(free[0].job_id, None);

        let j = jobs(&created);
        assert_eq!(j.len(), 1, "recorded while paused");
        let job = j[0];
        assert_eq!(job.status, JobStatus::Delivered);
        assert_eq!(job.game.game_revenue, Some(4_000));
        assert_eq!(job.game.earned_xp, Some(250));
        assert_eq!(job.game.cargo_damage, Some(0.012));
        assert_eq!(job.game.game_distance_km, Some(48.0));
        assert_eq!(job.game.auto_park, Some(false));
        assert_eq!(job.game.auto_load, Some(true));
        let costs = crate::commands::expenses::fetch_job_expenses(&pool, &job.id)
            .await
            .unwrap();
        assert_eq!(costs.len(), 1);
        assert_eq!(costs[0].kind, crate::db::models::ExpenseKind::Fine);
        assert_eq!(costs[0].detail.as_deref(), Some("speeding_camera"));

        // Deleting the job deletes its costs, not the free-roam one.
        crate::commands::jobs::remove_job(&pool, &job.id)
            .await
            .unwrap();
        let (left,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM expenses")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(left, 1);
    }

    #[tokio::test]
    async fn a_cancel_event_keeps_the_penalty() {
        use crate::telemetry::GameplayEvent;
        let pool = test_pool().await;
        let mut p = Pipeline::default();
        let created = feed(
            &mut p,
            &pool,
            vec![
                (0, Some(snap(100.0, 0.0, None))),
                (1, Some(snap(101.0, 60.0, Some(job())))),
                (2, Some(snap(130.0, 60.0, Some(job())))),
                (
                    3,
                    Some(with_events(
                        snap(131.0, 60.0, None),
                        vec![GameplayEvent::JobCancelled { penalty: 1_250 }],
                    )),
                ),
            ],
        )
        .await;
        let j = jobs(&created);
        assert_eq!(j.len(), 1);
        assert_eq!(j[0].status, JobStatus::Cancelled);
        assert_eq!(j[0].game.cancel_penalty, Some(1_250));
        assert_eq!(j[0].game.game_revenue, None);
    }

    /// A toll paid on the job is saved with the job in progress, so a restart keeps it.
    #[tokio::test]
    async fn a_jobs_costs_survive_a_restart() {
        use crate::telemetry::GameplayEvent;
        let pool = test_pool().await;
        let mut p = Pipeline::default();
        feed(
            &mut p,
            &pool,
            vec![
                (0, Some(snap(100.0, 0.0, None))),
                (1, Some(snap(101.0, 60.0, Some(job())))),
                (
                    2,
                    Some(with_events(
                        snap(110.0, 60.0, Some(job())),
                        vec![GameplayEvent::Tollgate { amount: 12 }],
                    )),
                ),
            ],
        )
        .await;
        drop(p);
        let mut p = Pipeline::restored(&pool).await;
        assert_eq!(p.active_job().unwrap().expenses.len(), 1);
        let created = feed(
            &mut p,
            &pool,
            vec![
                (100, Some(snap(111.0, 60.0, Some(job())))),
                (
                    101,
                    Some(with_events(
                        snap(160.0, 60.0, None),
                        vec![GameplayEvent::JobDelivered(Default::default())],
                    )),
                ),
            ],
        )
        .await;
        let j = jobs(&created);
        assert_eq!(j.len(), 1);
        let costs = crate::commands::expenses::fetch_job_expenses(&pool, &j[0].id)
            .await
            .unwrap();
        assert_eq!(costs.len(), 1);
        assert_eq!(costs[0].amount, 12);
    }

    #[test]
    fn wear_ignores_refuels_repairs_and_a_detached_trailer() {
        use crate::telemetry::state_machine::JobWear;
        let mut w = JobWear::default();
        let mut truck = snap(100.0, 50.0, Some(job())).truck;
        let mut frame = |fuel: f32, chassis: f32, trailer: f32, attached: bool| {
            truck.fuel_litres = fuel;
            truck.wear_chassis = chassis;
            truck.wear_trailer = trailer;
            truck.trailer_attached = attached;
            w.track(&truck);
        };
        frame(500.0, 0.10, 0.02, true);
        frame(490.0, 0.12, 0.03, true); // 10 L, +0.02 + 0.01
        frame(790.0, 0.12, 0.03, true); // refuelled: nothing used
        frame(780.0, 0.00, 0.00, false); // repaired, trailer off: 10 L
        frame(770.0, 0.01, 0.05, true); // +0.01 truck; the trailer is a new baseline
        let (fuel, damage, tracked) = w.totals();
        assert!(tracked);
        assert!((fuel - 30.0).abs() < 1e-3, "{fuel}");
        assert!((damage - 0.04).abs() < 1e-4, "{damage}");
    }

    #[tokio::test]
    async fn old_speed_flags_are_cleared_by_the_migration_sql() {
        // 0004 ran in test_pool already; replay its updates on rows written the old way.
        let pool = test_pool().await;
        let insert = |id: &'static str, km: f64, secs: i64, reason: &'static str| {
            let pool = pool.clone();
            async move {
                sqlx::query(
                    "INSERT INTO jobs (id, source_city, source_company, dest_city, dest_company,
                       cargo_id, cargo_name, cargo_mass_kg, declared_income, distance_km,
                       duration_sec, avg_speed_kmh, started_at, completed_at, status, source,
                       flagged, flag_reason)
                     VALUES (?, 'A', 'a', 'B', 'b', 'c', 'C', 1, 1, ?, ?, 0,
                       '2026-11-15T10:00:00Z', '2026-11-15T12:00:00Z', 'DELIVERED', 'TELEMETRY', 1, ?)",
                )
                .bind(id)
                .bind(km)
                .bind(secs)
                .bind(reason)
                .execute(&pool)
                .await
                .unwrap();
            }
        };
        insert("long", 2455.2, 8797, "Average speed over 150 km/h").await;
        insert(
            "short",
            0.5,
            1,
            "Distance under 1 km; Average speed over 150 km/h",
        )
        .await;
        insert("teleport", 4000.0, 3600, "Average speed over 150 km/h").await;
        insert("mine", 300.0, 3600, "looked odd").await;
        let sql = include_str!("../../migrations/0004_game_time_speed.sql");
        let updates: Vec<&str> = sql
            // Statements end a line; the flag reasons have "; " inside them.
            .split(";\n")
            .filter(|q| q.trim_start().lines().any(|l| l.starts_with("UPDATE jobs")))
            .collect();
        assert_eq!(updates.len(), 2);
        for q in updates {
            sqlx::query(q).execute(&pool).await.unwrap();
        }
        let rows: Vec<(String, bool, Option<String>)> =
            sqlx::query_as("SELECT id, flagged, flag_reason FROM jobs ORDER BY id")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(
            rows,
            vec![
                ("long".into(), false, None),
                ("mine".into(), true, Some("looked odd".into())),
                ("short".into(), true, Some("Distance under 1 km".into())),
                (
                    "teleport".into(),
                    true,
                    Some("Average speed over 150 km/h".into())
                ),
            ]
        );
    }
}

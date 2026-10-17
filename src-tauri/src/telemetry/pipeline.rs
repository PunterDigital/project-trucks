//! Telemetry pipeline: feeds samples to the state machine and writes what comes out to SQLite.
//!
//! Sessions and jobs go through the same `insert_*_row` helpers as the manual-entry commands. A
//! completed delivery writes its session and job in one transaction; the truck rollup is
//! recomputed once that commits. Cancelled deliveries keep their session (the kilometres still
//! happened) but get no job row.

use anyhow::{Context, Result};
use chrono::NaiveDateTime;
use sqlx::SqlitePool;

use super::state_machine::{Config, DrivingState, Event, JobInProgress, StateMachine};
use super::TelemetrySnapshot;
use crate::commands::jobs::insert_job_row;
use crate::commands::sessions::insert_session_row;
use crate::db::models::{
    DrivingSession, Job, JobSource, JobStatus, NewJob, NewSession, SessionKind, SessionStatus,
};
use crate::db::rollups::recompute_truck;

/// Sessions shorter than this are dropped (a free-roam session opened right after a delivery and
/// parked has no distance worth keeping). A session that belongs to a completed job is always kept.
pub const MIN_SESSION_KM: f64 = 0.05;

/// A row the pipeline wrote.
#[derive(Debug, Clone)]
pub enum Created {
    Session(DrivingSession),
    Job(Box<Job>),
}

pub struct Pipeline {
    machine: StateMachine,
    /// Session row of a delivery that was parked (not finished), so a `JobCompleted` that arrives
    /// without its own `SessionEnded` can still point at it.
    parked_job_session: Option<String>,
}

impl Default for Pipeline {
    fn default() -> Self {
        Self::new(Config::default())
    }
}

type FinishedJob = (JobInProgress, f32, NaiveDateTime);

impl Pipeline {
    pub fn new(cfg: Config) -> Self {
        Self {
            machine: StateMachine::new(cfg),
            parked_job_session: None,
        }
    }

    pub fn state(&self) -> DrivingState {
        self.machine.state()
    }

    /// Feed one sample (wall-clock `at`; `None` = no frame) and write the resulting rows.
    pub async fn step(
        &mut self,
        pool: &SqlitePool,
        at: NaiveDateTime,
        frame: Option<&TelemetrySnapshot>,
    ) -> Result<Vec<Created>> {
        let events = self.machine.step(at, frame);
        let game_time = frame.and_then(|f| f.game_time);
        self.apply(pool, events, game_time).await
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
                Event::JobStarted { .. } | Event::JobCancelled { .. } => {
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
                    // A delivery's session is followed by its JobCompleted: write them together.
                    let job: Option<FinishedJob> = if kind == SessionKind::Job
                        && status == SessionStatus::Completed
                        && matches!(events.peek(), Some(Event::JobCompleted { .. }))
                    {
                        match events.next() {
                            Some(Event::JobCompleted {
                                job,
                                end_odometer_km,
                                ended_at,
                            }) => Some((job, end_odometer_km, ended_at)),
                            _ => None,
                        }
                    } else {
                        None
                    };
                    self.write(pool, Some(session), job, game_time, &mut created)
                        .await?;
                }
                Event::JobCompleted {
                    job,
                    end_odometer_km,
                    ended_at,
                } => {
                    // Finished while parked: the session ended earlier.
                    self.write(
                        pool,
                        None,
                        Some((job, end_odometer_km, ended_at)),
                        game_time,
                        &mut created,
                    )
                    .await?;
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
            Some((j, end_odometer_km, ended_at)) => {
                let session_id = session_row
                    .as_ref()
                    .map(|s| s.id.clone())
                    .or_else(|| self.parked_job_session.take());
                let new_job = new_job(&j, end_odometer_km, ended_at, game_time, session_id);
                trucks.push((new_job.truck_make.clone(), new_job.truck_model.clone()));
                Some(insert_job_row(&mut tx, new_job).await?)
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

fn new_job(
    j: &JobInProgress,
    end_odometer_km: f32,
    ended_at: NaiveDateTime,
    game_time: Option<NaiveDateTime>,
    session_id: Option<String>,
) -> NewJob {
    let late = matches!((j.job.deadline, game_time), (Some(d), Some(now)) if now > d);
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
        fuel_used_l: None,
        damage_delta: None,
        started_at: j.started_at.and_utc(),
        completed_at: ended_at.and_utc(),
        status: if late {
            JobStatus::Late
        } else {
            JobStatus::Delivered
        },
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
                fuel_litres: 300.0,
                fuel_capacity_litres: 600.0,
                wear_chassis: 0.0,
                wear_trailer: 0.0,
            },
            job,
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
                Created::Job(_) => None,
            })
            .collect()
    }

    fn jobs(created: &[Created]) -> Vec<&Job> {
        created
            .iter()
            .filter_map(|c| match c {
                Created::Job(j) => Some(j.as_ref()),
                Created::Session(_) => None,
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
    async fn cancelled_job_keeps_its_session_but_gets_no_job_row() {
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
        assert!(jobs(&created).is_empty());
        let n: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM jobs")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(n.0, 0);
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
}

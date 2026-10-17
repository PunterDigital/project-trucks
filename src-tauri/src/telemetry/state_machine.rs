//! Driving state machine: turns a stream of telemetry samples into driving-session and job events.
//!
//! Pure logic, no I/O and no clock: the caller passes the wall-clock time of each sample, so every
//! rule is testable with hand-made sequences. Three states (`NotDriving`, `FreeRoam`, `Delivery`),
//! and every kilometre lands in exactly one session. Zero-distance sessions can happen (a
//! free-roam session opened right after a delivery, then parked); the writer in M2.4 decides
//! whether to keep them.
//!
//! Deviation from `CLAUDE_CODE_PLAN.md` task 2.3: the plugin's frame has neither the player's money
//! nor the destination's position, so "income jumped near the destination" cannot be checked.
//! A job counts as cancelled when the game was paused on the frame before the job disappeared
//! (cancelling happens in the pause menu; deliveries finish during play) or when the truck hardly
//! moved since pickup. That rule lives in [`StateMachine::job_outcome`] and still needs checking
//! against the real game.

use chrono::{Duration, NaiveDateTime};

use super::{JobSnapshot, TelemetrySnapshot};
use crate::db::models::{SessionKind, SessionStatus};

/// Tunable thresholds (the defaults follow the plan).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Config {
    /// A gap in samples strictly longer than this ends the session ("game disconnected").
    pub disconnect_after: Duration,
    /// No movement for this long ends the session ("parked + idle").
    pub idle_after: Duration,
    /// Speed (km/h, absolute) above which the truck counts as moving even if the odometer is flat.
    pub moving_speed_kmh: f32,
    /// A job that moved less than this since pickup cannot have been delivered.
    pub min_delivery_km: f32,
    /// A position change larger than this (world units) counts as activity for the idle timer
    /// (ferries and fast travel move the truck without touching the odometer).
    pub teleport_units: f32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            disconnect_after: Duration::seconds(60),
            idle_after: Duration::minutes(5),
            moving_speed_kmh: 0.5,
            min_delivery_km: 0.5,
            teleport_units: 0.5,
        }
    }
}

/// A job the machine is tracking, with where it started.
#[derive(Debug, Clone, PartialEq)]
pub struct JobInProgress {
    pub job: JobSnapshot,
    pub truck_make: Option<String>,
    pub truck_model: Option<String>,
    pub start_odometer_km: f32,
    pub started_at: NaiveDateTime,
}

impl JobInProgress {
    /// Same job as `other`? The game has no job id, so compare what the plugin exposes.
    fn same_job(&self, other: &JobSnapshot) -> bool {
        let a = &self.job;
        a.income == other.income
            && a.source_company == other.source_company
            && a.destination_company == other.destination_company
            && a.destination_city == other.destination_city
            && a.cargo_id == other.cargo_id
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    SessionStarted {
        kind: SessionKind,
        at: NaiveDateTime,
        odometer_km: f32,
    },
    SessionEnded {
        kind: SessionKind,
        status: SessionStatus,
        start_odometer_km: f32,
        end_odometer_km: f32,
        started_at: NaiveDateTime,
        ended_at: NaiveDateTime,
        truck_make: Option<String>,
        truck_model: Option<String>,
    },
    /// A new job began (not emitted again when a parked job is resumed).
    JobStarted { job: JobInProgress },
    /// Accompanies a `SessionEnded { kind: Job }`, except when the job finished while the machine
    /// was `NotDriving` (then the job's last session already ended).
    JobCompleted {
        job: JobInProgress,
        end_odometer_km: f32,
        ended_at: NaiveDateTime,
    },
    JobCancelled {
        job: JobInProgress,
        end_odometer_km: f32,
        ended_at: NaiveDateTime,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrivingState {
    NotDriving,
    FreeRoam,
    Delivery,
}

#[derive(Debug, Clone)]
struct Session {
    kind: SessionKind,
    start_odometer_km: f32,
    started_at: NaiveDateTime,
    truck_make: Option<String>,
    truck_model: Option<String>,
}

/// The previous sample, for movement and gap checks.
#[derive(Debug, Clone, Copy)]
struct Last {
    at: NaiveDateTime,
    odometer_km: f32,
    position: (f32, f32, f32),
    paused: bool,
}

pub struct StateMachine {
    cfg: Config,
    session: Option<Session>,
    /// The job being driven, kept across a parked/disconnected break.
    job: Option<JobInProgress>,
    last: Option<Last>,
    /// When the truck last stopped being active, while it is idle.
    idle_since: Option<NaiveDateTime>,
}

impl Default for StateMachine {
    fn default() -> Self {
        Self::new(Config::default())
    }
}

impl StateMachine {
    pub fn new(cfg: Config) -> Self {
        Self {
            cfg,
            session: None,
            job: None,
            last: None,
            idle_since: None,
        }
    }

    pub fn state(&self) -> DrivingState {
        match &self.session {
            None => DrivingState::NotDriving,
            Some(s) if s.kind == SessionKind::FreeRoam => DrivingState::FreeRoam,
            Some(_) => DrivingState::Delivery,
        }
    }

    /// The job that is running or parked, if any.
    pub fn active_job(&self) -> Option<&JobInProgress> {
        self.job.as_ref()
    }

    /// Feed one sample taken at wall-clock time `at`. `None` means no frame arrived (game closed or
    /// plugin missing); call it on the poll interval so disconnects are noticed.
    pub fn step(&mut self, at: NaiveDateTime, frame: Option<&TelemetrySnapshot>) -> Vec<Event> {
        let mut events = Vec::new();
        self.check_gap(at, &mut events);
        let Some(s) = frame else {
            return events;
        };

        let prev = self.last;
        let moved = prev.is_some()
            && (prev.is_some_and(|p| s.truck.odometer_km != p.odometer_km)
                || s.truck.speed_kmh.abs() > self.cfg.moving_speed_kmh);
        let teleported =
            prev.is_some_and(|p| distance(p.position, s.truck.position) > self.cfg.teleport_units);
        let prev_paused = prev.is_some_and(|p| p.paused);
        self.last = Some(Last {
            at,
            odometer_km: s.truck.odometer_km,
            position: s.truck.position,
            paused: s.paused,
        });

        if s.paused {
            // Pausing is not idling, and nothing changes in the world meanwhile.
            self.idle_since = None;
            return events;
        }

        if moved || teleported {
            self.idle_since = None;
        } else {
            self.idle_since.get_or_insert(at);
        }

        let job_now = current_job(s);
        match self.state() {
            DrivingState::NotDriving => {
                self.step_parked(at, s, moved, job_now, prev_paused, &mut events)
            }
            DrivingState::FreeRoam => {
                if let Some(j) = job_now {
                    self.end_session(
                        at,
                        s.truck.odometer_km,
                        SessionStatus::Completed,
                        &mut events,
                    );
                    self.begin_delivery(at, s, j, &mut events);
                }
            }
            DrivingState::Delivery => {
                let same =
                    job_now.is_some_and(|j| self.job.as_ref().is_some_and(|a| a.same_job(j)));
                if !same {
                    let (status, job_event) = self.finish_job(at, s.truck.odometer_km, prev_paused);
                    self.end_session(at, s.truck.odometer_km, status, &mut events);
                    events.extend(job_event);
                    match job_now {
                        Some(j) => self.begin_delivery(at, s, j, &mut events),
                        None => self.begin_free_roam(at, s, &mut events),
                    }
                }
            }
        }

        // Parked too long ends whatever is running (ended when the truck stopped, not when we noticed).
        if self.session.is_some() {
            if let Some(since) = self.idle_since {
                if at - since >= self.cfg.idle_after {
                    self.end_session_at(
                        since,
                        s.truck.odometer_km,
                        SessionStatus::Completed,
                        &mut events,
                    );
                }
            }
        }
        events
    }

    /// `NotDriving`: wait for the truck to move, finishing any parked job that vanished meanwhile.
    fn step_parked(
        &mut self,
        at: NaiveDateTime,
        s: &TelemetrySnapshot,
        moved: bool,
        job_now: Option<&JobSnapshot>,
        prev_paused: bool,
        events: &mut Vec<Event>,
    ) {
        if let Some(parked) = &self.job {
            if !job_now.is_some_and(|j| parked.same_job(j)) {
                events.extend(self.finish_job(at, s.truck.odometer_km, prev_paused).1);
            }
        }
        if !moved {
            return;
        }
        match job_now {
            Some(j) => self.begin_delivery(at, s, j, events),
            None => self.begin_free_roam(at, s, events),
        }
    }

    /// A sample gap longer than `disconnect_after` means the game went away: end the session at the
    /// last sample we saw.
    fn check_gap(&mut self, at: NaiveDateTime, events: &mut Vec<Event>) {
        let (Some(last), true) = (self.last, self.session.is_some()) else {
            return;
        };
        if at - last.at > self.cfg.disconnect_after {
            self.end_session_at(last.at, last.odometer_km, SessionStatus::Completed, events);
        }
    }

    fn begin_free_roam(
        &mut self,
        at: NaiveDateTime,
        s: &TelemetrySnapshot,
        events: &mut Vec<Event>,
    ) {
        self.open_session(SessionKind::FreeRoam, at, s, events);
    }

    fn begin_delivery(
        &mut self,
        at: NaiveDateTime,
        s: &TelemetrySnapshot,
        job: &JobSnapshot,
        events: &mut Vec<Event>,
    ) {
        let resumed = self.job.as_ref().is_some_and(|a| a.same_job(job));
        if !resumed {
            let started = JobInProgress {
                job: job.clone(),
                truck_make: s.truck.make.clone(),
                truck_model: s.truck.model.clone(),
                start_odometer_km: s.truck.odometer_km,
                started_at: at,
            };
            events.push(Event::JobStarted {
                job: started.clone(),
            });
            self.job = Some(started);
        }
        self.open_session(SessionKind::Job, at, s, events);
    }

    fn open_session(
        &mut self,
        kind: SessionKind,
        at: NaiveDateTime,
        s: &TelemetrySnapshot,
        events: &mut Vec<Event>,
    ) {
        self.session = Some(Session {
            kind,
            start_odometer_km: s.truck.odometer_km,
            started_at: at,
            truck_make: s.truck.make.clone(),
            truck_model: s.truck.model.clone(),
        });
        events.push(Event::SessionStarted {
            kind,
            at,
            odometer_km: s.truck.odometer_km,
        });
    }

    fn end_session(
        &mut self,
        at: NaiveDateTime,
        odometer_km: f32,
        status: SessionStatus,
        events: &mut Vec<Event>,
    ) {
        self.end_session_at(at, odometer_km, status, events);
    }

    fn end_session_at(
        &mut self,
        at: NaiveDateTime,
        odometer_km: f32,
        status: SessionStatus,
        events: &mut Vec<Event>,
    ) {
        if let Some(s) = self.session.take() {
            events.push(Event::SessionEnded {
                kind: s.kind,
                status,
                start_odometer_km: s.start_odometer_km,
                end_odometer_km: odometer_km,
                started_at: s.started_at,
                ended_at: at,
                truck_make: s.truck_make,
                truck_model: s.truck_model,
            });
        }
        self.idle_since = None;
    }

    /// Close the tracked job as completed or cancelled and return which.
    fn finish_job(
        &mut self,
        at: NaiveDateTime,
        odometer_km: f32,
        paused_before: bool,
    ) -> (SessionStatus, Option<Event>) {
        let Some(job) = self.job.take() else {
            return (SessionStatus::Completed, None);
        };
        let status = self.job_outcome(&job, odometer_km, paused_before);
        let event = match status {
            SessionStatus::Completed => Event::JobCompleted {
                job,
                end_odometer_km: odometer_km,
                ended_at: at,
            },
            SessionStatus::Cancelled => Event::JobCancelled {
                job,
                end_odometer_km: odometer_km,
                ended_at: at,
            },
        };
        (status, Some(event))
    }

    /// Delivered or cancelled? See the module docs for why this is a heuristic.
    fn job_outcome(
        &self,
        job: &JobInProgress,
        odometer_km: f32,
        paused_before: bool,
    ) -> SessionStatus {
        if paused_before || odometer_km - job.start_odometer_km < self.cfg.min_delivery_km {
            SessionStatus::Cancelled
        } else {
            SessionStatus::Completed
        }
    }
}

/// The job in this sample, if the truck is hauling one: trailer hitched and a destination set.
fn current_job(s: &TelemetrySnapshot) -> Option<&JobSnapshot> {
    s.job.as_ref().filter(|j| {
        s.truck.trailer_attached && j.destination_city.as_deref().is_some_and(|c| !c.is_empty())
    })
}

fn distance(a: (f32, f32, f32), b: (f32, f32, f32)) -> f32 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2) + (a.2 - b.2).powi(2)).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telemetry::TruckSnapshot;
    use chrono::NaiveDate;

    fn t(secs: i64) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 10, 17)
            .unwrap()
            .and_hms_opt(8, 0, 0)
            .unwrap()
            + Duration::seconds(secs)
    }

    fn job(dest: &str, income: i32) -> JobSnapshot {
        JobSnapshot {
            income,
            deadline: None,
            source_city: Some("Calais".into()),
            destination_city: Some(dest.into()),
            source_company: Some("Euroports".into()),
            destination_company: Some("Posped".into()),
            cargo_id: Some("beer".into()),
            cargo_name: Some("Beer".into()),
            cargo_mass_kg: 12_000.0,
        }
    }

    /// A sample: odometer, speed, optional job (with trailer hitched).
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
                model: Some("S".into()),
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

    fn driving(odo: f32) -> TelemetrySnapshot {
        snap(odo, 80.0, None)
    }
    fn hauling(odo: f32) -> TelemetrySnapshot {
        snap(odo, 80.0, Some(job("Paris", 4_000)))
    }
    fn parked(odo: f32) -> TelemetrySnapshot {
        snap(odo, 0.0, None)
    }
    fn parked_hauling(odo: f32) -> TelemetrySnapshot {
        snap(odo, 0.0, Some(job("Paris", 4_000)))
    }

    /// Tests that are not about disconnects use sparse timestamps, so switch the gap rule off.
    fn machine() -> StateMachine {
        StateMachine::new(Config {
            disconnect_after: Duration::days(30),
            ..Config::default()
        })
    }

    fn feed(m: &mut StateMachine, secs: i64, s: &TelemetrySnapshot) -> Vec<Event> {
        m.step(t(secs), Some(s))
    }

    fn ended(events: &[Event]) -> Vec<(SessionKind, SessionStatus, f32)> {
        events
            .iter()
            .filter_map(|e| match e {
                Event::SessionEnded {
                    kind,
                    status,
                    start_odometer_km,
                    end_odometer_km,
                    ..
                } => Some((*kind, *status, end_odometer_km - start_odometer_km)),
                _ => None,
            })
            .collect()
    }

    fn names(events: &[Event]) -> Vec<&'static str> {
        events
            .iter()
            .map(|e| match e {
                Event::SessionStarted {
                    kind: SessionKind::FreeRoam,
                    ..
                } => "start:free",
                Event::SessionStarted {
                    kind: SessionKind::Job,
                    ..
                } => "start:job",
                Event::SessionEnded {
                    kind: SessionKind::FreeRoam,
                    ..
                } => "end:free",
                Event::SessionEnded {
                    kind: SessionKind::Job,
                    ..
                } => "end:job",
                Event::JobStarted { .. } => "job-started",
                Event::JobCompleted { .. } => "job-completed",
                Event::JobCancelled { .. } => "job-cancelled",
            })
            .collect()
    }

    /// A driver mid-delivery: free roam, then a job picked up at 120 km.
    fn on_a_job() -> StateMachine {
        let mut m = machine();
        feed(&mut m, 0, &driving(100.0));
        feed(&mut m, 1, &driving(100.5));
        feed(&mut m, 60, &hauling(120.0));
        assert_eq!(m.state(), DrivingState::Delivery);
        m
    }

    // ---- not driving -------------------------------------------------------------------------

    #[test]
    fn starts_not_driving() {
        assert_eq!(StateMachine::default().state(), DrivingState::NotDriving);
    }

    #[test]
    fn disconnected_game_emits_nothing() {
        let mut m = machine();
        assert!(m.step(t(0), None).is_empty());
        assert!(m.step(t(600), None).is_empty());
        assert_eq!(m.state(), DrivingState::NotDriving);
    }

    #[test]
    fn parked_truck_stays_not_driving() {
        let mut m = machine();
        for i in 0..10 {
            assert!(feed(&mut m, i * 60, &parked(50.0)).is_empty());
        }
        assert_eq!(m.state(), DrivingState::NotDriving);
    }

    #[test]
    fn first_frame_alone_starts_nothing() {
        let mut m = machine();
        assert!(feed(&mut m, 0, &driving(10.0)).is_empty());
    }

    // ---- free roam ---------------------------------------------------------------------------

    #[test]
    fn odometer_change_starts_free_roam() {
        let mut m = machine();
        feed(&mut m, 0, &parked(10.0));
        let ev = feed(&mut m, 1, &snap(10.2, 0.0, None));
        assert_eq!(names(&ev), ["start:free"]);
        assert_eq!(m.state(), DrivingState::FreeRoam);
    }

    #[test]
    fn speed_alone_starts_free_roam() {
        let mut m = machine();
        feed(&mut m, 0, &parked(10.0));
        let ev = feed(&mut m, 1, &snap(10.0, 30.0, None));
        assert_eq!(names(&ev), ["start:free"]);
    }

    #[test]
    fn free_roam_start_event_carries_odometer_and_time() {
        let mut m = machine();
        feed(&mut m, 0, &parked(10.0));
        let ev = feed(&mut m, 5, &driving(10.5));
        assert_eq!(
            ev,
            [Event::SessionStarted {
                kind: SessionKind::FreeRoam,
                at: t(5),
                odometer_km: 10.5
            }]
        );
    }

    #[test]
    fn continuing_to_drive_emits_nothing() {
        let mut m = machine();
        feed(&mut m, 0, &driving(10.0));
        feed(&mut m, 1, &driving(10.5));
        assert!(feed(&mut m, 2, &driving(11.0)).is_empty());
    }

    #[test]
    fn free_roam_only_session_ends_when_parked_for_five_minutes() {
        let mut m = machine();
        feed(&mut m, 0, &driving(10.0));
        feed(&mut m, 1, &driving(10.5));
        feed(&mut m, 600, &driving(40.0));
        feed(&mut m, 601, &parked(40.0));
        let ev = feed(&mut m, 601 + 300, &parked(40.0));
        assert_eq!(
            ended(&ev),
            [(SessionKind::FreeRoam, SessionStatus::Completed, 29.5)]
        );
        assert_eq!(m.state(), DrivingState::NotDriving);
    }

    #[test]
    fn idle_session_ends_at_the_moment_the_truck_stopped() {
        let mut m = machine();
        feed(&mut m, 0, &driving(10.0));
        feed(&mut m, 1, &driving(11.0));
        feed(&mut m, 100, &parked(12.0));
        feed(&mut m, 130, &parked(12.0));
        let ev = feed(&mut m, 431, &parked(12.0));
        match &ev[0] {
            Event::SessionEnded {
                ended_at,
                started_at,
                ..
            } => {
                assert_eq!(*ended_at, t(130)); // first still frame after the last movement
                assert_eq!(*started_at, t(1));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn idle_just_under_five_minutes_keeps_the_session() {
        let mut m = machine();
        feed(&mut m, 0, &driving(10.0));
        feed(&mut m, 1, &driving(11.0));
        feed(&mut m, 2, &parked(11.0));
        assert!(feed(&mut m, 2 + 299, &parked(11.0)).is_empty());
        assert_eq!(m.state(), DrivingState::FreeRoam);
    }

    #[test]
    fn idle_for_exactly_five_minutes_ends_the_session() {
        let mut m = machine();
        feed(&mut m, 0, &driving(10.0));
        feed(&mut m, 1, &driving(11.0));
        feed(&mut m, 2, &parked(11.0));
        let ev = feed(&mut m, 2 + 300, &parked(11.0));
        assert_eq!(names(&ev), ["end:free"]);
    }

    #[test]
    fn moving_again_resets_the_idle_timer() {
        let mut m = machine();
        feed(&mut m, 0, &driving(10.0));
        feed(&mut m, 1, &driving(11.0));
        feed(&mut m, 2, &parked(11.0));
        feed(&mut m, 200, &parked(11.0));
        feed(&mut m, 201, &driving(11.2)); // creeps forward
        feed(&mut m, 202, &parked(11.2));
        assert!(feed(&mut m, 202 + 299, &parked(11.2)).is_empty());
        assert_eq!(m.state(), DrivingState::FreeRoam);
    }

    #[test]
    fn paused_frames_do_not_count_as_idle() {
        let mut m = machine();
        feed(&mut m, 0, &driving(10.0));
        feed(&mut m, 1, &driving(11.0));
        let mut paused = parked(11.0);
        paused.paused = true;
        for i in 0..20 {
            assert!(feed(&mut m, 2 + i * 60, &paused).is_empty());
        }
        assert_eq!(m.state(), DrivingState::FreeRoam);
        // The idle clock starts only once play resumes.
        assert!(feed(&mut m, 2000, &parked(11.0)).is_empty());
        assert_eq!(names(&feed(&mut m, 2300, &parked(11.0))), ["end:free"]);
    }

    #[test]
    fn restarting_after_idle_opens_a_new_free_roam_session() {
        let mut m = machine();
        feed(&mut m, 0, &driving(10.0));
        feed(&mut m, 1, &driving(11.0));
        feed(&mut m, 2, &parked(11.0));
        feed(&mut m, 400, &parked(11.0));
        let ev = feed(&mut m, 401, &driving(11.1));
        assert_eq!(names(&ev), ["start:free"]);
    }

    // ---- delivery ----------------------------------------------------------------------------

    #[test]
    fn hitching_a_job_closes_free_roam_and_opens_a_delivery() {
        let mut m = machine();
        feed(&mut m, 0, &driving(100.0));
        feed(&mut m, 1, &driving(100.5));
        let ev = feed(&mut m, 60, &hauling(120.0));
        assert_eq!(names(&ev), ["end:free", "job-started", "start:job"]);
        assert_eq!(
            ended(&ev),
            [(SessionKind::FreeRoam, SessionStatus::Completed, 19.5)]
        );
        assert_eq!(m.state(), DrivingState::Delivery);
    }

    #[test]
    fn job_started_event_carries_the_job_details() {
        let mut m = machine();
        feed(&mut m, 0, &driving(100.0));
        feed(&mut m, 1, &driving(100.5));
        let ev = feed(&mut m, 60, &hauling(120.0));
        let Event::JobStarted { job } = &ev[1] else {
            panic!("no JobStarted")
        };
        assert_eq!(job.job.income, 4_000);
        assert_eq!(job.job.destination_city.as_deref(), Some("Paris"));
        assert_eq!(job.start_odometer_km, 120.0);
        assert_eq!(job.started_at, t(60));
        assert_eq!(job.truck_make.as_deref(), Some("Scania"));
    }

    #[test]
    fn trailer_without_a_destination_is_not_a_job() {
        let mut m = machine();
        feed(&mut m, 0, &driving(100.0));
        feed(&mut m, 1, &driving(100.5));
        let mut s = hauling(101.0);
        s.job.as_mut().unwrap().destination_city = Some(String::new());
        assert!(feed(&mut m, 2, &s).is_empty());
        assert_eq!(m.state(), DrivingState::FreeRoam);
    }

    #[test]
    fn job_data_without_a_trailer_is_not_a_job() {
        let mut m = machine();
        feed(&mut m, 0, &driving(100.0));
        feed(&mut m, 1, &driving(100.5));
        let mut s = hauling(101.0);
        s.truck.trailer_attached = false;
        assert!(feed(&mut m, 2, &s).is_empty());
        assert_eq!(m.state(), DrivingState::FreeRoam);
    }

    #[test]
    fn driving_with_the_job_emits_nothing() {
        let mut m = on_a_job();
        assert!(feed(&mut m, 120, &hauling(140.0)).is_empty());
        assert!(feed(&mut m, 180, &hauling(160.0)).is_empty());
    }

    #[test]
    fn clean_delivery_completes_the_job_and_reopens_free_roam() {
        let mut m = on_a_job();
        feed(&mut m, 600, &hauling(320.0));
        let ev = feed(&mut m, 601, &driving(320.0));
        assert_eq!(names(&ev), ["end:job", "job-completed", "start:free"]);
        assert_eq!(
            ended(&ev),
            [(SessionKind::Job, SessionStatus::Completed, 200.0)]
        );
        assert_eq!(m.state(), DrivingState::FreeRoam);
        assert!(m.active_job().is_none());
    }

    #[test]
    fn completed_event_has_the_job_and_distance() {
        let mut m = on_a_job();
        feed(&mut m, 600, &hauling(320.0));
        let ev = feed(&mut m, 601, &driving(320.0));
        let Event::JobCompleted {
            job,
            end_odometer_km,
            ended_at,
        } = &ev[1]
        else {
            panic!()
        };
        assert_eq!(job.job.cargo_name.as_deref(), Some("Beer"));
        assert_eq!(*end_odometer_km - job.start_odometer_km, 200.0);
        assert_eq!(*ended_at, t(601));
    }

    #[test]
    fn job_session_records_the_truck() {
        let mut m = on_a_job();
        feed(&mut m, 600, &hauling(320.0));
        let ev = feed(&mut m, 601, &driving(320.0));
        let Event::SessionEnded {
            truck_make,
            truck_model,
            started_at,
            ended_at,
            ..
        } = &ev[0]
        else {
            panic!()
        };
        assert_eq!(truck_make.as_deref(), Some("Scania"));
        assert_eq!(truck_model.as_deref(), Some("S"));
        assert_eq!((*started_at, *ended_at), (t(60), t(601)));
    }

    #[test]
    fn cancelling_from_the_pause_menu_cancels_the_job() {
        let mut m = on_a_job();
        feed(&mut m, 300, &hauling(180.0));
        let mut menu = hauling(180.0);
        menu.paused = true;
        feed(&mut m, 301, &menu);
        let ev = feed(&mut m, 320, &driving(180.0));
        assert_eq!(names(&ev), ["end:job", "job-cancelled", "start:free"]);
        assert_eq!(
            ended(&ev),
            [(SessionKind::Job, SessionStatus::Cancelled, 60.0)]
        );
    }

    #[test]
    fn dropping_a_job_right_after_pickup_is_a_cancel() {
        let mut m = on_a_job();
        feed(&mut m, 70, &hauling(120.1));
        let ev = feed(&mut m, 80, &driving(120.2));
        assert_eq!(names(&ev), ["end:job", "job-cancelled", "start:free"]);
    }

    #[test]
    fn the_km_of_a_cancelled_job_still_count() {
        let mut m = on_a_job();
        feed(&mut m, 300, &hauling(180.0));
        let mut menu = hauling(180.0);
        menu.paused = true;
        feed(&mut m, 301, &menu);
        let ev = feed(&mut m, 320, &driving(180.0));
        let Event::SessionEnded {
            start_odometer_km,
            end_odometer_km,
            ..
        } = &ev[0]
        else {
            panic!()
        };
        assert_eq!((*start_odometer_km, *end_odometer_km), (120.0, 180.0));
    }

    #[test]
    fn free_roam_between_two_jobs_is_its_own_session() {
        let mut m = on_a_job();
        feed(&mut m, 600, &hauling(320.0));
        feed(&mut m, 601, &driving(320.0)); // delivered, free roam opens at 320
        feed(&mut m, 700, &driving(345.0));
        let ev = feed(&mut m, 800, &snap(350.0, 80.0, Some(job("Berlin", 6_500))));
        assert_eq!(names(&ev), ["end:free", "job-started", "start:job"]);
        assert_eq!(
            ended(&ev),
            [(SessionKind::FreeRoam, SessionStatus::Completed, 30.0)]
        );
        assert_eq!(
            m.active_job().unwrap().job.destination_city.as_deref(),
            Some("Berlin")
        );
    }

    #[test]
    fn every_km_lands_in_exactly_one_session() {
        let mut m = machine();
        let mut all = Vec::new();
        all.extend(feed(&mut m, 0, &driving(0.0)));
        all.extend(feed(&mut m, 1, &driving(10.0)));
        all.extend(feed(&mut m, 60, &hauling(50.0)));
        all.extend(feed(&mut m, 600, &hauling(250.0)));
        all.extend(feed(&mut m, 601, &driving(250.0)));
        all.extend(feed(&mut m, 700, &driving(280.0)));
        all.extend(feed(&mut m, 701, &parked(280.0)));
        all.extend(feed(&mut m, 1100, &parked(280.0)));
        let total: f32 = ended(&all).iter().map(|e| e.2).sum();
        assert_eq!(total, 270.0); // the first sample only arms the movement check
        assert_eq!(m.state(), DrivingState::NotDriving);
    }

    #[test]
    fn back_to_back_jobs_in_one_frame_swap_cleanly() {
        let mut m = on_a_job();
        feed(&mut m, 600, &hauling(320.0));
        let ev = feed(&mut m, 601, &snap(320.0, 20.0, Some(job("Berlin", 6_500))));
        assert_eq!(
            names(&ev),
            ["end:job", "job-completed", "job-started", "start:job"]
        );
        assert_eq!(m.state(), DrivingState::Delivery);
        assert_eq!(m.active_job().unwrap().start_odometer_km, 320.0);
    }

    #[test]
    fn back_to_back_jobs_with_a_gap_swap_through_free_roam() {
        let mut m = on_a_job();
        feed(&mut m, 600, &hauling(320.0));
        let first = feed(&mut m, 601, &driving(320.0));
        let second = feed(&mut m, 610, &snap(320.3, 20.0, Some(job("Berlin", 6_500))));
        assert_eq!(names(&first), ["end:job", "job-completed", "start:free"]);
        assert_eq!(names(&second), ["end:free", "job-started", "start:job"]);
    }

    #[test]
    fn a_different_job_replaces_the_current_one_even_if_the_destination_matches() {
        let mut m = on_a_job();
        feed(&mut m, 600, &hauling(320.0));
        let ev = feed(&mut m, 601, &snap(320.0, 20.0, Some(job("Paris", 5_000))));
        assert!(names(&ev).contains(&"job-completed"));
        assert!(names(&ev).contains(&"job-started"));
    }

    // ---- ferries and loading screens ---------------------------------------------------------

    #[test]
    fn ferry_teleport_keeps_the_delivery_going() {
        let mut m = on_a_job();
        feed(&mut m, 200, &hauling(150.0));
        let mut boarding = parked_hauling(150.0);
        boarding.truck.position = (1_000.0, 0.0, 0.0);
        feed(&mut m, 210, &boarding);
        let mut landed = parked_hauling(150.0);
        landed.truck.position = (500_000.0, 0.0, 9_000.0);
        assert!(feed(&mut m, 260, &landed).is_empty());
        assert!(feed(&mut m, 262, &hauling(150.4)).is_empty());
        assert_eq!(m.state(), DrivingState::Delivery);
    }

    #[test]
    fn a_teleport_alone_never_starts_a_session() {
        let mut m = machine();
        feed(&mut m, 0, &parked(10.0));
        let mut jumped = parked(10.0);
        jumped.truck.position = (9_999.0, 0.0, 0.0);
        assert!(feed(&mut m, 5, &jumped).is_empty());
        assert_eq!(m.state(), DrivingState::NotDriving);
    }

    #[test]
    fn a_four_minute_ferry_wait_does_not_end_the_delivery() {
        let mut m = on_a_job();
        feed(&mut m, 200, &hauling(150.0));
        feed(&mut m, 201, &parked_hauling(150.0));
        let ev = feed(&mut m, 201 + 240, &parked_hauling(150.0));
        assert!(ev.is_empty());
        assert_eq!(m.state(), DrivingState::Delivery);
    }

    // ---- disconnects -------------------------------------------------------------------------

    #[test]
    fn a_thirty_second_disconnect_keeps_the_session() {
        let mut m = StateMachine::default();
        feed(&mut m, 0, &driving(10.0));
        feed(&mut m, 1, &driving(11.0));
        assert!(m.step(t(11), None).is_empty());
        assert!(m.step(t(21), None).is_empty());
        assert!(m.step(t(31), None).is_empty());
        assert!(feed(&mut m, 32, &driving(12.0)).is_empty());
        assert_eq!(m.state(), DrivingState::FreeRoam);
    }

    #[test]
    fn a_ninety_second_disconnect_ends_the_session_at_the_last_frame() {
        let mut m = StateMachine::default();
        feed(&mut m, 0, &driving(10.0));
        feed(&mut m, 1, &driving(11.0));
        feed(&mut m, 20, &driving(15.0));
        assert!(m.step(t(50), None).is_empty());
        let ev = m.step(t(90), None);
        match &ev[0] {
            Event::SessionEnded {
                ended_at,
                end_odometer_km,
                ..
            } => {
                assert_eq!(*ended_at, t(20));
                assert_eq!(*end_odometer_km, 15.0);
            }
            other => panic!("unexpected {other:?}"),
        }
        assert_eq!(m.state(), DrivingState::NotDriving);
    }

    #[test]
    fn exactly_sixty_seconds_is_not_yet_a_disconnect() {
        let mut m = StateMachine::default();
        feed(&mut m, 0, &driving(10.0));
        feed(&mut m, 1, &driving(11.0));
        assert!(m.step(t(61), None).is_empty());
        assert_eq!(names(&m.step(t(62), None)), ["end:free"]);
    }

    #[test]
    fn a_gap_without_none_samples_is_still_noticed_on_the_next_frame() {
        let mut m = StateMachine::default();
        feed(&mut m, 0, &driving(10.0));
        feed(&mut m, 1, &driving(11.0));
        let ev = feed(&mut m, 500, &parked(11.0));
        assert_eq!(names(&ev), ["end:free"]);
        assert_eq!(m.state(), DrivingState::NotDriving);
    }

    #[test]
    fn coming_back_moving_after_a_long_gap_starts_a_fresh_session() {
        let mut m = StateMachine::default();
        feed(&mut m, 0, &driving(10.0));
        feed(&mut m, 1, &driving(11.0));
        feed(&mut m, 2, &driving(12.0));
        let ev = feed(&mut m, 500, &driving(13.0));
        assert_eq!(names(&ev), ["end:free", "start:free"]);
        assert_eq!(
            ended(&ev),
            [(SessionKind::FreeRoam, SessionStatus::Completed, 1.0)]
        );
    }

    #[test]
    fn a_delivery_survives_a_game_restart() {
        let mut m = StateMachine::default();
        feed(&mut m, 0, &driving(100.0));
        feed(&mut m, 1, &driving(100.5));
        feed(&mut m, 30, &hauling(120.0));
        feed(&mut m, 40, &hauling(140.0));
        let ev = m.step(t(110), None);
        assert_eq!(names(&ev), ["end:job"]);
        assert_eq!(
            ended(&ev),
            [(SessionKind::Job, SessionStatus::Completed, 20.0)]
        );
        assert!(m.active_job().is_some());
        // Game comes back with the same job loaded and the truck rolling again.
        feed(&mut m, 200, &parked_hauling(140.0));
        let ev = feed(&mut m, 210, &hauling(141.0));
        assert_eq!(names(&ev), ["start:job"]); // no second JobStarted
    }

    // ---- parking overnight -------------------------------------------------------------------

    #[test]
    fn parking_overnight_mid_delivery_ends_the_segment_but_not_the_job() {
        let mut m = on_a_job();
        feed(&mut m, 300, &hauling(180.0));
        feed(&mut m, 301, &parked_hauling(180.0));
        let ev = feed(&mut m, 301 + 300, &parked_hauling(180.0));
        assert_eq!(names(&ev), ["end:job"]);
        assert_eq!(
            ended(&ev),
            [(SessionKind::Job, SessionStatus::Completed, 60.0)]
        );
        assert_eq!(m.state(), DrivingState::NotDriving);
        assert!(m.active_job().is_some());
        assert!(feed(&mut m, 30_000, &parked_hauling(180.0)).is_empty()); // all night
    }

    #[test]
    fn resuming_the_next_morning_continues_the_same_job() {
        let mut m = on_a_job();
        feed(&mut m, 300, &hauling(180.0));
        feed(&mut m, 301, &parked_hauling(180.0));
        feed(&mut m, 601, &parked_hauling(180.0));
        feed(&mut m, 30_000, &parked_hauling(180.0));
        let ev = feed(&mut m, 30_010, &hauling(181.0));
        assert_eq!(names(&ev), ["start:job"]);
        assert_eq!(m.active_job().unwrap().start_odometer_km, 120.0);
    }

    #[test]
    fn delivering_after_a_parked_break_reports_the_whole_job_distance() {
        let mut m = on_a_job();
        feed(&mut m, 300, &hauling(180.0));
        feed(&mut m, 301, &parked_hauling(180.0));
        feed(&mut m, 601, &parked_hauling(180.0));
        feed(&mut m, 30_000, &parked_hauling(180.0));
        feed(&mut m, 30_010, &hauling(181.0));
        feed(&mut m, 31_000, &hauling(320.0));
        let ev = feed(&mut m, 31_001, &driving(320.0));
        let Event::JobCompleted {
            job,
            end_odometer_km,
            ..
        } = &ev[1]
        else {
            panic!("{ev:?}")
        };
        assert_eq!(*end_odometer_km - job.start_odometer_km, 200.0);
        assert_eq!(
            ended(&ev),
            [(SessionKind::Job, SessionStatus::Completed, 139.0)]
        );
    }

    #[test]
    fn a_job_that_vanishes_while_parked_is_finished_without_a_session_event() {
        let mut m = on_a_job();
        feed(&mut m, 300, &hauling(180.0));
        feed(&mut m, 301, &parked_hauling(180.0));
        feed(&mut m, 601, &parked_hauling(180.0)); // session ended, job parked
        let ev = feed(&mut m, 30_000, &parked(180.0));
        assert_eq!(names(&ev), ["job-completed"]);
        assert!(m.active_job().is_none());
        assert_eq!(m.state(), DrivingState::NotDriving);
    }

    #[test]
    fn resuming_with_a_different_job_finishes_the_old_one_first() {
        let mut m = on_a_job();
        feed(&mut m, 300, &hauling(180.0));
        feed(&mut m, 301, &parked_hauling(180.0));
        feed(&mut m, 601, &parked_hauling(180.0));
        let ev = feed(
            &mut m,
            30_000,
            &snap(180.0, 0.0, Some(job("Berlin", 6_500))),
        );
        assert_eq!(names(&ev), ["job-completed"]);
        let ev = feed(
            &mut m,
            30_010,
            &snap(180.5, 40.0, Some(job("Berlin", 6_500))),
        );
        assert_eq!(names(&ev), ["job-started", "start:job"]);
    }

    #[test]
    fn starting_the_app_mid_delivery_picks_the_job_up_when_the_truck_rolls() {
        let mut m = machine();
        feed(&mut m, 0, &parked_hauling(500.0));
        let ev = feed(&mut m, 10, &hauling(500.5));
        assert_eq!(names(&ev), ["job-started", "start:job"]);
        assert_eq!(m.active_job().unwrap().start_odometer_km, 500.5);
    }

    #[test]
    fn a_parked_truck_with_a_job_does_not_open_a_session_by_itself() {
        let mut m = machine();
        feed(&mut m, 0, &parked_hauling(500.0));
        assert!(feed(&mut m, 60, &parked_hauling(500.0)).is_empty());
        assert_eq!(m.state(), DrivingState::NotDriving);
    }

    #[test]
    fn custom_thresholds_are_respected() {
        let mut m = StateMachine::new(Config {
            idle_after: Duration::seconds(30),
            ..Config::default()
        });
        feed(&mut m, 0, &driving(10.0));
        feed(&mut m, 1, &driving(11.0));
        feed(&mut m, 2, &parked(11.0));
        assert_eq!(names(&feed(&mut m, 32, &parked(11.0))), ["end:free"]);
    }
}

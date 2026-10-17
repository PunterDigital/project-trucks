//! The background task: polls the telemetry source once a second, runs the pipeline and tells the
//! frontend what happened.
//!
//! Tauri events: `telemetry:status` (a string, only when it changes), `session:created` (a
//! `DrivingSession`) and `job:created` (a `Job`). The latest status is also kept in
//! [`TelemetryStatus`] so a page that opens later can ask for it.

use std::sync::Mutex;
use std::time::Duration;

use chrono::{NaiveDateTime, Utc};
use sqlx::SqlitePool;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::watch;

use super::pipeline::{Created, Pipeline};
use super::reader::TelemetryReader;
use super::replay::Replay;
use super::state_machine::DrivingState;
use super::struct_v::MMF_NAME;
use super::TelemetrySnapshot;

const POLL_INTERVAL: Duration = Duration::from_secs(1);

pub const STATUS_NOT_CONNECTED: &str = "not_connected";
pub const STATUS_PARKED: &str = "parked";
pub const STATUS_FREE_ROAM: &str = "free_roam";
pub const STATUS_DELIVERY: &str = "delivery";

/// Latest value of the `telemetry:status` event, as Tauri managed state.
pub struct TelemetryStatus(pub Mutex<String>);

impl Default for TelemetryStatus {
    fn default() -> Self {
        Self(Mutex::new(STATUS_NOT_CONNECTED.to_string()))
    }
}

pub enum Source {
    /// Read the plugin's memory-mapped file.
    Live,
    /// Replay a recorded sequence, one sample per `interval`.
    Replay(Replay, Duration),
}

/// Status name for the frontend: no frame means the game (or plugin) is not there; otherwise the
/// state machine says what the truck is doing.
pub fn status_name(connected: bool, state: DrivingState) -> &'static str {
    match (connected, state) {
        (false, _) => STATUS_NOT_CONNECTED,
        (true, DrivingState::NotDriving) => STATUS_PARKED,
        (true, DrivingState::FreeRoam) => STATUS_FREE_ROAM,
        (true, DrivingState::Delivery) => STATUS_DELIVERY,
    }
}

/// Run until the app exits (live) or the fixture is used up (replay).
///
/// `enabled` is the auto-capture setting. While it is false nothing is read and the status is
/// `not_connected`. Turning it off drops a live session in progress (the pipeline starts over), so
/// a drive resumed later is not stitched to the old one; a replay just pauses.
pub async fn run(
    app: AppHandle,
    pool: SqlitePool,
    source: Source,
    mut enabled: watch::Receiver<bool>,
) {
    let mut pipeline = Pipeline::default();
    match source {
        Source::Live => {
            let mut reader: Option<TelemetryReader> = None;
            let mut interval = tokio::time::interval(POLL_INTERVAL);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                if !*enabled.borrow_and_update() {
                    reader = None;
                    pipeline = Pipeline::default();
                    set_status(&app, STATUS_NOT_CONNECTED);
                    if enabled.changed().await.is_err() {
                        return;
                    }
                    interval.reset();
                    continue;
                }
                tokio::select! {
                    _ = interval.tick() => {}
                    changed = enabled.changed() => {
                        if changed.is_err() {
                            return;
                        }
                        continue;
                    }
                }
                if reader.is_none() {
                    reader = TelemetryReader::open_named(MMF_NAME, Duration::ZERO).ok();
                }
                let snapshot = match &reader {
                    Some(r) => match r.read() {
                        Ok(s) => Some(s),
                        Err(e) => {
                            tracing::debug!(error = %e, "telemetry read failed, reconnecting");
                            reader = None;
                            None
                        }
                    },
                    None => None,
                };
                tick(
                    &app,
                    &pool,
                    &mut pipeline,
                    Utc::now().naive_utc(),
                    snapshot.as_ref(),
                )
                .await;
            }
        }
        Source::Replay(replay, every) => {
            tracing::info!(
                samples = replay.samples.len(),
                "replaying telemetry fixture"
            );
            let mut interval = tokio::time::interval(every);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            let mut started = None;
            for sample in &replay.samples {
                while !*enabled.borrow_and_update() {
                    set_status(&app, STATUS_NOT_CONNECTED);
                    if enabled.changed().await.is_err() {
                        return;
                    }
                    interval.reset();
                }
                let start = *started.get_or_insert_with(|| Utc::now().naive_utc());
                interval.tick().await;
                let at = start + chrono::Duration::milliseconds((sample.t * 1000.0) as i64);
                tick(&app, &pool, &mut pipeline, at, sample.snapshot.as_ref()).await;
            }
            tracing::info!("telemetry replay finished");
            // Stay around so switching auto-capture off still resets the status.
            while enabled.changed().await.is_ok() {
                if !*enabled.borrow_and_update() {
                    set_status(&app, STATUS_NOT_CONNECTED);
                }
            }
        }
    }
}

async fn tick(
    app: &AppHandle,
    pool: &SqlitePool,
    pipeline: &mut Pipeline,
    at: NaiveDateTime,
    frame: Option<&TelemetrySnapshot>,
) {
    match pipeline.step(pool, at, frame).await {
        Ok(created) => {
            for row in created {
                let sent = match &row {
                    Created::Session(s) => app.emit("session:created", s),
                    Created::Job(j) => app.emit("job:created", j),
                };
                if let Err(e) = sent {
                    tracing::warn!(error = %e, "could not emit telemetry event");
                }
            }
        }
        Err(e) => tracing::error!(error = %e, "writing telemetry rows failed"),
    }
    set_status(app, status_name(frame.is_some(), pipeline.state()));
}

fn set_status(app: &AppHandle, name: &str) {
    let state = app.state::<TelemetryStatus>();
    let mut current = state.0.lock().expect("telemetry status lock");
    if *current != name {
        *current = name.to_string();
        drop(current);
        if let Err(e) = app.emit("telemetry:status", name) {
            tracing::warn!(error = %e, "could not emit telemetry:status");
        }
    }
}

#[tauri::command]
pub fn get_telemetry_status(state: tauri::State<'_, TelemetryStatus>) -> String {
    state.0.lock().expect("telemetry status lock").clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_names() {
        assert_eq!(
            status_name(false, DrivingState::Delivery),
            "not_connected",
            "no frame wins over a stale state"
        );
        assert_eq!(status_name(true, DrivingState::NotDriving), "parked");
        assert_eq!(status_name(true, DrivingState::FreeRoam), "free_roam");
        assert_eq!(status_name(true, DrivingState::Delivery), "delivery");
    }
}

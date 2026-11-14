//! Live "on the road" status for the Hub (#4635).
//!
//! The telemetry runner publishes what the truck is doing on every tick ([`publish`]); this task
//! turns that into `PUT /status` calls while the app is linked: straight away when something that
//! matters changes (truck, job, parked), otherwise every [`HEARTBEAT`] so the Hub (which drops a
//! status after 120 s) keeps showing the driver. When the game closes or auto-capture goes off it
//! sends `DELETE /status` once. A failure is only logged: the next tick tries again, and the
//! telemetry loop never waits on the network.

use std::sync::Arc;
use std::time::{Duration, Instant};

use sqlx::SqlitePool;
use tauri::{AppHandle, Manager};
use tokio::sync::watch;

use super::api::{HubError, LiveJob, LivePayload, LivePosition, LiveTruck};
use super::Hub;
use crate::telemetry::current_job::CurrentJob;
use crate::telemetry::TelemetrySnapshot;

/// Resend at least this often while driving.
pub const HEARTBEAT: Duration = Duration::from_secs(30);
/// Never send changes closer together than this.
pub const MIN_GAP: Duration = Duration::from_secs(5);

/// What the truck is doing; `None` = the game isn't running (or auto-capture is off).
#[derive(Debug, Clone, PartialEq)]
pub struct LiveSnapshot {
    pub truck_make: Option<String>,
    pub truck_model: Option<String>,
    pub job: Option<CurrentJob>,
    /// World x / z and the SCS heading (0..1 of a turn). Sent with every update, but a move
    /// alone never triggers one: it rides along with the heartbeat (#4686).
    pub position: Option<(f32, f32, f32)>,
}

/// Sender half, as Tauri managed state; the runner publishes through [`publish`].
pub struct LiveFeed(pub watch::Sender<Option<LiveSnapshot>>);

/// Called by the telemetry runner on every tick.
pub fn publish(app: &AppHandle, frame: Option<&TelemetrySnapshot>, job: Option<&CurrentJob>) {
    let Some(feed) = app.try_state::<LiveFeed>() else {
        return;
    };
    let next = frame.map(|f| LiveSnapshot {
        truck_make: f.truck.make.clone(),
        truck_model: f.truck.model.clone(),
        job: job.cloned(),
        position: Some((f.truck.position.0, f.truck.position.2, f.truck.heading)),
    });
    feed.0.send_if_modified(|current| {
        if *current == next {
            false
        } else {
            *current = next;
            true
        }
    });
}

/// Cargo, from, to, parked.
type JobKey = (Option<String>, Option<String>, Option<String>, bool);
/// Truck make, model and the job's key.
type LiveKey = (Option<String>, Option<String>, Option<JobKey>);

impl LiveSnapshot {
    /// The parts whose change is worth an immediate update (not the km ticking by).
    fn key(&self) -> LiveKey {
        (
            self.truck_make.clone(),
            self.truck_model.clone(),
            self.job.as_ref().map(|j| {
                (
                    j.cargo_name.clone(),
                    j.source_city.clone(),
                    j.destination_city.clone(),
                    j.parked,
                )
            }),
        )
    }

    pub fn payload(&self) -> LivePayload {
        LivePayload {
            truck: (self.truck_make.is_some() || self.truck_model.is_some()).then(|| LiveTruck {
                make: self.truck_make.clone(),
                model: self.truck_model.clone(),
            }),
            job: self.job.as_ref().map(|j| LiveJob {
                cargo_name: j.cargo_name.clone(),
                cargo_mass_kg: f64::from(j.cargo_mass_kg),
                source_city: j.source_city.clone(),
                source_company: j.source_company.clone(),
                destination_city: j.destination_city.clone(),
                destination_company: j.destination_company.clone(),
                income: i64::from(j.income),
                driven_km: round1(f64::from(j.driven_km)),
                distance_left_km: j.distance_left_km.map(|v| round1(f64::from(v))),
                progress_pct: j
                    .progress_pct
                    .map(|v| round1(f64::from(v)).clamp(0.0, 100.0)),
                parked: j.parked,
            }),
            position: self.position.map(|(x, z, heading)| LivePosition {
                x: round1(f64::from(x)),
                z: round1(f64::from(z)),
                heading_deg: heading_degrees(heading),
            }),
        }
    }
}

/// SCS heading (fraction of a full turn) as degrees in 0..360, one decimal.
fn heading_degrees(turns: f32) -> f64 {
    round1((f64::from(turns) * 360.0).rem_euclid(360.0)) % 360.0
}

fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

/// When to send, given what was last sent and when.
pub fn due(last: Option<(&LiveSnapshot, Instant)>, next: &LiveSnapshot, now: Instant) -> bool {
    match last {
        None => true,
        Some((sent, at)) => {
            let since = now.saturating_duration_since(at);
            (sent.key() != next.key() && since >= MIN_GAP) || since >= HEARTBEAT
        }
    }
}

pub async fn run(
    app: AppHandle,
    pool: SqlitePool,
    hub: Arc<Hub>,
    mut feed: watch::Receiver<Option<LiveSnapshot>>,
) {
    let mut last: Option<(LiveSnapshot, Instant)> = None;
    // The Hub shows us as on the road (we sent something and haven't cleared it yet).
    let mut shown = false;
    loop {
        tokio::select! {
            changed = feed.changed() => if changed.is_err() { return },
            _ = tokio::time::sleep(MIN_GAP) => {}
        }
        let (Some(client), Some(token)) = (hub.client().cloned(), hub.token()) else {
            last = None;
            shown = false;
            continue;
        };
        let snapshot = feed.borrow().clone();
        let result = match snapshot {
            None if shown => client.delete_status(&token).await.map(|_| {
                shown = false;
                last = None;
            }),
            None => Ok(()),
            Some(next) => {
                if !due(last.as_ref().map(|(s, at)| (s, *at)), &next, Instant::now()) {
                    continue;
                }
                client.put_status(&token, &next.payload()).await.map(|_| {
                    shown = true;
                    last = Some((next, Instant::now()));
                })
            }
        };
        match result {
            Ok(()) => {}
            Err(HubError::Unauthorized) => {
                last = None;
                shown = false;
                super::handle_unauthorized(&app, &pool, &hub).await;
            }
            Err(e) => tracing::debug!(error = %e, "hub live status failed"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(cargo: &str, parked: bool, driven: f32) -> LiveSnapshot {
        LiveSnapshot {
            truck_make: Some("Volvo".into()),
            truck_model: Some("FH16".into()),
            job: Some(CurrentJob {
                cargo_name: Some(cargo.into()),
                cargo_mass_kg: 18_000.0,
                source_city: Some("Calais".into()),
                source_company: Some("Tradeaux".into()),
                destination_city: Some("Lyon".into()),
                destination_company: Some("Kaarfor".into()),
                income: 9_000,
                deadline: None,
                game_time: None,
                minutes_to_deadline: None,
                driven_km: driven,
                distance_left_km: Some(500.0),
                route_minutes_left: None,
                progress_pct: Some(12.34),
                parked,
            }),
            position: Some((-12_345.67, 4_321.04, 0.25)),
        }
    }

    #[test]
    fn sends_at_once_then_on_changes_and_heartbeats() {
        let t0 = Instant::now();
        let first = snapshot("Wine", false, 10.0);
        assert!(due(None, &first, t0));

        // Only the km moved: wait for the heartbeat.
        let moved = snapshot("Wine", false, 12.0);
        assert!(!due(
            Some((&first, t0)),
            &moved,
            t0 + Duration::from_secs(10)
        ));
        assert!(due(Some((&first, t0)), &moved, t0 + HEARTBEAT));

        // Parked: send after the minimum gap.
        let parked = snapshot("Wine", true, 12.0);
        assert!(!due(
            Some((&first, t0)),
            &parked,
            t0 + Duration::from_secs(1)
        ));
        assert!(due(Some((&first, t0)), &parked, t0 + MIN_GAP));
    }

    #[test]
    fn payload_matches_the_hub_api() {
        let value = serde_json::to_value(snapshot("Wine", false, 10.04).payload()).unwrap();
        assert_eq!(value["truck"]["make"], "Volvo");
        assert_eq!(value["job"]["destination_city"], "Lyon");
        assert_eq!(value["job"]["driven_km"], 10.0);
        assert_eq!(value["job"]["progress_pct"], 12.3);
        assert_eq!(value["job"]["income"], 9_000);
        assert_eq!(value["position"]["x"], -12_345.7);
        assert_eq!(value["position"]["z"], 4_321.0);
        assert_eq!(value["position"]["heading_deg"], 90.0);

        let free_roam = LiveSnapshot {
            job: None,
            ..snapshot("Wine", false, 0.0)
        };
        let free_roam = serde_json::to_value(free_roam.payload()).unwrap();
        assert_eq!(free_roam["job"], serde_json::Value::Null);
        // Free-roaming still shows on the map, with the truck.
        assert_eq!(free_roam["truck"]["model"], "FH16");
        assert_eq!(free_roam["position"]["heading_deg"], 90.0);
    }

    #[test]
    fn moving_alone_waits_for_the_heartbeat() {
        let t0 = Instant::now();
        let first = snapshot("Wine", false, 10.0);
        let moved = LiveSnapshot {
            position: Some((0.0, 0.0, 0.5)),
            ..first.clone()
        };
        assert!(!due(Some((&first, t0)), &moved, t0 + MIN_GAP));
        assert!(due(Some((&first, t0)), &moved, t0 + HEARTBEAT));
    }

    #[test]
    fn headings_are_degrees_in_range() {
        assert_eq!(heading_degrees(0.0), 0.0);
        assert_eq!(heading_degrees(0.125), 45.0);
        assert_eq!(heading_degrees(1.0), 0.0);
        assert_eq!(heading_degrees(-0.25), 270.0);
        assert_eq!(heading_degrees(0.99999), 0.0);
    }
}

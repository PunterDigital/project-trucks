//! The "Job in progress" view (#4620): what the truck is hauling right now, how far is left and
//! how far along it is. Built each tick from the state machine's active job (where it started)
//! and the latest frame (where the truck is now and what the route advisor says).

use chrono::NaiveDateTime;

use super::state_machine::{DrivingState, JobInProgress};
use super::TelemetrySnapshot;

/// Route-advisor distances under this many km count as "no route" (it reads 0 when unset).
const NO_ROUTE_KM: f32 = 0.05;

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CurrentJob {
    pub cargo_name: Option<String>,
    pub cargo_mass_kg: f32,
    pub source_city: Option<String>,
    pub source_company: Option<String>,
    pub destination_city: Option<String>,
    pub destination_company: Option<String>,
    pub income: i32,
    /// In-game delivery deadline.
    pub deadline: Option<NaiveDateTime>,
    /// In-game clock now.
    pub game_time: Option<NaiveDateTime>,
    /// Game minutes until the deadline; negative once it has passed.
    pub minutes_to_deadline: Option<i64>,
    /// Odometer km since the job was picked up.
    pub driven_km: f32,
    /// Route advisor: km to the destination (`None` when no route is set).
    pub distance_left_km: Option<f32>,
    /// Route advisor: estimated game minutes to the destination (`None` when no route is set).
    pub route_minutes_left: Option<f32>,
    /// `driven / (driven + left)` as 0–100, or `None` without a route.
    pub progress_pct: Option<f32>,
    /// The truck is stopped (or the game paused) mid-job.
    pub parked: bool,
}

/// The job to show, or `None` when there is no frame, no job in the truck, or the frame's job is
/// not the one the machine is tracking (it switches on the next step).
pub fn current_job(
    active: Option<&JobInProgress>,
    frame: Option<&TelemetrySnapshot>,
    state: DrivingState,
) -> Option<CurrentJob> {
    let frame = frame?;
    let active = active?;
    let job = frame.job.as_ref()?;
    if !active.same_job(job) {
        return None;
    }
    let driven_km = (frame.truck.odometer_km - active.start_odometer_km).max(0.0);
    let has_route = job.route_distance_km >= NO_ROUTE_KM;
    let distance_left_km = has_route.then_some(job.route_distance_km);
    let route_minutes_left =
        (has_route && job.route_time_minutes > 0.0).then_some(job.route_time_minutes);
    let progress_pct = distance_left_km.map(|left| {
        let total = driven_km + left;
        if total <= 0.0 {
            0.0
        } else {
            (driven_km / total * 100.0).clamp(0.0, 100.0)
        }
    });
    let minutes_to_deadline = match (job.deadline, frame.game_time) {
        (Some(deadline), Some(now)) => Some((deadline - now).num_minutes()),
        _ => None,
    };
    Some(CurrentJob {
        cargo_name: job.cargo_name.clone(),
        cargo_mass_kg: job.cargo_mass_kg,
        source_city: job.source_city.clone(),
        source_company: job.source_company.clone(),
        destination_city: job.destination_city.clone(),
        destination_company: job.destination_company.clone(),
        income: job.income,
        deadline: job.deadline,
        game_time: frame.game_time,
        minutes_to_deadline,
        driven_km,
        distance_left_km,
        route_minutes_left,
        progress_pct,
        parked: frame.paused || state != DrivingState::Delivery,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telemetry::{JobSnapshot, TruckSnapshot};
    use chrono::NaiveDate;

    fn at(h: u32, m: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(1, 1, 8)
            .unwrap()
            .and_hms_opt(h, m, 0)
            .unwrap()
    }

    fn job(route_km: f32) -> JobSnapshot {
        JobSnapshot {
            income: 9_000,
            deadline: Some(at(18, 0)),
            source_city: Some("Calais".into()),
            destination_city: Some("Lyon".into()),
            source_company: Some("Tradeaux".into()),
            destination_company: Some("Kaarfor".into()),
            cargo_id: Some("wine".into()),
            cargo_name: Some("Wine".into()),
            cargo_mass_kg: 18_000.0,
            route_distance_km: route_km,
            route_time_minutes: route_km, // ~60 km/h
        }
    }

    fn frame(odo: f32, job: Option<JobSnapshot>) -> TelemetrySnapshot {
        TelemetrySnapshot {
            tick: 1,
            paused: false,
            game_time: Some(at(12, 30)),
            plugin_revision: 9,
            game_version: (1, 55),
            truck: TruckSnapshot {
                make: Some("Volvo".into()),
                make_id: Some("volvo".into()),
                model: Some("FH16".into()),
                speed_kmh: 80.0,
                odometer_km: odo,
                engine_enabled: true,
                trailer_attached: true,
                position: (0.0, 0.0, 0.0),
                fuel_litres: 300.0,
                fuel_capacity_litres: 600.0,
                wear_chassis: 0.0,
                wear_trailer: 0.0,
            },
            job,
        }
    }

    fn active(start: f32) -> JobInProgress {
        JobInProgress {
            job: job(400.0),
            truck_make: Some("Volvo".into()),
            truck_model: Some("FH16".into()),
            start_odometer_km: start,
            started_at: at(10, 0),
        }
    }

    #[test]
    fn progress_is_driven_over_driven_plus_left() {
        let a = active(1_000.0);
        let f = frame(1_100.0, Some(job(300.0)));
        let c = current_job(Some(&a), Some(&f), DrivingState::Delivery).unwrap();
        assert_eq!(c.driven_km, 100.0);
        assert_eq!(c.distance_left_km, Some(300.0));
        assert_eq!(c.route_minutes_left, Some(300.0));
        assert_eq!(c.progress_pct, Some(25.0));
        assert_eq!(c.minutes_to_deadline, Some(330));
        assert_eq!(c.destination_city.as_deref(), Some("Lyon"));
        assert!(!c.parked);
    }

    #[test]
    fn no_route_means_no_percentage() {
        let a = active(1_000.0);
        let f = frame(1_050.0, Some(job(0.0)));
        let c = current_job(Some(&a), Some(&f), DrivingState::Delivery).unwrap();
        assert_eq!(c.distance_left_km, None);
        assert_eq!(c.route_minutes_left, None);
        assert_eq!(c.progress_pct, None);
        assert_eq!(c.driven_km, 50.0);
    }

    #[test]
    fn a_reroute_that_adds_distance_lowers_the_bar_but_stays_in_range() {
        let a = active(1_000.0);
        // Odometer below the start (a new truck, a profile switch): driven clamps to 0.
        let f = frame(900.0, Some(job(500.0)));
        let c = current_job(Some(&a), Some(&f), DrivingState::Delivery).unwrap();
        assert_eq!(c.driven_km, 0.0);
        assert_eq!(c.progress_pct, Some(0.0));
    }

    #[test]
    fn parked_or_paused_mid_job_still_shows_the_job() {
        let a = active(1_000.0);
        let mut f = frame(1_100.0, Some(job(300.0)));
        let c = current_job(Some(&a), Some(&f), DrivingState::NotDriving).unwrap();
        assert!(c.parked);
        f.paused = true;
        let c = current_job(Some(&a), Some(&f), DrivingState::Delivery).unwrap();
        assert!(c.parked);
    }

    #[test]
    fn nothing_without_a_frame_a_job_or_a_tracked_job() {
        let a = active(1_000.0);
        let f = frame(1_100.0, Some(job(300.0)));
        assert_eq!(current_job(Some(&a), None, DrivingState::Delivery), None);
        assert_eq!(current_job(None, Some(&f), DrivingState::Delivery), None);
        let no_job = frame(1_100.0, None);
        assert_eq!(
            current_job(Some(&a), Some(&no_job), DrivingState::Delivery),
            None
        );
        let mut other = job(300.0);
        other.destination_city = Some("Berlin".into());
        let f = frame(1_100.0, Some(other));
        assert_eq!(
            current_job(Some(&a), Some(&f), DrivingState::Delivery),
            None
        );
    }

    #[test]
    fn a_passed_deadline_is_negative() {
        let a = active(1_000.0);
        let mut f = frame(1_100.0, Some(job(300.0)));
        f.game_time = Some(at(19, 15));
        let c = current_job(Some(&a), Some(&f), DrivingState::Delivery).unwrap();
        assert_eq!(c.minutes_to_deadline, Some(-75));
    }
}

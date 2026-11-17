//! ETS2 telemetry: a plugin in the game writes a memory-mapped frame; this module reads it.
//! RenCloud's scs-telemetry (`struct_scs`) is the one the app ships since #4619; Funbit's
//! ets2-telemetry-server (`struct_v`) is still read for installs that haven't updated.
//!
//! Windows-only at runtime (the plugin and the game are), but the types compile everywhere so the
//! state machine can be tested anywhere.

pub mod current_job;
pub mod pipeline;
pub mod reader;
pub mod replay;
pub mod runner;
pub mod state_machine;
pub mod struct_scs;
pub mod struct_v;

pub use reader::{Plugin, ReaderError, TelemetryReader};
pub use state_machine::{Expense, ExpenseKind};
pub use struct_v::{FrameError, TelemetryFrame};

use chrono::NaiveDateTime;

/// One reading of the game, in units the rest of the app wants (km/h, km, `chrono` times,
/// `Option` instead of empty strings).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TelemetrySnapshot {
    /// Plugin tick counter from the frame header (increases while the game runs).
    pub tick: u32,
    pub paused: bool,
    /// In-game clock. ETS2 counts minutes from 0001-01-01 00:00 (a Monday).
    pub game_time: Option<NaiveDateTime>,
    pub plugin_revision: u32,
    pub game_version: (u32, u32),
    pub truck: TruckSnapshot,
    pub job: Option<JobSnapshot>,
    /// Gameplay events the game reported since the previous frame (#4756): RenCloud's plugin only.
    /// Older replay fixtures don't have them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<GameplayEvent>,
}

/// One of the game's gameplay events, as RenCloud's scs-telemetry passes it on (#4756). The plugin
/// flips a flag per event and fills the event's values in the same frame.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum GameplayEvent {
    /// The job was handed in (the results screen is up).
    JobDelivered(DeliveryReport),
    /// The job was cancelled; `penalty` is what the game charged for it.
    JobCancelled {
        penalty: i64,
    },
    /// `offence` is the game's id (`speeding_camera`, `red_signal`, ...).
    Fined {
        amount: i64,
        offence: Option<String>,
    },
    Tollgate {
        amount: i64,
    },
    Ferry {
        amount: i64,
        source: Option<String>,
        target: Option<String>,
    },
    Train {
        amount: i64,
        source: Option<String>,
        target: Option<String>,
    },
}

/// What the game says about a delivery at the moment it is handed in.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryReport {
    /// What the game paid, which can differ from the job's offered income.
    pub revenue: i64,
    pub earned_xp: i32,
    /// Cargo damage, 0.0 to 1.0.
    pub cargo_damage: f32,
    /// The job's distance as the game counts it.
    pub distance_km: f32,
    pub auto_park: bool,
    pub auto_load: bool,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TruckSnapshot {
    pub make: Option<String>,
    pub make_id: Option<String>,
    pub model: Option<String>,
    /// Signed: negative when reversing.
    pub speed_kmh: f32,
    /// Truck odometer in km.
    pub odometer_km: f32,
    pub engine_enabled: bool,
    pub trailer_attached: bool,
    /// World coordinates (x, y, z).
    pub position: (f32, f32, f32),
    /// Heading as SCS gives it: 0.0 to 1.0 of a full turn, 0 = north, counter-clockwise.
    #[serde(default)]
    pub heading: f32,
    pub fuel_litres: f32,
    pub fuel_capacity_litres: f32,
    /// Damage, 0.0 (new) to 1.0 (wrecked).
    pub wear_chassis: f32,
    pub wear_trailer: f32,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobSnapshot {
    pub income: i32,
    /// In-game deadline of the delivery window.
    pub deadline: Option<NaiveDateTime>,
    pub source_city: Option<String>,
    pub destination_city: Option<String>,
    pub source_company: Option<String>,
    pub destination_company: Option<String>,
    pub cargo_id: Option<String>,
    pub cargo_name: Option<String>,
    pub cargo_mass_kg: f32,
    /// Route advisor: distance left to the destination in km (0 when no route is set).
    /// Older replay fixtures don't have it, hence the default.
    #[serde(default)]
    pub route_distance_km: f32,
    /// Route advisor: estimated time left in game minutes (0 when no route is set).
    #[serde(default)]
    pub route_time_minutes: f32,
}

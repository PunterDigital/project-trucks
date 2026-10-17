//! ETS2 telemetry: Funbit's plugin writes a memory-mapped frame; this module mirrors it.
//!
//! Windows-only at runtime (the plugin and the game are), but the types compile everywhere so the
//! state machine can be tested anywhere.

pub mod pipeline;
pub mod reader;
pub mod replay;
pub mod runner;
pub mod state_machine;
pub mod struct_v;

pub use reader::{ReaderError, TelemetryReader};
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
}

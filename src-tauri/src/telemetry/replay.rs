//! Replay mode: feed a recorded sequence of snapshots to the pipeline instead of reading ETS2.
//!
//! Set `PROJECTTRUCKS_TELEMETRY_REPLAY` to a JSON file shaped like [`Replay`]. Each sample carries
//! a virtual time `t` (seconds since the start); the state machine sees `start + t`, so a fixture
//! can cover a 5-minute idle timeout while the replay itself runs at
//! `PROJECTTRUCKS_TELEMETRY_REPLAY_INTERVAL_MS` per sample (default 1000). A sample with
//! `"snapshot": null` stands for "no frame" (the game is closed).
//!
//! This is how the pipeline is tested end to end without ETS2 (see `e2e/telemetry.spec.ts`).

use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::TelemetrySnapshot;

/// Path of the fixture to replay.
pub const REPLAY_ENV: &str = "PROJECTTRUCKS_TELEMETRY_REPLAY";
/// Milliseconds between samples (default [`DEFAULT_INTERVAL`]).
pub const REPLAY_INTERVAL_ENV: &str = "PROJECTTRUCKS_TELEMETRY_REPLAY_INTERVAL_MS";
pub const DEFAULT_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Replay {
    pub samples: Vec<ReplaySample>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplaySample {
    /// Seconds since the replay started.
    pub t: f64,
    pub snapshot: Option<TelemetrySnapshot>,
}

impl Replay {
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading replay fixture {}", path.display()))?;
        serde_json::from_str(&text)
            .with_context(|| format!("parsing replay fixture {}", path.display()))
    }
}

/// The replay configured through the environment, if any: the fixture and the pause between samples.
pub fn from_env() -> Result<Option<(Replay, Duration)>> {
    let Some(path) = std::env::var_os(REPLAY_ENV).filter(|v| !v.is_empty()) else {
        return Ok(None);
    };
    let interval = std::env::var(REPLAY_INTERVAL_ENV)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .map(Duration::from_millis)
        .unwrap_or(DEFAULT_INTERVAL);
    Ok(Some((Replay::load(Path::new(&path))?, interval)))
}

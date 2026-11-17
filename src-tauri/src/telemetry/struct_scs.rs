//! Decoder for RenCloud's `scs-telemetry` plugin (`scsTelemetryMap_t`, plugin revision 12).
//!
//! Source of truth: `assets/telemetry-plugin/scs-telemetry/headers/scs-telemetry-common.hpp`
//! (RenCloud/scs-sdk-plugin @ V.1.12.1, MIT). Unlike Funbit's struct, this one is laid out in
//! fixed "zones" padded with explicit buffers, and the header documents each zone's start offset
//! (zone 2 at 40, zone 4 at 700, zone 9 at 2300, the trailers at 6000, ...). So instead of a
//! `#[repr(C)]` mirror of 21 KB of mostly unused fields, the fields we need are read at fixed
//! offsets, worked out from the header with MSVC x64 rules (1-byte bool, natural alignment) and
//! checked against the zone offsets in the tests below.
//!
//! Job state: the plugin keeps the last job's strings and income in the frame after the job ends
//! (it only overwrites them with the next job), but it sets `special_b.onJob` from the game's job
//! config and clears it on the delivered/cancelled gameplay events. So a job is present only while
//! `onJob` is set, which is the real signal Funbit's plugin never had (#4734).

use chrono::{Duration, NaiveDate, NaiveDateTime};

use super::{DeliveryReport, GameplayEvent, JobSnapshot, TelemetrySnapshot, TruckSnapshot};

/// Name of the plugin's file mapping (`SCS_PLUGIN_MMF_NAME`).
pub const MMF_NAME: &str = "Local\\SCSTelemetry";
/// Size of the file mapping (`SCS_PLUGIN_MMF_SIZE`).
pub const MMF_SIZE: usize = 32 * 1024;
/// Revisions whose layout this decoder knows (`PLUGIN_REVID` of v1.12 / v1.12.1).
pub const SUPPORTED_REVISIONS: &[u32] = &[12];
/// Bytes we copy out of the mapping: everything up to the end of the first trailer.
pub const FRAME_SIZE: usize = off::TRAILER0 + TRAILER_SIZE;
const TRAILER_SIZE: usize = 1560;
const STRING_SIZE: usize = 64;

/// Byte offsets into `scsTelemetryMap_t`.
mod off {
    // Zone 1 (0..40)
    pub const SDK_ACTIVE: usize = 0;
    pub const PAUSED: usize = 4;
    /// `time`: the plugin's continuous timestamp in microseconds (u64).
    pub const TIME: usize = 8;
    // Zone 2 (40..500)
    pub const REVISION: usize = 40;
    pub const VERSION_MAJOR: usize = 44;
    pub const VERSION_MINOR: usize = 48;
    pub const TIME_ABS: usize = 64;
    pub const TIME_ABS_DELIVERY: usize = 88;
    // Zone 4 (700..1500)
    pub const FUEL_CAPACITY: usize = 704;
    pub const CARGO_MASS: usize = 748;
    pub const SPEED: usize = 948;
    pub const FUEL: usize = 1000;
    pub const WEAR_CHASSIS: usize = 1048;
    pub const ODOMETER: usize = 1056;
    pub const ROUTE_DISTANCE: usize = 1060;
    pub const ROUTE_TIME: usize = 1064;
    // Zone 5 (1500..1640)
    pub const ENGINE_ENABLED: usize = 1576;
    // Zone 8 (2200..2300)
    pub const COORDINATE_X: usize = 2200;
    pub const ROTATION_X: usize = 2224;
    // Zone 9 (2300..4000)
    pub const TRUCK_BRAND_ID: usize = 2300;
    pub const TRUCK_BRAND: usize = 2364;
    pub const TRUCK_NAME: usize = 2492;
    pub const CARGO_ID: usize = 2556;
    pub const CARGO: usize = 2620;
    pub const CITY_DST: usize = 2748;
    pub const COMP_DST: usize = 2876;
    pub const CITY_SRC: usize = 3004;
    pub const COMP_SRC: usize = 3132;
    // Zone 2 again: gameplay_ui follows truck_ui (4 u32 + 16 + 32 + 32 u32 arrays).
    // (jobDeliveredDeliveryTime 440, jobStartingTime 444, jobFinishedTime 448: not read.)
    // Zone 3 (500..700): restStop, gear, gearDashboard, hshifterResulting[32], then gameplay_i.
    pub const DELIVERED_XP: usize = 640;
    // Zone 4: gameplay_f follows truck_f.
    pub const DELIVERED_CARGO_DAMAGE: usize = 1456;
    pub const DELIVERED_DISTANCE: usize = 1460;
    // Zone 5: gameplay_b follows truck_b (24 bools, 16 wheels, 2 shifter toggles, 5 axle bools).
    pub const DELIVERED_AUTOPARK: usize = 1613;
    pub const DELIVERED_AUTOLOAD: usize = 1614;
    // Zone 9: gameplay_s follows config_s (14 strings, shifterType[16], 3 strings, jobMarket[32]).
    pub const FINE_OFFENCE: usize = 3436;
    pub const FERRY_SOURCE: usize = 3468;
    pub const FERRY_TARGET: usize = 3532;
    pub const TRAIN_SOURCE: usize = 3724;
    pub const TRAIN_TARGET: usize = 3788;
    // Zone 10 (4000..4200)
    pub const JOB_INCOME: usize = 4000;
    // Zone 11 (4200..4300): gameplay_ll (i64)
    pub const CANCEL_PENALTY: usize = 4200;
    pub const DELIVERED_REVENUE: usize = 4208;
    pub const FINE_AMOUNT: usize = 4216;
    pub const TOLL_AMOUNT: usize = 4224;
    pub const FERRY_AMOUNT: usize = 4232;
    pub const TRAIN_AMOUNT: usize = 4240;
    // Zone 12 (4300..4400): special_b
    pub const ON_JOB: usize = 4300;
    pub const JOB_CANCELLED: usize = 4302;
    pub const JOB_DELIVERED: usize = 4303;
    pub const FINED: usize = 4304;
    pub const TOLLGATE: usize = 4305;
    pub const FERRY: usize = 4306;
    pub const TRAIN: usize = 4307;
    // Zone 14: trailer[0] (6000..7560)
    pub const TRAILER0: usize = 6000;
    pub const TRAILER0_ATTACHED: usize = TRAILER0 + 80;
    pub const TRAILER0_WEAR_CHASSIS: usize = TRAILER0 + 156;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ScsFrameError {
    #[error("telemetry buffer is {got} bytes, need at least {need}")]
    TooShort { got: usize, need: usize },
    /// The game isn't running the SDK (not started yet, or shutting down).
    #[error("the scs-telemetry plugin is not active")]
    NotActive,
    #[error("unsupported scs-telemetry plugin revision {0} (supported: {SUPPORTED_REVISIONS:?})")]
    UnsupportedRevision(u32),
}

/// A checked view of one copied frame.
pub struct ScsFrame<'a>(&'a [u8]);

/// The plugin's gameplay-event flags (`special_b`). It flips one (`^= true`, scs_telemetry.cpp
/// V.1.12.1) each time the game sends that event, so a change between two frames is one event,
/// whichever way it went. Shay's recorded delivery: the toll flag went 0 → 1 at the toll and
/// stayed; at the drop-off `jobDelivered` went 0 → 1 with the revenue and XP filled in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EventFlags {
    pub delivered: bool,
    pub cancelled: bool,
    pub fined: bool,
    pub tollgate: bool,
    pub ferry: bool,
    pub train: bool,
}

/// Turns flag flips into [`GameplayEvent`]s. The first frame it sees is the baseline: whatever
/// the flags say then happened before the app was looking.
#[derive(Debug, Default)]
pub struct EventTracker {
    last: Option<EventFlags>,
}

impl EventTracker {
    pub fn events(&mut self, frame: &ScsFrame<'_>) -> Vec<GameplayEvent> {
        let now = frame.event_flags();
        let Some(before) = self.last.replace(now) else {
            return Vec::new();
        };
        let mut events = Vec::new();
        if now.delivered != before.delivered {
            events.push(GameplayEvent::JobDelivered(frame.delivery_report()));
        }
        if now.cancelled != before.cancelled {
            events.push(GameplayEvent::JobCancelled {
                penalty: frame.i64(off::CANCEL_PENALTY),
            });
        }
        if now.fined != before.fined {
            events.push(GameplayEvent::Fined {
                amount: frame.i64(off::FINE_AMOUNT),
                offence: frame.short_string(off::FINE_OFFENCE, 32),
            });
        }
        if now.tollgate != before.tollgate {
            events.push(GameplayEvent::Tollgate {
                amount: frame.i64(off::TOLL_AMOUNT),
            });
        }
        if now.ferry != before.ferry {
            events.push(GameplayEvent::Ferry {
                amount: frame.i64(off::FERRY_AMOUNT),
                source: frame.string(off::FERRY_SOURCE),
                target: frame.string(off::FERRY_TARGET),
            });
        }
        if now.train != before.train {
            events.push(GameplayEvent::Train {
                amount: frame.i64(off::TRAIN_AMOUNT),
                source: frame.string(off::TRAIN_SOURCE),
                target: frame.string(off::TRAIN_TARGET),
            });
        }
        events
    }
}

fn game_minutes(minutes: u32) -> Option<NaiveDateTime> {
    let epoch = NaiveDate::from_ymd_opt(1, 1, 1)?.and_hms_opt(0, 0, 0)?;
    epoch.checked_add_signed(Duration::minutes(i64::from(minutes)))
}

fn positive(v: f32) -> f32 {
    if v.is_finite() && v > 0.0 {
        v
    } else {
        0.0
    }
}

impl<'a> ScsFrame<'a> {
    pub fn from_bytes(bytes: &'a [u8]) -> Result<Self, ScsFrameError> {
        if bytes.len() < FRAME_SIZE {
            return Err(ScsFrameError::TooShort {
                got: bytes.len(),
                need: FRAME_SIZE,
            });
        }
        let frame = ScsFrame(bytes);
        if frame.u8(off::SDK_ACTIVE) == 0 {
            return Err(ScsFrameError::NotActive);
        }
        match frame.u32(off::REVISION) {
            r if SUPPORTED_REVISIONS.contains(&r) => Ok(frame),
            r => Err(ScsFrameError::UnsupportedRevision(r)),
        }
    }

    fn u8(&self, at: usize) -> u8 {
        self.0[at]
    }

    fn flag(&self, at: usize) -> bool {
        self.0[at] != 0
    }

    fn u32(&self, at: usize) -> u32 {
        u32::from_le_bytes(self.0[at..at + 4].try_into().expect("4 bytes"))
    }

    fn u64(&self, at: usize) -> u64 {
        u64::from_le_bytes(self.0[at..at + 8].try_into().expect("8 bytes"))
    }

    fn f32(&self, at: usize) -> f32 {
        f32::from_le_bytes(self.0[at..at + 4].try_into().expect("4 bytes"))
    }

    fn f64(&self, at: usize) -> f64 {
        f64::from_le_bytes(self.0[at..at + 8].try_into().expect("8 bytes"))
    }

    fn i64(&self, at: usize) -> i64 {
        i64::from_le_bytes(self.0[at..at + 8].try_into().expect("8 bytes"))
    }

    pub fn event_flags(&self) -> EventFlags {
        EventFlags {
            delivered: self.flag(off::JOB_DELIVERED),
            cancelled: self.flag(off::JOB_CANCELLED),
            fined: self.flag(off::FINED),
            tollgate: self.flag(off::TOLLGATE),
            ferry: self.flag(off::FERRY),
            train: self.flag(off::TRAIN),
        }
    }

    fn delivery_report(&self) -> DeliveryReport {
        DeliveryReport {
            revenue: self.i64(off::DELIVERED_REVENUE),
            earned_xp: self.u32(off::DELIVERED_XP) as i32,
            cargo_damage: positive(self.f32(off::DELIVERED_CARGO_DAMAGE)).min(1.0),
            distance_km: positive(self.f32(off::DELIVERED_DISTANCE)),
            auto_park: self.flag(off::DELIVERED_AUTOPARK),
            auto_load: self.flag(off::DELIVERED_AUTOLOAD),
        }
    }

    /// A NUL-terminated string field; empty reads as `None`, invalid UTF-8 is replaced.
    fn string(&self, at: usize) -> Option<String> {
        self.short_string(at, STRING_SIZE)
    }

    fn short_string(&self, at: usize, size: usize) -> Option<String> {
        let raw = &self.0[at..at + size];
        let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
        Some(String::from_utf8_lossy(&raw[..end]).into_owned()).filter(|s| !s.is_empty())
    }

    pub fn revision(&self) -> u32 {
        self.u32(off::REVISION)
    }

    pub fn to_snapshot(&self) -> TelemetrySnapshot {
        let on_job = self.flag(off::ON_JOB);
        let deadline = self.u32(off::TIME_ABS_DELIVERY);
        TelemetrySnapshot {
            // Milliseconds of the plugin's continuous clock (stops while paused, wraps after ~49 days).
            tick: (self.u64(off::TIME) / 1000) as u32,
            paused: self.flag(off::PAUSED),
            game_time: game_minutes(self.u32(off::TIME_ABS)),
            plugin_revision: self.revision(),
            game_version: (self.u32(off::VERSION_MAJOR), self.u32(off::VERSION_MINOR)),
            truck: TruckSnapshot {
                make: self.string(off::TRUCK_BRAND),
                make_id: self.string(off::TRUCK_BRAND_ID),
                model: self.string(off::TRUCK_NAME),
                speed_kmh: self.f32(off::SPEED) * 3.6,
                odometer_km: self.f32(off::ODOMETER),
                engine_enabled: self.flag(off::ENGINE_ENABLED),
                trailer_attached: self.flag(off::TRAILER0_ATTACHED),
                position: (
                    self.f64(off::COORDINATE_X) as f32,
                    self.f64(off::COORDINATE_X + 8) as f32,
                    self.f64(off::COORDINATE_X + 16) as f32,
                ),
                heading: self.f64(off::ROTATION_X) as f32,
                fuel_litres: self.f32(off::FUEL),
                fuel_capacity_litres: self.f32(off::FUEL_CAPACITY),
                wear_chassis: self.f32(off::WEAR_CHASSIS),
                wear_trailer: self.f32(off::TRAILER0_WEAR_CHASSIS),
            },
            job: on_job.then(|| JobSnapshot {
                income: i32::try_from(self.u64(off::JOB_INCOME)).unwrap_or(i32::MAX),
                // 0 = none set; u32::MAX = the job has no deadline (external-market jobs).
                deadline: if deadline != 0 && deadline != u32::MAX {
                    game_minutes(deadline)
                } else {
                    None
                },
                source_city: self.string(off::CITY_SRC),
                destination_city: self.string(off::CITY_DST),
                source_company: self.string(off::COMP_SRC),
                destination_company: self.string(off::COMP_DST),
                cargo_id: self.string(off::CARGO_ID),
                cargo_name: self.string(off::CARGO),
                cargo_mass_kg: self.f32(off::CARGO_MASS),
                route_distance_km: positive(self.f32(off::ROUTE_DISTANCE)) / 1000.0,
                route_time_minutes: positive(self.f32(off::ROUTE_TIME)) / 60.0,
            }),
            // Filled by the reader's EventTracker, which remembers the previous frame's flags.
            events: Vec::new(),
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Writes values at the decoder's offsets, standing in for the plugin.
    pub(crate) struct Builder(pub Vec<u8>);

    impl Builder {
        pub fn active() -> Self {
            let mut b = Builder(vec![0; MMF_SIZE]);
            b.put(off::SDK_ACTIVE, &[1]);
            b.put(off::REVISION, &12u32.to_le_bytes());
            b
        }
        pub fn put(&mut self, at: usize, bytes: &[u8]) -> &mut Self {
            self.0[at..at + bytes.len()].copy_from_slice(bytes);
            self
        }
        pub fn u32(&mut self, at: usize, v: u32) -> &mut Self {
            self.put(at, &v.to_le_bytes())
        }
        pub fn f32(&mut self, at: usize, v: f32) -> &mut Self {
            self.put(at, &v.to_le_bytes())
        }
        pub fn str(&mut self, at: usize, s: &str) -> &mut Self {
            self.put(at, s.as_bytes())
        }
    }

    /// The offsets are hand-computed sums over the header's fields; these zone starts are the
    /// header's own comments. Each assertion re-derives a field from the start of its zone.
    #[test]
    fn offsets_follow_the_documented_zones() {
        // Zone 2 starts at 40: 6 u32 scs_values, then common_ui.time_abs, then config_ui
        // (gears, gears_reverse, retarderStepCount, truckWheelCount, selectorCount, time_abs_delivery).
        assert_eq!(off::REVISION, 40);
        assert_eq!(off::TIME_ABS, 40 + 6 * 4);
        assert_eq!(off::TIME_ABS_DELIVERY, off::TIME_ABS + 4 + 5 * 4);
        // Zone 4 starts at 700: common_f.scale, then config_f (12 floats, 16 + 24 + 8 arrays, unitMass).
        assert_eq!(off::FUEL_CAPACITY, 700 + 4);
        assert_eq!(off::CARGO_MASS, off::FUEL_CAPACITY + 11 * 4);
        assert_eq!(off::SPEED, off::CARGO_MASS + 4 + (16 + 24 + 8) * 4 + 4);
        assert_eq!(off::FUEL, off::SPEED + 13 * 4);
        assert_eq!(off::WEAR_CHASSIS, off::SPEED + 25 * 4);
        assert_eq!(off::ODOMETER, off::SPEED + 27 * 4);
        assert_eq!(off::ROUTE_DISTANCE, off::ODOMETER + 4);
        assert_eq!(off::ROUTE_TIME, off::ODOMETER + 8);
        // ...and zone 4 still ends at 1500: truck_f has 31 floats + 6 x 16, then gameplay_f (3),
        // job_f (1) and buffer_f[28].
        assert_eq!(off::SPEED + (31 + 6 * 16) * 4 + 3 * 4 + 4 + 28, 1500);
        // Zone 5 starts at 1500: config_b (4 x 16 + 2 bools), then truck_b, engineEnabled is the 11th.
        assert_eq!(off::ENGINE_ENABLED, 1500 + 66 + 10);
        // ...and ends at 1640: truck_b has 24 + 16 + 2 + 5 bools, gameplay_b 2, buffer_b[25].
        assert_eq!(1500 + 66 + (24 + 16 + 2 + 5) + 2 + 25, 1640);
        // Zone 8 (truck_dp doubles) at 2200, zone 9 (config_s strings) at 2300.
        assert_eq!(off::COORDINATE_X, 2200);
        assert_eq!(off::ROTATION_X, 2200 + 3 * 8);
        assert_eq!(off::TRUCK_BRAND_ID, 2300);
        assert_eq!(off::TRUCK_NAME, 2300 + 3 * STRING_SIZE);
        assert_eq!(off::CARGO_ID, 2300 + 4 * STRING_SIZE);
        assert_eq!(off::CITY_DST, 2300 + 7 * STRING_SIZE);
        assert_eq!(off::COMP_SRC, 2300 + 13 * STRING_SIZE);
        // ...and zone 9 ends at 4000: 14 strings, shifterType[16], 3 strings, jobMarket[32],
        // then gameplay_s (fineOffence[32] + 8 strings) and buffer_s[20].
        assert_eq!(
            2300 + 17 * STRING_SIZE + 16 + 32 + 32 + 8 * STRING_SIZE + 20,
            4000
        );
        assert_eq!(off::JOB_INCOME, 4000);
        assert_eq!(off::ON_JOB, 4300);
        // Trailer zones: 4 x 16 bools, then wheelOnGround[16], attached; com_f at 152.
        assert_eq!(off::TRAILER0_ATTACHED, 6000 + 4 * 16 + 16);
        assert_eq!(off::TRAILER0_WEAR_CHASSIS, 6000 + 152 + 4);
        assert_eq!(FRAME_SIZE, 7560);
    }

    #[test]
    fn decodes_a_frame_on_a_job() {
        let mut b = Builder::active();
        b.u32(off::VERSION_MAJOR, 1)
            .u32(off::VERSION_MINOR, 19)
            .put(off::TIME, &5_000_000u64.to_le_bytes())
            .u32(off::TIME_ABS, 60 * 24 * 7 + 90)
            .u32(off::TIME_ABS_DELIVERY, 60 * 24 * 8)
            .f32(off::SPEED, 20.0)
            .f32(off::ODOMETER, 123_456.5)
            .f32(off::FUEL, 300.0)
            .f32(off::FUEL_CAPACITY, 800.0)
            .f32(off::ROUTE_DISTANCE, 808_000.0)
            .f32(off::ROUTE_TIME, 87_480.0)
            .f32(off::CARGO_MASS, 12_000.0)
            .put(off::ENGINE_ENABLED, &[1])
            .put(off::TRAILER0_ATTACHED, &[1])
            .put(off::COORDINATE_X, &(-1234.5f64).to_le_bytes())
            .put(off::COORDINATE_X + 16, &(987.25f64).to_le_bytes())
            .put(off::ROTATION_X, &0.25f64.to_le_bytes())
            .str(off::TRUCK_BRAND_ID, "volvo")
            .str(off::TRUCK_BRAND, "Volvo")
            .str(off::TRUCK_NAME, "FH Series 5")
            .str(off::CARGO_ID, "apples")
            .str(off::CARGO, "Apples")
            .str(off::CITY_SRC, "Poznań")
            .str(off::COMP_SRC, "Tradeaux")
            .str(off::CITY_DST, "Málaga")
            .str(off::COMP_DST, "Lisette")
            .put(off::JOB_INCOME, &45_210u64.to_le_bytes())
            .put(off::ON_JOB, &[1]);
        let snap = ScsFrame::from_bytes(&b.0).unwrap().to_snapshot();
        assert_eq!(snap.tick, 5000);
        assert!(!snap.paused);
        assert_eq!(snap.plugin_revision, 12);
        assert_eq!(snap.game_version, (1, 19));
        assert_eq!(snap.game_time, game_minutes(60 * 24 * 7 + 90));
        assert!((snap.truck.speed_kmh - 72.0).abs() < 1e-3);
        assert_eq!(snap.truck.odometer_km, 123_456.5);
        assert_eq!(snap.truck.make.as_deref(), Some("Volvo"));
        assert_eq!(snap.truck.make_id.as_deref(), Some("volvo"));
        assert_eq!(snap.truck.model.as_deref(), Some("FH Series 5"));
        assert!(snap.truck.engine_enabled && snap.truck.trailer_attached);
        assert_eq!(snap.truck.position, (-1234.5, 0.0, 987.25));
        assert_eq!(snap.truck.heading, 0.25);
        assert_eq!(snap.truck.fuel_capacity_litres, 800.0);
        let job = snap.job.expect("on a job");
        assert_eq!(job.income, 45_210);
        assert_eq!(job.deadline, game_minutes(60 * 24 * 8));
        assert_eq!(job.source_city.as_deref(), Some("Poznań"));
        assert_eq!(job.destination_city.as_deref(), Some("Málaga"));
        assert_eq!(job.source_company.as_deref(), Some("Tradeaux"));
        assert_eq!(job.destination_company.as_deref(), Some("Lisette"));
        assert_eq!(job.cargo_id.as_deref(), Some("apples"));
        assert_eq!(job.cargo_name.as_deref(), Some("Apples"));
        assert_eq!(job.cargo_mass_kg, 12_000.0);
        assert_eq!(job.route_distance_km, 808.0);
        assert_eq!(job.route_time_minutes, 1458.0);
    }

    /// After a delivery or cancel the plugin leaves the job's strings in place but clears onJob.
    #[test]
    fn a_finished_job_left_in_the_frame_is_no_job() {
        let mut b = Builder::active();
        b.str(off::CITY_DST, "Málaga")
            .put(off::JOB_INCOME, &45_210u64.to_le_bytes())
            .put(off::PAUSED, &[1]);
        let snap = ScsFrame::from_bytes(&b.0).unwrap().to_snapshot();
        assert!(snap.paused);
        assert_eq!(snap.job, None);
    }

    /// A frame captured from the real game (Nov 15, plugin V.1.12.1, free roam with a
    /// trailer hitched and no job). Funbit's plugin, loaded alongside, read the same odometer,
    /// position, fuel, wear and trailer state at the same moment.
    #[test]
    fn decodes_a_real_free_roam_frame() {
        let bytes = include_bytes!("testdata/scs-rev12-free-roam-frame.bin");
        let snap = ScsFrame::from_bytes(bytes).unwrap().to_snapshot();
        assert_eq!(snap.plugin_revision, 12);
        assert!(!snap.paused);
        assert_eq!(snap.truck.make.as_deref(), Some("Volvo"));
        assert_eq!(snap.truck.make_id.as_deref(), Some("volvo"));
        assert_eq!(snap.truck.model.as_deref(), Some("FH4"));
        assert!((snap.truck.odometer_km - 518_507.47).abs() < 0.01);
        assert!(snap.truck.engine_enabled);
        assert!(snap.truck.trailer_attached);
        assert_eq!(snap.truck.fuel_capacity_litres, 800.0);
        assert!((snap.truck.fuel_litres - 552.4).abs() < 1.0);
        assert!((snap.truck.position.0 - -71_704.69).abs() < 0.01);
        assert!((snap.truck.position.2 - 68_086.25).abs() < 0.01);
        assert!((snap.truck.heading - 0.7113).abs() < 1e-3);
        assert!(snap.truck.speed_kmh.abs() < 0.01);
        assert!(snap.game_time.is_some());
        assert_eq!(snap.job, None);
    }

    /// #4756: the gameplay fields, re-derived from their zones the same way.
    #[test]
    fn gameplay_offsets_follow_the_documented_zones() {
        // Zone 2: after config_ui (9 u32) comes truck_ui (4 + 16 + 32 + 32 u32), then
        // gameplay_ui (3 u32) and buffer_ui[48] up to 500.
        assert_eq!(
            off::TIME_ABS + 4 + 9 * 4 + (4 + 16 + 32 + 32) * 4 + 3 * 4 + 48,
            500
        );
        // Zone 3 starts at 500: restStop, gear, gearDashboard, hshifterResulting[32], then
        // gameplay_i.jobDeliveredEarnedXp and buffer_i[56] up to 700.
        assert_eq!(off::DELIVERED_XP, 500 + 3 * 4 + 32 * 4);
        assert_eq!(off::DELIVERED_XP + 4 + 56, 700);
        // Zone 4: gameplay_f follows truck_f's 31 floats + 6 x 16.
        assert_eq!(off::DELIVERED_CARGO_DAMAGE, off::SPEED + (31 + 6 * 16) * 4);
        assert_eq!(off::DELIVERED_DISTANCE, off::DELIVERED_CARGO_DAMAGE + 4);
        // Zone 5: truck_b is 24 bools, wheelOnGround[16], shifterToggle[2] and 5 axle bools; then
        // gameplay_b (2) and buffer_b[25] up to 1640.
        assert_eq!(off::DELIVERED_AUTOPARK, 1500 + 66 + 24 + 16 + 2 + 5);
        assert_eq!(off::DELIVERED_AUTOLOAD + 1 + 25, 1640);
        // Zone 9: config_s is 14 strings, shifterType[16], 3 strings and jobMarket[32]; then
        // gameplay_s: fineOffence[32] and 8 strings, buffer_s[20] up to 4000.
        assert_eq!(off::FINE_OFFENCE, 2300 + 14 * 64 + 16 + 3 * 64 + 32);
        assert_eq!(off::FERRY_SOURCE, off::FINE_OFFENCE + 32);
        assert_eq!(off::FERRY_TARGET, off::FERRY_SOURCE + 64);
        assert_eq!(off::TRAIN_SOURCE, off::FERRY_SOURCE + 4 * 64);
        assert_eq!(off::TRAIN_TARGET, off::TRAIN_SOURCE + 64);
        assert_eq!(off::FERRY_SOURCE + 8 * 64 + 20, 4000);
        // Zone 11 starts at 4200: six i64, buffer_ll[52] up to 4300.
        assert_eq!(off::CANCEL_PENALTY, 4200);
        assert_eq!(off::TRAIN_AMOUNT + 8 + 52, 4300);
        // Zone 12 starts at 4300: onJob, jobFinished, jobCancelled, jobDelivered, fined, tollgate,
        // ferry, train, refuel, refuelPayed, buffer_special[90] up to 4400.
        assert_eq!(off::JOB_CANCELLED, off::ON_JOB + 2);
        assert_eq!(off::TRAIN, off::ON_JOB + 7);
        assert_eq!(off::ON_JOB + 10 + 90, 4400);
    }

    fn i64_at(b: &mut Builder, at: usize, v: i64) {
        b.put(at, &v.to_le_bytes());
    }

    fn events(tracker: &mut EventTracker, b: &Builder) -> Vec<GameplayEvent> {
        tracker.events(&ScsFrame::from_bytes(&b.0).unwrap())
    }

    /// The plugin flips a flag per event (`^= true`): the first frame is only the baseline, and
    /// every change after it, either way, is one event with the values of that frame.
    #[test]
    fn flag_flips_become_gameplay_events() {
        let mut t = EventTracker::default();
        let mut b = Builder::active();
        // Set before the app was looking: not an event.
        b.put(off::TOLLGATE, &[1]);
        assert_eq!(events(&mut t, &b), vec![]);
        assert_eq!(events(&mut t, &b), vec![]);

        // A second toll flips it back.
        i64_at(&mut b, off::TOLL_AMOUNT, 19);
        b.put(off::TOLLGATE, &[0]);
        assert_eq!(
            events(&mut t, &b),
            vec![GameplayEvent::Tollgate { amount: 19 }]
        );

        i64_at(&mut b, off::FINE_AMOUNT, 400);
        b.str(off::FINE_OFFENCE, "red_signal").put(off::FINED, &[1]);
        b.str(off::FERRY_SOURCE, "Calais")
            .str(off::FERRY_TARGET, "Dover")
            .put(off::FERRY, &[1]);
        i64_at(&mut b, off::FERRY_AMOUNT, 1_090);
        assert_eq!(
            events(&mut t, &b),
            vec![
                GameplayEvent::Fined {
                    amount: 400,
                    offence: Some("red_signal".into())
                },
                GameplayEvent::Ferry {
                    amount: 1_090,
                    source: Some("Calais".into()),
                    target: Some("Dover".into())
                },
            ]
        );

        i64_at(&mut b, off::DELIVERED_REVENUE, 4_478);
        b.u32(off::DELIVERED_XP, 320)
            .f32(off::DELIVERED_CARGO_DAMAGE, 0.05)
            .f32(off::DELIVERED_DISTANCE, 229.0)
            .put(off::DELIVERED_AUTOPARK, &[1])
            .put(off::JOB_DELIVERED, &[1]);
        assert_eq!(
            events(&mut t, &b),
            vec![GameplayEvent::JobDelivered(DeliveryReport {
                revenue: 4_478,
                earned_xp: 320,
                cargo_damage: 0.05,
                distance_km: 229.0,
                auto_park: true,
                auto_load: false,
            })]
        );

        i64_at(&mut b, off::CANCEL_PENALTY, 900);
        b.put(off::JOB_CANCELLED, &[1]);
        assert_eq!(
            events(&mut t, &b),
            vec![GameplayEvent::JobCancelled { penalty: 900 }]
        );
        b.put(off::TRAIN, &[1]);
        assert!(matches!(
            events(&mut t, &b)[..],
            [GameplayEvent::Train { .. }]
        ));
        assert_eq!(events(&mut t, &b), vec![]);
    }

    /// A frame captured on a real job (Nov 15: an external-market job, Málaga → Algeciras, on the
    /// way to the pickup). Funbit's plugin read the same job at the same moment; the only field that
    /// differed is the deadline, which the game sends as u32::MAX for "no deadline".
    #[test]
    fn decodes_a_real_job_frame() {
        let bytes = include_bytes!("testdata/scs-rev12-job-frame.bin");
        let snap = ScsFrame::from_bytes(bytes).unwrap().to_snapshot();
        assert!(snap.truck.trailer_attached);
        assert!((snap.truck.speed_kmh - 41.9).abs() < 0.1);
        let job = snap.job.expect("on a job");
        assert_eq!(job.income, 4478);
        assert_eq!(job.deadline, None);
        assert_eq!(job.source_city.as_deref(), Some("Málaga"));
        assert_eq!(job.destination_city.as_deref(), Some("Algeciras"));
        assert_eq!(job.source_company.as_deref(), Some("FLE"));
        assert_eq!(
            job.destination_company.as_deref(),
            Some("Transportes Caballería")
        );
        assert_eq!(job.cargo_id.as_deref(), Some("comp_process"));
        assert_eq!(job.cargo_name.as_deref(), Some("Computer Processors"));
        assert!((job.cargo_mass_kg - 3854.4).abs() < 0.01);
        assert!((job.route_distance_km - 101.74).abs() < 0.01);
        assert!((job.route_time_minutes - 83.37).abs() < 0.01);
    }

    #[test]
    fn a_job_with_a_deadline_keeps_it() {
        let mut b = Builder::active();
        b.put(off::ON_JOB, &[1]).u32(off::TIME_ABS_DELIVERY, 60_000);
        let snap = ScsFrame::from_bytes(&b.0).unwrap().to_snapshot();
        assert_eq!(snap.job.unwrap().deadline, game_minutes(60_000));
    }

    #[test]
    fn rejects_inactive_short_and_unknown_frames() {
        assert_eq!(
            ScsFrame::from_bytes(&[0; 100]).err(),
            Some(ScsFrameError::TooShort {
                got: 100,
                need: FRAME_SIZE
            })
        );
        assert_eq!(
            ScsFrame::from_bytes(&vec![0; MMF_SIZE]).err(),
            Some(ScsFrameError::NotActive)
        );
        let mut b = Builder::active();
        b.u32(off::REVISION, 11);
        assert_eq!(
            ScsFrame::from_bytes(&b.0).err(),
            Some(ScsFrameError::UnsupportedRevision(11))
        );
    }
}

/// A whole delivery recorded from the real game (Nov 15, Shay's Málaga → Algeciras external-market
/// job, scs-telemetry V.1.12.1): pickup to drop-off, results screen and 90 s after.
#[cfg(test)]
mod recorded {
    use super::*;
    use crate::telemetry::replay::{Replay, ReplaySample};
    use crate::telemetry::state_machine::{Config, Event, StateMachine};

    /// Opt-in converter, never run in CI. Reads a recording made with the control repo's
    /// read-only recorder (records of `[i64 unix ms][7560 bytes scs-telemetry][2048 bytes funbit]`,
    /// one per second) and writes a replay fixture: one sample every 5 s, plus every sample within
    /// 10 s of a change in pause or job state or a gameplay event, so the fixture stays small but
    /// keeps every edge. Events go through the same [`EventTracker`] as the reader.
    /// `PROJECTTRUCKS_SCS_RECORDING=<in> PROJECTTRUCKS_SCS_REPLAY_OUT=<out> cargo test --lib recording_to_replay`
    #[test]
    fn recording_to_replay() {
        let (Some(input), Some(output)) = (
            std::env::var_os("PROJECTTRUCKS_SCS_RECORDING"),
            std::env::var_os("PROJECTTRUCKS_SCS_REPLAY_OUT"),
        ) else {
            eprintln!("skipped: set PROJECTTRUCKS_SCS_RECORDING and PROJECTTRUCKS_SCS_REPLAY_OUT");
            return;
        };
        const RECORD: usize = 8 + FRAME_SIZE + 2048;
        let bytes = std::fs::read(input).unwrap();
        let mut frames = Vec::new();
        let mut tracker = EventTracker::default();
        for rec in bytes.as_chunks::<RECORD>().0 {
            let ms = i64::from_le_bytes(rec[..8].try_into().unwrap());
            let snap = ScsFrame::from_bytes(&rec[8..8 + FRAME_SIZE]).ok().map(|f| {
                let mut s = f.to_snapshot();
                s.events = tracker.events(&f);
                s
            });
            frames.push((ms, snap));
        }
        let key = |s: &Option<TelemetrySnapshot>| {
            s.as_ref().map(|s| {
                (
                    s.paused,
                    s.job.is_some(),
                    s.truck.trailer_attached,
                    !s.events.is_empty(),
                )
            })
        };
        let edges: Vec<usize> = (1..frames.len())
            .filter(|&i| key(&frames[i].1) != key(&frames[i - 1].1))
            .collect();
        let first_ms = frames[0].0;
        let samples: Vec<ReplaySample> = frames
            .iter()
            .enumerate()
            .filter(|(i, _)| {
                i % 5 == 0 || *i == frames.len() - 1 || edges.iter().any(|&e| i.abs_diff(e) <= 10)
            })
            .map(|(_, (ms, snap))| ReplaySample {
                t: (ms - first_ms) as f64 / 1000.0,
                snapshot: snap.clone(),
            })
            .collect();
        eprintln!(
            "{} records -> {} samples, edges at {edges:?}",
            frames.len(),
            samples.len()
        );
        std::fs::write(output, serde_json::to_string(&Replay { samples }).unwrap()).unwrap();
    }

    #[test]
    fn the_recorded_delivery_is_one_completed_job() {
        let replay: Replay =
            serde_json::from_str(include_str!("testdata/scs-recorded-delivery.json")).unwrap();
        let start = chrono::NaiveDate::from_ymd_opt(2026, 11, 15)
            .unwrap()
            .and_hms_opt(10, 53, 45)
            .unwrap();
        let mut machine = StateMachine::new(Config::default());
        let mut events = Vec::new();
        for s in &replay.samples {
            let at = start + chrono::Duration::milliseconds((s.t * 1000.0) as i64);
            events.extend(machine.step(at, s.snapshot.as_ref()));
        }
        // The recording shows the game pause on the results screen, the job clear 3 s later (still
        // paused, 0.11 km from the drop-off) with the game's "delivered" event, then the trailer
        // unhitch, and it ends while the results screen is still up. The event finishes the job
        // there (#4756): before, nothing was recorded until the player left the results screen.
        let last = replay.samples.last().unwrap().snapshot.as_ref().unwrap();
        assert!(last.paused && last.job.is_none());
        let started: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, Event::JobStarted { .. }))
            .collect();
        assert_eq!(started.len(), 1, "{events:#?}");
        let completed: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                Event::JobCompleted {
                    job,
                    end_odometer_km,
                    report,
                    ..
                } => Some((job, *end_odometer_km, report)),
                _ => None,
            })
            .collect();
        assert_eq!(completed.len(), 1, "{events:#?}");
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, Event::JobCancelled { .. })),
            "{events:#?}"
        );
        let (job, end_km, report) = completed[0];
        let report = report.as_ref().expect("ended by the game's event");
        assert_eq!(report.revenue, 4478);
        assert_eq!(report.earned_xp, 320);
        assert_eq!(report.distance_km, 229.0);
        assert!(report.auto_park && report.auto_load);
        // The 19 toll on the way is on the job.
        assert_eq!(job.expenses.len(), 1);
        assert_eq!(job.expenses[0].kind, crate::db::models::ExpenseKind::Toll);
        assert_eq!(job.expenses[0].amount, 19);
        assert_eq!(job.job.destination_city.as_deref(), Some("Algeciras"));
        assert_eq!(job.job.income, 4478);
        assert_eq!(job.job.deadline, None);
        assert_eq!(job.truck_model.as_deref(), Some("FH4"));
        // Odometer 518,612.84 at the first frame, 518,736.66 at the drop-off.
        let driven = end_km - job.start_odometer_km;
        assert!((driven - 123.8).abs() < 0.2, "driven {driven} km");
        // After the delivery the machine is not still on that job.
        assert_eq!(machine.active_job(), None);
    }
}

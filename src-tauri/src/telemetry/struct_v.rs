//! Bit-exact Rust mirror of `ets2TelemetryMap_t` from Funbit's `ets2-sdk-plugin` (rev 10).
//!
//! Source of truth: `assets/telemetry-plugin/headers/ets2-telemetry-common.hpp`. The plugin is an
//! MSVC build, so `bool` is 1 byte, `long` is 4 bytes and every member has at most 4-byte
//! alignment; `#[repr(C)]` with `u8` for `bool` reproduces that layout. Booleans stay `u8` so a
//! garbage byte in shared memory can never become an invalid Rust `bool`.

use std::mem::size_of;

use chrono::{Duration, NaiveDate, NaiveDateTime};

use super::{JobSnapshot, TelemetrySnapshot, TruckSnapshot};

/// Name of the plugin's file mapping (`ETS2_PLUGIN_MMF_NAME`).
pub const MMF_NAME: &str = "Local\\Ets2TelemetryServer";
/// Size of the file mapping (`ETS2_PLUGIN_MMF_SIZE`); the frame is the first part of it.
pub const MMF_SIZE: usize = 16 * 1024;
/// Plugin revisions whose layout this struct mirrors. Upstream promises later revisions keep the
/// layout, but we only trust what we have read the header of.
pub const SUPPORTED_REVISIONS: &[u32] = &[10];

const GENERAL_STRING_SIZE: usize = 64;
const SHIFTER_TYPE_SIZE: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum FrameError {
    #[error("telemetry buffer is {got} bytes, need at least {need}")]
    TooShort { got: usize, need: usize },
    #[error("the plugin has not written a frame yet (revision 0)")]
    NotInitialised,
    #[error("unsupported telemetry plugin revision {0} (supported: {SUPPORTED_REVISIONS:?})")]
    UnsupportedRevision(u32),
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct RevId {
    pub ets2_telemetry_plugin_revision: u32,
    pub ets2_version_major: u32,
    pub ets2_version_minor: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct TelRev1 {
    pub engine_enabled: u8, // deprecated upstream since rev 5
    pub trailer_attached: u8,
    pub speed: f32, // m/s, negative in reverse
    pub acceleration: [f32; 3],
    pub coordinate: [f32; 3],
    pub rotation: [f32; 3],
    pub gear: i32,
    pub gears: i32,
    pub gear_ranges: i32,
    pub gear_range_active: i32,
    pub engine_rpm: f32,
    pub engine_rpm_max: f32,
    pub fuel: f32,
    pub fuel_capacity: f32,
    pub fuel_rate: f32,
    pub fuel_avg_consumption: f32,
    pub user_steer: f32,
    pub user_throttle: f32,
    pub user_brake: f32,
    pub user_clutch: f32,
    pub game_steer: f32,
    pub game_throttle: f32,
    pub game_brake: f32,
    pub game_clutch: f32,
    pub truck_weight: f32,
    pub trailer_weight: f32,
    pub model_type: [i32; 2],
    pub trailer_type: [i32; 2],
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct TelRev2 {
    /// Game minutes since 0001-01-01 00:00 (`long` is 32-bit on Windows).
    pub time_abs: i32,
    pub gears_reverse: i32,
    pub trailer_mass: f32,
    pub trailer_id: [u8; GENERAL_STRING_SIZE],
    pub trailer_name: [u8; GENERAL_STRING_SIZE],
    pub job_income: i32,
    /// Game minutes since 0001-01-01 00:00; 0 when there is no job.
    pub time_abs_delivery: i32,
    pub city_src: [u8; GENERAL_STRING_SIZE],
    pub city_dst: [u8; GENERAL_STRING_SIZE],
    pub comp_src: [u8; GENERAL_STRING_SIZE],
    pub comp_dst: [u8; GENERAL_STRING_SIZE],
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct TelRev3 {
    pub retarder_brake: i32,
    pub shifter_slot: i32,
    pub shifter_toggle: i32,
    pub fill: i32,
    pub cruise_control: u8,
    pub wipers: u8,
    pub park_brake: u8,
    pub motor_brake: u8,
    pub electric_enabled: u8,
    pub engine_enabled: u8,
    pub blinker_left_active: u8,
    pub blinker_right_active: u8,
    pub blinker_left_on: u8,
    pub blinker_right_on: u8,
    pub lights_parking: u8,
    pub lights_beam_low: u8,
    pub lights_beam_high: u8,
    pub lights_aux_front: u32,
    pub lights_aux_roof: u32,
    pub lights_beacon: u8,
    pub lights_brake: u8,
    pub lights_reverse: u8,
    pub battery_voltage_warning: u8,
    pub air_pressure_warning: u8,
    pub air_pressure_emergency: u8,
    pub adblue_warning: u8,
    pub oil_pressure_warning: u8,
    pub water_temperature_warning: u8,
    pub air_pressure: f32,
    pub brake_temperature: f32,
    pub fuel_warning: i32,
    pub adblue: f32,
    pub adblue_consumption: f32,
    pub oil_pressure: f32,
    pub oil_temperature: f32,
    pub water_temperature: f32,
    pub battery_voltage: f32,
    pub lights_dashboard: f32,
    pub wear_engine: f32,
    pub wear_transmission: f32,
    pub wear_cabin: f32,
    pub wear_chassis: f32,
    pub wear_wheels: f32,
    pub wear_trailer: f32,
    /// km
    pub truck_odometer: f32,
    pub cruise_control_speed: f32,
    pub truck_make: [u8; GENERAL_STRING_SIZE],
    pub truck_make_id: [u8; GENERAL_STRING_SIZE],
    pub truck_model: [u8; GENERAL_STRING_SIZE],
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct TelRev4 {
    pub fuel_warning_factor: f32,
    pub adblue_capacity: f32,
    pub air_pressure_warning_value: f32,
    pub air_pressure_emergency_value: f32,
    pub oil_pressure_warning_value: f32,
    pub water_temperature_warning_value: f32,
    pub battery_voltage_warning_value: f32,
    pub retarder_step_count: u32,
    pub cabin_position: [f32; 3],
    pub head_position: [f32; 3],
    pub hook_position: [f32; 3],
    pub shifter_type: [u8; SHIFTER_TYPE_SIZE],
    pub local_scale: f32,
    pub next_rest_stop: i32,
    pub trailer_coordinate: [f32; 3],
    pub trailer_rotation: [f32; 3],
    pub displayed_gear: i32,
    pub navigation_distance: f32,
    pub navigation_time: f32,
    pub navigation_speed_limit: f32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct TelemetryFrame {
    /// Plugin tick counter.
    pub time: u32,
    pub paused: u32,
    pub rev_id: RevId,
    pub rev1: TelRev1,
    pub rev2: TelRev2,
    pub rev3: TelRev3,
    pub rev4: TelRev4,
}

/// Bytes the plugin writes for the frame (the rest of the 16 KB mapping is not used by rev 10).
pub const FRAME_SIZE: usize = size_of::<TelemetryFrame>();

/// Trim at the first NUL and decode; invalid UTF-8 is replaced rather than failing the frame.
fn c_str(bytes: &[u8]) -> String {
    String::from_utf8_lossy(trim_nul(bytes)).into_owned()
}

fn c_str_opt(bytes: &[u8]) -> Option<String> {
    Some(c_str(bytes)).filter(|s| !s.is_empty())
}

fn trim_nul(bytes: &[u8]) -> &[u8] {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    &bytes[..end]
}

/// Borrowed, NUL-trimmed view of a fixed-size C string. Non-UTF-8 content reads as "".
fn str_field(bytes: &[u8]) -> &str {
    std::str::from_utf8(trim_nul(bytes)).unwrap_or("")
}

/// ETS2 game minutes (since 0001-01-01 00:00) as a date-time.
fn game_minutes(minutes: i32) -> Option<NaiveDateTime> {
    let epoch = NaiveDate::from_ymd_opt(1, 1, 1)?.and_hms_opt(0, 0, 0)?;
    epoch.checked_add_signed(Duration::minutes(i64::from(minutes)))
}

impl TelemetryFrame {
    /// Copy a frame out of a raw buffer (e.g. the memory-mapped file) and check its version.
    ///
    /// The buffer may be larger than the frame (the mapping is 16 KB); the rest is ignored.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, FrameError> {
        if bytes.len() < FRAME_SIZE {
            return Err(FrameError::TooShort {
                got: bytes.len(),
                need: FRAME_SIZE,
            });
        }
        // SAFETY: the buffer holds at least FRAME_SIZE bytes, and TelemetryFrame is plain data:
        // every field is an integer or float (valid for any bit pattern), so an unaligned read
        // of arbitrary bytes yields a valid value.
        let frame = unsafe { std::ptr::read_unaligned(bytes.as_ptr().cast::<TelemetryFrame>()) };
        frame.check_version()?;
        Ok(frame)
    }

    pub fn check_version(&self) -> Result<(), FrameError> {
        match self.rev_id.ets2_telemetry_plugin_revision {
            0 => Err(FrameError::NotInitialised),
            r if SUPPORTED_REVISIONS.contains(&r) => Ok(()),
            r => Err(FrameError::UnsupportedRevision(r)),
        }
    }

    pub fn truck_make(&self) -> &str {
        str_field(&self.rev3.truck_make)
    }

    pub fn truck_model(&self) -> &str {
        str_field(&self.rev3.truck_model)
    }

    pub fn city_src(&self) -> &str {
        str_field(&self.rev2.city_src)
    }

    pub fn city_dst(&self) -> &str {
        str_field(&self.rev2.city_dst)
    }

    pub fn trailer_name(&self) -> &str {
        str_field(&self.rev2.trailer_name)
    }

    pub fn to_snapshot(&self) -> TelemetrySnapshot {
        let r1 = &self.rev1;
        let r2 = &self.rev2;
        let r3 = &self.rev3;
        let has_job = r2.time_abs_delivery != 0 || r2.job_income != 0 || r2.city_dst[0] != 0;
        TelemetrySnapshot {
            tick: self.time,
            paused: self.paused != 0,
            game_time: game_minutes(r2.time_abs),
            plugin_revision: self.rev_id.ets2_telemetry_plugin_revision,
            game_version: (
                self.rev_id.ets2_version_major,
                self.rev_id.ets2_version_minor,
            ),
            truck: TruckSnapshot {
                make: c_str_opt(&r3.truck_make),
                make_id: c_str_opt(&r3.truck_make_id),
                model: c_str_opt(&r3.truck_model),
                speed_kmh: r1.speed * 3.6,
                odometer_km: r3.truck_odometer,
                engine_enabled: r3.engine_enabled != 0,
                trailer_attached: r1.trailer_attached != 0,
                position: (r1.coordinate[0], r1.coordinate[1], r1.coordinate[2]),
                fuel_litres: r1.fuel,
                fuel_capacity_litres: r1.fuel_capacity,
                wear_chassis: r3.wear_chassis,
                wear_trailer: r3.wear_trailer,
            },
            job: has_job.then(|| JobSnapshot {
                income: r2.job_income,
                deadline: if r2.time_abs_delivery != 0 {
                    game_minutes(r2.time_abs_delivery)
                } else {
                    None
                },
                source_city: c_str_opt(&r2.city_src),
                destination_city: c_str_opt(&r2.city_dst),
                source_company: c_str_opt(&r2.comp_src),
                destination_company: c_str_opt(&r2.comp_dst),
                cargo_id: c_str_opt(&r2.trailer_id),
                cargo_name: c_str_opt(&r2.trailer_name),
                cargo_mass_kg: r2.trailer_mass,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::{align_of, offset_of};

    /// Offsets worked out by hand from the C++ header with MSVC x64 rules (natural alignment,
    /// 1-byte bool, 4-byte long). If one of these fails the Rust mirror has drifted.
    #[test]
    fn layout_matches_cpp_header() {
        assert_eq!(align_of::<TelemetryFrame>(), 4);
        assert_eq!(offset_of!(TelemetryFrame, time), 0);
        assert_eq!(offset_of!(TelemetryFrame, paused), 4);
        assert_eq!(offset_of!(TelemetryFrame, rev_id), 8);
        assert_eq!(offset_of!(TelemetryFrame, rev1), 20);
        assert_eq!(offset_of!(TelemetryFrame, rev2), 160);
        assert_eq!(offset_of!(TelemetryFrame, rev3), 564);
        assert_eq!(offset_of!(TelemetryFrame, rev4), 880);
        assert_eq!(FRAME_SIZE, 1012);
        const { assert!(FRAME_SIZE <= MMF_SIZE) };
    }

    #[test]
    fn rev1_layout() {
        assert_eq!(offset_of!(TelRev1, trailer_attached), 1);
        assert_eq!(offset_of!(TelRev1, speed), 4);
        assert_eq!(offset_of!(TelRev1, coordinate), 20);
        assert_eq!(offset_of!(TelRev1, gear), 44);
        assert_eq!(offset_of!(TelRev1, fuel), 68);
        assert_eq!(offset_of!(TelRev1, truck_weight), 116);
        assert_eq!(offset_of!(TelRev1, trailer_type), 132);
        assert_eq!(size_of::<TelRev1>(), 140);
    }

    #[test]
    fn rev2_layout() {
        assert_eq!(offset_of!(TelRev2, trailer_mass), 8);
        assert_eq!(offset_of!(TelRev2, trailer_id), 12);
        assert_eq!(offset_of!(TelRev2, trailer_name), 76);
        assert_eq!(offset_of!(TelRev2, job_income), 140);
        assert_eq!(offset_of!(TelRev2, time_abs_delivery), 144);
        assert_eq!(offset_of!(TelRev2, city_src), 148);
        assert_eq!(offset_of!(TelRev2, comp_dst), 340);
        assert_eq!(size_of::<TelRev2>(), 404);
    }

    #[test]
    fn rev3_layout() {
        assert_eq!(offset_of!(TelRev3, cruise_control), 16);
        assert_eq!(offset_of!(TelRev3, lights_beam_high), 28);
        assert_eq!(offset_of!(TelRev3, lights_aux_front), 32);
        assert_eq!(offset_of!(TelRev3, lights_beacon), 40);
        assert_eq!(offset_of!(TelRev3, water_temperature_warning), 48);
        assert_eq!(offset_of!(TelRev3, air_pressure), 52);
        assert_eq!(offset_of!(TelRev3, wear_chassis), 104);
        assert_eq!(offset_of!(TelRev3, truck_odometer), 116);
        assert_eq!(offset_of!(TelRev3, truck_make), 124);
        assert_eq!(offset_of!(TelRev3, truck_model), 252);
        assert_eq!(size_of::<TelRev3>(), 316);
    }

    #[test]
    fn rev4_layout() {
        assert_eq!(offset_of!(TelRev4, retarder_step_count), 28);
        assert_eq!(offset_of!(TelRev4, hook_position), 56);
        assert_eq!(offset_of!(TelRev4, shifter_type), 68);
        assert_eq!(offset_of!(TelRev4, local_scale), 84);
        assert_eq!(offset_of!(TelRev4, displayed_gear), 116);
        assert_eq!(size_of::<TelRev4>(), 132);
    }

    /// A zeroed 16 KB buffer with writes at known absolute offsets, like the plugin produces.
    struct Buf(Vec<u8>);

    impl Buf {
        fn new() -> Self {
            Buf(vec![0; MMF_SIZE])
        }
        fn u32(&mut self, at: usize, v: u32) -> &mut Self {
            self.0[at..at + 4].copy_from_slice(&v.to_le_bytes());
            self
        }
        fn i32(&mut self, at: usize, v: i32) -> &mut Self {
            self.u32(at, v as u32)
        }
        fn f32(&mut self, at: usize, v: f32) -> &mut Self {
            self.0[at..at + 4].copy_from_slice(&v.to_le_bytes());
            self
        }
        fn u8(&mut self, at: usize, v: u8) -> &mut Self {
            self.0[at] = v;
            self
        }
        fn s(&mut self, at: usize, v: &str) -> &mut Self {
            self.0[at..at + v.len()].copy_from_slice(v.as_bytes());
            self
        }
    }

    /// Frame as the plugin would write it mid-delivery. Offsets are absolute (see the layout
    /// test): rev1 @20, rev2 @160, rev3 @564.
    fn delivery_buf() -> Buf {
        let mut b = Buf::new();
        b.u32(0, 123_456) // time
            .u32(4, 0) // paused
            .u32(8, 10) // plugin revision
            .u32(12, 1) // game major
            .u32(16, 57) // game minor
            .u8(21, 1) // rev1.trailer_attached
            .f32(24, 25.0) // rev1.speed m/s
            .f32(40, 100.5) // rev1.coordinate x
            .f32(44, 12.0)
            .f32(48, -300.25)
            .f32(88, 310.0) // rev1.fuel
            .f32(92, 800.0) // rev1.fuel_capacity
            .i32(160, 7 * 24 * 60 + 10 * 60 + 30) // rev2.time_abs: day 8 (Mon) 10:30
            .f32(168, 18_000.0) // rev2.trailer_mass
            .s(172, "wood_chips")
            .s(236, "Wood Chips")
            .i32(300, 4_321) // rev2.job_income
            .i32(304, 8 * 24 * 60 + 3 * 60 + 34) // deadline: day 9 03:34
            .s(308, "Calais")
            .s(372, "Berlin")
            .s(436, "Sellplan")
            .s(500, "Aria Foods")
            .u8(585, 1) // rev3.engine_enabled
            .f32(676, 0.02) // rev3.wear_trailer
            .f32(680, 98_765.5) // rev3.truck_odometer
            .s(688, "Scania")
            .s(752, "scania")
            .s(816, "R 2016");
        b
    }

    #[test]
    fn round_trips_a_hand_built_buffer_into_a_snapshot() {
        let frame = TelemetryFrame::from_bytes(&delivery_buf().0).unwrap();
        assert_eq!(frame.truck_make(), "Scania");
        assert_eq!(frame.truck_model(), "R 2016");
        assert_eq!(frame.city_src(), "Calais");
        assert_eq!(frame.city_dst(), "Berlin");
        assert_eq!(frame.trailer_name(), "Wood Chips");

        let snap = frame.to_snapshot();
        assert_eq!(snap.tick, 123_456);
        assert!(!snap.paused);
        assert_eq!(snap.plugin_revision, 10);
        assert_eq!(snap.game_version, (1, 57));
        assert_eq!(
            snap.game_time,
            NaiveDate::from_ymd_opt(1, 1, 8)
                .unwrap()
                .and_hms_opt(10, 30, 0)
        );
        assert_eq!(snap.truck.make.as_deref(), Some("Scania"));
        assert_eq!(snap.truck.make_id.as_deref(), Some("scania"));
        assert_eq!(snap.truck.model.as_deref(), Some("R 2016"));
        assert!((snap.truck.speed_kmh - 90.0).abs() < 1e-3);
        assert_eq!(snap.truck.odometer_km, 98_765.5);
        assert!(snap.truck.engine_enabled);
        assert!(snap.truck.trailer_attached);
        assert_eq!(snap.truck.position, (100.5, 12.0, -300.25));
        assert_eq!(snap.truck.fuel_litres, 310.0);
        assert_eq!(snap.truck.fuel_capacity_litres, 800.0);
        assert_eq!(snap.truck.wear_trailer, 0.02);

        let job = snap.job.expect("job present");
        assert_eq!(job.income, 4_321);
        assert_eq!(
            job.deadline,
            NaiveDate::from_ymd_opt(1, 1, 9)
                .unwrap()
                .and_hms_opt(3, 34, 0)
        );
        assert_eq!(job.source_city.as_deref(), Some("Calais"));
        assert_eq!(job.destination_city.as_deref(), Some("Berlin"));
        assert_eq!(job.source_company.as_deref(), Some("Sellplan"));
        assert_eq!(job.destination_company.as_deref(), Some("Aria Foods"));
        assert_eq!(job.cargo_id.as_deref(), Some("wood_chips"));
        assert_eq!(job.cargo_name.as_deref(), Some("Wood Chips"));
        assert_eq!(job.cargo_mass_kg, 18_000.0);
    }

    #[test]
    fn no_job_when_the_job_fields_are_empty() {
        let mut b = Buf::new();
        b.u32(8, 10);
        let snap = TelemetryFrame::from_bytes(&b.0).unwrap().to_snapshot();
        assert!(snap.job.is_none());
        assert!(snap.truck.make.is_none());
        assert!(!snap.paused);
    }

    #[test]
    fn paused_flag_and_reverse_speed() {
        let mut b = Buf::new();
        b.u32(8, 10).u32(4, 1).f32(24, -5.0);
        let snap = TelemetryFrame::from_bytes(&b.0).unwrap().to_snapshot();
        assert!(snap.paused);
        assert!(snap.truck.speed_kmh < 0.0);
    }

    #[test]
    fn strings_are_trimmed_at_the_first_nul_and_survive_full_width() {
        let mut b = delivery_buf();
        // "Scania\0garbage": everything after the NUL is ignored.
        b.s(688 + 7, "garbage");
        // A string that fills all 64 bytes with no terminator.
        b.0[372..436].fill(b'x');
        let frame = TelemetryFrame::from_bytes(&b.0).unwrap();
        assert_eq!(frame.truck_make(), "Scania");
        assert_eq!(frame.city_dst().len(), 64);
        assert_eq!(
            frame
                .to_snapshot()
                .job
                .unwrap()
                .destination_city
                .unwrap()
                .len(),
            64
        );
    }

    #[test]
    fn invalid_utf8_does_not_fail_the_frame() {
        let mut b = delivery_buf();
        b.0[688] = 0xFF; // not valid UTF-8
        let frame = TelemetryFrame::from_bytes(&b.0).unwrap();
        assert_eq!(frame.truck_make(), "");
        assert!(frame.to_snapshot().truck.make.unwrap().contains('\u{FFFD}'));
    }

    #[test]
    fn rejects_unsupported_and_uninitialised_versions() {
        let mut b = delivery_buf();
        b.u32(8, 9);
        assert_eq!(
            TelemetryFrame::from_bytes(&b.0).unwrap_err(),
            FrameError::UnsupportedRevision(9)
        );
        b.u32(8, 11);
        assert_eq!(
            TelemetryFrame::from_bytes(&b.0).unwrap_err(),
            FrameError::UnsupportedRevision(11)
        );
        b.u32(8, 0);
        assert_eq!(
            TelemetryFrame::from_bytes(&b.0).unwrap_err(),
            FrameError::NotInitialised
        );
    }

    #[test]
    fn rejects_short_buffers() {
        let b = delivery_buf();
        assert_eq!(
            TelemetryFrame::from_bytes(&b.0[..FRAME_SIZE - 1]).unwrap_err(),
            FrameError::TooShort {
                got: FRAME_SIZE - 1,
                need: FRAME_SIZE
            }
        );
        assert!(TelemetryFrame::from_bytes(&b.0[..FRAME_SIZE]).is_ok());
    }

    #[test]
    fn reads_from_an_unaligned_slice() {
        let b = delivery_buf();
        let mut shifted = vec![0u8; 1];
        shifted.extend_from_slice(&b.0);
        let frame = TelemetryFrame::from_bytes(&shifted[1..]).unwrap();
        assert_eq!(frame.truck_make(), "Scania");
    }
}

//! Reads the plugin's memory-mapped file.
//!
//! Two plugins are understood: RenCloud's `scs-telemetry.dll` (`Local\SCSTelemetry`, what the app
//! ships since #4619) and Funbit's `ets2-telemetry-server.dll` (`Local\Ets2TelemetryServer`, what
//! it shipped before). `open()` prefers the first and falls back to the second, so a player who
//! hasn't pressed "Update" yet keeps recording.
//!
//! `TelemetryReader::open()` waits (blocking, with backoff) for the plugin to create the mapping,
//! which only exists while ETS2 runs with the plugin loaded, so call it from a blocking task.
//! `read()` copies the frame out and converts it. The plugin writes without locking, so a read
//! can in theory straddle a tick; every field is a plain number, so the worst case is one frame
//! with mixed old/new values.
//!
//! Windows only. Elsewhere the same API exists but never connects, so the rest of the app (and
//! its tests) builds on any OS.

use std::time::Duration;

use super::struct_scs::{self, ScsFrame, ScsFrameError};
use super::struct_v::{self, FrameError};
use super::TelemetrySnapshot;

/// Which plugin wrote the mapping, i.e. which layout to decode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Plugin {
    /// RenCloud's scs-telemetry (bundled since #4619).
    Scs,
    /// Funbit's ets2-telemetry-server (bundled before #4619).
    Funbit,
}

impl Plugin {
    pub fn mapping_name(self) -> &'static str {
        match self {
            Plugin::Scs => struct_scs::MMF_NAME,
            Plugin::Funbit => struct_v::MMF_NAME,
        }
    }

    fn frame_size(self) -> usize {
        match self {
            Plugin::Scs => struct_scs::FRAME_SIZE,
            Plugin::Funbit => struct_v::FRAME_SIZE,
        }
    }

    fn mapping_size(self) -> usize {
        match self {
            Plugin::Scs => struct_scs::MMF_SIZE,
            Plugin::Funbit => struct_v::MMF_SIZE,
        }
    }
}

/// How long `open()` keeps looking for the mapping (the game may still be starting).
pub const OPEN_TIMEOUT: Duration = Duration::from_secs(5);
const FIRST_BACKOFF: Duration = Duration::from_millis(100);
const MAX_BACKOFF: Duration = Duration::from_millis(800);

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ReaderError {
    /// The mapping does not exist: the game is not running, or the plugin is not installed.
    #[error("not connected to the ETS2 telemetry plugin")]
    NotConnected,
    /// Another Windows error (access denied, out of memory, ...), with its `GetLastError` code.
    #[error("could not map the telemetry file (Windows error {0})")]
    Os(u32),
    #[error(transparent)]
    Frame(#[from] FrameError),
    #[error(transparent)]
    ScsFrame(#[from] ScsFrameError),
}

pub struct TelemetryReader {
    mapping: platform::Mapping,
    plugin: Plugin,
}

impl TelemetryReader {
    /// Connect to whichever plugin is running (scs-telemetry first), retrying for about 5 seconds.
    pub fn open() -> Result<Self, ReaderError> {
        Self::open_any(OPEN_TIMEOUT)
    }

    /// Like `open()` with another wait (the runner polls with `Duration::ZERO`).
    pub fn open_any(timeout: Duration) -> Result<Self, ReaderError> {
        Self::open_first(
            &[
                (Plugin::Scs.mapping_name(), Plugin::Scs),
                (Plugin::Funbit.mapping_name(), Plugin::Funbit),
            ],
            timeout,
        )
    }

    /// One plugin's layout under another mapping name (tests use their own mapping).
    pub fn open_named(name: &str, plugin: Plugin, timeout: Duration) -> Result<Self, ReaderError> {
        Self::open_first(&[(name, plugin)], timeout)
    }

    fn open_first(candidates: &[(&str, Plugin)], timeout: Duration) -> Result<Self, ReaderError> {
        let started = std::time::Instant::now();
        let mut backoff = FIRST_BACKOFF;
        loop {
            for &(name, plugin) in candidates {
                match platform::Mapping::open(name, plugin.mapping_size()) {
                    Ok(mapping) => return Ok(Self { mapping, plugin }),
                    // Only "it isn't there yet" is worth waiting for.
                    Err(ReaderError::NotConnected) => {}
                    Err(e) => return Err(e),
                }
            }
            if started.elapsed() >= timeout {
                return Err(ReaderError::NotConnected);
            }
            std::thread::sleep(backoff.min(timeout.saturating_sub(started.elapsed())));
            backoff = (backoff * 2).min(MAX_BACKOFF);
        }
    }

    /// Which plugin this reader is connected to.
    pub fn plugin(&self) -> Plugin {
        self.plugin
    }

    /// Copy the current frame out of the mapping and convert it.
    pub fn read(&self) -> Result<TelemetrySnapshot, ReaderError> {
        let buf = self.read_raw();
        Ok(match self.plugin {
            Plugin::Scs => ScsFrame::from_bytes(&buf)?.to_snapshot(),
            Plugin::Funbit => super::TelemetryFrame::from_bytes(&buf)?.to_snapshot(),
        })
    }

    /// The frame bytes as copied out of the mapping, undecoded (for capturing test fixtures).
    pub fn read_raw(&self) -> Vec<u8> {
        let mut buf = vec![0u8; self.plugin.frame_size()];
        self.mapping.copy_to(&mut buf);
        buf
    }
}

#[cfg(windows)]
mod platform {
    use std::ptr;

    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_FILE_NOT_FOUND, HANDLE};
    use windows_sys::Win32::System::Memory::{
        MapViewOfFile, OpenFileMappingW, UnmapViewOfFile, VirtualQuery, FILE_MAP_READ,
        MEMORY_BASIC_INFORMATION, MEMORY_MAPPED_VIEW_ADDRESS,
    };

    use super::ReaderError;

    pub struct Mapping {
        handle: HANDLE,
        view: *const u8,
        /// Bytes that may be read from `view`: the plugin's documented size, capped at what
        /// Windows actually mapped.
        len: usize,
    }

    // The view is read-only memory owned by this struct until Drop.
    unsafe impl Send for Mapping {}
    unsafe impl Sync for Mapping {}

    impl Mapping {
        pub fn open(name: &str, expected: usize) -> Result<Self, ReaderError> {
            let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
            // SAFETY: `wide` is a NUL-terminated UTF-16 string that outlives the call.
            let handle = unsafe { OpenFileMappingW(FILE_MAP_READ, 0, wide.as_ptr()) };
            if handle.is_null() {
                return Err(last_error());
            }
            // SAFETY: `handle` is a valid mapping handle; mapping the whole object read-only.
            let view = unsafe { MapViewOfFile(handle, FILE_MAP_READ, 0, 0, 0) };
            if view.Value.is_null() {
                let err = last_error();
                // SAFETY: handle was opened above and is not used again.
                unsafe { CloseHandle(handle) };
                return Err(err);
            }
            // SAFETY: an all-zero MEMORY_BASIC_INFORMATION is valid; VirtualQuery fills it in for
            // the region that starts at our view.
            let mut info: MEMORY_BASIC_INFORMATION = unsafe { std::mem::zeroed() };
            let got = unsafe {
                VirtualQuery(
                    view.Value,
                    &mut info,
                    std::mem::size_of::<MEMORY_BASIC_INFORMATION>(),
                )
            };
            let mapped = if got == 0 { 0 } else { info.RegionSize };
            Ok(Mapping {
                handle,
                view: view.Value.cast::<u8>(),
                len: expected.min(mapped),
            })
        }

        /// Copy up to `dst.len()` bytes from the start of the view.
        pub fn copy_to(&self, dst: &mut [u8]) {
            let n = dst.len().min(self.len);
            // SAFETY: `self.len` bytes from `view` are mapped and readable (checked in `open`).
            // A shorter copy leaves the rest of `dst` zeroed, which the decoders reject.
            unsafe { ptr::copy_nonoverlapping(self.view, dst.as_mut_ptr(), n) };
        }
    }

    impl Drop for Mapping {
        fn drop(&mut self) {
            // SAFETY: view and handle came from MapViewOfFile / OpenFileMappingW above.
            unsafe {
                UnmapViewOfFile(MEMORY_MAPPED_VIEW_ADDRESS {
                    Value: self.view as *mut _,
                });
                CloseHandle(self.handle);
            }
        }
    }

    fn last_error() -> ReaderError {
        // SAFETY: GetLastError has no preconditions.
        match unsafe { GetLastError() } {
            ERROR_FILE_NOT_FOUND => ReaderError::NotConnected,
            code => ReaderError::Os(code),
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use super::ReaderError;

    /// No telemetry off Windows: never connects.
    pub struct Mapping;

    impl Mapping {
        pub fn open(_name: &str, _expected: usize) -> Result<Self, ReaderError> {
            Err(ReaderError::NotConnected)
        }

        pub fn copy_to(&self, _dst: &mut [u8]) {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    mod windows_tests {
        use super::*;
        use crate::telemetry::struct_scs;
        use crate::telemetry::struct_v::MMF_SIZE;
        use std::sync::atomic::{AtomicU32, Ordering};
        use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
        use windows_sys::Win32::System::Memory::{
            CreateFileMappingW, MapViewOfFile, UnmapViewOfFile, FILE_MAP_WRITE, PAGE_READWRITE,
        };

        static NEXT: AtomicU32 = AtomicU32::new(0);

        fn unique_name() -> String {
            format!(
                "Local\\ProjectTrucksTest-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            )
        }

        /// A mapping the test owns and can write into, standing in for the plugin.
        struct FakePlugin {
            handle: HANDLE,
            view: *mut u8,
            size: usize,
        }

        impl FakePlugin {
            fn create(name: &str) -> Self {
                Self::create_sized(name, MMF_SIZE)
            }

            fn create_sized(name: &str, size: usize) -> Self {
                let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
                let handle = unsafe {
                    CreateFileMappingW(
                        INVALID_HANDLE_VALUE,
                        std::ptr::null(),
                        PAGE_READWRITE,
                        0,
                        size as u32,
                        wide.as_ptr(),
                    )
                };
                assert!(!handle.is_null(), "CreateFileMappingW failed");
                let view = unsafe { MapViewOfFile(handle, FILE_MAP_WRITE, 0, 0, 0) };
                assert!(!view.Value.is_null(), "MapViewOfFile failed");
                Self {
                    handle,
                    view: view.Value.cast(),
                    size,
                }
            }

            fn write(&self, at: usize, bytes: &[u8]) {
                assert!(at + bytes.len() <= self.size);
                unsafe {
                    std::ptr::copy_nonoverlapping(bytes.as_ptr(), self.view.add(at), bytes.len())
                };
            }

            fn write_u32(&self, at: usize, v: u32) {
                self.write(at, &v.to_le_bytes());
            }

            fn write_f32(&self, at: usize, v: f32) {
                self.write(at, &v.to_le_bytes());
            }
        }

        impl Drop for FakePlugin {
            fn drop(&mut self) {
                unsafe {
                    UnmapViewOfFile(
                        windows_sys::Win32::System::Memory::MEMORY_MAPPED_VIEW_ADDRESS {
                            Value: self.view.cast(),
                        },
                    );
                    CloseHandle(self.handle);
                }
            }
        }

        #[test]
        fn reads_a_known_frame_from_a_named_mapping() {
            let name = unique_name();
            let plugin = FakePlugin::create(&name);
            plugin.write_u32(0, 42); // tick
            plugin.write_u32(8, 10); // revision
            plugin.write_f32(24, 20.0); // speed m/s
            plugin.write_f32(680, 1234.5); // odometer
            plugin.write(688, b"Volvo");

            let reader =
                TelemetryReader::open_named(&name, Plugin::Funbit, Duration::from_secs(1)).unwrap();
            let snap = reader.read().unwrap();
            assert_eq!(snap.tick, 42);
            assert_eq!(snap.plugin_revision, 10);
            assert!((snap.truck.speed_kmh - 72.0).abs() < 1e-3);
            assert_eq!(snap.truck.odometer_km, 1234.5);
            assert_eq!(snap.truck.make.as_deref(), Some("Volvo"));

            // The reader sees later writes (it maps the same memory, not a copy).
            plugin.write_u32(0, 43);
            assert_eq!(reader.read().unwrap().tick, 43);
        }

        #[test]
        fn read_rejects_a_frame_the_plugin_has_not_initialised() {
            let name = unique_name();
            let _plugin = FakePlugin::create(&name);
            let reader =
                TelemetryReader::open_named(&name, Plugin::Funbit, Duration::from_secs(1)).unwrap();
            assert_eq!(
                reader.read().unwrap_err(),
                ReaderError::Frame(FrameError::NotInitialised)
            );
        }

        #[test]
        fn read_rejects_a_mismatched_plugin_revision() {
            let name = unique_name();
            let plugin = FakePlugin::create(&name);
            plugin.write_u32(8, 99);
            let reader =
                TelemetryReader::open_named(&name, Plugin::Funbit, Duration::from_secs(1)).unwrap();
            assert_eq!(
                reader.read().unwrap_err(),
                ReaderError::Frame(FrameError::UnsupportedRevision(99))
            );
        }

        #[test]
        fn open_waits_for_the_game_to_start() {
            let name = unique_name();
            let name2 = name.clone();
            let starter = std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(400));
                let plugin = FakePlugin::create(&name2);
                plugin.write_u32(8, 10);
                // Keep the mapping alive until the reader has opened it.
                std::thread::sleep(Duration::from_secs(2));
            });
            let started = std::time::Instant::now();
            let reader =
                TelemetryReader::open_named(&name, Plugin::Funbit, Duration::from_secs(5)).unwrap();
            assert!(started.elapsed() >= Duration::from_millis(300));
            assert_eq!(reader.read().unwrap().plugin_revision, 10);
            starter.join().unwrap();
        }

        #[test]
        fn reads_an_scs_telemetry_frame_from_a_named_mapping() {
            let name = unique_name();
            let plugin = FakePlugin::create_sized(&name, struct_scs::MMF_SIZE);
            let frame = struct_scs::tests::Builder::active()
                .f32(1056, 4321.0) // odometer
                .str(2364, "Scania") // truck brand
                .0
                .clone();
            plugin.write(0, &frame);
            let reader =
                TelemetryReader::open_named(&name, Plugin::Scs, Duration::from_secs(1)).unwrap();
            assert_eq!(reader.plugin(), Plugin::Scs);
            let snap = reader.read().unwrap();
            assert_eq!(snap.plugin_revision, 12);
            assert_eq!(snap.truck.odometer_km, 4321.0);
            assert_eq!(snap.truck.make.as_deref(), Some("Scania"));

            // Game shutting down: sdkActive goes false.
            plugin.write(0, &[0]);
            assert_eq!(
                reader.read().unwrap_err(),
                ReaderError::ScsFrame(struct_scs::ScsFrameError::NotActive)
            );
        }

        /// A mapping smaller than the plugin's documented size is read only as far as it goes.
        #[test]
        fn a_short_mapping_is_never_over_read() {
            let name = unique_name();
            let _plugin = FakePlugin::create_sized(&name, 4096);
            let reader =
                TelemetryReader::open_named(&name, Plugin::Scs, Duration::from_secs(1)).unwrap();
            assert_eq!(
                reader.read().unwrap_err(),
                ReaderError::ScsFrame(struct_scs::ScsFrameError::NotActive)
            );
        }
    }

    #[test]
    fn open_gives_up_when_the_game_never_starts() {
        let started = std::time::Instant::now();
        let err = TelemetryReader::open_named(
            r"Local\ProjectTrucksTest-never-created",
            Plugin::Scs,
            Duration::from_millis(300),
        )
        .err()
        .expect("must not connect");
        assert_eq!(err, ReaderError::NotConnected);
        let took = started.elapsed();
        assert!(took >= Duration::from_millis(250), "gave up after {took:?}");
        assert!(took < Duration::from_secs(2), "waited {took:?}");
    }

    /// Needs ETS2 running with the plugin installed. Opt in with PROJECTTRUCKS_LIVE_TELEMETRY=1;
    /// CI never sets it, so it passes without doing anything there. With
    /// PROJECTTRUCKS_LIVE_TELEMETRY_DUMP=<file> it also writes the raw frame there (how the
    /// `testdata/*-frame.bin` fixtures are captured). Read-only: it never writes to the game.
    #[test]
    fn live_game() {
        if std::env::var_os("PROJECTTRUCKS_LIVE_TELEMETRY").is_none() {
            eprintln!("skipped: set PROJECTTRUCKS_LIVE_TELEMETRY=1 with ETS2 running");
            return;
        }
        // PROJECTTRUCKS_LIVE_TELEMETRY=funbit reads the old plugin's mapping instead (both can be
        // loaded at once, which is how the two decoders were compared against each other).
        let reader = if std::env::var("PROJECTTRUCKS_LIVE_TELEMETRY").as_deref() == Ok("funbit") {
            TelemetryReader::open_named(Plugin::Funbit.mapping_name(), Plugin::Funbit, OPEN_TIMEOUT)
        } else {
            TelemetryReader::open()
        }
        .expect("connect to the live plugin");
        eprintln!("plugin: {:?}", reader.plugin());
        if let Some(path) = std::env::var_os("PROJECTTRUCKS_LIVE_TELEMETRY_DUMP") {
            std::fs::write(&path, reader.read_raw()).expect("write the frame dump");
        }
        let snap = reader.read().expect("read a live frame");
        eprintln!("{snap:#?}");
    }
}

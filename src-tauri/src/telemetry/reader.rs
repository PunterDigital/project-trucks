//! Reads the plugin's memory-mapped file.
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

use super::struct_v::{FrameError, MMF_NAME};
use super::TelemetrySnapshot;

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
}

pub struct TelemetryReader {
    mapping: platform::Mapping,
}

impl TelemetryReader {
    /// Connect to the plugin's mapping, retrying for about 5 seconds.
    pub fn open() -> Result<Self, ReaderError> {
        Self::open_named(MMF_NAME, OPEN_TIMEOUT)
    }

    /// Like `open()` with another mapping name and wait (tests use their own mapping).
    pub fn open_named(name: &str, timeout: Duration) -> Result<Self, ReaderError> {
        let started = std::time::Instant::now();
        let mut backoff = FIRST_BACKOFF;
        loop {
            match platform::Mapping::open(name) {
                Ok(mapping) => return Ok(Self { mapping }),
                // Only "it isn't there yet" is worth waiting for.
                Err(ReaderError::NotConnected) if started.elapsed() < timeout => {
                    std::thread::sleep(backoff.min(timeout.saturating_sub(started.elapsed())));
                    backoff = (backoff * 2).min(MAX_BACKOFF);
                }
                Err(e) => return Err(e),
            }
        }
    }

    /// Copy the current frame out of the mapping and convert it.
    pub fn read(&self) -> Result<TelemetrySnapshot, ReaderError> {
        let mut buf = [0u8; super::struct_v::FRAME_SIZE];
        self.mapping.copy_to(&mut buf);
        Ok(super::TelemetryFrame::from_bytes(&buf)?.to_snapshot())
    }
}

#[cfg(windows)]
mod platform {
    use std::ptr;

    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_FILE_NOT_FOUND, HANDLE};
    use windows_sys::Win32::System::Memory::{
        MapViewOfFile, OpenFileMappingW, UnmapViewOfFile, FILE_MAP_READ, MEMORY_MAPPED_VIEW_ADDRESS,
    };

    use super::super::struct_v::MMF_SIZE;
    use super::ReaderError;

    pub struct Mapping {
        handle: HANDLE,
        view: *const u8,
    }

    // The view is read-only memory owned by this struct until Drop.
    unsafe impl Send for Mapping {}
    unsafe impl Sync for Mapping {}

    impl Mapping {
        pub fn open(name: &str) -> Result<Self, ReaderError> {
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
            Ok(Mapping {
                handle,
                view: view.Value.cast::<u8>(),
            })
        }

        /// Copy up to `dst.len()` bytes from the start of the view.
        pub fn copy_to(&self, dst: &mut [u8]) {
            let n = dst.len().min(MMF_SIZE);
            // SAFETY: the plugin's mapping is MMF_SIZE bytes and `n <= MMF_SIZE`. A mapping
            // created smaller than that would be a protocol violation by the other process;
            // `Mapping::open` in tests uses full-size mappings too.
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
        pub fn open(_name: &str) -> Result<Self, ReaderError> {
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
        }

        impl FakePlugin {
            fn create(name: &str) -> Self {
                let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
                let handle = unsafe {
                    CreateFileMappingW(
                        INVALID_HANDLE_VALUE,
                        std::ptr::null(),
                        PAGE_READWRITE,
                        0,
                        MMF_SIZE as u32,
                        wide.as_ptr(),
                    )
                };
                assert!(!handle.is_null(), "CreateFileMappingW failed");
                let view = unsafe { MapViewOfFile(handle, FILE_MAP_WRITE, 0, 0, 0) };
                assert!(!view.Value.is_null(), "MapViewOfFile failed");
                Self {
                    handle,
                    view: view.Value.cast(),
                }
            }

            fn write(&self, at: usize, bytes: &[u8]) {
                assert!(at + bytes.len() <= MMF_SIZE);
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

            let reader = TelemetryReader::open_named(&name, Duration::from_secs(1)).unwrap();
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
            let reader = TelemetryReader::open_named(&name, Duration::from_secs(1)).unwrap();
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
            let reader = TelemetryReader::open_named(&name, Duration::from_secs(1)).unwrap();
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
            let reader = TelemetryReader::open_named(&name, Duration::from_secs(5)).unwrap();
            assert!(started.elapsed() >= Duration::from_millis(300));
            assert_eq!(reader.read().unwrap().plugin_revision, 10);
            starter.join().unwrap();
        }
    }

    #[test]
    fn open_gives_up_when_the_game_never_starts() {
        let started = std::time::Instant::now();
        let err = TelemetryReader::open_named(
            "Local\\ProjectTrucksTest-never-created",
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
    /// CI never sets it, so it passes without doing anything there.
    #[test]
    fn live_game() {
        if std::env::var_os("PROJECTTRUCKS_LIVE_TELEMETRY").is_none() {
            eprintln!("skipped: set PROJECTTRUCKS_LIVE_TELEMETRY=1 with ETS2 running");
            return;
        }
        let reader = TelemetryReader::open().expect("connect to the live plugin");
        let snap = reader.read().expect("read a live frame");
        eprintln!("{snap:#?}");
    }
}

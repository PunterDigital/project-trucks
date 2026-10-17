//! Prints a live ETS2 telemetry snapshot once a second.
//!
//! Needs Windows, ETS2 running, and the telemetry plugin installed:
//! `cargo run --example dump_telemetry` (from `src-tauri`).

use std::time::Duration;

use projecttrucks_lib::telemetry::TelemetryReader;

fn main() {
    println!("Waiting for ETS2 (up to 5 s)...");
    let reader = match TelemetryReader::open() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{e}. Is ETS2 running with the telemetry plugin installed?");
            std::process::exit(1);
        }
    };
    println!("Connected. Ctrl+C to stop.");
    loop {
        match reader.read() {
            Ok(snap) => println!("{snap:#?}"),
            Err(e) => eprintln!("read failed: {e}"),
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}

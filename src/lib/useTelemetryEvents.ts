// Events the Rust telemetry task emits (src-tauri/src/telemetry/runner.rs).

import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";

import { api } from "@/lib/api";

/** Telemetry connection / driving status, as sent in `telemetry:status`. */
export type TelemetryStatus =
  | "not_connected"
  | "parked"
  | "free_roam"
  | "delivery";

/**
 * A counter that goes up whenever auto-capture writes a session or a job.
 * Put it in a data-loading effect's dependencies to refetch when rows appear.
 */
export function useCaptureRefresh(): number {
  const [count, setCount] = useState(0);

  useEffect(() => {
    let cancelled = false;
    const unlisten: Array<() => void> = [];
    for (const name of ["session:created", "job:created"]) {
      listen(name, () => setCount((c) => c + 1))
        .then((off) => {
          if (cancelled) off();
          else unlisten.push(off);
        })
        .catch(() => {
          // No event bridge (plain browser): nothing to refresh from.
        });
    }
    return () => {
      cancelled = true;
      unlisten.forEach((off) => off());
    };
  }, []);

  return count;
}

/** Latest `telemetry:status` (asks for the current one on mount). */
export function useTelemetryStatus(): TelemetryStatus {
  const [status, setStatus] = useState<TelemetryStatus>("not_connected");

  useEffect(() => {
    let cancelled = false;
    let off: (() => void) | undefined;
    listen<TelemetryStatus>("telemetry:status", (e) => setStatus(e.payload))
      .then((fn) => {
        if (cancelled) fn();
        else off = fn;
      })
      .catch(() => {});
    api
      .getTelemetryStatus()
      .then((s) => {
        if (!cancelled) setStatus(s);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
      off?.();
    };
  }, []);

  return status;
}

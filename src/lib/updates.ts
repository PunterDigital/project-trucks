// App update status from the Rust updater (src-tauri/src/updater.rs), kept in one small store so
// the banner and the Settings page show the same thing. The Rust side emits `update:status` on
// every change; the first value comes from `get_update_status`.

import { useSyncExternalStore } from "react";
import { listen } from "@tauri-apps/api/event";

import { api } from "@/lib/api";
import type { UpdateStatus } from "@/types";

let current: UpdateStatus | null = null;
let heardEvent = false;
let started = false;
const listeners = new Set<() => void>();

export function publishUpdateStatus(next: UpdateStatus) {
  current = next;
  listeners.forEach((l) => l());
}

function start() {
  if (started) return;
  started = true;
  listen<UpdateStatus>("update:status", (e) => {
    heardEvent = true;
    publishUpdateStatus(e.payload);
  }).catch(() => {
    // No event bridge (plain browser).
  });
  api
    .getUpdateStatus()
    // An event that arrived first is newer than this answer.
    .then((s) => !heardEvent && publishUpdateStatus(s))
    .catch(() => {});
}

/** The latest update status, or `null` until the first one arrives. */
export function useUpdateStatus(): UpdateStatus | null {
  return useSyncExternalStore(
    (listener) => {
      start();
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    () => current,
  );
}

/** Whole-number download progress, or `null` when the size isn't known. */
export function downloadPercent(status: Pick<UpdateStatus, "received" | "total">): number | null {
  if (status.total <= 0) return null;
  return Math.min(100, Math.floor((status.received / status.total) * 100));
}

/** One line for the Settings page. */
export function updateSummary(status: UpdateStatus): string {
  const latest = status.latest ? `v${status.latest.version}` : "A new version";
  switch (status.state) {
    case "disabled":
      return status.message ?? "Update checks are off in this build.";
    case "idle":
      return "Not checked yet.";
    case "checking":
      return "Checking for updates…";
    case "upToDate":
      return "You have the latest version.";
    case "available":
      return `${latest} is available.`;
    case "downloading": {
      const pct = downloadPercent(status);
      return `Downloading ${latest}${pct === null ? "…" : ` (${pct}%)`}`;
    }
    case "installing":
      return `Installing ${latest}. ProjectTrucks will restart.`;
    case "error":
      return status.message ?? "The update check failed.";
  }
}

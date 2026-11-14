// ProjectTrucks Hub status from the Rust side (src-tauri/src/hub), in one small store so every
// component shows the same thing. Rust emits `hub:status` on every change; the first value comes
// from `get_hub_status`.

import { useSyncExternalStore } from "react";
import { listen } from "@tauri-apps/api/event";

import { api } from "@/lib/api";
import type { HubStatus } from "@/types";

let current: HubStatus | null = null;
let heardEvent = false;
let started = false;
const listeners = new Set<() => void>();

export function publishHubStatus(next: HubStatus) {
  current = next;
  listeners.forEach((l) => l());
}

function start() {
  if (started) return;
  started = true;
  listen<HubStatus>("hub:status", (e) => {
    heardEvent = true;
    publishHubStatus(e.payload);
  }).catch(() => {
    // No event bridge (plain browser).
  });
  api
    .getHubStatus()
    .then((s) => !heardEvent && publishHubStatus(s))
    .catch(() => {});
}

/** The latest Hub status, or `null` until the first one arrives. */
export function useHubStatus(): HubStatus | null {
  return useSyncExternalStore(
    (listener) => {
      start();
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    () => current,
  );
}

function plural(n: number, one: string, many = `${one}s`) {
  return `${n} ${n === 1 ? one : many}`;
}

/** One line about the sync, for the Settings page. */
export function syncSummary(
  status: Pick<HubStatus, "synced" | "pending" | "failed">,
): string {
  const parts = [`${plural(status.synced, "job")} on the Hub`];
  if (status.pending > 0) parts.push(`${status.pending} waiting to sync`);
  if (status.failed > 0) parts.push(`${status.failed} refused`);
  return parts.join(" · ");
}

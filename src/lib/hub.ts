// ProjectTrucks Hub status from the Rust side (src-tauri/src/hub), in one small store so every
// component shows the same thing. Rust emits `hub:status` on every change; the first value comes
// from `get_hub_status`.

import { useEffect, useMemo, useSyncExternalStore } from "react";
import { listen } from "@tauri-apps/api/event";

import { api } from "@/lib/api";
import type { HubStatus, JobHubState } from "@/types";

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
export function syncSummary(status: Pick<HubStatus, "synced" | "pending" | "failed">): string {
  const parts = [`${plural(status.synced, "job")} on the Hub`];
  if (status.pending > 0) parts.push(`${status.pending} waiting to sync`);
  if (status.failed > 0) parts.push(`${status.failed} refused`);
  return parts.join(" · ");
}

// --- Per-job Hub state (#4654) ---------------------------------------------

export const SYNC_LABELS: Record<JobHubState["state"], string> = {
  synced: "On the Hub",
  pending: "Syncing",
  refused: "Refused by the Hub",
  local: "Stays on this PC",
  notSynced: "Not on the Hub",
};

const jobStates = new Map<string, JobHubState>();
const jobListeners = new Set<() => void>();
let jobVersion = 0;

function bumpJobs() {
  jobVersion += 1;
  jobListeners.forEach((l) => l());
}

/** A fresh state from `hub_set_hidden`, shown everywhere at once. */
export function publishJobHubState(state: JobHubState) {
  jobStates.set(state.jobId, state);
  bumpJobs();
}

/**
 * The Hub state of each job in `ids`, refreshed whenever the Hub status changes (a sync round
 * ends, a job is queued). Empty while unlinked.
 */
export function useJobHubStates(ids: string[]): Map<string, JobHubState> {
  const status = useHubStatus();
  const key = ids.join(",");
  const version = useSyncExternalStore(
    (listener) => {
      jobListeners.add(listener);
      return () => jobListeners.delete(listener);
    },
    () => jobVersion,
  );
  const linked = status?.linked ?? false;
  const statusKey = status
    ? `${status.synced}/${status.pending}/${status.failed}/${status.lastSyncAt}`
    : "";

  useEffect(() => {
    if (!linked || key === "") return;
    let cancelled = false;
    api
      .hubJobStates(key.split(","))
      .then((states) => {
        if (cancelled) return;
        for (const s of states) jobStates.set(s.jobId, s);
        bumpJobs();
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [linked, key, statusKey]);

  return useMemo(() => {
    void version;
    const out = new Map<string, JobHubState>();
    if (!linked) return out;
    for (const id of ids) {
      const s = jobStates.get(id);
      if (s) out.set(id, s);
    }
    return out;
    // eslint-disable-next-line react-hooks/exhaustive-deps -- `key` stands for `ids`
  }, [linked, key, version]);
}

// --- Header pill (#4654) ---------------------------------------------------

export interface HubPill {
  state: "linked" | "syncing" | "offline" | "attention";
  label: string;
  title: string;
}

/** The header pill, or `null` when there's nothing to show (Hub off or not linked). */
export function hubPill(status: HubStatus | null): HubPill | null {
  if (!status || status.hubUrl === null || !status.linked || !status.account) return null;
  const who = status.account.username ? `@${status.account.username}` : status.account.name;
  if (status.lastError?.startsWith("Couldn't sync")) {
    return {
      state: "offline",
      label: "Hub offline",
      title: `${status.lastError} Click for Hub settings.`,
    };
  }
  if (status.pending > 0) {
    return {
      state: "syncing",
      label: `Syncing ${status.pending}`,
      title: `Linked as ${who}. ${plural(status.pending, "job")} on the way to the Hub.`,
    };
  }
  if (status.failed > 0) {
    return {
      state: "attention",
      label: `${status.failed} refused`,
      title: `Linked as ${who}. ${status.lastError ?? "The Hub refused some jobs."} Click to retry.`,
    };
  }
  return {
    state: "linked",
    label: who,
    title: `Linked to the ProjectTrucks Hub as ${who}. ${syncSummary(status)}.`,
  };
}

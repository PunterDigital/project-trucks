// App settings, kept in one small store so the header pill, the Settings page and the distance
// formatters all see the same values. The source of truth is the Rust side (`settings` table).

import { useSyncExternalStore } from "react";

import { api } from "@/lib/api";
import { setUnits } from "@/lib/format";
import type { Settings, SettingsPatch } from "@/types";

let current: Settings | null = null;
const listeners = new Set<() => void>();

function publish(next: Settings) {
  current = next;
  setUnits(next.units);
  listeners.forEach((l) => l());
}

/** Fetch the saved settings (call once when the app starts). */
export async function loadSettings(): Promise<void> {
  publish(await api.getSettings());
}

/** Save a change and publish the result to every subscriber. */
export async function updateSettings(patch: SettingsPatch): Promise<Settings> {
  const next = await api.updateSettings(patch);
  publish(next);
  return next;
}

/** The settings, or `null` until the first load finishes. */
export function useSettings(): Settings | null {
  return useSyncExternalStore(
    (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    () => current,
  );
}

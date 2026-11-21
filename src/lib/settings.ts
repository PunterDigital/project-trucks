// App settings, kept in one small store so the header pill, the Settings page and the distance
// formatters all see the same values. The source of truth is the Rust side (`settings` table).

import { useSyncExternalStore } from "react";

import { api } from "@/lib/api";
import { setUnits } from "@/lib/format";
import { applyTheme } from "@/lib/theme";
import type { Settings, SettingsPatch } from "@/types";

let current: Settings | null = null;
const listeners = new Set<() => void>();

function publish(next: Settings) {
  current = next;
  setUnits(next.units);
  applyTheme(next.theme);
  listeners.forEach((l) => l());
}

/** Fetch the saved settings (call once when the app starts). */
export async function loadSettings(): Promise<void> {
  publish(await api.getSettings());
}

// Saves go out one at a time. Each `update_settings` writes its keys and answers with the whole
// row, so two saves in flight (the ETS2 folder's blur, then a click on Miles) could answer out of
// order and the older reply put the old units back on the page, although Miles was saved.
let queue: Promise<unknown> = Promise.resolve();

/** Save a change and publish the result to every subscriber. */
export function updateSettings(patch: SettingsPatch): Promise<Settings> {
  const run = queue.then(async () => {
    const next = await api.updateSettings(patch);
    publish(next);
    return next;
  });
  // A failed save rejects its own caller but doesn't hold up the next one.
  queue = run.catch(() => undefined);
  return run;
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

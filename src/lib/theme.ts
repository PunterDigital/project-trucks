// Light / Dark / System (#4658). The choice is a setting like the others (Rust `settings` table);
// this module turns it into the `dark` class on <html> that the shadcn tokens in index.css key on.
//
// First paint: `public/theme-init.js` runs from index.html's <head> before anything is drawn and
// applies the choice mirrored in localStorage (same WebView2 profile folder as the database), so
// a dark start never flashes white. Keep `STORAGE_KEY` and the rule in `resolveTheme` in step
// with that file.

import { useSyncExternalStore } from "react";

import type { Theme } from "@/types";

export type ResolvedTheme = "light" | "dark";

export const STORAGE_KEY = "projecttrucks-theme";
export const THEMES: readonly Theme[] = ["light", "dark", "system"];

const DARK_QUERY = "(prefers-color-scheme: dark)";

/** What a choice means on screen; "system" follows Windows' app mode. */
export function resolveTheme(theme: Theme, systemDark: boolean): ResolvedTheme {
  if (theme === "system") return systemDark ? "dark" : "light";
  return theme;
}

function systemDark(): boolean {
  return typeof window !== "undefined" && typeof window.matchMedia === "function"
    ? window.matchMedia(DARK_QUERY).matches
    : false;
}

let chosen: Theme = "system";
let resolved: ResolvedTheme = "light";
let watching = false;
const listeners = new Set<() => void>();

function paint() {
  const next = resolveTheme(chosen, systemDark());
  const root = document.documentElement;
  root.classList.toggle("dark", next === "dark");
  // Native controls (scrollbars, date pickers) follow this.
  root.style.colorScheme = next;
  if (next !== resolved) {
    resolved = next;
    listeners.forEach((l) => l());
  }
}

/** Apply a theme choice to the page and remember it for the next start's first paint. */
export function applyTheme(theme: Theme): void {
  chosen = theme;
  if (typeof document === "undefined") return;
  try {
    window.localStorage.setItem(STORAGE_KEY, theme);
  } catch {
    // Storage can be unavailable; the Rust setting still holds the choice.
  }
  if (!watching && typeof window.matchMedia === "function") {
    watching = true;
    // "System" keeps following Windows while the app is open.
    window.matchMedia(DARK_QUERY).addEventListener("change", paint);
  }
  paint();
}

/** "light" or "dark" as currently shown (for components that can't use CSS, e.g. toasts). */
export function useResolvedTheme(): ResolvedTheme {
  return useSyncExternalStore(
    (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    () => resolved,
  );
}

// The head script has already set the class; start from what is on screen.
if (typeof document !== "undefined" && document.documentElement.classList.contains("dark")) {
  resolved = "dark";
}

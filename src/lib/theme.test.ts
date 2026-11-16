// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from "vitest";

import script from "../../public/theme-init.js?raw";
import { applyTheme, resolveTheme, STORAGE_KEY } from "./theme";

/** jsdom has no matchMedia: a fake whose answer and `change` listeners the test controls. */
function fakeSystem(initialDark: boolean) {
  let dark = initialDark;
  const listeners = new Set<() => void>();
  window.matchMedia = vi.fn().mockImplementation(() => ({
    get matches() {
      return dark;
    },
    addEventListener: (_: string, l: () => void) => listeners.add(l),
    removeEventListener: (_: string, l: () => void) => listeners.delete(l),
  }));
  return {
    set(next: boolean) {
      dark = next;
      listeners.forEach((l) => l());
    },
  };
}

// One fake for the whole file: applyTheme subscribes to the system preference once.
const system = fakeSystem(false);

const isDark = () => document.documentElement.classList.contains("dark");

describe("resolveTheme", () => {
  it("keeps an explicit choice whatever Windows says", () => {
    expect(resolveTheme("light", true)).toBe("light");
    expect(resolveTheme("dark", false)).toBe("dark");
  });

  it("follows Windows for system", () => {
    expect(resolveTheme("system", true)).toBe("dark");
    expect(resolveTheme("system", false)).toBe("light");
  });
});

describe("applyTheme", () => {
  beforeEach(() => {
    system.set(false);
    window.localStorage.clear();
    document.documentElement.className = "";
  });

  it("puts the dark class on <html> and remembers the choice", () => {
    applyTheme("dark");
    expect(isDark()).toBe(true);
    expect(document.documentElement.style.colorScheme).toBe("dark");
    expect(window.localStorage.getItem(STORAGE_KEY)).toBe("dark");

    applyTheme("light");
    expect(isDark()).toBe(false);
    expect(document.documentElement.style.colorScheme).toBe("light");
    expect(window.localStorage.getItem(STORAGE_KEY)).toBe("light");
  });

  it("follows Windows while set to system, and stops when a theme is chosen", () => {
    applyTheme("system");
    expect(isDark()).toBe(false);
    system.set(true);
    expect(isDark()).toBe(true);
    system.set(false);
    expect(isDark()).toBe(false);

    applyTheme("light");
    system.set(true);
    expect(isDark()).toBe(false);
  });
});

describe("theme-init.js (first paint)", () => {
  const run = () => new Function(script)();

  beforeEach(() => {
    system.set(false);
    window.localStorage.clear();
    document.documentElement.className = "";
  });

  it("uses the same storage key as the app", () => {
    expect(script).toContain(`"${STORAGE_KEY}"`);
  });

  it.each([
    ["dark", false, true],
    ["dark", true, true],
    ["light", true, false],
    ["system", true, true],
    ["system", false, false],
    [null, true, true],
    [null, false, false],
  ] as const)("stored %s, Windows dark %s -> dark %s", (stored, systemDark, expected) => {
    if (stored) window.localStorage.setItem(STORAGE_KEY, stored);
    system.set(systemDark);
    run();
    expect(isDark()).toBe(expected);
  });
});

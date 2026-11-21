import { beforeEach, describe, expect, it, vi } from "vitest";

import type { Settings, SettingsPatch } from "@/types";

// The Rust side writes a patch's keys and then reads the whole settings row back. Two saves in
// flight at once (the ETS2 folder's blur, then a click on Miles) could answer out of order, and
// the older reply put Kilometres back on the page (CI failure on main after #5243).

type Pending = { patch: SettingsPatch; resolve: () => void };
const pending: Pending[] = [];
let stored: Settings;

vi.mock("@/lib/api", () => ({
  api: {
    getSettings: vi.fn(async () => ({ ...stored })),
    // Applies the patch when the test releases the call, and answers with the row at that moment.
    updateSettings: vi.fn(
      (patch: SettingsPatch) =>
        new Promise<Settings>((done) => {
          pending.push({
            patch,
            resolve: () => {
              stored = { ...stored, ...patch } as Settings;
              done({ ...stored });
            },
          });
        }),
    ),
  },
}));
vi.mock("@/lib/theme", () => ({ applyTheme: vi.fn() }));

const { api } = await import("@/lib/api");
const { loadSettings, updateSettings } = await import("./settings");

const flush = () => new Promise((r) => setTimeout(r, 0));

beforeEach(async () => {
  pending.length = 0;
  vi.mocked(api.updateSettings).mockClear();
  stored = {
    units: "km",
    ets2InstallPath: null,
    theme: "system",
    autoCapture: false,
    checkUpdates: true,
    autoInstallUpdates: false,
  } as unknown as Settings;
  await loadSettings();
});

describe("updateSettings", () => {
  it("sends one save at a time, so a later change is never undone by an earlier reply", async () => {
    const first = updateSettings({ ets2InstallPath: "D:\\Games\\ETS2" });
    const second = updateSettings({ units: "mi" });
    await flush();

    // The second save waits for the first instead of racing it.
    expect(api.updateSettings).toHaveBeenCalledTimes(1);
    pending[0].resolve();
    await first;
    await flush();
    expect(api.updateSettings).toHaveBeenCalledTimes(2);
    pending[1].resolve();

    const last = await second;
    expect(last.units).toBe("mi");
    expect(last.ets2InstallPath).toBe("D:\\Games\\ETS2");
  });

  it("a failed save rejects its caller and doesn't block the next one", async () => {
    vi.mocked(api.updateSettings).mockRejectedValueOnce(new Error("disk full"));
    const failed = updateSettings({ units: "mi" });
    const next = updateSettings({ theme: "dark" } as SettingsPatch);
    await expect(failed).rejects.toThrow("disk full");
    await flush();
    pending[0].resolve();
    expect((await next).theme).toBe("dark");
  });
});

import { createHash } from "node:crypto";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync, existsSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";

import { expect, test } from "./fixtures";

test.skip(process.platform !== "win32", "WebView2 (Windows) is the only webview with CDP");

// A recorded drive, replayed in place of ETS2 (see src-tauri/src/telemetry/replay.rs):
// park, free-roam 10 km, haul Calais -> Rotterdam for 26 km, deliver, drive 5 km, park.
// `t` is virtual seconds (one sample per 400 ms of real time), so the last sample crosses the 5 minute idle limit at once.

type Job = {
  income: number;
  deadline: null;
  sourceCity: string;
  destinationCity: string;
  sourceCompany: string;
  destinationCompany: string;
  cargoId: string;
  cargoName: string;
  cargoMassKg: number;
};

const job: Job = {
  income: 12500,
  deadline: null,
  sourceCity: "Calais",
  destinationCity: "Rotterdam",
  sourceCompany: "Tradeaux",
  destinationCompany: "Kaarfor",
  cargoId: "apples",
  cargoName: "Apples",
  cargoMassKg: 18000,
};

function snapshot(odometerKm: number, speedKmh: number, withJob: boolean) {
  return {
    tick: 0,
    paused: false,
    gameTime: null,
    pluginRevision: 10,
    gameVersion: [1, 55],
    truck: {
      make: "Scania",
      makeId: "scania",
      model: "S 730",
      speedKmh,
      odometerKm,
      engineEnabled: true,
      trailerAttached: withJob,
      position: [odometerKm * 1000, 0, 0],
      fuelLitres: 300,
      fuelCapacityLitres: 600,
      wearChassis: 0,
      wearTrailer: 0,
    },
    job: withJob ? job : null,
  };
}

const replay = {
  samples: [
    { t: 0, snapshot: snapshot(1000, 0, false) },
    { t: 1, snapshot: snapshot(1005, 80, false) },
    { t: 2, snapshot: snapshot(1010, 80, false) },
    { t: 3, snapshot: snapshot(1015, 80, true) },
    { t: 4, snapshot: snapshot(1030, 80, true) },
    { t: 5, snapshot: snapshot(1040, 80, true) },
    { t: 6, snapshot: snapshot(1041, 40, false) },
    { t: 7, snapshot: snapshot(1046, 80, false) },
    { t: 8, snapshot: snapshot(1046, 0, false) },
    { t: 400, snapshot: snapshot(1046, 0, false) },
  ],
};

test.describe("auto-capture from a replayed drive", () => {
  test.use({ replay });

  test("the delivery and both free-roam stretches appear in the UI", async ({ page }) => {
    // Auto-capture is off by default: nothing is recorded until it is switched on.
    await page.getByRole("link", { name: "Settings", exact: true }).click();
    await page.getByLabel("Record drives and deliveries from ETS2 automatically").click();
    await expect(page.getByTestId("auto-capture-pill")).toHaveText("Auto-capture: on");

    // The replay takes a few seconds, so this page is open when the job is written: the
    // `job:created` event has to refetch it.
    await page.getByRole("link", { name: "Jobs", exact: true }).click();
    const row = page.getByRole("row").filter({ hasText: "Rotterdam" });
    await expect(row).toHaveCount(1);
    await expect(row).toContainText("Calais");
    await expect(row).toContainText("Apples");

    // Every kilometre landed in a session: 10 free-roam + 26 delivery + 5 free-roam. The last
    // session is written when the replay ends, a while after the job.
    await expect
      .poll(async () => {
        const sessions = await page.evaluate(async () => {
          const invoke = (
            window as unknown as {
              __TAURI_INTERNALS__: { invoke: (cmd: string, args?: unknown) => Promise<unknown> };
            }
          ).__TAURI_INTERNALS__.invoke;
          return (await invoke("list_sessions", { filter: null })) as {
            items: { kind: string; status: string; distanceKm: number }[];
          };
        });
        return sessions.items.map((s) => `${s.kind}:${s.status}:${s.distanceKm}`).sort();
      })
      .toEqual(["FREE_ROAM:COMPLETED:10", "FREE_ROAM:COMPLETED:5", "JOB:COMPLETED:26"]);

    // The truck rollup is live on the Trucks page.
    await page.getByRole("link", { name: "Trucks", exact: true }).click();
    const truck = page.getByRole("row").filter({ hasText: "S 730" });
    await expect(truck).toHaveCount(1);
    await expect(truck).toContainText("41");

    // The status started as not_connected and ended parked (the event or the command got it there).
    await page.getByRole("link", { name: "Dashboard", exact: true }).click();
    await expect(page.getByText("telemetry: parked")).toBeVisible();
  });
});

const parkedReplay = {
  samples: [0, 1, 2].map((t) => ({ t, snapshot: snapshot(2000, 0, false) })),
};

test.describe("settings", () => {
  test.use({ replay: parkedReplay });

  test("auto-capture is off by default and the toggle drives the pill and the status badge", async ({
    page,
  }) => {
    await page.getByRole("link", { name: "Settings", exact: true }).click();
    const pill = page.getByTestId("auto-capture-pill");
    const badge = page.getByTestId("telemetry-badge");
    const toggle = page.getByLabel("Record drives and deliveries from ETS2 automatically");

    await expect(pill).toHaveText("Auto-capture: off");
    await expect(toggle).not.toBeChecked();
    await expect(badge).toHaveText("Not connected");
    // Off means off: well past the replay's few samples, still nothing connected.
    await page.waitForTimeout(2000);
    await expect(badge).toHaveText("Not connected");

    await toggle.click();
    await expect(pill).toHaveText("Auto-capture: on");
    await expect(badge).toHaveText("Connected, parked", { timeout: 2500 });

    await toggle.click();
    await expect(pill).toHaveText("Auto-capture: off");
    await expect(badge).toHaveText("Not connected", { timeout: 2500 });
  });

  test("units and the ETS2 folder are saved", async ({ page }) => {
    await page.getByRole("link", { name: "Settings", exact: true }).click();
    const folder = page.getByLabel("ETS2 install folder");
    await folder.fill("D:\\Games\\ETS2");
    await folder.blur();
    await page.getByRole("button", { name: "Miles" }).click();
    await expect(page.getByRole("button", { name: "Miles" })).toHaveAttribute("aria-pressed", "true");

    await page.reload({ waitUntil: "load" });
    await page.getByRole("link", { name: "Settings", exact: true }).click();
    await expect(page.getByLabel("ETS2 install folder")).toHaveValue("D:\\Games\\ETS2");
    await expect(page.getByRole("button", { name: "Miles" })).toHaveAttribute("aria-pressed", "true");
    await expect(page.getByRole("button", { name: "Kilometres" })).toHaveAttribute(
      "aria-pressed",
      "false",
    );
  });

  test("the plugin installer reports status and installs into a fake ETS2 folder", async ({
    page,
  }) => {
    const game = mkdtempSync(path.join(tmpdir(), "projecttrucks-fake-ets2-"));
    try {
      const bin = path.join(game, "bin", "win_x64");
      mkdirSync(bin, { recursive: true });
      const notEts2 = mkdtempSync(path.join(tmpdir(), "projecttrucks-not-ets2-"));

      await page.getByRole("link", { name: "Settings", exact: true }).click();
      const folder = page.getByLabel("ETS2 install folder");
      const badge = page.getByTestId("plugin-badge");
      const install = page.getByRole("button", { name: /(Install|Update) plugin/ });

      // A folder without the game's exe is rejected.
      await folder.fill(notEts2);
      await folder.blur();
      await expect(badge).toHaveText("Not an ETS2 folder");
      await expect(install).toBeDisabled();
      rmSync(notEts2, { recursive: true, force: true });

      // The exe makes it look like ETS2: not installed yet.
      writeFileSync(path.join(bin, "eurotrucks2.exe"), "exe");
      await folder.fill(game);
      await folder.blur();
      await expect(badge).toHaveText("Not installed");
      await install.click();
      await expect(badge).toHaveText("Installed");
      await expect(install).toBeDisabled();

      const dll = path.join(bin, "plugins", "ets2-telemetry-server.dll");
      const bundled = path.join(
        import.meta.dirname,
        "..",
        "assets",
        "telemetry-plugin",
        "win_x64",
        "ets2-telemetry-server.dll",
      );
      const sha = (p: string) => createHash("sha256").update(readFileSync(p)).digest("hex");
      expect(sha(dll)).toBe(sha(bundled));

      // A different DLL is outdated; updating keeps it as .bak.
      writeFileSync(dll, "some other plugin");
      await folder.fill(game + "\\");
      await folder.blur();
      await folder.fill(game);
      await folder.blur();
      await expect(badge).toHaveText("Outdated");
      await page.getByRole("button", { name: "Update plugin" }).click();
      await expect(badge).toHaveText("Installed");
      expect(sha(dll)).toBe(sha(bundled));
      expect(readFileSync(dll + ".bak", "utf8")).toBe("some other plugin");
      expect(existsSync(dll + ".bak")).toBe(true);
    } finally {
      rmSync(game, { recursive: true, force: true });
    }
  });
});

test.describe("first-launch plugin prompt", () => {
  const dllIn = (game: string) => path.join(game, "bin", "win_x64", "plugins", "ets2-telemetry-server.dll");

  function fakeEts2(): string {
    const game = mkdtempSync(path.join(tmpdir(), "projecttrucks-fake-ets2-"));
    mkdirSync(path.join(game, "bin", "win_x64"), { recursive: true });
    writeFileSync(path.join(game, "bin", "win_x64", "eurotrucks2.exe"), "exe");
    return game;
  }

  test("suggests the plugin, installs it into a fake ETS2 folder and stays quiet afterwards", async ({
    page,
  }) => {
    const game = fakeEts2();
    try {
      const prompt = page.getByTestId("plugin-prompt");
      await expect(prompt).toContainText("Set up live telemetry?");

      // A folder that is not ETS2 fails with a message, not a crash.
      const notEts2 = mkdtempSync(path.join(tmpdir(), "projecttrucks-not-ets2-"));
      await prompt.getByLabel("ETS2 folder for the plugin").fill(notEts2);
      await prompt.getByRole("button", { name: "Install", exact: true }).click();
      await expect(prompt.getByRole("alert")).toContainText("does not look like an ETS2 folder");
      rmSync(notEts2, { recursive: true, force: true });

      await prompt.getByLabel("ETS2 folder for the plugin").fill(game);
      await prompt.getByRole("button", { name: "Install", exact: true }).click();
      await expect(prompt).toBeHidden();
      expect(existsSync(dllIn(game))).toBe(true);

      // The next launch (a reload re-reads the saved settings) does not ask again.
      await page.reload({ waitUntil: "load" });
      await page.getByRole("link", { name: "Settings", exact: true }).click();
      await expect(page.getByTestId("plugin-badge")).toHaveText("Installed");
      await page.getByRole("link", { name: "Dashboard", exact: true }).click();
      await expect(prompt).toBeHidden();
    } finally {
      rmSync(game, { recursive: true, force: true });
    }
  });

  test("an outdated plugin is offered as an update; Not now is remembered", async ({ page }) => {
    const game = fakeEts2();
    try {
      mkdirSync(path.dirname(dllIn(game)), { recursive: true });
      writeFileSync(dllIn(game), "some other plugin");
      const prompt = page.getByTestId("plugin-prompt");
      // The saved folder holds a different DLL: the prompt offers an update.
      await page.getByRole("link", { name: "Settings", exact: true }).click();
      await page.getByLabel("ETS2 install folder").fill(game);
      await page.getByLabel("ETS2 install folder").blur();
      await expect(page.getByTestId("plugin-badge")).toHaveText("Outdated");
      await page.getByRole("link", { name: "Dashboard", exact: true }).click();
      await expect(prompt).toContainText("Update live telemetry?");
      await prompt.getByRole("button", { name: "Not now" }).click();
      await expect(prompt).toBeHidden();

      await page.reload({ waitUntil: "load" });
      await page.getByRole("link", { name: "Jobs", exact: true }).click();
      await page.getByRole("link", { name: "Dashboard", exact: true }).click();
      await page.waitForTimeout(500);
      await expect(prompt).toBeHidden();
      expect(readFileSync(dllIn(game), "utf8")).toBe("some other plugin");
    } finally {
      rmSync(game, { recursive: true, force: true });
    }
  });
});

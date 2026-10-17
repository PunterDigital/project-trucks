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
});

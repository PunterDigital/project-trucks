import { writeFileSync } from "node:fs";
import path from "node:path";
import type { Page } from "@playwright/test";

import { expect, launchApp, test as base } from "./fixtures";

base.skip(process.platform !== "win32", "WebView2 (Windows) is the only webview with CDP");

// Restarting the app mid-delivery (#4730): the job in progress is saved in the settings table, so
// the second run carries on from the original pickup instead of starting the job again. Two
// launches on one data dir, each with its own replay: the first picks the job up and drives
// 4.8 km of it, the second drives on with the same job and delivers it.

const job = {
  income: 7200,
  deadline: null,
  sourceCity: "Poznań",
  destinationCity: "Málaga",
  sourceCompany: "Tree-ET",
  destinationCompany: "Construcciones Elcano",
  cargoId: "sawdust_panels",
  cargoName: "Sawdust Panels",
  cargoMassKg: 21600,
};

function snapshot(odometerKm: number, speedKmh: number, hauling: boolean) {
  return {
    tick: 0,
    paused: false,
    gameTime: null,
    pluginRevision: 9,
    gameVersion: [1, 55],
    truck: {
      make: "Volvo",
      makeId: "volvo",
      model: "FH",
      speedKmh,
      odometerKm,
      engineEnabled: true,
      trailerAttached: hauling,
      position: [odometerKm * 1000, 0, 0],
      fuelLitres: 300,
      fuelCapacityLitres: 600,
      wearChassis: 0,
      wearTrailer: 0,
    },
    job: hauling ? job : null,
  };
}

type Sample = { t: number; snapshot: ReturnType<typeof snapshot> };

/** Samples 55 virtual seconds apart, 1.2 km each, from `from` km for `steps` steps. */
function drive(samples: Sample[], from: number, steps: number, hauling: boolean) {
  let t = samples.length ? samples[samples.length - 1].t : 0;
  for (let i = 1; i <= steps; i++) {
    t += 55;
    samples.push({ t, snapshot: snapshot(from + i * 1.2, 78, hauling) });
  }
}

// First run: parked with the job at 5000 km, then 5 samples of driving (seen from 5001.2).
const first: Sample[] = [{ t: 0, snapshot: snapshot(5000, 0, true) }];
drive(first, 5000, 5, true); // up to 5006

// Second run: the game still has the same job; drive 6 km more and deliver.
const second: Sample[] = [{ t: 0, snapshot: snapshot(5006, 0, true) }];
drive(second, 5006, 5, true); // up to 5012
second.push({ t: second[second.length - 1].t + 5, snapshot: snapshot(5012, 10, false) });
second.push({ t: second[second.length - 1].t + 5, snapshot: snapshot(5012, 0, false) });

const invoke = (page: Page, cmd: string, args?: unknown) =>
  page.evaluate(
    ([c, a]) =>
      (
        window as unknown as {
          __TAURI_INTERNALS__: { invoke: (cmd: string, args?: unknown) => Promise<unknown> };
        }
      ).__TAURI_INTERNALS__.invoke(c as string, a),
    [cmd, args] as const,
  );

const drivenKm = async (page: Page) =>
  ((await invoke(page, "get_current_job")) as { drivenKm: number } | null)?.drivenKm ?? null;

base("a restart mid-delivery keeps the job's start and its distance", async ({
  dataDir,
}, testInfo) => {
  base.setTimeout(120_000);
  const port = 9300 + testInfo.workerIndex;
  const replayEnv = (name: string, samples: Sample[]) => {
    const file = path.join(dataDir, name);
    writeFileSync(file, JSON.stringify({ samples }));
    return {
      PROJECTTRUCKS_TELEMETRY_REPLAY: file,
      PROJECTTRUCKS_TELEMETRY_REPLAY_INTERVAL_MS: "400",
    };
  };

  const one = await launchApp({ dataDir, port, env: replayEnv("first.json", first) });
  try {
    await one.page.getByRole("link", { name: "Settings", exact: true }).click();
    await one.page.getByLabel("Record drives and deliveries from ETS2 automatically").click();
    await expect(one.page.getByTestId("auto-capture-pill")).toHaveAttribute(
      "data-capture",
      "on",
    );
    // 5001.2 -> 5006: 4.8 km into the job when the app goes away.
    await expect.poll(() => drivenKm(one.page), { timeout: 30_000 }).toBeCloseTo(4.8, 1);
    expect(one.errors).toEqual([]);
  } finally {
    await one.close();
  }

  const two = await launchApp({ dataDir, port, env: replayEnv("second.json", second) });
  try {
    // Capture is still on. The job carries on from the first run's pickup, so the delivered job
    // has all 10.8 km (5001.2 -> 5012), not just the 6 km of this run.
    await two.page.getByRole("link", { name: "Jobs", exact: true }).click();
    const row = two.page.getByRole("row").filter({ hasText: "Málaga" });
    await expect(row).toHaveCount(1, { timeout: 30_000 });
    await expect(row).toContainText("10.8 km");
    await expect(row).not.toContainText("Cancelled");
    await expect.poll(() => invoke(two.page, "get_current_job")).toBeNull();
    expect(two.errors).toEqual([]);
  } finally {
    await two.close();
  }
});

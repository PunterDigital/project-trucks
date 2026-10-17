import { expect, test } from "./fixtures";

test.skip(process.platform !== "win32", "WebView2 (Windows) is the only webview with CDP");

// One replayed shift covering the awkward real-world cases (see src-tauri/src/telemetry/replay.rs):
// a cancelled job, a clean on-time delivery, a ferry crossing (the truck teleports mid-job), an
// alt-tab (the plugin stops sending frames for a while) and a suspiciously fast delivery that
// the sanity checks must flag. `t` is virtual seconds, one sample per 400 ms of real time.

type JobSpec = {
  income: number;
  deadline: string | null;
  sourceCity: string;
  destinationCity: string;
  sourceCompany: string;
  destinationCompany: string;
  cargoId: string;
  cargoName: string;
  cargoMassKg: number;
};

const makeJob = (destinationCity: string, cargoName: string, income: number): JobSpec => ({
  income,
  deadline: null,
  sourceCity: "Calais",
  destinationCity,
  sourceCompany: "Tradeaux",
  destinationCompany: "Kaarfor",
  cargoId: cargoName.toLowerCase(),
  cargoName,
  cargoMassKg: 18000,
});

type Extra = { paused?: boolean; position?: [number, number, number]; gameTime?: string };

function snapshot(odometerKm: number, speedKmh: number, job: JobSpec | null, extra: Extra = {}) {
  return {
    tick: 0,
    paused: extra.paused ?? false,
    gameTime: extra.gameTime ?? null,
    pluginRevision: 10,
    gameVersion: [1, 55],
    truck: {
      make: "Scania",
      makeId: "scania",
      model: "S 730",
      speedKmh,
      odometerKm,
      engineEnabled: true,
      trailerAttached: job !== null,
      position: extra.position ?? [odometerKm * 1000, 0, 0],
      fuelLitres: 300,
      fuelCapacityLitres: 600,
      wearChassis: 0,
      wearTrailer: 0,
    },
    job,
  };
}

type Sample = { t: number; snapshot: ReturnType<typeof snapshot> | null };

class Shift {
  samples: Sample[] = [];
  t = 0;
  odo = 5000;

  /** One sample, `dt` virtual seconds after the previous one. */
  push(dt: number, snap: ReturnType<typeof snapshot> | null) {
    this.t += dt;
    this.samples.push({ t: this.t, snapshot: snap });
  }

  park(job: JobSpec | null = null, dt = 5) {
    this.push(dt, snapshot(this.odo, 0, job));
  }

  /** Drive `km` at a plausible 78 km/h: one sample per 55 s, 1.2 km each. */
  drive(km: number, job: JobSpec | null) {
    const steps = Math.ceil(km / 1.2);
    for (let i = 0; i < steps; i++) {
      this.odo += km / steps;
      this.push(55, snapshot(this.odo, 78, job));
    }
  }

  /** Drive `km` in `seconds` of game time, in samples at most 30 s apart. */
  rush(km: number, seconds: number, job: JobSpec | null) {
    const steps = Math.ceil(seconds / 30);
    for (let i = 0; i < steps; i++) {
      this.odo += km / steps;
      this.push(seconds / steps, snapshot(this.odo, 120, job));
    }
  }
}

const cancelled = makeJob("Brussels", "Bricks", 3000);
const onTime: JobSpec = { ...makeJob("Lyon", "Wine", 8000), deadline: "2026-01-02T12:00:00" };
const ferry = makeJob("Dover", "Timber", 6000);
const altTab = makeJob("Antwerp", "Steel", 7000);
const speedy = makeJob("Amsterdam", "Tulips", 5000);

const shift = new Shift();
shift.park();
shift.drive(6, null); // free-roam

// Cancel: hitch up, drive 6 km, open the pause menu, drop the job.
shift.drive(6, cancelled);
shift.push(5, snapshot(shift.odo, 0, cancelled, { paused: true }));
shift.push(5, snapshot(shift.odo, 20, null));
shift.drive(2, null);

// A clean on-time delivery (game clock before the deadline).
shift.drive(12, onTime);
shift.push(5, snapshot(shift.odo, 20, null, { gameTime: "2026-01-02T10:00:00" }));
shift.drive(2, null);

// Ferry: park at the port, the truck jumps 500 km away, carry on to the destination.
shift.drive(6, ferry);
shift.park(ferry);
shift.push(20, snapshot(shift.odo, 0, ferry, { position: [900000, 0, 12000] }));
shift.drive(6, ferry);
shift.push(5, snapshot(shift.odo, 20, null));
shift.drive(2, null);

// Alt-tab: three missing frames (45 s) in the middle of a delivery.
shift.drive(6, altTab);
for (let i = 0; i < 3; i++) shift.push(15, null);
shift.drive(6, altTab);
shift.push(5, snapshot(shift.odo, 20, null));
shift.drive(2, null);

// Suspiciously fast: 20 km in two minutes of game time is 600 km/h on average.
shift.drive(2, speedy);
shift.rush(20, 120, speedy);
shift.push(5, snapshot(shift.odo, 20, null));
shift.drive(1.2, null);
shift.park(null, 5);
shift.park(null, 400); // crosses the idle limit: the last free-roam session is written

test.describe("auto-capture over a whole shift", () => {
  test.use({ replay: { samples: shift.samples } });
  test.setTimeout(180_000);

  test("cancel, on-time, ferry, alt-tab and a suspicious delivery are each logged correctly", async ({
    page,
  }) => {
    await page.getByRole("link", { name: "Settings", exact: true }).click();
    await page.getByLabel("Record drives and deliveries from ETS2 automatically").click();
    await expect(page.getByTestId("auto-capture-pill")).toHaveText("Auto-capture: on");
    await page.getByRole("link", { name: "Jobs", exact: true }).click();

    const rowFor = (city: string) => page.getByRole("row").filter({ hasText: city });

    // Delivered jobs, one row each; the cancelled job never becomes a job row.
    await expect(rowFor("Amsterdam")).toHaveCount(1, { timeout: 60_000 });
    await expect(rowFor("Lyon")).toHaveCount(1);
    await expect(rowFor("Dover")).toHaveCount(1);
    await expect(rowFor("Antwerp")).toHaveCount(1);
    await expect(rowFor("Brussels")).toHaveCount(0);

    await expect(rowFor("Lyon")).toContainText("On time");
    // Ferry and alt-tab deliveries keep their whole distance: 12 km driven, minus the 1.2 km
    // before the job was first seen on the road.
    await expect(rowFor("Dover")).toContainText("10.8 km");
    await expect(rowFor("Antwerp")).toContainText("10.8 km");

    // Only the 600 km/h delivery is flagged, and the badge says why.
    await expect(page.getByTestId("flagged-badge")).toHaveCount(1);
    await expect(rowFor("Amsterdam").getByTestId("flagged-badge")).toHaveAttribute(
      "title",
      "Average speed over 150 km/h",
    );

    // The cancelled delivery still left its kilometres behind in a session.
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
    const cancelledSessions = sessions.items.filter(
      (s) => s.kind === "JOB" && s.status === "CANCELLED",
    );
    expect(cancelledSessions.map((s) => Math.round(s.distanceKm))).toEqual([5]); // 6 km driven, minus the first 1.2 km before the job was seen
  });
});

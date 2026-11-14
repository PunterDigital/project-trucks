import { expect, test } from "./fixtures";

test.skip(process.platform !== "win32", "WebView2 (Windows) is the only webview with CDP");

// The Dashboard's "Job in progress" card (#4620), fed by a replayed delivery (see
// src-tauri/src/telemetry/replay.rs; one sample per 400 ms): pick up a Calais -> Lyon job with
// 100 km on the route advisor, drive half of it, hold there long enough to read the card, then
// deliver. The card must follow the replay live and go away once the job is done.

const job = (routeKm: number) => ({
  income: 9000,
  deadline: "2026-01-02T15:00:00",
  sourceCity: "Calais",
  destinationCity: "Lyon",
  sourceCompany: "Tradeaux",
  destinationCompany: "Kaarfor",
  cargoId: "wine",
  cargoName: "Wine",
  cargoMassKg: 18000,
  routeDistanceKm: routeKm,
  routeTimeMinutes: routeKm,
});

function snapshot(odometerKm: number, speedKmh: number, routeKm: number | null) {
  return {
    tick: 0,
    paused: false,
    gameTime: "2026-01-02T12:30:00",
    pluginRevision: 9,
    gameVersion: [1, 55],
    truck: {
      make: "Volvo",
      makeId: "volvo",
      model: "FH16",
      speedKmh,
      odometerKm,
      engineEnabled: true,
      trailerAttached: routeKm !== null,
      position: [odometerKm * 1000, 0, 0],
      fuelLitres: 300,
      fuelCapacityLitres: 600,
      wearChassis: 0,
      wearTrailer: 0,
    },
    job: routeKm === null ? null : job(routeKm),
  };
}

const samples: { t: number; snapshot: ReturnType<typeof snapshot> }[] = [];
let t = 0;
const push = (s: ReturnType<typeof snapshot>) => samples.push({ t: t++, snapshot: s });
push(snapshot(1000, 0, null));
push(snapshot(1005, 80, null));
push(snapshot(1010, 80, 100)); // hitched: 0 driven, 100 left
push(snapshot(1035, 80, 75));
// Halfway: 50 driven, 50 left. Held for ~16 s so the assertions below see it.
for (let i = 0; i < 40; i++) push(snapshot(1060 + i * 0.001, 80, 50));
push(snapshot(1100, 80, 10));
push(snapshot(1110, 20, 0));
push(snapshot(1110.5, 20, null)); // delivered: the job is gone
push(snapshot(1111, 0, null));

test.use({ replay: { samples } });

test("the job in progress card follows the delivery and goes away when it is done", async ({
  page,
}) => {
  // No job yet (auto-capture is off): no card.
  await expect(page.getByTestId("telemetry-status")).toBeVisible();
  await expect(page.getByTestId("job-in-progress")).toHaveCount(0);

  await page.getByRole("link", { name: "Settings", exact: true }).click();
  await page.getByLabel("Record drives and deliveries from ETS2 automatically").click();
  await expect(page.getByTestId("auto-capture-pill")).toHaveAttribute("data-capture", "on");
  await page.getByRole("link", { name: "Dashboard", exact: true }).click();

  const card = page.getByTestId("job-in-progress");
  await expect(card).toBeVisible({ timeout: 15_000 });
  await expect(card.getByTestId("job-cargo")).toContainText("Wine");
  await expect(card.getByTestId("job-cargo")).toContainText("18 t");
  await expect(card.getByTestId("job-route")).toHaveText("Calais (Tradeaux) → Lyon (Kaarfor)");
  await expect(card.getByTestId("job-income")).toHaveText("9,000 €");

  // Halfway point (live update, no reload).
  await expect(card.getByTestId("job-progress-label")).toHaveText("50%", { timeout: 15_000 });
  await expect(card.getByTestId("job-progress")).toHaveAttribute("aria-valuenow", "50");
  await expect(card.getByTestId("job-distance-left")).toHaveText("50 km");
  await expect(card.getByTestId("job-time-left")).toHaveText("50m");
  await expect(card.getByTestId("job-deadline")).toHaveText("2h 30m left");
  await expect(card.getByTestId("job-driven")).toHaveText("Driven so far: 50 km");
  await expect(card.getByTestId("job-driving")).toBeVisible();

  // A page opened mid-job gets the card from `get_current_job`, not only from events.
  await page.getByRole("link", { name: "Jobs", exact: true }).click();
  await page.getByRole("link", { name: "Dashboard", exact: true }).click();
  await expect(page.getByTestId("job-in-progress")).toBeVisible();

  // Delivered: the card goes away and the job is in the list.
  await expect(page.getByTestId("job-in-progress")).toHaveCount(0, { timeout: 30_000 });
  await page.getByRole("link", { name: "Jobs", exact: true }).click();
  await expect(page.getByRole("row").filter({ hasText: "Lyon" })).toHaveCount(1);
});

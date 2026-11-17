import { expect, test } from "./fixtures";

test.skip(process.platform !== "win32", "WebView2 (Windows) is the only webview with CDP");

// The game's own gameplay events (#4756), as RenCloud's scs-telemetry passes them on: a toll in
// free roam, a fine and a toll on the job, then the "delivered" event on the results screen with
// the game's revenue and XP. The replay ends with the game still paused on the results screen, so
// the delivery must be recorded from the event alone: before, nothing was written until the
// player left that screen.

type JobSpec = ReturnType<typeof makeJob>;

const makeJob = (destinationCity: string, income: number) => ({
  income,
  deadline: null,
  sourceCity: "Calais",
  destinationCity,
  sourceCompany: "Tradeaux",
  destinationCompany: "Kaarfor",
  cargoId: "apples",
  cargoName: "Apples",
  cargoMassKg: 18000,
});

type GameplayEvent = Record<string, unknown> & { type: string };

function snapshot(
  odometerKm: number,
  speedKmh: number,
  job: JobSpec | null,
  options: { paused?: boolean; events?: GameplayEvent[] } = {},
) {
  return {
    tick: 0,
    paused: options.paused ?? false,
    gameTime: null as string | null,
    pluginRevision: 12,
    gameVersion: [1, 58],
    truck: {
      make: "Volvo",
      makeId: "volvo",
      model: "FH4",
      speedKmh,
      odometerKm,
      engineEnabled: true,
      trailerAttached: job !== null,
      position: [odometerKm * 1000, 0, 0],
      fuelLitres: 300,
      fuelCapacityLitres: 600,
      wearChassis: 0,
      wearTrailer: 0,
    },
    job,
    events: options.events ?? [],
  };
}

const samples: { t: number; snapshot: ReturnType<typeof snapshot> }[] = [];
let t = 0;
let odo = 1000;
const push = (dt: number, s: ReturnType<typeof snapshot>) => {
  t += dt;
  // The game clock runs with the virtual seconds (the average speed is per in-game hour).
  s.gameTime = new Date(Date.UTC(2026, 0, 2, 8, 0, 0) + t * 1000).toISOString().slice(0, 19);
  samples.push({ t, snapshot: s });
};
const drive = (km: number, job: JobSpec | null) => {
  for (let done = 0; done < km; done += 1.2) {
    odo += 1.2;
    push(55, snapshot(odo, 78, job));
  }
};

const rotterdam = makeJob("Rotterdam", 5000);

push(0, snapshot(odo, 0, null));
drive(2.4, null);
push(55, snapshot((odo += 1.2), 60, null, { events: [{ type: "tollgate", amount: 7 }] }));
drive(1.2, null);
drive(4.8, rotterdam);
push(
  55,
  snapshot((odo += 1.2), 78, rotterdam, {
    events: [{ type: "fined", amount: 300, offence: "speeding_camera" }],
  }),
);
drive(2.4, rotterdam);
push(55, snapshot((odo += 1.2), 60, rotterdam, { events: [{ type: "tollgate", amount: 19 }] }));
drive(2.4, rotterdam);
push(30, snapshot(odo, 0, rotterdam)); // parked in the bay
push(5, snapshot(odo, 0, rotterdam, { paused: true })); // the results screen pauses the game
push(
  3,
  snapshot(odo, 0, null, {
    paused: true,
    events: [
      {
        type: "jobDelivered",
        revenue: 4650,
        earnedXp: 320,
        cargoDamage: 0.012,
        distanceKm: 229,
        autoPark: true,
        autoLoad: false,
      },
    ],
  }),
);
// Still on the results screen when the replay ends.
for (let i = 0; i < 5; i++) push(5, snapshot(odo, 0, null, { paused: true }));

test.use({ replay: { samples } });
test.setTimeout(120_000);

test("the game's events record the delivery on the results screen, with its figures and costs", async ({
  page,
}) => {
  await page.getByRole("link", { name: "Settings", exact: true }).click();
  await page.getByLabel("Record drives and deliveries from ETS2 automatically").click();
  await expect(page.getByTestId("auto-capture-pill")).toHaveAttribute("data-capture", "on");
  await page.getByRole("link", { name: "Jobs", exact: true }).click();

  const row = page.getByRole("row").filter({ hasText: "Rotterdam" });
  await expect(row).toHaveCount(1, { timeout: 60_000 });
  await expect(row).not.toContainText("Cancelled");
  await row.click();

  const income = page.locator('[data-testid="job-stat"][data-label="Income"]');
  await expect(income).toContainText("4,650 €");
  await expect(income).toContainText("Offered 5,000 €");

  const game = page.getByTestId("game-result");
  await expect(game).toContainText("320");
  await expect(game).toContainText("1.2%");
  await expect(game).toContainText("229");
  await expect(game.getByText("Auto-park").locator("..")).toContainText("Yes");
  await expect(game.getByText("Auto-load").locator("..")).toContainText("No");

  // The fine and the toll on the job; the free-roam toll belongs to no job.
  const costs = page.getByTestId("job-costs");
  await expect(costs).toContainText("Speeding camera");
  await expect(costs).toContainText("300 €");
  await expect(costs).toContainText("Toll");
  await expect(costs).toContainText("19 €");
  await expect(costs).not.toContainText("7 €");
  await expect(page.getByRole("heading", { name: "Costs" }).locator("..")).toContainText("319 €");

  // Dashboard: income is what the game paid; costs include the free-roam toll.
  await page.getByRole("link", { name: "Dashboard", exact: true }).click();
  const lifetimeIncome = page.locator('[data-testid="stat-card"][data-label="Income"]').first();
  await expect(lifetimeIncome).toContainText("4,650 €");
  await expect(lifetimeIncome).toContainText("326 € in tolls, fines and fares");
});

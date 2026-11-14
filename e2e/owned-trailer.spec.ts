import { expect, test } from "./fixtures";

test.skip(process.platform !== "win32", "WebView2 (Windows) is the only webview with CDP");

// An owned-trailer delivery (#4734). The bundled plugin clears the job only when the trailer is
// unhitched, and an owned trailer stays hitched after the drop-off, so the job fields stay in the
// frame. What changes is the route advisor: it counts down and is cleared when the job is handed
// in (the results screen pauses the game first). The app must log the delivery, ignore the stale
// job while the driver free-roams on, and start the next job normally.

type JobSpec = ReturnType<typeof makeJob>;

const makeJob = (destinationCity: string, cargoName: string, income: number, routeKm: number) => ({
  income,
  deadline: null,
  sourceCity: "Poznań",
  destinationCity,
  sourceCompany: "Tree-ET",
  destinationCompany: "Construcciones Elcano",
  cargoId: cargoName.toLowerCase().replace(/ /g, "_"),
  cargoName,
  cargoMassKg: 21600,
  routeDistanceKm: routeKm,
  routeTimeMinutes: routeKm,
});

function snapshot(odometerKm: number, speedKmh: number, job: JobSpec | null, paused = false) {
  return {
    tick: 0,
    paused,
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
      trailerAttached: true, // owned trailer: hitched the whole time
      position: [odometerKm * 1000, 0, 0],
      fuelLitres: 300,
      fuelCapacityLitres: 600,
      wearChassis: 0,
      wearTrailer: 0,
    },
    job,
  };
}

const samples: { t: number; snapshot: ReturnType<typeof snapshot> }[] = [];
let t = 0;
let odo = 2000;
const push = (dt: number, s: ReturnType<typeof snapshot>) => {
  t += dt;
  samples.push({ t, snapshot: s });
};

const malaga = (routeKm: number) => makeJob("Málaga", "Sawdust Panels", 104104, routeKm);
const sevilla = (routeKm: number) => makeJob("Sevilla", "Cement", 52000, routeKm);

push(0, snapshot(odo, 0, malaga(12)));
// Drive 12 km to the drop-off at a plausible speed, the route counting down.
for (let left = 10.8; left > 0.3; left -= 1.2) {
  odo += 1.2;
  push(55, snapshot(odo, 78, malaga(Math.max(left, 0.2))));
}
push(30, snapshot(odo, 0, malaga(0.2))); // parked in the bay
push(5, snapshot(odo, 0, malaga(0.2), true)); // results screen: paused
push(20, snapshot(odo, 0, malaga(0), true));
push(5, snapshot(odo, 0, malaga(0))); // back in the cab: route gone, job fields still there
push(1, snapshot(odo, 0, malaga(0)));
// Free roam on with the stale Málaga job in the frame.
for (let i = 0; i < 5; i++) {
  odo += 1.2;
  push(55, snapshot(odo, 78, malaga(0)));
}
// The next job, delivered the same way.
push(30, snapshot(odo, 0, sevilla(6)));
for (let left = 4.8; left > 0.3; left -= 1.2) {
  odo += 1.2;
  push(55, snapshot(odo, 78, sevilla(Math.max(left, 0.3))));
}
push(5, snapshot(odo, 0, sevilla(0)));
push(1, snapshot(odo, 0, sevilla(0)));
push(1, snapshot(odo, 0, sevilla(0)));
push(400, snapshot(odo, 0, sevilla(0))); // crosses the idle limit

test.use({ replay: { samples } });
test.setTimeout(120_000);

test("an owned-trailer delivery is logged and the stale job is not picked up again", async ({
  page,
}) => {
  await page.getByRole("link", { name: "Settings", exact: true }).click();
  await page.getByLabel("Record drives and deliveries from ETS2 automatically").click();
  await expect(page.getByTestId("auto-capture-pill")).toHaveAttribute("data-capture", "on");
  await page.getByRole("link", { name: "Jobs", exact: true }).click();

  const rowFor = (city: string) => page.getByRole("row").filter({ hasText: city });
  await expect(rowFor("Sevilla")).toHaveCount(1, { timeout: 60_000 });
  await expect(rowFor("Málaga")).toHaveCount(1);
  await expect(rowFor("Málaga")).not.toContainText("Cancelled");
  await expect(rowFor("Sevilla")).not.toContainText("Cancelled");
  // 10.8 km driven to the drop-off, minus the first 1.2 km before the truck rolled; the 6 km of
  // free roam after it belong to no job.
  await expect(rowFor("Málaga")).toContainText("9.6 km");
  await expect(rowFor("Sevilla")).toContainText("4.8 km");

  const invoke = (cmd: string, args?: unknown) =>
    page.evaluate(
      ([c, a]) =>
        (
          window as unknown as {
            __TAURI_INTERNALS__: { invoke: (cmd: string, args?: unknown) => Promise<unknown> };
          }
        ).__TAURI_INTERNALS__.invoke(c as string, a),
      [cmd, args] as const,
    );
  await expect.poll(() => invoke("get_current_job")).toBeNull();
  const sessions = (await invoke("list_sessions", { filter: null })) as {
    items: { kind: string; status: string; distanceKm: number }[];
  };
  const freeRoam = sessions.items.filter((s) => s.kind === "FREE_ROAM");
  expect(freeRoam.map((s) => Math.round(s.distanceKm))).toContain(6);
  expect(sessions.items.filter((s) => s.kind === "JOB").map((s) => s.status)).toEqual([
    "COMPLETED",
    "COMPLETED",
  ]);
});

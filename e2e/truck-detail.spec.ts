import type { Page } from "@playwright/test";

import { expect, test } from "./fixtures";

test.skip(process.platform !== "win32", "WebView2 (Windows) is the only webview with CDP");

// The truck page (M3.2) reads `get_truck_stats` plus the jobs and sessions lists filtered to the
// truck. Its numbers must agree with the Trucks list (the rollup row) and the Dashboard.

type Invoke = (cmd: string, args: unknown) => Promise<unknown>;

async function invoke(page: Page, cmd: string, args: unknown): Promise<unknown> {
  return page.evaluate(
    ([cmd, args]: readonly [string, unknown]) => {
      const tauri = (window as unknown as { __TAURI_INTERNALS__: { invoke: Invoke } })
        .__TAURI_INTERNALS__;
      return tauri.invoke(cmd, args);
    },
    [cmd, args] as const,
  );
}

type TruckName = { truckMake: string; truckModel: string };
const SCANIA: TruckName = { truckMake: "Scania", truckModel: "S 730" };
const VOLVO: TruckName = { truckMake: "Volvo", truckModel: "FH16" };

function job(truck: TruckName, overrides: Record<string, unknown>) {
  const completed = new Date(Date.now() - 10 * 60_000);
  return {
    sessionId: null,
    ...truck,
    sourceCity: "Calais",
    sourceCompany: "Tradeaux",
    destCity: "Rotterdam",
    destCompany: "Kaarfor",
    cargoId: "apples",
    cargoName: "Apples",
    cargoMassKg: 18000,
    declaredIncome: 5000,
    distanceKm: 100,
    durationSec: 4 * 3600,
    fuelUsedL: null,
    damageDelta: null,
    startedAt: new Date(completed.getTime() - 4 * 3600_000).toISOString(),
    completedAt: completed.toISOString(),
    status: "DELIVERED",
    source: "MANUAL",
    notes: null,
    ...overrides,
  };
}

function session(truck: TruckName, kind: "JOB" | "FREE_ROAM", km: number, minutes: number) {
  const ended = new Date(Date.now() - 10 * 60_000);
  return {
    kind,
    ...truck,
    startOdometer: 1000,
    endOdometer: 1000 + km,
    startedAt: new Date(ended.getTime() - minutes * 60_000).toISOString(),
    endedAt: ended.toISOString(),
    status: "COMPLETED",
  };
}

async function seed(page: Page) {
  // Scania: a recorded 200 km delivery (the job row says 100; the session wins), 30 km of free
  // roam, a manual 412 km late job and a manual 50 km cancelled job: 692 km, 3 jobs.
  const ses = (await invoke(page, "create_session", {
    input: session(SCANIA, "JOB", 200, 150),
  })) as { id: string };
  await invoke(page, "create_job", {
    input: job(SCANIA, { sessionId: ses.id, source: "TELEMETRY" }),
  });
  await invoke(page, "create_session", { input: session(SCANIA, "FREE_ROAM", 30, 45) });
  await invoke(page, "create_job", {
    input: job(SCANIA, { destCity: "Berlin", distanceKm: 412, declaredIncome: 8500, status: "LATE" }),
  });
  await invoke(page, "create_job", {
    input: job(SCANIA, { destCity: "Oslo", distanceKm: 50, declaredIncome: 9999, status: "CANCELLED" }),
  });
  // Volvo: its own rows, which must stay off the Scania's page: 1,999 km, 1 job.
  await invoke(page, "create_session", { input: session(VOLVO, "FREE_ROAM", 1000, 600) });
  await invoke(page, "create_job", {
    input: job(VOLVO, {
      sourceCity: "Warsaw",
      destCity: "Gdansk",
      distanceKm: 999,
      declaredIncome: 1,
    }),
  });
}

function stat(page: Page, label: string) {
  return page.locator(`[data-testid="stat-card"][data-label="${label}"]`);
}

test("a truck's page shows its own stats, jobs and sessions, matching the list and dashboard", async ({
  page,
}) => {
  await seed(page);

  // The Trucks list is the rollup row: 692 km over 3 trips for the Scania.
  await page.getByRole("link", { name: "Trucks", exact: true }).click();
  const row = page.getByRole("row").filter({ hasText: "Scania" });
  await expect(row.getByRole("cell").nth(2)).toHaveText("3");
  await expect(row.getByRole("cell").nth(3)).toContainText("692 km");
  await expect(row.getByRole("cell").nth(3)).toContainText("662 km delivery");
  await row.click();

  // Header and stat cards agree with the list.
  await expect(page.getByRole("heading", { name: "Scania S 730" })).toBeVisible();
  await expect(page.getByText(/^First seen/)).toBeVisible();
  const total = stat(page, "Total distance");
  await expect(total.getByTestId("stat-value")).toHaveText("692 km");
  await expect(total.getByTestId("stat-sub")).toHaveText("662 km delivery · 30 km free roam");
  await expect(stat(page, "Jobs").getByTestId("stat-value")).toHaveText("3");
  // Mean of the job rows' own distances: (100 + 412 + 50) / 3.
  await expect(stat(page, "Average job distance").getByTestId("stat-value")).toHaveText("187.3 km");
  await expect(stat(page, "On time").getByTestId("stat-value")).toHaveText("50%");
  await expect(stat(page, "On time").getByTestId("stat-sub")).toHaveText("1 on time · 1 late");

  // The chart draws today's stacked bars.
  const chart = page.getByTestId("distance-chart");
  await expect(chart.getByRole("application")).toBeVisible();
  await expect(chart.locator(".recharts-bar")).toHaveCount(2);
  await expect(chart.locator(".recharts-bar-rectangle path")).toHaveCount(2);
  await expect(chart.locator(".recharts-legend-wrapper")).toContainText("Free roam");

  // Jobs tab (open by default): the Scania's three jobs, not the Volvo's.
  await expect(page.getByRole("tab", { name: "Jobs" })).toHaveAttribute("aria-selected", "true");
  const jobs = page.getByTestId("truck-jobs");
  const jobRows = jobs.getByRole("row").filter({ hasText: "Calais" });
  await expect(jobRows).toHaveCount(3);
  await expect(jobRows.filter({ hasText: "Berlin" })).toContainText("Late");
  await expect(jobRows.filter({ hasText: "Oslo" })).toContainText("Cancelled");
  await expect(jobRows.filter({ hasText: "Rotterdam" })).toContainText("On time");
  await expect(jobs.getByRole("row").filter({ hasText: "Gdansk" })).toHaveCount(0);
  await expect(jobs.getByText("Showing 1–3 of 3")).toBeVisible();

  // Sessions tab: the two recorded drives with their kind, newest first (same minute here).
  await page.getByRole("tab", { name: "All sessions" }).click();
  const sessions = page.getByTestId("truck-sessions");
  const sessionRows = sessions.getByRole("row").filter({ has: page.getByTestId("session-kind") });
  await expect(sessionRows).toHaveCount(2);
  const delivery = sessionRows.filter({ hasText: "Delivery" });
  await expect(delivery).toContainText("200 km");
  await expect(delivery).toContainText("2h 30m");
  await expect(delivery).toContainText("Completed");
  const freeRoam = sessionRows.filter({ hasText: "Free roam" });
  await expect(freeRoam).toContainText("30 km");
  await expect(freeRoam).toContainText("45m");
  await expect(sessions.getByText("Showing 1–2 of 2")).toBeVisible();

  // Back to the Jobs tab: a row opens the job page.
  await page.getByRole("tab", { name: "Jobs" }).click();
  await jobRows.filter({ hasText: "Berlin" }).click();
  await expect(page.getByRole("heading", { name: "Calais → Berlin" })).toBeVisible();

  // The Volvo's page has only its own rows, and no recorded delivery.
  await page.getByRole("link", { name: "Trucks", exact: true }).click();
  await page.getByRole("row").filter({ hasText: "Volvo" }).click();
  await expect(page.getByRole("heading", { name: "Volvo FH16" })).toBeVisible();
  await expect(stat(page, "Total distance").getByTestId("stat-value")).toHaveText("1,999 km");
  await expect(stat(page, "Jobs").getByTestId("stat-value")).toHaveText("1");
  await expect(stat(page, "Average job distance").getByTestId("stat-value")).toHaveText("999 km");
  await expect(stat(page, "On time").getByTestId("stat-value")).toHaveText("100%");
  await expect(page.getByTestId("truck-jobs").getByRole("row").filter({ hasText: "Gdansk" })).toHaveCount(
    1,
  );
  await expect(page.getByTestId("truck-jobs").getByRole("row").filter({ hasText: "Calais" })).toHaveCount(
    0,
  );

  // The Dashboard sums both trucks: 692 + 1,999.
  await page.getByRole("link", { name: "Dashboard", exact: true }).click();
  await expect(stat(page, "Total km driven").getByTestId("stat-value")).toHaveText("2,691 km");

  // Miles flow through the truck page too.
  await page.getByRole("link", { name: "Settings", exact: true }).click();
  await page.getByRole("button", { name: "Miles" }).click();
  await page.getByRole("link", { name: "Trucks", exact: true }).click();
  await page.getByRole("row").filter({ hasText: "Scania" }).click();
  await expect(stat(page, "Total distance").getByTestId("stat-value")).toHaveText("430 mi");
});

test("a truck with only free-roam driving shows its km and empty job list", async ({ page }) => {
  await invoke(page, "create_session", { input: session(SCANIA, "FREE_ROAM", 75, 60) });
  await page.getByRole("link", { name: "Trucks", exact: true }).click();
  await page.getByRole("row").filter({ hasText: "Scania" }).click();
  await expect(page.getByRole("heading", { name: "Scania S 730" })).toBeVisible();
  await expect(stat(page, "Total distance").getByTestId("stat-value")).toHaveText("75 km");
  await expect(stat(page, "Total distance").getByTestId("stat-sub")).toHaveText(
    "0 km delivery · 75 km free roam",
  );
  await expect(stat(page, "Jobs").getByTestId("stat-value")).toHaveText("0");
  await expect(stat(page, "Average job distance").getByTestId("stat-value")).toHaveText("–");
  await expect(stat(page, "On time").getByTestId("stat-value")).toHaveText("–");
  await expect(page.getByTestId("truck-jobs")).toContainText("No jobs on this truck yet");
  await page.getByRole("tab", { name: "All sessions" }).click();
  await expect(page.getByTestId("truck-sessions").getByTestId("session-kind")).toHaveText("Free roam");
});

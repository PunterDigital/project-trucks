import type { Page } from "@playwright/test";

import { expect, test } from "./fixtures";

test.skip(process.platform !== "win32", "WebView2 (Windows) is the only webview with CDP");

// M3.4: a fresh data dir shows an empty state with a next action on every page, lists load
// behind skeletons (never "Loading…" text), and a filter with no matches offers to clear itself.
// The fixture fails the test on any console error.

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

function job(overrides: Record<string, unknown>) {
  const completed = new Date(Date.now() - 10 * 60_000);
  return {
    sessionId: null,
    truckMake: "Scania",
    truckModel: "S 730",
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

/** Every page settles with no skeleton left and no "Loading…" text anywhere. */
async function expectSettled(page: Page) {
  await expect(page.getByTestId("skeleton")).toHaveCount(0);
  await expect(page.getByText(/Loading…|Loading\.\.\./)).toHaveCount(0);
}

test("a fresh install shows an empty state with a next action on every page", async ({
  page,
}) => {
  // Dashboard
  await expect(page.getByRole("heading", { name: "Nothing tracked yet" })).toBeVisible();
  await expectSettled(page);
  const dashboardEmpty = page.getByTestId("empty-state");
  await expect(dashboardEmpty.getByRole("link", { name: "Log a job" })).toBeVisible();
  await expect(dashboardEmpty.getByRole("link", { name: "Turn on auto-capture" })).toBeVisible();

  // Jobs
  await page.getByRole("link", { name: "Jobs", exact: true }).click();
  await expect(page.getByRole("heading", { name: "No jobs yet" })).toBeVisible();
  await expect(page.getByText("0 deliveries logged")).toBeVisible();
  await expectSettled(page);
  const jobsEmpty = page.getByTestId("empty-state");
  await expect(jobsEmpty.getByRole("link", { name: "Log your first job manually" })).toBeVisible();
  await expect(jobsEmpty.getByRole("link", { name: "Turn on auto-capture" })).toBeVisible();

  // Trucks
  await page.getByRole("link", { name: "Trucks", exact: true }).click();
  await expect(page.getByRole("heading", { name: "No trucks yet" })).toBeVisible();
  await expect(page.getByText("0 trucks driven")).toBeVisible();
  await expectSettled(page);
  const trucksEmpty = page.getByTestId("empty-state");
  await expect(trucksEmpty.getByRole("link", { name: "Log a job" })).toBeVisible();

  // The auto-capture action lands on Settings with the checkbox in view.
  await trucksEmpty.getByRole("link", { name: "Turn on auto-capture" }).click();
  await expect(page.getByRole("heading", { name: "Settings" })).toBeVisible();
  await expect(page.getByRole("checkbox", { name: /Record drives/ })).toBeVisible();
  await expectSettled(page);
});

test("an unknown job or truck id shows a not-found state with a way back", async ({ page }) => {
  await page.evaluate(() => {
    window.location.hash = "#/jobs/does-not-exist";
  });
  await expect(page.getByRole("heading", { name: "Job not found" })).toBeVisible();
  await expectSettled(page);
  await page.getByTestId("empty-state").getByRole("link", { name: "Back to jobs" }).click();
  await expect(page.getByRole("heading", { name: "Jobs", exact: true })).toBeVisible();

  await page.evaluate(() => {
    window.location.hash = "#/trucks/does-not-exist";
  });
  await expect(page.getByRole("heading", { name: "Truck not found" })).toBeVisible();
  await expectSettled(page);
  await page.getByTestId("empty-state").getByRole("link", { name: "Back to trucks" }).click();
  await expect(page.getByRole("heading", { name: "Trucks", exact: true })).toBeVisible();
});

test("a filter with no matches offers to clear itself", async ({ page }) => {
  await invoke(page, "create_job", { input: job({ destCity: "Rotterdam" }) });
  await invoke(page, "create_job", { input: job({ destCity: "Berlin", status: "LATE" }) });

  await page.getByRole("link", { name: "Jobs", exact: true }).click();
  await expect(page.getByText("2 deliveries logged")).toBeVisible();
  await expect(page.getByRole("row").filter({ hasText: "Calais" })).toHaveCount(2);

  await page.getByPlaceholder("Search city, company, cargo…").fill("Zanzibar");
  await expect(page.getByRole("heading", { name: "No jobs match these filters" })).toBeVisible();
  await expectSettled(page);
  // The first-run state is not shown for a filtered-out list.
  await expect(page.getByRole("heading", { name: "No jobs yet" })).toHaveCount(0);

  await page.getByTestId("empty-state").getByRole("button", { name: "Clear filters" }).click();
  await expect(page.getByRole("row").filter({ hasText: "Calais" })).toHaveCount(2);
  await expect(page.getByTestId("empty-state")).toHaveCount(0);
});

test("a truck with only recorded free roam shows compact empty states in its tabs", async ({
  page,
}) => {
  const ended = new Date(Date.now() - 10 * 60_000);
  await invoke(page, "create_session", {
    input: {
      kind: "FREE_ROAM",
      truckMake: "Volvo",
      truckModel: "FH16",
      startOdometer: 100,
      endOdometer: 130,
      startedAt: new Date(ended.getTime() - 3600_000).toISOString(),
      endedAt: ended.toISOString(),
      status: "COMPLETED",
    },
  });

  await page.getByRole("link", { name: "Trucks", exact: true }).click();
  await page.getByRole("row").filter({ hasText: "Volvo" }).click();
  await expect(page.getByRole("heading", { name: "Volvo FH16" })).toBeVisible();
  await expectSettled(page);

  const jobs = page.getByTestId("truck-jobs");
  await expect(jobs.getByTestId("empty-state")).toContainText("No jobs on this truck yet");

  await page.getByRole("tab", { name: "All sessions" }).click();
  const sessions = page.getByTestId("truck-sessions");
  await expect(sessions.getByRole("row").filter({ hasText: "Free roam" })).toHaveCount(1);
  await expect(sessions.getByTestId("empty-state")).toHaveCount(0);
});

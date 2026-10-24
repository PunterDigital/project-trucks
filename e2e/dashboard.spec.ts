import type { Page } from "@playwright/test";

import { expect, test } from "./fixtures";

test.skip(process.platform !== "win32", "WebView2 (Windows) is the only webview with CDP");

// The dashboard (M3.1) reads everything through `get_dashboard_stats`; the numbers follow the
// truck-rollup rules: sessions carry the km of recorded drives, a manual job carries its own.

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

function session(kind: "JOB" | "FREE_ROAM", km: number) {
  const ended = new Date(Date.now() - 10 * 60_000);
  return {
    kind,
    truckMake: "Scania",
    truckModel: "S 730",
    startOdometer: 1000,
    endOdometer: 1000 + km,
    startedAt: new Date(ended.getTime() - 3600_000).toISOString(),
    endedAt: ended.toISOString(),
    status: "COMPLETED",
  };
}

async function seed(page: Page) {
  // A recorded delivery: 200 km on the session, the job row says 100 (the session wins).
  const ses = (await invoke(page, "create_session", { input: session("JOB", 200) })) as {
    id: string;
  };
  await invoke(page, "create_job", {
    input: job({
      sessionId: ses.id,
      source: "TELEMETRY",
      destCity: "Rotterdam",
      declaredIncome: 5000,
    }),
  });
  // 30 km of free roam.
  await invoke(page, "create_session", { input: session("FREE_ROAM", 30) });
  // A manual late delivery: 412 km of its own.
  await invoke(page, "create_job", {
    input: job({ destCity: "Berlin", distanceKm: 412, declaredIncome: 8500, status: "LATE" }),
  });
  // A cancelled manual job: km count like the rollup, but no delivery or income.
  await invoke(page, "create_job", {
    input: job({ destCity: "Oslo", distanceKm: 50, declaredIncome: 9999, status: "CANCELLED" }),
  });
}

function stat(page: Page, label: string) {
  return page.locator(`[data-testid="stat-card"][data-label="${label}"]`);
}

test("a fresh install shows the empty dashboard", async ({ page }) => {
  await expect(page.getByRole("heading", { name: "Dashboard", exact: true })).toBeVisible();
  await expect(page.getByTestId("telemetry-status")).toHaveText("ETS2 not connected");
  await expect(page.getByRole("heading", { name: "Nothing tracked yet" })).toBeVisible();
  await page.getByRole("link", { name: "Log a job" }).click();
  await expect(page.getByRole("heading", { name: "Log a job" })).toBeVisible();
});

test("stat cards, charts and recent jobs follow the seeded data", async ({ page }) => {
  await seed(page);
  // The seed went in through commands, not the capture task, so reload the page.
  await page.getByRole("link", { name: "Jobs", exact: true }).click();
  await page.getByRole("link", { name: "Dashboard", exact: true }).click();

  // Lifetime: 200 (session) + 30 (free roam) + 412 + 50 (manual) = 692 km.
  const hero = stat(page, "Total km driven");
  await expect(hero.getByTestId("stat-value")).toHaveText("692 km");
  await expect(hero.getByTestId("stat-sub")).toHaveText("662 km delivery · 30 km free roam");

  const lifetime = page.getByRole("region", { name: "Lifetime" });
  await expect(lifetime.locator(stat(page, "Deliveries")).getByTestId("stat-value")).toHaveText(
    "2",
  );
  await expect(lifetime.locator(stat(page, "Income")).getByTestId("stat-value")).toHaveText(
    "13,500 €",
  );
  await expect(lifetime.locator(stat(page, "On time")).getByTestId("stat-value")).toHaveText(
    "50%",
  );

  // This month holds the same rows; last month is empty, so every delta is "+".
  const month = page.getByRole("region", { name: "This month" });
  const distance = month.locator(stat(page, "Distance"));
  await expect(distance.getByTestId("stat-value")).toHaveText("692 km");
  await expect(distance.getByTestId("stat-delta")).toContainText("+692 km");
  await expect(distance.getByTestId("stat-delta")).toContainText("vs last month");
  await expect(month.locator(stat(page, "Deliveries")).getByTestId("stat-delta")).toContainText(
    "+2",
  );
  await expect(month.locator(stat(page, "Income")).getByTestId("stat-delta")).toContainText(
    "+13,500 €",
  );
  // No last-month on-time rate to compare with: no delta line.
  await expect(month.locator(stat(page, "On time")).getByTestId("stat-value")).toHaveText("50%");
  await expect(month.locator(stat(page, "On time")).getByTestId("stat-delta")).toHaveCount(0);

  // Charts render as SVG: two stacked bar series (with today's bars drawn), day ticks, a legend
  // and a two-slice donut.
  const chart = page.getByTestId("distance-chart");
  await expect(chart.getByRole("application")).toBeVisible();
  await expect(chart.locator(".recharts-bar")).toHaveCount(2);
  await expect(chart.locator(".recharts-bar-rectangle path")).toHaveCount(2);
  // Tick labels are laid out once the axis has measured its text, which needs the chart on screen.
  await chart.scrollIntoViewIfNeeded();
  await expect
    .poll(() =>
      chart
        .locator(".recharts-cartesian-axis-tick-value", { hasText: /^\d{1,2} [A-Z][a-z]{2,4}$/ })
        .count(),
    )
    .toBeGreaterThan(3);
  await expect(chart.locator(".recharts-legend-wrapper")).toContainText("Delivery");
  await expect(chart.locator(".recharts-legend-wrapper")).toContainText("Free roam");
  const donut = page.getByTestId("on-time-donut");
  await expect(donut.locator(".recharts-pie-sector")).toHaveCount(2);
  await expect(donut.getByTestId("on-time-pct")).toHaveText("50%");

  // Recent jobs: all three, newest first (same timestamp here, so just the set), with status.
  const rows = page.getByTestId("recent-jobs").getByRole("row").filter({ hasText: "Calais" });
  await expect(rows).toHaveCount(3);
  await expect(rows.filter({ hasText: "Berlin" })).toContainText("Late");
  await expect(rows.filter({ hasText: "Oslo" })).toContainText("Cancelled");
  await expect(rows.filter({ hasText: "Rotterdam" })).toContainText("On time");

  // Miles setting flows through the cards.
  await page.getByRole("link", { name: "Settings", exact: true }).click();
  await page.getByRole("button", { name: "Miles" }).click();
  await page.getByRole("link", { name: "Dashboard", exact: true }).click();
  await expect(stat(page, "Total km driven").getByTestId("stat-value")).toHaveText("430 mi");

  // A recent job opens the job page.
  await page
    .getByTestId("recent-jobs")
    .getByRole("row")
    .filter({ hasText: "Berlin" })
    .click();
  await expect(page.getByRole("heading", { name: "Calais → Berlin" })).toBeVisible();
});

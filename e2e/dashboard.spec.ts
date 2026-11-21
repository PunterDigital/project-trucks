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

/** WCAG contrast ratio of two CSS colours, painted through a 1×1 canvas (computed values may be oklch()). */
async function contrast(page: Page, a: string, b: string): Promise<number> {
  return page.evaluate(
    ([a, b]) => {
      const ctx = document.createElement("canvas").getContext("2d", { willReadFrequently: true })!;
      const luminance = (css: string) => {
        ctx.clearRect(0, 0, 1, 1);
        ctx.fillStyle = css;
        ctx.fillRect(0, 0, 1, 1);
        const [r, g, b] = Array.from(ctx.getImageData(0, 0, 1, 1).data.slice(0, 3)).map((v) => {
          const c = v / 255;
          return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
        });
        return 0.2126 * r + 0.7152 * g + 0.0722 * b;
      };
      const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
      return (hi + 0.05) / (lo + 0.05);
    },
    [a, b] as const,
  );
}

/** A theme token's computed colour (a probe element resolves the var). */
async function token(page: Page, name: string): Promise<string> {
  return page.evaluate((name) => {
    const probe = document.createElement("div");
    probe.style.color = `var(${name})`;
    document.body.append(probe);
    const color = getComputedStyle(probe).color;
    probe.remove();
    return color;
  }, name);
}

// #5243: free roam (and the donut's "late") were the faintest grey, unreadable on the light card,
// and recharts painted the tooltip/legend text in the same grey.
test("chart series, tooltip and legend stay readable in light and dark", async ({ page }) => {
  await seed(page);
  await page.getByRole("link", { name: "Jobs", exact: true }).click();
  await page.getByRole("link", { name: "Dashboard", exact: true }).click();

  const chart = page.getByTestId("distance-chart");
  await chart.scrollIntoViewIfNeeded();
  const bars = chart.locator(".recharts-bar-rectangle path");
  await expect(bars).toHaveCount(2);

  for (const scheme of ["light", "dark"] as const) {
    // The theme setting defaults to System, which follows this.
    await page.emulateMedia({ colorScheme: scheme });
    if (scheme === "dark") await expect(page.locator("html")).toHaveClass(/dark/);
    else await expect(page.locator("html")).not.toHaveClass(/dark/);
    const card = await token(page, "--card");

    // Both series stand out from the card (3:1, WCAG's bar for graphics), and from each other.
    const fills = await bars.evaluateAll((paths) => paths.map((p) => getComputedStyle(p).fill));
    for (const fill of fills) expect(await contrast(page, fill, card)).toBeGreaterThanOrEqual(3);
    expect(await contrast(page, fills[0], fills[1])).toBeGreaterThan(1.5);
    const donutFills = await page
      .getByTestId("on-time-donut")
      .locator(".recharts-pie-sector path")
      .evaluateAll((paths) => paths.map((p) => getComputedStyle(p).fill));
    for (const fill of donutFills) {
      expect(await contrast(page, fill, card)).toBeGreaterThanOrEqual(3);
    }

    // Legend labels are in the text colour, not the series colour (recharts colours its own
    // wrapper span; the formatter's inner span is what's painted).
    const foreground = await token(page, "--foreground");
    for (const name of ["Delivery", "Free roam"]) {
      const label = chart.locator(".recharts-legend-item-text > span", { hasText: name });
      await expect(label).toHaveCSS("color", foreground);
    }

    // Hovering today's bar shows both values in the popover's text colour, one line on the y axis.
    await bars.last().hover();
    const tooltip = chart.getByTestId("chart-tooltip");
    await expect(tooltip).toContainText("Free roam: 30 km");
    await expect(tooltip).toContainText("Delivery: ");
    const popoverText = await token(page, "--popover-foreground");
    await expect(tooltip.getByText("Free roam: 30 km")).toHaveCSS("color", popoverText);
    await page.mouse.move(0, 0);
  }

  // Y-axis labels ("… km") are a single line each: recharts wraps into extra <tspan>s.
  const ticks = chart.locator(".recharts-cartesian-axis-tick-value", { hasText: /km$/ });
  await expect.poll(() => ticks.count()).toBeGreaterThan(1);
  for (const tick of await ticks.all()) await expect(tick.locator("tspan")).toHaveCount(1);
});

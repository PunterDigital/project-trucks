import type { Page } from "@playwright/test";

import { expect, test as base } from "./fixtures";
import { link, StubHub } from "./stub-hub";

base.skip(process.platform !== "win32", "WebView2 (Windows) is the only webview with CDP");

// ProjectTrucks Hub sync (#4634) and live status (#4635), against a stub Hub served from Node.
// The app only talks to a Hub in a test profile when PROJECTTRUCKS_HUB_URL points at one, and
// never opens a browser there. The stub follows the Hub's docs/api.md.

const test = base.extend<{ hub: StubHub }>({
  hub: async ({}, use) => {
    const hub = new StubHub();
    await hub.start();
    await use(hub);
    await hub.stop();
  },
  appEnv: async ({ hub }, use) => {
    await use({ PROJECTTRUCKS_HUB_URL: hub.url });
  },
});

type Invoke = (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;

/** A job as the telemetry runner records it (test profiles may seed those), or one logged by hand. */
async function createJob(
  page: Page,
  destCity: string,
  notes: string | null = null,
  source: "TELEMETRY" | "MANUAL" = "TELEMETRY",
): Promise<string> {
  return page.evaluate(
    async ({ destCity, notes, source }) => {
      const tauri = (window as unknown as { __TAURI_INTERNALS__: { invoke: Invoke } })
        .__TAURI_INTERNALS__;
      const completed = new Date(Date.UTC(2026, 10, 14, 12));
      const job = (await tauri.invoke("create_job", {
        input: {
          sessionId: null,
          truckMake: "Volvo",
          truckModel: "FH16",
          sourceCity: "Calais",
          sourceCompany: "Tradeaux",
          destCity,
          destCompany: "Euroacres",
          cargoId: "apples",
          cargoName: "Apples",
          cargoMassKg: 18000,
          declaredIncome: 12000,
          distanceKm: 290,
          durationSec: 14400,
          fuelUsedL: null,
          damageDelta: null,
          startedAt: new Date(completed.getTime() - 14_400_000).toISOString(),
          completedAt: completed.toISOString(),
          status: "DELIVERED",
          source,
          notes,
        },
      })) as { id: string };
      return job.id;
    },
    { destCity, notes, source },
  );
}

async function invoke(page: Page, cmd: string, args: Record<string, unknown>) {
  await page.evaluate(
    ({ cmd, args }) =>
      (window as unknown as { __TAURI_INTERNALS__: { invoke: Invoke } }).__TAURI_INTERNALS__.invoke(
        cmd,
        args,
      ),
    { cmd, args },
  );
}


test("linking, then new jobs, edits and deletes sync; old jobs wait for Sync history", async ({
  page,
  hub,
}) => {
  const old = await createJob(page, "Before linking");
  // Logged by hand: never synced, not even by Sync history (#4751).
  const oldManual = await createJob(page, "Typed in before", null, "MANUAL");
  await link(page, hub);
  const section = page.getByTestId("hub-settings");

  // Nothing from before linking was sent.
  await expect(section.getByTestId("hub-sync-summary")).toContainText("0 jobs on the Hub");
  expect(hub.jobs.size).toBe(0);
  await expect(section.getByRole("button", { name: "Sync history (1 older job)" })).toBeVisible();

  // A new job syncs by itself, without its notes.
  const fresh = await createJob(page, "Paris", "my private note");
  await expect.poll(() => hub.jobs.has(fresh), { timeout: 15_000 }).toBe(true);
  const sent = hub.jobs.get(fresh)!;
  expect(sent.dest_city).toBe("Paris");
  expect(sent.status).toBe("DELIVERED");
  expect(sent.source).toBe("TELEMETRY");
  expect(sent).not.toHaveProperty("notes");
  await expect(section.getByTestId("hub-sync-summary")).toContainText("1 job on the Hub");

  // A job logged by hand after linking stays on this PC, edits included.
  const manual = await createJob(page, "Typed in", null, "MANUAL");
  await invoke(page, "update_job", { id: manual, patch: { flagged: true, flagReason: "odd" } });

  // An edit follows it.
  await invoke(page, "update_job", { id: fresh, patch: { flagged: true, flagReason: "odd" } });
  await expect.poll(() => hub.jobs.get(fresh)?.flagged, { timeout: 15_000 }).toBe(true);

  // Older jobs only on request.
  await section.getByRole("button", { name: "Sync history (1 older job)" }).click();
  await expect.poll(() => hub.jobs.has(old), { timeout: 15_000 }).toBe(true);
  await expect(section.getByTestId("hub-sync-summary")).toContainText("2 jobs on the Hub");
  await expect(section.getByRole("button", { name: /Sync history/ })).toHaveCount(0);
  expect(hub.jobs.has(manual)).toBe(false);
  expect(hub.jobs.has(oldManual)).toBe(false);
  expect(hub.jobs.size).toBe(2);

  // Deleting a synced job deletes it on the Hub.
  await invoke(page, "delete_job", { id: fresh });
  await expect.poll(() => hub.deletes, { timeout: 15_000 }).toContain(fresh);
  await expect(section.getByTestId("hub-sync-summary")).toContainText("1 job on the Hub");
});

test("unlinked on the website: the app notices and stops syncing", async ({ page, hub }) => {
  await link(page, hub);
  hub.revoked = true;
  await createJob(page, "Lyon");

  const section = page.getByTestId("hub-settings");
  await expect(section.getByTestId("hub-error")).toContainText("unlinked on the website", {
    timeout: 15_000,
  });
  await expect(section.getByRole("button", { name: "Link account" })).toBeVisible();
  expect(hub.jobs.size).toBe(0);
});

test("unlinking in the app revokes its token on the Hub", async ({ page, hub }) => {
  await link(page, hub);
  const section = page.getByTestId("hub-settings");
  await section.getByRole("button", { name: "Unlink" }).click();
  await page.getByRole("alertdialog").getByRole("button", { name: "Unlink" }).click();
  await expect(section.getByRole("button", { name: "Link account" })).toBeVisible();
  expect(hub.tokenRevokedByApp).toBe(true);

  // Nothing syncs any more.
  await createJob(page, "Milano");
  await page.waitForTimeout(4_000);
  expect(hub.jobs.size).toBe(0);
});

test("cancelling a link stops polling", async ({ page, hub }) => {
  await page.getByRole("link", { name: "Settings", exact: true }).click();
  const section = page.getByTestId("hub-settings");
  await section.getByRole("button", { name: "Link account" }).click();
  await expect(section.getByTestId("hub-code")).toBeVisible();
  await section.getByRole("button", { name: "Cancel" }).click();
  await expect(section.getByRole("button", { name: "Link account" })).toBeVisible();
  hub.approved = true;
  // A check that nothing happens: give a stray poll time to land (nothing positive to wait on).
  await page.waitForTimeout(3_000);
  await expect(section.getByTestId("hub-linked")).toHaveCount(0);
});

// Live status (#4635): a replayed delivery, Calais → Lyon. The Hub sees the job in progress and
// is told when the drive ends (auto-capture switched off).
function snapshot(odometerKm: number, routeKm: number | null) {
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
      speedKmh: 80,
      odometerKm,
      engineEnabled: true,
      trailerAttached: routeKm !== null,
      position: [odometerKm * 1000, 0, -2500],
      heading: 0.25,
      fuelLitres: 300,
      fuelCapacityLitres: 600,
      wearChassis: 0,
      wearTrailer: 0,
    },
    job:
      routeKm === null
        ? null
        : {
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
          },
  };
}

const samples: { t: number; snapshot: ReturnType<typeof snapshot> }[] = [];
samples.push({ t: 0, snapshot: snapshot(1000, null) });
samples.push({ t: 1, snapshot: snapshot(1010, 100) });
// Hold the job long enough for the link and a couple of updates.
for (let i = 0; i < 150; i++) samples.push({ t: 2 + i, snapshot: snapshot(1020 + i * 0.01, 90) });

test.describe("live status", () => {
  test.use({ replay: { samples } });

  test("the job in progress reaches the Hub, and leaving clears it", async ({ page, hub }) => {
    await link(page, hub);
    await page.getByLabel("Record drives and deliveries from ETS2 automatically").click();

    await expect
      .poll(
        () =>
          hub.statuses.some(
            (s) =>
              s !== "deleted" &&
              (s.job as Record<string, unknown> | null)?.destination_city === "Lyon",
          ),
        { timeout: 30_000 },
      )
      .toBe(true);
    const status = hub.statuses.find((s) => s !== "deleted" && s.job) as Record<string, unknown>;
    expect(status.truck).toEqual({ make: "Volvo", model: "FH16" });
    expect((status.job as Record<string, unknown>).cargo_name).toBe("Wine");
    // The map position rides along (#4686): world x / z and the heading in degrees.
    const position = status.position as { x: number; z: number; heading_deg: number };
    expect(position.x).toBeGreaterThanOrEqual(1_010_000);
    expect(position.z).toBe(-2500);
    expect(position.heading_deg).toBe(90);

    // Switching auto-capture off ends the drive: the Hub is told.
    await page.getByLabel("Record drives and deliveries from ETS2 automatically").click();
    await expect.poll(() => hub.statuses.at(-1), { timeout: 15_000 }).toBe("deleted");
  });
});

test("the app shows what reached the Hub: header pill, company, job badges and Hide on Hub (#4654)", async ({
  page,
  hub,
}) => {
  hub.company = { name: "Chart Haulage", slug: "chart", tag: "CHT" };
  // No pill before linking.
  await expect(page.getByTestId("auto-capture-pill")).toBeVisible();
  await expect(page.getByTestId("hub-pill")).toHaveCount(0);

  const old = await createJob(page, "Before linking");
  await link(page, hub);
  const section = page.getByTestId("hub-settings");
  await expect(section.getByTestId("hub-company")).toHaveText("Company: Chart Haulage [CHT]", {
    timeout: 15_000,
  });
  await expect(page.getByTestId("hub-pill-label")).toHaveText("@erin");

  const fresh = await createJob(page, "Paris");
  const manual = await createJob(page, "Typed in", null, "MANUAL");
  await expect.poll(() => hub.jobs.has(fresh), { timeout: 15_000 }).toBe(true);

  // Jobs list: only the synced job carries a Hub badge.
  await page.getByRole("link", { name: "Jobs", exact: true }).click();
  const row = (city: string) => page.getByRole("row").filter({ hasText: city });
  await expect(row("Paris").getByTestId("hub-sync-badge")).toHaveAttribute("data-state", "synced", {
    timeout: 15_000,
  });
  await expect(row("Typed in").getByTestId("hub-sync-badge")).toHaveCount(0);
  await expect(row("Before linking").getByTestId("hub-sync-badge")).toHaveCount(0);

  // Job page: state, link and the Hide on Hub switch, which reaches the Hub.
  await row("Paris").click();
  const panel = page.getByTestId("job-hub-panel");
  await expect(panel.getByTestId("job-hub-state")).toHaveText("On the Hub.");
  await expect(panel.getByRole("button", { name: "Open on the Hub" })).toBeVisible();
  const hide = panel.getByRole("checkbox", { name: "Hide on Hub" });
  await expect(hide).not.toBeChecked();
  await hide.click();
  await expect(hide).toBeChecked();
  expect(hub.hidden.has(fresh)).toBe(true);
  await expect(panel.getByTestId("job-hub-state")).toHaveText("On the Hub (hidden).");

  // An edit re-syncs it; the Hub's "hidden" comes back and the job stays hidden.
  await invoke(page, "update_job", { id: fresh, patch: { flagged: true, flagReason: "odd" } });
  await expect.poll(() => hub.jobs.get(fresh)?.flagged, { timeout: 15_000 }).toBe(true);
  await page.getByRole("link", { name: "Jobs", exact: true }).click();
  await expect(row("Paris").getByTestId("hub-sync-badge")).toHaveText("Hub ✓ hidden", {
    timeout: 15_000,
  });

  // Showing it again.
  await row("Paris").click();
  await panel.getByRole("checkbox", { name: "Hide on Hub" }).click();
  await expect(panel.getByRole("checkbox", { name: "Hide on Hub" })).not.toBeChecked();
  expect(hub.hidden.has(fresh)).toBe(false);

  // Jobs that never go to the Hub say why, without a switch.
  await page.goto(page.url().replace(fresh, manual));
  await expect(panel.getByTestId("job-hub-state")).toHaveText(
    "Logged by hand, so it stays on this PC.",
  );
  await expect(panel.getByRole("checkbox")).toHaveCount(0);
  await page.goto(page.url().replace(manual, old));
  await expect(panel.getByTestId("job-hub-state")).toContainText(
    "recorded before this app was linked",
  );

  // Offline: the pill says so; it opens Settings → Hub.
  hub.revoked = false;
  await hub.stop();
  await createJob(page, "While offline");
  await expect(page.getByTestId("hub-pill")).toHaveAttribute("data-state", "offline", {
    timeout: 20_000,
  });
  await page.getByTestId("hub-pill").click();
  await expect(page).toHaveURL(/\/settings#hub$/);
  await expect(page.getByTestId("hub-settings")).toBeInViewport();
});

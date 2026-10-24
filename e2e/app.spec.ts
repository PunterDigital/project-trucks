import { expect, test } from "./fixtures";

test.skip(process.platform !== "win32", "WebView2 (Windows) is the only webview with CDP");

test("launches to the dashboard with a working database", async ({ page }) => {
  await expect(page.getByRole("heading", { name: "Dashboard", exact: true })).toBeVisible();
  await expect(page.getByText("ETS2 not connected")).toBeVisible();
  await expect(page.getByRole("heading", { name: "Nothing tracked yet" })).toBeVisible();
  for (const item of ["Dashboard", "Jobs", "Trucks", "Settings"]) {
    await expect(page.getByRole("link", { name: item, exact: true })).toBeVisible();
  }
});

test("log a job, see it in history and the fleet, then delete it", async ({ page }) => {
  await page.getByRole("link", { name: "Jobs", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Jobs", exact: true })).toBeVisible();
  await page.getByRole("link", { name: "Log your first job manually" }).click();

  await expect(page.getByRole("heading", { name: "Log a job" })).toBeVisible();
  await page.getByLabel("Source city").fill("Calais");
  await page.getByLabel("Source company").fill("Tradeaux");
  await page.getByLabel("Destination city").fill("Rotterdam");
  await page.getByLabel("Destination company").fill("Kaarfor");
  await page.getByLabel("Cargo id").fill("apples");
  await page.getByLabel("Cargo name").fill("Apples");
  await page.getByLabel("Cargo mass (kg)").fill("18000");
  await page.getByLabel("Declared income").fill("12500");
  await page.getByLabel("Distance (km)").fill("412");
  await page.getByLabel("Truck make").fill("Scania");
  await page.getByLabel("Truck model").fill("S 730");

  // Validation: an empty required field blocks the save.
  await page.getByLabel("Truck model").fill("");
  await page.getByRole("button", { name: "Save job" }).click();
  await expect(page.getByText("Required")).toBeVisible();
  await page.getByLabel("Truck model").fill("S 730");

  await page.getByRole("button", { name: "Save job" }).click();
  await expect(page.getByText("Job logged")).toBeVisible();
  await expect(page.getByRole("heading", { name: "Jobs", exact: true })).toBeVisible();

  const row = page.getByRole("row").filter({ hasText: "Rotterdam" });
  await expect(row).toHaveCount(1);
  await expect(row).toContainText("Calais");
  await expect(row).toContainText("Apples");

  // Search narrows the list.
  await page.getByPlaceholder("Search city, company, cargo…").fill("nowhere-at-all");
  await expect(page.getByRole("row").filter({ hasText: "Rotterdam" })).toHaveCount(0);
  await page.getByPlaceholder("Search city, company, cargo…").fill("Rotterdam");
  await expect(page.getByRole("row").filter({ hasText: "Rotterdam" })).toHaveCount(1);

  // The truck rollup picks the job up: a manual job has no driving session,
  // so its own distance counts as delivery km.
  await page.getByRole("link", { name: "Trucks", exact: true }).click();
  const truck = page.getByRole("row").filter({ hasText: "Scania" });
  await expect(truck).toContainText("S 730");
  await expect(truck.getByRole("cell").nth(2)).toHaveText("1");
  const distance = truck.getByRole("cell").nth(3);
  await expect(distance).toContainText("412 km delivery");
  await expect(distance).toContainText("0 km free roam");
  await expect(distance.locator("div").first()).toHaveText("412 km");

  // Open the job and delete it.
  await page.getByRole("link", { name: "Jobs", exact: true }).click();
  await page.getByRole("row").filter({ hasText: "Rotterdam" }).click();
  await page.getByRole("button", { name: "Delete job" }).click();
  await expect(page.getByRole("heading", { name: "Delete this job?" })).toBeVisible();
  await page.getByRole("alertdialog").getByRole("button", { name: "Delete" }).click();
  await expect(page.getByText("Job deleted")).toBeVisible();
  await expect(page.getByRole("link", { name: "Log your first job manually" })).toBeVisible();
});

test("pages through jobs and goes back to page 1 when the search changes", async ({ page }) => {
  // Seed 30 jobs straight through the Tauri command the form uses.
  await page.evaluate(async () => {
    const tauri = (window as unknown as {
      __TAURI_INTERNALS__: { invoke: (cmd: string, args: unknown) => Promise<unknown> };
    }).__TAURI_INTERNALS__;
    for (let i = 1; i <= 30; i++) {
      const completed = new Date(Date.UTC(2026, 4, 1, 12) + i * 3600_000);
      await tauri.invoke("create_job", {
        input: {
          sessionId: null,
          truckMake: "Volvo",
          truckModel: "FH16",
          sourceCity: "Lyon",
          sourceCompany: "Posped",
          destCity: `Town ${String(i).padStart(2, "0")}`,
          destCompany: "Tradeaux",
          cargoId: "planks",
          cargoName: "Planks",
          cargoMassKg: 10000,
          declaredIncome: 1000,
          distanceKm: 100,
          durationSec: 3600,
          fuelUsedL: null,
          damageDelta: null,
          startedAt: new Date(completed.getTime() - 3600_000).toISOString(),
          completedAt: completed.toISOString(),
          status: "DELIVERED",
          source: "MANUAL",
          notes: null,
        },
      });
    }
  });

  await page.getByRole("link", { name: "Jobs", exact: true }).click();
  await expect(page.getByText("30 deliveries logged")).toBeVisible();
  await expect(page.getByText("Showing 1–25 of 30")).toBeVisible();
  await expect(page.getByText("Page 1 of 2")).toBeVisible();

  await page.getByRole("button", { name: "Next" }).click();
  await expect(page.getByText("Showing 26–30 of 30")).toBeVisible();
  await expect(page.getByRole("row").filter({ hasText: "Town" })).toHaveCount(5);
  await expect(page.getByRole("button", { name: "Next" })).toBeDisabled();

  // A new search starts from the first page again ("Town 1" = Town 10–19).
  await page.getByPlaceholder("Search city, company, cargo…").fill("Town 1");
  await expect(page.getByText("Showing 1–10 of 10")).toBeVisible();
  await expect(page.getByText("Page 1 of 1")).toBeVisible();
  await expect(page.getByRole("row").filter({ hasText: "Town 1" })).toHaveCount(10);

  // Opening a row loads that job.
  await page.getByRole("row").filter({ hasText: "Town 15" }).click();
  await expect(page.getByText("Town 15").first()).toBeVisible();
  await expect(page.getByText("Lyon").first()).toBeVisible();
});

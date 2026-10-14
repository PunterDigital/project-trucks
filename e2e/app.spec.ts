import { expect, test } from "./fixtures";

test.skip(process.platform !== "win32", "WebView2 (Windows) is the only webview with CDP");

test("launches to the dashboard with a working database", async ({ page }) => {
  await expect(page.getByRole("heading", { name: "Dashboard", exact: true })).toBeVisible();
  await expect(page.getByText("db_ping: DB ok")).toBeVisible();
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

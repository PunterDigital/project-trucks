import { existsSync, readFileSync } from "node:fs";
import path from "node:path";
import { DatabaseSync } from "node:sqlite";
import type { Page } from "@playwright/test";

import { expect, test } from "./fixtures";

test.skip(process.platform !== "win32", "WebView2 (Windows) is the only webview with CDP");

// CSV export and database backup (M3.3). The Settings buttons open the native save dialog, which
// Playwright can't drive, so the commands are called with a path in the test's temp folder and the
// files are checked from Node.

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

/** Minimal RFC 4180 parser: quoted fields, doubled quotes, CRLF rows. */
function parseCsv(text: string): string[][] {
  const rows: string[][] = [];
  let row: string[] = [];
  let field = "";
  let quoted = false;
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (quoted) {
      if (c === '"') {
        if (text[i + 1] === '"') {
          field += '"';
          i++;
        } else {
          quoted = false;
        }
      } else {
        field += c;
      }
    } else if (c === '"') {
      quoted = true;
    } else if (c === ",") {
      row.push(field);
      field = "";
    } else if (c === "\r" && text[i + 1] === "\n") {
      row.push(field);
      rows.push(row);
      row = [];
      field = "";
      i++;
    } else {
      field += c;
    }
  }
  if (field || row.length) {
    row.push(field);
    rows.push(row);
  }
  return rows;
}

test("the Settings page offers export and backup", async ({ page }) => {
  await page.getByRole("link", { name: "Settings", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Your data" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Export jobs to CSV" })).toBeEnabled();
  await expect(page.getByRole("button", { name: "Back up database" })).toBeEnabled();
});

test("export_jobs_csv writes a parseable CSV of every job", async ({ page, dataDir }) => {
  await invoke(page, "create_job", { input: job({}) });
  await invoke(page, "create_job", {
    input: job({ destCity: "Berlin", status: "LATE", notes: 'fragile, keep "cool"\nsecond line' }),
  });

  const out = path.join(dataDir, "jobs.csv");
  const count = await invoke(page, "export_jobs_csv", { path: out });
  expect(count).toBe(2);
  expect(existsSync(out)).toBe(true);

  const rows = parseCsv(readFileSync(out, "utf8"));
  expect(rows).toHaveLength(3);
  const header = rows[0];
  expect(header.slice(0, 4)).toEqual(["id", "source", "status", "completed_at"]);
  expect(header).toContain("notes");
  for (const row of rows.slice(1)) expect(row).toHaveLength(header.length);
  const col = (name: string) => header.indexOf(name);
  const berlin = rows.find((r) => r[col("dest_city")] === "Berlin")!;
  expect(berlin[col("status")]).toBe("LATE");
  expect(berlin[col("notes")]).toBe('fragile, keep "cool"\nsecond line');
  expect(berlin[col("truck_model")]).toBe("S 730");
  expect(Number(berlin[col("distance_km")])).toBe(100);

  // Nothing to export still produces the header.
  const emptyOut = path.join(dataDir, "none.csv");
  await invoke(page, "delete_job", { id: rows[1][col("id")] });
  await invoke(page, "delete_job", { id: rows[2][col("id")] });
  expect(await invoke(page, "export_jobs_csv", { path: emptyOut })).toBe(0);
  expect(parseCsv(readFileSync(emptyOut, "utf8"))).toEqual([header]);
});

test("backup_database writes a SQLite snapshot with the same jobs", async ({ page, dataDir }) => {
  await invoke(page, "create_job", { input: job({}) });
  await invoke(page, "create_job", { input: job({ destCity: "Berlin" }) });

  const out = path.join(dataDir, "backup.db");
  await invoke(page, "backup_database", { path: out });
  expect(readFileSync(out).subarray(0, 15).toString("latin1")).toBe("SQLite format 3");

  const db = new DatabaseSync(out, { readOnly: true });
  try {
    const jobs = db.prepare("SELECT dest_city FROM jobs ORDER BY dest_city").all() as {
      dest_city: string;
    }[];
    expect(jobs.map((j) => j.dest_city)).toEqual(["Berlin", "Rotterdam"]);
    const trucks = db.prepare("SELECT total_jobs FROM trucks").all() as { total_jobs: number }[];
    expect(trucks).toEqual([{ total_jobs: 2 }]);
  } finally {
    db.close();
  }

  // A second backup to the same path replaces the first (the dialog confirmed the overwrite).
  await invoke(page, "create_job", { input: job({ destCity: "Oslo" }) });
  await invoke(page, "backup_database", { path: out });
  const again = new DatabaseSync(out, { readOnly: true });
  try {
    expect((again.prepare("SELECT COUNT(*) AS n FROM jobs").get() as { n: number }).n).toBe(3);
  } finally {
    again.close();
  }

  // A path that can't be written comes back as an error naming the file.
  await expect(
    invoke(page, "backup_database", { path: path.join(dataDir, "missing-dir", "x.db") }),
  ).rejects.toThrow(/x\.db/);
});

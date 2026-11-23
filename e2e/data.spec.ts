import { existsSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import path from "node:path";
import { DatabaseSync } from "node:sqlite";
import type { Page } from "@playwright/test";

import { expect, test } from "./fixtures";

test.skip(process.platform !== "win32", "WebView2 (Windows) is the only webview with CDP");

// CSV export and database backup (M3.3). The commands open the native save dialog themselves and
// take no path from the webview (#5613). Test profiles have no dialog: the app reads the "chosen"
// path from <data dir>/e2e-save-as (written here, from Node; no file = cancelled), and the files
// are checked from Node.

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

/** What the save dialog "answers" next (the app's data dir is `<dataDir>/data`, see fixtures). */
function saveAs(dataDir: string, file: string | null) {
  const answer = path.join(dataDir, "data", "e2e-save-as");
  if (file === null) rmSync(answer, { force: true });
  else writeFileSync(answer, file);
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

test("the Settings buttons export and back up to the file the dialog chose", async ({
  page,
  dataDir,
}) => {
  await invoke(page, "create_job", { input: job({}) });
  await page.getByRole("link", { name: "Settings", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Your data" })).toBeVisible();
  const exportButton = page.getByRole("button", { name: "Export jobs to CSV" });
  const backupButton = page.getByRole("button", { name: "Back up database" });

  // Cancelled: nothing written, no toast.
  saveAs(dataDir, null);
  await exportButton.click();
  await expect(exportButton).toBeEnabled();

  const csv = path.join(dataDir, "from-button.csv");
  saveAs(dataDir, csv);
  await exportButton.click();
  await expect(page.getByText(`Exported 1 job to ${csv}`)).toBeVisible();
  expect(parseCsv(readFileSync(csv, "utf8"))).toHaveLength(2);

  const db = path.join(dataDir, "from-button.db");
  saveAs(dataDir, db);
  await backupButton.click();
  await expect(page.getByText(`Backup saved to ${db}`)).toBeVisible();
  expect(readFileSync(db).subarray(0, 15).toString("latin1")).toBe("SQLite format 3");

  // The wrong extension is refused with an error in the UI, and nothing is written.
  const bat = path.join(dataDir, "run.bat");
  saveAs(dataDir, bat);
  await exportButton.click();
  await expect(page.getByText(/Export failed: Choose a file name ending in \.csv/)).toBeVisible();
  expect(existsSync(bat)).toBe(false);
});

test("the export and backup commands take no path from the webview", async ({ page, dataDir }) => {
  // An old-style call with a path argument is ignored: the dialog's answer (none) wins.
  saveAs(dataDir, null);
  const ignored = path.join(dataDir, "ignored.csv");
  expect(await invoke(page, "export_jobs_csv", { path: ignored })).toBeNull();
  expect(await invoke(page, "backup_database", { path: ignored })).toBeNull();
  expect(existsSync(ignored)).toBe(false);
});

test("export_jobs_csv writes a parseable CSV of every job", async ({ page, dataDir }) => {
  await invoke(page, "create_job", { input: job({}) });
  await invoke(page, "create_job", {
    input: job({ destCity: "Berlin", status: "LATE", notes: 'fragile, keep "cool"\nsecond line' }),
  });

  const out = path.join(dataDir, "jobs.csv");
  saveAs(dataDir, out);
  expect(await invoke(page, "export_jobs_csv", {})).toEqual({ path: out, count: 2 });
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
  saveAs(dataDir, emptyOut);
  expect(await invoke(page, "export_jobs_csv", {})).toEqual({ path: emptyOut, count: 0 });
  expect(parseCsv(readFileSync(emptyOut, "utf8"))).toEqual([header]);
});

test("backup_database writes a SQLite snapshot with the same jobs", async ({ page, dataDir }) => {
  await invoke(page, "create_job", { input: job({}) });
  await invoke(page, "create_job", { input: job({ destCity: "Berlin" }) });

  const out = path.join(dataDir, "backup.db");
  saveAs(dataDir, out);
  expect(await invoke(page, "backup_database", {})).toBe(out);
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

  // A second backup to the same path replaces the first (an older backup is a database).
  await invoke(page, "create_job", { input: job({ destCity: "Oslo" }) });
  await invoke(page, "backup_database", {});
  const again = new DatabaseSync(out, { readOnly: true });
  try {
    expect((again.prepare("SELECT COUNT(*) AS n FROM jobs").get() as { n: number }).n).toBe(3);
  } finally {
    again.close();
  }

  // A file that isn't a database is never replaced (#5613).
  const notes = path.join(dataDir, "notes.db");
  writeFileSync(notes, "my notes");
  saveAs(dataDir, notes);
  await expect(invoke(page, "backup_database", {})).rejects.toThrow(/isn't a database/);
  expect(readFileSync(notes, "utf8")).toBe("my notes");

  // A path that can't be written comes back as an error naming the file.
  saveAs(dataDir, path.join(dataDir, "missing-dir", "x.db"));
  await expect(invoke(page, "backup_database", {})).rejects.toThrow(/x\.db/);
});

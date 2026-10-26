// Launches the real ProjectTrucks binary and drives its WebView2 window with
// Playwright over the Chrome DevTools Protocol. Windows only: WebView2 is the
// only Tauri webview that exposes CDP.
//
// The app opens the WebView2 DevTools port itself when PROJECTTRUCKS_E2E_CDP_PORT
// is set (WebView2's own WEBVIEW2_* env vars are ignored on CI runners). Every
// test gets a fresh, throwaway PROJECTTRUCKS_DATA_DIR, which holds both the
// database and the WebView2 profile, so the suite never touches real data.
//
// Every test fails on a console error or uncaught exception, which includes
// Content Security Policy violations ("Refused to load ...").

import { spawn, type ChildProcess } from "node:child_process";
import { existsSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { chromium, expect, test as base, type Browser, type Page } from "@playwright/test";

const repoRoot = path.resolve(import.meta.dirname, "..");

function resolveExe(): string {
  const fromEnv = process.env.PROJECTTRUCKS_EXE;
  if (fromEnv) return path.resolve(fromEnv);
  for (const profile of ["release", "debug"]) {
    const candidate = path.join(repoRoot, "src-tauri", "target", profile, "projecttrucks.exe");
    if (existsSync(candidate)) return candidate;
  }
  throw new Error(
    "No ProjectTrucks binary found. Run `pnpm tauri build --no-bundle` (or set PROJECTTRUCKS_EXE).",
  );
}

async function waitForCdp(port: number, app: ChildProcess, timeoutMs = 60_000): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (app.exitCode !== null) throw new Error(`App exited early with code ${app.exitCode}`);
    try {
      const res = await fetch(`http://127.0.0.1:${port}/json/version`);
      if (res.ok) return;
    } catch {
      // not listening yet
    }
    await new Promise((r) => setTimeout(r, 250));
  }
  throw new Error(`WebView2 debug port ${port} never came up`);
}

async function appPage(browser: Browser, timeoutMs = 30_000): Promise<Page> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const page = browser
      .contexts()
      .flatMap((c) => c.pages())
      .find((p) => !p.url().startsWith("about:"));
    if (page) return page;
    await new Promise((r) => setTimeout(r, 250));
  }
  throw new Error("App window never loaded");
}

export interface RunningApp {
  page: Page;
  process: ChildProcess;
  /** Console errors and uncaught exceptions seen since the page loaded. */
  errors: string[];
  /** Resolves with the exit code once the process has ended. */
  exited: Promise<number | null>;
  /** Closes the CDP connection and kills the app (if it is still running). */
  close: () => Promise<void>;
}

/**
 * Starts the app on a throwaway data dir and connects to its window. The `page` fixture uses
 * this; specs that need to restart the app (or watch it exit) call it themselves.
 */
export async function launchApp(options: {
  dataDir: string;
  port: number;
  env?: Record<string, string>;
}): Promise<RunningApp> {
  const app = spawn(resolveExe(), [], {
    env: {
      ...process.env,
      ...options.env,
      // The app also puts its WebView2 profile under this folder.
      PROJECTTRUCKS_DATA_DIR: path.join(options.dataDir, "data"),
      PROJECTTRUCKS_E2E_CDP_PORT: String(options.port),
    },
    stdio: "inherit",
  });
  const exited = new Promise<number | null>((resolve) => {
    if (app.exitCode !== null) resolve(app.exitCode);
    else app.once("exit", (code) => resolve(code));
  });
  const kill = async () => {
    if (app.exitCode === null) {
      app.kill();
      await new Promise((r) => setTimeout(r, 500));
    }
  };
  try {
    await waitForCdp(options.port, app);
    const browser = await chromium.connectOverCDP(`http://127.0.0.1:${options.port}`);
    const page = await appPage(browser);
    await page.waitForLoadState("domcontentloaded");
    // Listen first, then reload, so errors from the initial load (CSP
    // violations, failed fonts) are caught too.
    const errors: string[] = [];
    page.on("console", (msg) => {
      if (msg.type() === "error") errors.push(msg.text());
    });
    page.on("pageerror", (err) => errors.push(err.message));
    await page.reload({ waitUntil: "load" });
    return {
      page,
      process: app,
      errors,
      exited,
      close: async () => {
        await browser.close().catch(() => {});
        await kill();
      },
    };
  } catch (e) {
    await kill();
    throw e;
  }
}

type Fixtures = {
  page: Page;
  dataDir: string;
  /** Telemetry replay fixture (see src-tauri/src/telemetry/replay.rs); `null` = live reader. */
  replay: unknown;
  /** Extra environment for the app (e.g. the fake update feed in `updates.spec.ts`). */
  appEnv: Record<string, string>;
};

export const test = base.extend<Fixtures>({
  replay: [null, { option: true }],
  appEnv: [{}, { option: true }],

  dataDir: async ({}, use) => {
    const dir = mkdtempSync(path.join(tmpdir(), "projecttrucks-e2e-"));
    await use(dir);
    rmSync(dir, { recursive: true, force: true, maxRetries: 5, retryDelay: 500 });
  },

  page: async ({ dataDir, replay, appEnv }, use, testInfo) => {
    const env: Record<string, string> = { ...appEnv };
    if (replay) {
      const file = path.join(dataDir, "replay.json");
      writeFileSync(file, JSON.stringify(replay));
      env.PROJECTTRUCKS_TELEMETRY_REPLAY = file;
      env.PROJECTTRUCKS_TELEMETRY_REPLAY_INTERVAL_MS = "400";
    }
    const app = await launchApp({ dataDir, port: 9300 + testInfo.workerIndex, env });
    try {
      await use(app.page);
      expect(app.errors, "console errors").toEqual([]);
    } finally {
      await app.close();
    }
  },
});

export { expect };

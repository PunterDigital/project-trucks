// Launches the real ProjectTrucks binary and drives its WebView2 window with
// Playwright over the Chrome DevTools Protocol. Windows only: WebView2 is the
// only Tauri webview that exposes CDP.
//
// Every test gets a fresh, throwaway data directory (PROJECTTRUCKS_DATA_DIR)
// and WebView2 profile, so the suite never touches the real database.

import { spawn, type ChildProcess } from "node:child_process";
import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { chromium, test as base, type Browser, type Page } from "@playwright/test";

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

type Fixtures = { page: Page; dataDir: string };

export const test = base.extend<Fixtures>({
  dataDir: async ({}, use) => {
    const dir = mkdtempSync(path.join(tmpdir(), "projecttrucks-e2e-"));
    await use(dir);
    rmSync(dir, { recursive: true, force: true, maxRetries: 5, retryDelay: 500 });
  },

  page: async ({ dataDir }, use, testInfo) => {
    const port = 9300 + testInfo.workerIndex;
    const app = spawn(resolveExe(), [], {
      env: {
        ...process.env,
        PROJECTTRUCKS_DATA_DIR: path.join(dataDir, "data"),
        WEBVIEW2_USER_DATA_FOLDER: path.join(dataDir, "webview"),
        WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
      },
      stdio: "inherit",
    });
    try {
      await waitForCdp(port, app);
      const browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`);
      const page = await appPage(browser);
      await page.waitForLoadState("domcontentloaded");
      await use(page);
      await browser.close();
    } finally {
      app.kill();
      await new Promise((r) => setTimeout(r, 500));
    }
  },
});

export { expect } from "@playwright/test";

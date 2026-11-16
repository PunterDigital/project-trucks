import { defineConfig } from "@playwright/test";

// The E2E suite drives the built desktop app (see e2e/fixtures.ts). Build it
// first: `pnpm tauri build --no-bundle`, then `pnpm test:e2e`.
//
// Tests run in parallel (#4675): every test starts its own app on a throwaway data dir, the
// WebView2 debug port is 9300 + the worker index and the Hub / update mocks listen on port 0,
// so nothing is shared between workers. CI (windows-latest, 4 vCPU) uses 3 workers; on Shay's
// PC 2, so a run doesn't take much CPU from a game. PT_E2E_WORKERS overrides both.
const workers = process.env.PT_E2E_WORKERS
  ? Number(process.env.PT_E2E_WORKERS)
  : process.env.CI
    ? 3
    : 2;

export default defineConfig({
  testDir: "./e2e",
  globalSetup: "./e2e/global-setup.ts",
  globalTeardown: "./e2e/global-teardown.ts",
  workers,
  fullyParallel: true,
  timeout: 90_000,
  expect: { timeout: 10_000 },
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? [["list"], ["html", { open: "never" }]] : "list",
  use: {
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
  },
});

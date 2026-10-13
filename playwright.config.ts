import { defineConfig } from "@playwright/test";

// The E2E suite drives the built desktop app (see e2e/fixtures.ts). Build it
// first: `pnpm tauri build --no-bundle`, then `pnpm test:e2e`.
export default defineConfig({
  testDir: "./e2e",
  // One app window per test; the WebView2 debug port is per worker.
  workers: 1,
  fullyParallel: false,
  timeout: 90_000,
  expect: { timeout: 10_000 },
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? [["list"], ["html", { open: "never" }]] : "list",
  use: {
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
  },
});

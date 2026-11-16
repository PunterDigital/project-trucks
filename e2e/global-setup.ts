// Notes when this run started, so global-teardown.ts removes only this run's leftover data dirs.
export default function globalSetup(): void {
  process.env.PT_E2E_RUN_STARTED_MS = String(Date.now());
}

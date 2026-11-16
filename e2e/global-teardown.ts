// Removes the throwaway data dirs that a test's own cleanup couldn't delete because WebView2
// still held a file (see the dataDir fixture). By now every app has exited. Only folders made
// during this run are touched, so another run's live data dir is never deleted.
import { readdirSync, rmSync, statSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";

export default function globalTeardown(): void {
  const started = Number(process.env.PT_E2E_RUN_STARTED_MS ?? Number.NaN);
  if (!Number.isFinite(started)) return;

  for (const name of readdirSync(tmpdir())) {
    if (!name.startsWith("projecttrucks-e2e-")) continue;
    const dir = path.join(tmpdir(), name);
    try {
      if (statSync(dir).birthtimeMs < started) continue;
      rmSync(dir, { recursive: true, force: true, maxRetries: 10, retryDelay: 500 });
    } catch (error) {
      console.warn(`Left ${dir}: ${(error as Error).message}`);
    }
  }
}

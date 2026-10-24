import { fileURLToPath } from "node:url";
import { defineConfig } from "vitest/config";

// Unit tests for frontend logic. Kept apart from vite.config.ts (Tauri dev
// server settings) and from e2e/ (Playwright, which also uses *.spec.ts).
export default defineConfig({
  resolve: {
    alias: {
      "@": fileURLToPath(new URL("./src", import.meta.url)),
    },
  },
  test: {
    include: ["src/**/*.test.{ts,tsx}"],
  },
});

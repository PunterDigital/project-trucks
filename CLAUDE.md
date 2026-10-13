# ProjectTrucks — notes for Claude

Local Euro Truck Simulator 2 delivery tracker. Tauri 2 desktop app: React 19 + Vite + Tailwind v4 +
shadcn/ui frontend, Rust backend with SQLite via sqlx. Read `PLAN.md` (architecture) and
`CLAUDE_CODE_PLAN.md` (milestone tasks) before feature work; if they conflict, `PLAN.md` wins.

## Commands

| What | Command |
|---|---|
| Install | `pnpm install` |
| Run the app | `pnpm tauri dev` |
| Frontend typecheck | `pnpm typecheck` (app + E2E sources) |
| Frontend build | `pnpm build` |
| Rust format | `cd src-tauri && cargo fmt --check` |
| Rust lint | `cd src-tauri && cargo clippy --all-targets -- -D warnings` |
| Rust tests | `cd src-tauri && cargo test` (unit + `tests/m1_demo_flow.rs`, in-memory SQLite) |
| Release binary (for E2E) | `pnpm tauri build --no-bundle` |
| Installers | `pnpm tauri build` (Windows: NSIS `.exe` + `.msi`) |
| E2E | `pnpm test:e2e` (Windows only; needs the release or debug binary) |

`src-tauri`'s build script needs `dist/` to exist: run `pnpm build` before cargo commands on a fresh
checkout.

## E2E approach

Playwright drives the **real app binary** (not a dev server): `e2e/fixtures.ts` spawns
`src-tauri/target/{release,debug}/projecttrucks.exe` (or `PROJECTTRUCKS_EXE`) with
`PROJECTTRUCKS_E2E_CDP_PORT=<port>`. The app then builds its main window with
`--remote-debugging-port` (see `create_main_window` in `src-tauri/src/lib.rs`; the window is
`create: false` in `tauri.conf.json`), and the test connects with `chromium.connectOverCDP`.
WebView2's own `WEBVIEW2_*` env vars are ignored on GitHub's Windows runners, so don't switch back to
them. Only WebView2 exposes CDP, so the suite skips on macOS; macOS CI does a build +
`codesign --verify` + launch smoke test instead.

Isolation: every test sets `PROJECTTRUCKS_DATA_DIR` (see `src-tauri/src/db/mod.rs`), which holds the
SQLite database and the WebView2 profile (`<dir>/webview`). Never run E2E without it: the default
data dir is Shay's real database at `%APPDATA%\co.shaypunter.projecttrucks\`.

## CI / release

- `ci.yml`: ubuntu frontend typecheck/build; windows fmt/clippy/test, installers, E2E against the release
  binary; macos universal build, signature check, launch smoke.
- `release.yml` on `v*` tags: creates the release with generated notes, `tauri-action` uploads the
  Windows `_x64-setup.exe` / `.msi` and macOS `_universal.dmg`, then a job verifies all three exist.
- Version lives in `package.json`, `src-tauri/tauri.conf.json` and `src-tauri/Cargo.toml` (+ `Cargo.lock`):
  bump all of them in the release commit.
- macOS builds are ad-hoc signed (`bundle.macOS.signingIdentity: "-"`), not notarised.

## Quirks

- The ETS2 telemetry plugin DLLs under `assets/telemetry-plugin/` are third-party binaries. Their
  SHA-256s are in that folder's README; re-check them if the files change.
- Telemetry (M2) is Windows-only (memory-mapped file from the plugin). The macOS build is useful for
  manual logging only.
- A copied-in `src-tauri/target` from another path breaks the build ("failed to read plugin
  permissions … app_hide.toml"): delete `src-tauri/target`.

# ProjectTrucks

[![CI](https://github.com/PunterDigital/project-trucks/actions/workflows/ci.yml/badge.svg)](https://github.com/PunterDigital/project-trucks/actions/workflows/ci.yml)

A local Euro Truck Simulator 2 delivery tracker. Runs on your own machine, reads
telemetry while you play, and writes every kilometre and every job to a local
SQLite database. No accounts, no cloud, no telemetry-back-to-us.

**Status:** **M1 complete.** You can log jobs by hand, search and filter the
history, edit notes, flag rows, delete jobs, and watch the per-truck rollups
update. Auto-capture from ETS2 telemetry lands in M2. The architectural plan is
in [PLAN.md](PLAN.md); the milestone-by-milestone implementation plan is in
[CLAUDE_CODE_PLAN.md](CLAUDE_CODE_PLAN.md).

Licensed under **MIT** (see [LICENSE](LICENSE)). The plugin DLL we bundle in
M2 is also MIT (built from
[Funbit/ets2-sdk-plugin](https://github.com/Funbit/ets2-sdk-plugin)) —
provenance is in [assets/telemetry-plugin/README.md](assets/telemetry-plugin/README.md).

> _Screenshot placeholder — add a shot of the Jobs page once you've got a few
> rows logged. Drop it in `docs/screenshots/jobs.png` and reference it here._

## Download

Grab the latest Windows installer or macOS `.dmg` from the
[downloads page](https://punterdigital.github.io/personal-apps/) or the
[GitHub Releases](https://github.com/PunterDigital/project-trucks/releases).
Builds are unsigned: on Windows click **More info → Run anyway** at the
SmartScreen prompt; on macOS right-click the app and choose **Open** (or allow
it under System Settings → Privacy & Security). ETS2 telemetry capture (M2) is
Windows-only; the macOS build supports manual logging.

## What you can do today (M1)

- **Log a job by hand** at `/jobs/new`. Every column the auto-capture path will
  eventually fill is editable (route, cargo, distance, duration, truck, status,
  fuel, damage, notes). Full client-side validation via Zod.
- **Browse the history** at `/jobs`. Search across city/company/cargo, filter
  by status, narrow by completed-at date range, paginate at 25 rows per page.
- **Open any job** at `/jobs/:id` for the full record. Notes / flagged / flag
  reason are debounced-saved (800 ms). Delete with a confirm dialog.
- **See your fleet** at `/trucks` — list of every truck driven, sorted by
  total distance, with the delivery / free-roam split shown underneath. Click
  through to a placeholder detail page (real one lands in Task 3.2).
- **Trust the rollups.** `recompute_truck` runs inside every write path; the
  `trucks` table is purely derived state. There's a regression-guard
  integration test (`tests/m1_demo_flow.rs`) that walks the whole demo flow:
  log seven jobs, exercise list / search / filter / pagination, patch one,
  delete another, then assert the truck rollups still match what's left.

## Prerequisites (Windows)

- Rust toolchain (`rustup` stable) — verify with `rustc --version`
- Node.js 20+ and `pnpm` — verify with `node --version` and `pnpm --version`
- Microsoft C++ Build Tools (required for native Rust crates)
- WebView2 runtime (preinstalled on Windows 11; bundled by the Tauri installer
  otherwise)

The Tauri 2 CLI ships as a local devDependency, so you don't need a global
install.

## Local development

```sh
# install frontend dependencies
pnpm install

# run the desktop app in dev mode (vite + cargo run + window)
pnpm tauri dev
```

First launch will compile the full Tauri dep tree from scratch — expect ~2
minutes on a warm cache. Subsequent runs are incremental (~10–30 s).

## Build a production bundle

```sh
pnpm tauri build
```

This produces an MSI installer under `src-tauri/target/release/bundle/msi/`.
You can install that as a normal Windows app.

## Where the data lives

The local SQLite database is stored at:

```
%APPDATA%\co.shaypunter.projecttrucks\projecttrucks.db
```

You can copy that file as a backup any time. Schema migrations are append-only
SQL files under `src-tauri/migrations/` and run automatically on every launch.

## Project layout

```
src/                React frontend (Vite + TypeScript + Tailwind v4 + shadcn/ui)
  components/       UI primitives (shadcn) + app components (Sidebar, Header,
                    DateRangePicker, JobStatusBadge)
  routes/           One file per top-level page (Dashboard, Jobs, JobNew,
                    JobDetail, Trucks, TruckDetail, Settings)
  lib/              api wrapper, formatters, hooks
  types.ts          Hand-mirrored wire types for the Rust command surface
src-tauri/          Rust backend
  migrations/       SQL migrations applied by sqlx at startup
  src/db/           Pool, AppState, models, truck-rollup helper
  src/commands/     Tauri commands (sessions, jobs, trucks, db)
  tests/            Integration tests against in-memory SQLite
```

## Useful scripts

```sh
pnpm dev             # vite dev server only (no Tauri window)
pnpm build           # tsc + vite build (frontend bundle)
pnpm tauri dev       # full app
pnpm tauri build     # release installer

cd src-tauri && cargo test          # all Rust tests (unit + integration)
cd src-tauri && cargo clippy        # Rust lints (CI runs with -D warnings)
pnpm typecheck                      # TypeScript (app + E2E)
pnpm tauri build --no-bundle && pnpm test:e2e   # Playwright E2E against the real app (Windows)
```

The E2E suite launches the built app with a throwaway data directory
(`PROJECTTRUCKS_DATA_DIR`), so it never touches your real database.

## Contributing

This is a personal project, but the code is MIT — feel free to fork. There's
no contribution process yet.

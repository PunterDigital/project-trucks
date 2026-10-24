# ProjectTrucks

[![CI](https://github.com/PunterDigital/project-trucks/actions/workflows/ci.yml/badge.svg)](https://github.com/PunterDigital/project-trucks/actions/workflows/ci.yml)

A local Euro Truck Simulator 2 delivery tracker. Runs on your own machine, reads
telemetry while you play, and writes every kilometre and every job to a local
SQLite database. No accounts, no cloud, no telemetry-back-to-us.

**Status:** **M3 complete (v0.4.0).** Deliveries and free-roam drives are
captured from ETS2 automatically through the bundled telemetry plugin, or
logged by hand. There is a dashboard with lifetime and monthly stats and a
30-day chart, a page per truck, a searchable job history, CSV export and a
one-click database backup. The architectural plan is in [PLAN.md](PLAN.md);
the milestone-by-milestone implementation plan is in
[CLAUDE_CODE_PLAN.md](CLAUDE_CODE_PLAN.md).

Licensed under **MIT** (see [LICENSE](LICENSE)). The bundled plugin DLL is
also MIT (built from
[Funbit/ets2-sdk-plugin](https://github.com/Funbit/ets2-sdk-plugin)) —
provenance is in [assets/telemetry-plugin/README.md](assets/telemetry-plugin/README.md).

## Screenshots

![Dashboard: lifetime and monthly stats, a 30-day distance chart and the on-time rate](docs/screenshots/dashboard.png)

![Jobs: every delivery with route, cargo, distance, status and income, searchable and filterable](docs/screenshots/jobs.png)

![A truck's page: its own stats, 30-day chart, jobs and driving sessions](docs/screenshots/truck.png)

## Download

ProjectTrucks is **Windows only**. Grab the latest
installer (`.exe` or `.msi`) from the
[downloads page](https://punterdigital.github.io/personal-apps/) or the
[GitHub Releases](https://github.com/PunterDigital/project-trucks/releases).
Builds are unsigned: click **More info → Run anyway** at the SmartScreen prompt.
(v0.1.0 also shipped a macOS `.dmg` for manual logging; later versions don't.)

## What you can do today (M1–M3)

- **Auto-capture from ETS2.** Install the telemetry plugin from Settings (the
  app offers it on first launch), switch auto-capture on and play: every
  delivery and every free-roam stretch is recorded with its distance, duration,
  truck, cargo, income and on-time / late / cancelled outcome. Suspicious
  records (impossible average speed, under 1 km) are flagged for you to check.
- **Dashboard.** Lifetime and this-month kilometres (delivery vs free roam),
  deliveries, income and on-time rate with month-on-month deltas, a 30-day
  distance chart, an on-time donut and your most recent jobs.
- **Log a job by hand** at `/jobs/new` when you'd rather not run the plugin.
  Every column is editable, with client-side validation.
- **Browse the history** at `/jobs`. Search across city/company/cargo, filter
  by status, narrow by completed-at date range, paginate at 25 rows per page.
- **Open any job** at `/jobs/:id` for the full record. Notes, flagged and flag
  reason save as you type. Delete with a confirm dialog.
- **See your fleet** at `/trucks`: every truck driven with its delivery /
  free-roam split, and a page per truck with its own stats, 30-day chart, jobs
  and driving sessions.
- **Your data is yours.** Export every job to CSV or save a snapshot of the
  database from Settings. The local SQLite file is the only copy; nothing is
  sent anywhere.

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
pnpm lint                           # ESLint
pnpm test                           # Vitest unit tests
pnpm tauri build --no-bundle && pnpm test:e2e   # Playwright E2E against the real app (Windows)
```

The E2E suite launches the built app with a throwaway data directory
(`PROJECTTRUCKS_DATA_DIR`), so it never touches your real database.

## Contributing

This is a personal project, but the code is MIT — feel free to fork. There's
no contribution process yet.

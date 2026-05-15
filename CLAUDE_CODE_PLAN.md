# ProjectTrucks — Claude Code Implementation Plan

A step-by-step plan for Claude Code to execute. Each task is sized to be a single, self-contained session: clear goal, files to touch, acceptance criteria.

Read this file alongside `PLAN.md` (the architectural plan). If the two ever conflict, the architectural plan wins — open an issue and ask Shay rather than guessing.

## Project context (for every session)

**What:** ProjectTrucks — a Tauri 2 desktop app that auto-logs your Euro Truck Simulator 2 deliveries to a local SQLite database and visualises them in a React UI.

**Locked decisions:**

* Brand: ProjectTrucks
* Game: ETS2 only (no ATS, no other sims)
* Licence: GPL-3, open source. Forced by our use of Funbit's GPL-3 telemetry plugin DLL.
* Updates: ship manually, no auto-update infra
* Multi-user / hosted version: not in scope, don't preserve the upgrade path

**Target OS:** Windows (primary). Tauri may build for Linux/Mac too but we don't test those — ETS2 only ships for Windows in practice.

## Prerequisites

Before starting M0, confirm the dev environment has:

* [ ] Rust toolchain (`rustup`, stable channel, latest)
* [ ] Node.js 20+ and `pnpm` (preferred package manager)
* [ ] Tauri 2 CLI: `cargo install tauri-cli --version "^2.0.0"`
* [ ] WebView2 runtime (preinstalled on Windows 11; bundled by Tauri installer otherwise)
* [ ] Microsoft C++ Build Tools (for native Rust crates on Windows)
* [ ] Git
* [ ] (Optional) Visual Studio with C++ workload — needed only if we build the SCS telemetry plugin ourselves in M2

## Repository layout (target)

```
ProjectTrucks/
├── PLAN.md                      # architectural plan
├── CLAUDE_CODE_PLAN.md          # this file
├── README.md                    # written in M4
├── LICENSE                      # MIT, added in M0
├── .gitignore
├── package.json                 # frontend deps
├── pnpm-lock.yaml
├── vite.config.ts
├── tsconfig.json
├── tailwind.config.ts
├── postcss.config.js
├── index.html
├── src/                         # React frontend
│   ├── main.tsx
│   ├── App.tsx
│   ├── routes/
│   │   ├── Dashboard.tsx
│   │   ├── Jobs.tsx
│   │   ├── JobDetail.tsx
│   │   ├── Trucks.tsx
│   │   └── Settings.tsx
│   ├── components/
│   │   ├── ui/                  # shadcn/ui generated components
│   │   ├── Sidebar.tsx
│   │   ├── StatCard.tsx
│   │   └── ...
│   ├── lib/
│   │   ├── api.ts               # thin wrapper around Tauri invoke()
│   │   └── format.ts            # km/mi, currency, dates
│   └── types.ts                 # shared with Rust via specta/ts-rs (or hand-mirrored)
├── src-tauri/                   # Rust backend
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── build.rs
│   ├── migrations/              # SQL migrations (sqlx)
│   │   └── 0001_init.sql
│   ├── icons/
│   └── src/
│       ├── main.rs
│       ├── lib.rs
│       ├── commands/
│       │   ├── mod.rs
│       │   ├── jobs.rs
│       │   ├── trucks.rs
│       │   └── settings.rs
│       ├── db/
│       │   ├── mod.rs
│       │   └── models.rs
│       ├── telemetry/
│       │   ├── mod.rs
│       │   ├── reader.rs        # MMF reader
│       │   ├── struct_v.rs      # SCS struct layout (versioned)
│       │   ├── state_machine.rs # job detection
│       │   └── installer.rs     # plugin DLL installer
│       └── error.rs
└── assets/
    └── telemetry-plugin/        # Funbit plugin DLL we ship (see M2 Task 0)
        └── README.md            # provenance + licence note
```

## Coding conventions

* Rust: edition 2021, `cargo fmt` and `cargo clippy -- -D warnings` on every commit. Prefer `anyhow::Result` at the boundary, custom error types inside modules.
* TypeScript: strict mode on, `pnpm lint` and `pnpm typecheck` clean. No `any` outside `lib/api.ts` where Tauri types are loose.
* SQL: sqlx migrations are append-only. Never edit a migration after it's been committed.
* Commits: Conventional Commits (`feat:`, `fix:`, `chore:`, `docs:`).
* Branching: one branch per task in this plan. Merge to `main` only when acceptance criteria pass.
* Tests: Rust unit tests next to the code (`#[cfg(test)]`). Frontend tests with Vitest where logic is non-trivial. We don't aim for a coverage number — test the state machine and any maths.

## M0 — Tauri scaffold

**Goal:** A Tauri app that boots, shows an empty sidebar shell, and has SQLite plumbed through. No real features yet.

### Task 0.1 — Initialise the repo

**Goal:** Git repo with GPL-3 licence and `.gitignore`.

**Files:** `LICENSE` (full GPL-3 text from https://www.gnu.org/licenses/gpl-3.0.txt), `.gitignore`, `README.md` (one-paragraph placeholder).

**Steps:**

1. `git init`
2. Add the GPL-3 licence text. Add a copyright header (`Copyright (C) 2026 Shay Punter`).
3. `.gitignore` should cover: `node_modules/`, `dist/`, `target/`, `src-tauri/target/`, `.env*` (except `.env.example`), `*.db`, `*.db-journal`.
4. Stage README placeholder: "ProjectTrucks — a local ETS2 delivery tracker. WIP. Licensed under GPL-3."
5. `git commit -m "chore: init repo"`

**Acceptance:** Clean repo, one commit, GPL-3 licence present.

### Task 0.2 — Bootstrap the Tauri 2 + Vite + React + TS template

**Goal:** Working `tauri dev` that opens a blank window.

**Steps:**

1. From the repo root: `pnpm create tauri-app@latest .` — choose React, TypeScript, pnpm, Vite. Use package name `projecttrucks` and window title `ProjectTrucks`.
2. `pnpm install`
3. `cd src-tauri && cargo build` to verify Rust toolchain is healthy.
4. `pnpm tauri dev` and confirm the default Tauri window opens.
5. Commit: `chore: scaffold tauri + vite + react`.

**Acceptance:** `pnpm tauri dev` opens a window saying "Welcome to Tauri" (or similar default).

### Task 0.3 — Add Tailwind + shadcn/ui

**Goal:** Tailwind classes work; shadcn `Button` and `Card` components installed.

**Steps:**

1. Install Tailwind v3 (or v4 if stable): follow current Tailwind + Vite + React install guide.
2. Configure `tailwind.config.ts` to scan `./src/**/*.{ts,tsx}`.
3. Add base `index.css` with the `@tailwind` directives, import in `main.tsx`.
4. Init shadcn/ui: `pnpm dlx shadcn@latest init` — pick neutral base colour, CSS variables on.
5. Add starter components: `pnpm dlx shadcn@latest add button card table input badge` (anything we know we'll need).
6. Replace the default `App.tsx` body with a Tailwind-styled heading "ProjectTrucks" centered in the viewport, to prove styling works.
7. Commit: `feat: add tailwind and shadcn/ui`.

**Acceptance:** App shows a styled heading; running `pnpm dlx shadcn@latest add <name>` adds components to `src/components/ui/`.

### Task 0.4 — App shell (sidebar + routing)

**Goal:** Sidebar with nav items (Dashboard, Jobs, Trucks, Settings) routes to empty placeholder pages.

**Steps:**

1. Add React Router: `pnpm add react-router-dom`.
2. Create `src/routes/` with four placeholder components, each showing just their page title.
3. Create `src/components/Sidebar.tsx`: vertical nav with icons (use `lucide-react`, already in shadcn deps) and active-state highlighting.
4. Wire up routes in `App.tsx` with a two-column layout (sidebar + outlet).
5. Add a header bar with the app name and a placeholder "Auto-capture: off" pill (will become a real toggle in M2).
6. Commit: `feat: app shell with sidebar nav`.

**Acceptance:** Clicking each sidebar item swaps the main pane to the right placeholder page. Layout doesn't break at 1280×720 or 1920×1080.

### Task 0.5 — SQLite + sqlx wired up

**Goal:** SQLite database file is created on first run in the app data dir. A trivial Tauri command (`db_ping`) confirms the connection works from the frontend.

**Steps:**

1. In `src-tauri/Cargo.toml`, add deps: `sqlx = { version = "0.8", features = ["runtime-tokio", "sqlite", "macros", "migrate", "chrono"] }`, `tokio = { version = "1", features = ["full"] }`, `chrono = { version = "0.4", features = ["serde"] }`, `serde = { version = "1", features = ["derive"] }`, `serde_json = "1"`, `anyhow = "1"`, `thiserror = "2"`, `uuid = { version = "1", features = ["v4", "serde"] }`, `tracing = "0.1"`, `tracing-subscriber = "0.3"`.
2. Create `src-tauri/migrations/0001_init.sql` with the schema from PLAN.md §5 (driving_sessions, jobs, trucks, settings tables — note `jobs.session_id` is an FK into `driving_sessions`).
3. Create `src-tauri/src/db/mod.rs`: exposes `pub async fn init_pool(app_handle: &AppHandle) -> Result<SqlitePool>`. Resolves the DB path via `app_handle.path().app_data_dir()` + `/projecttrucks.db`. Creates the file if missing. Runs `sqlx::migrate!("./migrations")` on startup.
4. Update `src-tauri/src/lib.rs` to initialise the pool in `setup()` and store it as managed state.
5. Add a `db_ping` Tauri command that runs `SELECT 1` and returns the result.
6. From the frontend, call it on Dashboard mount and render the result so we can eyeball it.
7. Commit: `feat: sqlite + sqlx wired up with migrations`.

**Acceptance:** Launching the app creates `projecttrucks.db` in the app data dir, the `driving_sessions`/`jobs`/`trucks`/`settings` tables exist (verify with `sqlite3` CLI), and the Dashboard shows "DB ok" after mount.

### Task 0.6 — M0 wrap-up

**Goal:** README updated, screenshot taken, milestone closed.

**Steps:**

1. Update README with build instructions (`pnpm install`, `pnpm tauri dev`) and a screenshot of the shell.
2. Tag: `git tag v0.0.1-m0`.

**Acceptance:** A fresh clone, fresh install, `pnpm tauri dev` works first try.

## M1 — Job CRUD + manual entry

**Goal:** Manually log a job through a form; see it in the history table; delete it; per-truck rollups stay correct.

### Task 1.1 — Domain model in Rust

**Goal:** `DrivingSession`, `Job` and `Truck` structs with serde + sqlx mappings.

**Files:** `src-tauri/src/db/models.rs`.

**Steps:**

1. Define `DrivingSession` struct mirroring the SQL schema. `#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]`.
2. Define `Job` struct mirroring the SQL schema. All fields including `session_id: Option<String>`.
3. Define enums for `SessionKind` (`FreeRoam` | `Job`), `SessionStatus` (`Completed` | `Cancelled`), `JobStatus`, `JobSource`. Serialize as SCREAMING_SNAKE_CASE strings to match the DB.
4. Define `Truck` struct.
5. Define input types: `NewSession`, `NewJob` — no `id`, `id` generated server-side.
6. Add unit tests that round-trip each through JSON.

**Acceptance:** `cargo test` passes. `cargo clippy` is clean.

### Task 1.2 — Session and job commands

**Goal:** Tauri commands for sessions and jobs. Truck rollups stay correct through every write path.

**Files:** `src-tauri/src/commands/sessions.rs`, `src-tauri/src/commands/jobs.rs`.

**Commands to expose:**

* `create_session(input: NewSession) -> DrivingSession` — used by the telemetry pipeline (and by tests).
* `list_sessions(filter: SessionFilter) -> SessionsPage`.
* `create_job(input: NewJob) -> Job` — generates UUID, inserts. If `session_id` is provided, the session row must exist. Returns the created row.
* `list_jobs(filter: JobFilter) -> JobsPage` — supports limit/offset, sort by `completed_at`, filter by status, date range, free-text search on city/cargo.
* `get_job(id: String) -> Option<Job>`
* `update_job(id: String, patch: JobPatch) -> Job` — only allow editing notes and `flagged`/`flag_reason` for MVP.
* `delete_job(id: String) -> ()` — also recomputes the affected `trucks` rollup. Does not delete the linked session row (the km still happened).
* `delete_session(id: String) -> ()` — also deletes any linked `jobs` row (CASCADE) and recomputes the truck rollup.

**Steps:**

1. Implement each command. Use parameterised queries via `sqlx::query_as!`.
2. Centralise truck-rollup maintenance in a single fn `recompute_truck(make: &str, model: &str, pool)` that does the math from scratch (sums sessions + counts jobs). Call it inside every write transaction.
3. Truck `id = sha256(make|model)[..16]`. Upsert truck row on first session/job for that make+model.
4. Add Rust integration tests against an in-memory SQLite covering: insert free-roam session → totals update; insert job-linked session + job → both totals update; delete job → delivery total falls but free-roam unchanged; delete session → both fall.
5. Register the commands in `lib.rs` invoke handler.

**Acceptance:** Tests pass. From a Vite dev console, creating a session followed by a job linked to it returns consistent truck rollups.

### Task 1.3 — Frontend API layer

**Goal:** Type-safe wrappers for the Tauri commands.

**Files:** `src/lib/api.ts`, `src/types.ts`.

**Steps:**

1. Hand-mirror the Rust types in `src/types.ts` (`Job`, `Truck`, `NewJob`, `JobFilter`, `JobsPage`, enums). Keep them in lockstep with Rust manually — if this becomes annoying, swap to `specta` later.
2. `src/lib/api.ts` wraps `invoke<T>(name, args)` for each command. Return real types, not `unknown`.
3. Add a `lib/format.ts` with helpers: `formatKm`, `formatCurrency` (game currency, EUR sign), `formatDuration` (h/m), `formatRelativeDate`.

**Acceptance:** `pnpm typecheck` clean. The API layer is the only file that touches `invoke`.

### Task 1.4 — Manual job entry form

**Goal:** A form on `/jobs/new` that lets the user log a job by hand. Every field that telemetry will eventually fill is editable.

**Files:** `src/routes/JobNew.tsx`, route added to `App.tsx`.

**Steps:**

1. Use `react-hook-form` + `zod` for validation: `pnpm add react-hook-form zod @hookform/resolvers`.
2. Build the form with shadcn `Form`, `Input`, `Select`, `Textarea`. Fields: source city/company, dest city/company, cargo id/name, cargo mass kg, declared income, distance km, duration (input as h:m:m or split), started at, completed at, status (DELIVERED/LATE/CANCELLED), truck make/model, fuel used (optional), damage delta (optional), notes (optional).
3. Compute `avg_speed_kmh = distance_km / (duration_sec / 3600)` server-side; don't ask the user.
4. On submit, call `api.createJob`, redirect to `/jobs`, toast success.
5. Validation: distance > 0, completed_at > started_at, cargo_mass_kg ≥ 0, declared_income ≥ 0.

**Acceptance:** Submitting a valid form creates a job; submitting an invalid one shows per-field errors and doesn't call the backend.

### Task 1.5 — Job history table

**Goal:** `/jobs` shows a paginated, sortable, filterable table.

**Files:** `src/routes/Jobs.tsx`, helper components as needed.

**Steps:**

1. Use shadcn `Table` for the layout, `Input` for the search box, `Select` for status filter, `DateRangePicker` (build from `Calendar` + `Popover`) for date range.
2. Page size 25. Pagination controls below.
3. Columns: completed_at, source → dest (with company in muted text), cargo, distance, on-time pill, income.
4. Row click opens `/jobs/:id`.
5. Hook up to `api.listJobs` with debounced filter changes (300ms).
6. Empty state when no jobs yet: link to "Log your first job manually" pointing at `/jobs/new`.

**Acceptance:** Filters, sort, and pagination all work. Empty state appears on a fresh DB.

### Task 1.6 — Job detail page

**Goal:** `/jobs/:id` shows the full record and a delete button.

**Files:** `src/routes/JobDetail.tsx`.

**Steps:**

1. Fetch via `api.getJob`. 404 state if missing.
2. Render every field. Editable notes field (debounced save through `update_job`).
3. Delete button with confirm dialog. On confirm, calls `api.deleteJob`, redirects to `/jobs` with a toast.

**Acceptance:** Editing notes persists across reload. Deletion removes the row and adjusts the truck's rollup correctly.

### Task 1.7 — Trucks page

**Goal:** `/trucks` shows the list of trucks driven with mileage and trip count.

**Files:** `src/routes/Trucks.tsx`, `commands/trucks.rs` (`list_trucks`, `get_truck`).

**Steps:**

1. `list_trucks` returns all rows sorted by `total_distance_km DESC`.
2. Table columns: make/model, first seen, total jobs, total distance (with delivery / free-roam split underneath in muted text).
3. Row click → `/trucks/:id` (next task).

**Acceptance:** After logging a few manual jobs across two trucks, both appear with correct totals. Free-roam columns stay zero until M2 ships (manual entries don't generate sessions).

### Task 1.8 — M1 wrap-up

**Goal:** End-to-end manual logging flow is solid.

**Steps:**

1. Add 5–10 fake jobs via the form; confirm filters, sort, search, deletion, and truck rollups all behave.
2. Update README with a screenshot of the Jobs page.
3. Tag: `git tag v0.1.0-m1`.

**Acceptance:** Demo flow: open app → log job → see in history → click → edit notes → delete → trucks rollup adjusts.

## M2 — Telemetry reader

**Goal:** Real deliveries completed in ETS2 land in the app automatically.

This is the hardest milestone. Start with Task 2.0 (licensing/plugin sourcing) before touching code.

### Task 2.0 — Bundle Funbit's telemetry plugin DLL

**Goal:** Funbit's `ets2-telemetry.dll` lives in the repo, ready to be shipped inside our installer. The MMF layout is documented for the Rust struct to mirror in Task 2.1.

**Background:** Funbit's plugin is GPL-3, which is fine — ProjectTrucks is GPL-3 too. We're not modifying the plugin; we just bundle it and read its MMF output.

**Steps:**

1. Go to https://github.com/Funbit/ets2-telemetry-server/releases and download the latest stable release archive.
2. Extract `Ets2Telemetry/Plugins/scs-telemetry-input.dll` (or the equivalent path in the current release — the plugin filename has varied) and also the 32-bit and 64-bit plugin DLLs if the release ships both. ETS2's modern builds use win_x64 only, but verify.
3. Commit the DLL(s) into `assets/telemetry-plugin/` in this repo.
4. Write `assets/telemetry-plugin/README.md` documenting: source release tag, download URL, SHA256 of each DLL, and the line "Licensed under GPL-3; see upstream repository for source."
5. Add the upstream link and licence note to ProjectTrucks' main README.
6. Identify the C++ struct that Funbit's plugin writes to its MMF. Look in their repo under `scs-telemetry-input/inc/sharedmemory.hpp` (path may vary by release). Save a copy of that header as a reference inside `assets/telemetry-plugin/sharedmemory.hpp` — we'll mirror its layout in Rust in Task 2.1.
7. Identify the MMF name (Funbit historically uses `Local\\SimTelemetry`). Record it in the README.

**Acceptance:**

* `assets/telemetry-plugin/scs-telemetry-input.dll` (or equivalent) is in the repo.
* `assets/telemetry-plugin/README.md` documents version, SHA256, source URL, GPL-3 notice.
* `assets/telemetry-plugin/sharedmemory.hpp` (or equivalent) is committed for reference.
* MMF name is recorded.

**Note on bundling GPL-3 binaries:** GPL-3 obliges us to provide source for the bundled DLL (or a written offer thereof). The README's link to Funbit's repo satisfies §6 of GPL-3 ("a written offer ... to give anyone who possesses the object code ... a copy of the corresponding Source"). Verify the upstream repo is still accessible at release time and snapshot the source archive if it ever isn't.

### Task 2.1 — Define the shared-memory struct in Rust

**Goal:** A Rust struct that bit-exactly mirrors the C++ struct the plugin writes.

**Files:** `src-tauri/src/telemetry/struct_v.rs`.

**Steps:**

1. Define `#[repr(C)] struct TelemetryFrame { ... }` matching the C++ layout from Task 2.0. Include a `version: u32` header field so we can reject mismatched plugin versions.
2. All strings are fixed-size byte arrays — provide `&str` accessors that trim null terminators.
3. Add `to_snapshot(&self) -> TelemetrySnapshot` that converts to a friendlier Rust-idiomatic struct (Options, Strings, chrono types).
4. Unit-test the conversion with a hand-built byte buffer.

**Acceptance:** Tests pass. The struct's `size_of` matches what the C++ side writes.

### Task 2.2 — Memory-mapped file reader

**Goal:** A `TelemetryReader` that opens the MMF and yields `TelemetrySnapshot` on demand.

**Files:** `src-tauri/src/telemetry/reader.rs`.

**Steps:**

1. Add deps: `memmap2 = "0.9"`, `widestring = "1"` (for Windows wide-string MMF names).
2. `pub struct TelemetryReader { mmap: Mmap }` with `open()` and `read(&self) -> Result<TelemetrySnapshot>`.
3. `open()` retries with backoff for ~5s if the MMF doesn't exist (game not running yet).
4. `read()` does a fresh memcpy from the MMF into a `TelemetryFrame`, then converts.
5. Add a manual integration test guarded by an env var that opens a real MMF (requires the game running). Skip in CI.

**Acceptance:** With ETS2 running and the plugin installed, a Rust binary in `examples/dump_telemetry.rs` prints a live snapshot once per second.

### Task 2.3 — Driving state machine

**Goal:** A pure-Rust state machine that, given a sequence of `TelemetrySnapshot`s, produces driving-session events (`SessionStarted`, `SessionEnded`) and job events (`JobStarted`, `JobCompleted`, `JobCancelled`). Tracks both delivery driving and free-roam driving — every kilometre is accounted for.

**Files:** `src-tauri/src/telemetry/state_machine.rs`.

**States:**

* `NotDriving` — game disconnected, or truck parked + idle.
* `FreeRoam { session_start: SnapshotRef }` — driving with no active job.
* `Delivery { session_start: SnapshotRef, job_data: JobInProgress }` — driving with a job.

**Events emitted:**

* `SessionStarted { kind }`
* `SessionEnded { kind, start_odometer, end_odometer, started_at, ended_at, status }`
* `JobCompleted { job_data, completion_snapshot }` — accompanies a `SessionEnded { kind: JOB }`
* `JobCancelled { job_data }` — accompanies a `SessionEnded { kind: JOB, status: CANCELLED }`

**Transition rules:**

* `NotDriving → FreeRoam`: game connected AND truck odometer has changed in the last sample (truck is moving) AND no trailer with a job attached. Snapshot start odometer + timestamp.
* `FreeRoam → Delivery`: `trailer.attached` flips true AND `job.destinationCity` non-empty. End the free-roam session (emit `SessionEnded` with `kind=FREE_ROAM`), start a delivery session.
* `Delivery → FreeRoam` (delivered): trailer detaches near destination AND in-game money jumped by ≥ declared income within a 30s window. Emit `JobCompleted` + `SessionEnded { kind=JOB, status=COMPLETED }`. Open a new free-roam session immediately.
* `Delivery → FreeRoam` (cancelled): trailer detaches without an income jump AND not near destination. Emit `JobCancelled` + `SessionEnded { kind=JOB, status=CANCELLED }`. New free-roam session opens.
* `FreeRoam | Delivery → NotDriving`: game disconnected for >60s, OR truck idle (speed ~0 and odometer unchanged) for >5 minutes. End the current session.

**Steps:**

1. Implement the state machine with `step(&mut self, snapshot: TelemetrySnapshot) -> Vec<Event>` (a single step may emit multiple events — e.g. `SessionEnded` + `SessionStarted` on a transition).
2. Unit-test with hand-crafted sequences. Cover: pure free-roam play, job picked up cleanly, job cancelled mid-route, ferry use, game alt-tabbed and disconnected for 30s vs 90s, multiple jobs back-to-back, parked overnight in a hotel and resumed.

**Acceptance:** 30+ unit tests passing. Free-roam km between two jobs is captured as its own session.

### Task 2.4 — Background telemetry task

**Goal:** A tokio task that polls the reader, feeds the state machine, and writes both sessions and jobs to SQLite. Emits Tauri events on each event.

**Files:** `src-tauri/src/telemetry/mod.rs`.

**Steps:**

1. Spawn the task in `lib.rs` `setup()` if auto-capture is enabled in settings.
2. Poll loop: every 1s. For each emitted state-machine event:
   * `SessionEnded { kind=FREE_ROAM }` → insert into `driving_sessions`, recompute truck rollup.
   * `JobCompleted` + `SessionEnded { kind=JOB, status=COMPLETED }` → insert session row first, then insert job row referencing it (single transaction), recompute truck rollup.
   * `JobCancelled` + `SessionEnded { kind=JOB, status=CANCELLED }` → insert session row only (no job row), recompute truck rollup.
3. Refactor the core write logic out of the Tauri commands into reusable fns so both telemetry and the manual-entry form go through the same path.
4. Emit Tauri events: `telemetry:status` (connected | not_connected | free_roam | delivery | parked), `session:created`, `job:created`.
5. Frontend listens to `session:created` and `job:created` on Dashboard and Jobs/Trucks pages — refetch.
6. Add a "test mode" that replays a recorded snapshot sequence from a fixture file, so we can exercise the pipeline end-to-end without ETS2 running.

**Acceptance:** With ETS2 running, all km driven land in `driving_sessions`; deliveries also land in `jobs` linked to the right session; truck mileage on the Trucks page matches reality.

### Task 2.5 — Settings page wiring

**Goal:** Toggle auto-capture on/off, show telemetry status, point at ETS2 install path.

**Files:** `src/routes/Settings.tsx`, `commands/settings.rs`.

**Steps:**

1. Settings keys in DB: `units` (`metric` default), `auto_capture` (`false` default), `ets2_install_path` (auto-detected), `units`.
2. Auto-detect ETS2 install path on first launch via the Steam library registry key (`HKLM\\SOFTWARE\\WOW6432Node\\Valve\\Steam` → `InstallPath` → parse `libraryfolders.vdf` → find appid `227300`).
3. Settings UI: toggle for auto-capture (restarts the background task), telemetry status badge (green/yellow/red driven by the `telemetry:status` event), path picker for ETS2 install with "Detect" button, units radio (km/mi).
4. Header pill "Auto-capture: on/off" reflects the setting.

**Acceptance:** Toggling auto-capture stops/starts polling within 2s. Status badge accurately reflects whether the game is running.

### Task 2.6 — One-click plugin installer

**Goal:** A button in Settings that copies the plugin DLL into the game's `bin/win_x64/plugins/` folder, with backup of any existing plugin.

**Files:** `src-tauri/src/telemetry/installer.rs`.

**Steps:**

1. Bundle the plugin DLL into the Tauri resources (`tauri.conf.json` → `bundle.resources`).
2. Tauri command `install_telemetry_plugin(ets2_path: String) -> Result<InstallReport>`:
   * Verify the path exists and looks like an ETS2 install.
   * Ensure `bin/win_x64/plugins/` exists, create if not.
   * If `scs-telemetry-input.dll` (or the bundled filename from Task 2.0) is already there, back it up to `<name>.bak.<timestamp>`. Don't overwrite without backing up — a user may already be running another tool (TrucksBook etc.) that depends on the same plugin.
   * Copy the bundled DLL into place.
   * Return success + path.
3. If the path is in `Program Files`, the operation may require elevation. Either request UAC via Tauri's `windows-sys` helpers, or surface a clear error asking the user to run as admin once.
4. Settings UI: "Install plugin" button, shows status (installed / not installed / outdated based on file hash).

**Acceptance:** Click "Install plugin", restart ETS2, telemetry status flips green.

### Task 2.7 — M2 wrap-up

**Goal:** End-to-end auto-capture demonstrated and resilient.

**Steps:**

1. Run a full session: install plugin, start ETS2, take a job, deliver it, see it auto-logged.
2. Try edge cases: cancel a job, get fined, use a ferry, alt-tab mid-job. Confirm the state machine handles all of these.
3. Add `flagged=true` for anything that looks weird in the sanity checks (avg speed >150, distance <1km).
4. Tag: `git tag v0.2.0-m2`.

**Acceptance:** Three real deliveries done back to back, all logged correctly with no manual fix-up.

## M3 — Dashboards & polish

**Goal:** The app is genuinely useful day-to-day.

### Task 3.1 — Dashboard page

**Goal:** Stat cards + recent jobs + 30-day distance chart + delivery/free-roam split.

**Files:** `src/routes/Dashboard.tsx`, `src/components/StatCard.tsx`, `commands/stats.rs`.

**Steps:**

1. New command `get_dashboard_stats() -> DashboardStats` returning:
   * Lifetime: total km, delivery km, free-roam km, deliveries, income, on-time %.
   * This month: same fields.
   * Last-30-days distance series — per day, split into `delivery_km` and `free_roam_km` for a stacked chart.
   * All-time on-time vs late ratio.
2. Stat cards (shadcn `Card`): big number + label + delta-vs-last-month indicator. Hero card is "Total km driven" with the delivery/free-roam split shown beneath as smaller text.
3. Recent jobs: latest 5 rows.
4. `pnpm add recharts`. Stacked area chart: delivery km + free-roam km per day, last 30 days. Donut for on-time vs late.
5. Listen for both `session:created` and `job:created` and refetch.

**Acceptance:** After driving in free roam (no job), the "Total km" card increases and the chart shows free-roam km on that day. After completing a delivery, delivery km goes up and on-time % updates.

### Task 3.2 — Truck detail page

**Goal:** `/trucks/:id` shows a truck's history and stats.

**Files:** `src/routes/TruckDetail.tsx`.

**Steps:**

1. Stats: total distance (with delivery / free-roam split), total jobs, avg job distance, on-time %.
2. Tabs: "Jobs" (reuse Jobs table component, filtered to this truck) and "All sessions" (chronological list of all driving sessions on this truck, with kind badge).
3. Distance-over-time chart, stacked delivery vs free-roam.

**Acceptance:** Click a truck from `/trucks`, land on a detail page with consistent numbers. Free-roam km accrued in this truck is visible.

### Task 3.3 — Data export + backup

**Goal:** Export jobs to CSV; back up the entire SQLite file to a user-chosen path.

**Files:** `commands/export.rs`, Settings UI buttons.

**Steps:**

1. `export_jobs_csv(path: String) -> Result<usize>` — writes all jobs to CSV at the given path, returns row count.
2. `backup_database(path: String) -> Result<()>` — copies the SQLite file with `VACUUM INTO` for a clean snapshot.
3. Settings UI: two buttons that open native file pickers and call the commands.

**Acceptance:** CSV opens cleanly in Excel; backup file restores correctly when swapped in.

### Task 3.4 — Empty states, loading, errors

**Goal:** No raw "loading..." text anywhere; every page handles empty and error states.

**Steps:**

1. Audit every route. Add shadcn `Skeleton` for loading states. Add empty-state illustrations or terse copy with a clear next action. Add error toasts via `sonner` (`pnpm add sonner`).
2. Add a global error boundary.

**Acceptance:** Fresh-install flow feels intentional: no empty pages, every "what do I do here" question has a visible answer.

### Task 3.5 — M3 wrap-up

**Steps:**

1. Update README screenshots.
2. Tag: `git tag v0.3.0-m3`.

## M4 — Install experience

**Goal:** Anyone can install the app and start tracking in under a minute.

### Task 4.1 — App icons and metadata

**Steps:**

1. Design a simple icon (truck silhouette or "PT" mark). Use [iconkitchen](https://icon.kitchen/) or commission later. Generate `.ico`, `.png` sizes, and `icon.icns`.
2. Drop them in `src-tauri/icons/`.
3. Update `tauri.conf.json`: productName, version, identifier (`co.shaypunter.projecttrucks`), description, publisher.

**Acceptance:** Installer shows ProjectTrucks branding, not Tauri defaults.

### Task 4.2 — Installer

**Steps:**

1. Configure `tauri.conf.json` bundle for `nsis` and `msi` Windows targets.
2. Include the plugin DLL as a bundled resource (already done in M2).
3. Build with `pnpm tauri build`. Confirm a single MSI ships.
4. Test installation on a clean Windows VM — app installs, runs, can install the plugin to a real ETS2 install.

**Acceptance:** Fresh Windows VM goes from MSI → working app in under 60 seconds.

### Task 4.3 — README, screenshots, release notes

**Steps:**

1. Final README: what it is, screenshots, install instructions, plugin install instructions, how to back up your data, how to file an issue.
2. `CHANGELOG.md` for v0.4.0.
3. `git tag v0.4.0-m4` and push.
4. Create a GitHub release with the MSI attached and release notes.

**Acceptance:** Someone landing on the GitHub page can understand the app, download the installer, and use it without asking us anything.

## Working with Claude Code — guidance

* One task per branch, one branch per PR. Don't combine tasks in this plan into a single session unless explicitly trivial.
* If a task's acceptance criteria can't be met, stop and open an issue. Don't paper over it.
* Update this file as you go. Strike tasks as completed. If reality diverges from the plan, edit the plan rather than working around it silently.
* Tests first when the logic is non-trivial. The state machine in M2 especially.
* `cargo clippy -- -D warnings` and `pnpm typecheck` are part of "done".
* Don't add dependencies casually. Each one is a maintenance liability. Especially in Rust.

*Last updated: 2026-05-15.*

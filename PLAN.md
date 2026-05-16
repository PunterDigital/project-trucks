# ProjectTrucks — Planning Doc

A ProjectFly-style personal delivery tracker for **Euro Truck Simulator 2** (and American Truck Simulator). Local install, single user, no cloud, no auth. One installable Windows app that auto-logs your deliveries while you play.

---

## 1. What we're building

A desktop app you install on your own machine. While you're playing ETS2 it reads telemetry in the background, detects when you start and complete deliveries, and writes them to a local SQLite database. The app's UI gives you a stats dashboard, a searchable job history, a per-truck fleet view, and trend charts.

This is a personal tracker first. The data model is shaped so we *could* grow it into a hosted multi-user platform later (VTCs, leaderboards, dispatchers), but the v1 build doesn't optimise for that — it optimises for "install it and it just works."

**Inspirations and prior art**

- *ProjectFly* — the flight-sim equivalent: client app + logbook + passport (country stamps) + achievements.
- *TrucksBook* — the dominant ETS2 platform we're loosely cloning. Worth studying for feature ideas later.
- *Funbit's ets2-telemetry-server* (GPL-3) and the underlying *Funbit/ets2-sdk-plugin* (MIT) — **we use the plugin DLL directly**. Funbit's C++ plugin DLL goes into the game's plugins folder and writes telemetry to a memory-mapped file; our Rust backend reads from that file. The plugin is MIT-licensed (the GPL-3 covers the C# server, which we don't use), so ProjectTrucks itself is free to pick its own licence — we've chosen MIT (see §8).

---

## 2. MVP feature scope

In scope for v1:

1. **Telemetry auto-capture** — detect job start/end while you play and record the delivery without any user action.
2. **All-distance tracking** — record every kilometre driven, whether or not you're on a job. Free-roam driving counts toward lifetime and per-truck mileage, and is reported separately from delivery km on the dashboard.
3. **Manual job entry** — fallback form for jobs the telemetry missed, and useful for testing the data model before the telemetry reader is built.
4. **Job history** — paginated, sortable, filterable table (date range, on-time/late, search).
5. **Driver dashboard** — lifetime + this-month totals (total km split delivery / free-roam, deliveries, income, on-time %), recent jobs, distance-over-time chart.
6. **Fleet view** — list of trucks you've driven with per-truck mileage (delivery + free-roam combined) and trip count.
7. **Settings** — units (km/mi), enable/disable auto-capture, game install path, data export (CSV), one-click DB backup.
8. **One-click telemetry plugin install** — copy Funbit's plugin DLL into the game's `plugins` folder for the user (with UAC prompt where needed).

Explicitly **out of scope** for v1 (deferred until/unless we go multi-user):

- Auth, accounts, anything cloud
- Leaderboards, virtual trucking companies, dispatchers, convoys, events
- Live-map tracking of an in-progress delivery
- Achievement system, country "passport" stamps
- Mobile / web companion
- Auto-update infrastructure (handle this in a v1.x if it matters)

---

## 3. Recommended tech stack

| Layer | Choice | Why |
|---|---|---|
| App shell | **Tauri 2** | Single small Windows installer (~10–20 MB). Uses the system WebView (no Chromium bundled). Rust backend is a natural fit for reading the ETS2 memory-mapped file. |
| UI framework | **Vite + React + TypeScript** | Idiomatic Tauri frontend. No SSR needed for a local app. Fast HMR during dev. |
| Styling/UI | **Tailwind + shadcn/ui** | Polished components out of the box, easy to theme. |
| Charts | **Recharts** | Lightweight React charts, plays well with our data shape. |
| Database | **SQLite via `tauri-plugin-sql`** | Single file, embedded, no server. Trivial to back up (copy one file). |
| Schema/migrations | **Embedded SQL migrations** (or Drizzle for type-safety on the JS side) | Stable, simple, file-versioned. |
| Telemetry reader | **Rust + `memmap2` crate** | Reads the SCS plugin's shared-memory struct natively. No GPL infection, no C# dependency. |
| Telemetry plugin | **Funbit's `ets2-telemetry-server.dll`** (MIT, built from `Funbit/ets2-sdk-plugin`) | Battle-tested community plugin. We bundle it and our installer copies it into the game's `bin/win_x64/plugins/` folder. |
| Build/dist | **Tauri bundler → MSI or NSIS** | Single Windows installer. Optional code signing later. |

**Why not Electron?** Same UX, ~10× the binary size, and we'd still need a separate native helper (or Node native module) to read the memory-mapped file. Tauri is the better tool for a Rust-heavy native integration.

**Why not pure C# WPF?** We *could* go single-language, but the UI side would be slower to iterate on without web tooling (Tailwind, shadcn, hot reload). Splitting Rust (system half) + React (UI half) gives the best of both, and Tauri tightly couples them.

**Why not Next.js (as I'd recommended earlier)?** Next.js shines when you need SSR, API routes, and a hosted multi-user app. In a local Tauri app there's no server to serve — Tauri commands replace API routes, and there's no SEO to worry about. Vite is the simpler, more idiomatic choice for a Tauri frontend.

---

## 4. Architecture

```
   ┌──────────────────────────────────────────────────────────┐
   │  ETS2 (running)                                          │
   │                                                          │
   │     SCS Telemetry SDK plugin DLL  ──►  Memory-Mapped     │
   │     (in game's plugins folder)         File              │
   │                                        Local\SimTelemetryETS2
   └────────────────────────────────────────────┬─────────────┘
                                                │
   ┌────────────────────────────────────────────┼─────────────┐
   │  ProjectTrucks (Tauri app, one process)    │             │
   │                                            ▼             │
   │   ┌───────────────────────────────────────────────┐      │
   │   │ Rust backend                                  │      │
   │   │  - MMF reader (polls ~1s)                     │      │
   │   │  - Job-detection state machine                │      │
   │   │  - SQLite writes (jobs, trucks, settings)     │      │
   │   │  - Exposes Tauri commands to the UI           │      │
   │   └────────────┬──────────────────────────────────┘      │
   │                │ Tauri IPC (commands + events)           │
   │                ▼                                         │
   │   ┌───────────────────────────────────────────────┐      │
   │   │ React UI (in system WebView)                  │      │
   │   │  - Dashboard, history, fleet, settings        │      │
   │   │  - Listens to "job_completed" events          │      │
   │   └───────────────────────────────────────────────┘      │
   │                                                          │
   │   ┌───────────────────────────────────────────────┐      │
   │   │ SQLite file (in app data dir)                 │      │
   │   └───────────────────────────────────────────────┘      │
   └──────────────────────────────────────────────────────────┘
```

**Driving lifecycle in the Rust backend**

We track *all* driving, not just deliveries. The state machine has three driving states (`NotDriving`, `FreeRoam`, `Delivery`) and writes a row to `driving_sessions` whenever a state ends. Job rows in `jobs` are linked back to their underlying `driving_sessions` row via `job_id`.

1. Background async task opens the MMF and polls every ~1 s.
2. **Game connected + truck moves** → enter `FreeRoam`. Snapshot start odometer + timestamp.
3. **Trailer attaches with a valid job** → close the current `FreeRoam` session (write row), enter `Delivery`. Snapshot start odometer + job fields (source/dest, cargo, declared income, deadline).
4. **Trailer detaches near destination + income lands** → close the `Delivery` session and write a `jobs` row pointing at it. Re-enter `FreeRoam`.
5. **Trailer detaches without income** (cancellation) → close the `Delivery` session with `status='CANCELLED'` (no `jobs` row written), return to `FreeRoam`.
6. **Game disconnects, OR truck idle for >5 minutes** → close the current session and enter `NotDriving`.

Every state transition emits a Tauri event so the UI can refresh live. Lifetime km is the sum of `distance_km` across all `driving_sessions`; delivery km is the sum of sessions with `kind = 'JOB'`; free-roam km is the rest.

**Sanity checks (cheap, in-process)**

- Reject jobs with avg speed >150 km/h or distance <1 km — almost certainly an MMF read glitch.
- Reject duplicate jobs (same start timestamp).
- Flag but keep anything that looks odd, so the user can review and delete if needed.

These exist mostly to protect against telemetry weirdness on the user's own machine, not cheating.

---

## 5. Data model (SQLite)

```sql
CREATE TABLE driving_sessions (
  id              TEXT PRIMARY KEY,
  kind            TEXT NOT NULL,         -- 'FREE_ROAM' | 'JOB'
  truck_make      TEXT,
  truck_model     TEXT,
  start_odometer  REAL NOT NULL,
  end_odometer    REAL NOT NULL,
  distance_km     REAL NOT NULL,         -- end_odometer - start_odometer, denormalised for fast sums
  started_at      TEXT NOT NULL,         -- ISO 8601
  ended_at        TEXT NOT NULL,         -- ISO 8601
  status          TEXT NOT NULL          -- 'COMPLETED' | 'CANCELLED' (cancelled only meaningful for JOB sessions)
);
CREATE INDEX idx_sessions_ended_at ON driving_sessions(ended_at);
CREATE INDEX idx_sessions_kind     ON driving_sessions(kind);

CREATE TABLE jobs (
  id              TEXT PRIMARY KEY,
  session_id      TEXT,                  -- FK to driving_sessions.id; nullable for MANUAL entries
  truck_make      TEXT,
  truck_model     TEXT,
  source_city     TEXT NOT NULL,
  source_company  TEXT NOT NULL,
  dest_city       TEXT NOT NULL,
  dest_company    TEXT NOT NULL,
  cargo_id        TEXT NOT NULL,
  cargo_name      TEXT NOT NULL,
  cargo_mass_kg   REAL NOT NULL,
  declared_income INTEGER NOT NULL,      -- game currency
  distance_km     REAL NOT NULL,         -- mirror of session.distance_km for telemetry rows
  duration_sec    INTEGER NOT NULL,
  avg_speed_kmh   REAL NOT NULL,
  fuel_used_l     REAL,
  damage_delta    REAL,                  -- 0..1
  started_at      TEXT NOT NULL,         -- ISO 8601
  completed_at    TEXT NOT NULL,         -- ISO 8601
  status          TEXT NOT NULL,         -- 'DELIVERED' | 'LATE' | 'CANCELLED'
  source          TEXT NOT NULL,         -- 'TELEMETRY' | 'MANUAL'
  flagged         INTEGER NOT NULL DEFAULT 0,
  flag_reason     TEXT,
  notes           TEXT,
  FOREIGN KEY (session_id) REFERENCES driving_sessions(id) ON DELETE SET NULL
);
CREATE INDEX idx_jobs_completed_at ON jobs(completed_at);
CREATE INDEX idx_jobs_status      ON jobs(status);

CREATE TABLE trucks (
  id                       TEXT PRIMARY KEY,    -- hash of (make, model)
  make                     TEXT NOT NULL,
  model                    TEXT NOT NULL,
  first_seen_at            TEXT NOT NULL,
  total_distance_km        REAL NOT NULL DEFAULT 0,   -- all driving
  total_delivery_distance  REAL NOT NULL DEFAULT 0,
  total_free_roam_distance REAL NOT NULL DEFAULT 0,
  total_jobs               INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE settings (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
```

`driving_sessions` is the source of truth for km driven. `jobs` carries the delivery-specific fields and points back to its session row. Truck rollups are updated inside the transaction that closes a session.

No `users` table — single user is implicit.

---

## 6. Telemetry fields we'll use

| SDK field | What we do with it |
|---|---|
| `game.gameName` | Tag job as ETS2 vs ATS |
| `game.connected` | Know the player is actually in-game |
| `truck.make`, `truck.model` | Record the truck used; key into `trucks` table |
| `truck.odometer` | Start/end snapshot → distance driven |
| `truck.speed` | Sanity-check ceiling |
| `truck.fuel`, `truck.fuelAverageConsumption` | Fuel used in this job |
| `truck.wear*`, `trailer.wear` | Damage delta over the job |
| `trailer.attached` | Job start/end signal |
| `trailer.id`, `trailer.name`, `trailer.mass` | Cargo details |
| `job.income`, `job.deadlineTime` | Pay and deadline → on-time vs late |
| `job.sourceCity`, `job.sourceCompany`, `job.destinationCity`, `job.destinationCompany` | Route |
| `truck.placement` | Position (kept around for a future live-map feature) |

Note: the SDK does not emit explicit "job completed" events — completion has to be inferred from `trailer.attached` flipping false plus income landing. Same pattern other trackers use.

---

## 7. MVP roadmap

Approximate timeline assumes part-time evenings/weekends.

**M0 — Tauri scaffold (week 1)**

- `cargo create-tauri-app` with Vite + React + TypeScript template
- Tailwind + shadcn/ui set up
- `tauri-plugin-sql` wired up with initial migration (jobs / trucks / settings tables)
- App shell: sidebar nav (Dashboard, Jobs, Trucks, Settings)
- *Exit:* app boots, empty pages render, SQLite file is created on first run

**M1 — Job CRUD + manual entry (week 2)**

- Tauri commands: `create_job`, `list_jobs`, `delete_job`, `update_job`
- Manual job entry form (every field that telemetry will eventually fill)
- Job history table with sort, filter, pagination, search
- Job detail view
- Auto-update `trucks` rollup on insert/delete
- *Exit:* I can manually log a job and see it in history; trucks page populates

**M2 — Telemetry reader (weeks 3–4)**

- Rust struct mirroring Funbit plugin's MMF layout (read `ets2-telemetry-common` C++ headers in their repo)
- `memmap2`-based reader, async background task
- Job-detection state machine (idle → started → in-progress → completed/cancelled)
- Tauri event emission on completion → UI refresh
- Settings UI: enable/disable auto-capture, choose game install path, test connection
- One-click "Install telemetry plugin" button that copies Funbit's `ets2-telemetry.dll` into `bin/win_x64/plugins/` (handles UAC, backs up any existing plugin first)
- *Exit:* drive a real delivery in ETS2 → it appears in history without me touching anything

**M3 — Dashboards & polish (week 5)**

- Dashboard page: stat cards (lifetime + this-month), recent-jobs strip, 30-day distance chart, on-time % chart
- Trucks page: detail view with per-truck job list
- Settings: units (km/mi), CSV export, "Back up database" (copy file to user-chosen path)
- Empty states, loading skeletons, error toasts
- *Exit:* genuinely useful for me day-to-day

**M4 — Install experience (week 6)**

- Tauri MSI/NSIS installer with proper icons, start-menu entry, optional auto-start
- README with screenshots
- (Optional) Windows code-signing cert (~$80–200/yr) to skip SmartScreen warning
- *Exit:* I can hand a friend the installer and they're tracking deliveries in <60 s

---

## 8. Risks & open decisions

**Risks**

- **Plugin DLL install needs write access to the game folder.** The installer/UI has to handle UAC and the case where the user installed ETS2 to a non-standard path. Steam install path is usually discoverable from the registry.
- **Struct layout drift.** SCS occasionally bumps the telemetry struct version. We need to read the header version and degrade gracefully (warn instead of crash) when we see a newer version we don't know.
- **Unsigned installer.** Trips SmartScreen for new users. Tolerable for personal use; cert money if/when we share it.

**Decisions locked in** (2026-05-15):

1. **Brand name:** ProjectTrucks.
2. **Games supported:** ETS2 only. ATS is explicitly out.
3. **Licence:** MIT, open source from day one. (The plan originally assumed Funbit's plugin was GPL-3 and that pulled us along; on closer inspection the plugin DLL is built from MIT-licensed `Funbit/ets2-sdk-plugin`, so we picked MIT for ourselves.)
4. **Updates:** ship manually. No auto-update infra in MVP.
5. **Multi-user upgrade path:** not preserved. Build for single-user-local and don't over-engineer for a hypothetical hosted version.

---

## 9. What success looks like

Three things, in order:

1. A real delivery I complete in ETS2 lands in the app's history automatically, with correct route, cargo, distance, on-time status.
2. After a month of playing, the dashboard is genuinely interesting to look at and tells me something I didn't already know.
3. The installer puts a friend up and running with their own local copy in under a minute.

---

*Last updated: 2026-05-15. Author: Shay (with planning support from Claude).*

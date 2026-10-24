# Changelog

All notable changes to ProjectTrucks. Versions are semver tags; each one has a
[GitHub Release](https://github.com/PunterDigital/project-trucks/releases) with the
installers attached. Pre-1.0: features bump the minor version, fixes the patch.

## v0.5.0 — 2026-10-24

Milestone M4 (polish and release). This completes the original roadmap (M1–M4).

### Features
- A ProjectTrucks app icon (truck silhouette) on the exe, the taskbar, the Start menu and both
  installers, replacing Tauri's scaffold icon. The NSIS installer now carries it too, instead of
  NSIS's default. ([#39](https://github.com/PunterDigital/project-trucks/pull/39))
- Installer metadata: publisher, description and copyright in the exe's version resource and in
  Add/Remove Programs. ([#39](https://github.com/PunterDigital/project-trucks/pull/39))
- README install guide (SmartScreen, telemetry plugin, backup and restore, reporting a problem) and
  this changelog. ([#40](https://github.com/PunterDigital/project-trucks/pull/40))

### Internal
- CI fails if a build ever ships without the app icon (`scripts/check-win-icon.ps1`).

## v0.4.0 — 2026-10-24

Milestone M3 (dashboard, fleet, your data, polish).

### Features
- **Dashboard**: lifetime and this-month kilometres (delivery vs free roam), deliveries, income and
  on-time rate with month-on-month deltas, a 30-day distance chart, an on-time donut and your most
  recent jobs. ([#34](https://github.com/PunterDigital/project-trucks/pull/34))
- **Truck pages**: every truck in your fleet gets its own page with stats, a 30-day chart and its
  jobs and driving sessions. ([#35](https://github.com/PunterDigital/project-trucks/pull/35))
- **Export and backup**: export every job to CSV, or save a snapshot of the database, from
  Settings → Your data. ([#36](https://github.com/PunterDigital/project-trucks/pull/36))
- Skeleton loading on every page, empty states that say what to do next, a "Try again" on failed
  loads and a reload screen if the app ever hits a rendering error.
  ([#37](https://github.com/PunterDigital/project-trucks/pull/37))
- README screenshots and status. ([#38](https://github.com/PunterDigital/project-trucks/pull/38))

### Internal
- Tauri npm packages and crates bumped together; TypeScript 6, sqlx 0.9, sha2 0.11, GitHub Actions
  majors. ([#31](https://github.com/PunterDigital/project-trucks/pull/31),
  [#33](https://github.com/PunterDigital/project-trucks/pull/33))

## v0.3.0 — 2026-10-18

### Features
- On first launch, a banner offers to install (or update) the ETS2 telemetry plugin into your saved
  ETS2 folder in one click. "Not now" hides it until a new plugin version ships.
  ([#28](https://github.com/PunterDigital/project-trucks/pull/28))

## v0.2.0 — 2026-10-17

Milestone M2 (live capture from ETS2). ProjectTrucks is Windows-only from this release: the
telemetry plugin only exists for Windows, and ETS2 telemetry is the point of the app.

### Features
- **Auto-capture**: with the bundled telemetry plugin installed and auto-capture switched on, every
  delivery and every free-roam stretch is recorded while you play, with distance, duration, truck,
  cargo, income and the on-time / late / cancelled outcome.
  ([#20](https://github.com/PunterDigital/project-trucks/pull/20),
  [#21](https://github.com/PunterDigital/project-trucks/pull/21),
  [#22](https://github.com/PunterDigital/project-trucks/pull/22),
  [#23](https://github.com/PunterDigital/project-trucks/pull/23))
- Settings page: auto-capture on/off, the ETS2 folder (found through Steam by default) and km/mi
  units. ([#24](https://github.com/PunterDigital/project-trucks/pull/24))
- One-click telemetry plugin installer from Settings.
  ([#25](https://github.com/PunterDigital/project-trucks/pull/25))
- Suspicious recorded jobs (average speed over 150 km/h, under 1 km) are flagged for you to check.
  ([#26](https://github.com/PunterDigital/project-trucks/pull/26))

### Fixes
- Manually logged jobs now count towards a truck's distance.
  ([#16](https://github.com/PunterDigital/project-trucks/pull/16))
- A Content Security Policy for the app window.
  ([#17](https://github.com/PunterDigital/project-trucks/pull/17))
- Patched dependency advisories. ([#14](https://github.com/PunterDigital/project-trucks/pull/14))

### Removed
- The macOS build (manual logging only; it stays available on v0.1.0).
  ([#19](https://github.com/PunterDigital/project-trucks/pull/19))

## v0.1.0 — 2026-10-14

Milestone M1 (manual logging). First release.

### Features
- Log a delivery by hand: route, cargo, distance, duration, truck, status, fuel, damage and notes,
  with validation.
- Job history with search across cities, companies and cargo, a status filter, a date range and
  pagination.
- A job page with auto-saving notes and flags, and delete with confirmation.
- A Trucks page listing every truck you've driven, with trip counts.
- CI, release pipeline and Playwright E2E against the real app.
  ([#1](https://github.com/PunterDigital/project-trucks/pull/1))

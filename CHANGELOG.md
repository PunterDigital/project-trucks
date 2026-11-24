# Changelog

All notable changes to ProjectTrucks. Versions are semver tags; each one has a
[GitHub Release](https://github.com/PunterDigital/project-trucks/releases) with the
installers attached. Pre-1.0: features bump the minor version, fixes the patch.

## v0.13.0 — 2026-11-24

Security hardening from an audit, plus a smaller installer and a lighter Dashboard while you drive. Nothing changes in
how you use the app, apart from where updates wait to install.

### Performance
- **Smaller installer.** The app is built more compactly: the installer every one-click update downloads is about 7%
  smaller and the app itself about 13% smaller. ([#102](https://github.com/PunterDigital/project-trucks/pull/102))
- **Lighter Dashboard while driving.** The job-in-progress card updates every second on its own, without redrawing the
  rest of the Dashboard and its charts each time. ([#102](https://github.com/PunterDigital/project-trucks/pull/102))
- **Faster truck pages.** New database indexes for the truck page and truck filters.
  ([#102](https://github.com/PunterDigital/project-trucks/pull/102))

### Fixes
- **Hub links only open Hub pages.** "Link account" and the other Hub buttons only open an https page on the Hub's own
  address. Anything else the Hub might send back is refused with an error instead of being opened.
  ([#100](https://github.com/PunterDigital/project-trucks/pull/100))
- **Export and backup choose the file themselves.** The save dialog for **Export jobs (CSV)** and **Back up database**
  now opens from the app's core, which only writes `.csv` and `.db` files. A backup only replaces an older backup or an
  empty file (never the app's own database), and a failed backup no longer deletes the previous one.
  ([#100](https://github.com/PunterDigital/project-trucks/pull/100))
- **Stricter updates.** An update is only downloaded from this project's GitHub releases and only installed when GitHub
  lists its checksum; the file is checked again right before it runs. Downloaded updates now wait in
  `%LOCALAPPDATA%\co.shaypunter.projecttrucks\update` instead of the temp folder, and an update waiting for the next
  launch only installs if the release still lists that same file.
  ([#101](https://github.com/PunterDigital/project-trucks/pull/101))
- **Installers have no test hooks.** The settings used by the automated tests (a debugging port, replayed drives, a
  fake update feed) are no longer in the released app. ([#101](https://github.com/PunterDigital/project-trucks/pull/101))
- **Safer CSV export.** Text cells that start with `=`, `+`, `-` or `@` get a leading `'`, so a spreadsheet can't run
  them as formulas. ([#101](https://github.com/PunterDigital/project-trucks/pull/101))
- **Plugin status after an update.** Settings → Telemetry plugin no longer gets stuck on "Checking…" when you click
  Install or Update right after changing the ETS2 folder. ([#102](https://github.com/PunterDigital/project-trucks/pull/102))

## v0.12.0 — 2026-11-23

### Features
- **Dark theme.** Settings → Theme has Light, Dark and System (the default, which follows Windows' app mode and keeps
  following it while the app is open). The window opens in the right colours with no white flash, and the charts and
  pop-up messages are readable in the dark theme. ([#85](https://github.com/PunterDigital/project-trucks/pull/85))
- **Install updates automatically.** A new setting in Settings → Updates (off by default). When it's on, a new version
  downloads in the background and installs when you close ProjectTrucks, which then reopens on the new version. It
  never installs while ETS2 is running or a job is being recorded; it waits for the next launch instead. **Install
  now** in the banner does it straight away. ([#87](https://github.com/PunterDigital/project-trucks/pull/87),
  [#88](https://github.com/PunterDigital/project-trucks/pull/88))
- **What the game says about your jobs.** With the scs-telemetry plugin, deliveries and cancellations now come
  straight from the game, so a job finishes even if you quit from the results screen. Each job records what the game
  paid, the XP, cargo damage, the distance the game counted, auto-park/auto-load and any cancellation penalty. Tolls,
  fines and ferry or train fares are listed under **Costs** on the job page, and the Dashboard's income cards show
  what you spent on them. The CSV export has eight new columns at the end.
  ([#89](https://github.com/PunterDigital/project-trucks/pull/89))
- **Those figures reach the Hub.** Linked apps now send the game's revenue, XP, cargo damage, cancellation penalty and
  the job's total costs to ProjectTrucks Hub. ([#97](https://github.com/PunterDigital/project-trucks/pull/97))

### Fixes
- **Easier-to-read dashboard charts.** Free roam (distance chart) and Late (on-time chart) use a darker colour, the
  tooltip and legend text are readable in both themes, and long distances on the axis no longer wrap.
  ([#95](https://github.com/PunterDigital/project-trucks/pull/95))
- **Settings changes no longer flip back.** Changing two settings quickly one after the other could show the first
  one's old value (for example Kilometres after you picked Miles), though both were saved.
  ([#96](https://github.com/PunterDigital/project-trucks/pull/96))

### Maintenance
- Dependency updates, Tauri 2.12.7 included. ([#94](https://github.com/PunterDigital/project-trucks/pull/94), [#98](https://github.com/PunterDigital/project-trucks/pull/98))
- The app's pages are loaded as separate chunks instead of one 1.1 MB bundle, and the build prints no warnings.
  ([#92](https://github.com/PunterDigital/project-trucks/pull/92))
- The end-to-end tests run in parallel. ([#86](https://github.com/PunterDigital/project-trucks/pull/86))

## v0.11.0 — 2026-11-16

### Features
- **Hub status inside the app.** Once the app is linked to ProjectTrucks Hub, the Jobs list shows a badge on each
  recorded job (on the Hub, syncing, or refused, with the reason on hover), and the job page has a "ProjectTrucks Hub"
  section with **Open on the Hub** and a **Hide on Hub** checkbox. A pill in the header shows your Hub username,
  how many jobs are still syncing or were refused, or that the Hub is offline; it opens Settings → Hub, which now
  also shows your company. ([#82](https://github.com/PunterDigital/project-trucks/pull/82))
- **Fuel used and damage are recorded.** Recorded jobs now fill in the fuel burned and the damage picked up on the
  way (truck and trailer). Refuels and repairs during the job don't cancel them out.
  ([#81](https://github.com/PunterDigital/project-trucks/pull/81))
- **In-game time on the job page.** Each recorded job shows how long it took on the game's clock.
  ([#81](https://github.com/PunterDigital/project-trucks/pull/81))

### Fixes
- **Average speed is no longer hundreds of km/h.** The game's kilometres pass much faster than real time, so
  dividing them by real minutes gave averages like 1,004 km/h and flagged nearly every delivery. The average is now
  per in-game hour, and jobs that were flagged only for speed are un-flagged after updating. Jobs recorded before
  this version have no in-game time, so their average shows "—".
  ([#81](https://github.com/PunterDigital/project-trucks/pull/81))
- **Jobs you add by hand stay on this PC.** Manually logged jobs are no longer sent to the Hub, and ones that were
  already there are removed from it. Recorded jobs sync as before.
  ([#80](https://github.com/PunterDigital/project-trucks/pull/80))

### Maintenance
- Weekly dependency updates. ([#83](https://github.com/PunterDigital/project-trucks/pull/83))

## v0.10.0 — 2026-11-15

### Features
- **A newer telemetry plugin.** ProjectTrucks now installs RenCloud's `scs-telemetry.dll` (v1.12.1, MIT), the
  plugin most ETS2 trackers use, in place of the 2016 plugin it shipped before. After updating, the app offers
  **Update** in the plugin prompt (or Settings → Telemetry plugin): that installs the new plugin and removes the old
  `ets2-telemetry-server.dll` it put there. Other trackers' plugins are left alone. Until you press it, recording
  keeps working through the old plugin. Checked against a real delivery recorded from the game.
  ([#78](https://github.com/PunterDigital/project-trucks/pull/78))
- **Your truck's real model name.** The new plugin reports the model the game shows (e.g. "Volvo FH4" instead of
  "Volvo STD"), so jobs from before and after this update appear as two trucks on the Trucks page.

### Fixes
- **Jobs without a deadline are no longer "Late".** External-market jobs have no delivery deadline, but the old
  plugin's value for "none" read as a date in the past, so every such delivery was recorded as late. They now never
  count as late. ([#78](https://github.com/PunterDigital/project-trucks/pull/78))

## v0.9.2 — 2026-11-14

### Fixes
- **Restarting mid-delivery keeps the job.** Closing ProjectTrucks (or updating it) during a delivery used to start
  the job over from where you were when it reopened, so the finished job was missing everything driven before.
  The job in progress is now saved as you go and picked up again when the app starts, as long as the game still
  has the same job. ([#76](https://github.com/PunterDigital/project-trucks/pull/76))

## v0.9.1 — 2026-11-14

### Fixes
- **Deliveries with your own trailer are recorded again.** The telemetry plugin only clears a job when the trailer
  is unhitched, so with an owned trailer (which stays hitched after the drop-off) a finished delivery never showed
  up in Jobs and the app kept treating it as in progress. A job now counts as delivered when the route advisor was
  nearly at the drop-off and then clears, and the finished job isn't picked up again while you drive on.
  ([#74](https://github.com/PunterDigital/project-trucks/pull/74))

## v0.9.0 — 2026-11-14

### Features
- **Your truck on the Hub's live map.** While the app is linked to the ProjectTrucks Hub, the live status now
  includes where your truck is in the game world and which way it's heading, on a job and while
  free-roaming. The Hub's new Live map (https://project-trucks.shaypunter.co.uk/map) shows it with your truck
  and current job. Don't want to be on it? Untick "Show me on the live map" in the Hub's Settings → Profile and
  the Hub stops storing your position; private profiles never appear.
  ([#71](https://github.com/PunterDigital/project-trucks/pull/71))

## v0.8.0 — 2026-11-14

### Features
- **ProjectTrucks Hub.** Link the app to an account on the ProjectTrucks Hub
  (https://project-trucks.shaypunter.co.uk) in Settings → ProjectTrucks Hub: click Link account, approve the
  code on the website, and your new jobs sync to your public driver profile by themselves (edits and deletes
  follow). Jobs from before linking only upload if you click Sync history, and job notes never leave your PC.
  The sign-in for the Hub is kept in Windows Credential Manager. Unlink any time, in the app or on the website.
  ([#69](https://github.com/PunterDigital/project-trucks/pull/69))
- **On the road, live.** While linked, the Hub shows what you're driving right now (truck, cargo, route and
  progress) on your profile and your company's page, and stops when you close the game or turn auto-capture
  off. ([#69](https://github.com/PunterDigital/project-trucks/pull/69))

## v0.7.0 — 2026-11-14

### Features
- **Job in progress on the Dashboard.** While you're on a delivery, the Dashboard shows what you're hauling,
  from where to where, the pay, the distance and (game) time left from the route advisor, how long until the
  deadline, and a completion bar. It updates live and goes away once the job is delivered or cancelled.
  ([#66](https://github.com/PunterDigital/project-trucks/pull/66))
- **Live telemetry indicator.** The header pill now shows whether the app is actually tracking: Tracking
  (green), Waiting for ETS2 (auto-capture on, game not connected) or Not tracking (auto-capture off). Hover it
  to see what's being recorded; click it to open Settings.
  ([#67](https://github.com/PunterDigital/project-trucks/pull/67))

## v0.6.4 — 2026-11-14

### Fixes
- **Cancelled jobs now show in Jobs.** A job you cancel in the game is recorded as a Jobs row marked
  Cancelled. Before, only its kilometres were kept, so the cancel didn't appear anywhere in the list. Cancelled
  jobs don't count as deliveries or income and are never marked Flagged. Cancels recorded before this
  version can't be added after the fact. ([#64](https://github.com/PunterDigital/project-trucks/pull/64))

## v0.6.3 — 2026-11-14

### Fixes
- **Auto-capture now works with the real game.** The telemetry plugin ProjectTrucks installs reports
  itself as revision 9, and the app only accepted revision 10, so it never connected while ETS2 was
  running. Both are accepted now (same data layout). ([#62](https://github.com/PunterDigital/project-trucks/pull/62))

## v0.6.2 — 2026-11-14

### Fixes
- **The app now opens with a database made by an earlier build.** If the first migration's
  line endings differed between the build that created your database and the one you
  installed, ProjectTrucks closed during startup without ever showing a window. It now
  repairs the recorded checksum and starts normally; if startup still fails for another
  reason, an error box says why. ([#58](https://github.com/PunterDigital/project-trucks/pull/58))

### Internal
- Build tooling: @vitejs/plugin-react 6. ([#52](https://github.com/PunterDigital/project-trucks/pull/52))

## v0.6.1 — 2026-11-09

Maintenance release: no new features or behaviour changes.

### Internal
- Build tooling moves to Vite 8, and form validation to @hookform/resolvers 6 (the job form's
  checks work as before). ([#50](https://github.com/PunterDigital/project-trucks/pull/50))
- Dependency updates: lucide-react 1.66, eslint 10.14.
  ([#51](https://github.com/PunterDigital/project-trucks/pull/51))

## v0.6.0 — 2026-11-02

### Features
- **One-click app updates**: the app checks GitHub for a newer release when it starts (Settings >
  Updates, on by default) or when you click "Check now". When one is out, a banner offers
  "Update to vX.Y.Z": the app downloads the installer, checks its size and SHA-256 against
  GitHub's, runs it in passive mode and restarts on the new version. Updating from v0.5.0 to
  v0.6.0 is still a manual install; later versions can update themselves.
  ([#42](https://github.com/PunterDigital/project-trucks/pull/42))

### Fixes
- Security advisories in build-time tools (body-parser, undici via the shadcn CLI); nothing in the
  app itself was affected. ([#41](https://github.com/PunterDigital/project-trucks/pull/41))

### Internal
- Dependency updates: Tauri 2.12.5 (crates and npm packages together), tokio, uuid, reqwest,
  lucide-react, react-hook-form, react-router-dom, shadcn, typescript-eslint.
  ([#43](https://github.com/PunterDigital/project-trucks/pull/43),
  [#47](https://github.com/PunterDigital/project-trucks/pull/47),
  [#48](https://github.com/PunterDigital/project-trucks/pull/48))

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

# ProjectTrucks

A local Euro Truck Simulator 2 delivery tracker. Runs on your own machine, reads
telemetry while you play, and writes every kilometre and every job to a local
SQLite database. No accounts, no cloud, no telemetry-back-to-us.

Status: **work in progress**. The architectural plan is in [PLAN.md](PLAN.md);
the milestone-by-milestone implementation plan is in
[CLAUDE_CODE_PLAN.md](CLAUDE_CODE_PLAN.md). M0 is complete — the app boots, the
sidebar nav works, and a local SQLite database is created on first launch.

Licensed under **GPL-3** (see [LICENSE](LICENSE)). Required by our use of
Funbit's GPL-3 telemetry plugin — we'll bundle that DLL in M2.

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
  components/       UI primitives (shadcn) + app components (Sidebar, Header)
  routes/           One file per top-level page
  lib/              utils, formatters, Tauri command wrappers
src-tauri/          Rust backend
  migrations/       SQL migrations applied by sqlx at startup
  src/db/           Connection pool + AppState
  src/commands/     Tauri commands invoked from the frontend
```

## Useful scripts

```sh
pnpm dev             # vite dev server only (no Tauri window)
pnpm build           # tsc + vite build (frontend bundle)
pnpm tauri dev       # full app
pnpm tauri build     # release installer

cd src-tauri && cargo test    # Rust unit tests
cd src-tauri && cargo clippy  # Rust lints
```

## Contributing

This is a personal project, but the code is GPL-3 — feel free to fork. There's
no contribution process yet.

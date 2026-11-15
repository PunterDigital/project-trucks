# Bundled telemetry plugin: RenCloud scs-telemetry (provenance)

Since #4619 ProjectTrucks ships RenCloud's `scs-telemetry.dll`, the plugin most ETS2/ATS
trackers use (Trucky and others), in place of Funbit's 2016 `ets2-telemetry-server.dll`
(still kept one folder up, for provenance and because the app still reads its frame on
installs that haven't updated).

The installer copies `win_x64/scs-telemetry.dll` into `<ets2>/bin/win_x64/plugins/`. The game
loads it and the plugin writes a 32 KB frame to the memory-mapped file `Local\SCSTelemetry`
every tick, which `src-tauri/src/telemetry/struct_scs.rs` decodes.

## Upstream

- Repo: https://github.com/RenCloud/scs-sdk-plugin (MIT, `LICENSE-upstream.txt`)
- Release: `V.1.12.1` (2023-09-21), asset `release_v_1_12_1.zip`
- File: `release_v_1_12_1/Win64/scs-telemetry.dll` from that zip
- Plugin revision it writes: 12 (`PLUGIN_REVID` in the header)
- The DLL has no version resource, so the installer can't compare versions: a different
  `scs-telemetry.dll` already in the game folder is replaced and kept as `scs-telemetry.dll.bak`.

The same DLL (byte for byte) was already in Shay's ETS2 plugins folder on 2026-11-15.

## Files

| Path | What |
|---|---|
| `win_x64/scs-telemetry.dll` | The plugin for the 64-bit game |
| `headers/scs-telemetry-common.hpp` | `scsTelemetryMap_t` at `V.1.12.1`: the layout the Rust decoder follows (zone offsets in its comments) |
| `LICENSE-upstream.txt` | MIT licence from the upstream repo, verbatim |

## SHA-256 fingerprints

```
1d03dbc7a975e72203c60a7b9998021ceb8800b836bf28a131279979ad386cd4  win_x64/scs-telemetry.dll
8e195082bde8876aa5906da8f9bea835bdd7440ee444c308887be52a0f35fbd4  headers/scs-telemetry-common.hpp
3679ec81407de0ebfb45241e496e2ad31e30a6e409ef0246fc86195bfee9c9a5  release_v_1_12_1.zip (upstream, not stored here)
```

## What the decoder relies on

- `sdkActive` (offset 0) and `telemetry_plugin_revision` (40) gate every read: inactive or a
  revision other than 12 is refused.
- `special_b.onJob` (4300) says whether there is a job. The plugin sets it from the game's job
  config and clears it on the delivered/cancelled gameplay events, but leaves the last job's
  strings and income in the frame, so a job is read only while `onJob` is set.
- `paused` (4) comes from the SDK's paused/started events; the plugin starts paused.
- `truck_f.routeDistance` (m) / `routeTime` (game s) are the route advisor, like Funbit's rev4
  `navigation_distance` / `navigation_time`.
- Trailer 0's `attached` (6080) is the hitch state.

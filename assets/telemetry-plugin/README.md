# Bundled telemetry plugin (provenance)

ProjectTrucks ships a prebuilt copy of Funbit's ETS2 telemetry plugin DLL.
When the M2 installer copies that DLL into the user's ETS2 install at
`bin/win_x64/plugins/ets2-telemetry-server.dll`, the game's `scs_sdk_telemetry`
hook loads it and starts writing a 16 KB telemetry frame to a Windows
memory-mapped file every tick. Our Rust backend reads from that MMF.

This directory keeps everything needed to:

1. **Ship the DLL** (under `win_x64/` and `win_x86/`).
2. **Mirror the binary's struct layout in Rust** without guessing — the C++
   source headers that defined the struct live under `headers/`.
3. **Honour the upstream licence** — the plugin is MIT; the copyright +
   permission notice is in `LICENSE-upstream.txt` and reproduced below.
4. **Survive upstream going dark** — `source-snapshot/` carries a full copy
   of the source archive that produced the DLL, so we don't depend on
   GitHub being around forever.

## Files in this directory

| Path | What | Size |
|---|---|---|
| `win_x64/ets2-telemetry-server.dll` | Plugin DLL for the 64-bit game (the only thing modern ETS2 ships) | 93,696 B |
| `win_x86/ets2-telemetry-server.dll` | 32-bit fallback (kept in case anyone runs a really old install) | 79,872 B |
| `headers/sharedmemory.hpp` | C++ class wrapping the MMF handle | 981 B |
| `headers/sharedmemory.cpp` | MMF open/close impl | 2,263 B |
| `headers/ets2-telemetry-common.hpp` | **The big one.** `ets2TelemetryMap_s` C++ struct — this is what we mirror in Rust in Task 2.1. Defines MMF name, size, and the full per-tick struct. | 5,954 B |
| `headers/ets2-telemetry.cpp` | Plugin main; the code that fills the struct each tick | 20,964 B |
| `source-snapshot/ets2-sdk-plugin-rev10.zip` | Full source archive (Funbit/ets2-sdk-plugin @ rev10) — kept so the provenance trail survives even if upstream disappears | 64,689 B |
| `source-snapshot/Telemetry.md` | Field-by-field documentation from the telemetry-server distribution. Useful when implementing the Rust mirror. | 20,092 B |
| `LICENSE-upstream.txt` | The MIT licence from Funbit/ets2-sdk-plugin, included verbatim | 1,071 B |

## Upstream sources

The DLLs and the C++ source come from **two different Funbit repositories**,
which surprised us during M2 prep. The plan originally assumed a single
GPL-3 repo; in reality:

### DLL binaries — `Funbit/ets2-telemetry-server` @ `3.2.5`

The C# telemetry web server ships the prebuilt plugin DLL alongside its
server code. We extract just the plugin, not the rest of the server.

- Repo: https://github.com/Funbit/ets2-telemetry-server
- Tag: `3.2.5`
- Source archive: <https://github.com/Funbit/ets2-telemetry-server/archive/refs/tags/3.2.5.zip>
- File extracted: `server/Ets2Plugins/win_x64/plugins/ets2-telemetry-server.dll`
  (and the matching `win_x86` build)
- DLL date stamp: 2016-03-28 14:47 (matches the rev9 source commit
  `02babc04` from the same morning)

The repo itself is **GPL-3** (covering the C# server). The plugin DLL
inside is **MIT** because it's built from the sister repo below; bundling
the DLL doesn't infect us with copyleft.

### DLL source — `Funbit/ets2-sdk-plugin` @ `rev10`

The C++ plugin source, in a separate repo, MIT-licensed (forked from
[`nlhans/ets2-sdk-plugin`](https://github.com/nlhans/ets2-sdk-plugin),
also MIT).

- Repo: https://github.com/Funbit/ets2-sdk-plugin
- Tag: `rev10`
- Source archive: <https://github.com/Funbit/ets2-sdk-plugin/archive/refs/tags/rev10.zip>
- Commit: `7525c245c408f75c2bbfca5d6629cbefe3819285`
- Date: 2016-06-19

The DLLs in telemetry-server@3.2.5 were built from the `02babc04` commit
("Revision 9", 2016-03-28). `rev10` is the next tag and adds only a
`.gitignore` line — the C++ source between rev9 and rev10 is **identical**,
which we verified by inspecting the diff. We snapshot `rev10` because it
has a proper tag; rev9's commit-archive URL no longer resolves.

## SHA-256 fingerprints

Verify a downloaded copy of this repo against these. If any of them don't
match, somebody has tampered with the bundled binaries.

```
9923e368d5e1abb19f67fbc560f6213433ebc51d664b6227785486f6bc36c160  win_x64/ets2-telemetry-server.dll
60828a1bc29f8c018e453e24b8b02c39234715b5abdb428f8ed0d04e41d7ab5f  win_x86/ets2-telemetry-server.dll
a75f96ba28fc93042b8a1f1891569fa9d03f2ed3fe24916157792cf460e06c37  source-snapshot/ets2-sdk-plugin-rev10.zip
381f80cc41d0b1ce54121cbb43e7193514b2a3480e62608d214eef027462d323  source-snapshot/Telemetry.md
8f33a6ac809db78bd57ae5b5e0bf82243f3fc8b668b62b24dfa0c31e6f97322b  headers/sharedmemory.hpp
6ac89f488b4efa4d82eab79e8ae59a0824dbb8b6d14d18f642ee3d16939ed971  headers/ets2-telemetry-common.hpp
```

## MMF layout — what Task 2.1 needs to mirror

These constants come straight out of `headers/ets2-telemetry-common.hpp`
and `headers/sharedmemory.cpp`. They drive the Rust struct in Task 2.1.

| What | Value |
|---|---|
| MMF name | `Local\Ets2TelemetryServer` |
| MMF size | `16 * 1024` (16 KB) |
| Plugin revision baked in | `ETS2_PLUGIN_REVID = 10` (lives at offset of `tel_revId.ets2_telemetry_plugin_revision`) |
| Truck strings offset | `15 * 1024` (so the last 1 KB of the buffer holds fixed-size strings) |
| Trailer strings offset | `15 * 1024 + 64` |
| String size | 64 bytes, null-padded |

Note: an earlier (`rev3`) version of this plugin used the MMF name
`Local\SimTelemetry`. From `rev4` onward Funbit renamed it to
`Local\Ets2TelemetryServer`. Older trackers like TrucksBook still
occasionally publish guides referencing the old name; ignore those.

The big struct is `ets2TelemetryMap_s` in `headers/ets2-telemetry-common.hpp`.
It's versioned: `tel_revId.ets2_telemetry_plugin_revision` is the first
field after `time`/`paused`, so Task 2.1 can read just that prefix first
and bail with a friendly error if it sees a revision we don't know.

## Upstream MIT licence (verbatim)

> The MIT License (MIT)
>
> Copyright (c) 2014 Hans
>
> Permission is hereby granted, free of charge, to any person obtaining a copy
> of this software and associated documentation files (the "Software"), to deal
> in the Software without restriction, including without limitation the rights
> to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
> copies of the Software, and to permit persons to whom the Software is
> furnished to do so, subject to the following conditions:
>
> The above copyright notice and this permission notice shall be included in
> all copies or substantial portions of the Software.
>
> THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
> IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
> FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
> AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
> LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
> FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS
> IN THE SOFTWARE.

The full notice is also kept verbatim in `LICENSE-upstream.txt`.

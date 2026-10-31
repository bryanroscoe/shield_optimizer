# ATV Optimizer — developer guide

This is the source for **ATV Optimizer**, the official desktop app (Tauri 2 + Rust + Svelte 5), and
for ATV Optimizer for Android in [`mobile/`](mobile/). Both share one audited engine in
[`crates/core`](crates/core).

**Looking to install it?** See the [main README](../README.md). This page is for building from
source and contributing.

[`HANDOFF.md`](HANDOFF.md) carries the current roadmap and priorities; read it before starting work.
Release notes live in [`CHANGELOG.md`](CHANGELOG.md).

## Technology choices

| Layer | Pick | Rationale |
|---|---|---|
| Application framework | **Tauri 2** | Native installers (.msi / .dmg / .deb / .rpm / .AppImage) built in, signed auto-update plugin, single codebase covers desktop *and* mobile (Tauri 2 mobile support is stable). |
| Backend language | **Rust** | Tauri's host language. Engine + ADB driver written in Rust, exposed to the frontend via Tauri commands. |
| Frontend framework | **Svelte 5 + SvelteKit** (locked) | Picked for small bundle, rune-based reactivity, and minimal boilerplate. React and Solid would also have been reasonable; switching after the first views land is the expensive moment so we lock here. |
| Build / package mgmt | `cargo` (Rust) + `npm` (Node, for frontend bundling) | Standard Tauri layout |
| Update channel | Tauri Updater plugin → GitHub Releases JSON manifest | Signed (Ed25519), automatic on desktop |

Rust + Tauri 2 was picked because it is the only stack that covers "installable desktop + mobile from one codebase" without paying for features we don't use.

## Architecture

Three-layer separation. The legacy v1 script tangles policy, I/O and UI in one PowerShell file; v2 keeps them apart.

```
┌─────────────────────────────────────────────────────────────┐
│  Frontend (Svelte/TS, runs in webview)                      │
│  - Routes, views, forms, state                              │
│  - Calls Tauri commands; no business logic                  │
└──────────────────────────┬──────────────────────────────────┘
                           │ Tauri command bridge
┌──────────────────────────▼──────────────────────────────────┐
│  Tauri commands (Rust, thin)                                │
│  - Adapts engine results to TS-friendly shapes              │
│  - Handles cancellation, progress streams                   │
└──────────────────────────┬──────────────────────────────────┘
                           │
                  ┌────────┴────────┐
                  │                 │
┌─────────────────▼──┐   ┌─────────▼────────────────────────┐
│  Engine (Rust lib) │   │  ADB driver (Rust lib)           │
│  - App lists       │   │  - subprocess: `adb` binary      │
│  - Snapshot schema │   │  - Parses dumpsys outputs        │
│  - Policy rules    │   │  - Returns structured types      │
│  - No I/O          │   │  - One impl per transport        │
└────────────────────┘   └──────────────────────────────────┘
```

The engine is the part that's portable. It knows what the rules are (which packages are bloat on which device, which settings get tuned to what values, what a valid snapshot looks like) without knowing how to talk to a device.

## Project layout

```
v2/
├── README.md, HANDOFF.md, CHANGELOG.md
├── Cargo.toml, Cargo.lock    # Rust workspace
├── package.json              # desktop frontend dependencies
├── crates/
│   └── core/
│       ├── data/app-lists/   # embedded app catalog JSON
│       └── src/
│           ├── engine/       # pure safety/planning logic (no I/O)
│           ├── adb/          # shared driver trait + parsers
│           └── commands/     # driver-generic Tauri commands
├── src/                      # desktop Svelte frontend (TypeScript)
├── mobile/                   # ATV Optimizer for Android (see mobile/HANDOFF.md)
└── src-tauri/                # desktop Tauri app
    ├── Cargo.toml, build.rs, tauri.conf.json
    ├── icons/, capabilities/, resources/
    └── src/
        ├── adb/              # desktop SubprocessAdb + install/scan helpers
        ├── commands/         # desktop-only file/install/update commands
        ├── lib.rs            # desktop Tauri entrypoint
        └── main.rs
```

## Architectural commitments

These are non-negotiable; deviating is a regression:

1. **Engine has no I/O.** It returns plans and inspects results — does not call `adb`, does not read files, does not make HTTP requests, does not log. The tests prove this by injecting a mock ADB driver.
2. **App lists are runtime data, not embedded code.** A separate **loader** lives in the shared command layer (next to the command bridge, not in the engine) and embeds the JSON at compile time via `include_str!`. The engine accepts app lists as inputs and doesn't care where they came from, which leaves the door open to fetching a newer list at runtime — **not built.** Editing the JSON currently requires a rebuild and a release. If runtime fetching
is ever added it needs signature verification and an offline fallback to embedded.
3. **All ADB output goes through one wrapper.** Single point for tracing, retries, structured logging. No naked `adb ...` calls scattered through the codebase.
4. **The detection logic exists exactly once.** v1 grew two device-type-detection paths that disagree on edge cases (see `docs/FEATURES.md` §13.1). v2 has one.
5. **Snapshots are versioned.** `schemaVersion` in every snapshot file. The reader handles old versions or rejects them with a clear error.
6. **Strict mode everywhere.** Rust's compiler catches the equivalent of v1's `Set-StrictMode -Version Latest`; preserve that. No `unwrap()` on values that can fail at runtime — propagate `Result`.
7. **Reversibility model preserved.** Same disable/uninstall/settings tiers as v1. Recovery requests re-enable/reset actions and reports failures; firmware restrictions can prevent a backout.

## Explicit non-goals

- **Not a root or Magisk tool.** Only ADB-shell-level changes. On-device root operations belong in Magisk modules.
- **Not a custom-ROM flasher.** No fastboot, no recovery image flashing, no Project Treble manipulation.
- **No cloud sync.** Snapshots are local JSON files. No account, no cloud backup.
- **No third-party analytics.** The only reporting is the app's own anonymous device and error
  reports (below), sent to an owner-operated endpoint and off with one setting. Full diagnostics
  stay in local log files the user can choose to share.
- **No firmware-protection bypass.** When firmware refuses a change (for example a protected Fire TV stock launcher, [#205](https://github.com/bryanroscoe/shield_optimizer/issues/205)), the app reports the command error, restore outcome and actual Home readback; it doesn't work around the refusal.

## Local setup

```bash
# Rust toolchain (macOS)
brew install rust

# Or via rustup if you don't have Homebrew:
# curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Tauri OS prerequisites (webkit2gtk on Linux, etc.)
# See https://v2.tauri.app/start/prerequisites/

# From repo root:
cd v2
npm install                # install frontend deps
npm run tauri dev          # run in development (opens a window)

# Bundler build — produces the platform installers the release workflow ships:
npm run tauri build
```

The development flow is:
- `cd v2 && npm run tauri dev` to run the GUI
- `cd v2 && cargo test -p shield-optimizer-core -p shield-optimizer-v2` to run Rust tests
- `cd v2 && npm run check` to type-check the frontend

## Telemetry and support tiers

Anonymous reports are **on by default and opt-out**, and compiled in only when the build sets
`ATVOPT_TELEMETRY_URL` (the base URL of the owner-operated Cloudflare Worker, no trailing path).
Without it, telemetry is a complete no-op: nothing is queued and nothing touches the network.
The URL must be `https://`; plain `http://` is accepted only for a loopback host (local Worker
development), and anything else is ignored as if unset.

```bash
ATVOPT_TELEMETRY_URL=https://telemetry.example.workers.dev npm run tauri build
```

The release workflow passes the optional repository variable `ATVOPT_TELEMETRY_URL` to the
desktop build; leave it unset to ship a build that never reports.

- Payloads are built and redacted in the pure `crates/core/src/engine/telemetry.rs`; queueing and
  upload live in `crates/core/src/telemetry.rs` (`POST <base>/v1/events`, at most 50 events and
  64 KB per request, 5 s connect / 10 s total timeouts, bounded queue, silent on failure). Never
  sent: `ro.serialno` or any hardware id, IPs or ports, TV names, package lists, file paths,
  license data, snapshot contents.
- Events: `device_seen` once per TV per session after profiling, `action_result` for connect,
  scan, disable, enable, uninstall, optimize, snapshot apply, launcher, settings writes and
  pairing, and `error` for Rust panics, uncaught frontend errors and failed untracked commands
  (rate limited). The frontend's one invoke wrapper reports outcomes through `telemetry_action` /
  `telemetry_error`; `shared/telemetry.ts` maps commands to actions.
- The setting lives in `<data_dir>/telemetry.json`, written atomically. Off clears the queue and
  cancels an upload in flight; back on generates a new random install id. A file that exists but
  can't be read counts as opted out. Messages are also scrubbed of every transport serial and
  hardware id seen this session, whatever their shape.
- **Support tiers** (`unknown`, `not_supported`, `beta`, `supported`, `fully_supported`) come from
  `crates/core/data/device-support.json`, embedded like the app lists and matched by the pure
  `engine/support.rs` against what the device reported. When reports are on, the app also fetches
  `GET <base>/v1/support-tiers`, validates it, and caches it as `<data_dir>/support-tiers.json`;
  any failure keeps the embedded rules. When fetched rules change, Rust emits
  `support-tiers-updated` and both frontends re-list devices. Tiers are informational only and
  never feed `engine::safety`.

## See also

- [`HANDOFF.md`](HANDOFF.md) — current roadmap and priorities; read before changing anything
- [`CHANGELOG.md`](CHANGELOG.md) — release notes (the release workflow reads each `## v2-X.Y.Z` section)
- [`ATVTOOLS-PARITY.md`](ATVTOOLS-PARITY.md) — remaining feature gaps vs. aTV Tools
- [`mobile/README.md`](mobile/README.md) — ATV Optimizer for Android
- [`e2e/README.md`](e2e/README.md) and [`screenshots/README.md`](screenshots/README.md) — UI tests and the generated gallery
- [`../docs/LEGACY-V1.md`](../docs/LEGACY-V1.md) — the legacy PowerShell script

## Third-party components

The Remote tab's low-latency input channel bundles the
[scrcpy](https://github.com/Genymobile/scrcpy) server
(`src-tauri/resources/scrcpy-server-v3.1`, © Genymobile, licensed under the
[Apache License 2.0](https://github.com/Genymobile/scrcpy/blob/master/LICENSE)).
It is pushed to the device's temp storage (`/data/local/tmp`) and runs with
shell privileges only while a Remote session is open; the process exits when
the session closes.

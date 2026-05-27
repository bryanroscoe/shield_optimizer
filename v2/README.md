# Shield Optimizer v2

A ground-up rewrite of Shield Optimizer as a **native installable desktop app** with **built-in auto-update** and an eventual **mobile companion** — replacing the v1 PowerShell script while preserving every behavior catalogued in [`docs/FEATURES.md`](../docs/FEATURES.md).

This directory is the v2 workspace. v1 (`Shield-Optimizer.ps1` at the repo root) continues to ship and accept patches until v2 reaches parity.

## Status

🚧 **Foundation built — read-only features working.** Phases 1–4 from [PLAN.md](PLAN.md) are landed. v2 currently:

- Builds: `cargo build` and `npm run build` both produce artifacts
- Tests: **36 Rust tests** (engine + ADB parsers + loader sanity), 0 failing
- Runs as a real Tauri desktop app (`npm run tauri dev` opens a window)
- Lists ADB devices with friendly Shield model names + device-type detection
- Shows a per-device profile, health report (display mode, HDR support, top memory), launcher catalog (with installed/disabled state), full app list for the detected device type, and snapshot save / list / preview-apply

**Not yet wired (next phases):**

- Optimize / Restore execution (the engine computes plans; execution is the next layer)
- Launcher set-default actions (UI shows status; promotion logic next)
- Snapshot *apply* (preview works; execution comes with optimize)
- Tweaks (HDMI-CEC, match-content-frame-rate, long-press-timeout)
- Display Scaling, APK sideload, Reboot, Recovery
- Bundler config for installers (Phase 10)
- Auto-update plugin (Phase 10)
- Mobile (Phase 11 — pending the ADB-wire-protocol research spike)

The behavior spec is at [`docs/FEATURES.md`](../docs/FEATURES.md). The porting roadmap is in [`PLAN.md`](PLAN.md).

## Technology choices

| Layer | Pick | Rationale |
|---|---|---|
| Application framework | **Tauri 2** | Native installers (.msi / .dmg / .deb / .rpm / .AppImage) built in, signed auto-update plugin, single codebase covers desktop *and* mobile (Tauri 2 mobile support is stable). |
| Backend language | **Rust** | Tauri's host language. Engine + ADB driver written in Rust, exposed to the frontend via Tauri commands. |
| Frontend framework | **Pending decision** | Defaulting to **Svelte** in the plan unless overridden. React and Solid are also reasonable; the choice affects developer experience more than user experience. |
| Build / package mgmt | `cargo` (Rust) + `npm` (Node, for frontend bundling) | Standard Tauri layout |
| Update channel | Tauri Updater plugin → GitHub Releases JSON manifest | Signed, automatic on desktop; manual-confirm on Android due to Google's APK-install policy |
| Mobile distribution | GitHub Releases APK + F-Droid + recommend [Obtainium](https://obtainium.imranr.dev/) | Skipping Play Store — they reject apps that disable other apps via ADB-style mechanisms |

See the conversation history in commit `0362933`'s parent thread for the full Go-vs-Rust-vs-Compose discussion. Short version: Rust + Tauri 2 is the only stack that covers "installable desktop + mobile from one codebase" without paying tax for features we don't use.

## Architecture

Three-layer separation. v1's main pain point is that policy, I/O, and UI are tangled inside one PowerShell file. v2 keeps them apart.

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
│  - No I/O          │   │  - Single impl per platform      │
└────────────────────┘   └──────────────────────────────────┘
```

The engine is the part that's portable. It knows what the rules are (which packages are bloat on which device, which settings get tuned to what values, what a valid snapshot looks like) without knowing how to talk to a device.

## Project layout

```
v2/
├── README.md, PLAN.md       # this doc + porting roadmap
├── package.json             # frontend dependencies
├── svelte.config.js         # SvelteKit (SPA mode, adapter-static)
├── vite.config.js
├── tsconfig.json
├── data/
│   └── app-lists/
│       ├── common.json      # universal bloat list (incl. defunct apps)
│       ├── shield.json      # Shield-specific
│       └── googletv.json    # Google TV / Onn / Chromecast-specific
├── src/                     # Svelte frontend (TypeScript)
│   ├── app.html
│   ├── lib/
│   │   ├── api.ts           # typed wrappers around Tauri invoke()
│   │   └── types.ts         # TS counterparts of Rust types
│   └── routes/
│       ├── +layout.svelte   # app shell, nav, global styles
│       ├── +layout.ts       # SSR disabled (Tauri SPA mode)
│       ├── +page.svelte     # device list + Connect IP form
│       ├── devices/[serial]/+page.svelte  # tabs: Overview / Health / Launcher / Apps / Snapshot
│       └── snapshots/+page.svelte         # global snapshot list
└── src-tauri/               # Rust backend
    ├── Cargo.toml, build.rs, tauri.conf.json
    ├── icons/, capabilities/
    └── src/
        ├── lib.rs           # Tauri entry — registers commands, manages state
        ├── main.rs
        ├── engine/          # pure logic (no I/O — commitment #1)
        │   ├── types.rs     # Device, AppEntry, OptimizeAction, etc.
        │   ├── detection.rs # ONE device-type-detection fn (resolves v1 duplicate paths)
        │   ├── app_lists.rs # merge logic for common + device-specific lists
        │   ├── launcher.rs  # custom launcher catalog + package validation
        │   └── snapshot.rs  # versioned schema + apply-plan computation
        ├── adb/             # ADB driver
        │   ├── driver.rs    # AdbDriver trait + SubprocessAdb impl
        │   └── parse.rs     # output parsers (devices, packages, meminfo, display)
        └── commands/        # Tauri command bridge (thin)
            ├── state.rs     # AppState held by tauri::manage
            ├── loader.rs    # embeds + loads app-lists JSON (host layer, not engine)
            ├── devices.rs   # list_devices, device_profile, connect/disconnect
            ├── health.rs    # health_report, app_list_for_device
            ├── launcher.rs  # list_launchers, current_launcher, channel_provider_disabled
            └── snapshot.rs  # list/save snapshots, preview_apply
```

## Architectural commitments

These are non-negotiable; deviating is a regression:

1. **Engine has no I/O.** It returns plans and inspects results — does not call `adb`, does not read files, does not make HTTP requests, does not log. The tests prove this by injecting a mock ADB driver.
2. **App lists are runtime data, not embedded code.** A separate **loader** lives in the Tauri host layer (next to the command bridge, not in the engine). The loader is responsible for: shipping with embedded JSON defaults; fetching the latest from a versioned URL (`raw.githubusercontent.com/.../v2/data/app-lists/<file>.json` or similar) on launch; falling back to embedded on offline; signature-verifying fetched lists. The engine accepts app lists as inputs and is agnostic to where they came from. This is the only way to honor commitment #1 while supporting hot-shipping of dead-app updates.
3. **All ADB output goes through one wrapper.** Single point for tracing, retries, structured logging. No naked `adb ...` calls scattered through the codebase.
4. **The detection logic exists exactly once.** v1 has two device-type-detection paths that don't agree on edge cases (see `docs/FEATURES.md` §13.1). v2 must have one.
5. **Snapshots are versioned.** `schemaVersion` in every snapshot file. The reader handles old versions or rejects them with a clear error.
6. **Strict mode everywhere.** Rust's compiler catches the equivalent of v1's `Set-StrictMode -Version Latest`; preserve that. No `unwrap()` on values that can fail at runtime — propagate `Result`.
7. **Reversibility model preserved.** Same disable/uninstall/settings tiers as v1. The Recovery action remains a one-click backout.

## Explicit non-goals

To bound scope expectations:

- **Not a root or Magisk tool.** v2 makes only ADB-shell-level changes. If a user wants on-device root operations, they should keep using Magisk modules.
- **Not a custom-ROM flasher.** No fastboot, no recovery image flashing, no Project Treble manipulation.
- **No cloud sync.** Snapshots are local-file JSON. No account, no telemetry, no opt-in cloud backup.
- **No crash/usage telemetry.** This is a privacy-adjacent tool; users opt in to *us*, not to a third-party analytics vendor. Sentry-style crash uploads are out — diagnostics live in local log files the user can choose to share.
- **No Play Store distribution.** Google bans tools that disable other apps via ADB-style mechanisms. Distribution is sideload-only.

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

# Bundler build (when packaging is wired up — Phase 10):
# npm run tauri build      # would produce platform installers
```

For now, the development flow is:
- `cd v2 && npm run tauri dev` to run the GUI
- `cd v2/src-tauri && cargo test --lib` to run engine tests
- `cd v2 && npm run check` to type-check the frontend

## Frontend framework decision

The choice between **Svelte / React / Solid** doesn't affect what the app *does*. It affects:

- Learning curve (Svelte ~ Solid < React for greenfield)
- Bundle size (Svelte / Solid << React)
- Hiring pool / ecosystem (React >> Svelte ~ Solid)
- Tooling maturity (React >> Svelte ~ Solid)

Default in the plan is **Svelte**. Override before running `create-tauri-app` if preferred.

## See also

- [PLAN.md](PLAN.md) — phased porting roadmap with milestones
- [`../docs/FEATURES.md`](../docs/FEATURES.md) — behavior spec (the source of truth)
- v1: `Shield-Optimizer.ps1` at repo root

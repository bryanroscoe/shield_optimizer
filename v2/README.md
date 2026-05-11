# Shield Optimizer v2

A ground-up rewrite of Shield Optimizer as a **native installable desktop app** with **built-in auto-update** and an eventual **mobile companion** — replacing the v1 PowerShell script while preserving every behavior catalogued in [`docs/FEATURES.md`](../docs/FEATURES.md).

This directory is the v2 workspace. v1 (`Shield-Optimizer.ps1` at the repo root) continues to ship and accept patches until v2 reaches parity.

## Status

🛠 **Planning.** No code yet. Toolchain pending install (see "Local setup" below).

The behavior spec is locked at [`docs/FEATURES.md`](../docs/FEATURES.md) — 17 sections covering every v1 feature, every ADB command, every edge case. v2 implementation work walks down that spec.

The porting roadmap is in [`PLAN.md`](PLAN.md).

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

## Architectural commitments

These are non-negotiable; deviating is a regression:

1. **Engine has no I/O.** It returns plans and inspects results — does not call `adb`, does not read files, does not log. The tests prove this by injecting a mock ADB driver.
2. **App lists are runtime data, not embedded code.** `data/app-lists/*.json` ships with the binary as defaults, but the app fetches the latest from a versioned URL (`raw.githubusercontent.com/.../v2/data/app-lists/<file>.json` or similar) on launch, falling back to embedded on offline. Lets dead-app additions ship without a binary release.
3. **All ADB output goes through one wrapper.** Single point for tracing, retries, structured logging. No naked `adb ...` calls scattered through the codebase.
4. **The detection logic exists exactly once.** v1 has two device-type-detection paths that don't agree on edge cases (see `docs/FEATURES.md` §13.1). v2 must have one.
5. **Snapshots are versioned.** `schemaVersion` in every snapshot file. The reader handles old versions or rejects them with a clear error.
6. **Strict mode everywhere.** Rust's compiler catches the equivalent of v1's `Set-StrictMode -Version Latest`; preserve that. No `unwrap()` on values that can fail at runtime — propagate `Result`.
7. **Reversibility model preserved.** Same disable/uninstall/settings tiers as v1. The Recovery action remains a one-click backout.

## Local setup (once toolchain ready)

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install Tauri prerequisites (platform-specific deps)
# See https://v2.tauri.app/start/prerequisites/

# From repo root:
cd v2
cargo install create-tauri-app
cargo create-tauri-app .   # interactive; pick Svelte frontend
npm install                 # install frontend deps
npm run tauri dev          # run in development
npm run tauri build        # produce platform installers
```

When that's done, this README gets updated with the actual project structure that `create-tauri-app` produced.

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

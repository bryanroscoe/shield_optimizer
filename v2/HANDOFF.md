# v2 Handoff — Resume Plan

This file exists to survive a context clear. It captures **exactly where we are**, what's committed vs. uncommitted, and what to do next, in priority order.

Last updated mid-session right before context exhaustion. Working branch: `feat/v2-foundation` → PR #25 at `https://github.com/bryanroscoe/shield_optimizer/pull/25`.

## 1. Snapshot of the working tree

### Uncommitted local changes (must be committed before clearing context — or reproduced from this doc)

Modified files:
- `v2/src-tauri/src/commands/launcher.rs` — Added `set_default_launcher_impl(state, serial, package)` so other commands can call the multi-strategy promotion without `tauri::State` lifetimes. Body changed from `String`/`&str` to all `&str`; references like `Some(package)` updated to `Some(package.to_string())`.
- `v2/src-tauri/src/commands/mod.rs` — registers `apps`, `recovery`, `reboot`, `tuning`, `sideload` modules.
- `v2/src-tauri/src/commands/snapshot.rs` — Added `apply_snapshot` Tauri command that actually executes the previewed plan (disables packages from list, sets launcher via `set_default_launcher_impl`, writes settings in one batched `settings put` shell call). Reuses the same canonicalize-and-contain path-traversal protection as `preview_apply`.
- `v2/src-tauri/src/lib.rs` — imports `reboot, recovery, tuning` and registers their commands in `generate_handler!`.
- `v2/src/routes/devices/[serial]/+page.svelte` — UI changes:
  - Removed the "Default" column from the App List table (it pre-selected the Optimize wizard's default action in v1; v2 has no wizard yet, so it was meaningless).
  - Added a `RECOMMENDED` tag (uses existing `tag installed` class) next to app names where `default_optimize` is true, with a tooltip saying it'll be pre-selected by the future Optimize wizard.
  - Fixed `.small-action.danger` styling — was outlined-only red on transparent, looked inconsistent next to the filled Disable button. Now uses `background: #21262d` with a subtle red border, fills on hover.

New files (untracked):
- `v2/src-tauri/src/commands/recovery.rs` — `panic_recovery` command. Reads `pm list packages -d`, runs `pm enable` for each, returns `RecoveryResult { restored: Vec<String>, failed: Vec<RecoveryFailure>, message }`.
- `v2/src-tauri/src/commands/reboot.rs` — `reboot_device` command with `RebootMode` enum (`normal` / `recovery` / `bootloader`).
- `v2/src-tauri/src/commands/tuning.rs` — three commands:
  - `get_tweaks(serial)` → batched `settings get` for all 9 tracked keys (HDMI-CEC quad, match-content-frame-rate, long-press-timeout, animation triple), returns `TweaksState`.
  - `write_setting(serial, namespace, key, value)` → `settings put <ns> <key> <value>` (or `settings delete` when value is empty). Namespace whitelist (`global` / `secure` / `system`), shell-metacharacter rejection in value.
  - `set_display_scaling(serial, preset)` with `DisplayScalePreset` enum (`uhd_4k` = 3840x2160 / density 540, `fhd_1080p` = 1920x1080 / density 320, `reset`). Runs `wm size` + `wm density` in one shell call.

**Local build/test state at the time of writing:** `cargo build` passes. `cargo fmt` / `cargo clippy -D warnings` / `cargo test --lib` (56 tests) all clean. `npm run check` clean.

### What's already merged into PR #25 on origin (so you don't redo it)

Latest pushed commit: `2bfe1de`. Major landings:
- Engine + ADB + commands layout, JSON-loaded app catalogs (incomplete — see §3)
- Network scan + auto-scan-on-boot
- Auto-download platform-tools (`install_platform_tools` + Tauri `install_adb` command)
- Device list (with `[NET]`/`[USB]` tags), Profile view, Health Report (display/memory/temp/storage), Launcher tab with `set_default_launcher` (multi-strategy: pm enable → role API → set-home-activity → HOME-intent), App List with per-app Disable/Enable/Uninstall buttons, Snapshot save/list/preview (apply was deferred, NOW LANDED LOCALLY — see §1).
- APK install via tauri-plugin-dialog file picker + `install_apk` command + error decoder
- Health tab live-refresh checkbox + last-refreshed time + Top Memory entries 1-20 + per-row Disable buttons
- Custom icon designed (heater-shield + checkmark, blue palette). SVG at `v2/src-tauri/icons/source.svg`.
- CI: `.github/workflows/v2-tests.yml` (cargo fmt/clippy/test matrix + svelte-check + vite build)

## 2. Critical-path TODO, prioritized

**Status as of commit e5a6b0e+: every v1 must-fix is now landed.** See §2A
"Parity status" below for the full row-by-row match-up against FEATURES.md.

### Big feature: Optimize / Restore wizard (LANDED in 1d9cd41)

v1's `Run-Task -Mode Optimize` does:
1. Prompts "Apply all default actions without prompting?" (defaults mode)
2. For each app in the device's list (CommonAppList + ShieldAppList for Shield):
   - Skip if not installed, already disabled, already uninstalled
   - Show `(using X MB RAM)` if running
   - Prompt: DISABLE / SKIP / UNINSTALL / ABORT (default depends on method + `default_optimize`)
   - Apply the chosen action via `pm`
3. After app loop: Performance Settings prompt — set the animation triple (0.5)
4. Summary of counts
5. Offer reboot

**v2 plan for Optimize:**

- **Engine side (pure)**: Add `engine::optimize::compute_plan(apps: &[AppEntry], inputs: OptimizeInputs) -> OptimizePlan` where `OptimizeInputs` has `installed_packages`, `disabled_packages`, `memory_map: HashMap<String, f64>`. Returns `OptimizePlan { items: Vec<OptimizePlanItem> }`. Already have the types in `engine/types.rs` — `OptimizeAction`, `SkipReason`, `OptimizePlanItem`. Just need the function.

- **Command side**: `commands/optimize.rs`:
  - `prepare_optimize(serial, mode)` — fetches installed/disabled lists + memory map, runs the engine, returns the plan as JSON to the frontend.
  - `execute_optimize_item(serial, package, action)` — runs the one `pm` command for that item, returns ActionResult. Frontend iterates and shows progress.
  - Or: `execute_optimize_plan(serial, plan)` — runs everything sequentially and streams progress via Tauri events.
  
  **Recommendation:** per-item via the existing `apps.rs` commands (disable_package / uninstall_package / enable_package). The wizard UI iterates and calls them one at a time, showing live progress. Simpler, no event streaming required.

- **Frontend**: New tab or modal "Optimize Wizard". Three modes: Review-each / Apply-defaults / Cancel. Shows each app row with RAM annotation, risk tag, recommended action, override dropdown. Big "Run" button. Per-app progress + abort. Summary screen at the end.

### Below-the-fold features — all landed

- **Light theme support** — `prefers-color-scheme: light` block flips body/chrome via CSS vars in `+layout.svelte`.
- **PIN pairing** — `pair_device` command + Pair PIN form on home page.
- **Restart ADB** — `restart_adb` command + button on home toolbar.
- **Report All** — `report_all` command + collapsible panel on home page.
- **Help screen** — skipped intentionally for v2; this is a GUI and every action is a labeled, hover-tooltipped button. v1's Help was a TUI keyboard-shortcut reference that doesn't translate.
- **Disable/Restore stock launchers wizard** — `list_home_handlers` + `disable_stock_launchers` + `restore_stock_launchers`. UI lives at the bottom of the Launcher tab.

## 2A. Parity status vs. v1 (FEATURES.md row-by-row)

| FEATURES.md § | v1 feature | v2 status |
|---|---|---|
| §0.1 | -LightMode / -DarkMode | Auto via `prefers-color-scheme`, no CLI flag (Tauri inherits OS) |
| §0.1 | -ForceAdbDownload | `install_adb` button forces a fresh download |
| §0.1 | -Subnet override | Auto-detect only — manual override TODO (rare path) |
| §1.1 | Network scan | `scan_network` + boot autorun + Scan Network button |
| §1.2 | Connect IP | `connect_device` + Connect IP form |
| §1.3 | PIN pairing | `pair_device` + Pair PIN form |
| §1.4 | Disconnect | `disconnect_device` + device-header button |
| §1.5 | ADB lifecycle (install/restart) | `install_adb`, `restart_adb`, `adb_status` |
| §1.6 | Device enumeration | `list_devices` + Profile view |
| §1.7 | UNAUTHORIZED guidance | Inline numbered help on the device row |
| §2.1 | Main menu actions | All landed except Help (GUI inherent) |
| §3   | Device action menu | All actions present as tabs / header buttons |
| §4   | Optimize/Restore | `engine::optimize::compute_plan` + `prepare_optimize` + Optimize tab |
| §5.1 | Health Report | `health_report` + Health tab |
| §5.2 | Live Monitor | Live-refresh checkbox on Health tab — close enough |
| §5.3 | Display + audio diagnostics | display + `parse_active_audio_device` |
| §6.1 | Custom launcher catalog | `launcher_catalog` |
| §6.2 | Setup wizard | Launcher tab |
| §6.3 | Multi-strategy set-default | `set_default_launcher_impl` (role API → set-home-activity → intent kick) |
| §6.4 | Disable stock launchers | `list_home_handlers` + `disable_stock_launchers` + UI |
| §6.5 | Restore stock launchers | `restore_stock_launchers` + UI |
| §6.6 | Channel dependency warning | `channel_provider_disabled` + tag |
| §7   | Tweaks | `get_tweaks` + `write_setting` + Tweaks tab |
| §8   | Display scaling | `set_display_scaling` + Tweaks tab section |
| §9   | Snapshot save/preview/apply | All landed |
| §10  | APK sideload | `install_apk` + Install APK tab |
| §11  | Reboot menu | `reboot_device` + header dropdown |
| §12  | Recovery | `panic_recovery` + Overview tab button |
| §13  | Device profile + detection | `device_profile` + `detect_device_type` |
| §14.4| Help | Skipped — GUI inherent |
| §15  | Data catalogs (~67 entries) | Complete: common(49) + shield(13) + googletv(5) |
| §16.5| Play Store deep-link | `open_play_store` + buttons in App List |
| §16.6| Uninstall error decoding | `decode_uninstall_error` + auto-applied to `uninstall_package` result |

Known minor TODO items (not blockers): manual `-Subnet` override.

## 3. Release pipeline — NOT YET STARTED

User asked for installers for Windows / Linux / Mac / Flatpak. None of this is done yet. Plan:

### `.github/workflows/v2-release.yml`

Triggered on tag push matching `v2-*` (so v1's `v0.x.x` tags don't trigger it). Matrix build with `tauri-apps/tauri-action` which bundles installers and uploads them to a GitHub release.

```yaml
strategy:
  matrix:
    include:
      - { os: ubuntu-latest, target: 'x86_64-unknown-linux-gnu' }
      - { os: macos-latest, target: 'aarch64-apple-darwin' }
      - { os: macos-latest, target: 'x86_64-apple-darwin' }
      - { os: windows-latest, target: 'x86_64-pc-windows-msvc' }
```

Per-OS prerequisites:
- **Linux**: `sudo apt-get install -y libwebkit2gtk-4.1-dev libssl-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev`
- **macOS**: nothing extra (notarization later)
- **Windows**: nothing extra

Tauri's bundler will produce:
- `.dmg` + `.app.tar.gz` (macOS)
- `.msi` + `.exe` (Windows)
- `.deb` + `.rpm` + `.AppImage` (Linux)

### Flatpak

Flatpak isn't in `tauri-apps/tauri-action`'s default outputs. Either:
- Use `flatpak-builder` directly in a separate job, with a manifest at `v2/com.shieldoptimizer.app.json`. Manifest builds from source against `org.freedesktop.Platform//23.08` and a `rust-stable` + `node-22.12` SDK extension. The build command is `cd v2 && npm install && npm run tauri build`. Output `.flatpak` bundle.
- OR skip Flatpak in v2.0 and document the `.AppImage` as the Linux distribution. Users can flatpak-build locally from source.

### Code signing (Phase 10.2 from PLAN.md)

- **macOS**: Apple Developer Program ($99/yr) + notarization. Set `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID` secrets. Tauri-action picks them up automatically.
- **Windows**: Need to make the decision — EV cert ($300-500/yr) for immediate SmartScreen trust, or unsigned + documented SmartScreen-bypass instructions in README. As-shipped today, unsigned is fine for a development build, not for v2.0 GA.
- **Linux**: GPG-sign the `.deb` and `.AppImage` with a personal key. Optional.

### Auto-updater wiring (Phase 10.3)

Tauri Updater plugin needs:
1. `tauri-plugin-updater = "2"` in Cargo.toml
2. Generate Ed25519 signing key with `tauri signer generate` — PRIVATE KEY CUSTODY IS CRITICAL (see PLAN.md §10.5).
3. `tauri.conf.json` updater config pointing at `https://github.com/bryanroscoe/shield_optimizer/releases/latest/download/latest.json`.
4. Release workflow uploads a signed `latest.json` alongside each release.
5. Frontend: on startup, call `await check()`, show "Update available" prompt, `await update.downloadAndInstall()`.

## 4. How to resume cleanly

1. **Check what's pushed**: `git fetch && git log --oneline origin/feat/v2-foundation -10`. Last pushed should be `2bfe1de`. If anything from §1's uncommitted list isn't there, replay it from this doc.
2. **Run local validation**: `cd v2/src-tauri && cargo fmt --check && cargo clippy --lib --tests -- -D warnings && cargo test --lib && cd .. && npm run check && npm run build` — all should be green.
3. **Read** `docs/FEATURES.md`, `v2/PLAN.md`, this file. The audit findings in §2 of this file are the ground truth for "what's missing".
4. **Pick a critical-path item from §2**, implement engine + command + UI, commit, push.

### Open questions to defer until you're back

- Windows code signing budget decision (EV cert vs. unsigned).
- Flatpak as a Phase 10 deliverable vs. v2.1+.
- Whether to ship Optimize as a wizard modal or as a dedicated tab. (Recommendation: tab, because the App List tab is already there showing the same data — Optimize is the action layer on top of it.)

## 5. Key file/function pointers

- **Engine entry**: `v2/src-tauri/src/engine/mod.rs`
- **ADB driver**: `v2/src-tauri/src/adb/driver.rs` — `SubprocessAdb`, `AdbDriver` trait, `discover_adb_binary`
- **Command list**: `v2/src-tauri/src/lib.rs` `invoke_handler` block — single source of truth for what commands the frontend can call
- **TS API**: `v2/src/lib/api.ts` — one wrapper per command; keep names in sync with Rust
- **TS types**: `v2/src/lib/types.ts` — mirrors serde Rust types
- **Device page** (where most UI lives): `v2/src/routes/devices/[serial]/+page.svelte`
- **App catalog data**: `v2/data/app-lists/{common,shield,googletv}.json`
- **v1 reference**: `Shield-Optimizer.ps1` at repo root, plus `docs/FEATURES.md` spec.
- **v1 device for live testing**: Bryan's Shield at `192.168.42.71:5555` (may require re-auth on TV).
- **Repo-local v1 adb binary**: `./adb` at repo root, used by `discover_adb_binary`'s repo-local fallback.

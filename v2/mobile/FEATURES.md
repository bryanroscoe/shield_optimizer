# ATV Optimizer mobile — features inventory

> Historical snapshot from the initial architecture review. Many row statuses below are stale.
> Use [`BACKLOG.md`](BACKLOG.md) and [`HANDOFF.md`](HANDOFF.md) for the current implementation and
> ordered queue; update this matrix only as a dedicated reconciliation pass.

This is the buildout checklist for the mobile re-architecture: every desktop feature mapped to
the mobile design's 20 screens, with a Built/Partial/Todo status and the Free/Pro split. It's a
snapshot, not a contract — desktop's `v2/src/lib/api.ts` keeps evolving, so re-check this doc
against `api.ts` and `v2/mobile/src-tauri/src/lib.rs`'s `invoke_handler!` list whenever either
changes. Screen numbers/names are from the design file `ATV Optimizer Mobile.dc.html` (the
`<!-- N.N ... -->` section markers). "Backend" status reflects whether the Tauri command is
*registered* on mobile (`v2/mobile/src-tauri/src/lib.rs`); "Frontend" reflects whether a mobile
screen actually calls it (`v2/mobile/src/screens/*.svelte`).

Sources read: `v2/src/lib/api.ts`, `v2/src/lib/types.ts`, `v2/crates/core/src/license.rs`,
`v2/crates/core/src/commands/{apps,snapshot,launcher,optimize,reboot,tuning}.rs`,
`v2/mobile/src-tauri/src/lib.rs`, `v2/mobile/src/screens/*.svelte`,
`v2/mobile/ARCHITECTURE-REVIEW.md`, `v2/ATVTOOLS-PARITY.md`, `docs/FEATURES.md`.

## Screen → feature map

Status legend: **Built** = a mobile screen exists and calls the real command. **Partial** = a
screen exists but is mocked/fake data, missing the real invoke, or the backend command isn't
registered on mobile yet. **Todo** = no mobile screen exists for this at all.

| # | Design screen | Desktop command(s) / feature | Free or Pro | Mobile status | Notes |
|---|---|---|---|---|---|
| 1.0 | Reconnect (returning) | Saved-TV list + auto-reconnect (no desktop equivalent — desktop always re-scans) | Free | **Todo** | No persistence anywhere on mobile (no localStorage/store); `Onboarding.svelte` always starts at scan. Tracked as ARCHITECTURE-REVIEW F8/T6 — the intended first screen, entirely unbuilt. |
| 1.1 | Scan / Find TV | `wireless_discover` (mobile-only transport; desktop equivalent is `scanNetwork`/`adbStatus`) | Free | **Built** | `Onboarding.svelte` step `"scan"`, calls `wireless_discover`. Manual IP entry (host/pairPort/connectPort inputs) present too. |
| 1.2 | Pair code | `wireless_pair` (mobile-only; desktop has no pairing UI, uses `pairDevice`/`connectDevice` for wireless debug) | Free | **Built** | `Onboarding.svelte` step `"pair"`. |
| 1.3 | Connected success | `wireless_connect`, then `list_devices`/`device_profile` for enrichment | Free | **Built**, enrichment flaky | Step `"connected"`. Device name/model enrichment depends on `harvest_properties` over `exec:`, which ARCHITECTURE-REVIEW F9/T5 flags as historically falling back to "Android TV"/"Unknown Device" — needs on-device re-verification now that `exec:` works. |
| 2.1 | Dashboard | `health_report`, `trim_caches`, `take_screenshot`, `reboot_device`, `find_remote`, `wireless_disconnect` | Free (all of these are free-tier commands on desktop) | **Partial** | `Dashboard.svelte` calls all of the above, but: `reboot_device` is called with the wrong arg name (`rebootMode` instead of `mode`) so it silently never reboots (F1); a failed `health_report` renders as a fabricated 100-score "healthy" device instead of an error state (F2); `healthScore`/`freedRamEst` are invented client-side, not backend values. |
| 2.2 | Health / Diagnostics | `health_report` (storage/RAM/temp/display/audio breakdown), `safety_info` for per-app tags | Free | **Partial** | `Diagnostics.svelte` calls `health_report`, but hardcodes plausible-looking fallbacks (`storageTotal ?? "16G"`, `tempC ?? 48`, `audioOutput ?? "Atmos"`, …) when fields are null (F3) — indistinguishable from real data. Also reimplements safety classification inline (`getSafetyTag`) instead of calling `safety_info`, bypassing the audited core classifier (F5). |
| 3.1 | Optimize review | `prepare_optimize` (`Feature::OptimizeWizard`) | **Pro** | **Partial** | `Optimize.svelte` hardcodes a mock debloat list + "182 MB" sizes; it does call `prepare_optimize` but swallows the real `OptimizePlan`/`LOCKED:` response and discards it (F4). No real plan is ever shown. |
| 3.2 | Optimize running | Per-row apply of the plan from 3.1 (`disable_package` etc., each gated by `Feature::CuratedDebloat`) | **Pro** | **Todo** | No progress/running UI exists — 3.1 doesn't produce a real plan to run. |
| 4.1 | App list | `list_other_packages` (free), `disable_package`/`uninstall_package` (**Pro**, `Feature::CuratedDebloat`), `enable_package`/`force_stop`/`reinstall_existing`/`open_play_store` (free), `app_memory_map`/`app_usage_map`/`package_states`/`safety_info` (free) | Mixed | **Partial** | `Apps.svelte` only calls `list_other_packages` + `enable_package`/`disable_package` toggling. Missing: the curated catalog (`app_list_for_device`), force-stop, memory/usage columns, safety tags, and any Pro gate messaging on disable/uninstall (currently the command just returns a `LOCKED:`-style error with no UI explanation). |
| 4.2 | Launcher | `list_launchers`, `current_launcher`, `channel_provider_disabled` (free); `set_default_launcher`/`disable_launcher` (**Pro**, `Feature::LauncherTakeover`) | Mixed | **Todo (backend ready)** | All 4 commands are registered in mobile's `lib.rs`, but no `Launcher.svelte` screen exists yet. Pure frontend gap. |
| 5.1 | Tweaks | `get_tweaks` (free); `write_setting`/`set_display_scaling`/`set_private_dns` (**Pro**, `Feature::TweaksWrite`); `get_display_scaling`/`get_private_dns` (free) | Mixed | **Todo (backend ready)** | All 6 commands registered on mobile; no `Tweaks.svelte` screen. CEC/frame-rate/animation/scaling/DNS controls from the design are entirely unbuilt on mobile. |
| 5.2 | Snapshots | `list_snapshots`/`save_snapshot`/`preview_apply`/`apply_snapshot`/`delete_snapshot`/`snapshot_dir_path` — all **Pro**, `Feature::Snapshot` | **Pro** | **Todo (backend ready)** | All 6 commands registered on mobile; no `Snapshots.svelte` screen. "Clone this setup to a second TV" (cross-device apply) also needs `MultiDevice` handling, which mobile has no concept of yet (single connected device at a time). |
| 6.1 | Remote | `send_key`, `send_text`, `find_remote` | Free (desktop has no Pro gate on remote input; `Feature::AdvancedRemote` exists in the enum but has no call site anywhere yet) | **Built**, one bug | `Remote.svelte` calls all three. `find_remote` runs `am start -n com.nvidia.remotelocator/...`, which is Shield-only — it errors (silently swallowed) on Google TV devices and should be gated on `device_type === "shield"` (F10), not shown unconditionally as it is on nearly every screen. |
| 6.2 | More / Settings | `wireless_disconnect`, `find_remote`; license/entitlement state (no desktop equivalent — desktop is always Pro) | Free (screen itself); gates Pro features | **Partial** | `More.svelte` fakes Pro status by string-matching the literal text `"pro"` in a license-key input (`licenseStatus = "Pro Unlocked"` if `licenseKey.trim().toLowerCase() === "pro"`) — a placeholder comment even says "Since license logic isn't fully compiled into Rust core yet". Entitlement is never actually read from the backend (`AppState::with_entitlement`). |
| 6.3 | Pro paywall | Same entitlement concept as above — no desktop equivalent (desktop ships as Pro unconditionally) | Pro (marketing surface) | **Todo** | No dedicated paywall screen; the "Pro" upsell is inline copy inside `More.svelte`. The design's dedicated paywall (lifetime license, Ko-fi tip, feature bullets) is unbuilt. |
| 7.1 | Devices hub | `list_devices`, `report_all`, `rename_device` — multi-device management, `Feature::MultiDevice` (enum exists, no call site wired yet on desktop or mobile) | Pro (per design) | **Todo** | Mobile has no multi-device concept at all — one connected device, no saved-device list, no `report_all` roll-up. Overlaps with 1.0's saved-TV persistence gap. |
| 7.2 | File transfer | `list_dir`, `pull_file`, `push_file`, `delete_path`, `find_files`, `copy_file_to_device` — desktop-only, live in `v2/src-tauri` (not shared core), `Feature::FileManager` (enum exists, unwired) | Pro (per design) | **Todo — needs Android SAF** | None of these commands exist in `v2/crates/core`; they're desktop-only Rust using local filesystem paths. Mobile has no filesystem access model — porting requires Android Storage Access Framework (SAF) document-tree APIs via the Kotlin plugin, a materially different implementation, not a straight port. |
| 7.3 | Backups + Google Drive | Overlaps with 5.2 Snapshots (`save_snapshot`/`list_snapshots`) plus new ground: Google Drive upload/restore, cross-device "this phone" backup destination | Pro | **Todo** | Snapshot backend exists (see 5.2) but is unused on mobile; Google Drive integration doesn't exist anywhere in the codebase (desktop or mobile) — net-new scope, not a port. |
| 8.1 | App detail sheet | `safety_info`, `app_memory_map`/`app_usage_map`, then the same disable/uninstall/keep actions as 4.1 | Mixed (view free, destructive actions Pro) | **Todo** | No bottom-sheet/detail view component exists on mobile; `Apps.svelte` only supports a flat list with inline actions. |
| 8.2 | Risk & actions guide | `safety_info` (drives Safe/Blocked/Protected tiers); static "disable vs uninstall" explainer copy | Free | **Todo** | No standalone guide screen; `Diagnostics.svelte`'s inline `getSafetyTag` reimplementation (see 2.2/F5) is the closest thing, and it doesn't call `safety_info` at all. |

## Desktop `api.ts` commands not yet surfaced anywhere in mobile

Not registered in `v2/mobile/src-tauri/src/lib.rs` at all (backend gap, not just frontend):

- **ADB/network lifecycle** (mobile doesn't need these — it uses the embedded wireless-ADB
  Kotlin transport instead of managing a local `adb` binary): `adbStatus`, `checkForUpdate`,
  `installAdb`, `restartAdb`, `scanNetwork`, `connectDevice`, `disconnectDevice`, `pairDevice`
  (mobile has `wireless_discover`/`wireless_pair`/`wireless_connect`/`wireless_disconnect`
  instead).
- **Sideload / app cloning**: `installApk`, `backupApk`, `cloneApp` — Pro-shaped (`Feature::Sideload`
  exists in the license enum but has no call site on desktop either). These live in
  `v2/src-tauri` desktop commands, not shared core.
- **File manager**: `listDir`, `pullFile`, `pushFile`, `deletePath`, `findFiles`,
  `copyFileToDevice`, `listApksInFolder` — same story (`Feature::FileManager`, unwired,
  desktop-only code). Needs SAF on Android (see 7.2 above).

Registered on mobile's backend but **not yet called by any mobile screen** (pure frontend gap,
no port work needed):

- `report_all` (7.1 Devices hub)
- `list_launchers`, `current_launcher`, `channel_provider_disabled`, `set_default_launcher`,
  `disable_launcher` (4.2 Launcher)
- `get_tweaks`, `write_setting`, `get_display_scaling`, `set_display_scaling`,
  `get_private_dns`, `set_private_dns` (5.1 Tweaks)
- `list_snapshots`, `save_snapshot`, `preview_apply`, `apply_snapshot`, `delete_snapshot`,
  `snapshot_dir_path` (5.2 Snapshots)
- `app_memory_map`, `app_usage_map`, `package_states`, `safety_info` (4.1 App list / 8.1 App
  detail / 8.2 Risk guide — data is available, no screen reads it)
- `set_app_permission`, `app_permission_state`, `set_app_op`, `get_app_op` — permissions
  grant/revoke; not represented in the 20-screen design at all, and `docs/ATVTOOLS-PARITY.md`
  lists desktop permissions UI as `❌` too, so this is behind on both platforms.
- `panic_recovery` — the do-not-brick recovery path is registered but has no mobile entry
  point (design has no dedicated recovery screen; desktop has one under Advanced).
- `device_profile` — used implicitly by enrichment flows but no screen calls it directly for a
  refresh/re-detect action.

## Free/Pro split (from `v2/crates/core/src/license.rs::Feature`)

Gated today (has a `require_pro` call site): `CuratedDebloat` (disable/uninstall package),
`OptimizeWizard` (prepare_optimize), `Snapshot` (all 6 snapshot commands), `LauncherTakeover`
(set/disable launcher), `TweaksWrite` (write_setting, set_display_scaling, set_private_dns),
`AppPermissionWrite` (set_app_permission, set_app_op), `AdvancedReboot` (reboot_device).

Defined in the enum but **no call site anywhere yet** (reserved for future gating, likely when
these land on mobile): `MultiDevice`, `AdvancedRemote`, `Sideload`, `FileManager`,
`BackupClone`. Treat their eventual Free/Pro placement as an open design decision, not settled
fact — the design screens (7.1–7.3) imply Pro, but nothing in code enforces that today.

Mobile-specific gating caveat (ARCHITECTURE-REVIEW B6): the do-not-disable safety classifier
runs *before* `require_pro` in every destructive path, and mobile reuses the same core
commands as desktop — so entitlement is client-crackable but the safety gate is independent of
it; a cracked-Pro mobile build still can't brick a device. Real gap is M5 (signed license
validation), which is a revenue concern, not a safety one. Also currently broken: `More.svelte`
fakes entitlement by string-matching, so mobile's actual Free/Pro state is never read from the
backend at all (see 6.2 above) — every gated command above still enforces server-side (Rust)
regardless of what the UI displays.

## Mobile-only concerns (no desktop equivalent)

- **Wireless pairing flow** (1.1/1.2/1.3) — Android TV's wireless-debugging pair/connect dance
  via `wireless_discover`/`wireless_pair`/`wireless_connect`, backed by the Kotlin
  `tauri-plugin-atv-adb` plugin and its libadb transport. Desktop assumes a pre-configured
  `adb` binary and doesn't need this.
- **Saved-TV persistence + auto-reconnect** (1.0) — needs local storage of paired devices
  (host/port/last-known-name/device_type) and a launch-time reconnect attempt with fallback to
  Scan. Entirely unbuilt (ARCHITECTURE-REVIEW F8/T6). The RSA keypair is already persisted
  Kotlin-side, so re-auth after the first pair is silent — only the app-side bookkeeping is
  missing.
- **Safe-area / gesture-nav insets** — `BottomTabs` has no `env(safe-area-inset-bottom)`
  padding, so tab labels sit under the home indicator on gesture-nav Android devices (B5).
  No desktop equivalent (desktop has a title bar, not a bottom tab bar).
- **Offline/bundled fonts** — Material Symbols is shipped as the full 5.34 MB variable font for
  ~35 used glyphs (B2); needs subsetting for APK size. Desktop ships as a native binary with no
  equivalent web-font payload concern.
- **Webview lifecycle** — no `visibilitychange`/resume handler, so the Android webview can go
  blank when the app is backgrounded and resumed (B5). Not a concern for a native Tauri desktop
  window.
- **Transport concurrency** (T1/T2) — the Kotlin `AdbService` wraps every shell call in one
  global mutex, serializing what should be concurrent `dumpsys`/`df` reads and causing
  `health_report` timeouts under load; `exec:` also hardcodes `exit_code = 0` so command-failure
  detection is degraded on this transport specifically. Desktop's `SubprocessAdb` has neither
  issue. Several mobile-only band-aids in shared core (`health.rs` per-command timeouts,
  `apps.rs::list_other_packages_impl` sequential execution) exist only to compensate for this
  and should be reverted once the mutex is fixed (T3).
- **Android release packaging** — no signing keystore, no versionCode bump process, no release
  workflow, `isMinifyEnabled = false`, universal (all-ABI) APK (B4). Desktop has a full,
  working `v2-release.yml` pipeline; mobile has none of this yet.

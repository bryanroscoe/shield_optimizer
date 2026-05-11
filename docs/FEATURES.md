# Shield Optimizer — Feature Catalog

This document is the **source of truth** for what Shield Optimizer does. It exists for two reasons:

1. **Reference for the v2 (Tauri/Rust) rewrite** — a language-agnostic spec of every behavior, every ADB command, and every edge case the current v1 PowerShell implementation handles, so v2 can reach behavioral parity without reverse-engineering.
2. **Living changelog of capabilities** — every PR that adds, removes, or changes a feature is expected to update this doc in the same commit. If a feature isn't here, it doesn't exist as far as the project is concerned.

> **Maintenance rule:** when you add a feature to v1 *or* v2, you update this doc. When the two implementations diverge, that's a regression — flag it in the **Parity** column.

> **Source-of-truth precedence:** if the script and this doc disagree, the script wins for v1 behavior; the doc wins for v2 specification. Resolve drift by updating whichever side is wrong.

Catalog reflects v1 at commit `df70dd5` (post-v0.75.0). Function references point at `Shield-Optimizer.ps1` line numbers from that revision.

---

## Table of contents

0. [Invocation & startup](#0-invocation--startup)
1. [Connection & discovery](#1-connection--discovery)
2. [Main menu](#2-main-menu)
3. [Device action menu](#3-device-action-menu)
4. [Optimization engine](#4-optimization-engine)
5. [Health report & live monitor](#5-health-report--live-monitor)
6. [Launcher management](#6-launcher-management)
7. [Display & input tuning](#7-display--input-tuning)
8. [Display scaling](#8-display-scaling)
9. [Snapshot / restore](#9-snapshot--restore)
10. [APK sideloading](#10-apk-sideloading)
11. [Reboot operations](#11-reboot-operations)
12. [Recovery / safety](#12-recovery--safety)
13. [Device profile & detection](#13-device-profile--detection)
14. [UI framework](#14-ui-framework)
15. [Data catalogs](#15-data-catalogs)
16. [Cross-cutting concerns](#16-cross-cutting-concerns)

---

## 0. Invocation & startup

### 0.1 Command-line parameters

The script accepts parameters on launch:

| Param | Purpose |
|---|---|
| `-ForceAdbDownload` | Force re-download of platform-tools even if already present. Used to recover from corrupt ADB binary. |
| `-LightMode` | Force the light color theme regardless of system setting. |
| `-DarkMode` | Force the dark color theme regardless of system setting. |
| `-Subnet` (string) | Override auto-detected subnet for Scan Network (e.g. `-Subnet "10.0.0"`). Useful for multi-subnet networks or where gateway detection fails. |

Stored as `$Script:ForceAdbDownload`, `$Script:LightMode`, `$Script:DarkMode`, `$Script:Subnet` for use throughout the script.

### 0.2 Platform detection

| | |
|---|---|
| **Purpose** | Identify host OS for ADB binary download, color detection, and OS-specific shell quirks |
| **Behavior** | Sets `$Script:Platform` to `Windows` / `macOS` / `Linux` / `Unknown` via PowerShell 7's `$IsWindows` / `$IsMacOS` / `$IsLinux`. Sets `$Script:IsUnix` = `$Platform -in @("macOS", "Linux")` |

### 0.3 Startup sequence

When the script is invoked:

1. **Strict mode** enabled (`Set-StrictMode -Version Latest`) and `$ErrorActionPreference = "Stop"`.
2. **Platform detection** (§0.2).
3. **Color theme** initialized via `Initialize-Colors` honoring `-LightMode` / `-DarkMode` flags or auto-detecting (§15.1).
4. **ADB lifecycle check** via `Check-Adb`: locates or downloads `platform-tools`, validates binary version, may prompt user.
5. **Optional window resize**: if terminal width < 80 cols, attempts to resize to 100×35. Silently no-ops on terminals that don't support resize (Windows Terminal, VS Code).
6. **Main menu loop** entered (§2).

### 0.4 Strictness conventions

Functions exit on first uncaught error (`-Stop`) and strict mode rejects uninitialized variables. v2 should preserve this strictness for safety — quiet failures during privileged ops are how users brick devices.

---

## 1. Connection & discovery

### 1.1 Network scan

| | |
|---|---|
| **Purpose** | Auto-discover Android TV devices on the local subnet |
| **v1 source** | `Scan-Network` (line 1217), `Get-ArpTable` (1110), `Get-LocalSubnet` (1174), `Get-SubnetFromGateway` (655) |
| **Behavior** | Determine host's local subnet from default gateway, parallel-ping the /24 range, ARP-resolve responders, attempt `adb connect` to each on port 5555, classify each as `device` / `unauthorized` / `offline` |
| **ADB calls** | `adb connect <ip>:5555` per candidate; `adb devices` for state |
| **Concurrency** | Parallel via PowerShell jobs |
| **Edge cases** | Hosts behind firewall return as `offline`; multi-subnet networks need manual entry; macOS/Linux ARP table parsing differs from Windows |

### 1.2 Connect by IP

| | |
|---|---|
| **Purpose** | Manual connection when scan fails or device is on a different subnet |
| **v1 source** | Main menu flow, validated via `Test-ValidIP` (3392) |
| **Behavior** | Prompt for `IP[:port]`, default port 5555, validate as IPv4, `adb connect` |
| **ADB calls** | `adb connect <ip>:<port>` |
| **Edge cases** | Invalid IP rejected with explanation; existing connection on that IP is overwritten |

### 1.3 PIN pairing (Android 11+)

| | |
|---|---|
| **Purpose** | Pair newer Chromecast w/ Google TV and Android 11+ devices that require the pairing-code flow |
| **v1 source** | `Connect-PinPairing` (943) |
| **Behavior** | Prompt user for IP, pair port, and 6-digit PIN from TV; invoke `adb pair`; on success, `adb connect` to the standard 5555 port |
| **ADB calls** | `adb pair <ip>:<pair_port> <pin>`, then `adb connect <ip>:5555` |
| **Status** | Marked **experimental** in v1; works on Google TV devices, not needed for Shield |

### 1.4 Disconnect

| | |
|---|---|
| **Purpose** | Drop a device's ADB connection without affecting others |
| **v1 source** | `Disconnect-Device` (931) |
| **ADB calls** | `adb disconnect <serial>` |

### 1.5 ADB lifecycle management

| | |
|---|---|
| **Purpose** | Ensure ADB binary is present, the right version, and the daemon is healthy |
| **v1 source** | `Check-Adb` (865), `Restart-AdbServer` (917), `Stop-AdbProcess` (847), `Get-AdbConfig` (813), `Get-ScriptDirectory` (805) |
| **Behavior** | First run: downloads platform-tools for the host OS (Linux/macOS/Windows) and extracts to `./platform-tools/` (or alongside script). Detects ADB version mismatch and offers to redownload. Restarts the ADB server via the "Restart ADB" main-menu action (§2) or `-ForceAdbDownload` flag. |
| **Download source** | Google's official `https://dl.google.com/android/repository/platform-tools-latest-<os>.zip` per OS |
| **Edge cases** | Existing ADB process owned by another user blocks server start; ARM Macs need x86_64 ADB under Rosetta for some Google ADB builds; `Stop-AdbProcess` kills `adb` system-wide before redownload to release file locks on Windows |

### 1.6 Device enumeration & friendly names

| | |
|---|---|
| **Purpose** | Convert `adb devices` raw output into a structured list with friendly display names |
| **v1 source** | `Get-Devices` (1012) |
| **Behavior** | Runs `adb devices`, parses each line for serial+status, classifies connection type (`Network` if `IP:port`, else `USB`). For each connected (`status=device`) entry, batches a single ADB call to read: `settings get global device_name; getprop ro.product.brand; getprop ro.product.model; getprop ro.product.device; getprop ro.product.manufacturer`. Then classifies device type and maps codenames to friendly names. |
| **Returned shape** | `@{ Serial; Name; Model; Type; Status; ConnectionType }` per device |
| **Shield codename → friendly model** | `mdarcy` → "Shield TV Pro (2019)", `sif` → "Shield TV (2019 Tube)", `darcy` → "Shield TV (2017)", `foster` → "Shield TV (2015)", default → "Shield TV (\<model code\>)" |
| **Note** | Device-type detection logic here (manufacturer/brand/device-based) differs slightly from the standalone `Get-DeviceType` (brand/model/device-based). v2 should consolidate into one canonical detection. See §13.1. |
| **USB support status** | Detected and tagged `[USB]`, but Shield TV does **not** support USB debugging (host ports only). Marked **experimental** in README. Useful for phones/tablets used during development. |

### 1.7 USB-debugging UNAUTHORIZED guidance

| | |
|---|---|
| **Purpose** | Help user resolve an UNAUTHORIZED device entry instead of leaving them confused |
| **Behavior** | When user selects a device in `unauthorized` state from the main menu, displays detailed numbered steps: (1) look at TV, (2) accept "Allow USB debugging?" prompt, (3) check "Always allow", (4) tap Allow. Falls back to revoke+reconnect guidance if no prompt appears. |
| **v1 source** | Inline in main menu loop (~line 3946) |

---

## 2. Main menu

The outermost menu (looped until Quit). Layout:

```
[Connected devices, one per row, with [NET]/[USB] tag and friendly name]
---
[Static actions below the separator]
```

Devices use numeric shortcuts (1-9 based on position), static actions use letter shortcuts.

### 2.1 Static main-menu actions

| Action | Shortcut | Description |
|---|---|---|
| **Scan Network** | S | Trigger §1.1 auto-discovery |
| **Connect IP** | C | Prompt for `IP[:port]`, validate via `Test-ValidIP` (line 3392), `adb connect` |
| **Pair Device (PIN)** | P | Trigger §1.3 PIN-pairing flow (experimental) |
| **Report All** | R | Run `Run-Report` against every device whose status is `device` |
| **Refresh** | F | No-op; falls through to top of loop which re-queries `Get-Devices` |
| **Restart ADB** | A | Kill and restart the ADB server (§1.5) |
| **Help** | H | Show keyboard/usage help (`Show-Help` at line 1530) |
| **Quit** | Q | Exit script |

### 2.2 Device row selection

- Selecting a device in `device` status drops into the action menu (§3).
- Selecting a device in `unauthorized` state shows the §1.7 guidance.
- Selecting a device in `offline` or other state shows "Cannot manage device in state: \<status\>" then returns to main menu.

---

## 3. Device action menu

Inner menu shown for a connected device. Shortcuts in parens.

| Action | Shortcut | Description | v1 source |
|---|---|---|---|
| **Optimize** | O | Walk through the device-specific bloat + perf list and apply changes | `Run-Task -Mode Optimize` (3095) |
| **Restore** | R | Reverse of Optimize — re-enable disabled packages, reset tuned settings | `Run-Task -Mode Restore` (3095) |
| **Report** | E | Health report (temp/RAM/storage/display/bloat) + optional Live Monitor | `Run-Report` (2066) → `Watch-Vitals` (2301) |
| **Launcher Setup** | L | Install / select / activate a custom launcher | `Setup-Launcher` (2875) |
| **Install APK** | I | Sideload one or more APKs | `Install-Apk` (1409) |
| **Profile** | P | Display device detection results, app-list breakdown | `Show-DeviceProfile` (458) |
| **Recovery** | C | Emergency re-enable of ALL disabled packages | `Run-PanicRecovery` (3405) |
| **Display Scaling** | S | Resolution / density preset switcher | `Set-DisplayScaling` (3461) |
| **Tweaks** | K | HDMI-CEC sub-toggles, match-frame-rate, long-press timeout | `Set-DisplayInputTuning` (3519) |
| **Snapshot** | N | Save / apply state snapshot | `Show-SnapshotMenu` (3778) |
| **Reboot** | T | Normal / recovery / bootloader reboot | `Show-RebootMenu` (3807) |
| **Disconnect** | D | Drop ADB connection (see §1.4) | `Disconnect-Device` |
| **Back** | B | Return to main menu | inline |
| **Quit** | Q | Exit program | inline |

---

## 4. Optimization engine

### 4.1 Optimize / Restore flow

| | |
|---|---|
| **Purpose** | Apply the device-specific app and performance changes in one guided pass |
| **v1 source** | `Run-Task` (3095) |
| **Behavior** | (1) Prompt whether to apply all defaults without per-item confirmation. (2) Query installed/all/disabled package lists. (3) Snapshot the per-app memory map once via `Get-AppMemoryMap`. (4) For each app in the device's list: skip if not installed / already disabled / already uninstalled (in Optimize mode), then prompt for `DISABLE` / `SKIP` / `UNINSTALL` / `ABORT` with default chosen per `DefaultOptimize` field, show "(using X MB RAM)" if currently running. (5) Apply the chosen action. (6) Run Performance Settings (animation triple). (7) Summarize. (8) Offer reboot. |
| **ADB calls (per app)** | `pm disable-user --user 0 <pkg>` / `pm enable <pkg>` / `pm uninstall --user 0 <pkg>` / `cmd package install-existing <pkg>` |
| **ADB calls (settings)** | `settings put global window_animation_scale 0.5` (+ transition_animation_scale, animator_duration_scale at the same value, all three set together) |
| **Restore-mode specifics** | Skips apps that are already enabled (`[ALREADY ACTIVE]`); reinstalls disabled apps; opens Play Store if package file is missing |
| **Memory annotation** | Optimize mode only; threshold 100 MB → yellow, else dark cyan |
| **Risk tiers** | Each app has `Safe` / `Medium` / `High Risk` shown as color-coded tag (green/yellow/red) |
| **Defaults** | Each app has `DefaultOptimize` (Y/N) and `DefaultRestore` (Y/N) that pre-select the prompt; respected in "apply all defaults" mode |
| **Abort behavior** | ESC or selecting ABORT halts the loop, shows partial summary, returns to action menu |
| **Edge cases** | Multi-process apps' RAM summed by base package via `Get-AppMemoryMap`; apps not in `pm list packages` but in `pm list packages -u` shown as `[ALREADY UNINSTALLED]`; uninstall failure causes `Open-PlayStore` prompt |

### 4.2 Task summary

| | |
|---|---|
| **Purpose** | Show counts after Optimize / Restore (full or aborted) |
| **v1 source** | `Show-TaskSummary` (3075) |
| **Behavior** | Reports Disabled / Uninstalled / Skipped / Failed (Optimize); Restored / Skipped / Failed (Restore) |

---

## 5. Health report & live monitor

### 5.1 Health Report

| | |
|---|---|
| **Purpose** | Single-shot snapshot of device vitals + bloat status |
| **v1 source** | `Run-Report` (2066) |
| **Behavior** | Batches multiple `dumpsys` calls into one shell invocation, parses sections delimited by `::SECTION::` markers, displays System Info, Vitals, Settings, Display (mode/refresh/HDR/audio), Top Memory Users, Bloat Check table |
| **ADB call (batched)** | Single `adb shell` with: `dumpsys thermalservice | head -50; dumpsys meminfo; df -h /data; getprop ro.board.platform; getprop ro.build.version.release; settings get global window_animation_scale; pm list packages -e` |
| **Display query (separate)** | `dumpsys display` (resolution / refresh from supportedModes table matched to active modeId; HDR from `HdrCapabilities mSupportedHdrTypes`), `dumpsys audio` (current output device) — see §5.3 |
| **Vitals shown** | Temperature (parsed from thermal zones), RAM (free/used/total/swap from meminfo), Storage (from df) |
| **Vital color coding** | `Get-VitalColor` thresholds — temp: <50/<70/<85/else; RAM: <60/<75/<85/else %; storage: similar; per-app memory: <50/<100/<200/else MB |
| **Bloat Check table** | One row per app in the device's list, columns: Name / RAM (if running) / Action (UNINSTALL/DISABLE) / Default (yes/no) |
| **Edge cases** | Stale dumpsys output if device just woke; missing thermal sensors → `N/A`; storage parsing tolerant of `/data` mount variations |

### 5.2 Live Monitor

| | |
|---|---|
| **Purpose** | Auto-refreshing dashboard for temp / RAM / top processes |
| **v1 source** | `Watch-Vitals` (2301) |
| **Behavior** | Loop polling `dumpsys thermalservice` + `dumpsys meminfo`, redraws in place at ~2s interval, ESC to exit |
| **ADB calls (per tick)** | `dumpsys meminfo 2>/dev/null; echo '::SEP::'; dumpsys thermalservice 2>/dev/null | head -30` (batched) |
| **Top apps** | Filtered via `Test-AppPackage` (skips system processes); top 10 shown |
| **Edge cases** | Long-running session leaks ADB connections on Windows if not cleaned; PowerShell cursor positioning differs by terminal |

### 5.3 Display & audio diagnostics

| | |
|---|---|
| **Purpose** | Show actual negotiated display mode so users can verify 4K@60 vs lower fallback |
| **v1 source** | `Get-DisplayMode` (731), called from `Run-Report` |
| **Behavior** | Parse `dumpsys display` for active `modeId`, look up that id in the `supportedModes` table for width/height/fps; parse `HdrCapabilities mSupportedHdrTypes=[...]` and decode 1=Dolby Vision, 2=HDR10, 3=HLG, 4=HDR10+; parse `dumpsys audio` first `Devices:` line for output device |
| **Outputs** | `Resolution`, `RefreshRate`, `HdrTypes` (comma-joined or `SDR only`), `AudioDevice` (uppercased) — each may be `$null` |
| **Edge cases** | dumpsys format varies by Android version — current parser tested on Shield Android 11; audio device detection is best-effort, may be empty on some builds |

---

## 6. Launcher management

### 6.1 Custom launcher catalog

Supported preset launchers (`$Script:Launchers`):

| Display name | Package |
|---|---|
| Projectivy Launcher | `com.spocky.projengmenu` |
| FLauncher | `me.efesser.flauncher` |
| ATV Launcher | `com.sweech.launcher` |
| Wolf Launcher | `com.wolf.firelauncher` |
| AT4K Launcher | `com.overdevs.at4k` |
| Dispatch Launcher | `com.spauldhaliwal.dispatch` |

A **"Custom..."** menu option lets the user type any package id (validated against package-name regex, must be installed).

Stock launchers handled (`$Script:StockLaunchers`): `com.google.android.tvlauncher`, `com.google.android.apps.tv.launcherx`, `com.google.android.leanbacklauncher`, `com.amazon.tv.launcher`.

Safe HOME-handler fallbacks (never disabled): `com.android.tv.settings`, `com.android.settings`.

### 6.2 Setup wizard

| | |
|---|---|
| **Purpose** | Install, select, and activate a custom launcher; disable stock launchers safely |
| **v1 source** | `Setup-Launcher` (2875) |
| **Behavior** | (1) Detect current launcher via `cmd package resolve-activity`. (2) List preset launchers + Custom + Back. (3) For chosen launcher: if not installed and not custom, offer Play Store install. (4) If already active, just offer to clean up other HOME handlers. (5) Else prompt to disable other launchers (calls `Disable-AllStockLaunchers`). (6) Call `Set-DefaultLauncher` and report success/failure with captured ADB error. (7) Run `Test-ChannelDependencies` to warn if `com.android.providers.tv` is disabled (Watch Next plumbing). |
| **ADB calls** | See `Set-DefaultLauncher` strategy in §6.3 |

### 6.3 Set default launcher (multi-strategy)

| | |
|---|---|
| **Purpose** | Programmatically promote a package to the default HOME app across Android versions and customized builds |
| **v1 source** | `Set-DefaultLauncher` (2820), `Set-HomeRoleHolder` (2491), `Get-HomeRoleHolder` (2473), `Get-LauncherActivity` (2762), `Get-CurrentLauncher` (2742) |
| **Strategy order** | (1) `pm enable <pkg>` — unblock previously-disabled launchers. (2) Role API: `cmd role add-role-holder android.app.role.HOME <pkg>`, verify by read-back, skip immediately if "Unknown command". (3) For each activity candidate (dumpsys-discovered first via `cmd package query-activities --components`, then `.MainActivity` / `.Main` / `.LauncherActivity` / `.HomeActivity` guesses): try `cmd package set-home-activity --user 0 <comp>` then `pm set-home-activity --user 0 <comp>`, verify after each. (4) Last resort: `am start -W -a android.intent.action.MAIN -c android.intent.category.HOME` to kick the system selector when only one launcher remains enabled. |
| **Error capture** | `$Script:LastSetHomeError` captures the underlying ADB response so the UI can display *why* the set failed |
| **Verified on** | Real Shield running Android 11 — Projectivy `.ui.home.MainActivity` |
| **Edge cases** | `cmd role` doesn't exist on Shield's customized Android 11; activity may be deeply nested (`.ui.home.MainActivity` etc.) — dumpsys parser was unreliable, replaced by `query-activities --components` |

### 6.4 Disable stock launchers

| | |
|---|---|
| **Purpose** | Disable every HOME-capable app except the chosen custom launcher and safe fallbacks |
| **v1 source** | `Disable-AllStockLaunchers` (2514), `Get-HomeHandlers` (2446) |
| **Behavior** | Query all HOME-capable apps via `cmd package query-activities -a android.intent.action.MAIN -c android.intent.category.HOME`; add known-problematic handlers (`setupwraith`, `droidlogic.launcher.provider`); skip already-disabled and safe-fallback packages; prompt per-launcher YES/NO; disable accepted via `pm disable-user --user 0`. Sets the HOME role holder first (if role API works). |
| **Friendly names** | Resolves package → display name via custom-launcher list + hardcoded stock-launcher map |
| **Edge cases** | Refuses to proceed if no HOME handlers detected; never disables the package supplied as `-CustomLauncherPkg`; never disables `com.android.tv.settings` (emergency fallback) |

### 6.5 Restore stock launchers

| | |
|---|---|
| **Purpose** | Re-enable every HOME-capable app that was previously disabled |
| **v1 source** | `Restore-AllStockLaunchers` (2668) |
| **ADB calls** | `pm enable <pkg>` for each disabled stock launcher; sets HOME role back to `com.google.android.tvlauncher` on Shield / equivalent on Google TV if possible |

### 6.6 Channel dependency warning

| | |
|---|---|
| **Purpose** | Warn user if `com.android.providers.tv` is disabled — without it, no streaming app can publish Watch Next / Continue Watching channels |
| **v1 source** | `Test-ChannelDependencies` (called from `Setup-Launcher`) |
| **Behavior** | Check disabled-package list for `com.android.providers.tv`; if found, surface warning and offer to re-enable via `pm enable` |

---

## 7. Display & input tuning

| | |
|---|---|
| **Purpose** | Toggle individual display/input settings outside the Optimize flow |
| **v1 source** | `Set-DisplayInputTuning` (3519), `Set-BoolSetting` (3615) |
| **Settings exposed** | HDMI-CEC master / auto-wake-TV / auto-off-TV / audio routing (global namespace); `match_content_frame_rate` (secure, values 0/1/2 = Never/Seamless/Always); `long_press_timeout` (secure, ms, presets 300/400/500) |
| **ADB calls (read)** | `settings get global <key>` / `settings get secure <key>` |
| **ADB calls (write)** | `settings put <ns> <key> <value>` / `settings delete <ns> <key>` for reset-to-default |
| **UX** | Sub-menu shows current values for all six settings; user picks one to change; per-setting prompts give ON/OFF/Reset/Cancel (for bools) or value list (for enums) |
| **Verified** | All key/namespace combinations tested against Shield Android 11 |

---

## 8. Display scaling

| | |
|---|---|
| **Purpose** | Switch resolution and density between presets (4K, 1080p, default) |
| **v1 source** | `Set-DisplayScaling` (3461) |
| **Presets** | Shield TV 4K = 3840x2160 / density 540; Shield TV 1080p = 1920x1080 / density 320; Reset to Default |
| **ADB calls** | `wm size <WxH>` / `wm density <DPI>` / `wm size reset` / `wm density reset` |
| **Edge cases** | Applies to all device types but presets are Shield-tuned; non-Shield users may need manual density tuning |

---

## 9. Snapshot / restore

### 9.1 Snapshot data model

JSON file at `./snapshots/<safe-device-name>_<timestamp>.json`:

```json
{
  "schemaVersion": 1,
  "savedAt": "2026-05-08T22:30:00Z",
  "deviceName": "...",
  "deviceSerial": "<ip>:5555",
  "deviceType": "Shield" | "GoogleTV" | "Unknown",
  "androidVersion": "11",
  "disabledPackages": ["com.foo", ...],
  "currentLauncher": "com.spocky.projengmenu",
  "settings": { "<ns>.<key>": "<value>", ... }
}
```

Tracked setting keys (`$Script:SnapshotSettingKeys`):
- `global`: `window_animation_scale`, `transition_animation_scale`, `animator_duration_scale`, `hdmi_control_enabled`, `hdmi_control_auto_wakeup_enabled`, `hdmi_control_auto_device_off_enabled`, `hdmi_system_audio_control_enabled`
- `secure`: `match_content_frame_rate`, `long_press_timeout`

### 9.2 Save

| | |
|---|---|
| **v1 source** | `Save-Snapshot` (3653) |
| **Behavior** | Enumerate disabled packages (`pm list packages -d`), capture current launcher, read all tracked settings (skip values that are `null` or empty), write JSON with timestamp |

### 9.3 Apply

| | |
|---|---|
| **v1 source** | `Apply-Snapshot` (3698) |
| **Behavior** | List snapshots sorted newest-first, preview the chosen one, confirm. (1) For each package in `disabledPackages`: if installed and not currently disabled, run `pm disable-user --user 0 <pkg>`. (2) Run `Set-DefaultLauncher` with saved launcher (works through full strategy chain). (3) For each setting: `settings put <ns> <key> <value>`. Summary with disabled / already-disabled / not-on-device / launcher result / settings-applied counts. |
| **Safety property** | **Never re-enables** a currently-enabled package not on the snapshot list. Apply is additive-only for disable state. |
| **Cross-device warning** | If `snap.deviceType ≠ current device type`, warns user before proceeding |

### 9.4 Menu

| | |
|---|---|
| **v1 source** | `Show-SnapshotMenu` (3778) |
| **Options** | Save current state / Apply a saved snapshot / Open snapshots folder / Back |

---

## 10. APK sideloading

| | |
|---|---|
| **Purpose** | Install one or more local APK files |
| **v1 source** | `Install-Apk` (1409), `Install-ApkFile` (1345), `Get-ApkFiles` (1329) |
| **Behavior** | Discover APKs in `./apks/` (or user-supplied path), show file list with sizes, install each via `adb install` with optional `-r` for reinstall |
| **ADB calls** | `adb install [-r] <path>` |
| **Edge cases** | `INSTALL_FAILED_VERSION_DOWNGRADE` surfaces hint to enable downgrade; `INSTALL_FAILED_INSUFFICIENT_STORAGE` surfaces storage hint |

---

## 11. Reboot operations

| | |
|---|---|
| **Purpose** | Restart the device with optional special modes |
| **v1 source** | `Show-RebootMenu` (3807) |
| **Options** | Normal Reboot / Recovery Mode / Bootloader / Cancel |
| **ADB calls** | `adb reboot` / `adb reboot recovery` / `adb reboot bootloader` |

---

## 12. Recovery / safety

| | |
|---|---|
| **Purpose** | Emergency re-enable of every disabled package — single command for "I broke my Shield" |
| **v1 source** | `Run-PanicRecovery` (3405) |
| **Behavior** | Query `pm list packages -d`, iterate and `pm enable <pkg>` each one, report restored/failed counts, recommend reboot |
| **Reversibility** | This is the back-out for any Optimize, Snapshot Apply, or Launcher Setup change |

---

## 13. Device profile & detection

### 13.1 Detection logic

v1 has **two device-type detection paths** that v2 should consolidate:

**Path A: `Get-DeviceType` (line 384)** — used by `Show-DeviceProfile` only:
- Inputs: `ro.product.brand`, `ro.product.model`, `ro.product.device`
- Classification:
  - `Shield` if brand=nvidia OR model matches /shield/ OR device matches /foster\|darcy\|mdarcy\|sif/
  - `GoogleTV` if brand=onn OR model matches /onn/ OR device matches /ott_/ OR brand=google OR model matches /chromecast\|sabrina\|boreal/
  - Falls back to `Unknown`

**Path B: inline detection in `Get-Devices` (line 1043)** — used for every device list refresh:
- Inputs: `ro.product.manufacturer`, `ro.product.brand`, `ro.product.device`
- Classification (different criteria than Path A):
  - `Shield` if manufacturer matches /NVIDIA/ OR brand matches /NVIDIA/
  - `GoogleTV` if manufacturer matches /Google\|Amlogic/ OR brand matches /onn\|google/ OR device matches /ott_\|sabrina\|boreal/
  - Falls back to `Unknown`

The two paths can disagree on edge cases. v2 should pick the union of inputs (manufacturer + brand + model + device) and have one detection function.

### 13.2 Profile display

| | |
|---|---|
| **Purpose** | Read-only view of detection results, current settings, and the device-specific app list |
| **v1 source** | `Show-DeviceProfile` (458), `Get-AppListForDevice` (429), `Get-DeviceTypeName` (420) |
| **All getprop inputs** | `ro.product.brand`, `ro.product.model`, `ro.product.device`, `ro.product.manufacturer`, `ro.build.version.release`, `ro.build.version.sdk`, `ro.build.id`, `ro.board.platform` |
| **Other inputs** | `settings get global device_name` for user-customized friendly name |
| **App list per device type** | Shield → CommonAppList + ShieldAppList. GoogleTV → CommonAppList + GoogleTVAppList. Unknown → CommonAppList only |
| **Shield codename → friendly model map** | See §1.6 |

---

## 14. UI framework

### 14.1 Themes

| | |
|---|---|
| **Purpose** | Light and dark color schemes, auto-detected from system or forced via `-LightMode` / `-DarkMode` |
| **v1 source** | `Initialize-Colors` (58), `Test-SystemLightMode` (37) |
| **Auto-detect** | macOS: `defaults read -g AppleInterfaceStyle`; Windows: registry `AppsUseLightTheme`; Linux: `gsettings color-scheme` |
| **Color slots** | Header / SubHeader / Success / Warning / Error / Info / Selected / Unselected / Bracket highlight / Separator / Label / Value / TextDim / Disabled / Shortcut (15 named colors per theme) |

### 14.2 Read-Menu

| | |
|---|---|
| **Purpose** | The keyboard-navigated list selector used everywhere |
| **v1 source** | `Read-Menu` (1781) |
| **Inputs** | `Title`, `Options[]`, `Descriptions[]`, optional `DefaultIndex`, optional `StaticStartIndex` (dynamic + static split for device list), optional `Shortcuts[]` |
| **Behavior** | Renders options, supports arrow-key navigation, number/letter shortcuts, ESC to cancel; shows description of focused item below; redraws in-place using `MoveTo` cursor positioning. Highlighted shortcut character is bracketed inline for letter shortcuts and prefixed for digit shortcuts (digits-on-IPs ambiguity). Separators (`---`) render as visual dividers. |
| **Returns** | Integer index of selected option, or `-1` on ESC |
| **Edge cases** | UNAUTHORIZED items rendered in error color; closing-arrow indicator on selected row; works under PowerShell on Windows Terminal / iTerm / GNOME Terminal |

### 14.3 Read-Toggle

| | |
|---|---|
| **Purpose** | YES/NO/ABORT-style horizontal toggle |
| **v1 source** | `Read-Toggle` (2002) |
| **Inputs** | `Prompt`, `Options[]` (2-3 entries), `DefaultIndex` |
| **Behavior** | Left/Right arrows to move; Enter confirms; first-letter shortcuts |
| **Returns** | Integer index or `-1` on ESC |

### 14.4 Help

| | |
|---|---|
| **Purpose** | In-app keyboard shortcut reference |
| **v1 source** | `Show-Help` (1530) |
| **Content** | Arrow / number / letter / Enter / ESC bindings |

---

## 15. Data catalogs

These are the structured arrays that drive Optimize behavior. **In v2 they should be runtime-fetched JSON, not embedded code.**

### 15.1 CommonAppList — universal Android TV bloat

Telemetry, defunct apps, streaming apps (UNINSTALL, DefaultOptimize=N — user choice), Canadian streaming apps, FAST apps, premium add-ons, medium-risk system shims, high-risk launcher/critical-service entries. Currently ~40 entries.

**Defunct apps section:** Google Play Movies, Google Play Music, Funimation, Stadia, Quibi, HBO Now (legacy).

### 15.2 ShieldAppList — Shield-specific

`com.nvidia.stats`, `com.nvidia.diagtools`, `com.nvidia.feedback`, `com.google.android.tvrecommendations`, `com.nvidia.osc`, `com.nvidia.shieldtech.hooks`, `com.nvidia.tegrazone3`, `com.nvidia.nvgamecast`, `com.google.android.backdrop`, `com.google.android.speech.pumpkin`, `com.nvidia.ota` (High Risk), `com.plexapp.mediaserver.smb` (Advanced), `com.google.android.tvlauncher` (High Risk, requires wizard).

### 15.3 GoogleTVAppList — Google TV / Onn / Chromecast-specific

`com.walmart.otto`, `com.google.android.leanbacklauncher.recommendations`, `com.google.android.tungsten.overscan`, `com.droidlogic.launcher.provider`, `com.google.android.apps.tv.launcherx` (High Risk, requires wizard).

### 15.4 App entry schema

```
@{
  Package = "com.example.app"
  Name = "Display Name"
  Method = "UNINSTALL" | "DISABLE"
  Risk = "Safe" | "Medium" | "High Risk" | "Advanced"
  OptimizeDescription = "Short why-disable."
  RestoreDescription  = "Short why-restore."
  DefaultOptimize = "Y" | "N"   # pre-selected action in Optimize prompt
  DefaultRestore  = "Y" | "N"   # pre-selected action in Restore prompt
}
```

### 15.5 PerfList — performance settings

Currently single entry: animation speed (window/transition/animator scales set together, Optimize=0.5, Restore=1.0).

### 15.6 Custom launchers — see §6.1

### 15.7 Snapshot setting keys — see §9.1

---

## 16. Cross-cutting concerns

### 16.1 ADB command invocation

| | |
|---|---|
| **v1 source** | `Invoke-AdbCommand` (675) |
| **Behavior** | Wraps `& $Script:AdbPath -s $Target shell <cmd>` and returns `@{ Success; Output; Error }`. Catches exceptions, normalizes output to string. |
| **v2 expectation** | Single point for ADB calls; should add structured logging, optional dry-run mode, request/response tracing for debugging |

### 16.2 Multi-device sessions

The script supports multiple connected devices simultaneously. The main menu (§2) shows all `adb devices` results with shortcuts (numeric for device rows, letter for static actions). Selecting a device drops into its action menu (§3).

### 16.3 Failure surfacing

- ADB stderr captured into a `LastSetHomeError`-style scoped variable (currently only for launcher set-default; v2 should generalize this pattern)
- Failed operations don't halt the loop — they're counted in the summary
- `Write-ErrorMsg` / `Write-Warn` go to the user's terminal in distinct colors

### 16.4 Reversibility model

- **Disable** (`pm disable-user`) — reversible via Restore / Recovery
- **Uninstall** (`pm uninstall --user 0`) — semi-reversible via `pm install-existing` (works if APK is still on the system partition) or Play Store
- **Settings writes** — reversible via Restore (resets to known values) or by writing `null` / deleting the key
- **Snapshot Apply** — additive-only for disable state; safe to re-apply

### 16.5 Play Store deep-link

| | |
|---|---|
| **Purpose** | Open the Play Store on the device to a specific package, used as a fallback path when reinstalling a previously-uninstalled app |
| **v1 source** | `Open-PlayStore` (1310) |
| **ADB call** | `am start -a android.intent.action.VIEW -d 'market://details?id=<pkg>'` |
| **Used by** | Optimize/Restore flow (when `pm install-existing` fails because the APK file is missing); Launcher Setup (when chosen custom launcher isn't installed) |

### 16.6 Uninstall / install error decoding

| | |
|---|---|
| **Purpose** | Translate Android's terse install/uninstall failure codes into user-readable hints |
| **v1 source** | `Get-UninstallErrorReason` (662) |
| **Recognized errors** | `INSTALL_FAILED_INSUFFICIENT_STORAGE`, `INSTALL_FAILED_VERSION_DOWNGRADE`, `INSTALL_FAILED_OLDER_SDK`, `INSTALL_FAILED_ALREADY_EXISTS`, `DELETE_FAILED_DEVICE_POLICY_MANAGER`, `DELETE_FAILED_USER_RESTRICTED`, and others — each mapped to a one-line explanation |

### 16.7 Utility helpers

| Function | Purpose |
|---|---|
| `Test-PackageInList` (555) | Boolean: is `<pkg>` present in the multiline output of `pm list packages [-d/-e/-u]`? Centralizes the parsing of `package:<name>` format |
| `Test-AppPackage` (564) | Boolean: is this package a user-installable app (vs. a system process)? Filters `dumpsys meminfo` rows down to apps. |
| `Test-ValidIP` (3392) | IPv4 + optional `:port` validator |
| `Format-FileSize` (1321) | Bytes → human-readable string (KB/MB/GB) for APK file listings |
| `Get-ParsedTemperature` (570) | Extracts CPU temp in °C from `dumpsys thermalservice` across multiple format variants |
| `Get-ParsedRamInfo` (594) | Extracts free/used/total/swap MB from `dumpsys meminfo` summary block |
| `Get-VitalColor` (625) | Maps a vital metric (Temperature / RAM / Storage / AppMemory) + value to a console color name based on threshold tiers |
| `Get-VitalAnsiColor` (640) | Same as above but returns an ANSI color code for cursor-positioned UIs |
| `Open-PlayStore` (1310) | See §16.5 |
| `Get-UninstallErrorReason` (662) | See §16.6 |

### 16.8 Code conventions used in v1 (not requirements for v2)

- All PowerShell function names are `Verb-Noun`
- `$Script:` scope for module-globals (theme, ADB path, summary counters)
- ANSI escape sequences for cursor control (cross-platform via PowerShell's RawUI)
- Inline error checking: every ADB call's result inspected for `Exception|Error|denied` patterns
- All `pm` commands use `--user 0` explicitly to avoid ambiguity on multi-user builds (Android TV rarely is, but the safety is cheap)

---

## Maintenance checklist (for PR authors)

When opening a PR that affects user-visible behavior, **at minimum**:

- [ ] Add or update the corresponding section here
- [ ] If you added a new menu item, update §2 (main menu) or §3 (device action menu)
- [ ] If you added a new ADB call, add it to the relevant feature's "ADB calls" row
- [ ] If you added a new setting key, update §7 (Tweaks) and §9.1 (snapshot setting keys)
- [ ] If you added a new app or list, update §15 (data catalogs)
- [ ] If you fixed a previously-broken edge case, update the "Edge cases" row to reflect the fix
- [ ] Cross-link from v1 implementation to this doc by referencing section numbers in commit messages where useful

If v2 (Rust/Tauri) work is in flight, also note which sections have parity vs. v1 in a status table here (to be added once v2 has structure).

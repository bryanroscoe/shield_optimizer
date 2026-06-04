# Changelog — Shield Optimizer v2

This file is the authoring surface for v2 release notes. The release workflow
(`.github/workflows/v2-release.yml`) looks for a section matching the tag
being released (e.g. `## v2-0.1.0-beta`) and uses its contents as the body of
the GitHub Release. If no section is found, the workflow falls back to
auto-generated notes from `git log`.

Tag conventions:

- `v2-0.1.0` — stable release.
- `v2-0.1.0-beta`, `v2-0.1.0-beta.2`, `v2-0.1.0-rc1`, `v2-0.1.0-alpha`,
  `v2-0.1.0-preview`, `v2-0.1.0-pre` — auto-flagged as **prerelease** by the
  workflow.

When you add a new section, put it at the top; older releases go below.

---

## v2-0.1.0-beta.7

### Changed

- **Per-row action dropdown in the Optimize / Restore wizard.** The old
  Apply/Skip checkbox is now a dropdown so you can choose what happens to each
  app: **Disable / Uninstall / Skip** (Enable / Skip in Restore mode). It
  defaults to the catalog's recommended action — so you can downgrade an
  Uninstall to a safer Disable, or skip individual rows, without giving up the
  recommendation. The do-not-disable safety list still gates every action.

### Fixed

- **Accurate success reporting for device actions.** Apply-snapshot,
  disable/restore-stock-launchers, and Emergency Recovery judged success by
  scanning only stdout — but `adb shell` exits 0 even when the on-device
  command fails, and `pm` / `settings` write failures to stderr on many builds.
  They now inspect both streams, so a package or setting that didn't actually
  change is reported as failed instead of silently "done."
- **Apply-snapshot no longer claims settings were written when they weren't.**
  Settings are now applied one at a time and each is checked, so the summary
  reflects what actually took (previously every key was reported written as
  long as the shell ran).
- **Hardened package-name and snapshot-value handling.** Package names from
  manual-entry paths are validated before they reach the shell, and snapshot
  setting values are rejected if they contain shell metacharacters (matching
  the existing Tweaks guard).
- Minor: the launcher-set error message no longer gets overwritten by benign
  empty output, and per-device state is fully reset if the device view ever
  switches devices without unmounting.

---

## v2-0.1.0-beta.6

### Fixed

- **Display Scaling now works.** Clicking Shield 4K / 1080p failed silently with
  "unknown variant `uhd_4k`" — the frontend and backend disagreed on the preset
  names. Fixed the mismatch so the presets apply.
- **Correct Shield 4K scaling values** (#24). The 4K preset is now
  **3839×2160 @ density 640** (was 3840×2160 @ 540). Shield TV rejects a 3840
  width, and density 540 broke some app menus (Disney+, HBO). Applied to both
  the v2 app and the v1 PowerShell script.
- **"Open folder" on the Snapshots page works.** It used an `open_path` call the
  app wasn't permitted to make; it now reveals the snapshot folder in the system
  file manager (and the folder is created up front so it works before your first
  snapshot).

### Added

- **Light-theme gallery** in the README alongside the dark one, and the
  screenshot pipeline now captures both themes.

---

## v2-0.1.0-beta.5

### Fixed

- **Light mode is now readable, and you can pick a theme.** On Linux/macOS in
  light appearance, the cards and text kept dark-mode colors against a light
  page, making text nearly invisible (#26). The whole UI is now theme-aware via
  CSS variables, with a proper light palette. A **Light / Dark / Auto** toggle
  in the header lets you override the system appearance (Auto follows the OS).
  Dark mode is unchanged.

---

## v2-0.1.0-beta.4

### Fixed

- **Restart ADB now reconnects network devices.** `adb kill-server` drops every
  TCP connection, and network devices (`ip:port`) don't re-attach on their own
  the way USB devices do — so after a restart the device list came back empty
  even though the daemon restarted fine. Restart ADB now remembers the
  connected network devices and reconnects them automatically, and reports
  which ones came back (or which to reconnect manually via Scan Network /
  Connect IP). (Reddit report.)
- **Windows: installers now upgrade in place.** Previously the `.msi` made you
  uninstall the old version first. The MSI version encoded the release counter
  in the 4th version field — which Windows Installer ignores for upgrade
  detection — so every build looked like the same version. The build/3rd field
  now carries a strictly-increasing, semver-ordered value, so new releases
  install on top of the old one. (First applies upgrading **from** this build
  forward.)
- **Network scan now connects the devices it finds.** The scan could report
  "found N devices, connected 0" — the port sweep detected them, but the
  follow-up `adb connect` raced a cold adb daemon and failed (which is why a
  manual Restart ADB then made them connect). The scan now starts the adb
  server first and retries each connect once, so found devices connect on the
  first pass. (Reddit report.)

---

## v2-0.1.0-beta.3

### Fixed

- **Windows: no more console-window "waterfall."** Every adb call (and the
  network-scan gateway lookup) now spawns with `CREATE_NO_WINDOW`, so the app
  no longer flashes a `cmd` window for each command. Previously, pressing
  anything that ran multiple adb commands popped a cascade of console windows
  open and closed. macOS/Linux were never affected. (Thanks to the Reddit
  report.)

---

## v2-0.1.0-beta.2

Plumbing-only release to exercise the new Homebrew tap auto-bump path —
no application changes since `v2-0.1.0-beta.1`. The release workflow now
ends with a `bump-tap` job that recomputes the universal DMG's SHA256
and pushes a `version` + `sha256` bump to
[`bryanroscoe/homebrew-shield-optimizer`](https://github.com/bryanroscoe/homebrew-shield-optimizer),
so `brew upgrade --cask shield-optimizer` tracks releases automatically.

macOS users can now install with:

```sh
brew tap bryanroscoe/shield-optimizer
brew install --cask shield-optimizer
```

The cask strips the quarantine bit in a postflight, so the app opens
with a normal double-click — no Gatekeeper prompt despite the unsigned
build.

---

## v2-0.1.0-beta.1

Re-cut of the first public beta to fix a Windows MSI bundling failure.

The original `v2-0.1.0-beta` build failed on the Windows runner with
`optional pre-release identifier in app version must be numeric-only and
cannot be greater than 65535 for msi target` — WiX requires a numeric
pre-release identifier. Fixed by setting `bundle.windows.wix.version` to
`0.1.0.1` (MSI-only override) while keeping `0.1.0-beta.1` as the human
version everywhere else. Linux and macOS builds were unaffected.

No other changes since `v2-0.1.0-beta` — feature set identical.

---

## v2-0.1.0-beta

First public beta of the Tauri rewrite. Feature parity with v1's PowerShell
script across every section of `docs/FEATURES.md` (see HANDOFF §2A for the
row-by-row match-up). Installable on macOS, Windows, and Linux — no
PowerShell, no terminal, no scripting required.

### What's new vs. v1

- **Native desktop app** built on Tauri 2 + Rust + Svelte 5. Same engine
  driving the same `adb` commands, but a GUI you click instead of a TUI
  you type into.
- **Optimize / Restore wizard** as a real interactive tab — review every
  app's recommended action with its current state and RAM usage, untick
  anything you want to keep, then Run with live per-row progress.
- **Cross-device snapshot apply** with a global Snapshots page. Save a
  device's state, then apply that snapshot to a different device from
  the same UI.
- **Real safety guardrails**: a curated do-not-disable list (system UI,
  shell, package installer, GMS, etc.) prevents brick-tier disables from
  every code path that could issue one — including the apply-snapshot
  flow, the stock-launcher wizard, and the memory-table Disable button.
- **APK auto-discovery** — point at a folder once and every `.apk`
  inside shows up with a one-click Install.
- **Live Health Report** with parsed display mode, HDR support, audio
  device, RAM, storage, and the top 20 memory consumers with risk tags.
- **Per-launcher actions**: install via Play Store, enable, disable, set
  as default. Multi-strategy promotion (role API → set-home-activity →
  HOME intent kick) handles Shield's customized Android 11 quirks.
- **Auto-installs ADB** on first launch if `adb` isn't on PATH — no
  manual platform-tools download required.
- **Network scan on boot**, PIN pairing for Android 11+, Restart ADB,
  Report All, Emergency Recovery (re-enable every disabled package), all
  ported from v1.

### Known limitations in this beta

- Unsigned binaries — first launch shows the Gatekeeper / SmartScreen
  warning. See dismissal steps below.
- No auto-updater yet. Future releases will need a manual download.
- Flatpak not built; use the `.AppImage` on Linux.

### What v1 users should know

The PowerShell script (`Shield-Optimizer.ps1`) still works and still gets
maintained on the `v0.x.x` tag track. v2 doesn't replace it — it sits
beside it. If you'd rather stay on the TUI, nothing changes for you.

---

<!-- Add new sections above this line. Older releases go below. -->

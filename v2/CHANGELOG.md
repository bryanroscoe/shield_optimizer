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

## v2-2.1.0

Launcher switching is now fast and reliable, with much clearer feedback across
the app.

### Launchers

- **Reliable switch away from the stock launcher.** On builds where an enabled
  stock launcher overrides the normal launcher-switch commands (e.g. Shield /
  Android 11), switching now goes straight to disabling stock — the only thing
  that actually hands HOME over — instead of grinding through several seconds of
  commands that report success but don't take effect. Your other launchers are
  never touched.
- **One click on a disabled launcher.** "Set as default" used to do nothing on a
  disabled launcher; it now reads **"Enable & set default"** and enables it
  first.
- **It opens the new launcher** on the TV the moment the switch succeeds, instead
  of leaving you on the old screen until you press Home.
- **Live, per-step status** while a switch runs, shown right in the launcher's
  row, and the list refreshes itself when done — no manual Refresh.

### App & memory actions

- Force stop / Disable buttons show a spinner and a clear label while working,
  and confirm what happened in plain language (friendly names, not raw package
  ids).
- Disabling an app from the Memory tab removes its row immediately.

### Everywhere

- **Cross-tab consistency** — an action in one tab (e.g. disabling a launcher
  from the Memory tab) now updates the others instead of leaving them stale.
- **Automatic updates now default to off** — opt in from Settings if you want
  them.

---

## v2-2.0.0

The first stable release of the v2 rewrite — Shield Optimizer is a full
Rust/Tauri desktop app for debloating and tuning Android TV devices, no
PowerShell required. Everything from the beta line, now stable:

### Highlights

- **One-click device management** over ADB — auto-discovers devices on USB and
  the local network (including Android 11+ wireless debugging), with guided
  on-TV authorization.
- **Optimize wizard** with safe, reversible recommendations: per-app RAM and
  last-used cues, a "remove if unused" review tier, and a mandatory
  do-not-disable safety gate so you can't brick a device.
- **Snapshots** — capture and restore the full disable/launcher/settings state.
- **App List, Launcher takeover, Install APK, Files, Health report.**
- **Instant Remote control** — a low-latency scrcpy channel (key presses in
  milliseconds), full-UTF-8 typing, hold-to-repeat D-pad, Recents and Settings
  buttons, with a compatible-mode fallback.
- **Tweaks** — HDMI-CEC, display scaling, frame-rate matching, background
  process limit, the Nvidia system-hooks and Assistant-mic toggles, and
  **Private DNS (DNS-over-TLS)** with a dead-host safety revert.
- **In-app auto-update** — signed updates with an auto-update toggle and an
  "Update now" button.
- Cross-platform: macOS (universal .dmg + Homebrew tap), Windows (.msi/.exe),
  Linux (.deb/.AppImage/.rpm).

## v2-2.0.0-beta.15

### Added

- **Instant Remote control.** The Remote tab now drives the TV over a persistent
  scrcpy control channel instead of a fresh ADB call per press — key presses go
  from ~0.7s to near-instant, typing supports full UTF-8, and holding a D-pad
  direction repeats. A live **● instant / ○ compatible** cue shows which transport
  is active, with a **Force compatible mode** toggle as a fallback.
- **Recents and Settings buttons** on the Remote. Settings opens via an intent
  (the Shield's gear/hamburger button), so it works where a raw keycode doesn't.
- **Private DNS (DNS-over-TLS)** in Tweaks — Off / Automatic / Custom hostname,
  with a safety net that reverts a dead custom host back to automatic so the
  device never loses DNS.

### Fixed

- **Wireless-debugging devices** (Android 11+) are now labeled **Network**, not USB.
- **Google Pixel phones are no longer mislabeled as Google TV** — device type is
  detected from the actual TV build characteristic, not brand alone.

## v2-2.0.0-beta.14

### Added

- **Auto-updater** — the app now checks for updates on launch using Tauri's
  signed updater. An "Auto-update" checkbox in the header controls whether
  updates download automatically; an "Update now" button appears when one is
  available. Builds are signed with an Ed25519 key.
- **Network scan after ADB install** — installing platform-tools now
  automatically scans the local network for devices, matching boot behavior.
- **Dev tooling** — `Makefile` (`make dev`, `make brew`, `make setup`) and
  `.nvmrc` for consistent Node versions.
- **`appops` commands** — new `set_app_op` / `get_app_op` Tauri commands for
  future use (AppOps-level permission control).

### Fixed

- **Tweaks: Nvidia System Hooks description** — corrected from "disables
  Netflix button" to its actual function (Xbox controller Guide → Home
  remapping). The Shield remote's Netflix button is firmware-level and cannot
  be disabled via ADB.

## v2-2.0.0-beta.13

### Fixed

- **Linux: AppImage blank window / EGL crash on modern Wayland systems**
  (CachyOS, Arch, and other rolling distros — "Could not create default EGL
  display: EGL_BAD_PARAMETER"). The AppImage bundled its own copies of
  `libwayland`, which clash with current Mesa graphics drivers; they are now
  removed so the system's own libraries are used, matching current AppImage
  tooling defaults. The crash was reproduced and the fix verified in CI
  against an Arch userspace. No more `LD_PRELOAD` workaround. (#60)

## v2-2.0.0-beta.12

The audit release: a full codebase review, every finding fixed, plus a big
Optimize-wizard usability pass.

### Fixed

- **Runaway memory / CPU in long sessions.** Opening the Snapshot tab with no
  saved snapshots (or the Install APK tab with an empty remembered folder)
  silently re-fetched in a tight loop forever. This is the likely cause of
  multi-GB memory reports.
- **Large file transfers no longer time out.** APK backups, app cloning, and
  file-manager uploads/downloads previously died at exactly 30 seconds —
  guaranteed failure for big apps and media files. Transfers now get a
  15-minute ceiling.
- **"Connect IP" honesty.** `adb connect` reports success even when it fails;
  the app now reads the actual result, and an unauthorized connection tells
  you to approve this computer on the TV.
- **Snapshot apply / panic recovery** now refresh the App List, Optimize, and
  Launcher views immediately instead of showing stale states.
- **Linux: blank window on some Wayland setups** (EGL_BAD_PARAMETER crash) —
  fixed by disabling WebKit's DMABUF renderer at startup; no more LD_PRELOAD
  workaround.
- Action-result messages (e.g. after an APK backup) are now dismissible
  instead of sticking around for the whole session.

### Optimize wizard

- **Review apps are spotlighted, not buried.** Streaming apps that need your
  judgment keep full opacity with an orange accent bar, the summary calls out
  how many — and which ones show **no recent use** ("3 show no recent use:
  Apple TV, …") — and each row's control now carries the rule itself:
  *Keep (if you use it)* / *Uninstall*, with an "Uninstall / disable if
  unused" hint.
- **Same look as the App List.** One shared row component drives both tabs:
  state badge, labeled RAM, "last used" cue, and REVIEW pill all match, and
  the wizard gains the same default-on **Hide not installed** filter.
- Usage cues now read "last used 5d ago" instead of a bare "5d ago".

### Changed

- **Overview's "Send text to TV" box is gone** — the Remote tab's live typing
  replaced it.
- The footer now links to [Ko-fi](https://ko-fi.com/bryanroscoe) if you want
  to support development.
- **App catalog accuracy pass**: Discovery+ is correctly reinstallable;
  YouTube Kids (TV), Showtime, and Epix Now are marked defunct (services
  discontinued); the STARZPLAY entry is renamed LIONSGATE+.

### Hardening & internals

- Device-shell input validation closed its last gaps (snapshot package names,
  settings keys/values are now validated or quoted everywhere).
- The launcher-takeover logic — the most safety-critical code path — gained
  its first full test suite (149 tests total), including the revert path.
- The 3,900-line device page was decomposed into per-tab components; CI now
  runs the full lint/test surface; docs were brought back in line with
  reality.

## v2-2.0.0-beta.11

A big App List / Optimize update centered on smarter, safer recommendations.

### Added

- **"Remove if unused" review tier.** Preinstalled streaming apps (Netflix,
  Disney+, Showtime, …) are surfaced as candidates to remove *if you don't use
  them* — never auto-selected — with a **last-used cue** ("used 3d ago", "no
  recent use") from usage stats so you can decide.
- **Per-app RAM badges.** The App List now shows live RAM (e.g. `RAM 243 MB`) on
  apps that are running right now — the cue for which unused app is quietly
  eating memory.
- **Friendly names + search for sideloads.** "Everything else" recognizes
  popular sideloads (Artemis/Moonlight, Overseerr, SmartTube, Jellyfin, …) by
  name, and search matches the name you actually see, not just the package id.
- **Files: optional system paths (power user).** A toggle lets you browse the
  whole filesystem beyond `/sdcard`; deletes outside `/sdcard` are
  double-confirmed and critical mounts are refused.
- **Tweaks: Background Process Limit.** Cap background apps to free RAM (with a
  clear note that Android resets it on reboot).

### Changed

- **Uninstall safety.** The wizard never recommends uninstalling an app you
  can't easily get back — non–Play-Store, non-defunct apps are disabled instead.
- **Recommendations reflect real benefit.** Dropped no-op suggestions (idle
  language keyboards), and RAM figures now only show for running, reclaimable
  apps.
- **App List defaults to installed apps** ("Hide not installed" on).
- The App List and Optimize tabs now stay in sync after an action.

### Fixed

- App List / Optimize tables no longer blank out, long system package names no
  longer overflow, and the Optimize plan loads faster (no redundant device
  re-detection).

## v2-2.0.0-beta.10

The version now reads as **2.0.0** so the app no longer looks like a v0 build —
it's been a v2 beta all along.

### Fixed

- **App List and Optimize tabs rendered blank.** Two streaming apps (Pluto TV,
  Tubi) were duplicated in the catalog; the table render aborted on the repeated
  key, so the counts loaded but the rows never appeared. Removed the duplicates
  and hardened the loader so a stray repeat can never blank the UI again.
- **Optimize was slow to load the plan.** It re-profiled every connected device
  first (and stalled on unauthorized ones). It now uses the type the page
  already detected and goes straight to the device query.
- **"Update available" link** now opens the full releases list instead of
  redirecting to the latest v1 (PowerShell) release.

## v2-0.1.0-beta.9

### Added

- **Files tab.** Browse the device's `/sdcard` storage with breadcrumbs and an
  Up button; **Download** files to your computer, **Upload here** from your
  computer, **Delete**, and **Copy to…** another connected device. Plus an
  **App file backups** finder that locates app exports (Projectivy `.plbackup`,
  SmartTube backups) and saves them locally.
- **Remote tab.** Live typing — keystrokes (with Backspace and Enter) go to the
  focused field on the TV — plus a D-pad, media, volume, and **Power / Wake**
  buttons.
- **App List shows every installed app.** A new "Everything else" section lists
  non-catalog packages (sideloaded apps like SmartTube get the same Backup /
  Copy-to tools), with a third-party vs. system toggle, a **search box**, and a
  **Hide not installed** filter.
- **More first-class apps.** 23 popular Android TV apps added to the catalog
  with friendly names and audited Play Store links: SmartTube, TizenTube,
  Kodi, VLC, Jellyfin, Moonlight, Steam Link, Plex-adjacent utilities,
  Downloader, RetroArch, Tubi, Pluto TV, MX Player, Tailscale, WireGuard,
  X-plore, Solid Explorer, TV Bro, and more.
- **Popular sideloads** section on the Install APK tab — official download
  links for apps that aren't on the Play Store (SmartTube, TizenTube, F-Droid,
  Aurora Store).
- **Install APK clarity.** Each discovered APK shows whether it's already
  installed (read from the APK's manifest), and the install result now renders
  as a concise ✓/✕ line under the row instead of a raw adb dump.
- **Update check.** The header shows the installed version and an
  "Update available →" badge when a newer GitHub release exists.
- **Name your snapshots.** Snapshots take an optional name; the per-device
  Snapshot tab now matches the global Snapshots page styling.

### Fixed

- **File browser showed nothing.** `/sdcard` is a symlink, so the directory
  listing came back with one bogus row and nothing was navigable. Fixed.
- **Launcher: enabling one no longer steals the default.** Android clears its
  preferred-HOME record when a launcher's state changes; re-enabling a launcher
  (especially stock) could hijack HOME. Enable now re-promotes your previous
  default and explains it.
- **Launcher: "Set as default" works on stubborn builds.** On TV builds where
  the role API silently no-ops and `set-home-activity` reports success without
  effect, the only working method is to disable the stock launcher — now done
  only with an explicit confirm, never silently, and reversible.
- **Snapshot apply no longer over-reports settings.** Settings already at the
  snapshot's value are skipped instead of re-written; the preview shows how many
  are no-ops.
- **Temperature reads on more devices.** Falls back to `hardware_properties`
  when `thermalservice` reports nothing (older Shield firmware).
- **Disabling voice components now warns** that in-app mic search (SmartTube,
  etc.) will stop working — Speech Services and the Assistant recognizer are
  flagged with a caution.
- **Install APK:** clicking Install on one APK in a folder list no longer shows
  every row as installing.

---

## v2-0.1.0-beta.8

### Added

- **TV screenshots.** A Screenshot button on the device header captures the
  TV screen, saves a PNG on your computer, and shows an inline preview with
  Open folder. (Protected/DRM content can't be captured — the error says so.)
- **APK backup.** Every App List row has a Backup button that saves the
  app's APK(s) to a folder you choose, named `<package>-<version>.apk`.
  Split APKs are pulled as a set and flagged.
- **Copy an app to another device.** "Copy to…" on App List rows installs
  the app onto a second connected device in one click. App data does not
  transfer, and DRM/licensed apps may refuse — the confirm spells it out.
- **Rename device.** A Rename button next to the device title writes the
  same setting the TV's own Settings → About → Device name does, so Cast /
  Google Home pick it up too.
- **Send text to TV** (Overview tab): type Wi-Fi passwords and searches
  from a real keyboard into whatever field has focus on the TV.
- **Force stop** on the Health tab's memory rows — frees an app's RAM now;
  it restarts on next launch.
- **Clear caches** (Health tab): one click trims every app's cache.
- **The stock launcher is back in the Launchers list.** It shows with a
  STOCK badge and a "Set as default" button — so switching back from a
  custom launcher is one click, even if stock was disabled.
- **All HOME-capable apps appear in the Launchers list** (HOME APP badge)
  with the same Enable / Set as default / Disable actions. The separate
  "Disable stock launchers" wizard is gone — it could neither show nor
  restore disabled launchers reliably. Disabling now asks for confirmation
  and refuses to disable the last enabled launcher on the device.

### Fixed

- **Network scan no longer hides devices that need authorization.** A TV
  that hasn't approved this computer shows up in the device list as
  UNAUTHORIZED with instructions, and the scan summary says how many are
  waiting for the "Allow USB debugging?" prompt — instead of "connected 0"
  over an empty page.
- **No more "Failed: Success" when setting a default launcher.**
  `set-home-activity` acknowledges with a bare "Success" on many builds; that
  ack was being recorded as the failure reason whenever the active-launcher
  check didn't confirm the switch in time. The launcher set now verifies with
  retries/backoff, treats the ack as acceptance, and when the device accepted
  the change but hasn't applied it yet, says exactly that ("press Home on the
  TV, then Refresh") instead of a contradictory error. (Beta feedback.)

---

## v2-0.1.0-beta.7

### Changed

- **Per-row action dropdown in the Optimize / Restore wizard.** The old
  Apply/Skip checkbox is now a dropdown so you can choose what happens to each
  app: **Disable / Uninstall / Skip** (Enable / Skip in Restore mode). It
  defaults to the catalog's recommended action — so you can downgrade an
  Uninstall to a safer Disable, or skip individual rows, without giving up the
  recommendation. The do-not-disable safety list still gates every action.

### Changed

- **Snapshot apply now shows a clear preview table** instead of a text dump.
  Applying a snapshot (from the global Snapshots page) opens a panel listing
  each package and what will happen — Disable / Set as launcher / Set setting,
  with already-disabled and not-installed rows clearly de-emphasized — mirroring
  the Optimize wizard. Apply happens from the reviewed panel.
- In the Optimize wizard, terminal rows (not installed / already in target
  state) now read as a neutral pill, visually distinct from a user-chosen
  "Skip".

### Fixed

- **"Play Store" buttons only show for apps that are actually on the Play
  Store.** Every catalog package was audited against Google Play; the button
  now appears only for the 33 packages with a real listing. System components
  and defunct apps (Print Spooler, Google Feedback, Funimation, Stadia, Quibi,
  Nvidia system packages, region/Fire-TV-only ids, …) no longer show a button
  that leads to a 404.
- **Optimize wizard defaults are no longer dangerously aggressive.** The wizard
  pre-selected an action for *every* installed app, so streaming apps (Netflix,
  Prime Video, Hulu, …) and anything not on the curated default list defaulted
  to Disable/Uninstall. It now respects each app's default the way v1 does:
  only default-optimize entries are pre-selected for action; everything else
  defaults to **Skip** (you can still pick Disable/Uninstall per row from the
  dropdown). Restore mode mirrors this with default_restore.
- **App descriptions are back in the App List.** Each app shows its inline help
  text ("Print service — irrelevant on a TV", etc.) under the name again,
  instead of only as a hover tooltip.
- **System keyboards added to the do-not-disable list.** Gboard, the Leanback
  keyboard, and the AOSP IME can no longer be disabled (from the memory table,
  a snapshot, or anywhere) — disabling the active keyboard removes all
  on-screen text input.
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

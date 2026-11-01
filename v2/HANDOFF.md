# v2 — current state

v2 is a shipping desktop app. Latest **published** version: **2.4.1** (2026-10-23). The `v2-2.4.0` tag's build
failed, so 2.4.0 was never published.

Installers for macOS/Linux/Windows are built on every `v2-*` tag push via
`.github/workflows/v2-release.yml`; macOS is also distributed via the Homebrew tap
(`bryanroscoe/homebrew-shield-optimizer`).

## Unreleased on `main`

Desktop help improvements and ADB connection recovery are pending release. Add by IP,
Scan LAN and restart now verify transport readiness; the confirmed restart can retry
one unchanged failed address. The subprocess driver remains the default. The native desktop test integration now shares the mobile Rust ADB transport while
retaining the existing Devices flow and 5555 LAN scan. Build with `native-adb-default`
for a Finder-launchable native test app; the driver label confirms the active backend.
Normal builds remain subprocess by default. Hardware acceptance and Windows native
identity support remain outstanding; this is not a published native release. These source fixes do not establish the full root
cause of reporter #235's environment. 2.3.1 was never tagged and 2.4.0 never built;
their notes shipped in 2.4.1. Put
the next change under `## Unreleased` in `v2/CHANGELOG.md`; `tests/notes-seen.mjs` expects that
heading to stay. Before tagging, run the
physical script in [`../docs/DEVICE-TEST-SCRIPT.md`](../docs/DEVICE-TEST-SCRIPT.md) and
`npm run e2e`.

Native Scan LAN now separates discovery from TV approval: owned, bounded connection
jobs continue after the scan returns, and ready TVs appear through serialized status
refreshes. Status polling does not re-query unchanged device properties and stops on
completion, backend unavailability, navigation, or its deadline. Pending handshakes
are not reported as unauthorized. Regression coverage is in `tests/native-scan-progress.mjs`
and the native driver/scan unit tests.

## Things the next reader needs that the code does not say

- **The app is ATV Optimizer.** `identifier`, the `ShieldOptimizer` data dir and the updater
  channel keep the old name on purpose, and the WiX UpgradeCode is pinned in `tauri.conf.json`
  to the value the old name derived, so existing installs upgrade in place. The Homebrew tap and
  cask token are still `shield-optimizer`; the cask's `url` names the renamed DMG.
- **Launchers are data**: `crates/core/data/app-lists/launchers.json`, parsed once in
  `loader.rs`. The Launcher list shows HOME handlers only. Don't add a `LEANBACK_LAUNCHER`
  query: every TV app's launch activity carries that category, so it lists every TV app as a
  Home app. `launcher-rows` and a Rust test guard this. The last-HOME-handler guard counts HOME
  handlers alone.
- **Stock takeover is its own step.** `set_home_any` (Launcher › Advanced) enables a package and
  asks Android to make it Home, then reports whether Android accepted it. It never disables
  stock. Disabling stock is the separate `disable_stock_launcher` command behind its own
  confirmed button. Both stay gated by `is_last_enabled_home_handler`, so the TV always keeps a
  Home screen.
- **Launcher setting on old Android.** Android 8 has neither `cmd package query-activities` (9+)
  nor `cmd role` (10+), so every way of setting Home shares one setter ladder that falls back
  past them (#87).
- **Snapshot semantics.** `engine/snapshot.rs` receives the current launcher in
  `ApplyPlanInputs` and sets `launcher_to_set` only when the snapshot's launcher differs from
  it; `apply_snapshot` skips the launcher ladder entirely when it is `None`. Settings already at
  the snapshot's value are carried as "already set", so the preview's Now column is real. A
  restore disables the recorded apps and writes the settings back; it never re-enables or
  reinstalls anything.
- **One label source, one recommendation source.** `src/lib/safety.ts` owns every verdict label
  (Protected, Caution, Safe to remove, Unknown, Checking…, Safety unavailable).
  `src/lib/recommendation.ts` owns `effectiveMethod`, `recommendation`, `canOfferUninstall` and
  the review labels; the App List, Optimize defaults and Health's Suggestion column all call it.
  Uninstall is recommended only when the app is reinstallable (`play_store || defunct`), and
  never labelled "Remove". Don't add a mapping inline in a component; `recommendation-labels`
  checks the screens agree.
- **Demo catalog**: `src/lib/demo-apps.json` is `common.json` followed by `shield.json`, copied
  verbatim. Regenerate it whenever those change, or the demo drifts from the product.
- **Pairing and discovery.** A successful `adb pair` never implies a connect port; the user
  enters the separate Connect port from the Wireless debugging screen. Scan reads
  `adb mdns services`, so a random wireless-debugging port is discoverable. There is no Android
  version gate anywhere (#88).
- **adb working directory.** Every adb spawn is pinned to a stable directory. The adb server
  outlives the app, so inheriting the launch cwd left a process pinning a DMG's `/Volumes/...`
  path and triggered repeated macOS removable-volume prompts (#89).
- **Remote paste** works through an `onpaste` handler on the focused remote element plus a Paste
  button; a focused non-editable element receives `clipboardData` in both Chromium and WebKit, so
  no clipboard plugin is needed (#91).
- **Stop ADB when quitting** (`src-tauri/src/commands/quit.rs`, `crates/core/src/adb/lifetimes.rs`)
  is off by default because the adb server is shared by every tool on the machine. Even when on,
  quit leaves the server running while any operation lease or adb process is outstanding after
  a short drain (an open Remote session holds a lease), when the app restarts for an update, and
  when adb points at a server the app cannot target. The app never infers that a server is its
  own. The platform-tools installer holds an operation lease from before download through driver
  replacement. Framework limits behind this design (Tauri 2.11.2, updater plugin 2.10.1):
  `ExitRequestApi::prevent_exit` ignores restart requests, and the updater's Windows install
  path calls its synchronous `on_before_exit` hook and exits directly, so an async exit listener
  cannot be assumed to run there. Keep updater integration separate from the quit path. Real
  macOS/Windows/Linux checks with other adb clients, the updater restart and custom servers have
  not been run.
- **Logs tab** (`crates/core/src/commands/logcat.rs`, desktop only; mobile does not register
  it) takes finite, read-only logcat snapshots, not a stream: each read replaces the last,
  entries can repeat between reads, and high-volume traffic can roll out of the window. Reads
  start only on an explicit click. Severity, one exact tag and an optional package constrain
  the device-side read; package filtering resolves the running main PID per snapshot and fails
  rather than falling back to unfiltered logs. Every call goes through the bounded driver (30 s,
  256 KiB per stream), one read at a time. Auto-refresh waits three seconds after each read and
  stops on error, tab departure, device-page reset, document hiding or Stop. Tags cannot start
  with a hyphen or carry shell/filter syntax, and nothing clears buffers, resizes them or writes
  settings. Logs are never saved, uploaded or added to bug reports.
- **App logs**: `<data_dir>/logs/`, daily rotation, 7 kept. Debug level is per-crate, not
  global, so adb lines are not buried under hyper. `collect_diagnostics` formats in the engine
  (pure) and reads in `src-tauri/src/commands/diagnostics.rs`.
- **Support tiers and telemetry** (both apps, shared core). `Device.support` comes from the pure
  `engine/support.rs` matching `crates/core/data/device-support.json` (or a validated fetched copy
  cached as `<data_dir>/support-tiers.json`). Tiers are copy only: they never feed
  `engine::safety`, and `not_supported` adds a warning before destructive actions but never blocks
  enable, restore or Recovery. Anonymous reports are on by default, opt-out, and a no-op unless
  the build sets `ATVOPT_TELEMETRY_URL`; payload and redaction are pure
  (`engine/telemetry.rs`), the queue and upload are `crates/core/src/telemetry.rs`, and the
  frontends report outcomes from their single invoke wrapper via `shared/telemetry.ts`. Error
  codes shown to users and sent in reports come from one table, `shared/errors.ts`. See
  `v2/README.md` "Telemetry and support tiers".
- **TV evidence** is a three-way value (`tv | not_tv | unknown`); only `not_tv` blocks a row. Do
  not re-introduce "unknown means phone" — that was #120.

## Roadmap

- Feature gaps against aTV Tools: [`ATVTOOLS-PARITY.md`](ATVTOOLS-PARITY.md).
- ATV Optimizer for Android: [`mobile/HANDOFF.md`](mobile/HANDOFF.md) and
  [`mobile/BACKLOG.md`](mobile/BACKLOG.md).

## Known deferred items

- **Screen recording and mouse mode** — the aTV Tools capabilities still absent; see
  [`ATVTOOLS-PARITY.md`](ATVTOOLS-PARITY.md).
- **Code signing and update-key custody** — builds are unsigned on macOS and Windows. The
  reasoning is in the root `CLAUDE.md`; setup notes are at the top of
  `.github/workflows/v2-release.yml`.
- **Frontend audit** — `npm audit` reports one low advisory, `cookie` 0.6.0 via SvelteKit
  (GHSA-pxg6-pf52-xh8x). The fix needs `cookie` ≥0.7.0, outside SvelteKit's supported range, so
  don't force an override; re-audit before each release.

## Invariants + release process

See **`CLAUDE.md`** (or `AGENTS.md`, a symlink to it) at the repo root — architecture
invariants, safety-gate rules, release script usage, and MSI versioning notes are all there.

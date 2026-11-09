# v2 — current state

Native desktop ADB takeover: see [NATIVE-ADB-HANDOFF.md](NATIVE-ADB-HANDOFF.md)
for PR #268, exact candidate provenance, completed checks and remaining native acceptance.

v2 is a shipping desktop app. Latest **published** version: **2.4.1** (2026-10-23). The `v2-2.4.0` tag's build
failed, so 2.4.0 was never published.

Installers for macOS/Linux/Windows are built on every `v2-*` tag push via
`.github/workflows/v2-release.yml`; macOS is also distributed via the Homebrew tap
(`bryanroscoe/homebrew-shield-optimizer`, cask `atv-optimizer`).

## Unreleased on `main`

Snapshot schema is **4** on `main` (PR #353): snapshots record catalog apps that weren't installed,
and restore disables any of them that are back (never uninstalls). 2.5.0 and earlier refuse v4
files as newer than they support. Review rows suggest Uninstall only for a store app in
`pm list packages -3`; see `AppEntry::suggested_method`.

Nothing yet. `v2-2.5.0` (2026-11-04) shipped the rename (identifier `com.atvoptimizer.app`, data
folder `ATVOptimizer`, binary `atv-optimizer`, crates `atv-optimizer` / `atv_optimizer_lib` /
`atv-optimizer-core`, Homebrew cask `atv-optimizer`) and the desktop connection work below; its
notes are in `v2/CHANGELOG.md`.

**2.5.0 shipped without the physical device script, at Bryan's direction.** The `v2 Upgrade Tests`
workflow passed on `1bd204a`, the commit under the version bump (all 16 jobs: NSIS, MSI and macOS
bundle upgrades from 2.2.0 and 2.4.1, Homebrew-managed, retries), and `v2-tests.yml` passed there on
all three OSes. Nothing has been run on real hardware: the R steps ("Upgrades across the 2.5.0
rename") and D10–D12 (macOS Local Network) in `../docs/DEVICE-TEST-SCRIPT.md` are still open, so
real in-app updates, Dock, Login Items, the Local Network prompt, `brew upgrade` through the rename
(R7) and Linux (R9) are unverified. Treat reports in those areas as likely real.

Desktop connection recovery: Add by IP, Scan LAN and restart verify transport readiness, and the
confirmed restart can retry one unchanged failed address. On macOS, an adb server left running by
the previous version is cut off from the network once the new version opens; the app restarts it
when it can reach a TV that adb can't (`src-tauri/src/adb/server_repair.rs`). The subprocess driver
was the desktop default through 2.5.0. PR #268 makes the built-in Rust ADB (the mobile
transport, `crates/direct-adb` and vendored `adb_client`) the default engine, with Settings ›
ADB › ADB engine switching back to platform-tools (Bryan, 2026-11-05). Its handoff is
`NATIVE-ADB-HANDOFF.md`; the release that ships it waits for Bryan's hardware batch,
`../docs/DEVICE-TEST-SCRIPT.md` N1–N18. Bryan's decisions (2026-11-05): 2.5.0 shipped without #268 on purpose, and
`server_repair.rs` stays for the platform-tools path and its fallback even once native is the
default. Reporter #235 has not
confirmed a fix. Put the next change under `## Unreleased` in `v2/CHANGELOG.md`;
`tests/notes-seen.mjs` expects that heading to stay. Before tagging, run the physical script in
[`../docs/DEVICE-TEST-SCRIPT.md`](../docs/DEVICE-TEST-SCRIPT.md) and `npm run e2e`.

Native Scan LAN now separates discovery from TV approval: owned, bounded connection
jobs continue after the scan returns, and ready TVs appear through serialized status
refreshes. Status polling does not re-query unchanged device properties and stops on
completion, backend unavailability, navigation, or its deadline. Pending handshakes
alone are not reported as unauthorized. Regression coverage is in `tests/native-scan-progress.mjs`
and the native driver/scan unit tests.

The current native owner-test app also retains the optional grouped IPv4/IPv6
address picker in collapsed Scan details. Scan tries alternative mDNS routes
serially and suppresses redundant routes after a ready connection or observed
AUTH; actual TV cards still merge only on verified hardware identity. The exact
624fd2f candidate passed seven CI jobs and the two-Shield duplicate-route,
explicit IPv6 selection and endpoint Retry/Forget checks. See the native handoff
above for its merge-ref provenance and remaining hardware acceptance.

macOS native builds declare their Local Network purpose and ADB Bonjour services in
`src-tauri/Info.plist`. Keep separately installed development apps on a distinct local
bundle identifier and sign the complete development bundle; `--no-sign` leaves only
the linker's executable signature. Local Network permissions can become ambiguous
when multiple unsigned versions share an identifier. A zero-result scan accompanied
by `No route to host` while the same address is reachable outside the app requires
checking app-specific Local Network permission, not resetting TV authorization keys.
The shipping identifier stays unchanged. Apple-issued signing remains a release task.

LAN sweep hits now start connecting as each responder arrives, before the remaining
subnet probes or mDNS observation window finish. Scan diagnostics record subnet
detection, first responder, sweep admission and mDNS elapsed times to distinguish
cold-network delays from authorization waits.
Built-in connections never offer the public key by themselves (2026-11-08, Bryan:
scanning must not pop the prompt on every TV). A TV that refuses every signature is
listed `unauthorized` as Not allowed yet, named by its Cast name, with Request
approval; only that button sends `AUTH_RSAPUBLICKEY` and it waits up to 10 minutes
(`APPROVAL_WAIT`), or until Cancel, Forget, an engine switch or quit. See
`DIRECT-ADB.md` › Approval. Failed or unidentified offline network addresses stay in
Scan details with exact-endpoint retry, Forget and diagnostics; they are not
additional identified devices. Test discovery with no previously approved TVs, not
just successful reconnections. The Android app still offers its key on connect
(follow-up). Request approval opens `src/lib/components/ApprovalDialog.svelte`, a pop-up with
a pulsing TV that closes itself once the TV connects; under platform-tools the same compact
row and pop-up replace the old instruction box (Request approval redials through the adb
server and watches that one transport for 10 minutes). The PR's native test packages set
`ATVOPT_TEST_BUILD` and `ATVOPT_TELEMETRY_URL`: Send logs works there, each upload is
marked `-dev` and "Test build <merge commit> (merge commit; PR head <head>)" (the merge
commit is what was built, as in the window title), and automatic reports stay off.

Native connection attempts use one handshake deadline (20 seconds quiet, the approval
wait for Request approval; the Android app keeps 30 seconds) and interrupt
their owned socket when canceled or superseded. Forget/retry and mobile cancellation
must not leave an old authorization read running until its per-response timeout.
The deadline is detached after successful authentication so it cannot later close
a live TV connection. Loopback transport regressions cover cancellation before AUTH,
cancellation while awaiting approval, timeout, and the first shell probe after success. Explicit TLS certificate-trust rejections
now point to Pair PIN and the separate connect port, rather than legacy AUTH prompts.

Native sync transfers now parse DATA/DONE/FAIL as a byte stream across ADB packets.
A real read-only system-file download exposed a permission rejection being written
into the local file before a misleading trailer error. Failure text is now an error,
never file data. Upload success requires an explicit sync OKAY, and STAT responses
are acknowledged and may be fragmented. Loopback tests cover these boundaries;
read-only physical download acceptance passed on the labeled Download Recovery Fix
app at 3e2f6ce. Upload/install acceptance remains outstanding. A current-binary denied pull exposed
another failure: it evicted the main session and left an empty local file. Native
downloads now use the existing bounded, hardware-verified dedicated connection
and publish a temporary file only after success; denial leaves the original TV
connection and any existing local destination intact. Uploads also consume early or
mid-transfer FAIL replies, including fragmented replies interleaved with a transport
ACK. They stop when rejection is observed and never replay or accept success before
DONE; the same total transfer deadline bounds incomplete error replies.

The current integration also retains main's Settings and privacy controls. Native
mode is identified as built-in in diagnostics, without probing a host server or
live TV sessions for scan reports. Scan statistics count the existing streaming
probes, and pending handshakes do not become failed-connection reports. Settings
keeps host-server installation/restart controls inactive in native mode.
Subprocess status distinguishes a missing binary from a failed availability probe;
timeout/startup/I/O failures retain their diagnostic and Restart recovery instead
of being labeled "ADB not found".

## Things the next reader needs that the code does not say

- **Optimize catalog expansion**: twenty source leads reviewed for identity and purpose now live in `crates/core/data/app-lists/common.json`. They appear as optional Disable-if-unused suggestions on both apps, with Caution verdicts, default Keep and no automatic Restore. See `../docs/OPTIMIZE-CATALOG-REVIEW-2026-11-05-BATCH-2.md` and `../docs/OPTIMIZE-CATALOG-REVIEW-2026-11-06.md` and `../docs/OPTIMIZE-CATALOG-REVIEW-2026-11-07.md` and `../docs/OPTIMIZE-CATALOG-REVIEW-2026-11-08-BATCH.md` for the two-agent reviews and `../docs/OPTIMIZE-CATALOG-REVIEW-PROCESS.md` for the continuing batch process. The imported source catalog remains unreviewed; it is not loaded into the action planner.

- **Source catalog** (`shared/components/ProfileCatalog.svelte`, desktop `/catalog`, mobile More → Device & package catalog) exposes the MIT source snapshot under `catalogs/cleanyourtv/`. It is research information with original source/profile context and the full MIT notice. Package-detail links prefill an identity but require explicit source-profile selection. The view never reads or matches the connected device and has no action or safety-classification imports. Core app lists, safety and support tiers remain the action authorities. `test:profile-catalog` and mobile `profile-catalog.test.mjs` cover source isolation and navigation.

- **RojasNet requests** (#258–#262): Shell bookmarks can be edited and include the requested user-0 package presets; App List has enabled/disabled filtering; both snapshot lists expose confirmed local deletion; app-report notes prefill GitHub Details from the redacted preview. App inventory and post-disable verification target user 0, matching package actions (#233). These changes are unreleased. Focused regressions are `test:shell-bookmarks`, `test:app-state-filter`, `test:snapshot-delete`, and `test:app-report`.

- **Desktop Settings** (`src/routes/settings/`, the header gear) holds every app-wide preference.
  Its switches are the same state as their other copies, not duplicates: the report setting
  lives in `src/lib/reports.svelte.ts` (also the Report a bug dialog), theme and auto-update in
  `src/lib/ui-prefs.svelte.ts` (the header theme switch), Stop ADB when quitting in
  `src/lib/stop-adb-on-quit.svelte.ts` (Devices). Its Updates and About rows drive the layout's
  own update flow and dialogs through `src/lib/app-shell.ts`. Support ID and Send logs need the
  backend's `support_id` and `upload_diagnostics`. Settings explanations open from question-mark
  controls (`HelpTip.svelte`); keep routine explanatory copy collapsed. The diagnostics Worker
  and migration 0003 were deployed and verified with a synthetic upload and readback on 2026-11-03.

- **The rename to ATV Optimizer (2.5.0).** `src-tauri/src/rename.rs` runs first in `run()`,
  before logging and before any webview. It copies the old `ShieldOptimizer` data folder
  (everything except `platform-tools/`) and the old WebView profile (`~/Library/WebKit/<id>` on
  macOS, `<local data>/<id>` on Windows and Linux, caches and lock files skipped) into
  `<name>.migrating-<pid>`, checks that the source did not change meanwhile (else it retries
  once), writes `.atvoptimizer-migrated` into the copy and renames it into place. A folder
  without that marker is unfinished: an unmarked data folder is merged into (old entries it
  lacks, plus the old `telemetry.json`, so the install id and opt-out survive); an unmarked
  profile is renamed to `<id>.unmigrated-<time>` and copied again. If the data copy fails, the
  session runs on the old folder, so nothing creates the new one and the next launch retries.
  The profile is copied once: `.webview-profile-migrated` in the data folder stops it being
  refilled after a user or an uninstaller deletes it. Unreadable non-essential files are
  skipped and listed; snapshots, `telemetry.json`, `disabled-home-handlers.json` and the
  localStorage store must copy. The old data is never moved or deleted, and failures are
  logged without failing the launch. Old `shield-optimizer.*.log` files are renamed in the copy
  so rotation covers them. The old downloaded adb is still found where it is
  (`adb_path_in_legacy_install_root`). localStorage keys keep their `shieldopt.*` names: the
  origin did not change, so the copied profile is read as is. Pinned on purpose, never to change:
  the WiX `upgradeCode`, `bundle.publisher` (`shieldoptimizer`, the value the old identifier
  derived, which NSIS and WiX use for `Software\shieldoptimizer\<product>` and the install-folder
  lookup), the updater `pubkey` and endpoint, the repo name and the `v2-*` tags.
  Windows: `src-tauri/windows/hooks.nsh` closes a running `shield-optimizer-v2.exe` before the
  install (asking first unless silent or passive), then removes a leftover "Shield Optimizer"
  install from 2.2.0 or earlier (files by name, shortcuts that still point at it, the Uninstall
  and `Software\shieldoptimizer\Shield Optimizer` keys). It creates an "ATV Optimizer" shortcut
  where it removed an old one and none exists, points taskbar pins at `atv-optimizer.exe`, and
  gives this install's shortcuts the new AppUserModelID. It never touches app data or runs the
  old uninstaller. The MSI has no hooks: MSI users were always major-upgraded in place.
  macOS: a bundle still named `Shield Optimizer.app` is renamed to `ATV Optimizer.app` at
  `RunEvent::Exit` (never on an update restart), unless it is translocated, the name is taken,
  or a `shield-optimizer*`/`atv-optimizer*` cask is installed and the bundle is in
  `/Applications` or `~/Applications`, where renaming it would break `brew upgrade`. The log is
  flushed at exit so that line survives.
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
- **Logs tab** (`crates/core/src/commands/logcat.rs`; desktop's Logs tab and the phone's
  More › Device logs, both polled by `v2/shared/log-poller.ts`) takes finite, read-only logcat snapshots, not a stream: each read replaces the last,
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
  (pure) and reads the device in core `commands/diagnostics.rs`, shared with mobile; desktop's
  `src-tauri/src/commands/diagnostics.rs` adds the adb binary path and its log tail.
- **Install outcome** (`engine/install_outcome.rs`, pure): the `Success`-line check, the
  `INSTALL_FAILED_*` hints and the strict `pm path` parser, shared by desktop sideload, backup and
  clone and by the phone's APK backups.
- **Support tiers and telemetry** (both apps, shared core). `Device.support` comes from the pure
  `engine/support.rs` matching `crates/core/data/device-support.json` (or a validated fetched copy
  cached as `<data_dir>/support-tiers.json`). Tiers are copy only: they never feed
  `engine::safety`, and `not_supported` adds a warning before destructive actions but never blocks
  enable, restore or Recovery. Anonymous reports are on by default, opt-out, and a no-op unless
  the build sets `ATVOPT_TELEMETRY_URL`; payload and redaction are pure
  (`engine/telemetry.rs`), the queue and upload are `crates/core/src/telemetry.rs`, and the
  frontends report outcomes from their single invoke wrapper via `shared/telemetry.ts`. Error
  codes shown to users and sent in reports come from one table, `shared/errors.ts`. Schema 2
  (2026-11-02) adds `diagnostic` events for scans and TVs that found nothing (`diag` built by the
  pure `engine/telemetry_diag.rs`), desktop-only redacted log excerpts (`engine/log_redact.rs`,
  20 a day), the support ID (`support_id` in `telemetry_settings`, the install id while reports
  are on) and desktop Send logs (`upload_diagnostics` → `POST /v1/diagnostics`, works with reports
  off). The phone app sends no logs: its Play Data safety form doesn't declare them. Find a user's
  data with `telemetry-worker/scripts/support-lookup.sh <support-id>`. See
  `v2/README.md` "Telemetry and support tiers". The backend has been live since 2026-11-01
  (`telemetry-worker/README.md` "Production"). `device-support.json` is the only tier list: the
  `support-tiers-sync` workflow pushes it to the Worker on merge, and a weekly workflow rewrites
  the "Device support report" issue with suggested moves. Change a tier only by a PR to that
  file with today's `updated` date and a `support.rs` test (`../docs/DEVICE-SUPPORT.md`).
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

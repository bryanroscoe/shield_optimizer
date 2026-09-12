<!--
Preserved 2026-09-12 from an external coordination tree that is being retired.
Original path: mayor/artifacts/hq-b1t/shield-release-decision-20260909.md
Content is unchanged below the horizontal rule; only this header and the closing
appendix were added. Nothing in this packet was ever executed: no tag, no
release, and no device run. Treat it as the proposal it was, not as a record of
work that happened.
-->

# Shield Optimizer — release decision packet (2026-09-09)

Prepared by shield_optimizer/crew/mechanic (Claude Fable 5.1) at the Mayor's request.
Nothing in this packet has been committed, pushed, tagged, released, or run on a device.
Every item below is a proposal for Bryan's decision.

> **Update 2026-09-10:** D1 executed under Bryan's commit/push policy: `main` is at d02de57 (36b561c core+desktop, d02de57 mobile). D2, D3 and D4 remain open.

## 0. What is being decided

| # | Decision | Recommendation |
|---|----------|----------------|
| D1 | Commit the crew/mechanic candidate to `main` | Two whole-file commits (core+desktop first, mobile second), pushed directly to `main` per repo convention, after D3 |
| D2 | Release version and channel | Desktop `v2-2.2.0-beta.1` first, promote to `v2-2.2.0` after device pass and reporter confirmations. Mobile bumps to `0.2.0` internal debug APK, no store channel |
| D3 | Physical test on Bryan's Shield TV | Run the script in section 3 before D1 push (about 45 minutes) |
| D4 | Cross-OS CI additions | Add mobile and desktop harness jobs to `v2-tests.yml`, Node 22+ (section 4) |

## 1. Candidate inventory and provenance

- Tree: `shield_optimizer/crew/mechanic`, working tree on top of `origin/main` 48cba23, HEAD unchanged, zero local commits.
- Size: 41 tracked files modified (4226 insertions, 1251 deletions) plus 12 untracked application files. Excluded from any commit: `v2/node_modules`, `v2/node_modules.portpair06-link` (preserved symlink), `.svelte-kit`, build output.
- Manifest with per-file SHA-256, included patches and gate outputs: `mayor/artifacts/hq-b1t/shield-so-vtm.9-integration/manifest.json` (sha256 3ab4e56b…).
- Gates already passed on this macOS host (arm64, Node 26.3.0): core cargo fmt/clippy/test 201, src-tauri 36, desktop svelte-check 0/0 + build, mobile svelte-check 0/0 + build, mobile Playwright suite 67/67, desktop pairing harness, app-files-catalog test, desktop gallery regenerated (gallery.gif 8db1dfc7…, gallery-light.gif d1ceea10…). Not run: Linux, Windows, any device.
- Every constituent was implemented by a Sol polecat in an isolated clone or worktree, source-reviewed by mechanic, and (where UI/copy) accepted by navigator; exact patches and hashes are on the beads and under `~/.local/share/gastown/evidence/shield_optimizer/`.

### 1.1 What the candidate contains

Desktop + core (GitHub-issue related):
- **#88 TCL Android 14 "Device Not Supported" (so-7iw)**: `adb pair` success no longer guesses `HOST:5555`; the app requires the separate Connect IP:port from the Wireless debugging screen, clears the PIN, keeps the pairing success message after a failed connect, and retains mDNS-discovered devices. Files: `v2/src-tauri/src/adb/driver.rs`, `v2/crates/core/src/commands/devices.rs`, `v2/src/routes/+page.svelte`, `README.md`, harness `v2/tests/pairing-flow.mjs`.
- **#87 Sony XBR "Unable to Set Default Launcher" (so-x02.1)**: launcher switching now reports each failure stage explicitly and verifies rollback instead of a silent failure. Files: `v2/crates/core/src/commands/launcher.rs`. Root cause on the Sony (Android 8) is NOT proven; this improves diagnosis and rollback reporting.
- **#89 macOS "relentlessly asking for permission to access removable volumes" (so-2en, so-2en.1)**: Locating the `adb` binary no longer probes PATH or the launch working directory (which can be a removable volume) until every higher-priority candidate (explicit override, app-managed platform-tools, ANDROID_HOME) has failed, and the Sideload tab scans a saved APK folder only on an explicit click (folder remembered in localStorage). Files: `v2/src-tauri/src/adb/driver.rs`, `v2/src/lib/components/SideloadTab.svelte`, `v2/src/lib/api.ts`. The macOS TCC trigger was NOT reproduced; this removes the known probe sources. Network scanning on boot is unchanged.
- **#86 SmartTube "App file backups not found" (so-3ty)**: the catalog fix (`Documents/SmartTubeBackup`, `org.smarttube.stable`) is already on `main` 48cba23 but absent from released v2-2.1.0. This candidate adds a durable test `v2/tests/app-files-catalog.mjs` (+1 script line in `v2/package.json`). Publication is what closes the report.
- **Unknown safety (so-fb3.2)**: `engine/safety.rs` no longer falls through to Safe; uncatalogued packages are `Unknown` with a reason. Desktop consumers (`AppRow.svelte`, `OptimizeTab.svelte`, `devices/[serial]/+page.svelte`, `types.ts`, `demo-mock.ts`) show Unknown, never auto-select it, and stop treating a failed inventory lookup as Enabled. New shared contract `v2/shared/safety.ts`.
- **Mobile backup scope (so-vtm.6)**: core `commands/apps.rs` gains the all-installed backup listing used by the mobile Backups screen.
- Tooling: `v2/screenshots/capture.mjs`, `screenshots/README.md`, regenerated `gallery.gif` / `gallery-light.gif`, `.gitignore` adds `.runtime/`.

Mobile companion app (`v2/mobile`, feedback items so-vtm.1–.9 and so-fb3.1/.2/.6):
- Saved-TV identity: hardware id is the only strong identity; no merge/rename/delete on IP-only evidence; coexisting identities at one address stay separate with neutral copy; discovery rows show Connected / Saved address (unverified) / Found / Not found; single saved TV auto-dials only when it is truly alone. (`savedDevices.ts`, `discoveryRows.ts`, `Onboarding.svelte`, `Devices.svelte`).
- Unknown safety consumers + unknown-app diagnostics logging (`safety.ts`, `Apps.svelte`, `AppDetailSheet.svelte`, `Diagnostics.svelte`, `More.svelte`, `RiskGuide.svelte`, `unknownDiagnostics.ts`).
- Apps system-app visibility filter (.4), APK backup scope and coverage (.6), frame-rate control copy (.5), Dashboard recommended-app count + Optional apps review with Keep default (.7; `Dashboard.svelte`, `Optimize.svelte`, `session.svelte.ts`).
- Tests: `savedDevices`, `unknownDiagnostics`, `apps-filter`, `backups`, `dashboard-optional-review`, updated `session` (67 cases).
- Docs: `HANDOFF.md`, `USER-FEEDBACK-2026-09-05.md`, new `LIFECYCLE-EVIDENCE.md`, `LOCAL-STATUS-2026-09-05.md`, `FEEDBACK-UX-2026-09-07.md` (navigator-owned; navigator's final reconciliation for .7 is still pending).

## 2. D1 — proposed commit plan

Repo convention (crew CLAUDE.md): crew pushes directly to `main`, no feature branches, no `--amend`, no `Co-Authored-By`. If Bryan wants a review surface instead, the alternative is a short-lived branch `convoy/shield-2026-09` with one PR; the content below is identical either way.

Why two commits rather than one per feature: `Apps.svelte`, `api.ts`, `types.ts`, `commands/mod.rs` and `commands/apps.rs` each carry several features. Per-feature commits would need hunk splitting and would not compile independently. Two whole-file commits keep every intermediate state buildable.

**Commit 1 — core, desktop, tooling**
```
Harden safety verdicts, pairing, launcher rollback and sideload scanning

- Safety engine returns Unknown instead of Safe for uncatalogued packages;
  desktop lists never auto-select Unknown and stop assuming Enabled on a
  failed inventory lookup (shared contract in v2/shared/safety.ts)
- Pairing no longer guesses HOST:5555 after adb pair; the Connect IP:port
  from Wireless debugging is required and the mDNS device list is kept
  (refs #88)
- Launcher switching reports each failure stage and verifies rollback
  (refs #87)
- adb binary lookup stops probing PATH and the launch directory before
  higher-priority candidates; saved APK folder is scanned only on demand
  (refs #89)
- Durable test for the app-files catalog covering SmartTube backup paths
  (refs #86); pairing-flow harness; regenerated screenshot galleries
```
Files: `.gitignore`, `README.md`, `v2/crates/core/**` (5 files), `v2/shared/safety.ts`, `v2/src-tauri/src/adb/driver.rs`, `v2/src/**` (7 files), `v2/tests/*` (2 files), `v2/package.json`, `v2/screenshots/*` (4 files).

**Commit 2 — mobile companion app**
```
Mobile: stable saved-TV identity, Unknown safety, backup scope and optional-app review

- Discovery recognizes saved TVs only on verified identity; duplicate
  addresses stay separate; truthful Connected/Saved/Found/Not found rows
- Unknown safety verdicts, unknown-app diagnostics log, system-app filter,
  all-installed backup scope, frame-rate copy
- Dashboard shows installed-recommended enabled count; Optimize gains an
  Optional apps review with Keep as default and explicit per-app actions
- 67 browser/pure test cases; handoff, lifecycle evidence and feedback docs
```
Files: `v2/mobile/**` (17 modified + 10 new).

GitHub issue handling: use `refs #NN` only. Do not auto-close. After release:
- #86: close with a comment pointing at the release (fix verified by catalog test; device check optional).
- #88, #89, #87: comment with the release and ask the reporters to confirm on their TCL / macOS / Sony; close on confirmation. #87 stays open if the Sony still refuses `set-home-activity`, since root cause is unproven.

## 3. D2 — proposed release

Current published desktop release: v2-2.1.0 (June 22, 2026). Candidate changes are user-facing behavior plus a safety-classification change, so a minor bump. Because pairing (#88), launcher (#87) and macOS TCC (#89) are unverified on the reporters' hardware, ship a beta first.

- Desktop: `v2-2.2.0-beta.1` via `cd v2 && ./release.sh --minor --beta` (interactive; script sets WiX version `2.2.301` per the documented band scheme, commits `Release v2-2.2.0-beta.1`, tags, pushes on confirmation; workflow builds Linux/macOS/Windows, regenerates the gallery, bumps the Homebrew cask). Promote with `./release.sh --set 2.2.0` once confirmations arrive.
- Mobile: bump `v2/mobile/package.json` to `0.2.0` (and the Android versionCode in the Tauri Android config) in the same release commit; build `npx tauri android build --apk --debug --target aarch64` for Bryan's phone; no store submission. Mobile has no release workflow today.

### 3.1 CHANGELOG section to add at the top of `v2/CHANGELOG.md`
```
## v2-2.2.0-beta.1

Safer defaults, honest pairing, and a companion-app overhaul.

### Safety
- Apps missing from the audited catalog are now labeled **Unknown** with a
  reason instead of Safe. Unknown apps are never pre-selected for disable
  or uninstall; you choose them explicitly and the confirmation says so.
- A failed inventory read no longer shows an app as Enabled.

### Devices and pairing
- Android 11+ pairing now asks for the separate Connect IP:port shown on
  the Wireless debugging screen instead of guessing port 5555 (#88).
- The PIN is cleared after each attempt and a successful pairing is not
  reported as a failure when the connect step fails.
- The app no longer probes PATH or its launch directory for adb before
  checking its own platform-tools and ANDROID_HOME (#89).

### Launchers
- Setting a default launcher reports exactly which step failed and confirms
  the rollback restored the previous launcher (#87).

### Sideload
- The Sideload tab remembers your APK folder and scans it only when you
  click Scan saved folder, which avoids repeated removable-volume prompts on
  macOS (#89).

### Backups
- SmartTube backup search now includes Documents/SmartTubeBackup (#86).

### Mobile companion (beta)
- Saved TVs are recognized by verified identity, not by IP address; devices
  that share an address stay separate.
- Unknown-safety labels, an unknown-app diagnostics log, a system-app
  filter, full installed-app backup scope, and a guided Optional apps review
  where Keep is the default.
```

## 4. D3 — physical test script for Bryan's Shield TV

Preconditions: Shield TV and the Mac on the same LAN; Settings → Device Preferences → Developer options → Network debugging ON; note the current default launcher and one app you are willing to disable and re-enable (for example a game). Build the desktop candidate from crew/mechanic with `cd v2 && npm run tauri dev` (or `npm run tauri build`). For the mobile checks, sideload the debug APK on an Android phone (companion app targets the TV; it is not installed on the TV).

Desktop (about 25 minutes):

| Step | Action | Expected |
|------|--------|----------|
| D1 | Launch app from the DMG with a USB drive mounted, do not click anything | Devices page loads; the usual boot-time scan may run; no macOS removable-volume permission prompt appears, and none recurs after quitting |
| D2 | Click Scan Network | Shield appears with [NET] tag; a single scan message |
| D3 | On the TV enable Wireless debugging → Pair device with pairing code; in the app click Pair PIN, enter that IP:port and PIN | Message says paired; PIN field clears; the app does NOT report a device on port 5555; the note tells you to enter the Connect IP:port |
| D4 | Enter the Connect IP:port from the main Wireless debugging screen in Connect IP | Device becomes `device`, row clickable |
| D5 | Open the device → Apps. Find an app that is not in the catalog (a sideloaded APK) and a known bloat entry | Sideloaded app shows Unknown with a reason; catalog bloat shows Caution; nothing Unknown is pre-ticked in Optimize |
| D6 | Optimize tab: leave defaults, click Apply, read the confirmation | Confirmation lists only Caution items; Unknown items absent unless you ticked them; apply completes; disabled apps re-enable from the Restore path |
| D7 | Launchers: switch default launcher to another installed launcher, then switch back | Each attempt reports its stage; on any failure the previous launcher is restored and the report says so; HOME on the TV matches the app's report |
| D8 | Sideload: pick a folder with APKs, quit and relaunch the app | Folder is remembered; nothing scans until Scan saved folder is clicked; on macOS no repeated volume prompt |
| D9 | Files → App file backups for SmartTube (if installed) after Settings → Backup/Restore → Local backup | The `.zip` under Documents/SmartTubeBackup is found |

Mobile companion (about 20 minutes, phone on the same LAN):

| Step | Action | Expected |
|------|--------|----------|
| M1 | Fresh install, Connect new device | Discovery shows the Shield as Found (no Saved label yet) |
| M2 | Connect and open the dashboard, then disconnect and rescan | The Shield shows as Saved with its name; exactly one saved row |
| M3 | Force a new DHCP lease (or change the TV's static IP), rescan | Same single saved row moves to the new address; no duplicate |
| M4 | Dashboard | "X of Y installed recommended apps are enabled" with real numbers; no ring or "System optimized"; Review app choices opens Optimize |
| M5 | Optimize → Recommended / Optional apps | Optional rows default to Keep; a sideloaded app shows UNKNOWN with a reason; no Select all on Optional |
| M6 | Choose Disable on one optional app, switch tabs, Apply | Summary shows 1 optional; confirmation names the app and the 0.5× animation post-pass; Cancel leaves the TV untouched; Apply disables only that app |
| M7 | Apps → system filter; Backups | System apps hidden by default and toggle on; Backups lists every installed app, not only the catalog |
| M8 | Diagnostics → unknown-app log | Entries exist for the Unknown apps seen in M5 |

Abort rule: if any step leaves the TV without a launcher or with an unexpected disabled app, use More → Emergency recovery (mobile) or Restore (desktop) and stop; record the step.

## 5. D4 — cross-OS CI needs

Current `v2-tests.yml`: Rust fmt/clippy/test on ubuntu-22.04, macos-latest, windows-latest; frontend svelte-check + build on ubuntu with Node 20. `v2-release.yml` builds bundles on tag and runs the Playwright gallery job. Gaps this candidate introduces:

1. **Node version**: `v2/tests/app-files-catalog.mjs` uses `node:module` `stripTypeScriptTypes`, available from Node 22.13; CI pins Node 20. Bump the frontend job to Node 22 (or 24).
2. **Desktop harnesses**: add steps `npm run test:app-files-catalog` and `npx playwright install --with-deps chromium && npm run test:pairing-flow` in `v2` on ubuntu.
3. **Mobile job** (new, ubuntu): `cd v2/mobile && npm ci && npm run check && npm run build && npx playwright install --with-deps chromium && npm test`. Expect 67 cases, about 2 minutes. so-ncj tracks transient failures when six browser suites start concurrently; until fixed, run with `node --test --test-concurrency=1 tests/*.test.mjs` or add a single retry.
4. **Windows/Linux Rust**: no new native dependencies; the matrix already covers the core and src-tauri changes. Mobile `src-tauri/src/lib.rs` (one-line command registration) is only compiled by the Android build, which CI does not run; add `cargo check -p` for the mobile crate on ubuntu if a cheap guard is wanted.
5. **What CI cannot prove**: macOS TCC prompts (#89), TCL/Sony device behavior (#87, #88), Android runtime lifecycle of the companion app. Those stay on the D3 script and reporter confirmations.

## 6. Open items that do not block D1–D4

- Navigator copy acceptance of so-vtm.7 and docs reconciliation (in progress; text-only).
- so-fb3.2 typed process identity for memory rows, so-fb3.3/.4/.5/.7 (product follow-ups, not in this candidate).
- so-ncj browser-suite flakiness (P3).


---

## Appendix — what is still unknown on #87 and #89

Preserved from a separate triage note dated 2026-09-08. Both fixes on `main` were written
without reproducing the reporter's hardware, so these are the open questions a future
investigation should answer rather than assuming the fix was the root cause.

For #89 causal completion, the missing evidence is the affected app/macOS version, installation and selected ADB/folder paths, close-window versus true-quit lifetime, and the process/path responsible for TCC requests. Current source and pinned Tauri 2.11.2 show no project exit-prevention loop; source inspection alone does not reproduce the report. Do not reset TCC, grant access, kill shared ADB, or silently change relative path semantics. Any later controlled reproduction must preserve explicit relative binary/transfer/install paths and stay within authorized host/device scope.

For #87 causal completion, compare actual target enabled state, resolved HOME/component, setter result and restoration result across the failing and successful sequences. The current generic error cannot identify which branch failed. AOSP Android 8 already supports the relevant package commands; no blanket version incompatibility or speculative fallback is justified. Existing never-disable, stock-only and explicit opt-in guards remain mandatory.


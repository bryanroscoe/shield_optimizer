# ATV Optimizer mobile backlog

Updated 2026-09-05 America/Chicago (lifecycle evidence/current authority). Bryan's
feedback records a debug build from `main` at `44d2d66` installed on the Pixel;
this is owner-reported use, not a completed physical stability matrix.
`FEATURES.md` and `ARCHITECTURE-REVIEW.md` are historical audits; use this file,
`HANDOFF.md`, and `USER-FEEDBACK-2026-09-05.md` for current work. Track execution in
`bd`, including restart feedback #7 on `so-vtm.8`.

**Scope note (corrected 2026-09-16):** the old host-only / no-commit restriction is dead — see
`CLAUDE.md`, work is committed and pushed continuously. What remains real is the *device* gate:
nothing in `v2/mobile` has been verified on a phone or TV since `44d2d66`. Accepted Navigator work is committed and pushed under
the September 10 policy in `AGENTS.md`; physical items below remain gated. The
older installation instructions are not current permission.

## Correctness checkpoint

Implemented in the current checkpoint:

- Bind every normal `WirelessAdb` operation to its requested serial while holding the connection
  lock, including transfer, screenshot, reboot, and raw disconnect paths.
- Serialize connect/disconnect lifecycle transitions so a slow connect cannot resurrect after a
  manual disconnect.
- Give the liveness shell probe transport-level timeouts; cancelling a blocking task no longer
  leaves the connection mutex held indefinitely.
- Treat reboot as successful only after adbd acknowledges the reboot service, and keep pre-ack
  transport failures as failures.
- Keep local push/pull filesystem errors from evicting a healthy TV connection.
- Await manual disconnect before returning to onboarding and suppress the resulting one-shot
  saved-TV auto-reconnect.
- Fully clear per-device caches on disconnect/switch and discard health/bloat responses that arrive
  for a device that is no longer current.
- Fail destructive app controls closed until the canonical safety lookup resolves, and prevent a
  late lookup for one package from changing another package's controls.
- Display `cross_device_warning` in the snapshot confirmation UI.
- Redact remote text and secret-bearing fields from frontend debug logs.
- Persist a valid license before changing the live entitlement.
- Make Optimize actually apply the selected audited plan one command at a time, report partial
  failures, and run the performance-settings post-pass.
- Use shell-v2 framing on direct TCP devices so stdout, stderr, and exit status are real. The
  Bedroom Shield live gate returned separate `out`/`err` streams and exit code 7; older devices
  that reject shell-v2 retain the shell-v1 compatibility fallback.
- Fix GitHub #86 by searching SmartTube's current `/sdcard/Documents/SmartTubeBackup` export
  location while retaining its legacy app-data location, and discard stale desktop file-browser
  directory responses.
- Complete fast-remote failure cleanup on Android: control/server streams are dropped on blocking
  workers on every handshake branch, failed starts kill the resident server, hold-repeat ticks are
  bounded instead of accumulating behind a slow fallback, and `find_remote` now respects real
  shell failure/exit status.
- Normalize malformed saved-TV records (including the legacy `googletv` spelling and invalid
  dates), show an explicit stale-data warning after Diagnostics refresh failure, isolate safety and
  Tweaks loads by device/request, capture the mutation target serial, and reload Tweaks after any
  possibly-partial write failure.
- Store every APK part in a versioned, manifest-backed backup bundle; restore complete bundles with
  `pm install-multiple`, path-confine every restore read, clean remote staging on every result, and
  label older base-only backups as incomplete instead of offering an unsafe restore.
- Subset the bundled Material Symbols font reproducibly from Svelte references (5.34 MB to about
  111 KB) and disable autocorrect, spellcheck, and automatic capitalization for license keys.
- Retain fast-remote sessions for a 30-second Android background grace period, cancel stale cleanup
  timers on resume, and serialize remote startup with teardown so an expired timer cannot kill a
  newly started session.
- Allow 30 seconds for the TV authorization prompt and replace raw transport errors with actionable
  retry/"Allow debugging?" guidance. Preserve the last real friendly name across rotating ADB ports,
  retain 16 recent TVs, expose previous TVs in the Dashboard switcher, and chart used rather than
  free RAM in Diagnostics.
- Prefer the durable cached friendly name in the live session label when a newly connected TV only
  reports a generic model. A device-less 384×812 interaction pass verified switching, connection
  guidance, and the used-RAM bar; the resulting `3eccc34` follow-up still needs installation and a
  focused physical UI spot-check.

## Saved-TV identity audit fixes (2026-09-30)

Fixed GitHub #115–#118, all found by a code-read audit. Each repro is now a test in
`tests/savedDevices.test.mjs`, `tests/discoveryRows.test.mjs` and `tests/session.test.mjs`;
**none has run on a phone or TV.**

- One hardware-id rule (`lib/identity.ts`, matching desktop `idKey`): trim, and empty or any
  casing of `unknown` is no id. Stored placeholder ids migrate to id-less rows, each keyed by its
  own stable random local id rather than its bare endpoint (see #146 below), so saved-TV list keys
  are unique and Forget removes exactly one TV (#116). A placeholder is no longer a wildcard for
  `adb-unknown-*` adverts.
- `cachedDeviceName` matches on verified id, or an id-less row at the exact endpoint; an
  identified row never names a TV that reports no id. Advertised serials match only exactly or
  with adbd's six-character suffix, so `shield` no longer verifies `adb-shield-a` (#117).
- Scan rows are built per stored TV, never per address; no synthesized "Saved TV" row (#115).
- `loadHealth` cannot wedge after its device vanishes mid-load, Cancel no longer shows a
  reconnect failure, and Diagnostics keys its safety lookup on the sorted package set (#118).

## Id-less saved TVs stay distinct (2026-10-01)

Fixed GitHub #146, raised by Codex on PR #144 and declined there as an edge case at the time.
Two id-less saved TVs (no hardware id) that had shared one `host:port` used to collapse into a
single storage row on migration/read -- the older one silently discarded -- because every id-less
row was keyed by its bare endpoint. Fixed in `savedDevices.ts`/`identity.ts`/`discoveryRows.ts`:

- Every id-less row gets a stable random `localId` the first time it is persisted; `savedDeviceKey`
  keys on that instead of the endpoint, so two id-less TVs stay two rows no matter what address
  they shared. Existing stored rows are migrated on first read without losing any.
- `rememberDevice` only refreshes an existing id-less row when exactly one saved row matches the
  connecting endpoint. When two or more already share it, which one just reconnected is unknowable,
  so the connection is deliberately not persisted against any of them -- it writes nothing, rather
  than either overwriting one of them with a guess or saving a new row on every repeat reconnect
  (the latter would eventually evict a genuine saved TV once `MAX` is reached).
- A shared `savedDeviceIsLiveConnection` check (`identity.ts`) is the one place that decides
  whether a saved row is unambiguously the live connection: always true for a verified hardware id,
  true for an id-less match only when it is the single id-less row at that endpoint. Both
  `buildDiscoveryRows` and the Devices screen's "Other TVs" filter use it, so neither marks more
  than one id-less saved row "connected" at a shared live endpoint, and neither hides an ambiguous
  row (and its Forget control) from the list -- the discovery row falls back to a generic name and
  both saved rows report "saved-address" instead of one of them claiming the connection.

Covered by new tests in `tests/savedDevices.test.mjs` and `tests/discoveryRows.test.mjs`; none has
run on a device.

## A different id-less device can no longer inherit a saved TV's row (2026-10-01)

Fixed GitHub #154, raised by Codex on PR #152 (the #146 fix above) and deferred there as an edge
case needing a product decision. #146 fixed the case where *two or more* id-less saved rows already
shared an address; it did not change the original, more common case: when exactly **one** id-less
row sits at an address, `rememberDevice` still trusted that lone match on endpoint alone, so a
*different* id-less TV that later answers at the same address (DHCP reassignment, a replaced
device) silently inherited the old row's name and `localId`.

Without a hardware id the app still cannot prove identity, so the fix is a soft signal, never a
promotion to "verified":

- Every id-less saved row now carries an optional `fingerprint` (`model`, `manufacturer`,
  `deviceCodename`, and the TV's own user-set `friendly_name`), captured from the live device's
  reported properties (`identity.ts`: `deviceFingerprintOf`). Hardware-identified rows don't carry
  one -- the id is already verified, so there's nothing for a fingerprint to add.
- On a lone id-less match, `fingerprintMismatch` compares saved vs. live `model`/`manufacturer`.
  A clear disagreement means `rememberDevice` sets the match aside and records the connection as a
  new, distinct row instead of refreshing the old one (`savedDevices.ts`). The old row is left
  untouched. A missing field on either side (an older row saved before this existed, or a device
  that reported nothing) is unknown, never a mismatch -- it does not block the match, so existing
  rows migrate for free with no separate migration step.
- Once two id-less rows share an endpoint this way, the existing #146 ambiguity rule
  (`savedDeviceIsLiveConnection`) already refuses to call either one "connected" or let a further
  reconnect silently refresh either -- no new ambiguity-handling code was needed there.
- `session.svelte.ts` surfaces the mismatch as `session.identityNote` ("A different device is now
  at this address."), read and cleared once by whichever screen's connect flow notices it
  (Dashboard's `onMount`/`switchDevice`, Devices' `reconnect`/`reconnectCurrent`) instead of silently
  going unmentioned.

Covered by new tests in `tests/savedDevices.test.mjs` (verified to fail before the fix); none has
run on a device.

**Codex review follow-up on PR #155 (same day):** two valid findings, both fixed:

- `recoverOrMarkLost()` (the silent one-shot reconnect `checkLiveness()` triggers when the cheap
  liveness probe fails) redialed the same host:port directly and never ran the identity check
  above, so a different id-less TV that took over mid-session during a silent recovery was
  trusted without comparison. It now calls `rememberCurrentDevice()` on a successful recovery,
  before flipping liveness to `"live"` -- the same check an explicit reconnect gets. Covered by a
  new Playwright case in `tests/session.test.mjs`, verified to fail before the fix.
- Devices' `reconnect(d)` cleared `session.identityNote` and showed its toast locally, then
  immediately navigated to Dashboard -- the toast never had a chance to be seen. It now leaves the
  note unread when connecting a saved row so Dashboard's own `onMount` can show it instead (that
  path already existed and is exercised, so there was nothing else to change there). Covered by a
  new Playwright case in `tests/session.test.mjs`, verified to fail before the fix.

## Stability reset (2026-09-04)

A four-track audit (feature parity vs v1/desktop, connection lifecycle, screen UX, Rust backend)
and fix sweep. Landed:

- Transport: bounded reads everywhere (30 s shell inactivity, 180 s installs, 120 s vendored
  safety net, 5 s TCP connect), socket-level-only eviction with a post-error probe, identity mirror
  outside the device mutex, poison-tolerant locks, no second disconnect from the status probe,
  shell-v1 exit normalization, PNG-first screenshots with a separate stderr sink. Vendored
  `adb_client` skips stray foreign-stream packets, bounds-checks sync payloads, rejects a zero
  local id, cannot spin on a framebuffer overshoot, and no longer compiles the rustls key log.
- Core: one sentinel-batched shell per read path (`adb/batch.rs`) for health, package lists,
  optimize plan, launchers, display scaling; entitlement default fail-closed to Free.
- Connection UX: `Connection to the TV was lost` errors flip the session from any screen, one
  silent redial before the global `ConnectionBanner` (Retry / Switch TV), 45 s visible heartbeat,
  saved-TV picker on launch (auto-dial only with one saved TV and no prior explicit Disconnect),
  separate `addtv` route, per-row dialing state, mDNS names in scan results, honest pairing copy,
  connect ordering that never pairs an old name with a new host, generation guards on
  refresh/liveness, host cleared on reset, back-button history kept to one spare entry.
- Screens: Dashboard shows the real `N of M` bloat count (no floored score), inline screenshot
  preview, outside-tap dropdown dismissal; Settings (renamed from More Options) grouped, with
  Emergency recovery (`panic_recovery` was registered but unreachable), recovery/bootloader reboot,
  and Disconnect confirmation everywhere; Devices no longer runs a duplicate health sweep and its
  Reconnect is guarded; one safety vocabulary (`lib/safety.ts`, exactly the three core kinds) used
  by Optimize, AppDetailSheet, Diagnostics, RiskGuide; Optimize renders core tiers (not catalog
  risk), rounded MB with honest "RAM in play" wording, Optimize/Restore mode, cancelable apply with
  tabs locked, shared PaywallSheet; Apps debounced search, reinstall-existing after uninstall,
  caution confirm on Disable; Diagnostics real live refresh, force-stop/disable on top memory,
  explicit no-TV state, no fabricated names or thresholds; Tweaks tri-state (on/off/unset)
  toggles, all four HDMI-CEC settings, 3-way frame-rate control, reset-to-default per setting,
  per-card errors, confirmed display scaling with the override size shown; Launcher no longer
  fabricates a DEFAULT badge, invalidates caches after changes, and warns when the channel
  provider is disabled; Snapshots pin the serial the plan was built against and show a real plan
  breakdown; Remote gains volume-down and wake, confirms power, shows send-text errors in the
  modal and a queued-press pill; Files guards stale listings and resets on TV switch; Backups
  lists every app, can delete a backup (`delete_backup`, path-confined), and drops the fake
  Drive tile; overpromising copy ("every change is reversible", "roll back instantly", "or a
  snapshot restore", "Tap a file to save it") corrected.

Verified device-less: all workspace gates green (fmt, clippy on 4 crates + aarch64 Android
clippy, 165 core tests, 18 mobile tests, 27 vendored tests, `npm run check` 0/0, `npm run
build`), plus a 384×812 Playwright pass with Tauri stubbed covering scan, the two-TV picker (no
auto-dial), single-TV auto-dial, no auto-dial after an explicit Disconnect, connect-failure
guidance, lost-connection probe/recovery, Emergency recovery, and the back-button stack.
Those were the 2026-09-04 host results; later owner-reported `44d2d66` use does not
complete the physical matrix.

## P0 — next correctness work

1. **Resolve restart feedback #7 with owner-driven lifecycle evidence (`so-vtm.8`).**
   Host review is complete at `48cba23`: 13 focused browser cases and 9 existing regressions
   pass; mobile check/build and 3 pure lifecycle tests pass. See
   [`LIFECYCLE-EVIDENCE.md`](LIFECYCLE-EVIDENCE.md) for exact scope, reproducible commands,
   and the physical gate. Visibility-only resume retains the screen; fresh JavaScript
   starts over. This does not identify Bryan's leaving action or Android lifecycle event.
   No session-persistence fix is authorized from that evidence alone.
   Separately, mechanic accepted `so-vtm.3` navigation locally: saved-TV success
   opens Dashboard directly after profile resolution (20 worker browser tests).
   Navigator did not rerun that candidate; baseline interstitial observations do
   not apply to its repaired saved-TV path. Physical/publication gates remain.

   **Physical stability follow-up (gated):** once separately authorized, first identify
   the installed build and capture exact Home/Back/Recents/recreation evidence. Do not
   assume the APK predates the sweep, change it before recording the report, or reuse old
   pairing endpoints. The broader real-Shield matrix also remains open: (a) put the TV to sleep or turn off Wi-Fi
   mid-session and confirm the app flips to "Reconnecting…" then the banner within ~45 s instead of
   staying green; (b) confirm a one-TV launch names the TV and can be cancelled, and a two-TV launch
   waits for a choice; (c) Disconnect, kill the app, relaunch — it must not redial; (d) run Optimize
   apply and confirm tabs lock and Cancel stops after the current item; (e) Emergency recovery on a
   TV with a few disabled apps; (f) back button from Settings → Devices → back → back reaches the
   dashboard, and one more press leaves the app; (g) Screenshot preview renders; (h) Tweaks shows
   "Unset" rather than OFF for a never-written setting; (i) Settings › Licensing shows "Debug build
   test key" after activating the dev key, and Settings › About opens the third-party notices.
2. **Verify code pairing end-to-end** per `PAIRING-PLAN.md` (host binary against the Pixel's
   pairing service first, then a Google TV; wrong code must fail cleanly; a paired TV must connect
   afterwards without an "Allow debugging" prompt). Pairing is implemented but has never talked to
   a real adbd.
3. **Finish the exact fast-remote device gates.** Living Room already passed concurrent diagnostics,
   file list/pull, and SHA verification while the channel was live. Repeat concurrency on Bedroom;
   on both Shields test hold, live-session background/resume before and after 30 seconds, sleep/wake,
   and reboot cleanup. Take a fresh controlled Living Room latency sample because one earlier p95
   was 108 ms even though later samples were 98 and 87 ms.

## P1 — required product features

1. Finish the remaining fast-remote Phase 4 matrix in `FAST-REMOTE-PLAN.md`: Bedroom concurrency,
   holds, sleep/wake, reboot, live-session background/foreground, reconnect, and the fresh Living
   Room latency sample.
2. **Verify** the new Android 11 wireless-debugging code pairing on a real device (implemented
   2026-09-04, unverified end-to-end; follow the manual test in `PAIRING-PLAN.md`: Pixel pairing
   service from the host first, then a Google TV, including the wrong-code and silent-reconnect
   checks).
3. Add Android SAF import/export and user-selected push destinations, then consider Google Drive
   sync for complete APK bundles.
4. Add panic recovery, advanced reboot, permission/app-op controls, reinstall-existing, and a
   deliberate performance/correctness pass across Launcher, Tweaks, Files, and Backups.
5. ~~Signed commercial licensing~~ — done 2026-09-04 (`crates/core/src/license.rs`,
   `tools/atvopt-license`, `mobile/LICENSING.md`). Remaining: decide the store/checkout that
   issues keys, and back up `~/.atvopt/license-signing-key.prod`.
6. Follow-ups surfaced by the 2026-09-04 audit:
   - ~~snapshot.rs batching~~, ~~`remote_warm`~~, ~~`pull_file` cap + de-dupe~~, ~~saved TVs
     keyed by `ro.serialno`~~ — done 2026-09-04.
   - Product decision: let free users see the Optimize plan read-only (core `prepare_optimize`
     is Pro-gated today, so the flagship tab is a lock card for free users while the Dashboard
     already shows the count for free).
   - Product decision: `Feature::FileManager` / `Feature::BackupClone` exist in core but gate
     nothing; either wire them or delete them.
   - Screenshots and pulled files land in app-private storage with no export; SAF (P1.3) is
     what makes them useful.

## P2 — release readiness

- ~~Android signing config, versionCode tooling, per-ABI docs~~ — done 2026-09-04
  (`mobile/RELEASE.md`). Still open: create the real keystore (or enroll Play App Signing), a CI
  job that builds a signed AAB, and a first signed release APK size measurement.
- ~~Third-party notices~~ — generated (`mobile/THIRD-PARTY-NOTICES.md`, in-app under Settings ›
  About; regenerate with `scripts/gen-notices.sh`). Still open: a legal/privacy review pass; note
  five MPL-2.0 crates (unmodified, fine) and the Unicode-3.0 family were accepted.
- Complete the accessibility/navigation review.
- Reconcile the mobile feature matrix with actual command behavior and remove overpromising copy
  such as “exact rollback,” “automatic snapshots,” or “fully reversible” where the implementation
  cannot guarantee it.

## Separate scope

Desktop rebranding remains separate because changing the Windows product identity can orphan
existing MSI installations. Do not mix it into the mobile release track.

# ATV Optimizer mobile backlog

Updated 2026-09-04. This is the current ordered queue. `FEATURES.md` and
`ARCHITECTURE-REVIEW.md` are historical audits and contain many findings that have already been
fixed; use this file plus `HANDOFF.md` for current work.

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
guidance, lost-connection probe/recovery, Emergency recovery, and the back-button stack. Nothing
in this sweep has been installed on the Pixel yet.

## P0 — next correctness work

1. **Install current HEAD on the Pixel and run the physical stability check.** The installed APK
   predates the whole 2026-09-04 sweep. On a real Shield: (a) put the TV to sleep or turn off Wi-Fi
   mid-session and confirm the app flips to "Reconnecting…" then the banner within ~45 s instead of
   staying green; (b) confirm a one-TV launch names the TV and can be cancelled, and a two-TV launch
   waits for a choice; (c) Disconnect, kill the app, relaunch — it must not redial; (d) run Optimize
   apply and confirm tabs lock and Cancel stops after the current item; (e) Emergency recovery on a
   TV with a few disabled apps; (f) back button from Settings → Devices → back → back reaches the
   dashboard, and one more press leaves the app; (g) Screenshot preview renders; (h) Tweaks shows
   "Unset" rather than OFF for a never-written setting.
2. **Finish the exact fast-remote device gates.** Living Room already passed concurrent diagnostics,
   file list/pull, and SHA verification while the channel was live. Repeat concurrency on Bedroom;
   on both Shields test hold, live-session background/resume before and after 30 seconds, sleep/wake,
   and reboot cleanup. Take a fresh controlled Living Room latency sample because one earlier p95
   was 108 ms even though later samples were 98 and 87 ms.

## P1 — required product features

1. Finish the remaining fast-remote Phase 4 matrix in `FAST-REMOTE-PLAN.md`: Bedroom concurrency,
   holds, sleep/wake, reboot, live-session background/foreground, reconnect, and the fresh Living
   Room latency sample.
2. Implement Android 11 wireless-debugging SPAKE2 pairing for new Google TV devices.
3. Add Android SAF import/export and user-selected push destinations, then consider Google Drive
   sync for complete APK bundles.
4. Add panic recovery, advanced reboot, permission/app-op controls, reinstall-existing, and a
   deliberate performance/correctness pass across Launcher, Tweaks, Files, and Backups.
5. Replace the hard-coded development Pro key with signed commercial license validation and a
   recovery/transfer policy.
6. Follow-ups surfaced by the 2026-09-04 audit, in rough order of value:
   - Batch the two remaining `pm list packages` joins in `snapshot.rs` (same `adb/batch.rs`
     pattern; 2 → 1 each).
   - Expose a `remote_warm` command so the Remote tab can start the scrcpy channel on mount
     instead of paying the cold start on the first press.
   - Product decision: let free users see the Optimize plan read-only (core `prepare_optimize`
     is Pro-gated today, so the flagship tab is a lock card for free users while the Dashboard
     already shows the count for free).
   - Product decision: `Feature::FileManager` / `Feature::BackupClone` exist in core but gate
     nothing; either wire them or delete them.
   - Cap `pull_file` size and stop clobbering same-named files in app storage.
   - Screenshots and pulled files land in app-private storage with no export; SAF (P1.3) is
     what makes them useful.
   - Key saved TVs by a device fingerprint rather than host so a DHCP lease reuse cannot inherit
     another TV's name.

## P2 — release readiness

- Android signing, release variants, versionCode/versionName automation, per-ABI output, and CI.
- Third-party notices and a legal/privacy/security review, including `adb_client` and its crypto
  dependency chain.
- Consolidate and surface third-party font notices, complete the accessibility/navigation review,
  and measure the signed release APK size.
- Reconcile the mobile feature matrix with actual command behavior and remove overpromising copy
  such as “exact rollback,” “automatic snapshots,” or “fully reversible” where the implementation
  cannot guarantee it.

## Separate scope

Desktop rebranding remains separate because changing the Windows product identity can orphan
existing MSI installations. Do not mix it into the mobile release track.

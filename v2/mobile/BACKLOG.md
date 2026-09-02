# ATV Optimizer mobile backlog

Updated 2026-09-01. This is the current ordered queue. `FEATURES.md` and
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

## P0 — next correctness work

1. **Install current HEAD and spot-check the feedback fixes.** The Pixel's last installed APK
   predates `3eccc34`. Verify cached names survive an actual TV switch, Diagnostics fills the RAM
   bar from used memory, and a missed authorization prompt produces the actionable 30-second error.
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

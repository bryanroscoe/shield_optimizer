# ATV Optimizer mobile backlog

Updated 2026-07-13. This is the current ordered queue. `FEATURES.md` and
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

## P0 — next correctness work

1. **Transport command truthfulness.** The upstream `adb_client` shell-v1 implementation does not
   expose stderr or an exit status. Stop claiming that it does, then add a tested shell-v2 or
   sentinel-based result path so failures are distinguishable from successful empty output.
2. **Fast-remote lifecycle races.** Serialize remote start/reset, make every failed start branch
   clean up its server/control resources, and ensure blocking stream drops never run on the async
   executor. Exercise concurrent diagnostics and file operations while the channel is live.
3. **File and backup honesty/correctness.** Guard directory requests against out-of-order results;
   implement restore or remove restore promises; handle split APKs instead of presenting a base-APK
   copy as a complete backup.
4. **Diagnostics/tweaks refresh correctness.** Mark stale metrics visibly after refresh failure,
   reload state after partial animation-setting writes, and prevent old screen requests from
   overwriting a new device's values.
5. **Saved-device storage hardening.** Validate/migrate malformed localStorage records so an invalid
   `lastUsed` value cannot crash onboarding.
6. **Remote result truthfulness.** Make `find_remote` inspect command failure, bound hold-repeat
   work, and verify fallback behavior after channel loss.

## P1 — required product features

1. Finish the fast-remote Phase 4 device matrix in `FAST-REMOTE-PLAN.md`: second Shield, holds,
   sleep/wake, reboot, phone background/foreground, reconnect, and concurrent operations.
2. Implement Android 11 wireless-debugging SPAKE2 pairing for new Google TV devices.
3. Add Android SAF import/export and push destinations; then add an explicit restore flow and only
   then consider Google Drive sync.
4. Add panic recovery, advanced reboot, permission/app-op controls, reinstall-existing, and a
   deliberate performance/correctness pass across Launcher, Tweaks, Files, and Backups.
5. Replace the hard-coded development Pro key with signed commercial license validation and a
   recovery/transfer policy.

## P2 — release readiness

- Android signing, release variants, versionCode/versionName automation, per-ABI output, and CI.
- Third-party notices and a legal/privacy/security review, including `adb_client` and its crypto
  dependency chain.
- Subset Material Symbols, review font licenses, fix license-input keyboard attributes, complete
  accessibility/navigation review, and measure release APK size.
- Reconcile the mobile feature matrix with actual command behavior and remove overpromising copy
  such as “exact rollback,” “automatic snapshots,” or “fully reversible” where the implementation
  cannot guarantee it.

## Separate scope

Desktop rebranding remains separate because changing the Windows product identity can orphan
existing MSI installations. Do not mix it into the mobile release track.

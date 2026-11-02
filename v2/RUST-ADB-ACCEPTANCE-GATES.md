# Rust-primary ADB acceptance gates

A passing fixture suite does not establish physical-device acceptance or authorize
a release or default switch. The current native fast Remote gate stays
disabled and platform-tools remains the shipped default.

## Fixture-tested implementation

- Ordinary shell and screenshots now count queue wait and command I/O on the existing owned
  connection against their total budgets (30 seconds, or 180 seconds for slow shell).
  Generic push/pull and limited downloads have fifteen-minute total budgets;
  dedicated limited downloads include authentication in that budget, and
  reboot acknowledgement has thirty seconds. Cancellation targets only the
  admitted owned socket. Ambiguous delivery is never replayed.
- Partial helper upload arms cleanup before SEND. Cleanup waits for the actual
  local producer to release its stream and lease, within the existing three-second
  cleanup budget. Expiry records an unconfirmed outcome and never unlinks later
  while a producer could still write. Dropped cleanup tasks also record uncertainty.
- Offline aarch64 Android Rust compilation covers core, shared driver, mobile
  library and plugin with the existing API-24 NDK toolchain. This is compile-only
  evidence, not Kotlin/JNI, packaged-app, Android cancellation or TV acceptance.

Local filesystem calls cannot be forcibly interrupted: a timed-out worker keeps
its lease until it ends. Local producer completion does not prove remote CLOSE
processing or deletion. Process/runtime termination can still leave helper files.
Android generic identity binding and finite stream lifetime require runtime
acceptance; desktop cancellation fixtures do not establish Android parity.

## First-run identity implementation

Unix Rust-mode startup now resolves the standard app data folder without a
working-directory fallback, validates an existing app-owned identity or publishes
a complete new identity without overwriting a concurrent winner. Storage uses a
private directory and file, refuses unsafe or corrupt existing entries, and never
imports standard ADB keys. Fixture tests do not run production generation.
Windows secure storage now uses current-user-owned protected ACLs, reparse-free
paths and no-clobber publication. Eleven Windows identity tests and native Clippy
passed at d81b54c; packaged Windows identity acceptance remains outstanding.
Packaged macOS identity creation and reuse have been exercised. Physical approval
of a previously unapproved TV remains unverified. See NATIVE-ADB-HANDOFF.md for
exact delivered-binary provenance; an integrated source change is not that binary.

## Physical acceptance scope

Routine regression coverage and validation are already authorized. Record the
exact build, TV and action for hardware evidence. The existing macOS scope covers
read-only discovery, retry/Forget and download recovery; it does not authorize
APK installation, device writes, power/reset or destructive commands.

Remote helper upload/start/stop/removal and input delivery need explicit action
scope before physical execution. Observe target identity, input release, helper
cleanup and socket cancellation; record failures and indeterminate outcomes.
Simulated results cannot fill those fields. Keep private LAN evidence local.

Native enablement and a shipped-default change require a separate decision after
code and hardware acceptance. No release or deployment follows automatically.

# Rust-primary ADB acceptance gates

No publication, hardware action, credential onboarding or default switch is
approved by a passing fixture suite. The current native fast Remote gate stays
disabled and platform-tools remains the shipped default.

## Fixture-tested implementation

- Ordinary shell and screenshots now count queue wait, authentication and command
  I/O against their total budgets (30 seconds, or 180 seconds for slow shell).
  Generic push/pull and limited downloads have fifteen-minute total budgets;
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

## Minimal approvals for physical acceptance

1. Name the permitted Mac/Windows/Linux packaged builds, TV models/firmware and
   exact endpoints. Approve only reuse of an already-authorized identity; identity
   creation/import, authorization or permission changes require separate approval.
2. Approve the device actions to test: generation-scoped helper upload/start/stop/
   removal and a named harmless key/text sequence, including disconnect during
   held input. APK installation, power/reset and destructive commands are separate.
3. Observe physical target identity, input delivery/release, helper/process/file
   cleanup and socket cancellation. Record failures and indeterminate outcomes;
   simulated results cannot fill these fields.
4. Approve exact GitHub wording before any publication. Native enablement and a
   shipped-default change require a separate decision after code and hardware
   acceptance; no release or deployment follows automatically.

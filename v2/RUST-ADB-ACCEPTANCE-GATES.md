# Rust-primary ADB acceptance gates

No publication, hardware action, credential onboarding or default switch is
approved by a passing fixture suite. The current native fast Remote gate stays
disabled and platform-tools remains the shipped default.

## Implementation work still pending

- Total wall-clock deadlines/cancellation for ordinary shell, screenshots and
  limited transfer paths; current memory caps and inactivity bounds are not that.
- Partial or unobserved helper-upload cleanup and cleanup during runtime shutdown;
  acknowledged uploads have an owned bounded cleanup attempt, not a physical
  deletion guarantee.
- Android generic-service compatibility after strict identity binding and finite
  stream lifetime; desktop-owned cancellation does not imply Android parity.

These are technical gaps, not requests to expand permissions. They must not be
marked passed by mocks or hidden by fallback.

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

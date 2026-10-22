# Issue212 launcher feedback evidence

All screenshots and the 15.84-second video are **SIMULATED**, generated locally
with the demo fixture and a test-only invoke wrapper. No real phone/TV operation,
APK, identity migration, release or deployment was performed. The reporter's
screenshot was inspected but is not redistributed here because it contains their
local device address. It shows v2.3.0 and a Google TV Streamer on Android14; the LG
G3 display in the report does not establish the Android host's firmware.

The confirmed defect is missing visible feedback, not established launcher
failure: the old UI placed results below the entire launcher list and Advanced
picker, and a refresh error hid the result and diagnostics entirely. The new
status is beside the clicked row, including when that row becomes Current
default. When refresh fails, the result remains above the error with an explicit
stale-state qualification. Diagnostic copy fallback also remains visible.
Backend success, refusal and unverified outcomes retain their distinct meaning;
a connection exception says the result cannot be confirmed. Initial progress is
visible before backend channel messages. Replies after leaving/changing the page
context do not update the new page or trigger stale refreshes.

`tests/launcher-feedback.mjs` covers 11 simulated cases:

- Verified success, refusal, accepted command with unchanged Home, and missing
  verification/error detail.
- Successful command followed by list/current-home/channel-provider refresh
  failure, plus refusal followed by refresh failure and clipboard denial.
- Interrupted command, cancelled takeover, and leaving while a command is pending.

Each completed attempt asserts exactly one original set-default command with
`allowStockDisable=false`; no added disable or takeover command is allowed.
Cancelling the existing takeover prompt never retries with opt-in enabled. The
backend and safety engine are unchanged; this does not prove device behavior.

Local validation passed: all 11 feedback cases, existing launcher-row/Advanced
picker/snapshot/ClearTV regressions, Svelte check with 0 errors and 0 warnings,
production frontend build, and regenerated 16 dark + 16 light gallery frames/GIFs.
Exact-head CI and independent review are the merge gates. No claim of
cross-platform or real-device validation is made from local browser checks.

Representative frames: [success](success-simulated.png),
[refusal](failure-simulated.png), [no-op](noop-simulated.png),
[refresh failure with diagnostic fallback](failure-refresh-simulated.png), and
[interruption](interrupted-simulated.png). The
[short recording](launcher-feedback-simulated.webm) includes the simulated outcome
sequence. SHA256 hashes in `visual-sha256.json` identify the submitted assets.

The migration-sensitive `com.shieldoptimizer.app` identifier and data paths remain
unchanged. PR211's experiment merge hold, PR180's approval gate and PR163's
physical/release acceptance gates remain separate. Issue212's actual device
launcher outcome is unverified; no reporter comment was needed to fix the
confirmed presentation defect.

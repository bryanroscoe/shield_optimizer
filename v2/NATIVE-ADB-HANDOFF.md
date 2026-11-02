# Native desktop ADB owner test — 2026-11-02

## Current delivery

The replacement implementation owner completed the immediate macOS acceptance.
The app is running and ready for Bryan's testing; the PR remains draft for owner
acceptance and the broader hardware checks below. Existing UI flows are preserved.
Normal builds still use platform-tools. No release or shipping-default change.

PR: https://github.com/bryanroscoe/shield_optimizer/pull/268
Branch: `feat/native-desktop-test-20261031`
Built and physically tested source: `3e2f6cede53f999c87373327fb5ade82c2f1e976`.
Download implementation: `b4df9fbd41b514a653fd5b20a7634d734476ead5`;
`3e2f6ce` only corrects a comment. Later acceptance documentation does not change
this binary.

- Delivery: `~/Developer/.agent-tasks/shield-native-test-20261031/ATV Optimizer Native Discovery Test.app`
- Window title: **ATV Optimizer — Native Rust ADB Test — Download Recovery Fix**
- Executable SHA256: `9ba9cd36f34db30425738ccfae0691b5a7da13e8a3e80c44e893f6998c07f865`
- Last verified running PID: **82951**. Recheck before subsequent work.
- Backend: native Rust, `native-adb-default` compiled in.
- Complete bundle ad-hoc signature passed strict verification. This is not
  Apple-issued signing or notarization.

Use the exact delivery path, not the bundle ID; the generated bundle shares
`com.shieldoptimizer.native-test`. Do not overwrite the running delivery. Quit it
through the app, verify exit, replace, verify hash/signature, then launch and
record the actual process/title. The previous screen-lock blocker is resolved.
Old Recovery Fix PID2949 and Transfer & Scan Fix PID50015 exited before replacement.

## What the replacement owner found and fixed

The staged 995fb2a app was launched and tested first. Discovery, retry and Forget
worked, and the corrected sync parser reported a real permission denial instead
of writing the error into the file. However, the denied download left an empty
local file and evicted the selected TV connection; the next directory browse
failed with `Not connected to a device`.

Native downloads now reuse the existing bounded, hardware-verified dedicated
connection path. They stage bytes beside the destination and publish only after
successful completion. A failed download preserves any previous destination,
removes its temporary file and leaves the main TV session usable. Uploads are
unchanged. No device write/replay or automatic reconnection is used to hide errors.

The real TCP regression
`native_download_publishes_only_success_and_keeps_selected_tv_connected` covers
success and fragmented rejection after partial data, with absent and existing
local destinations. It verifies a subsequent read on the original connection.

## Current native hardware evidence

On the exact delivered binary, using SHIELD TV (2017) and SHIELD TV Pro (2019):

- Both approved TVs appeared ready. First subnet responder arrived in 12 ms;
  authentication completed approximately 0.32 s and 0.58 s after subnet detection.
  These are transport timestamps, not frame-accurate UI-ready timings.
- Initial discovery finished in 6.52 s; repeat scan in 6.59 s. The mDNS window took
  about 6 s while subnet connections proceeded earlier. Subnet admission took 1.02 s
  initially and 0.28 s on repeat. Two samples on this LAN are not a cold-start bound.
- Initial discovery found 10 addresses; repeat found 13 and displayed 11 endpoint rows
  with 2 ready TVs. mDNS advertisements varied. Addresses/aliases are not physical
  TV counts, and this does not prove every unapproved TV has been discovered.
- Actual AUTH and failed cards remained visible. Retry changed one failed endpoint
  to Unauthorized. Forget removed its row and socket while both ready TV sockets
  stayed established. Repeat scan rediscovered that endpoint with AUTH guidance.
- Denied `//system/build.prop` pull displayed Permission denied and preserved a
  pre-existing local fixture byte-for-byte. The next directory browse succeeded
  without reconnecting. No error bytes or temporary download files remained.
- A subsequent `//system/etc/hosts` download through Files succeeded without
  reconnecting: 56 bytes, SHA256
  `425c3e713d5bae19b031bc8639c20c6a23e311a54647ba1824cbf45969a11ff4`.
- Actual native screenshots, AX states, socket checks and bounded app-log excerpts
  are local under `takeover-acceptance/`. No broad TV logs or TV modifications were
  used. Private LAN evidence has not been uploaded.

## Validation

All six CI checks passed at the exact built source 3e2f6ce:
https://github.com/bryanroscoe/shield_optimizer/actions/runs/41467806541
Linux/macOS/Windows Rust, desktop/mobile frontend and simulated-device E2E.
The new TCP regression was also individually verified in macOS CI at b4df9fb,
run 41466273759/job 125347897742. Existing vendor 123 tests passed there.

Local frontend check: 0 errors / 0 warnings. Production frontend build, workspace
formatting/diff checks, native macOS packaging and strict signature verification
passed. Packaging exited 0 without memory/growth cutoff. Nonfatal nice/strip
warnings occurred. The redundant local focused Rust test was canceled while
queued after its exact regression passed in CI; it is not a local test pass.

Earlier 829 native/core/shared tests and 123 vendor tests at 995fb2a are historical
coverage, not new local executions. No unchanged full suite was rerun merely
because the local compiler queue was occupied.

## Remaining limits

No completed physical TV approval, successful TLS PIN pairing, upload/install,
Remote, sleep/wake or packaged Windows/Linux hardware acceptance is claimed.
Fast native Remote remains gated. The earlier public screenshots/video are
explicitly simulated discovery evidence. Current native screenshots remain local;
a new publicly shareable native download video is still an evidence gap.
Owner-test readiness is distinct from merge/release certification.

## Workspace and cleanup

Reuse `~/Developer/.worktrees/shield_optimizer/native-test-20261031` (~36 MiB).
Keep unrelated primary-checkout changes untouched. Reuse the mixed-ownership
`~/Developer/shield-issue205-20261019/v2/target` (~7.4 GiB); do not broad-clean it.
All compiles use one job and the existing guard. The local runner/config and
receipts remain in `~/Developer/.agent-tasks/shield-native-test-20261031/`.

The only retained task delivery is the running app (~73 MiB). Superseded deliveries,
duplicate staged bundles, this turn's frontend dependencies/output and test
downloads were removed after verification; their small receipts remain. No own
compiler/watcher/server remains. Source, final app, small private evidence and
warm shared cache are retained for owner feedback, replacement implementation
owner, review 2026-11-07. Review dates are not deletion authority.

Read `app-receipt.json`, `takeover-acceptance/receipt.json`, `manifest.md` and
`takeover-cleanup-receipt.json` for exact local provenance and free-space readings.
`candidate-current/app-receipt.json` now records removal of the duplicate staging
copy; it is not another launch target.

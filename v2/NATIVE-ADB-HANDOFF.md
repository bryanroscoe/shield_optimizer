# Native desktop ADB owner test — 2026-11-02

## Current delivery

The integrated universal macOS owner-test app is running and ready for Bryan's
continued testing. PR #268 remains draft for owner acceptance and the hardware
checks below. Normal builds still use platform-tools. No release, tag, publication
or shipping-default change occurred.

PR: https://github.com/bryanroscoe/shield_optimizer/pull/268
Branch: `feat/native-desktop-test-20261031`
Built and physically tested source: `285f651267307435961787a23d179576a94ad195`.
Later handoff-only commits do not change this binary.

- Delivery: `~/Developer/.agent-tasks/shield-native-test-20261031/ATV Optimizer Native Discovery Test.app`
- Window title: **ATV Optimizer — Native Rust ADB Test — 285f651**
- Executable SHA256: `141431463f6195dd054610bd4ff7a49553e802fbb5ef3988a7aa37216f4f351e`
- Last verified running PID: **27809**, 2026-11-03 UTC. Recheck before subsequent work.
- Backend: native Rust, `native-adb-default` compiled in; label verified in UI.
- Architectures: **arm64 and x86_64**, verified with `lipo -verify_arch`.
- Complete bundle passed `codesign --verify --deep --strict`. Signing is ad-hoc,
  not Apple-issued signing or notarization.
- Build: https://github.com/bryanroscoe/shield_optimizer/actions/runs/41576615915
  Artifact `13204802989`; provenance and both archive/binary hashes verified before
  replacement. All seven jobs, including native packaging, succeeded.

Use the exact delivery path, not the bundle ID (`com.shieldoptimizer.native-test`).
The previous app was quit through its UI and its process exit verified before
replacement. The replacement was verified again at the delivery path and launched;
it was then quit/relaunched for a fresh-process discovery test. No security or
permission settings were changed. Superseded bundles and the downloaded archive
were removed after acceptance; small provenance receipts remain locally.

## Physical evidence on the delivered revision

Read-only testing used SHIELD TV (2017) and SHIELD TV Pro (2019):

- Both approved TVs appeared ready. Other discovered endpoints remained visible
  with actual AUTH guidance, connecting state or concrete failure messages.
- The first launch finished discovery in 6.77 s, repeat in 6.55 s, fresh-process
  relaunch in 6.60 s and rediscovery in 6.55 s. These are backend scan timestamps,
  not frame-accurate UI-ready timings. The mDNS observation window took about 6 s.
- **Open observation:** the first launch's subnet sweep returned zero responders
  in 12 ms, although mDNS still found both Shields. The three subsequent sweeps,
  including a fresh-process launch, found four responders in about 1.02 s with
  first responses in 12–14 ms. The cause of the initial zero is unproven; these
  samples do not establish a reliable cold-start bound or complete LAN coverage.
- Polling settled after outstanding handshakes failed: the Still connecting
  message cleared, concrete failure rows remained, and both TVs stayed ready.
- Each scan found 13 network addresses and settled to two ready TVs. Aliases are
  not additional physical TVs. This does not prove every unapproved TV is found.
- Retry of a timed-out IPv4 endpoint displayed AUTH guidance. Forget removed that
  endpoint, reducing the count from 11 to 10 while both ready TVs remained. Scan
  LAN rediscovered it and returned the count to 11.
- Denied `//system/build.prop` download displayed Permission denied, preserved an
  existing disposable local fixture byte-for-byte and left no staging file. The
  next directory browse succeeded without reconnecting.
- A subsequent `//system/etc/hosts` download succeeded without reconnecting:
  56 bytes, SHA256
  `425c3e713d5bae19b031bc8639c20c6a23e311a54647ba1824cbf45969a11ff4`.
- Native Devices, Settings and Files were visually inspected. Native Settings
  disabled platform-tools/server controls. Reports were disabled in this test
  build. Show system paths was restored off after read-only file testing.
- Actual screenshots, AX states and bounded scan-log excerpts are private under
  `acceptance-285f651/` in the local task directory. No device writes, broad TV-log
  collection, telemetry upload or public upload of LAN evidence was performed.

## Integration and validation

The built revision merges main `61e59ef112ad55896e639354475f01fc8babc386`
(PR #278: shell bookmarks, package-state filters, snapshot deletion and report-note
prefilling) into the native branch. It also includes main's earlier Settings and
mobile diagnostics work. Both sets of release notes were retained, both 17-screen
galleries regenerated, and representative light/dark fixture screens inspected.

Local merged frontend check: **0 errors / 0 warnings**. Production build,
app-state-filter, shell-bookmarks, snapshot-delete, app-report, native-discovery,
native-scan-progress and direct-adb-status regressions passed. Workspace formatting,
workflow validation and diff checks passed. No duplicate local compiler build was
started; the shared compiler guard remained intact while other projects used it.

The manual run above passed Linux/macOS/Windows Rust checks, desktop/mobile
frontend checks, simulated-device E2E and universal native packaging on the exact
built source. Normal PR CI also passed its six validation jobs; its optional
packaging job was skipped. That skip is not packaging evidence—the separate manual
run is. Simulated E2E and physical testing remain distinct evidence.

Historical integrated tests at `05f1d816c3fa237750d660c2ccc8c4fedce9f56b` passed
934 tests with one existing live test ignored, plus workspace/all-targets Clippy
with direct-adb and `-D warnings`. Its local offline E2E compile was stopped by the
disk floor before scenarios ran. The newer CI E2E pass supersedes that incomplete
local attempt; it does not turn the stopped local attempt into a pass.

## Behaviors to preserve

Native discovery streams subnet responders before mDNS finishes; owned connection
jobs have bounded handshakes, cancellation and exact-endpoint retry/Forget.
Authorization guidance requires an observed public-key AUTH packet. Verified
hardware identity alone collapses aliases. Unknown devices remain unknown.

Native sync parsing handles fragmented DATA/DONE/FAIL. Downloads use a dedicated,
bounded, hardware-verified connection and atomic local publication; rejection
preserves the destination and main session. The TCP regression
`native_download_publishes_only_success_and_keeps_selected_tv_connected` covers
success and partial-data rejection with existing/absent destinations, followed by
a read on the original connection. Upload semantics were not changed by this fix.

Native diagnostics skip host-server/session probes. Native Settings identifies the
backend and disables platform-tools actions. Subprocess timeout/startup/I/O errors
retain their real diagnostic and Restart instead of suggesting an ADB download.
The subprocess shipping default, existing UI flows and privacy controls remain.

## Remaining acceptance

No completed physical first-time TV approval, successful TLS PIN pairing, device
writes/upload/install, Remote, sleep/wake or packaged Windows/Linux hardware
acceptance is claimed. Fast native Remote remains gated. Investigate the initial
zero-responder sweep if it recurs; do not reset authorization keys or change OS
security settings to conceal it. Public UI media remains explicitly simulated;
there is no new public native download video. Owner-test readiness is distinct
from merge/release certification. No formal approval was submitted.

## Workspace and cleanup

Retain these existing paths for owner testing and feedback, review **2026-11-07**:

- `~/Developer/.worktrees/shield_optimizer/native-test-20261031` (~38 MiB), source
  checkout for PR follow-up. Preserve unrelated primary-checkout edits.
- The exact delivered universal app above (~144 MiB), intentionally running.
- `~/Developer/.agent-tasks/shield-native-test-20261031/`, small receipts and private
  acceptance evidence; parent/native ADB task owns this follow-up.
- `~/Developer/shield-issue205-20261019/v2/target` (~7.6 GiB), mixed historical Cargo
  ownership. It was not broadly cleaned and is not disposable under this task.

Own temporary frontend dependencies, build/capture output, superseded app,
archive/staging and transfer fixtures were removed. No own compiler, watcher or
server remains. Actual free-space readings during final cleanup were
25,297,993,728 → 25,410,953,216 bytes; concurrent jobs also affect these readings.
Review dates are review triggers, not deletion authority.

Local authoritative receipts: `app-receipt.json`, `acceptance-285f651/receipt.json`,
`cleanup-285f651.json`, `ci-285f651/provenance.json` and `manifest.md`.

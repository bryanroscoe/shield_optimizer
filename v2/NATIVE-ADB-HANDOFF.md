# Native desktop ADB owner test — 2026-11-02

## Duplicate discovery follow-up — 2026-11-03

Reproduced on the exact 66542dd app: two approved Shields plus additional IPv4/IPv6
approval cards, each repeating its address and a full authorization guide. Native
scan now tries alternative routes for one mDNS service serially and stops after a
ready route or an observed AUTH request. IPv4 failure can still fall back to IPv6;
a previous scan's success is rechecked before it suppresses alternatives. Distinct
services remain separate. This selects a connection route, not a physical identity:
device row deduplication still requires verified `ro.serialno`. Unknown endpoints
remain individually actionable, with the address shown once and one shared guide.

Frontend check/build and the expanded native scan UI regression passed locally.
Rust regression coverage includes route suppression, AUTH, IPv6 fallback, stale
success and separate services. The replacement app and cross-platform checks are
not yet delivered at this source checkpoint; the current delivery below remains
66542dd until its verified replacement receipt is recorded.

## Owner UI feedback — 2026-11-03

The Devices presentation now separates unconfirmed connecting/failed endpoints
into the existing collapsed Scan details, retaining per-address retry, Forget and
diagnostics. Observed AUTH requests remain in the main list. The heading counts
connected devices and approval requests rather than all discovered addresses.
Supported Shield cards omit redundant support badges and copy; other support
warnings and the device detail page are unchanged. Frontend regression coverage
checks collapsed failures, visible approval, exact endpoint actions and narrow
window layout and stale-action removal when the backend is unavailable. Svelte
check (zero errors/warnings), frontend build, native-scan-progress, native-discovery,
device-list-rows and support-tiers-ui passed. Both galleries were regenerated and
light/dark Devices captures reviewed. The previous mixed Cargo cache is now absent;
no fresh local compiler target was created. CI packaged the replacement, which is
now running at the delivery path below. Both approved Shields and pending AUTH
requests were observed on the real native screen. Fixture screenshots/video are retained locally; no PR response was posted
or edited without approved wording.

## Current delivery

The integrated universal macOS owner-test app is running and ready for Bryan's
continued testing. PR #268 remains draft for owner acceptance and the hardware
checks below. Normal builds still use platform-tools. No release, tag, publication
or shipping-default change occurred.

PR: https://github.com/bryanroscoe/shield_optimizer/pull/268
Branch: `feat/native-desktop-test-20261031`
Built source: `66542dd556d088ca5baba86a9d08a2280f265d3a`; native UI verification
is recorded below. Later main integration adds README/CHANGELOG documentation from
`f52a73e` and updates this handoff; it does not alter driver or UI implementation.
The delivered binary does not include those later documentation changes.

- Delivery: `~/Developer/.agent-tasks/shield-native-test-20261031/ATV Optimizer Native Discovery Test.app`
- Window title: **ATV Optimizer — Native Rust ADB Test — 66542dd**
- Executable SHA256: `4c8bf8da416c6c5cf41c021ca9cec851a25c3c531431973b93be974c56c11566`
- Last verified running PID: **48668**, 2026-11-03 UTC. Recheck before subsequent work.
- Backend: native Rust, `native-adb-default` compiled in; label verified in UI.
- Architectures: **arm64 and x86_64**, verified with `lipo -verify_arch`.
- Complete bundle passed `codesign --verify --deep --strict`. Signing is ad-hoc,
  not Apple-issued signing or notarization.
- Build: https://github.com/bryanroscoe/shield_optimizer/actions/runs/41650433571
  Artifact `13237238859`; provenance and both archive/binary hashes verified before
  replacement. All seven jobs, including native packaging, succeeded.

Use the exact delivery path, not the bundle ID (`com.shieldoptimizer.native-test`).
The previous app was quit through its UI and its process exit verified before
replacement. The replacement was verified again at the delivery path and launched. No security or
permission settings were changed. Superseded bundles and the downloaded archive
were removed after acceptance; small provenance receipts remain locally.

## Native UI acceptance at 66542dd

Launch verified the exact revision title and native backend label. Both approved
Shields appeared connected; observed AUTH requests remained visible. The first
subnet responder arrived in 21 ms, four subnet responders were admitted in 1.03 s,
and discovery finished in 6.60 s. These are backend timestamps, not a cold-start
guarantee. After pending handshakes expired, failed addresses remained in collapsed
Scan details and both TVs stayed connected. Expanded details retained exact-address
actions: Retry produced a visible AUTH row; Forget removed only that endpoint.
A rescan restored discovery. Shield support copy is absent from the compact cards.
Private native captures remain under `acceptance-66542dd/`.

## Prior driver acceptance at 285f651

The previous delivered binary underwent read-only testing using SHIELD TV (2017) and SHIELD TV Pro (2019):

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

The driver/UI base merges main `61e59ef112ad55896e639354475f01fc8babc386`
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
frontend checks, simulated-device E2E and universal native packaging on source
66542dd. Main f52a73e changed only README and CHANGELOG; the changelog conflict was
resolved by retaining both the known-issue warning and native progress notes.
Notes-seen, release-notes and diff checks passed on the merged documentation. Normal PR CI also passed its six validation jobs; its optional
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
- The former mixed Cargo target at `~/Developer/shield-issue205-20261019/v2/target`
  is now absent. This task did not remove it or create a new compiler cache.

Own temporary frontend dependencies, build/capture output, superseded app,
archive/staging and transfer fixtures were removed. No own compiler, watcher or
server remains. Actual free-space readings during final cleanup were
81,023,500,288 → 81,210,793,984 bytes; concurrent jobs also affect these readings.
Review dates are review triggers, not deletion authority.

Local current receipts: `app-receipt.json`, `acceptance-66542dd/receipt.json`,
`cleanup-66542dd.json`, `ci-66542dd/provenance.json` and `manifest.md`. Earlier
285f651 acceptance receipts preserve the separate download/driver test evidence.

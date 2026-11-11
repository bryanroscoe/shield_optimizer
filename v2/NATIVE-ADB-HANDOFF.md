# Native desktop ADB owner test — 2026-11-02

## Status at merge — 2026-11-08

PR #268 merged on 2026-11-09 and is on `main`, unreleased (2.5.0 predates it).

Built-in is the default engine, and Settings › ADB engine switches to Google platform-tools.
- **Scans never prompt.** Scan LAN, Add by IP, reconnects and every dedicated connection sign only with trusted keys. Only Request approval shows the TV's prompt: a pop-up that waits up to 10 minutes, with the same row and pop-up under both engines.
- **Engine switches don't prompt either.** The two engines share keys in both directions.
- **Fast Remote** is enabled under Built-in.
- **Test packages** can Send logs, marked as test builds. Their usage reports are always off.

Hardware evidence:
- Bryan ran the packaged macOS app on his LAN on 2026-11-08: scans, the Request approval wait and the engine switch.
- The Android TV 14 emulator covers the rest.
- Still unchecked on hardware: steps N1–N18 in `docs/DEVICE-TEST-SCRIPT.md` beyond those.

The sections below are the dated history. Where they say platform-tools is the default, that #268 is a draft, or that fast Remote is gated, this section supersedes them. The pre-merge slice records (`DIRECT-ADB-PARITY.md`, `NATIVE-DISCOVERY-ACCEPTANCE.md`, `NATIVE-INSTALL-ACCEPTANCE.md`, `RUST-ADB-ACCEPTANCE-GATES.md`) are archived in `docs/ARCHIVE-INDEX.md`; their current limits are in `DIRECT-ADB.md`.

## No reprompt and fast Remote — 2026-11-06

Bryan: "Ideally there is no reprompt, yes we need fast remote as well."

- **Key reuse.** Built-in offers platform-tools' `adbkey` first, then the app key.
  - The key is located as adb does: `<home>/.android/adbkey`, where home is `$HOME` or the Windows profile folder; adb ignores `ANDROID_USER_HOME`. It is read-only and read once per engine start.
  - AUTH signs with each key in turn, then offers the first key's public key.
  - TLS presents the key adbd names in its CA hints (`O=AdbKey-0, CN=<SHA-256>`), retrying once per other key if refused.
  - PIN pairing registers the first key.
  - `ADB_VENDOR_KEYS` names the app key for any adb server our commands start; a server another tool started keeps its keys.
  - `ATV_OPTIMIZER_DIRECT_KEY` means that key alone. Mobile is unchanged.
  - Emulator (Android TV 14): `[adbkey, fresh]` and `[fresh, adbkey]` connect with no prompt; a fresh key alone prompts with its own fingerprint; TLS picks the hinted key on the first try.
  - Quirk seen there: after a burst of authorized connections Android can drop a new prompt and adbd holds later ones. `service call adb 2` (deny, same as Cancel) clears it.
- **Fast Remote under Built-in** is enabled; Android is unchanged.
  - The harness is `src-tauri/tests/native_remote_emulator.rs` (`#[ignore]`, env-gated, never run by CI).
  - The real run found two bugs, fixed in `9eed0d9`:
    - cancelling startup cut the TV's main identity check short and dropped its connection;
    - a session idle past the 5-minute stream lifetime failed its next press. Sessions now renew first.
  - Emulator results:
    - start about 320 ms;
    - press p50 2.8 ms (shell input 105 ms);
    - held keys, text, 12 stop/reopen rounds, cancel, TLS and a relay that stops mid-session all pass.
  - Details are in `NATIVE-REMOTE-ACCEPTANCE.md`.
- Still open: Bryan's batch (N1–N18), which now also checks no-prompt switching (N2, N3, N14, N16) and fast Remote (N8). Real-TV Wi-Fi latency, sleep/wake and packaged Windows/Linux are unverified.

## Built-in is the default; ready for the hardware batch — 2026-11-05

- Bryan chose built-in Rust ADB as the default engine with Settings switching back to
  platform-tools. Normal builds include it (`default = ["native-adb-default"]`);
  `--no-default-features` builds the platform-tools-only app, which CI tests.
- Pair PIN connects by itself under built-in: the pair reports the TV's `[guid=…]` and
  `mdns services` is answered from one native browse (see `DIRECT-ADB.md`, Pairing).
- Test packages for the batch come from the PR's `native-test-app` label: macOS app,
  Windows installer, Linux AppImage. The last verified macOS app (`e84265b`, run
  42165613638, with key reuse and fast Remote) is in the task folder; Windows/Linux
  artifacts expire after 7 days (2026-11-13).
- Open: Bryan's batch (N1–N18), then the release. (Fast native Remote: enabled 2026-11-06, above.)

## Engine setting, device checks and the hardware batch — 2026-11-05

- **Settings → ADB → ADB engine** (Built-in / Google platform-tools) is saved in
  `<data folder>/adb-engine.json` and swaps the driver without relaunching. Precedence:
  `ATV_OPTIMIZER_TRANSPORT`, then the saved choice, then the build default (still platform-tools
  for normal builds; flipping it is Bryan's call). If the built-in engine cannot start (no key
  storage, or a build without it) the app uses platform-tools and Settings shows why, with Try
  again. Never per command. See `DIRECT-ADB.md`.
- **Checked without Bryan** with `vendor/adb_client/examples/device_smoke.rs` on the merged crate:
  both Shields (`suite` 28/28, `control` 17/17, single/split/3 MiB/corrupt/downgrade installs,
  devtest package removed) and the Android TV 14 emulator (plain and TLS: both suites, first-time
  approval, cancel during the prompt, PIN pairing). Logs stay local in the task folder. Found and
  fixed: Android 14 adbd closes ~1 in 10 TLS connections opened right after the previous one
  closed, before answering; the handshake now retries that once (`greet` in
  `adb_message_device.rs`).
- **Not run by agents:** the desktop app on Bryan's LAN. Its launch scan dials every TV, and two
  TVs there must not be prompted. Bryan's batch is `docs/DEVICE-TEST-SCRIPT.md` N1–N18.

## Main 2.5.0 and Kadb parity merged — 2026-11-05

Main `2af122f` is integrated: the 2.5.0 rename, the vendored `adb_client` Kadb parity rewrite
(#289), the macOS adb server repair (#286) and the Local Network notice (#288).

- **Vendored crate.** Main's rewrite is the base: CNXN/maxdata negotiation, one handshake
  deadline, monotonic stream ids, spare-OKAY tolerance (Android 14), desync poisoning, sync v2
  (`adb_sync.rs`), `ShellExit`, `install_apks` and `pull_with_limit`. #268's required-key
  constructors, cancelable handshake with AUTH observer, bounded shell-v2, bound services and
  guarded streamed install are rebuilt on it. #268's own sync parser is gone; its regression
  cases run on main's fake adbd and found one gap in main (a sync OKAY before DONE counted as a
  finished upload). A `Success` reply before the last APK byte is sent is not confirmed success.
  `SHIELD-OPTIMIZER-PATCH.md` describes both histories.
- **Shared transport (`crates/direct-adb`, now package `atv-direct-adb`).** Carries main's mobile
  fixes: an unreported or lost shell exit never becomes success, sync refusals keep the TV
  connected, lost framing drops the connection, TLS rejection says pair first while a TLS
  hang-up claims nothing. Installs use one dedicated, hardware-verified connection and then
  `install_apks`, so split installs (`install-multiple`) now work on desktop native too.
  Screenshots use `exec:` when a TV lacks shell v2.
- **Renames.** Feature `atv-optimizer/direct-adb`; `native-adb-default` is unchanged. Overrides
  `ATV_OPTIMIZER_TRANSPORT` and `ATV_OPTIMIZER_DIRECT_KEY` (never released, so no old names).
  Test bundle id `com.atvoptimizer.native-test`, so macOS asks for Local Network again once.
- **Key location.** `<data folder>/native-adb/identity.pkcs8.pem` in the session's data folder,
  which `rename.rs` copies from `ShieldOptimizer` with the folder's permissions. A key is never
  created in `migrate_host`'s working-directory fallback.
- **macOS.** `server_repair.rs` lives inside the platform-tools driver, so it never runs under
  native and stays for platform-tools and its fallback (Bryan, 2026-11-05). Under native a
  "No route to host" never suggests Restart ADB or a stale server: blocked says turn ATV
  Optimizer on and try again, unknown offers Privacy & Security only, allowed blames nothing.

## Approval clarity and offline-route follow-up — 2026-11-04

An overnight disconnected IPv6 route reproduced a gap after the two-Shield test:
the native transport still listed an unprofiled offline address after its scan
job ended, so it escaped Scan details and appeared as an extra Unknown card.
Unidentified offline network addresses now remain in Scan details with explicit
Disconnected state, Retry connection, Forget and diagnostics. Profiled offline
devices and USB rows retain their normal cards. No identities are inferred or
merged from addresses, advertised names or a previously used route.

Observed AUTH rows now say Approval requested and Retry approval. Shared guidance
makes the initial automatic request explicit, refers to the device rather than
assuming it is a TV, and does not promise that a dialog was displayed. Retry
reconnects only the selected endpoint. A native offline transport whose scan job
has ended now uses the same bounded connection path; it no longer reports
already authorized without retrying.

The UI regression reproduces the offline route without any pending/failed scan
job, exercises retry through approval and Forget while both ready TV cards stay,
and preserves profiled offline/USB cards. Rust regression coverage checks the
exact offline endpoint and leaves a ready TV untouched. CI owns Rust validation
and the replacement package; no local Cargo target was created. The previous
verified delivery below is historical. The exact latest app, package provenance
and physical acceptance are recorded in the local task's `app-receipt.json` and
dated acceptance receipts after delivery. Use that receipt before subsequent
live testing rather than assuming this source checkpoint is the installed app.

## Current integration and package pipeline — 2026-11-03

Main `d70e027` is integrated, including PR #282's automatic empty-scan help and
PR #280's mobile release preparation. Native mode still hides shared-server
restart controls, and connecting/approval rows do not count as an empty scan.
The duplicate-route fix and grouped address picker remain intact.

The `native-test-app` label opts a PR into the existing macOS native package job
after all six validation jobs succeed. It reuses that PR run instead of starting
a second full validation workflow. Manual dispatch remains available. `skip-ci`
still pauses validation, and unrelated label events do not start or cancel checks.
The job uploads a signed universal debug app, not a release or updater artifact.

For PR runs, `provenance.json` records the actual checkout commit (GitHub's merge
ref), its source tree and the PR head separately. Verify all three against the run
before accepting the app; do not assume the artifact name is the branch head.
Run `41736352516` passed all seven jobs and delivered the newer empty-help
integration. Its merge-ref tree exactly matches PR head `624fd2f`; the verified
app and physical acceptance are recorded below. The earlier manual `9383c7c`
package was never installed. The opt-in label was removed after acceptance so
documentation-only follow-up does not rebuild an identical app.

## Grouped connection choices retained — 2026-11-03

Owner feedback identified the optional address picker from prototype `efa1a18`.
That prototype's full passive-discovery screen was never part of this integration.
The current Devices flow now retains its useful service/interface grouping and
IPv4/IPv6 selector inside collapsed Scan details. Selecting an option does not
connect; its explicit Connect button uses the existing Add by IP command path.
Connected devices continue to collapse only by verified hardware identity. mDNS
grouping describes advertised routes, not a verified physical TV.

The scan returns the bounded snapshot it already collected, with observation age
advanced before returning. No extra discovery or property probes were added.
Cached, expired and unusable scoped addresses cannot connect; Scan LAN refreshes
the choices. Pairing ports never appear as connection choices. The selector only
mounts while Scan details is open and stops its freshness timer when closed.
Regression coverage includes exact selected address, no automatic connect from
selection/refresh, one verified TV row, separate interfaces, stale/cached/scope
guards, busy controls and narrow/light/dark fixture captures. This combined
candidate supersedes the uninstalled `8f74731` package; do not install an
intermediate candidate or restore the old prototype's whole Devices screen.

The combined branch also integrates main `5191667` (PR #281): Settings keeps
on-demand help and honest upload errors, with native server-only controls still
inactive and native-specific help retained. Local check/build, Settings, shared
error and native connection suites passed after resolving this merge. Both
17-screen galleries were regenerated and Settings reviewed in both themes.
The `04808b3` packaging run was superseded before installation; use the final
merged candidate and its exact receipt for physical acceptance.

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
success and separate services. The cross-platform checks and replacement app now
passed the duplicate-route and address-picker acceptance below. This is a focused
native owner-test result; broader hardware qualification remains open.

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

## Verified delivery — 2026-11-03

The integrated universal macOS owner-test app is running and ready for Bryan's
continued testing. PR #268 remains draft for owner acceptance and the hardware
checks below. Normal builds still use platform-tools. No release, tag, publication
or shipping-default change occurred.

PR: https://github.com/bryanroscoe/shield_optimizer/pull/268
Branch: `feat/native-desktop-test-20261031`
PR source: `624fd2f9033e0b7c1f597e7bd940056fc1abc16b`.
Built source: GitHub merge ref `697513bc6968437a4d411732abf6c0971d1b7ea7`.
Both commits have tree `8f17be73b5d98ca695ffa010da3e29b2fbe42db9`.
This receipt updates handoff documentation after that build; the delivered app
does not include these later documentation changes.

- Delivery: `~/Developer/.agent-tasks/shield-native-test-20261031/ATV Optimizer Native Discovery Test.app`
- Window title: **ATV Optimizer — Native Rust ADB Test — 697513b**
- Executable SHA256: `b1e20009353e58d21f69a2274550f00e32c14a78a8161a94cb9c92037a12de5e`
- Last verified running PID: **84531**, 2026-11-03 UTC. Recheck before subsequent work.
- Backend: native Rust, `native-adb-default` compiled in; label verified in UI.
- Architectures: **arm64 and x86_64**, verified with `lipo -verify_arch`.
- Complete bundle passed `codesign --verify --deep --strict`. Signing is ad-hoc,
  not Apple-issued signing or notarization.
- Build: https://github.com/bryanroscoe/shield_optimizer/actions/runs/41736352516
  Artifact `13279548229`; provenance and both archive/binary hashes verified before
  replacement. All seven jobs, including native packaging, succeeded.

Use the exact delivery path, not the bundle ID (`com.atvoptimizer.native-test`).
The previous app was quit through its UI and its process exit verified before
replacement. The replacement was verified again at the delivery path and launched. No security or
permission settings were changed. Superseded bundles and the downloaded archive
were removed after acceptance; small provenance receipts remain locally.

## Native UI acceptance at 697513b / PR source 624fd2f

Launch verified the exact revision title and native backend label. The approved
2017 Shield and 2019 Shield Pro appeared as two connected cards, one named card
per TV, across four scans. Backend log intervals from subnet detection to scan
completion were 6.31–6.78 seconds, with four subnet responders each time and first
responses in 12–17 ms. These are observed backend intervals, not a cold-start or
UI-readiness guarantee. Unapproved endpoints remained separate: observed AUTH
requests used compact rows and one shared guide; failed attempts settled into
collapsed Scan details.

The grouped service/interface address picker exposed the known Shield's IPv4 and
IPv6 routes. Selecting IPv6 changed the explicit Connect target without adding a
device. Pressing Connect succeeded over that IPv6 route and still left one card
per TV. Expired choices disabled Connect; Scan LAN refreshed them. Retry of one
failed IPv4 endpoint produced an observed AUTH row, Forget removed that endpoint
while both approved TVs stayed connected, and rescan restored it. Settings showed
the built-in native backend and disabled shared-server controls. Returning to
Devices retained both connected cards. Final native Devices and address-picker
captures were visually inspected.

Private screenshots, AX states, bounded scan excerpts and the exact receipt are
under `acceptance-624fd2f/` in the task directory. This acceptance covers the
duplicate-route/address-picker slice. Earlier physical download recovery testing
belongs to the earlier binary below; it was not repeated on this candidate.

## Prior native UI acceptance at 66542dd

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

## Prior integration and validation

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
Built-in is the shipping default (platform-tools stays selectable); existing UI flows and privacy controls remain.

## Remaining acceptance

No completed physical first-time TV approval, successful TLS PIN pairing, device
writes/upload/install, Remote, sleep/wake or packaged Windows/Linux hardware
acceptance is claimed. Investigate the initial
zero-responder sweep if it recurs; do not reset authorization keys or change OS
security settings to conceal it. Public UI media remains explicitly simulated;
there is no new public native download video. Owner-test readiness is distinct
from merge/release certification. No formal approval was submitted.

## Workspace and cleanup

Retain these existing paths for owner testing and feedback, review **2026-11-10**:

- `~/Developer/.worktrees/shield_optimizer/native-test-20261031` (~44 MiB), source
  checkout for PR follow-up. Preserve unrelated primary-checkout edits.
- The exact delivered universal app above (~145 MiB), intentionally running.
- `~/Developer/.agent-tasks/shield-native-test-20261031/`, small receipts and private
  acceptance evidence; parent/native ADB task owns this follow-up.
- The former mixed Cargo target at `~/Developer/shield-issue205-20261019/v2/target`
  is now absent. This task did not remove it or create a new compiler cache.

Own temporary frontend dependencies, build/capture output, superseded app and
downloaded archive/staging were removed. No own compiler, watcher or server
remains. Actual free-space readings during final cleanup were
67,420,688,384 → 67,608,064,000 bytes; concurrent jobs also affect these readings.
The small existing root `node_modules` cache (~560 KiB) remains because its prior
ownership is unverified. Review dates are review triggers, not deletion authority.

Local current receipts: `app-receipt.json`, `acceptance-624fd2f/receipt.json`,
`cleanup-624fd2f.json`, `ci-624fd2f/provenance.json` and `manifest.md`. Parent/root
owns the retained source, app and private evidence for owner testing. Earlier
285f651 acceptance receipts preserve the separate download/driver test evidence.

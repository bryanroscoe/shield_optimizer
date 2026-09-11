# Mobile restart report: lifecycle evidence

Source-review addendum (Navigator workspace date 2026-09-07):
[new feedback UX contract](FEEDBACK-UX-2026-09-07.md) covers saved discovery,
Unknown risk, process/app identity, details and background-control copy at the
same `48cba23` baseline. No lifecycle test, GUI, build or device action was rerun
for that review while Bryan was using the host. It adds no persistence evidence
and does not close the owner-driven physical gate below. Its implementation and
integrated-candidate acceptance remain separate from this baseline report.

Navigator, `so-vtm.8`, convoy `hq-cv-z4oev`. Recorded 2026-09-05 America/Chicago
(2026-09-06 UTC), against unchanged application source at
`48cba2384ca3938e6655b77ccf85716dd86cca5f` in `crew/navigator`.

**Finding:** host visibility-only resume preserves the current screen. A fresh
JavaScript context resets the screen, live device, and health cache. With one
saved TV and auto-connect enabled, reload connects again and shows **Connected**,
then requires **Open dashboard**; it does not restore the prior detail screen.
This reproduces a restart-like experience in a browser, but does not establish
which Android event caused Bryan's report or prove a physical fix.

## Pending navigation candidate is separate

Mechanic has locally accepted `so-vtm.3`, a three-file uncommitted candidate on
`48cba23`: Onboarding, its session browser tests, and feedback #5. Mechanic's bead
records **20 passing worker browser tests** plus check/build and hash verification;
Navigator did not rerun that candidate. Its saved-TV success path calls
`onConnected()` after the device profile resolves and opens Dashboard directly.
First-time/add-TV connections retain the Connected/Open dashboard interstitial.
This changes the baseline reload destination; it does not add prior-screen or
live-session persistence. Physical verification and publication remain gated.

The immutable application patch is at
`/Users/bryanroscoe/.local/share/gastown/evidence/shield_optimizer/so-vtm.3-d_91juzj/application.patch`
(SHA-256 `5cb8b816f7e0e7f79b3ec8167288ad9c74c7f9ed56e7c55f463fddd8c1953a51`).
This review read that patch and manifest; it did not edit the preserved Rust tree
or `USER-FEEDBACK-2026-09-05.md`. All 13 results below remain **baseline-only**.
The harness's Connected/Open dashboard assertions intentionally describe baseline
behavior and must be revised under a separate evidence scope before it can serve
as a candidate acceptance suite. No candidate rerun is requested.

## Report and authority

[Feedback #7](USER-FEEDBACK-2026-09-05.md#7-briefly-leaving-and-reopening-the-app-restarts-the-experience)
records Bryan's observation after installing a debug build from `main` at
`44d2d66` on a Pixel 10 Pro. The leaving/reopening action and lifecycle are not
identified. This owner-reported install supersedes older handoff assertions that
nothing after `bb33ecc` had reached a phone; it is not a completed stability matrix.

The current assignment permits host-only review, evidence, and documentation.
No phone or TV was attached, queried, installed, paired, launched, or changed for
this work. No application code was changed, no persistence was added, and nothing
was deployed. The evidence was initially captured uncommitted; its Navigator-owned
documentation and host harness are now committed and pushed under the September 10
policy in `AGENTS.md`. That source-control milestone is not application integration,
physical verification, or release. Mechanic owns integration and file reservations;
Sol implements any bounded follow-up. The physical investigation remains open on
`so-vtm.8`; historical device instructions do not authorize running them now.

## Reproduce on the host

From `v2/mobile/`, with dependencies from the checked-in lockfile:

```sh
npm ci --ignore-scripts
# Only if the matching host browser is missing:
npx playwright install chromium
node --test evidence/lifecycle/reproduce.mjs
npm test
npm run check
npm run build
```

The harness starts a loopback Vite development server and Chromium at 384×812.
All Tauri invokes go to an explicit host fixture; unexpected commands fail.
Browser HTTP requests outside the loopback origin are blocked, and mock hostnames
use `.invalid`. No Android or ADB binary runs. Fixture devices are test data only.
The production build is checked separately; these browser cases exercise Vite's
compiled source, not an installed APK or the production bundle.

Visibility cases override `document.visibilityState`, dispatch `visibilitychange`,
and advance Playwright's virtual clock. They test frontend event handling and the
45-second heartbeat, not actual OS backgrounding, real elapsed grace time, timer
throttling, Android suspension, or scrcpy reuse. Reload reconstructs JavaScript;
the host fixture separately models an empty or retained native connection. The
fresh-context case copies only browser storage, with an empty fixture backend.

Default output: `/tmp/so-vtm.8-lifecycle/observations.json`. Set
`LIFECYCLE_OUTPUT` to choose a separate directory. To review an integration
candidate without editing it, set `LIFECYCLE_MOBILE_ROOT` to its absolute mobile
path; its Vite/Svelte dependencies must be available and assertions must match the
intended candidate behavior. In particular, do not use the baseline saved-TV
interstitial assertions to reject the accepted `so-vtm.3` navigation repair.
The output records the actual source HEAD, dirty paths, source hashes, runtime versions, state, and invoke
sequence. Do not carry baseline results forward to a changed candidate.

Captured baseline: [observations](evidence/lifecycle/baseline-48cba23.json) and
[test output](evidence/lifecycle/baseline-48cba23.tap). Host was macOS arm64,
Node 26.3.0, Chromium 148.0.7778.96. An initial harness run was stopped after a
fixture-name assertion mismatch; the fixture was corrected and the full run below
passed. No application fix was needed for that harness error.

Durable owner-controlled archive: [so-vtm.8-3mgugq2g](/Users/bryanroscoe/.local/share/gastown/evidence/shield_optimizer/so-vtm.8-3mgugq2g). It preserves
[observations.json](/Users/bryanroscoe/.local/share/gastown/evidence/shield_optimizer/so-vtm.8-3mgugq2g/observations.json),
[the final browser log](/Users/bryanroscoe/.local/share/gastown/evidence/shield_optimizer/so-vtm.8-3mgugq2g/browser.tap),
[command results](/Users/bryanroscoe/.local/share/gastown/evidence/shield_optimizer/so-vtm.8-3mgugq2g/command-results.txt), the exact harness, and a
[SHA-256 manifest](/Users/bryanroscoe/.local/share/gastown/evidence/shield_optimizer/so-vtm.8-3mgugq2g/manifest.json). The manifest hashes each archived
artifact; it does not establish physical validation. Application documentation
patches and the workspace-only role patch are separate. Existing `.gitignore`,
`.beads`, and other runtime state are excluded from application publication scope.

## Controlled results

All **13 focused browser cases passed**. The table groups related cases.

| Case | Observed result | What it establishes |
| --- | --- | --- |
| Hidden/visible at virtual 5, 35, and 50 seconds (3 cases) | Diagnostics and live serial retained; zero hidden probes; one probe on visible; no new connect | Frontend visibility handler does not reset the router; crossing 30 seconds in browser has no native meaning |
| Lost fixture socket on resume | One reconnect attempt fails; prior Diagnostics screen remains; `isConnected=false`; Retry shown; no further auto-dial over 90 virtual seconds | Failed recovery is bounded in this loss episode |
| Visible heartbeat | No probe at 44,999 ms; one at 45,000 ms | Timer cadence, not a promise of a 45-second completed disconnect diagnosis |
| Reload with empty/retained fixture backend (2 cases) | Pending new connect has empty serial, no health cache, and onboarding screen; successful handshake/profile reaches Connected; Open dashboard lands on Dashboard | Local saved-TV data does not restore live session or prior screen; native presence alone is not adopted |
| Fresh browser context with saved storage | One fresh connect; Connected confirmation on onboarding route; health cache absent | Surviving storage provides connection bookkeeping, not a prior live runtime |
| Explicit Disconnect then reload | Auto-connect preference is `0`; Which TV? waits; no dial for 90 virtual seconds | Intentional disconnect survives reload while storage is available |
| Two saved TVs with a retained fixture connection | Which TV? waits; no dial; frontend remains disconnected | Multi-TV launch requires a choice even if the mock native layer has a connection |
| Empty saved list | Find your TV; no scan or connect | Clean start is user-driven |
| Browser Back from Diagnostics | Spare browser entry consumed; Dashboard reached | JavaScript history behavior only; Android root Back outcome untested |
| Completed mock Volume up then reload | Exactly one `send_key` total | That completed mock action is not replayed; interrupted native mutations are not covered |

The existing **9 browser regressions passed**. `npm run check` reported **0 errors
and 0 warnings**, and `npm run build` passed. Vite emitted a separate missing
parent `v2/.svelte-kit/tsconfig.json` warning in this fresh mobile-only workspace;
that warning is included in the raw output. `npm ci --ignore-scripts` installed
74 packages and reported four high-severity dependency audit findings; dependencies
and lockfiles were not changed or audited further in this lifecycle review.

The three existing pure native lifecycle tests also passed, compiled directly:

```sh
# From repository root:
mkdir -p /tmp/so-vtm.8-lifecycle
rustc --edition=2021 --test v2/mobile/src-tauri/src/remote_lifecycle.rs \
  -o /tmp/so-vtm.8-lifecycle/remote-lifecycle-tests
/tmp/so-vtm.8-lifecycle/remote-lifecycle-tests --nocapture
```

They verify cleanup is claimed once, resume invalidates the pending epoch, and a
newer suspend invalidates an older epoch. On macOS the Android-only handler is
excluded. This is not Android compilation or proof that the timer closes a real
channel. Full Rust workspace, cross-OS CI, APK, pairing, and physical background
matrices were not rerun in this documentation/evidence pass.

## Implemented persistence boundary

| State | Location | Reconstruction behavior |
| --- | --- | --- |
| Router stack/current screen | `src/lib/router.svelte.ts`, runes singleton | Starts at `onboarding`; no durable route restore |
| Device, host, liveness, apply flag, health/bloat caches | `src/lib/session.svelte.ts`, runes singleton | Starts empty/idle; connections must be resolved through the API; no operation resume journal |
| Saved TVs | `atv.savedDevices.v1` in localStorage | Host, port, label, type, optional hardware ID, last-used time; at most 16 written rows; equal hardware ID is the only cross-endpoint identity, while ID-less rows match only exact host+port; does not prove current reachability |
| Launch auto-connect preference | `atv.autoConnect.v1` in localStorage | `0` disables; exactly one saved TV with enabled preference may dial; multiple TVs wait |
| Remote compatibility preference | `atv.remote.forceShell` in localStorage | Remote transport preference only |
| ADB identity and license | Rust-managed app data, not the router | Key/validated license may survive ordinary restart; this pass did not read private app data or verify Android storage retention |
| Fast-remote control channels | Shared native session registry | 30-second suspension cleanup; no frontend route/session serialization |

Storage loss, uninstall/clear-data, profile read failure, Android renderer death,
and interrupted backend mutation outcomes are not covered by the passing browser
cases. Saved connection metadata must never be treated as a live device, and
unfinished mutations must never be automatically replayed to emulate persistence.

The identity rule in that table is the Mechanic-local `so-fb3.1.2` integration
(`savedDevices.ts` `163472a8`), not behavior exercised by this document's
baseline 13-case lifecycle run. It intentionally retains conflicting or
unverified histories as separate rows, including two entries at one address;
that makes the saved count greater than one and suppresses single-TV auto-dial.
`so-fb3.1.3` tracks the remaining screen consequence: Onboarding and Devices
must distinguish those rows for rendering, progress, current-TV filtering, and
forget actions. Until that lands, a shared host can highlight or hide the wrong
row, and an identical endpoint can make the two-argument forget operation target
the MRU identity rather than the row the user selected. None of this establishes
the physical cause of feedback #7.

## Android wiring inspected, not executed

Repository source paths:

- `src-tauri/gen/android/app/src/main/java/com/atvoptimizer/mobile/MainActivity.kt`:
  `onCreate` enables edge-to-edge then delegates to `TauriActivity`. It has no
  app-specific screen/session save/restore implementation.
- `src-tauri/gen/android/app/src/main/AndroidManifest.xml`: `singleTask` activity
  with explicit handled `configChanges`, including orientation and screen size.
  Rotation therefore cannot simply be assumed to recreate this Activity.
- `src-tauri/src/lib.rs`: the Android run loop forwards Tauri window events to
  `remote_lifecycle::handle_window_event`.
- `src-tauri/src/remote_lifecycle.rs`: Suspended schedules an epoch-bound 30-second
  cleanup; Resumed invalidates the epoch. The sole cleanup target is
  `AppState::drop_all_remote_sessions` in `../crates/core/src/commands/state.rs`,
  which drains/closes remote control sessions. It does not save or reset the
  frontend router or serialize the ordinary wireless connection.
- `src/App.svelte`: probes on visible resume and every 45 seconds while visible;
  the probe requires a frontend device. This is independent of the native grace.

Pinned dependency sources from the local Cargo registry were also inspected;
versions match `../Cargo.lock`:

| Source | Static wiring |
| --- | --- |
| Wry 0.55.1 `src/android/kotlin/WryActivity.kt` | ProcessLifecycleOwner observer calls Rust pause/resume; Activity pause/resume delegates to the WebView; destruction notifies Rust; saved state includes an Activity ID |
| Tao 0.35.3 `src/platform_impl/android/ndk_glue.rs` and `mod.rs` | Pause maps to Suspended, later resume to Resumed; initial resume is specially suppressed |
| tauri-runtime-wry 2.11.2 `src/lib.rs` | Mobile suspend/resume events become window events delivered to the run callback |
| Tauri 2.11.2 `mobile/android-codegen/TauriActivity.kt` and `mobile/android/src/main/java/app/tauri/AppPlugin.kt` | Tauri disables Wry's generic Back handler and supplies its own: WebView goes back when possible, otherwise delegates to Android; a registered back-button listener has a separate branch |

An upstream Activity ID or WebView lifecycle callback is not app-level restoration
of Svelte state. Native delegation at root is also not proof of process death or
of the exact behavior of Back on this Pixel/Android version. The browser tests do
not invoke those Kotlin callbacks. Actual Activity recreation might or might not
replace JavaScript; the observed reset applies when JavaScript is reconstructed.

## Physical reproduction gate and minimal owner evidence

**Gate:** keep the restart report open until an owner-driven, explicitly authorized
physical session identifies the leaving action and correlates it with the runtime
lifecycle. Do not request pairing codes repeatedly, attach to a device, install a
new APK, change developer settings, clear data, force-stop, send TV keys, or replay
Optimize while this host-only assignment remains in force.

The minimum useful owner report can come from ordinary use of the installed build:

1. Record build/commit if known, phone Android and WebView versions if available,
   one versus multiple saved TVs, whether Disconnect was used, starting screen,
   and whether any operation was still pending. Unknown values remain unknown.
2. Name the exact action: **Home**, switching through **Recents without dismissal**,
   **Android Back at the root**, **swiping the app away**, or **locking the phone**.
   Record whether reopening used the launcher icon or the Recents card and the
   elapsed time. These actions must not be collapsed into “closed the app.”
3. Record what returned: prior screen, splash, Find your TV, Which TV?, Connecting,
   Connected/Open dashboard, or Lost/Retry; also whether the correct TV name stayed.
   A short owner-captured before/after recording or timestamped description is enough
   to classify the visible symptom; no TV mutation is needed.
4. If available in that authorized session, correlate the timestamps with app
   `app suspended`, `app resumed`, and `background grace expired` logs. To distinguish
   process death, Activity recreation, and renderer loss, capture PID/process-start,
   Activity create/destroy, and renderer/document identity evidence. Existing remote
   epoch logs alone cannot make this distinction; report it as unknown when absent.
   Any additional instrumentation belongs to a separate mechanic-reserved Sol task.

A later controlled physical matrix should separate Home/Recents-switch returns at
roughly 5 seconds and over 30 seconds from Back-at-root, Recents dismissal, and
recreation/process-loss cases. Lock/unlock is another distinct row. Do not induce
process death or change lifecycle developer options without authorization. First
use a read-only screen and an idle TV; an interrupted mutation test is separate
scope. A live fast-remote grace test is also separate from this frontend screen
retention report and still requires the physical Phase 4 evidence.

Interpretation after evidence: same document plus a route reset points toward a
frontend transition; a new document explains reconstruction but not why Android
recreated it; retained screen with Lost/Retry points toward connection recovery.
Neither a splash nor a reconnect by itself proves process death. No persistence
implementation is justified solely by this host matrix.

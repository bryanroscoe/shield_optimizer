# Mobile restart report: lifecycle evidence

Tracks [#105](https://github.com/bryanroscoe/shield_optimizer/issues/105). On a Pixel 10 Pro debug
build, briefly leaving the app and reopening it restarted the whole experience. Which Android
event caused that is not known: background/resume, Activity or WebView recreation, process death
and dismissal from Recents all look alike from the outside. The report stays open until a physical
session identifies the leaving action and correlates it with the runtime lifecycle.

**What the host evidence shows:** a visibility-only resume preserves the current screen. A fresh
JavaScript context resets the screen, the live device and the health cache. That reproduces a
restart-like experience in a browser, but it does not establish which Android event caused the
report, and it is not a physical fix. No persistence change is justified by the host matrix alone.

## Reproduce on the host

From `v2/mobile/`, with dependencies from the checked-in lockfile:

```sh
npm ci --ignore-scripts
# Only if the matching host browser is missing:
npx playwright install chromium
node --test evidence/lifecycle/reproduce.mjs
```

The harness starts a loopback Vite development server and Chromium at 384×812. All Tauri invokes
go to an explicit host fixture; unexpected commands fail. Browser HTTP requests outside the loopback
origin are blocked, and mock hostnames use `.invalid`. No Android or ADB binary runs. The cases
exercise Vite's compiled source, not an installed APK or the production bundle.

Visibility cases override `document.visibilityState`, dispatch `visibilitychange` and advance
Playwright's virtual clock. They test frontend event handling and the 45-second heartbeat, not real
OS backgrounding, elapsed grace time, timer throttling, Android suspension or scrcpy reuse. Reload
reconstructs JavaScript; the fixture separately models an empty or retained native connection.

Output defaults to `/tmp/so-vtm.8-lifecycle/observations.json`; set `LIFECYCLE_OUTPUT` to choose
another directory, or `LIFECYCLE_MOBILE_ROOT` to run against another checkout's `v2/mobile`. The
output records the source HEAD, dirty paths, source hashes, runtime versions, state and invoke
sequence. The recorded baseline run is in `evidence/lifecycle/`.

**Four reload cases are out of date.** They still expect a saved-TV reconnect to stop on a
Connected / Open dashboard interstitial, but a saved-TV connect now opens the Dashboard directly
once the device profile resolves (first-time and Add-a-TV connections keep the interstitial).
Against current source those four time out waiting for the Connected heading; the other nine pass.
Update their assertions before relying on them.

## The matrix

| Case | Expected result | What it establishes |
| --- | --- | --- |
| Hidden/visible at virtual 5, 35 and 50 seconds (3 cases) | Diagnostics and live serial retained; no hidden probes; one probe on visible; no new connect | The visibility handler does not reset the router; crossing 30 seconds in a browser has no native meaning |
| Lost fixture socket on resume | One reconnect attempt fails; prior screen remains; `isConnected=false`; Retry shown; no further auto-dial over 90 virtual seconds | Failed recovery is bounded per loss episode |
| Visible heartbeat | No probe at 44,999 ms; one at 45,000 ms | Timer cadence, not a promise of a 45-second disconnect diagnosis |
| Reload with empty/retained fixture backend (2 cases) | A fresh connect with empty serial and no health cache | Saved-TV data does not restore the live session or prior screen; native presence alone is not adopted |
| Fresh browser context with saved storage | One fresh connect; health cache absent | Surviving storage provides connection bookkeeping, not a prior live runtime |
| Explicit Disconnect then reload | Auto-connect preference `0`; Which TV? waits; no dial for 90 virtual seconds | Intentional disconnect survives reload while storage is available |
| Two saved TVs with a retained fixture connection | Which TV? waits; no dial | Multi-TV launch requires a choice even if the native layer has a connection |
| Empty saved list | Find your TV; no scan or connect | A clean start is user-driven |
| Browser Back from Diagnostics | Spare browser entry consumed; Dashboard reached | JavaScript history only; Android root Back is untested |
| Completed mock Volume up then reload | Exactly one `send_key` | A completed action is not replayed; interrupted native mutations are not covered |

The pure native lifecycle tests compile directly:

```sh
# From the repository root:
mkdir -p /tmp/lifecycle
rustc --edition=2021 --test v2/mobile/src-tauri/src/remote_lifecycle.rs -o /tmp/lifecycle/tests
/tmp/lifecycle/tests --nocapture
```

They verify cleanup is claimed once, resume invalidates the pending epoch, and a newer suspend
invalidates an older one. On macOS the Android-only handler is excluded, so this is not Android
compilation or proof that the timer closes a real channel.

## What persists across a restart

| State | Location | Reconstruction behavior |
| --- | --- | --- |
| Router stack/current screen | `src/lib/router.svelte.ts`, runes singleton | Starts at `onboarding`; no durable route restore |
| Device, host, liveness, apply flag, health/bloat caches | `src/lib/session.svelte.ts`, runes singleton | Starts empty; connections must be resolved through the API; no operation resume journal |
| Saved TVs | `atv.savedDevices.v1` in localStorage | Endpoint, label, type, optional hardware id or local id and fingerprint, last-used time; at most 16 rows; does not prove current reachability |
| Launch auto-connect preference | `atv.autoConnect.v1` in localStorage | `0` disables; only a lone saved TV with the preference enabled may dial |
| Remote compatibility preference | `atv.remote.forceShell` in localStorage | Remote transport preference only |
| ADB identity and license | Rust-managed app data | May survive an ordinary restart; Android storage retention is unverified |
| Fast-remote control channels | Shared native session registry | 30-second suspension cleanup; no frontend serialization |

Storage loss, uninstall/clear-data, profile read failure, renderer death and interrupted backend
mutations are not covered by the browser cases. Saved connection metadata must never be treated as
a live device, and unfinished mutations must never be replayed automatically to emulate
persistence.

## Android wiring (read, not executed)

- `src-tauri/gen/android/app/src/main/java/com/atvoptimizer/mobile/MainActivity.kt`: `onCreate`
  enables edge-to-edge and delegates to `TauriActivity`. There is no app-specific screen or session
  save/restore.
- `src-tauri/gen/android/app/src/main/AndroidManifest.xml`: a `singleTask` activity with explicit
  handled `configChanges`, including orientation and screen size, so rotation cannot be assumed to
  recreate the Activity.
- `src-tauri/src/lib.rs`: the Android run loop forwards Tauri window events to
  `remote_lifecycle::handle_window_event`.
- `src-tauri/src/remote_lifecycle.rs`: Suspended schedules an epoch-bound 30-second cleanup;
  Resumed invalidates the epoch. The only cleanup target is `AppState::drop_all_remote_sessions`
  in `../crates/core/src/commands/state.rs`. It does not touch the router or the ordinary wireless
  connection.
- `src/App.svelte`: probes on visible resume and every 45 seconds while visible, independent of the
  native grace.

Upstream, at the versions in `../Cargo.lock`: Wry's `WryActivity.kt` forwards process and Activity
pause/resume and destruction to Rust; Tao maps pause to Suspended and a later resume to Resumed;
tauri-runtime-wry delivers those as window events; Tauri's `TauriActivity.kt`/`AppPlugin.kt`
replace Wry's Back handler (WebView goes back when it can, otherwise Android handles it). None of
that is app-level restoration of Svelte state, and the browser tests do not invoke those Kotlin
callbacks.

## Physical reproduction gate

The minimum useful report can come from ordinary use of an installed build:

1. Record the build, the phone's Android and WebView versions if available, one versus multiple
   saved TVs, whether Disconnect was used, the starting screen, and whether any operation was
   pending. Unknown values stay unknown.
2. Name the exact action: **Home**, switching through **Recents without dismissal**, **Android Back
   at the root**, **swiping the app away**, or **locking the phone**. Record whether reopening used
   the launcher icon or the Recents card, and the elapsed time. Don't collapse these into "closed
   the app".
3. Record what came back: the prior screen, splash, Find your TV, Which TV?, Connecting, Connected,
   or Lost/Retry, and whether the right TV name stayed. A short before/after recording or a
   timestamped description is enough; no TV mutation is needed.
4. Where possible, correlate the timestamps with the app's `app suspended`, `app resumed` and
   `background grace expired` logs. Telling process death, Activity recreation and renderer loss
   apart needs PID/process-start, Activity create/destroy and document identity evidence; the
   remote epoch logs alone cannot make that distinction, so report it as unknown when absent.

A controlled matrix should then separate Home/Recents returns at about 5 seconds and over 30
seconds from Back at the root, Recents dismissal, recreation/process loss, and lock/unlock. Use a
read-only screen and an idle TV first; an interrupted-mutation test is separate scope, and so is the
live fast-remote grace test (`FAST-REMOTE-PLAN.md` Phase 4). Don't induce process death or change
lifecycle developer options without the owner's go-ahead.

Reading the result: the same document plus a route reset points to a frontend transition; a new
document explains the reset but not why Android recreated it; a retained screen with Lost/Retry
points to connection recovery. Neither a splash nor a reconnect proves process death on its own.

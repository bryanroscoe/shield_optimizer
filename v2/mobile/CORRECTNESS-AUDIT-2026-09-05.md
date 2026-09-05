# Repository correctness audit — 2026-09-05

## Fix pass — 2026-09-05

All nine findings below are addressed in the working tree. The original findings are retained as the reproduction record. Desktop/shared-core and mobile were in scope; v1 was not. The other agent's handoff/backlog/cloud-task files remain untouched.

- Optimize navigation is locked centrally, including hardware Back; Cancel remains reachable, and the apply loop stops if its session changes.
- Remote input captures its target and generation, discards queued work on destruction/session change, and caps ordinary taps at eight pending/in-flight presses.
- Native connection publication is guarded by monotonic request IDs. Cancellation invalidates an unfinished handshake and removes a just-published canceled connection without dropping a newer connection. Onboarding uses per-attempt IDs for same-host retries. Profile refresh accepts only the expected authorized endpoint.
- Health and profile transport failures propagate; the frontend preserves the previous health report with an error. Recovery is limited to one automatic attempt per loss episode. Late errors from older generations cannot start recovery on the new TV.
- Downloads use a size-limited writer and a dedicated socket within the existing WirelessAdb driver, so a rejected local write does not evict the main control socket. Temporary files are removed on failure; successful downloads are published without overwriting existing files.
- Apps, Optimize, snapshot planning, and Launcher require successful installed/disabled read sections. Batches carry individual exit statuses and completion markers; optional HOME/memory reads may fail without inventing package state.
- Reboot completion uses the initiating generation and no longer schedules an unscoped delayed disconnect.

Regression coverage now lives in `tests/session.test.mjs` and the affected Rust modules. CI runs the mobile browser suite, the entire Rust workspace (including the license CLI), and the excluded vendored transport tests. Third-party notices were regenerated after promoting tempfile to a runtime dependency.

Validation for the fix pass: 9 browser regressions passed; core 186, mobile 26, desktop 35, and license CLI 2 Rust tests passed; vendored transport 54 passed with 1 device-dependent test ignored. Workspace formatting/Clippy passed. Desktop and mobile Svelte checks reported 0 errors/0 warnings and both production builds passed. Android aarch64 Clippy and an arm64 debug APK build also passed. These are local results; cross-OS CI and physical phone/TV stability, pairing, and background/resume tests still need to run.

PowerShell clarification: `pwsh` could not be located through PATH, common install locations, or Spotlight on this machine. The initial audit's claim that it was not installed was stronger than the evidence established.

## Original audit

Reviewed HEAD `34d0575` on `feat/atv-optimizer-mobile`. This was a review, not a fix sweep. No production source was changed. `HANDOFF.md`, `BACKLOG.md`, and `CLOUD-TASK.md` were left to the other agent.

The mobile app still has reproducible lifecycle bugs despite passing the host gates. The strongest findings concern commands surviving navigation, connection cancellation that only affects the frontend, and backend failures being converted into successful responses. These plausibly explain reported symptoms; no physical device was used to attribute a particular user incident to them.

## Findings

### 1. P1 — Back abandons Optimize's controls while mutations continue

Sources: `src/lib/router.svelte.ts:107`, `src/screens/Optimize.svelte:225`, `src/components/BottomTabs.svelte:28`.

The tab buttons honor `session.applyInProgress`, but the hardware-back/popstate handler calls `router.back()` unconditionally. Optimize's asynchronous apply loop survives component destruction. Its destroy hook only clears a toast timer.

**Browser reproduction:** at 384×812, supply a two-item plan through a stubbed Tauri bridge, start Apply, and hold the first disable response. Call browser `history.back()` (the event path used by the Android handler). The screen becomes Dashboard, `applyInProgress` remains true, and no Cancel button exists. Release the first response: the second `disable_package` is still invoked.

**Required behavior:** enforce navigation policy centrally while an operation owns the session, or move the operation into an application-level controller with persistent progress/cancel controls. Cover hardware back, connection-banner navigation, and component destruction.

### 2. P1 — Queued remote presses can be sent to a different TV

Sources: `src/screens/Remote.svelte:36`, `src/screens/Remote.svelte:90`, `src/screens/Remote.svelte:170`.

The queue captures a function that reads `session.serial` when it eventually executes, rather than capturing the target at the time of the press. Destroying the screen stops repeat timers but does not cancel queued work. The backend's serial validation cannot protect against this: the frontend supplies the new TV's valid serial.

**Browser reproduction:** hold the first Volume up invocation for A, click Volume down, navigate away, change the active session to B, then resolve the first invocation. Recorded calls:

```text
send_key { serial: "A:5555", key: "volume_up" }
send_key { serial: "B:5555", key: "volume_down" }
```

**Required behavior:** capture serial and connection generation at enqueue time; discard pending input on navigation, disconnect, or generation change. Bound ordinary queued presses as well as repeat ticks.

### 3. P1 — Device profiling can report a live device after losing its socket

Sources: `../crates/core/src/commands/devices.rs:51`, `../crates/core/src/commands/devices.rs:278`, `src/lib/session.svelte.ts:113`.

`list_devices_impl` reads the initial device list, then profiles each device. If that shell fails, `harvest_properties` returns default properties rather than an error. On mobile the failure may have just evicted the connection, but `list_devices_impl` still returns the original `status: device` entry. The frontend treats any returned device as live and can display a successful connection with a generic name and unknown type while the transport is disconnected. This also bypasses the API-level lost-connection signal because the command resolved successfully.

**Evidence:** source trace through the actual mobile eviction path, shared profile function, and frontend connect/recovery path. Not a physical-device reproduction.

**Required behavior:** propagate transport loss; distinguish unavailable optional properties from a disconnected device. Publish a coherent connection identity and profile instead of accepting a stale device-list entry after profiling fails.

### 4. P1 — Health failures bypass error/retry and lost-connection handling

Sources: `../crates/core/src/commands/health.rs:130`, `../crates/core/src/commands/health.rs:331`, `src/lib/session.svelte.ts:284`.

The health command converts both transport errors and its 30-second timeout into an empty successful report. `session.loadHealth` consequently overwrites the previous report, marks the cache loaded, and leaves `healthError` empty. The API connection-loss listener never sees a rejection. Dashboard's error/retry and Diagnostics' stale-data handling cannot work for these failures; subsequent non-forced loads accept the empty cache.

**Evidence:** the existing passing test `a_failed_batch_still_returns_a_default_report` explicitly asserts this behavior for `device offline`. This is a contract problem currently reinforced by a test.

**Required behavior:** preserve partial results when an optional metric is unavailable, but reject a failed whole transport operation. Preserve the last valid report with an explicit error/stale state and trigger session recovery.

### 5. P2 — Recovery is repeated indefinitely, despite the one-attempt policy

Sources: `src/lib/session.svelte.ts:216`, `src/lib/session.svelte.ts:237`, `src/App.svelte:40`.

After a recovery fails, `liveness` is `lost`, but the next heartbeat still probes and calls `recoverOrMarkLost`. There is no lost-state guard or recovery budget. `recovering` only deduplicates simultaneous attempts and is cleared after every attempt. Visibility changes, Dashboard visits, and subsequent command errors can also trigger another attempt.

**Executable reproduction:** compiled the existing session module with the installed TypeScript/Svelte compilers, mocked an offline status and failed connect, and called `checkLiveness()` twice. Result: two connection attempts, both ending in `lost`.

**Required behavior:** one automatic attempt per loss episode, followed by an explicit retry unless a different background-recovery policy is deliberately chosen. Reset the budget only on a new successful session or user retry.

### 6. P1 — Canceling connect does not cancel backend connection ownership

Sources: `src/lib/session.svelte.ts:164`, `src/lib/session.svelte.ts:200`, `src/screens/Onboarding.svelte:52`, `src-tauri/src/wireless_adb.rs:213`.

`cancelConnect` only increments a frontend counter. The native handshake can still complete and replace the active socket, while the frontend ignores its result. A subsequent failed connect invokes `restoreCurrentLiveness`, which adopts whichever device the backend returns without updating the stored host/port or clearing the old cache. This can separate the active device from the reconnect target. At launch it can produce a connected device with an empty stored host; with an existing session it can retain a different host.

There is a related same-host retry race: Onboarding identifies a pending attempt only by `connectingHost`. Cancel and immediately retry the same host, and the older completion matches the new attempt's host, allowing it to advance the screen or clear the new attempt's busy state.

**Executable reproduction:** delay a connect to B, cancel it, let the mocked backend finish B, then fail a connection to C. The session adopts B while retaining its previous host. The backend implementation confirms the canceled handshake still publishes its connection.

**Required behavior:** native and frontend connection generations must agree about which attempt owns the socket. Canceled work must not publish ownership. Use an attempt ID, not a hostname, for onboarding completion guards; restore identity and caches atomically.

### 7. P2 — The download size limit is applied after downloading the entire file

Sources: `src-tauri/src/file_commands.rs:294`, `src-tauri/src/file_commands.rs:299`, `src-tauri/src/wireless_adb.rs:386`.

`pull_file` checks the 2 GiB limit only after `raw_transfer` finishes. The transport streams without a byte limit into a normal file while holding the shared connection mutex. An oversized file can consume all available phone storage and block other ADB work for the duration of the transfer. A transfer failure returns before the oversize cleanup and leaves a partial file behind; retry then creates another suffixed file.

**Evidence:** source trace; no large transfer was performed.

**Required behavior:** preflight size when available, enforce the limit during streaming, use a temporary destination, remove partial output on every failure, and publish the final name only after success. Preserve a healthy connection when the destination rejects further writes.

### 8. P2 — Failed disabled-package reads are interpreted as “nothing disabled”

Sources: `../crates/core/src/adb/batch.rs:20`, `../crates/core/src/commands/apps.rs:89`, `../crates/core/src/commands/optimize.rs:74`, `../crates/core/src/commands/snapshot.rs:50`.

The batching format retains stdout sections but drops per-command status and stderr. Consumers reject an empty installed-package list, but accept an empty disabled-package section without checking whether that command succeeded. A failed `pm list packages -d` therefore makes Apps report disabled apps as enabled and Restore produce an empty plan. A truncated batch can have the same effect. Checking only the final shell exit code is insufficient for Optimize because its last command is `dumpsys meminfo`.

**Evidence:** source trace of the batch format and its consumers. Existing tests cover missing installed data, but do not establish the validity of an empty disabled list.

**Required behavior:** include per-section exit status/completion metadata and distinguish an empty successful result from a failed or missing section. Destructive planning should require complete relevant input.

### 9. P2 — Delayed reboot cleanup can disconnect a newer session

Sources: `src/screens/Dashboard.svelte:143`, `src/screens/More.svelte:153`, `src/App.svelte:34`.

Both reboot handlers schedule an unscoped `onDisconnect()` 1.5 seconds after success. They do not capture a session generation, cancel the timer on destruction, or verify that the active TV is still the reboot target. If the user changes screens and establishes another connection while the original reboot request or timer is pending, the old callback disconnects the new TV and resets navigation. It also persists the explicit-disconnect preference for that unrelated session.

**Evidence:** source trace; not timed against a physical TV.

**Required behavior:** bind reboot completion and cleanup to the initiating session generation. Never run a global disconnect from a stale screen callback.

## Validation performed

All results below are local macOS results, not a three-OS CI claim:

- `cargo fmt --all --check`: passed.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- Rust tests: shared core 179 passed; desktop 35 passed; mobile 21 passed; license CLI 2 passed. Doc tests passed.
- Vendored `adb_client`: 54 passed, 1 physical-device test ignored.
- Mobile `npm run check`: 0 errors, 0 warnings; `npm run build`: passed.
- Desktop `npm run check`: 0 errors, 0 warnings; `npm run build`: passed.
- Headless Chromium at 384×812, real Svelte components and a stubbed Tauri bridge: reproduced remote input crossing devices and an actual two-item Optimize apply continuing after browser Back.
- Executed compiled session/router logic with deferred mock API responses: reproduced repeated recovery, canceled-connect state divergence, and back-handler bypass.

The targeted checks above were run without committing a test harness or changing production behavior. Their results should become permanent regression cases when the defects are fixed.

## Coverage and limits

Reviewed mobile navigation/session/API flow, screen action handlers, wireless lifecycle/serialization, transfer storage, the shared profile/health/package/Optimize/snapshot paths, safety-gate call sites, and CI configuration. No new engine I/O or obvious missing never-disable gate was found in the reviewed mutation paths. That is a scoped review observation, not a proof of every possible path.

The current CI workflow runs host Rust checks and frontend checks/builds. It does not execute the frontend behavioral cases above, build/test Android-specific lifecycle and Kotlin code, or run the excluded vendored crate's unit tests. It also omits the license CLI from its explicit test/clippy package lists. These are concrete coverage gaps behind a green build.

No phone/TV interaction, Android APK build, Android-target clippy, physical pairing, mDNS validation, background/resume matrix, or Wi-Fi-loss test was performed in this audit. PowerShell/Pester validation could not run because `pwsh` is not installed. Desktop and v1 therefore should not be described as exhaustively verified by this pass.

The next stability pass should fix findings 1–6 together around explicit session ownership, add the reproduced scenarios as automated tests, then address transfer and batch-result correctness. After that, install the resulting APK and run the documented physical matrix; host mocks cannot establish real wireless or Android lifecycle reliability.

# Native Remote lifecycle acceptance

**Native fast Remote is enabled in desktop builds** (2026-11-06). Under the built-in engine the
Remote tab uses the scrcpy control channel over identity-bound streams (`NativeRemoteSession`,
`crates/core/src/adb/native_remote.rs`); platform-tools keeps its `adb forward` session
(`remote_input.rs`). Android is unchanged: it never compiles the native session and keeps its own
transport. The decision and the evidence behind it are below; physical TVs are still a separate
gate (see "Still open").

| Area | Implemented, simulated and run against a real adbd | Remaining acceptance |
| --- | --- | --- |
| Target ownership | One verified hardware binding across helper upload, resident shell and control services; all use dedicated cancellable connections. Emulator: two endpoints to the same device ran independent helpers, and closing one left the other working. | Real TV firmware, Wi-Fi loss and packaged-app interoperability. |
| Helper generation | Unique process/time/counter resource names, bound sync upload, resident shell cleanup trap, scoped cleanup requests and an owned partial-upload cleanup task that survives startup drop. No broad process kill or generic transfer fallback. Emulator: scrcpy unlinks its own jar at start, and every close, cancel and lost connection left no helper process within two seconds. | A TV that vanishes for good keeps its helper until its adbd notices the dead connection; that wait is the TV's TCP behavior, not ours. |
| Admission | Opaque decimal generation tokens; exact-token close, per-TV operation locks and internal revision fences. Close invalidates before waiting, and stale startup cannot publish. Cancelling startup never drops an identity probe midway, so the TV's own connection survives (it did not before; see "Found on the emulator"). | Already-dispatched shell/input cannot be unsent. |
| Sessions | Owned startup/cleanup workers, cancellation handles, independent TVs, repeated stop/reopen and driver-swap/quit draining. A session is renewed before its streams' five-minute lifetime and after its helper exits, before any input of the request. | Sleep/wake and reboot on hardware. |
| Held input | Track key-down before writing; normal close attempts key-up before cancellation. Failed/partial writes poison the session and report uncertain release, without replay. Emulator: a held Back left Settings only on release, and closing while held released it. | No control-message acknowledgement proves Android applied key-up. |
| Text | UTF-8 chunks preserve accepted text at scalar boundaries, maximum 300 bytes per control message and 64 KiB per request. Failure stops remaining chunks. Emulator: ASCII and Latin accents (`café über`) landed exactly. | CJK and emoji are dropped by Android's virtual keymap without an error (same server, same limit as platform-tools); IME behavior on TVs. |
| UI lifecycle | Lazy logical open; hide, unmount, blur, target/mode change discard unsent work. Late completions cannot change a new session. Keyboard D-pad and Unicode/IME input have browser fixtures. | Native webview and physical input testing across supported platforms. |

## Bounds and certainty

Startup has a 15-second total deadline; streams have a five-minute finite lifetime and are
renewed 30 seconds before it, individual I/O has a two-second deadline, helper upload is capped
at 16 MiB and pending service output is bounded. Resident output is discarded through a fixed
buffer, not accumulated. Idle streams remain usable within their total lifetime.

Only a correlated CLSE reply before the initial OKAY is a typed service refusal.
Control-socket readiness retries only that refusal, at most ten attempts separated
by a cancellable 150 ms delay. Lost OPEN acknowledgement, handshake failure,
helper launch/upload and input are never replayed. A canceled or failed worker
retains its lease until cleanup; cancellation does not prove remote termination.
Setup calls that check the TV's identity on its own connection run to completion even when
startup is cancelled (bounded at six seconds); whatever they open is then cancelled.

A channel startup failure before any input can use shell input through the same
selected driver. Explicit compatibility mode skips channel startup. Once input
is attempted, failure reports uncertainty and never resends through shell. Each
command captures a generation before startup and keeps it across any fallback.
Renewal happens before input, so it never turns into a resend.

## Emulator evidence — 2026-11-06

Android TV 14 emulator (arm64 AVD), its adbd reached natively over loopback with a throwaway,
already-allowed key. The harness is `src-tauri/tests/native_remote_emulator.rs`: `#[ignore]`,
env-gated, never run by CI. It drives the real `DirectNetworkAdb` and `AppState` Remote API
(`ensure_remote_session_for_generation`, key, held key, text, close) with the bundled scrcpy
server, and checks every effect through platform-tools `shell` observations (window focus,
`uiautomator`, `/proc`). Each press types a letter into a focused text field, so the field shows
whether a press landed exactly once. Final runs: 50 of 50 checks over the plain endpoint, 12 of 12
over TLS (paired key, port reached through a forward) and 3 of 3 for the five-minute lifetime.
Logs stay local in the task folder (`device-20261106/remote/`).

| Scenario | Result |
| --- | --- |
| Session start | 327 ms (jar upload, helper launch, control socket); 12 reopens 314–480 ms, p50 317 ms. The first start after the emulator booted took 1,051 ms |
| Warm press (down+up) | p50 2.8 ms, p95 9.0 ms; as one `send_key` command (ensure + send) p50 2.7 ms, p95 7.1 ms; 30 of 30 landed once, in order |
| Shell fallback, same emulator | `input keyevent` p50 105 ms, p95 134 ms (other runs: p50 82–131 ms). Real TVs measured about 690 ms |
| D-pad, Home, Back | Focus moved down and back; Home reached the launcher in 59 ms; Back left Settings |
| Held key | Back held 1.5 s kept Settings open and its release left it; closing while Back was held sent the release |
| Text | `atv remote 42!` and `café über` landed exactly; `漢字` and `😀` were dropped by the keymap while reported sent |
| Stop/reopen | 12 rounds without errors; close 55–82 ms; no helper process or jar after any close |
| Two sessions in a row | The old generation was refused; one helper at a time; nothing left after |
| With other native work | 20 presses during concurrent shell, file list and pull all succeeded, press p95 10.3 ms |
| Cancel during startup | Close at 0–700 ms and request drop at 30–300 ms returned within 0.1 ms (97 ms once the session was up), left no helper and kept the TV's connection |
| Two endpoints, same TV | Independent helpers; closing one left the other working |
| Helper killed on the TV | The next press renewed the session before input and landed once (380 ms) |
| TV vanished mid-session | The press reported "outcome unknown" after 4.0 s (2 s write deadline plus bounded cleanup) and was never resent; the key did not land; a new Remote failed at once instead of hanging; the helper exited once the TV side closed |
| TV came back | The one uncertain key landed late, once |
| Stream lifetime | Before the fix, a press after 310 s idle failed as "outcome unknown"; after it, the session renewed (373 ms) and the key landed once |
| Wireless debugging (TLS) | Start 364 ms, 10 reopens p50 295 ms; warm press p50 4.6 ms, p95 10.2 ms; same text results; no helper left. Shell fallback p50 111 ms |

### Found on the emulator and fixed

- **Cancelling Remote startup dropped the TV's connection.** Startup raced the identity probe
  (`device_binding`, and the probe inside every `open_bound_device_service`) against
  cancellation. Dropping a probe midway closes the TV's own connection, so hiding the Remote tab
  during startup left the TV disconnected for every other command. Probes now run to completion
  in a task; startup still returns at once, and anything opened late is cancelled.
- **A Remote left open for five minutes failed its next press.** The streams' five-minute
  lifetime closed the control socket, and the next press reported "outcome unknown" plus a held
  key, though nothing was sent. Sessions now renew before the lifetime and whenever the helper
  has exited, before the request's input.

## Still open

Packaged macOS/Windows/Linux builds on physical TVs: latency over Wi-Fi, sleep/wake, TV reboot,
Wi-Fi loss, older adbd versions (the Shield's Android 11) and real IMEs. Neither this work nor the
native transport establishes the cause of issue #235's routing failure.

Protocol references: [scrcpy 3.1 control serialization](https://github.com/Genymobile/scrcpy/blob/v3.1/app/src/control_msg.c)
and [AOSP ADB OPEN/CLOSE protocol](https://github.com/aosp-mirror/platform_system_core/blob/android-10.0.0_r1/adb/protocol.txt).
Implementation and tests are original; these sources define protocol behavior,
not evidence of hardware compatibility.

## Consolidation follow-up

The cleanup owner arms before SEND and retains one admitted lease and exact
binding/path through bounded cleanup. It waits for the local upload producer to
release its stream before unlink; expiry never schedules a later unlink. Failure,
timeout, task drop and closed admission are recorded as unconfirmed. Local worker
completion does not prove remote processing or physical deletion. Desktop compatibility cancellation shuts down only its
owned forwarded socket. Neither change resets a host server or replays input.

Remote pending work is capped across old and current frontend generations at
128 actions and 64 KiB, including buffered text. Paste preflight rejects an
oversized request before enqueuing any prefix; overflow discards unsent work
with a visible message. These limits do not cancel already delivered input.

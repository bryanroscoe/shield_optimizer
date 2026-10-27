# Native Remote lifecycle acceptance

This experimental stack follows the reviewed stream prerequisite at `d53cdf1`.
Platform-tools remains the shipped default. **Native fast Remote is still disabled
in production**; fake drivers exercise the compiled orchestration directly.

| Area | Implemented and locally simulated | Remaining acceptance |
| --- | --- | --- |
| Target ownership | One verified hardware binding across helper upload, resident shell and control services; all use dedicated cancellable connections. | Real AUTH/STLS, firmware and packaged interoperability. |
| Helper generation | Unique process/time/counter resource names, bound sync upload, resident shell cleanup trap, scoped cleanup requests and an owned partial-upload cleanup task that survives startup drop. No broad process kill or generic transfer fallback. | Cleanup waits for local producer completion before unlink and records expiry or task-drop uncertainty. Remote processing, closed admission and runtime shutdown can still leave this generation's file; physical deletion and process termination are not confirmed. |
| Admission | Opaque decimal generation tokens; exact-token close, per-TV operation locks and internal revision fences. Close invalidates before waiting, and stale startup cannot publish. Legacy commands capture one generation per request. | Actual OS/process failure behavior. Desktop forwarded-socket cancellation has blocked-read/write and isolation fixtures; Android cancellation remains unchanged. Already-dispatched shell/input cannot be unsent. |
| Sessions | Owned startup/cleanup workers, cancellation handles, independent TVs, repeated stop/reopen and driver-swap/quit draining. | Hardware socket-shutdown and helper-exit behavior. |
| Held input | Track key-down before writing; normal close attempts key-up before cancellation. Failed/partial writes poison the session and report uncertain release, without replay. | No control-message acknowledgement proves Android applied key-up. Real disconnect/held-key acceptance remains required. |
| Text | UTF-8 chunks preserve accepted text at scalar boundaries, maximum 300 bytes per control message and 64 KiB per request. Failure stops remaining chunks. | Android text-injection/keymap limitations, grapheme/IME behavior on TVs. |
| UI lifecycle | Lazy logical open; hide, unmount, blur, target/mode change discard unsent work. Late completions cannot change a new session. Keyboard D-pad and Unicode/IME input have browser fixtures. | Native webview and physical input testing across supported platforms. |

## Bounds and certainty

Startup has a 15-second total deadline; streams have a five-minute finite lifetime,
individual I/O has a two-second deadline, helper upload is capped at 16 MiB and
pending service output is bounded. Resident output is discarded through a fixed
buffer, not accumulated. Idle streams remain usable within their total lifetime.

Only a correlated CLSE reply before the initial OKAY is a typed service refusal.
Control-socket readiness retries only that refusal, at most ten attempts separated
by a cancellable 150 ms delay. Lost OPEN acknowledgement, handshake failure,
helper launch/upload and input are never replayed. A canceled or failed worker
retains its lease until cleanup; cancellation does not prove remote termination.

A channel startup failure before any input can use shell input through the same
selected driver. Explicit compatibility mode skips channel startup. Once input
is attempted, failure reports uncertainty and never resends through shell. Each
command captures a generation before startup and keeps it across any fallback.

## Evidence and release gates

Deterministic registry, fake-stream, command and browser cases cover repeated
open/close, stale generations, delayed startup, cancellation, held input, concurrent
TVs, queued operations and uncertain delivery. The protocol fixtures use only
self-owned loopback peers and existing in-memory identities. These tests do not
establish actual TV behavior or eliminate the cleanup limits above.

Targeted screenshots and a short video are labeled simulation and retained outside
Git. Normal captured gallery layouts are unchanged. No actual helper push, remote
control, LAN scan, host ADB/server action, credential generation, permissions change
or publication is required for these local checks.

Before enabling native fast Remote, review the recorded simulated results and
remaining failure cases. Packaged macOS/Windows/Linux plus physical TV acceptance,
normal identity onboarding, helper compatibility and meaningful cancellation/
release verification remain separate gates. Neither this work nor the native
transport establishes the cause of issue #235's routing failure.

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

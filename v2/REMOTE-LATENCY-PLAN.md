# Remote-control latency plan

Status: **planned, not started.** The Remote tab works but each key press takes
~700 ms, which makes the D-pad feel broken. This doc captures the investigation
and the agreed design so implementation can start cold.

## Why a key press takes ~700 ms today

Every press runs `adb -s X shell "input keyevent <code>"`
(`src-tauri/src/commands/input.rs`), paying four costs in series:

1. Spawn a fresh `adb` client + connect to the local adb server (~30–80 ms)
2. Open a new shell service on the device over the network (~50–150 ms)
3. **`input` boots a fresh Java VM on the TV** (`app_process`), binds to
   InputManager, injects one event, exits — ~300–600 ms on Shield firmware.
   This dominates.
4. Teardown + output round-trip

A persistent `adb shell` session only removes costs 1–2; the Java VM cold-start
happens per `input` invocation no matter how the command arrives. Earlier
probes: `sendevent` is SELinux-blocked on Shield, and `input` measured ~700 ms
per keyevent.

## Phase 0 — benchmark on real devices (15 min, needs a Shield)

Confirms the cost split, and checks for a fast path on newer firmware:

```bash
adb shell "time input keyevent 20"        # baseline (cost 3 in isolation)
adb shell "cmd input keyevent 20" 2>&1    # native binder path; exists on newer
                                          # Android and is ~10x faster if so
adb shell "time input keyevent 20 20 20"  # multi-key batching support?
```

Decision rule: if `cmd input` exists and lands under ~100 ms on the whole
fleet, a persistent shell + `cmd input` is competitive and much simpler — stop
here and build that instead. Otherwise (expected on 2019-era firmware),
proceed to the scrcpy design.

## The fix — scrcpy-server control channel

scrcpy (Apache-2.0) pushes a ~90 KB Java server to the device, runs it once via
`app_process` with shell privileges, and keeps it resident, injecting events
through InputManager directly. We use its control-only mode (no video/audio):

```text
once per device session:
  adb push scrcpy-server.jar /data/local/tmp/
  adb forward tcp:<port> localabstract:scrcpy_<scid>
  adb shell app_process ... com.genymobile.scrcpy.Server <ver> \
      scid=<scid> video=false audio=false control=true
  TcpStream::connect(localhost:<port>)        # stays open

per key press:
  socket.write(14-byte INJECT_KEYCODE message)  # ~5–30 ms total
```

Free wins that fall out of this design:

- **Real key-down/key-up** → press-and-hold D-pad scrolling, which
  `input keyevent` fundamentally cannot do.
- **UTF-8 text injection** → removes the printable-ASCII-only limit in
  `encode_input_text` (`input text` silently mangles non-ASCII).

## Implementation phases

**Phase 1 — session plumbing (Rust).** New `adb/remote_input.rs`:
`RemoteInputSession` per serial (push, forward, spawn server, connect socket),
stored in `AppState` behind `Mutex<HashMap<serial, Session>>`. Lazy-start on
first Remote-tab key; auto-reconnect on socket death; teardown on disconnect
and app exit (`kill_on_drop` on the wrapping shell + scid-based kill). All adb
invocations go through the existing `SubprocessAdb` — the raw TCP socket is a
documented exception to the one-wrapper rule, like `scan.rs`'s route/ip calls.

**Phase 2 — protocol client.** Implement exactly two scrcpy control messages
(fixed binary structs): `INJECT_KEYCODE` (action down/up, keycode, repeat,
meta) and `INJECT_TEXT`. Pin one exact scrcpy-server version + SHA-256.
Bundle the jar as a Tauri resource (Apache-2.0 attribution in the README).
Mind the handshake: the server sends a device-meta preamble before the control
socket is usable.

**Phase 3 — wire-up + fallback.** `send_key` / `send_text` try the session
first and fall back transparently to today's `input` path if the session can't
start (odd firmware, denied `app_process`) — nothing regresses. Frontend
(`RemoteTab.svelte`): pointerdown/pointerup for hold-to-repeat, drop the
250 ms typing batch, small "live / fallback" status indicator.

**Testing.** Session lifecycle against MockAdb (push/forward/spawn command
assertions, using the error-injection support added in PR #66); protocol
encoding as byte-exact unit-test fixtures; latency verified manually on
device.

## Estimates and open questions

- Phase 0: ~15 min with a device. Phases 1–2: ~2–3 days. Phase 3: ~half day.
- Riskiest bit: scrcpy handshake details — known and documented, just fiddly.
- Open: bundle vs download the server jar (leaning bundle — simpler, offline-
  friendly, ~90 KB); whether press-and-hold repeat is wanted in the UI
  (leaning yes — it's free and it's what makes a software remote feel real).

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

### Phase 0 results (2026-06-10, two Shield Android-TV units, Android 11)

Measured on `192.168.42.196` and `192.168.42.71` (both NVIDIA SHIELD Android TV,
Android 11) — identical results:

- **`input keyevent` ≈ 690 ms** per press (0.69s real, of which only ~0.18s is
  user CPU — the rest is JVM cold-start). First call ~6.1s (one-time warm-up).
  Confirms the original estimate and that the Java VM boot dominates.
- **`cmd input` → "No shell command implementation."** The fast binder path
  does **not** exist on Shield Android 11. The simple fallback is off the
  table; scrcpy is the answer.
- **Multi-key batching** (`input keyevent 20 20 20`) is still one ~0.69s call —
  no help (one JVM boot covers all three, but you can't stream live presses
  through it).

**scrcpy control channel — verified working on this hardware:**
- scrcpy-server **v3.1** (90 640 bytes, SHA-256
  `958f0944a62f23b1f33a16e9eb14844c1a04b882ca175a738c16d23cb22b86c0`) pushed to
  `/data/local/tmp`, launched via `app_process` in control-only mode
  (`video=false audio=false control=true tunnel_forward=true
  send_device_meta=false send_dummy_byte=true`).
- Server identified the device ("Device: [NVIDIA] NVIDIA SHIELD Android TV
  (Android 11)") and accepted a TCP control connection over `adb forward`.
- Our own client received the 1-byte handshake (`\x00`) and wrote
  `INJECT_KEYCODE` messages (14 bytes each: `>BBIII` = type, action, keycode,
  repeat, metaState) at **~0.01 ms/write**.
- **End-to-end injection proven with state read-back**: SLEEP (223) through
  the channel flipped the device Dreaming → Asleep — observed via an
  independent `dumpsys power` poll in **152 ms including the polling
  overhead** — and WAKEUP (224) restored it. The encoding is correct and the
  per-press cost collapses from 690 ms to network RTT.

**Verdict: build the scrcpy control channel (Phases 1–3 below).** Lifecycle
notes for the Rust impl, all verified on hardware:

- A control-only server **exits on its own the moment the control socket
  closes** — hold the socket for the tab's lifetime; on teardown drop the
  socket first, then best-effort `pkill -f com.genymobile.scrcpy` (present at
  `/system/bin/pkill` on this firmware, supports `-f`), then remove the
  forward.
- **Spawn the server child with stdin explicitly `/dev/null`** (open, EOF) and
  stdout/stderr piped or nulled. The device-side `app_process` aborts at
  startup (exit 134, zero Java output) when the adb client's stdin is fully
  closed; a GUI app's inherited stdin is unreliable, so never inherit it.

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

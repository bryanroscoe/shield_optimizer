# Remote-control input channel

The Remote tab injects input through a resident scrcpy-server control channel instead of one
`adb shell input` call per press. `RemoteInputSession` lives in
`crates/core/src/adb/remote_input.rs`, the pinned scrcpy v3.1 server is bundled at
`v2/src-tauri/resources/scrcpy-server-v3.1`, and `crates/core/src/commands/input.rs` drives the
channel with the per-press `input` path as a transparent fallback. The mobile app uses the same
channel over its own transport; see [`mobile/FAST-REMOTE-PLAN.md`](mobile/FAST-REMOTE-PLAN.md).

This doc keeps the hardware measurements and lifecycle behavior that justify the design. They
were measured on real devices and are recorded nowhere else.

## Why `input keyevent` is slow

Every fallback press runs `adb -s X shell "input keyevent <code>"`, paying four costs in series:

1. Spawn a fresh `adb` client and connect to the local adb server (~30–80 ms)
2. Open a new shell service on the device over the network (~50–150 ms)
3. **`input` boots a fresh Java VM on the TV** (`app_process`), binds to InputManager, injects
   one event, exits — ~300–600 ms on Shield firmware. This dominates.
4. Teardown and output round-trip

A persistent `adb shell` session only removes costs 1–2; the Java VM cold start happens per
`input` invocation no matter how the command arrives. `sendevent` is SELinux-blocked on Shield.

## Measurements (two NVIDIA SHIELD Android TV units, Android 11)

Both units gave identical results:

- **`input keyevent` ≈ 690 ms** per press (0.69 s real, of which only ~0.18 s is user CPU — the
  rest is JVM cold start). The first call takes ~6.1 s (one-time warm-up).
- **`cmd input` → "No shell command implementation."** The fast binder path does not exist on
  Shield Android 11, so a persistent shell plus `cmd input` is not an option there.
- **Multi-key batching** (`input keyevent 20 20 20`) is still one ~0.69 s call and can't stream
  live presses.

The probes, for re-checking other firmware:

```bash
adb shell "time input keyevent 20"        # baseline (cost 3 in isolation)
adb shell "cmd input keyevent 20" 2>&1    # native binder path, where it exists
adb shell "time input keyevent 20 20 20"  # multi-key batching
```

**scrcpy control channel on the same hardware:**

- scrcpy-server **v3.1** (90 640 bytes, SHA-256
  `958f0944a62f23b1f33a16e9eb14844c1a04b882ca175a738c16d23cb22b86c0`) pushed to
  `/data/local/tmp`, launched via `app_process` in control-only mode (`video=false audio=false
  control=true tunnel_forward=true send_device_meta=false send_dummy_byte=true`).
- The server accepts a TCP control connection over `adb forward`; the client receives the 1-byte
  handshake (`\x00`) and writes `INJECT_KEYCODE` messages (14 bytes each: `>BBIII` = type,
  action, keycode, repeat, metaState) at ~0.01 ms per write.
- End-to-end injection was confirmed by state read-back: SLEEP (223) through the channel flipped
  the device Dreaming → Asleep, observed by an independent `dumpsys power` poll in 152 ms
  including polling overhead, and WAKEUP (224) restored it. Per-press cost is network RTT.

## How the channel works

scrcpy (Apache-2.0) pushes a ~90 KB Java server to the device, runs it once via `app_process`
with shell privileges, and keeps it resident, injecting events through InputManager directly.
The app uses its control-only mode (no video/audio):

```text
once per device session:
  adb push scrcpy-server.jar /data/local/tmp/
  adb forward tcp:<port> localabstract:scrcpy_<scid>
  adb shell app_process ... com.genymobile.scrcpy.Server <ver> \
      scid=<scid> video=false audio=false control=true
  TcpStream::connect(localhost:<port>)        # stays open

per key press:
  socket.write(14-byte INJECT_KEYCODE message)
```

- Sessions are per serial, start lazily on the first Remote-tab key, reconnect on socket death,
  and tear down on disconnect and app exit. All adb invocations go through `SubprocessAdb`; the
  raw TCP socket to the forwarded port is a documented exception to the one-wrapper rule, like
  `scan.rs`'s route/ip calls.
- Only two control messages are implemented: `INJECT_KEYCODE` (down/up, keycode, repeat, meta)
  and `INJECT_TEXT`. The server version and SHA-256 are pinned.
- Real key-down/key-up gives press-and-hold D-pad repeat, which `input keyevent` cannot do, and
  `INJECT_TEXT` carries UTF-8 that `input text` would mangle.
- `send_key` / `send_text` fall back to the per-press `input` path when the session can't start
  (odd firmware, denied `app_process`), so nothing regresses.
- Tests cover session lifecycle against MockAdb (push/forward/spawn assertions with error
  injection) and byte-exact protocol encoding; latency is verified by hand on a device.

## Lifecycle rules (verified on hardware)

- A control-only server **exits on its own the moment the control socket closes**. Hold the
  socket for the session's lifetime; on teardown drop the socket first, then best-effort
  `pkill -f com.genymobile.scrcpy` (present at `/system/bin/pkill` on Shield firmware, supports
  `-f`), then remove the forward.
- **Spawn the server child with stdin explicitly `/dev/null`** (open, EOF) and stdout/stderr
  piped or nulled. The device-side `app_process` aborts at startup (exit 134, no Java output)
  when the adb client's stdin is fully closed; a GUI app's inherited stdin is unreliable, so
  never inherit it.

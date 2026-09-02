# Mobile fast-remote plan

Status (reviewed 2026-09-01): **Phases 1–3 complete; Phase 4 device gates in progress.** Mobile now uses a persistent
scrcpy control-only channel with transparent `adb shell input` fallback. The phone cannot reuse the
desktop's localhost `adb forward` path because it talks directly to `adbd`, so the server and
control socket each own a dedicated authenticated ADB connection while normal commands continue on
`WirelessAdb`'s existing connection.

## Spike result

The direct ADB protocol can open the socket that scrcpy exposes. The client sends an ADB `OPEN`
whose destination is `localabstract:scrcpy_<scid>`, then exchanges `WRTE`, `OKAY`, and `CLSE`
messages on that logical stream. An adb-server forward ultimately performs the same device-side
open, so no localhost forward is needed on mobile.

`adb_client` 3.2.2 already contains the protocol machinery, but its public API stops one layer too
high:

- `ADBMessageDevice::open_session` constructs the required `OPEN`, but it is `pub(crate)`.
- `ADBSession`, `ADBLocalCommand`, the message transport, and `ADBTcpDevice::inner` are private.
- `ADBLocalCommand` has no raw-service or `localabstract` variant.
- `ADBTcpDevice` exposes shell, exec, push, pull, and framebuffer operations, but no persistent raw
  byte stream.
- The crate's `Forward` and `Reverse` commands only work through `ADBServerDevice`; they do not
  apply to a direct `ADBTcpDevice` connection.

The current upstream `main` branch has the same API boundary as 3.2.2. This is not a dead end: the
missing surface is small, and the crate is MIT-licensed. It does mean the fast path must not be
implemented by simply removing the Android `cfg` gates from the desktop code.

## Chosen design

Add a small, upstreamable raw-service stream to `adb_client`, initially carried as a pinned,
documented patch so Android builds are reproducible. Keep the patch generic rather than adding
scrcpy-specific behavior to the dependency.

The API should create a **dedicated authenticated TCP connection** for one service and return an
owned stream. It must not clone the transport used by `WirelessAdb`: cloned `TcpTransport` values
share one socket, and `adb_client` has no dispatcher that routes incoming messages by logical
stream ID. Two concurrent readers on that socket can consume each other's frames.

The stream implementation must:

1. connect and authenticate with the same persisted RSA key as `WirelessAdb`;
2. send `OPEN(local_id, 0, "localabstract:scrcpy_<scid>")` and validate `OKAY` plus both IDs;
3. expose blocking `Read`/`Write` (or equivalent explicit methods);
4. acknowledge every incoming `WRTE` with `OKAY` and buffer payload bytes across partial reads;
5. split writes at a protocol-safe payload cap, waiting for `OKAY` after each `WRTE`;
6. treat `CLSE` as EOF, close correctly, and make `Drop` best-effort rather than
   panic-prone;
7. enforce timeouts so a dead TV cannot pin a blocking worker forever.

The mobile host will wrap the blocking stream with `spawn_blocking`; the core engine remains pure.
All ADB behavior stays behind the existing `WirelessAdb`/`AdbDriver` seam rather than creating a
second mobile ADB wrapper.

Do not hold `WirelessAdb.conn` for the life of the control channel. The existing connection must
remain available for health checks, shell commands, files, and disconnect. The control stream owns
its separate connection and is registered per serial in the same lazy session pattern used by the
desktop app.

## Startup and lifecycle

Reuse the pinned desktop scrcpy server v3.1 and the existing protocol encoders in
`crates/core/src/adb/remote_input.rs`:

- resource: `src-tauri/resources/scrcpy-server-v3.1`
- SHA-256: `958f0944a62f23b1f33a16e9eb14844c1a04b882ca175a738c16d23cb22b86c0`
- device path: `/data/local/tmp/shieldopt-scrcpy-server.jar`
- control messages: 14-byte big-endian key down/up and length-prefixed UTF-8 text

Avoid maintaining a second jar copy. Compile the existing resource into the mobile host with
`include_bytes!`, verify its hash in a test, and materialize it in app-private storage only when a
local path is needed for `adb_client::push`.

Cold start sequence:

1. materialize and push the jar through `WirelessAdb::raw_transfer`;
2. generate the 31-bit, eight-hex-digit `scid`;
3. launch the control-only server with `video=false`, `audio=false`, `control=true`,
   `tunnel_forward=true`, `send_device_meta=false`, and `send_dummy_byte=true`;
4. repeatedly open the dedicated `localabstract:scrcpy_<scid>` service until it exists;
5. read and validate the `0x00` dummy byte;
6. retain the service stream until disconnect, connection loss, or an explicit session reset.

The detached short-shell launch was rejected during the first device gate: the older Shield exits
the background child with its shell. The implemented path opens a generic long-lived `shell:`
service through `ADBTcpService`, retains that connection for the session, and opens the abstract
control socket on another dedicated connection. This keeps the normal command connection free and
gives teardown an owned server handle without using `ADBDeviceExt::exec`'s detached reader thread.

Teardown order:

1. close the control stream so scrcpy exits naturally;
2. best-effort `pkill -f shieldopt-scrcpy-server`;
3. close any long-lived server-command connection, if that fallback was required;
4. remove the session from the registry before doing slow cleanup.

Android backgrounding has a 30-second grace period. Returning before it expires keeps the owned
channel warm; expiry closes all remote sessions. Suspend/resume epochs cancel stale timers, and a
separate transition guard serializes startup with slow teardown so a resumed cold start cannot be
killed by an older cleanup task.

## Integration phases

### Phase 1 — dependency API and protocol tests

**Complete (2026-07-13).** The workspace now pins `vendor/adb_client` through
`[patch.crates-io]`. `adb_client::tcp::ADBTcpService` owns a dedicated authenticated connection and
implements the raw stream with a conservative 4 KiB ADB payload cap. Its in-memory transport tests
cover exact NUL-terminated `OPEN`, `WRTE`/`OKAY`, partial reads, interleaved traffic, multi-frame
writes, physical EOF, malformed and zero IDs, timeouts, local close, and peer `CLSE`. The patch
passes its native tests and clippy with warnings denied, and cross-compiles for
`aarch64-linux-android` with the mobile `framebuffer` feature.

### Phase 2 — mobile session transport

**Complete (2026-07-13).**

- Extend the existing driver seam with only the generic byte-stream capability the Android session
  needs; desktop may retain its current forward/socket implementation.
- Make `RemoteInputSession` and the `AppState` session registry available on Android with a
  platform-specific startup transport, while keeping the shared key/text encoders unchanged.
- Generalize server-resource resolution for Android using the embedded jar materialization above.
- Drop the session during `wireless_disconnect`, device switching, and liveness failure.

### Phase 3 — shared commands and fallback

**Complete (2026-07-13).**

- Remove the hard-coded `scrcpy channel unavailable on Android` branch in shared `send_key` and
  `send_text` once the Android session implementation exists.
- Preserve `forceShell`, transparent same-request fallback, session reset after a stream error, the
  key allowlist, and the existing `SendTextResult.transport` contract.
- Keep the frontend unchanged unless device testing exposes a status or lifecycle gap. It already
  enables hold-to-repeat only after the backend reports `transport: "channel"`.

### Phase 4 — device and regression gates

**In progress.** Validated from the Pixel 10 Pro on the Bedroom Shield (`192.168.42.196`):

- the first press cold-started the channel and landed (1,097 ms including push/server startup);
- 30 consecutive warm D-pad presses all reported `transport: "channel"`, with 75.2 ms p95
  (59.3–76.4 ms observed);
- UTF-8 text (`héllo ✓`) reported `transport: "channel"` successfully;
- explicit disconnect and force-stop both removed the resident app-process and
  `@scrcpy_<scid>` socket, and reconnect succeeded;
- the exact embedded jar was materialized on the Pixel and verified against the pinned SHA-256;
- the Android build and Android-target clippy pass with warnings denied.

The Living Room Shield (`192.168.42.71`) also delivered every warm press over the channel. Three
30-press samples measured 108, 98, and 87 ms p95 respectively as the channel/device warmed. UTF-8
channel text and ASCII compatible-shell text succeeded, Unicode compatible-shell text reported its
limitation honestly, and diagnostics plus file listing/pull remained usable while the channel was
live. The pulled sentinel matched by SHA-256, force-stop left no resident scrcpy process/socket, and
the 30-second background expiry plus resume cancellation were observed in lifecycle logs.

Still required: exercise hold-to-repeat, TV reboot, sleep/wake, and full background-reconnect flows
with a **live fast-remote session** on both Shields. The observed lifecycle logs proved timer expiry
and stale-timer cancellation, but do not by themselves prove warm-session reuse before 30 seconds or
cleanup after 30 seconds. Repeat concurrent diagnostics/file work on Bedroom; Living Room already
passed that portion. Take one fresh controlled Living Room warm-latency sample because the first of
its three runs exceeded the strict 100 ms stop condition. The Bedroom Shield was unauthorized during
the latest host-side attempt, so its remaining gates were not repeated.

### Next physical session order

1. Build and install current branch HEAD on the Pixel over wireless ADB; the last installed APK
   predates the cached-label fix in `3eccc34`. Spot-check the previous-TV menu, resulting header
   label, used-RAM bar, and missed-prompt error before returning to remote testing.
2. Authorize Bedroom if necessary, open one live channel, and run its outstanding concurrent
   diagnostics and file list/pull check.
3. On each Shield, test hold, sleep/wake, reboot cleanup, resume under 30 seconds, and expiry over
   30 seconds while the channel is actually live. Inspect process/socket cleanup after each terminal
   path.
4. Run one deliberate 30-press Living Room latency sample and record p95. Coordinate any repeated
   TV input with Bryan; do not generate blind button traffic during normal viewing.

On each known Shield (`192.168.42.196` and `192.168.42.71` when available):

- first press cold-starts and lands, then warm D-pad presses feel immediate;
- measured warm press latency is below 100 ms at p95 (target: network RTT);
- press-and-hold repeats without queuing slow shell calls;
- UTF-8 text works; compatible mode remains ASCII-only and honest about errors;
- sleep/wake, disconnect/reconnect, TV reboot, and app force-stop leave no orphaned
  `shieldopt-scrcpy-server` process; phone resume before 30 seconds reuses the owned session, while
  a longer background interval cleans it up before the next cold start;
- an unsupported or failed scrcpy launch falls back to shell on the same press;
- normal diagnostics and file operations still work while the control channel is open.

Then deploy to the Pixel over wireless ADB only and run the repository gates from `HANDOFF.md`.

## Stop conditions

Keep the shell fallback and do not ship the fast path if any of these remains true:

- a raw stream can steal frames from the normal command connection;
- cleanup routinely leaves the server resident;
- a failed cold start loses the user's original key press;
- warm p95 latency is 100 ms or worse;
- the patched dependency cannot cross-compile cleanly or its license/attribution cannot be included
  in the release notices.

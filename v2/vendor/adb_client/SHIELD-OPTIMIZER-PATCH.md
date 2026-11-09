# Shield Optimizer patch

This directory starts from the exact `adb_client` 3.2.2 crates.io source, whose upstream commit is
recorded in `.cargo_vcs_info.json`. It remains MIT-licensed; `LICENSE` is copied from the upstream
repository because crates.io did not include it in the published archive.

Most of the patch is protocol work any ADB client could use: an owned raw-service stream
(`adb_client::tcp::ADBTcpService`), finite timeouts, Android 11+ pairing, the Kadb-derived
connection, stream, sync, shell and install hardening, bounded and cancelable operations, and
a required-identity mode. Some of it is product-specific and says so where it lives:

- The hardware-identity guard. `run_identity_bound_shell`, `open_bound_service`,
  `run_streamed_install` and `ADBTcpDevice::verify_hardware_binding_bounded` refuse to start
  unless a fresh connection reports, through `getprop ro.serialno`, the hardware id the
  caller captured on its selected TV. That is ATV Optimizer's rule that a saved TV is matched
  on verified hardware id and never on its address alone.
- `examples/device_smoke.rs` writes device files only under `/data/local/tmp/atvopt-devtest-*`
  and uninstalls only packages starting with `com.atvoptimizer.devtest.`.
- The install outcome rules in `tcp/streamed_install.rs` (exact `Success` only, an early or
  lost result is indeterminate, never retried) are the app's safety policy, stricter than what
  `ADBTcpDevice::install_apks` reports.

Nothing here contains scrcpy-specific behavior.

Main entry points beyond upstream, all in `adb_client::tcp` unless noted: `ADBTcpService`,
`pair`, `BoundedShellControl`, `run_bounded_shell`, `run_identity_bound_shell`,
`open_bound_service`, `run_streamed_install`, the `ADBTcpDevice` constructors and methods listed
in the sections below, `adb_client::ConnectionInfo`, `adb_client::ShellExit` and the new
`RustADBError` variants. The section for each change lists the files it touched.

The workspace pins this directory through `[patch.crates-io]` in `v2/Cargo.toml`. Before replacing
it with a newer upstream release, verify that the release exposes an equivalent owned raw-service
stream, then rerun the tests and Android cross-compile documented in `mobile/FAST-REMOTE-PLAN.md`.

## Finite default read timeout (2026-09-04)

Upstream's `DEFAULT_READ_TIMEOUT` is effectively infinite, so `read_message()` and every caller
that passed `Duration::from_secs(u64::MAX)` (default `shell_command`, `open_session`, the sync
service's `recv_file`/push acknowledgements, `end_transaction`) would block forever after a silent
peer loss. On a phone that roams between access points this pinned the single connection mutex and
made the app look connected while nothing responded. The default is now a 120-second inactivity
bound, the crate-internal `adb_message_transport::DEFAULT_READ_TIMEOUT`; callers that need something
tighter still pass an explicit timeout. Changed files: `adb_message_transport.rs`,
`adb_message_device.rs`, `commands/shell.rs`, `tcp/adb_tcp_device.rs`.

## Android 11+ wireless-debugging pairing (2026-09-04)

Upstream `adb_client` has no code-pairing support: `ADBServer::pair` only forwards a `host:pair`
request to an external adb server, which does not exist on a phone. This patch adds a self-contained
client implementation of the Android 11+ pairing protocol, so `mobile` can pair a Google TV device
without an adb binary.

New public API on `adb_client::tcp`:

```rust
pub fn pair(addr: SocketAddr, code: &str, private_key_path: &Path)
    -> Result<PairingOutcome, PairingError>;
```

It connects to the device's `_adb-tls-pairing` port, completes a TLS 1.3 handshake presenting a
self-signed client certificate derived from `private_key_path` (the same derivation
`tcp_transport.rs` uses, so the certificate carries the same RSA public key a later
`ADBTcpDevice` connection presents), binds the TLS exporter output into a SPAKE2 password
alongside the six digits, and exchanges encrypted `PeerInfo` records. On success the device has
stored our ADB RSA public key and a follow-up `ADBTcpDevice::new_with_custom_private_key` connect
to `_adb-tls-connect` succeeds without an "Allow debugging" prompt.

New source files (nothing existing was modified except the module exports and the manifest):

- `src/message_devices/tcp/pairing/mod.rs` — TLS setup, deadline-bounded framing, the protocol
  driver, `pair`, `PairingError`, `PairingOutcome`
- `src/message_devices/tcp/pairing/packet.rs` — `PairingPacketHeader` and `PeerInfo` wire layout
- `src/message_devices/tcp/pairing/spake2.rs` — BoringSSL-flavoured SPAKE2 over edwards25519
- `src/message_devices/tcp/pairing/cipher.rs` — HKDF-SHA256 plus AES-128-GCM with the sequence
  counter AOSP uses as a nonce
- `src/message_devices/tcp/mod.rs` — `pub use pairing::{pair, PairingError, PairingOutcome, PeerInfo}`

New dependencies (all permissive, no GPL): `curve25519-dalek` (BSD-3-Clause), `sha2`, `hkdf`,
`aes-gcm` (MIT/Apache-2.0), `subtle` (BSD-3-Clause).

**The `spake2` crate is deliberately not used.** It implements the python-spake2 / CFRG flavour,
which is incompatible with the BoringSSL variant ADB speaks on every observable value — 33-byte
messages with a role prefix instead of 32, a SHA-256 transcript instead of SHA-512, different M/N
points, and no cofactor handling. Details, protocol citations, and the manual end-to-end test are
in `mobile/PAIRING-PLAN.md`.

The protocol is implemented from the Apache-2.0 AOSP and BoringSSL sources cited in each module's
header comment. No code was copied from them, and nothing was taken from any GPL implementation.

Pairing and the TLS connection now share one certificate derivation, `tcp/tls_identity.rs` (see
the 2026-11-04 connection-layer section), and pairing runs under the total deadline described in
"Pairing total deadline" below.

## Connection-layer hardening ported from Kadb (2026-11-04)

Kadb (Apache-2.0, `github.com/flyfishxu/Kadb` at commit `3d1c13e`) is a Kotlin ADB client whose
connection layer handles several cases this crate did not. Its logic was translated, not copied;
each Rust file that carries translated logic says so in its first line, and the app's
`mobile/THIRD-PARTY-NOTICES.md` carries the attribution. Sessions, sync, shell and install were
reworked afterwards; see the next section.

Behavior changes:

- **CNXN negotiation.** The host banner is now `host::features=shell_v2,cmd,stat_v2,ls_v2,
  sendrecv_v2,abb_exec` with no trailing NUL (a NUL would become part of the last feature name on
  the device). `delayed_ack` is not advertised. The device's CNXN is parsed like AOSP's
  `parse_banner`, but tolerates a NUL terminator and a `:` inside a property value. The
  negotiated version is `min(device, ours)`, the payload limit `min(device, 1 MiB)`, and a
  device limit of 0 is refused. The version we send stays `0x01000000`, so adbd keeps writing
  checksums and the reader keeps verifying them; the reader would skip the check only if a
  negotiated version reached `0x01000001`.
- **Handshake.** One deadline covers the whole exchange (TLS upgrade, AUTH rounds and the user
  approving the key), so a peer that trickles replies cannot stretch it. `ADBMessageDevice::new`
  and `ADBTcpService::new_with_timeouts` now default to a 10-second bound instead of the 120-second
  read net. Only `AUTH(TOKEN)` is accepted; the first is answered with a signature, later ones
  with the public key, and more than four fail. A stream packet (`OKAY`, `WRTE`, `CLSE`, `OPEN`)
  during the handshake fails it: the socket is fresh, so it can only come from a broken peer. STLS
  is answered with AOSP's `0x01000000` instead of `1`, and a second STLS is refused.
- **TLS.** The handshake is driven eagerly inside the deadline instead of lazily within the first
  read. The client certificate follows AOSP's `GenerateX509Certificate` shape (`C=US, O=Android,
  CN=Adb`, serial 1, CA, `keyCertSign`/`cRLSign`/`digitalSignature`), keeps rcgen's wide validity
  window so a wrong phone clock cannot invalidate it, and is shared with pairing through
  `tcp/tls_identity.rs`. Only a `certificate_required`, `unknown_ca`, `access_denied`,
  `certificate_unknown` or `bad_certificate` alert becomes `RustADBError::TlsAuthRejected`, as in
  Kadb's `TlsErrorMapper`. An EOF, reset, abort or broken pipe during the handshake or before the
  first message over TLS is `RustADBError::TlsConnectionClosed`: adbd always asks for our
  certificate, so a hang-up after it went out looks the same whether the network dropped or the
  key was refused, and claims nothing about pairing. (An earlier revision of this pass mapped
  that hang-up to `TlsAuthRejected`.)
- **Framing.** The header magic is checked on every header, zero-length ones included, and an
  unknown command is refused; both are `RustADBError::ProtocolError`. An incoming length above
  the limit is refused before anything is allocated; before CNXN that limit is `MAX_PAYLOAD`
  (1 MiB, as in AOSP's host). An outgoing payload above the limit is refused with
  `RustADBError::PayloadTooLarge` before anything is written, because adbd drops the whole
  connection on an oversized packet; before CNXN, writes stay within `MAX_PAYLOAD_V1` (4 KiB).
  After CNXN both directions use the negotiated value. A transport that does not record the
  negotiation (USB) writes at most 4 KiB: that is the `ADBMessageTransport::max_payload` default.
  Header and payload go out in one gathered write, without copying the payload, so a socket that
  accepts nothing is `WriteZero` rather than a busy loop.
- **Sockets.** Every ADB TCP socket gets `TCP_NODELAY` and keepalive (15 s idle, 5 s interval, 3
  probes where the platform allows), best effort, through `socket2`, which the app already links.

New public API:

- `adb_client::ConnectionInfo` with `protocol_version()`, `max_payload()`, `features()` (both
  sides), `has_feature(&str)`, `device_features()` (everything the device listed), `system_type()`
  and `property(&str)`.
- `ADBTcpDevice::connection_info()`, `has_feature(&str)`, `max_payload()`;
  `ADBTcpService::connection_info()` and `ADBTcpService::new_with_timeouts_and_auth_timeout`.
- `RustADBError::ProtocolError(String)`, `TlsAuthRejected(String)`,
  `TlsConnectionClosed(io::Error)` and `PayloadTooLarge(usize, usize)`.
- Crate-internal: `ADBMessageTransport::max_payload()` and `apply_connection_info()`, so
  sessions holding a transport clone can size their writes; `upgrade_connection` takes the
  remaining handshake time.

Changed files, with the Kadb sources translated into each:

- `src/message_devices/adb_connection_info.rs` (new) — `core/AdbProtocol.kt`,
  `core/AdbConnection.kt`
- `src/message_devices/adb_message_device.rs` — `core/AdbConnection.kt`
- `src/message_devices/tcp/tcp_transport.rs` — `core/AdbReader.kt`, `core/AdbWriter.kt`,
  `tls/TlsErrorMapper.kt`, `transport/PlainBlockingChannel.kt`, `transport/TcpKeepAlive.android.kt`
- `src/message_devices/tcp/tls_identity.rs` (new) — `cert/CertUtils.kt`
- `src/message_devices/adb_transport_message.rs` — header validation, with `core/AdbReader.kt`'s
  checks
- `src/message_devices/adb_message_transport.rs`, `src/message_devices/mod.rs`,
  `src/message_devices/tcp/mod.rs`, `src/message_devices/tcp/adb_tcp_device.rs`,
  `src/message_devices/tcp/adb_tcp_service.rs`, `src/message_devices/tcp/pairing/mod.rs`,
  `src/error.rs` — plumbing and accessors
- `src/message_devices/tcp/test_support.rs` (new, tests only) — loopback fake adbd with an
  optional rustls server
- `Cargo.toml`, `Cargo.lock` — `socket2` 0.6 (MIT OR Apache-2.0) with its `all` feature

Not ported: Kadb's background reader thread and per-stream message queue, its delayed-ACK flow
control, lazy reconnect with a generation counter, multi-key client-certificate selection from
the server's CA hints, and its key store. Several of its defaults are weaker than this crate's:
no I/O timeout by default, in-memory keys, and regenerating a key that fails to parse.

The USB transport gets the header magic check through the shared decoder but not the length
check or negotiated limits; it is not built for the app.

## Session, sync, shell and install hardening ported from Kadb (2026-11-04)

The second pass over Kadb `3d1c13e` covers the stream layer. Translated logic is again marked in
each file's first line. Every logical stream now goes through `ADBService`, so no command reads raw
transport packets any more.

Behavior changes:

- **Stream ids and stray packets.** Local ids count up from 1 per connection (zero skipped on
  wrap) instead of being random, so a late packet from a finished stream can never match a newer
  one. Every reader (OPEN replies included) checks both ids and skips at most 64 packets for other
  streams; more is a `ProtocolError`. As AOSP's host does, an orphan `OKAY` is answered with
  `CLSE(0, remote)` so the device closes a service whose OPEN we abandoned; a stray `WRTE` or
  `CLSE` gets nothing. A `CLSE` in reply to OPEN is the device refusing the service
  (`ADBRequestFailed`); any other reply is a `ProtocolError`. A peer `CLSE` is never answered, and
  data the device sent before it stays readable. `ADBSession` is now only an id holder; its
  20 ms drain on drop is gone. The OPEN destination always carries AOSP's trailing NUL.
- **Write sizing.** `ADBService` writes split at the negotiated max payload instead of 4 KiB. Sync
  requests are batched into WRTEs of exactly that size, and each DATA chunk is at most
  `min(64 KiB, max_payload - 8)`, so a TV with a small `maxdata` no longer gets
  `PayloadTooLarge`. Bulk streams (sync, install) use a 30-second socket write bound
  (`STREAM_WRITE_TIMEOUT`) instead of 2 seconds.
- **Desync poisoning.** `TcpTransport` tracks how much of a frame it consumed. A failure after the
  first byte (partial header or payload, timeout mid-message, bad magic, oversized length, bad
  checksum) or a write that fails after taking any byte marks the connection poisoned; every
  later read and write fails at once with `ProtocolError("…lost ADB message framing…")` until
  `connect()` resets it. A timeout before the first byte of a frame stays clean. Once a frame has
  started, the rest gets at least five seconds even when the wait for it was shorter.
- **Sync as a byte stream** (`adb_sync.rs`, from `AdbSyncStream.kt`). Replies are parsed from the
  stream, not from packet boundaries. Push reads the final `OKAY`/`FAIL` (it used to accept any
  WRTE) and a `FAIL` is `RustADBError::SyncFailed` with the device's message; an early `FAIL`
  mid-push stops sending data, sends DONE and reports the refusal, even if the device has also
  closed the stream. Push sends mode `0o100644` (decimal, as AOSP does) and the current time
  instead of `0777` and 0. Pull, stat and list handle `FAIL`; pull rejects DATA over 64 KiB. With
  `stat_v2`, `ls_v2` and `sendrecv_v2` negotiated, stat uses `STA2` (follows symlinks, 64-bit
  size, errno on failure), list uses `LIS2`/`DNT2` (entries the device could not lstat are left
  out) and push/pull use `SND2`/`RCV2` with flags 0. The legacy all-zero `STAT` reply is an error.
  QUIT is followed by a bounded wait for the device's close. Local reader or writer failures are
  `RustADBError::LocalIo`, not `IOError`.
- **Shell** (from `Kadb.kt` and `AdbShellStream.kt`). Shell v2 is chosen by the negotiated
  `shell_v2` feature; the legacy shell is used only without it or when the v2 OPEN is refused.
  `shell_command_with_timeout` returns the new `ShellExit` (`Code(u8)` or `Unreported`, the latter
  only from the legacy shell). A v2 stream that closes before its exit packet, or inside a packet,
  is the new `RustADBError::ShellExitUnknown` ("…its outcome is unknown") instead of `Ok(None)`;
  `ADBShellV2ParseError` is kept for malformed packets. Output stays raw bytes, so UTF-8 split across
  packets or WRTEs is intact (Kadb decodes each packet separately). `exec_out_with_timeout` runs
  binary output over v2 or, without it, `exec:` rather than the pty-backed legacy `shell:`. As in
  AOSP's host and Kadb, a legacy `shell:` service name over 4 KiB (`MAX_PAYLOAD_V1`) is refused
  before its OPEN is sent.
- **Install** (from `Kadb.kt`). `ADBTcpDevice::install_apks(paths, options, timeout)` follows
  AOSP's choice: one APK streams through `exec:cmd package install … -S <size>`; several use an
  `abb_exec` session (else `exec:cmd`) with `install-create`, `install-write … -` and
  `install-commit`. Without `cmd`, APKs are pushed to `/data/local/tmp` and installed with `pm`
  (`install`, or `install-create`/`install-write`/`install-commit`), and each temp file is
  removed. Streamed steps must start with `Success`; `pm` output must have a `Success` line, no
  `Failure` line and no nonzero reported exit. Anything else is `RustADBError::InstallFailed` with
  the package manager's text; a failed session is abandoned. If the device closes the stream
  mid-upload, what it printed before closing is the result; a write that fails any other way
  (timeout, lost framing) is returned as it is. Uninstall goes through the shell and trusts the v2
  exit status (Kadb), or the `Success` reply on the legacy shell.
- **Framebuffer, remount, root, verity** read through `ADBService`. The framebuffer size is
  bounded at 256 MiB. Root, remount and verity succeed once their OPEN is acknowledged: what adbd
  prints is read best effort, and the connection dropping afterwards (adbd restarts itself after
  `root:`) ends the message instead of failing the request. Root still fails on "cannot run as
  root". Reboot, as before, succeeds on the OPEN acknowledgement alone.
- **One bounded read-to-close.** Install replies, legacy `shell:`/`exec:` output, the root,
  remount and verity messages and the post-answer drains all use `ADBService::read_to_close` with
  a per-caller bound (64 KiB for status text, 64 MiB for legacy command output). Exceeding it is
  `ADBRequestFailed`, never a silent truncation.
- **Bounded pull.** `ADBTcpDevice::pull_with_limit(source, output, max_bytes)` stats and receives
  in one sync session; a file the device reports over the limit is `RustADBError::FileTooLarge`
  with nothing written.
- **Early close is a request failure.** A sync or framebuffer stream that the device closes
  before its reply is complete is `ADBRequestFailed` ("the device closed … before it finished"),
  not `IOError(UnexpectedEof)`, so the app keeps the connection; an EOF on the socket itself stays
  `IOError`.

API changes:

- `ADBTcpDevice::shell_command_with_timeout` returns `Result<ShellExit>` (was `Option<u8>`);
  `ADBDeviceExt::shell_command` still returns `Option<u8>`. New `ShellExit`,
  `ADBTcpDevice::exec_out_with_timeout`, `ADBTcpDevice::install_apks` and
  `ADBTcpDevice::pull_with_limit`.
- New errors `SyncFailed(String)`, `InstallFailed(String)`, `LocalIo(io::Error)`,
  `ShellExitUnknown` and `FileTooLarge(u64, u64)`; none means the connection is broken.
- `AdbStatResponse::file_size` and `mod_time` and `ADBListItem::size` and `time` are `u64`.

Changed files, with the Kadb sources translated into each:

- `src/message_devices/adb_sync.rs` (new) — `stream/AdbSyncStream.kt`
- `src/message_devices/adb_service.rs` — `stream/AdbStream.kt`
- `src/message_devices/commands/shell.rs` — `Kadb.kt`, `shell/AdbShellStream.kt`
- `src/message_devices/commands/install.rs` — `Kadb.kt` (now also holds uninstall;
  `commands/uninstall.rs` and `commands/utils/message_writer.rs` are removed)
- `src/message_devices/tcp/tcp_transport.rs`, `adb_message_device.rs`, `adb_session.rs`,
  `adb_transport_message.rs`, `commands/{push,pull,stat,list,framebuffer,remount,root,verity}.rs`,
  `commands/utils/shell_message_writer.rs`, `tcp/adb_tcp_device.rs`, `models/shell_exit.rs`
  (new), `models/adb_stat_response.rs`, `models/list_info.rs`, `models/mod.rs`,
  `server_device/commands/list.rs`, `error.rs`, `lib.rs`, `mod.rs` files — plumbing
- `src/message_devices/tcp/stream_tests.rs` (new, tests only) and `tcp/test_support.rs` — a
  scripted fake device (`Peer`) for stream, sync, shell and install tests

Not ported: Kadb's background reader thread and per-stream queues (one request runs at a time per
connection here, so ids plus bounded skipping do the same job), `delayed_ack` flow control (not
advertised), its PTY shell session and TCP forwarder, and recursive push/pull. Kadb weaknesses
deliberately not copied: unbounded waits, a legacy shell whose exit is reported as 0, decoding
shell output per packet, accepting any `[…]` as a session id, and judging old `pm` output only
by its first line.

## Real-device verification and the Android 14 OKAY fix (2026-11-04)

The transport was run against two NVIDIA Shields (Android 11, plain TCP), a Pixel 10 Pro's
wireless-debugging port (Android 16, TLS) and an Android TV 14 emulator (pairing, TLS and plain
TCP). Results and logs: `.agent-tasks/kadb-parity-20261104/device-test/host/RESULTS.md` on Bryan's
machine.

- **Spare OKAYs.** Android 14's adbd (`local_socket_flush_incoming` in
  `packages/modules/adb/sockets.cpp`, `android14-release`) sends an OKAY after every flush into the
  service that made progress, so one large WRTE can draw several. Android 13 and older send one. The
  stream layer treated OKAY as a per-WRTE acknowledgement and failed every multi-packet push and
  every install of an APK larger than one WRTE on Android 14 with `ProtocolError("Unexpected OKAY
  while reading ADB service")`. `ADBService` now treats OKAY as AOSP's host does, as "ready for
  more": any OKAY lets the next WRTE go and spares are skipped while reading. Each OKAY stands for
  at least one byte adbd delivered, so more OKAYs than bytes written on a stream is a
  `ProtocolError` rather than an endless loop. Tests: `spare_okays_after_a_write_are_skipped_by_the_next_read`,
  `a_spare_okay_lets_the_next_write_go`, `more_okays_than_bytes_written_is_a_protocol_error`
  (`adb_service.rs`), `push_survives_several_okays_for_each_write_as_android_14_sends` and
  `a_multi_packet_install_survives_several_okays_for_each_write` (`tcp/stream_tests.rs`, with
  `Peer::okays_per_write` in `tcp/test_support.rs`).
- **`examples/device_smoke.rs`** (registered in `Cargo.toml`, since `autoexamples` is off): a
  reusable smoke harness with `probe`, `suite`, `sync`, `pair`, `install`, `uninstall`, `shell` and
  `latency` subcommands. Without arguments it prints usage and exits 0. Device files go only under
  `/data/local/tmp/atvopt-devtest-*` and are removed; `uninstall` refuses packages outside
  `com.atvoptimizer.devtest.`.

## Required existing identity (desktop, 2026-10-25; rebuilt on the 2026-11-04 layer)

`ADBTcpDevice::new_with_required_private_key_and_auth_timeout`,
`ADBTcpDevice::new_with_required_key_and_control` and
`ADBTcpService::new_with_required_key_and_timeouts` opt out of the upstream ephemeral RSA-key
fallback: when the key file is gone they return `IOError(NotFound)` before connecting, and
nothing is generated or written. `new_with_control_and_auth_observer` does the same when asked
to. The dedicated connections below (`run_bounded_shell`, `run_identity_bound_shell`,
`open_bound_service`, `run_streamed_install`) always require the key. Existing constructors keep
their behavior; Android's identity-creation policy stays in the shared first-party caller.
`ADBTcpService::new_with_required_key_and_timeouts` bounds the whole handshake by its
`read_timeout`.

Changed source: `adb_message_device.rs` (`new_with_required_key`), `tcp/adb_tcp_device.rs`,
`tcp/adb_tcp_service.rs`.

## Pairing total deadline (2026-10-27)

Pairing wraps its owned socket so every underlying read and write refreshes the remaining total
budget, including repeated I/O inside rustls. Previously a slowly delivered TLS record could
extend the advertised exchange deadline. Timeout errors keep the dedicated `PairingError::Timeout`
classification. The certificate comes from `tcp/tls_identity.rs`, shared with the TLS
connection (2026-11-04). Tests use the checked-in public test identity in memory and self-owned
loopback sockets for refused ports, silent peers and trickled TLS records; these do not establish
Android or TV pairing acceptance.

Changed source: `tcp/pairing/mod.rs`.

## Bounded and cancelable operations: `BoundedShellControl` (2026-10-27, 2026-11-02)

`BoundedShellControl::new(total)` starts one total deadline with a cancel handle (`cancel`,
`remaining`, `was_registered`). `TcpTransport` takes an optional control:

- On a dedicated connection (`TcpTransport::with_shell_control`) the control owns the socket from
  the TCP connect on. The connect waits at most `min(remaining, 5 s)`; every read and write,
  the TLS handshake included, is capped by the remaining budget; a cancel or the deadline shuts
  the socket down. Socket waits are sliced to 50 ms so a cancel is also noticed where shutting a
  socket down does not wake a blocked Winsock `recv`, and the 2026-11-04 rules still apply
  underneath: a frame that has started still gets the frame-completion time, but never past the
  control's deadline, and one stopped part-way poisons the connection.
- On a connection that already exists, `set_current_control` registers the socket as borrowed
  for one operation: normal completion leaves it open, a cancel or the deadline shuts it down,
  and a control that is already spent is refused without touching the socket.
- The socket is held as one `Arc<TcpStream>` (`SharedSocket`) shared by the plain stream, the
  TLS stream and the control, instead of a `try_clone`d handle, so cancellation always reaches
  the handle the I/O runs on.
- A failure caused by the control reports the control's error (`TimedOut` or
  `ConnectionAborted`) rather than the hang-up its shutdown caused, so a cancel during TLS is not
  mistaken for `TlsConnectionClosed`.

`ADBTcpDevice::shell_command_with_control`, `exec_out_with_control`, `push_with_control`,
`pull_with_control` and `reboot_with_control` run one operation on the persistent connection
under a borrowed control, never retried. `exec_out_with_control` is `exec_out_with_timeout`
(shell v2 when negotiated, otherwise `exec:`) under the control, so binary output such as a
screenshot works on devices without shell v2. The shell is chosen as for `shell_command_with_timeout` (shell v2 when negotiated,
the legacy shell only without it or when the v2 OPEN is refused); it returns `Option<u8>`, `None`
being the legacy shell's missing exit status. Push and pull are the 2026-11-04 sync code.

Changed source: `tcp/bounded_shell.rs` (the control), `tcp/tcp_transport.rs`,
`tcp/adb_tcp_device.rs`.

## Bounded shell-v2 (2026-10-27)

`run_bounded_shell` and `run_identity_bound_shell` own a dedicated required-key connection under
a control. They OPEN `shell,v2,raw:<command>` whatever the device advertised, send shell v2's
stdin-close packet without waiting for its OKAY (the spare-OKAY handling of 2026-11-04 absorbs
it), and decode with the crate's shell v2 decoder, which now forwards output as it arrives instead
of allocating each declared packet. Each stream keeps at most `SHELL_OUTPUT_LIMIT` (256 KiB); a
stream that would exceed it ends the command with `BoundedShellTermination::OutputLimit`. The
deadline ends it with `Timeout` and the partial output that arrived; a stream that closes before
its exit packet stays `RustADBError::ShellExitUnknown`. Incoming frames are bounded by the
negotiated payload limit (at most 1 MiB). A refused v2 OPEN fails and is never replayed as a
legacy `shell:`; OPEN diagnostics name only the service kind, never the command. After the exit
packet, the close is awaited briefly (at most 2 s and half the remaining budget) and the command
is `Completed`. `run_identity_bound_shell` first checks the hardware id (see the guarded install
section).

Loopback tests use the fixture identity in memory and cover both streams, exit status,
continuous output, the exact limit, partial payloads, trickled headers, cancellation, oversized
frames, redaction and no replay. These scripted peers do not establish AUTH/STLS or TV
interoperability.

Changed source: `tcp/bounded_shell.rs`, `commands/shell.rs` (incremental decoder, now shared),
`commands/mod.rs`.

## Guarded single-APK streaming and identity-bound services (2026-10-27)

`run_streamed_install` streams one APK of a declared size through `exec:cmd package install
[-r] -S <size>` on a dedicated required-key connection, with an explicit replace flag. Before the
install OPEN, a bounded shell-v2 `getprop ro.serialno` on that connection must succeed (exit 0,
no stderr, one readable line under 1 KiB) and equal the caller's expected hardware id;
`unknown` and empty never match. Input must be exactly the declared length: a short or growing
source is caught before the last chunk is sent. Chunks are the negotiated payload size (at most
1 MiB); output that arrives while a chunk waits for its OKAY is held only up to
`INSTALL_RESPONSE_LIMIT` (64 KiB), and the last chunk is sent with `ADBService::write_final`,
without waiting for its OKAY, before the result is read. Only exactly `Success\n` (or
`Success\r\n`) followed by the service's clean close is `InstallStatus::Success`; a single
`Failure [...]` or `Error: ...` line with a clean close is `Rejected`; a success that arrived
before the upload finished, a lost or truncated result, a timeout or a cancel is
`Indeterminate`, and nothing is retried. Errors before the install OPEN are `Err`.

This is a separate policy layer over the same `ADBService` stream code that
`ADBTcpDevice::install_apks` (2026-11-04) uses, not a second framing implementation.
`install_apks` reports any reply starting with `Success` as installed and everything else as
`RustADBError::InstallFailed`, with no indeterminate outcome and no identity check, so it cannot
carry all of these guarantees without changing what the mobile app relies on. It does now carry
the early-result rule (2026-11-05): in its streamed path (`exec:cmd` or `abb_exec`, one APK or a
split), a reply that starts with `Success` but arrived before the last chunk went out, or with a
close before every declared byte was sent, is `ADBRequestFailed` ("... whether it installed is
not confirmed"), never success. A refusal that arrives early stays `InstallFailed` with its text,
and a `Success` after the last byte still counts when the device closes before acknowledging that
last WRTE. Callers that need the identity check run it on the same connection first.

`open_bound_service` opens an arbitrary service the same way: hardware-id check first, then the
OPEN, with a finite lifetime, an independent `cancellation()` handle and at most 64 KiB of unread
output held while a write waits. `BoundServiceError::BeforeOpen` means no OPEN was sent;
`Rejected` means the device answered CLSE; `OpenUncertain` means the OPEN may have reached the
device. Dropping the stream cancels its socket before protocol cleanup.

`ADBTcpDevice::hardware_serial_bounded` reads the same property on the persistent connection
under a borrowed control, and `verify_hardware_binding_bounded` checks it on a dedicated one.
The application supplies the expected id using its existing canonical validation.

Fixture tests use only the test identity in memory and scripted loopback peers. They do not
install APKs, exercise Android's package manager, establish AUTH/STLS interoperability or
certify hardware compatibility.

Changed source: `tcp/streamed_install.rs`, `tcp/bound_service.rs`, `commands/install.rs`
(the early-result rule), `adb_service.rs` (`write_final`, `with_pending_limit`, `take_pending`,
`set_read_timeout`, `bytes_sent`, and
`open_stream_classified`, which tells a CLSE refusal apart from an unknown OPEN outcome),
`adb_message_device.rs` (`open_stream_service_classified`).

## Cancelable connection handoff and TLS rejection (2026-11-02)

`ADBTcpDevice::new_with_control_and_auth_observer` authenticates under a
`BoundedShellControl`: the 2026-11-04 single handshake deadline is set to the control's
remaining budget, and the control also covers the TCP connect, the TLS upgrade and the wait for
the user's approval, so either limit ends the handshake and a cancel closes only that socket. The
observer runs once, after our RSA public key was written in answer to an AUTH token (the point at
which the device may show "Allow debugging?"); never for an already trusted key, over TLS, or
when that write fails. `ADBTcpDevice::new_with_key_and_auth_observer` does the same without a
control. The control keeps applying until the caller publishes the connection with
`finish_connection`, which detaches the deadline without closing the socket and fails once the
control was cancelled or expired. A failed attempt closes its socket.

`RustADBError::is_client_identity_rejected` follows the 2026-11-04 mapping: it is true for
`TlsAuthRejected`, and for the same typed TLS alerts (`certificate_required`, `unknown_ca`,
`access_denied`, `certificate_unknown`, `bad_certificate`) carried as a rustls error. It is false
for `TlsConnectionClosed`, timeouts, other transport errors and error text that merely names an
alert, so a hang-up never claims a pairing problem. The shared app transport uses it to explain
the PIN pairing and separate connect-port steps.

The shared `crates/direct-adb/src/connection_tests.rs` fixtures exercise cancellation before AUTH,
during public-key approval and during stalled TLS, a total handshake deadline, blocked
publication cleanup, the retained socket's first shell response and a TLS certificate-rejection
alert. Vendored tests cover the observer, cancellation during approval, the control deadline,
publication after expiry and a cancelled TLS handshake. These are protocol fixtures, not Android
pairing interoperability evidence.

Changed source: `adb_message_device.rs`, `tcp/adb_tcp_device.rs`, `tcp/tcp_transport.rs`,
`error.rs`.

## Sync response framing (2026-11-02; superseded by `adb_sync.rs`)

The desktop branch's sync receiver and upload acknowledgement reader in `adb_session.rs` were
replaced by the 2026-11-04 byte-stream sync (`adb_sync.rs`), which handles fragmented DATA,
DONE, FAIL and STAT, explicit FAIL, and the final OKAY. Its regression cases were ported to the
scripted fake device in `tcp/stream_tests.rs`: a FAIL split into 3-byte WRTEs at SEND, the first
DATA and a later DATA, with the transport OKAY before or after it; DONE-like bytes inside file
content at many WRTE widths; a fragmented permission FAIL never written as file data; truncated,
unterminated and unknown replies; a reply from the wrong remote stream; a stalled partial
rejection under a total deadline; and controlled push, pull and stat.

One gap they found is fixed in `adb_sync.rs`: a sync `OKAY` that arrived before DONE was sent
(after the push stopped early because the device answered) was accepted as a completed upload. It
is now `UnknownResponseType("sync push: premature OKAY ...")`, since adbd sends OKAY only after
DONE.

## Merge of the desktop branch onto the Kadb rewrite (2026-11-05)

The desktop native-ADB branch (bounded operations, required identity, guarded install) and the
2026-11-04 rewrite changed the same files. The rewrite is the base; the desktop features above
were rebuilt on its connection info, single handshake deadline, monotonic stream ids, bounded
stray-packet skipping, spare-OKAY handling, desync poisoning, byte-stream sync, `ShellExit`, and
read-to-close bounds, rather than on the internals the rewrite removed. Superseded and dropped:
the desktop branch's `RustADBError::ServiceRejected` (a CLSE in reply to OPEN stays
`ADBRequestFailed` as in the rewrite; `OpenFailure` classifies it internally), its random stream
ids, its OPEN label redaction (the rewrite's errors already name only the service kind), its
`shell_command_prefer_v2_with_timeout` (shell v2 is chosen by the negotiated feature), its sync
code in `adb_session.rs`, and its transport payload check (the rewrite's negotiated limit
applies).

## Real-device check of the merged crate, and one handshake retry (2026-11-05)

`examples/device_smoke.rs` gained two subcommands for the desktop layer: `control` (controlled
handshake and handoff, bounded hardware-ID reads that keep the connection, ID binding match and
mismatch, controlled shell, exec-out screenshot and a 1 MiB push/pull round trip, deadline and
cancel interrupting `sleep 30`, the dedicated bounded shell's streams, 256 KiB limit and partial
output on timeout, identity-bound shell and service, and a mismatched ID never running the
command) and `approve` (a key the device may not trust yet: the AUTH observer, the approval
prompt, `--cancel-after-prompt`). Both print the hardware ID's length, never its value.

Results on the merged crate: both Shields (Android 11, plain TCP) `suite` 28/28, `control` 17/17
and the install cases; the Android TV 14 emulator `suite` 28/28 and `control` 17/17 on the plain
port and over TLS after pairing, first-time approval with the observer firing ~45 ms after
connect, a trusted reconnect in ~35 ms, and a cancel during approval closing the socket ~10 ms
later.

Over TLS, adbd sometimes closed a connection without answering CNXN when it came right after the
previous connection closed: 2 of 20 back-to-back connects on the emulator, 0 of 30 with 300 ms
between them, 0 of 20 on the plain port. `ADBMessageDevice::greet` now opens one fresh
connection, 150 ms later and within the same deadline, when the socket closes before any reply;
nothing has been asked of the device by then. A close after any reply, or a second silent close,
is returned as before. After the change: 40/40 back-to-back TLS connects and five `control` runs
at 17/17. Tests: `a_connection_closed_before_any_reply_is_tried_once_more`,
`a_second_silent_close_is_returned_without_a_third_attempt`,
`a_close_after_the_device_answered_is_not_retried` (`tcp/stream_tests.rs`).

## Several identities: preferred keys before the key file (2026-11-06)

The desktop's built-in engine offers Google platform-tools' own key before the app's key, so a
TV that already allows platform-tools needs no new prompt. `adb_client::tcp::PreferredKeys` holds
those extra identities in memory only (parsed from unencrypted RSA 2048 PKCS#8 or PKCS#1 PEM,
`Debug` prints only the count, errors are `RustADBError::UnusableKey` and never echo the key). Every
connection orders its keys as the preferred keys, then the key file. With `PreferredKeys::none()`
every path is the one it was: the existing constructors and functions now delegate with none, and
the fake-adbd tests below pin that.

- **AUTH.** Token *k* is signed with key *k*; once every key is refused, the first key's public
  key is offered (AOSP's `send_auth_response` signs with each loaded key, then sends the user key).
  The limit is the key count plus the three public-key rounds one key always had, so one key still
  answers exactly four tokens.
- **TLS.** `ClientCertificates` presents, in order of preference: a certificate the connection
  was told to present, the first key whose SHA-256 `SubjectPublicKeyInfo` fingerprint adbd names
  in its CA hints (`O=AdbKey-0, CN=<hex>`, `tls/adb_ca_list.cpp`; the host side is
  `adb_tls_set_certificate` in `client/auth.cpp`), else the first key. When the device refuses the
  certificate (`is_client_identity_rejected`) and other keys remain, `ADBMessageDevice::connect`
  opens one fresh connection per untried key under the same deadline. One key never retries.
- **Pairing.** `pair_with_keys` registers the first preferred key (falling back to `pair` with
  none), the key a later connection offers for approval.
- **What was accepted.** `ADBTcpDevice::accepted_key()` returns `AcceptedKey::Signature(i)`,
  `PublicKey(0)` or `Tls { index, hinted, refused }`.

New entry points: `PreferredKeys`, `AcceptedKey`, `ADBTcpDevice::new_with_keys_control_and_auth_observer`,
`ADBTcpDevice::new_with_required_keys_and_control`, `run_identity_bound_shell_with_keys`,
`open_bound_service_with_keys`, `run_streamed_install_with_keys`, `pair_with_keys`.
`examples/device_smoke.rs` takes `--extra-key <pem>` (repeatable) and prints `accepted_key`.
Changed files: `adb_message_device.rs`, `adb_message_transport.rs`, `models/adb_rsa_key.rs`,
`error.rs`, `tcp/{mod,preferred_keys,tls_identity,tcp_transport,adb_tcp_device,bounded_shell,
bound_service,streamed_install,test_support}.rs`, `tcp/pairing/mod.rs`.

Tests (fake adbd over loopback): `each_auth_token_is_signed_with_the_next_key_in_order`,
`a_first_signature_accepted_needs_no_second`,
`after_every_signature_is_refused_the_first_key_is_offered_once`,
`endless_tokens_get_one_signature_per_key_then_three_public_keys` (also pins one key at four),
`no_preferred_keys_answer_auth_exactly_as_the_key_file_alone`,
`tls_presents_the_key_the_device_hints_at_first_try`,
`without_a_matching_hint_tls_presents_the_first_key`,
`a_refused_certificate_is_retried_once_with_the_next_key`,
`a_wrong_hint_falls_back_to_the_other_key_once`,
`when_every_certificate_is_refused_each_is_tried_once_and_the_refusal_returned`,
`one_key_presents_it_despite_hints_and_never_retries`,
`pairing_with_preferred_keys_registers_the_first_one`, `adb_key_hints_are_read_and_other_names_ignored`,
`fingerprint_is_the_sha256_of_the_standard_public_key_info`, and the `preferred_keys` parsing tests.

On the Android TV 14 emulator (`device_smoke`, logs kept locally): legacy port, the host's trusted
`adbkey` first and a never-seen key second connected with `Signature(0)`; the never-seen key first
connected with `Signature(1)` (PKCS#8 and PKCS#1 alike) and `control` passed 17/17 that way, every
dedicated connection signing twice; the never-seen key alone, and two never-seen keys, raised
"Allow USB debugging?" showing the first key's fingerprint, dismissed with Cancel. Over TLS,
adbd's hint list matched OpenSSL's SPKI SHA-256 of the paired keys; `[adbkey, paired]` and
`[never-seen, paired]` both connected on the first certificate by hint (`Tls { hinted: true,
refused: 0 }`, index 0 and 1), and two unpaired keys produced exactly two refused handshakes and
`TlsAuthRejected`.

## Signing-only AUTH: no public key unless asked (2026-11-08)

Bryan's first hands-on test of the built-in engine: a scan put "Allow debugging?" on every TV that
had not allowed this computer, because a connection answered the AUTH token after the last refused
signature with `AUTH_RSAPUBLICKEY`. The desktop now offers its key only from an explicit Request
approval, so the crate can connect without ever offering one.

- `ADBTcpDevice::new_with_keys_and_control_signing_only` (the same keys, control and TLS behavior as
  `new_with_keys_control_and_auth_observer`) answers AUTH tokens with a signature from each key in
  turn and nothing else. When the device sends one more token after refusing every signature, the
  handshake ends with the new `RustADBError::KeyNotAllowed` before anything is written, and the
  socket closes. adbd shows its prompt only on receiving a public key (`adbd_auth_verify` sends a
  fresh token after a bad signature), so the device shows nothing.
- `ADBMessageDevice` threads a `PublicKeyOffer` (`Offer(observer)` or `Never`) where it took an
  optional observer; every existing constructor passes `Offer`, so they behave exactly as before
  (the Android app still offers its key on connect).
- TLS (Wireless debugging) is unchanged: it presents certificates, never shows a prompt, and a
  refused certificate is still `TlsAuthRejected`.

Tests (fake adbd over loopback): `signing_only_never_writes_a_public_key_and_reports_not_allowed`
(one and two keys: every token signed, no `AUTH_RSAPUBLICKEY`, the socket closed at once rather than
at the deadline), `signing_only_connects_with_a_key_the_device_trusts`,
`signing_only_presents_tls_certificates_as_before`. `examples/device_smoke.rs` gains `quiet <addr>
--key <pem> [--expect-not-allowed]`.

On the Android TV 14 emulator (`device_smoke`, legacy port, throwaway keys; logs and screenshots
kept locally): `quiet` with a never-seen key ended `KeyNotAllowed` in about 30 ms, twice, with no
prompt on screen; adbd's log shows only its repeated `send_auth_request` and the host hanging up.
`approve` with a 600-second bound then raised "Allow USB debugging?" with that key's fingerprint
and connected 83.6 s later, after "Always allow" and Allow (`PublicKey(0)`); `quiet` with the same
key then connected at once with `Signature(0)`. A cancel 5 s after the prompt closed the socket in
8 ms, but the prompt stayed on screen, and allowing it then still let the next `quiet` connect.
Choosing Cancel on the TV (Deny) logged `adbd_auth: received packet: NO` and sent the host nothing:
the request waited out its 40-second bound.

`PreferredKeys::signing_only()` carries the same policy to every other connection made with
those keys, including the dedicated ones behind `connect_required` (identity-bound shells,
bound services, streamed installs, `new_with_required_keys_and_control`): `ADBMessageDevice`
never offers a public key for signing-only keys. Plain `PreferredKeys` (and `none()`) still
offer it, so the Android app is unchanged.
`signing_only_keys_keep_dedicated_connections_quiet_and_default_keys_unchanged` pins both.

Clippy 1.98 also flagged older code under `--all-targets -D warnings`; fixed without changing
behavior: `framebuffer_info.rs` reads its words with `as_chunks::<4>()`, `adb_emulator_device.rs`
uses `unwrap_or`, the pairing test modules allow `cast_possible_truncation` and `format_collect`
for their fixtures, and `device_smoke`'s `cmd_control` allows `too_many_lines`.

# Shield Optimizer patch

This directory starts from the exact `adb_client` 3.2.2 crates.io source, whose upstream commit is
recorded in `.cargo_vcs_info.json`. It remains MIT-licensed; `LICENSE` is copied from the upstream
repository because crates.io did not include it in the published archive.

Shield Optimizer adds one generic API: `adb_client::tcp::ADBTcpService`. It opens an arbitrary
device service over a dedicated authenticated connection and implements `std::io::Read` and
`std::io::Write` with correct ADB `OPEN`, `WRTE`, `OKAY`, and `CLSE` framing. The implementation is
generic and contains no scrcpy- or product-specific behavior.

Changed source files:

- `src/message_devices/adb_service.rs` — logical-stream framing and protocol tests
- `src/message_devices/adb_message_device.rs` — internal raw-service opener
- `src/message_devices/mod.rs` — internal module registration
- `src/message_devices/tcp/adb_tcp_service.rs` — public dedicated TCP service stream
- `src/message_devices/tcp/tcp_transport.rs` — physical-EOF handling for bounded stream failure
- `src/message_devices/tcp/mod.rs` — public export

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

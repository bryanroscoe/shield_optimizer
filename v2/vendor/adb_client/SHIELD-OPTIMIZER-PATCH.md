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
`mobile/THIRD-PARTY-NOTICES.md` carries the attribution. Sessions, sync, shell decoding and
install are unchanged.

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
  `tcp/tls_identity.rs`. A `certificate_required`, `unknown_ca`, `access_denied`,
  `certificate_unknown` or `bad_certificate` alert, or the device hanging up once it has our
  certificate and before its first message, becomes `RustADBError::TlsAuthRejected`.
- **Framing.** The header magic is checked on every header, zero-length ones included, and an
  unknown command is refused; both are `RustADBError::ProtocolError`. An incoming length above
  the negotiated limit (1 MiB before negotiation) is refused before anything is allocated. An
  outgoing payload above it is refused with `RustADBError::PayloadTooLarge` before anything is
  written, because adbd drops the whole connection on an oversized packet. Header and payload go
  out in one `write_all`, so a socket that accepts nothing is `WriteZero` rather than a busy loop.
- **Sockets.** Every ADB TCP socket gets `TCP_NODELAY` and keepalive (15 s idle, 5 s interval, 3
  probes where the platform allows), best effort, through `socket2`, which the app already links.

New public API:

- `adb_client::ConnectionInfo` with `protocol_version()`, `max_payload()`, `features()` (both
  sides), `has_feature(&str)`, `device_features()` (everything the device listed), `system_type()`
  and `property(&str)`.
- `ADBTcpDevice::connection_info()`, `has_feature(&str)`, `max_payload()`;
  `ADBTcpService::connection_info()` and `ADBTcpService::new_with_timeouts_and_auth_timeout`.
- `RustADBError::ProtocolError(String)`, `TlsAuthRejected(String)` and
  `PayloadTooLarge(usize, usize)`.
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

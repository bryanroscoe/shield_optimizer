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

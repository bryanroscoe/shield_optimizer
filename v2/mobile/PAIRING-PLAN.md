# Android 11+ wireless-debugging pairing

The six-digit-code flow, implemented clean-room in Rust inside the vendored `adb_client`
(`vendor/adb_client/src/message_devices/tcp/pairing/`) and called from
`WirelessAdb::pair` in `mobile/src-tauri/src/wireless_adb.rs`.

Before this, `pair` returned "isn't supported yet" and users had to fall back to Network
debugging on `:5555`, which modern Google TV builds do not offer. This closes that gap.

## Licensing

This product ships commercially, so only permissively licensed sources were referenced:

- AOSP `packages/modules/adb/**` — Apache-2.0 (`MODULE_LICENSE_APACHE2` at the module root).
- BoringSSL `crypto/curve25519/spake25519.cc` — Apache-2.0 (per its own file header).
- Rust crates: `curve25519-dalek` and `subtle` (BSD-3-Clause), `sha2` / `hkdf` / `aes-gcm`
  (MIT OR Apache-2.0), plus `rustls` / `rcgen` / `rsa` already in the vendored crate.

**No GPL source was consulted.** In particular nothing was taken from `libadb-android`, and no
code was copied verbatim from AOSP or BoringSSL — the implementation was written from the
protocol facts those sources establish, each of which is cited below.

## Protocol, point by point, with the file each was verified in

Everything below was read from the live AOSP / BoringSSL trees while implementing, not from
memory.

### 1. TLS 1.3 with a self-signed client certificate, any server certificate accepted

`packages/modules/adb/tls/tls_connection.cpp` pins **both** the minimum and maximum protocol
version to `TLS1_3_VERSION` in `TlsConnectionImpl::DoHandshake`, registers the caller's
certificate and key with `SSL_CTX_set_chain_and_key`, and sets
`SSL_VERIFY_PEER | SSL_VERIFY_FAIL_IF_NO_PEER_CERT` — so a client certificate is mandatory.

`pairing_connection/pairing_connection.cpp` `SetupTlsConnection()` then installs
`tls_->SetCertVerifyCallback([](X509_STORE_CTX*) { return 1; })` with the comment "Allow any
peer certificate". Both sides therefore accept whatever certificate the other presents.

That is safe because the certificate is not what authenticates the peer — SPAKE2 is, and the
TLS session is bound into it via the exporter output (step 2). An attacker who terminates the
TLS session gets a different exporter value and so a different SPAKE2 password, and cannot
relay the exchange onto another session.

We therefore use `rustls` with a `dangerous().with_custom_certificate_verifier(...)` verifier
that accepts any server certificate — the same shape `tcp_transport.rs` already uses for
`_adb-tls-connect`.

The client certificate is derived exactly as `vendor/adb_client/src/message_devices/tcp/
tcp_transport.rs::certificate_from_pk` derives it: `KeyPair::from_pkcs8_pem_and_sign_algo(pem,
&PKCS_RSA_SHA256)` then `CertificateParams::default().self_signed(&key_pair)`. Reusing that
derivation is load-bearing: the RSA public key inside the pairing certificate, the RSA public
key we send in `PeerInfo`, and the RSA public key the later `ADBTcpDevice` TLS connect presents
must all be the same key.

### 2. Exported keying material: label `"adb-label\0"`, 64 bytes

`tls/tls_connection.cpp` line 36: `static constexpr char kExportedKeyLabel[] = "adb-label";`
and `TlsConnectionImpl::ExportKeyingMaterial` calls

```
SSL_export_keying_material(ssl_.get(), out.data(), out.size(), kExportedKeyLabel,
                           sizeof(kExportedKeyLabel), nullptr, 0, false)
```

`sizeof(kExportedKeyLabel)` is **10**, i.e. the label *includes the NUL terminator*. The
length is `PairingConnectionCtx::kExportedKeySize = 64`
(`pairing_connection/pairing_connection.cpp`).

`use_context` is `false`. For TLS 1.3 BoringSSL maps that to an empty context, which is what
rustls does for `context: None`, so `export_keying_material(&mut out, b"adb-label\0", None)`
matches.

### 3. SPAKE2 — BoringSSL's variant, not the CFRG/python-spake2 one

`pairing_auth/pairing_auth.cpp`:

```
static constexpr spake2_role_t kClientRole = spake2_role_alice;
static const uint8_t kClientName[] = "adb pair client";
static const uint8_t kServerName[] = "adb pair server";
...
my_len = sizeof(kClientName);            // 16 — includes the NUL
SPAKE2_CTX_new(spake_role, my_name, my_len, their_name, their_len)
SPAKE2_generate_msg(ctx, key, &key_size, SPAKE2_MAX_MSG_SIZE, pswd.data(), pswd.size())
```

The client is Alice, the identities carry their NUL (`sizeof`, not `strlen`), and the password
is the raw buffer.

`pairing_connection/pairing_connection.cpp` `SetupTlsConnection()` builds that password:

```
pswd_.insert(pswd_.end(), exportedKeyMaterial.begin(), exportedKeyMaterial.end());
auth_ = CreatePairingAuthPtr(role_, pswd_);
```

so **password = the six ASCII digits, then the 64 exporter bytes** — concatenated, in that
order. `pswd_` starts as the user's code (`client/adb_wifi.cpp` `adb_wifi_pair`).

#### Why the `spake2` crate cannot be used

This was the one point where the plan had to change. `SPAKE2_MAX_MSG_SIZE` is **32**
(`include/openssl/curve25519.h`) and `SPAKE2_MAX_KEY_SIZE` is **64**. Reading
`crypto/curve25519/spake25519.cc` shows BoringSSL's construction is its own:

| | BoringSSL (what ADB speaks) | `spake2` crate / python-spake2 |
| --- | --- | --- |
| Message | 32 bytes, bare compressed point | 33 bytes, `"A"`/`"B"` side prefix + point |
| Transcript hash | SHA-512, 64-byte output | SHA-256, 32-byte output |
| M, N | iterated SHA-256 of `edwards25519 point generation seed (M\|N)` | CFRG draft constants |
| Password → scalar | `reduce(SHA-512(pw))`, low 3 bits cleared by *adding* multiples of the order | SHA-256-based blinding |
| Cofactor | ephemeral scalar multiplied by 8; unreduced 256-bit scalars throughout | not handled |

Both claim compatibility with "SPAKE2", but not with each other; the header comment in
`curve25519.h` points at draft-irtf-cfrg-spake2-02 while the implementation diverges from it.
Shipping the `spake2` crate would have produced a pairing that can never succeed against a
device. So `pairing/spake2.rs` implements BoringSSL's variant directly on
`curve25519-dalek`.

The details that have to match bit-for-bit, all from `spake25519.cc`:

- `M` compressed = `5ada7e4bf6ddd9adb6626d32131c6b5c51a1e347a3478f53cfcf441b88eed12e`,
  `N` compressed = `10e3df0ae37d8e7a99b5fe74b44672103dbddcbd06af680d71329a11693bc778`
  (documented at the top of the file next to their decimal affine coordinates and the Python
  generator that produced them). Alice masks with `M` and unmasks the peer with `N`.
- Ephemeral scalar: 64 random bytes → `x25519_sc_reduce` → `left_shift_3`, kept as an
  unreduced 256-bit little-endian integer. The `×8` is what clears any cofactor component of
  the peer's point.
- Password scalar: `x25519_sc_reduce(SHA512(password))`, then — because BoringSSL's original
  code omitted the matching `left_shift_3` and could not change the wire format — the low three
  bits are cleared by adding `order`, `2·order`, `4·order` conditionally, *each test reading the
  running value*. This changes the mask by a small-order point, which is why the scalar must
  stay unreduced and why every multiplication in our implementation is a full 256-bit ladder
  rather than a `Scalar` multiply. Both `x25519_ge_scalarmult_small_precomp` (64 iterations over
  four 64-bit limbs) and `x25519_ge_scalarmult` (`for i in 0..256 step 4`) in
  `crypto/curve25519/curve25519.cc` consume all 256 bits with no reduction, which is what
  licences that reading.
- Transcript: SHA-512 over six length-prefixed fields, the prefix being the length as a
  little-endian `u64` (`update_with_length_prefix`). Alice's order is `my_name`, `their_name`,
  `my_msg`, `their_msg`, `dh_shared_encoded`, `password_hash` — where `password_hash` is the
  **full 64-byte** `SHA512(password)` (`spake2_ctx_st.password_hash[64]` in
  `crypto/curve25519/internal.h`), not the reduced scalar.

### 4. `PairingPacketHeader`

`pairing_connection/pairing_connection.cpp`:

```
struct PairingPacketHeader {
    uint8_t version;   // == 1, and kMin == kMax == 1
    uint8_t type;      // PairingPacket::Type
    uint32_t payload;
} __attribute__((packed));
```

`WriteHeader` applies `htonl` to `payload` only, so the wire form is `[version][type][payload
big-endian]` — 6 bytes. `ReadHeader` rejects a version outside `[1, 1]`, an unknown type, and a
payload that is `0` or `> kMaxPayloadSize`, where `kMaxPayloadSize = kMaxPeerInfoSize * 2 =
16384`. `PairingPacket::Type` is `SPAKE2_MSG = 0`, `PEER_INFO = 1` (`pairing.proto`, and the
enum use sites in `DoExchangeMsgs` / `DoExchangePeerInfo`).

### 5. HKDF-SHA256 → AES-128-GCM with a counter nonce

`pairing_auth/aes_128_gcm.cpp`:

```
static constexpr size_t kHkdfKeyLength = 16;
uint8_t info[] = "adb pairing_auth aes-128-gcm key";
HKDF(key, sizeof(key), EVP_sha256(), key_material, key_material_len,
     nullptr, 0, info, sizeof(info) - 1)
```

So: HKDF-SHA256, **no salt**, info is the 32-byte string **without** its NUL
(`sizeof(info) - 1`), output 16 bytes. `EVP_AEAD_CTX_init` then uses
`EVP_aead_aes_128_gcm()` with `EVP_AEAD_DEFAULT_TAG_LENGTH` (16), so a record is
`plaintext + 16`.

Nonce layout, from `Encrypt`/`Decrypt`:

```
std::vector<uint8_t> nonce(EVP_AEAD_nonce_length(...), 0);   // 12 zero bytes
memcpy(nonce.data(), &enc_sequence_, sizeof(enc_sequence_)); // uint64_t, native endian
++enc_sequence_;
```

`enc_sequence_` and `dec_sequence_` are `uint64_t` (`aes_128_gcm.h`) and Android is
little-endian, so the nonce is **the counter little-endian in the first 8 bytes, the last 4
zero**, with separate counters per direction, each starting at 0.

### 6. `PeerInfo`

`pairing_connection/include/adb/pairing/pairing_connection.h`:

```
const uint32_t kMaxPeerInfoSize = 8192;
struct PeerInfo { uint8_t type; uint8_t data[kMaxPeerInfoSize - 1]; } __attribute__((packed));
static_assert(sizeof(PeerInfo) == kMaxPeerInfoSize);
enum PeerInfoType : uint8_t { ADB_RSA_PUB_KEY = 0, ADB_DEVICE_GUID = 1 };
```

`DoExchangePeerInfo` memcpys the **whole** struct — all 8192 bytes, never truncated — encrypts
it, and rejects a decrypted peer record whose size is not `sizeof(PeerInfo)`. So the record on
the wire is 8192 + 16 = **8208** bytes in each direction.

`client/adb_wifi.cpp` fills ours:

```
PeerInfo system_info = {};                     // zero-filled
system_info.type = ADB_RSA_PUB_KEY;
std::string public_key = adb_auth_get_userkey();
CHECK_LE(public_key.size(), sizeof(system_info.data) - 1);   // room for the NUL
memcpy(system_info.data, public_key.data(), public_key.size());
```

`adb_auth_get_userkey()` → `pubkey_from_privkey` → `adb::crypto::CalculatePublicKey`
(`crypto/rsa_2048_key.cpp`), which is `base64(android_pubkey_encode(...))` then `" "` then
`login@hostname`. The struct is zero-initialised, so `data` is NUL-terminated.

This is byte-for-byte the format the vendored crate already produces for `A_AUTH`
`ADB_AUTH_RSAPUBLICKEY`: `ADBRsaKey::android_pubkey_encode()` emits `<base64> adb_client@<ver>`.
We reuse it unchanged rather than adding a second encoder.

The device answers `ADB_DEVICE_GUID` with the `adb-<guid>` mDNS service name; we log it and
otherwise only require that it decrypts, which is what proves the codes matched.

### 7. Why a plain connect works afterwards

`daemon/auth.cpp` builds the TLS client-CA list by, for each key the device trusts, splitting
the stored string on space/tab, base64-decoding `split[0]`, requiring exactly
`ANDROID_PUBKEY_ENCODED_SIZE` bytes, `android_pubkey_decode`-ing it to an `RSA*`, and hashing
`i2d_RSA_PUBKEY` with SHA-256 into a CA issuer name. The follow-up `_adb-tls-connect` handshake
is accepted when the client certificate carries a public key with one of those fingerprints —
never mind the certificate itself. That is exactly why the pairing certificate and the connect
certificate must be derived from the same persisted RSA key, and why pairing alone is enough to
skip the *Allow debugging* prompt.

## Shape of the implementation

```
adb_client::tcp::pair(addr: SocketAddr, code: &str, private_key_path: &Path)
    -> Result<PairingOutcome, PairingError>
```

`WirelessAdb::pair` normalizes the code (strips spaces and dashes, requires exactly six
digits — a malformed code is rejected locally rather than burning the single guess the device
allows per displayed code), ensures the persisted RSA key exists, and runs `pair` on
`spawn_blocking`. Nothing else in `WirelessAdb` changed; pairing does not create or replace a
connection, so the frontend still calls `connect` afterwards exactly as before.

Timeouts: a 5-second TCP connect bound, then a single 30-second deadline for the whole
exchange, re-armed onto the socket's read and write timeouts before **every** syscall
(`read_exact_by_deadline`). A peer that trickles bytes or goes silent hits the deadline instead
of pinning the thread.

Error mapping (`pair_error_message`), one distinct next step each:

| `PairingError` | Message |
| --- | --- |
| `CodeMismatch` (AEAD open failed, or an EOF while waiting for the device's `PeerInfo` — a device that cannot open our record just closes the socket; an EOF earlier than that stays an `Io` error) | "The code didn't match. Check the 6 digits on the TV and try again." |
| `Connect` | "Pairing isn't open on the TV. Open Wireless debugging › Pair device with pairing code and try again." |
| `Timeout` | "The TV stopped responding while pairing…" |
| `Tls` / `Protocol` / `Io` | "Couldn't finish pairing with the TV. Reopen Pair device with pairing code…" |
| `Key` | "Couldn't use this phone's ADB key: …" |

A wrong code cannot be detected any earlier than the AEAD step: SPAKE2 always yields *some*
key material, and the mismatch only shows up when the peer's `PeerInfo` will not open. That is
the property that limits an attacker to one password guess per connection, and it is why
`PairingCipher::decrypt` maps an AEAD failure straight to `CodeMismatch`.

## What the tests prove, and what they do not

`cd vendor/adb_client && cargo test --lib` (53 tests, all green):

**Proven:**

- *Packet framing.* `PairingPacketHeader` encodes `[1][type][payload BE]`, round-trips, and
  rejects a bad version, an unknown type, a zero payload, and a payload over `kMaxPayloadSize`.
- *`PeerInfo` encoding.* Always 8192 bytes, type byte first, zero-padded, NUL-terminated,
  parses back, rejects an oversized payload and a short buffer. A separate test generates a real
  `ADBRsaKey`, encodes it, and checks the base64 up to the first space decodes to exactly 524
  bytes (`ANDROID_PUBKEY_ENCODED_SIZE`) — the check `daemon/auth.cpp` applies.
- *HKDF.* `derive_key` matches vectors computed independently in Python (`hmac`/`hashlib`
  HKDF-Extract with a zero salt, then one Expand block), and the info string is 32 bytes.
- *Nonce and AEAD framing.* The nonce is the counter little-endian in bytes 0..8; records grow
  by exactly 16; the two directions advance independently; replaying a record fails; and a
  record produced by `PairingCipher` is byte-identical to one built by hand from
  `Aes128Gcm::new(derive_key(..))` with `sequence_nonce(n)`.
- *SPAKE2 arithmetic against an independent model.* `known_answer_vector_matches_the_boringssl_algorithm`
  pins the ephemeral scalars, the password scalar, both 32-byte messages, and the 64-byte
  shared key against values produced by a separate Python implementation of `spake25519.cc`
  written from the same reading (pure-integer edwards25519, no shared code with the Rust). The
  Python model self-checks by re-deriving `M` and `N` from BoringSSL's seed strings and
  asserting they equal the decimal affine coordinates *and* the hex encodings documented in
  `spake25519.cc` — so the constants are verified against the source, not just copied. The Rust
  test re-derives `M`/`N` from the seeds too, so the hard-coded encodings cannot drift.
- *SPAKE2 ladder.* `mul_wide` agrees with reduced `Scalar` multiplication on the prime-order
  base point (where reduction is harmless), `clear_low_three_bits` yields a multiple of eight
  for all 256 low-byte values, and the shift/add helpers match wrapping 256-bit arithmetic.
- *Loopback of both halves.* `both_halves_of_the_exchange_round_trip` runs Alice against Bob
  through the real framing and the real cipher: SPAKE2 messages are framed and unframed, the
  derived keys agree, and each side's 8192-byte `PeerInfo` is sealed, framed, unframed and
  opened by the other. A companion test gives the two sides *different* codes and asserts the
  failure lands as `PairingError::CodeMismatch` at the AEAD step.
- *Failure surfaces.* A missing key file reports `Key`; a peer that accepts TCP and then speaks
  garbage fails the handshake as `Tls`/`Io` rather than hanging.

**Not proven — and this is the honest gap:**

- **No end-to-end run against a real device.** There is no Android device reachable from this
  environment, so the TLS handshake against `adbd`'s pairing server, the exporter value both
  sides compute, and the device's acceptance of our `PeerInfo` are all unverified in practice.
- **SPAKE2 wire compatibility with BoringSSL is verified against the *algorithm*, not against
  the *binary*.** The KAT pins the Rust against an independent Python model of the same source,
  which catches transcription and arithmetic mistakes but not a *misreading* shared by both.
  Residual risk is concentrated in three places: (a) that
  `x25519_ge_scalarmult_small_precomp` really consumes the unreduced 256-bit scalar the way I
  read it — checked by reading the loop, which walks bits `63..0` of four 64-bit limbs; (b) that
  `password_hash` in the transcript is the full 64 bytes rather than the 32-byte reduced scalar
  — checked in `crypto/curve25519/internal.h`, `uint8_t password_hash[64]`; (c) the
  little-endian `u64` length prefix in `update_with_length_prefix`. I would rate the odds of a
  first-try success as good but not certain.
- **The exporter semantics under rustls** (`context: None` versus BoringSSL's
  `use_context = false`) are argued from both implementations' source, not measured.
- **Pre-existing latent risk, unchanged by this work:** `ADBRsaKey::android_pubkey_encode`
  builds the modulus and `rr` fields with `BigUint::to_bytes_le`, which drops high-order zero
  bytes. If `rr` happens to have a zero top byte (~1 chance in 256 per key) the base64 decodes
  to 523 rather than 524 bytes and *both* pairing and ordinary RSA `A_AUTH` would be rejected by
  the device. Not introduced here and not fixed here (`adb_rsa_key.rs` is outside this change's
  file ownership), but worth a follow-up.

## The manual end-to-end test

Two ways to run it. The Pixel 10 Pro on the LAN is the easier target because its own
wireless-debugging pairing service is the same `adbd` implementation a Google TV runs, and the
code is readable on the phone screen rather than across the room.

### Against the Pixel, from a host binary

1. On the Pixel: **Settings › System › Developer options › Wireless debugging › Pair device
   with pairing code.** Leave that screen open — it advertises `_adb-tls-pairing._tcp` only
   while it is showing, and both the port and the code change every time it is reopened.
2. Note the IP, the **pairing** port (the one on the pairing dialog, not the connect port on
   the Wireless debugging screen), and the six digits.
3. Confirm the service is up and the port matches:
   `dns-sd -B _adb-tls-pairing._tcp` (macOS), or `adb mdns services`.
4. Run the pairing directly, using a *throwaway* key so a failure cannot disturb an existing
   trust relationship:

   ```rust
   // examples/pair.rs in vendor/adb_client, or a scratch bin
   fn main() {
       let args: Vec<String> = std::env::args().collect();
       let addr = args[1].parse().unwrap();          // 192.168.1.42:37099
       let code = &args[2];                          // 642091
       let key = std::path::Path::new(&args[3]);     // /tmp/pairkey.pem  (PKCS#8 PEM RSA-2048)
       println!("{:?}", adb_client::tcp::pair(addr, code, key));
   }
   ```

   Generate the key with `openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:2048 -out
   /tmp/pairkey.pem` (PKCS#8 PEM is what `pair` expects).
5. **Expected:** `Ok(PairingOutcome { peer_info: PeerInfo { kind: 1, data: "adb-…" } })`, and
   the phone shows the paired device. `kind: 1` is `ADB_DEVICE_GUID`.
6. Then prove step 7 above: `ADBTcpDevice::new_with_custom_private_key(<ip>:<connect port>,
   "/tmp/pairkey.pem")` and run `echo ok` — it must succeed with **no** "Allow debugging"
   prompt. That is the whole point of the exercise; pairing that succeeds but does not enable a
   silent connect would mean the `PeerInfo` public key and the TLS certificate key have drifted
   apart.
7. Negative case: reopen the pairing dialog and pass the wrong six digits. Expect
   `Err(CodeMismatch)`, not a hang and not a generic protocol error.
8. Clean up: remove the paired entry on the phone and delete `/tmp/pairkey.pem`.

### Against a Google TV, from the app

1. Build and install the app on the phone (wireless ADB, per `mobile/HANDOFF.md`).
2. On the TV: **Developer options › Wireless debugging › Pair device with pairing code.**
3. In the app's onboarding, scan, pick the TV (the pairing port comes from the
   `_adb-tls-pairing` mDNS record), enter the code, and tap **Pair & connect**.
4. **Expected:** pairing succeeds and the connect that follows needs no on-TV approval.
5. Check `tracing` output for `wireless pairing succeeded` and the logged `peer` GUID.

Until at least the Pixel run has been done, treat code pairing as new-and-unproven — which is
what the onboarding callout now says, alongside the Network-debugging fallback.

## Files changed

- `vendor/adb_client/src/message_devices/tcp/pairing/{mod,packet,spake2,cipher}.rs` — new
- `vendor/adb_client/src/message_devices/tcp/mod.rs` — export `pair`, `PairingError`,
  `PairingOutcome`, `PeerInfo`
- `vendor/adb_client/Cargo.toml` — add `curve25519-dalek`, `subtle`, `sha2`, `hkdf`, `aes-gcm`
- `vendor/adb_client/SHIELD-OPTIMIZER-PATCH.md` — document the addition
- `mobile/src-tauri/src/wireless_adb.rs` — implement `pair`, add `normalize_pairing_code` and
  `pair_error_message` plus their tests
- `mobile/src/screens/Onboarding.svelte` — the pairing-step callout copy only
- `mobile/PAIRING-PLAN.md` — this file

## Validation run

```
cargo fmt --check                                                    # from v2/
cargo clippy -p atv-optimizer-mobile --all-targets -- -D warnings
cargo test -p atv-optimizer-mobile                                   # 21 passed
cargo check -p atv-optimizer-mobile --target aarch64-linux-android    # with the NDK env
cd vendor/adb_client && cargo clippy --lib -- -D warnings && cargo test --lib   # 53 passed
cd mobile && npm run check                                           # 0 errors, 0 warnings
```

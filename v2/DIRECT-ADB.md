# Experimental desktop direct ADB

This is an opt-in development backend, not a replacement for the released desktop transport.
No phone, TV or hardware stability has been verified for this desktop adapter. It does not claim
to fix the routing or pairing reports in issues #235 and #228.

The shared `crates/direct-adb` crate contains the Android app's existing connection, authorization,
TLS/PIN pairing, liveness, shell, transfer and device-service implementation. Android NSD discovery
stays in the mobile command layer. Desktop wraps those sessions in a pool keyed by the exact
normalized transport endpoint; device identity and duplicate-row decisions still belong to the
existing shared profiling code and verified hardware IDs.

## Selection and identity

The normal desktop build and default selection remain subprocess/platform-tools. An experimental
build enables Cargo feature `direct-adb`; selecting it additionally requires
`SHIELD_OPTIMIZER_TRANSPORT=direct`. Invalid selection and a missing feature fail visibly rather
than silently selecting another driver. Explicit fallback is relaunching with
`SHIELD_OPTIMIZER_TRANSPORT=subprocess` (or removing the selection).

This initial stage also requires `SHIELD_OPTIMIZER_DIRECT_KEY` to name an existing app-owned
PKCS#8 PEM ADB identity. It never copies, rewrites or rotates the standard platform-tools `adbkey`,
never creates a desktop identity and never falls back to an ephemeral key if that file disappears.
No identity or environment configuration was created during validation. A future normal UI needs
an explicit identity-creation/authorization flow before this backend is suitable for general use.
Mobile keeps its existing app-owned key path and creation policy.

The vendored dependency already requires Rust 1.88/edition 2024. The new shared crate and mobile
manifest state that requirement honestly. Desktop's optional feature requires 1.88; its normal
subprocess build does not acquire this dependency. No Rust toolchain was installed for this work.

## Parity boundary

| Capability | Direct desktop stage |
| --- | --- |
| Legacy Network debugging, including port 5555 | Reuses mobile RSA AUTH, bounded approval wait and liveness checks; unverified on hardware |
| Android 11+ TLS/PIN | Reuses mobile pairing; pair success does not invent a connection; user supplies separate pairing/connect ports |
| Multiple TVs | One owned session per exact endpoint, maximum 32 in-memory transport slots; operations on established TVs are independent |
| Connect/disconnect transitions | Serialized at the pool boundary; canceled handshakes cannot publish a late row; abandoned attempts do not consume capacity |
| Discovery | Manual numeric IP and port only; bracketed IPv6 accepted. Desktop Scan LAN/mDNS and USB are explicitly unavailable |
| Ready state and Refresh | Probes owned sessions before listing; failed liveness remains an offline row with Forget available; at most four probes run concurrently |
| Curated app actions and profiling | Existing command/engine path and safety checks; shared shell results retain exit status and stderr |
| File push/pull and bounded downloads | Shared sync transfer and streaming size limit, finite inactivity timeouts; no hardware evidence |
| Screenshots and reboot | Shared binary screenshot/reboot services; no hardware evidence |
| APK installer | Desktop `adb install` adapter is unavailable; no silent conversion to shell install |
| Remote | Existing shell-input fallback remains in this backend. Desktop fast Remote is explicitly declined before pushing a jar; forwarding/spawn are unavailable |
| Expert shell and Device Logs | Dedicated shell-v2 connection, 30-second total deadline, 256 KiB retained per stream, partial timeout output and cancellation cleanup; shell-v1-only peers fail without replay |
| Restart ADB / install platform-tools | Explicitly refused in direct mode; neither resets a host server nor silently changes the selected backend |
| Quit | Blocking workers and owned streams hold the same operation counter adopted by AppState; after work drains, only owned direct sockets close |
| Logs/diagnostics | Status identifies direct mode and does not report a discovered host binary; configuration errors remain visible instead of offering an installer |

The mobile session's established per-response/inactivity deadlines remain. Four retry backoffs in
the separate subprocess recovery path total 1.8 seconds, not the whole operation runtime; this
adapter does not broaden retries or automatically reset a shared server.

## Validation and limits

Host tests cover two-TV target isolation, binary bytes, pairing remaining disconnected, cancellation
with a late worker, more than 32 abandoned/unconfirmed attempts, explicit unsupported operations,
required-key behavior, worker/stream leases and shutdown admission. Previously published endpoints
remain visible as offline after liveness fails, so the existing Forget action can release a slot
without hiding the connection or evicting another TV. The capacity regression exercises the shared
Forget command with one healthy TV and offline endpoints; no automatic eviction or reconnect is added. Android host tests compile the
same extracted code; NSD and Android fast Remote remain platform-specific.

Vendor validation includes inspected fake service-framing tests, ephemeral-loopback physical-EOF
tests, bounded-shell streams and a loopback TLS/exporter/SPAKE2/peer-info pairing exchange.
The two pairing peers share this implementation, so success is not Android interoperability
evidence. Legacy AUTH/STLS transport and hardware acceptance remain pending. No test contacts the real host ADB server,
connects to a TV, restarts a server, installs an APK or generates a host ADB identity. Browser status
fixtures are labeled simulation. Cross-platform CI, Android cross-compilation, packaged-app local
network permission behavior and real-device acceptance remain unverified.

The two reported incidents remain distinct: #228 described pairing protocol faults with reportedly
the same Homebrew path in Terminal and the app; #235 included connection routing failures even
before an initial standalone connection, followed by authorization after a manual server reset.
Neither observation proves duplicate binaries, a shared root cause or a resolution by this backend.

## Licensing

The transport continues to use vendored `adb_client` 3.2.2 under its retained MIT license. Original
source, copyright and patch notes remain in `vendor/adb_client`; the desktop bundle resources now
include the same full MIT notice. Shared first-party code is extracted from this repository,
not copied from another transport implementation. The Android notices generator inventories
313 third-party crates and regenerates byte-identical notices after the internal extraction.

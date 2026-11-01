# Native desktop test integration (2026-10-31)

The desktop can use the same Rust transport crate as mobile without changing the
Devices flow. Normal builds still select platform-tools. For an app that starts in
native mode even when opened in Finder:

```sh
CARGO_BUILD_JOBS=1 npm run tauri -- build --debug --features native-adb-default --bundles app
```

Devices shows **Native Rust ADB · direct connection** when active. Add by IP and
Scan LAN connect directly to the TV; Scan LAN combines fresh mDNS endpoints with
the existing port-5555 sweep. Pair PIN still requires the separate connect port.
The private app identity is created once on supported Unix hosts; approve it on
the TV, even if platform-tools was already approved. No standard ADB key is imported
or replaced. A failed native initialization stays visible and never silently
switches drivers. Native mode does not expose Restart ADB or Stop ADB when quitting.

For an explicitly requested diagnostic fallback, launch the executable with
`SHIELD_OPTIMIZER_TRANSPORT=subprocess`. The label changes to **Platform-tools ADB**.
That does not replay any operation. Windows first-run native identity is unsupported
in this integration. Physical TV acceptance, file/install operations, and cross-OS
validation must be completed before making native the shipping default. Native Remote
currently uses compatibility shell input; the fast Remote path remains gated, so
hold-to-repeat and full Unicode input are not yet at parity.

The notes below record the earlier implementation stages; observation-only picker
prototypes are not the integrated Devices UI.

---

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
| Discovery | Explicit bounded DNS-SD observations for legacy ADB, TLS pairing and TLS connect; selection only prefills manual forms. Cached/unusable observations cannot be selected. Interface and numeric IPv6 scope are preserved; no subnet sweep, USB discovery or automatic dialing |
| Ready state and Refresh | Probes owned sessions before listing; failed liveness remains an offline row with Forget available; at most four probes run concurrently |
| Curated app actions and profiling | Existing command/engine path and safety checks; shared shell results retain exit status and stderr |
| File push/pull and bounded downloads | Shared sync transfer and streaming size limit, finite inactivity timeouts; no hardware evidence |
| Screenshots and reboot | Shared binary screenshot/reboot services; no hardware evidence |
| APK installer | Single local APK with optional replacement, streamed through the native package service; requires verified hardware-ID agreement between the current control session and new install connection. Unknown IDs or unsupported shell-v2 identity queries refuse installation. Split APKs remain unavailable; no replay or cross-driver fallback |
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
Native install fixtures also cover exact bytes/flags, response bounds, interruption and
uncertain commit outcomes, plus hardware-ID mismatch refusal before installation starts.
Native discovery tests inject events/providers and exercise cache removal/age, scope,
cancellation and cleanup quarantine without constructing a real multicast daemon.
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

Experimental desktop discovery additionally uses `mdns-sd` 0.19.2 directly. Its four newly
activated packages are optional behind `direct-adb`; the default desktop dependency graph
excludes them. Full MIT notices for `mdns-sd`, `flume`, `if-addrs` and `socket-pktinfo` are
bundled in `resources/native-discovery-MIT-LICENSES.txt`. Android notices remain byte-identical.

Discovery observations are not DNS-freshness, pairing, authorization or physical-identity
proof. The client API does not expose remaining DNS TTL: the 30-second cap measures local
observation age, and old records not re-observed in the current window are disabled. Direct
and unknown backends do not auto-browse at startup; after pairing, an explicit connect port
is required instead of legacy polling or a guessed port. Multicast/OS permission failures
retain the manual IP/port path, without changing permissions or selecting another driver.

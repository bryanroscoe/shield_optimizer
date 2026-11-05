# Built-in (native Rust) desktop ADB

The desktop connects to TVs with the same Rust transport as the Android app
(`crates/direct-adb`, vendored `adb_client`), without Google's `adb` or its server. It is
the default engine in normal builds (Cargo default feature `native-adb-default`);
Settings › ADB › **ADB engine** switches to Google platform-tools at any time. A
platform-tools-only app builds with `--no-default-features`, which CI also tests. A
labeled owner-test bundle (`com.atvoptimizer.native-test`) comes from the PR's opt-in
`native-test-app` CI job.

Devices shows **Native Rust ADB · direct connection** when active. Add by IP and
Scan LAN connect directly to the TV; Scan LAN combines fresh mDNS endpoints with
the existing port-5555 sweep. Pair PIN connects by itself when the TV names itself
while pairing (see **Pairing** below); otherwise it asks for the separate connect port.
The private app identity is created once on supported Unix hosts; approve it on
the TV, even if platform-tools was already approved. No standard ADB key is imported
or replaced. Native mode does not expose Restart ADB or Stop ADB when quitting.

## Choosing the engine

Settings › ADB › **ADB engine** offers **Built-in** and **Google platform-tools**. The
choice is saved to `<data folder>/adb-engine.json` (written to a temporary file, then
renamed over it) and takes effect at once: the app builds the new driver the same way
it does at launch and swaps it in through `AppState::replace_adb`. Connections made
through the previous engine close; reconnect TVs from Devices. Choosing the engine
already running only saves the choice. A missing, unreadable or malformed file counts
as no choice and is logged.

The engine for a launch is decided in this order:

1. `ATV_OPTIMIZER_TRANSPORT`: `direct` is built-in, `subprocess` is platform-tools.
   While it is set, Settings shows the choice disabled and refuses changes. Any other
   value is a configuration error: every ADB call fails with the message and nothing
   falls back.
2. The saved Settings choice.
3. The build default: built-in in builds with feature `native-adb-default` (the
   default), platform-tools in a `--no-default-features` build.

**Fallback.** When the built-in engine is selected but cannot start, because the build
lacks feature `direct-adb` or its identity cannot be created or validated, that session
uses platform-tools. The choice stays Built-in and Settings shows the reason with
**Try again**. Fallback happens only while starting an engine: never per command, never
retried in the background, and never by replacing or importing a key or creating one in
the working-directory fallback. A build without the built-in engine disables that
option.

The built-in driver counts its work on the app's own admission counter and leaves it
open when a switch retires the driver, so quit still drains native work and a switch
away from native cannot close admission for the driver that replaces it. Commands:
`adb_engine` returns `{selected, source: env|saved|default, active, fallback_reason,
builtin_available}`; `set_adb_engine(engine: builtin|platform_tools)` saves, switches
and returns the same status. The E2E server reports platform-tools and refuses switching
so the simulator stays in place.

`ATV_OPTIMIZER_TRANSPORT=subprocess` remains the diagnostic override for one launch.
The label changes to **Platform-tools ADB**. Switching does not replay any operation.

## Pairing

The built-in pair keeps the name the TV sends in its pairing `PeerInfo`
(`ADB_DEVICE_GUID`, for example `adb-<serial>-<suffix>`) when it is printable ASCII of at
most 63 bytes, and prints it the way `adb pair` does: `Successfully paired to <address>
[guid=<name>]`. The shared pair command takes the instance from that output under both
engines. A missing or unusable name means no instance, and the app asks for the connect
port straight away.

With an instance, Devices waits up to 45 seconds for that TV's connect service, as it
does with platform-tools. Under built-in, each `mdns services` read is one bounded native
browse (6-second window) through the same discovery manager Scan LAN uses, rendered in
`adb mdns services` format. It lists only services resolved in that window, under the
30-second observation limit, with an endpoint discovery validated (numeric IPv6 scope
kept). Cached, expired, loopback, unscoped link-local and unprintable records are left out.
A browse that saw nothing and reported problems is an error, never an empty list. The
connect service is matched on the serial embedded in the instance, never on the address;
only its advertised port is dialled, and `connect_paired` checks `ro.serialno` before the
TV counts as the paired device. Probes run back to back while a browse is in flight, so a
45-second wait is about seven browses. Pair itself does not browse under built-in, and
Forget does not read mDNS there, because the built-in engine never re-attaches a TV by
itself.

## Limits

Built-in is network-only: a TV connected by USB needs Google platform-tools, and the
connection help on Devices says so. Remote uses shell input under built-in; the fast
native Remote path stays gated, so hold-to-repeat and full Unicode input are not yet at
parity. Each TV approves the built-in engine's own key once; the platform-tools key is
never read, imported or replaced. Physical acceptance is `docs/DEVICE-TEST-SCRIPT.md`
N1–N18; transport-level device results are in `vendor/adb_client/SHIELD-OPTIMIZER-PATCH.md`.

---

# History: the first desktop direct ADB stage (2026-10-25, superseded)

Kept for the reasoning behind the design. Several limits below no longer hold: the app
now creates its own key, split APKs install, the engine is chosen in Settings,
built-in is the default, and Pair PIN connects by itself when the TV names itself.

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
`ATV_OPTIMIZER_TRANSPORT=direct`. Invalid selection and a missing feature fail visibly rather
than silently selecting another driver. Explicit fallback is relaunching with
`ATV_OPTIMIZER_TRANSPORT=subprocess` (or removing the selection).

This initial stage also requires `ATV_OPTIMIZER_DIRECT_KEY` to name an existing app-owned
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

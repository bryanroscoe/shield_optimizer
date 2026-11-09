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
the existing port-5555 sweep. When every sweep probe fails at once (no refusal, no
timeout), the host could not reach its network yet: macOS does that to a new app until
Local Network access is granted, and for about a second after. The scan then sweeps once
more after the 6-second mDNS browse instead of reporting an empty subnet; a quiet subnet
is never swept twice. mDNS endpoints are dialled only after that browse ends. Pair PIN connects by itself when the TV names itself
while pairing (see **Pairing** below); otherwise it asks for the separate connect port.
The private app identity is created once on supported Unix hosts. The built-in engine
offers Google platform-tools' own key first when this computer has one, so a TV that
already allows platform-tools connects without a new prompt (see **Keys**). No ADB key
is imported, copied or replaced. Native mode does not expose Restart ADB or Stop ADB
when quitting.

Nothing the app does by itself puts a prompt on a TV: only **Request approval** on a
TV's row does (see **Approval**).

## Approval

Network debugging (legacy AUTH, port 5555) shows the TV's "Allow debugging?" prompt
when the host answers an AUTH token with its public key (`AUTH_RSAPUBLICKEY`). The
built-in engine sends it from one place only.

- **Quiet by default.** Scan LAN, Add by IP, Retry/Try again and the scan's reconnects
  sign each AUTH token with the known keys and nothing else (vendored
  `ADBTcpDevice::new_with_keys_and_control_signing_only`). When the TV asks again after
  refusing every signature, the connection closes with `KeyNotAllowed` and the TV shows
  nothing. Refresh only probes connections that already exist. A quiet connection is
  bounded at 20 seconds (`QUIET_CONNECT_TIMEOUT`) because it never waits for a person,
  and its failure messages never mention a prompt.
- **Dedicated connections are quiet too.** Downloads, installs, bounded shells (Device
  Logs, Expert shell) and the identity-bound services Remote and screen use open their
  own connections. On desktop they sign only (`WirelessAdb::with_dedicated_connections`,
  `PreferredKeys::signing_only` in the vendored crate). A TV that no longer allows the
  key there (trust revoked, or Allow chosen without "Always allow" and the session since
  reconnected) fails the operation with "This TV no longer allows this computer. In
  Devices, use Request approval on its row, then tick “Always allow from this computer”".
- **Not allowed yet.** Such an endpoint is listed by `adb devices` as `unauthorized`
  under its exact `host:port`, with the time it last refused. It has no verified hardware
  id, so it is never merged with another row and claims nothing about the device. For 5
  minutes after a refusal (`NOT_ALLOWED_RECENT`) it counts as current evidence:
  `scan_connection_progress().not_allowed` reports it, the Devices list may name it by its
  Cast name (`unqueried_names` asks only for those, and for requests whose key reached the
  TV), and a scan skips that service's other routes. After that it is still listed, by
  its address only. It stays until a later connection there succeeds (it becomes a normal
  TV), a scan reaches for it and gets no answer or an error (it moves to Scan details),
  or Forget drops it. A later scan only retries quietly, which succeeds once the TV
  allows the key.
- **Request approval** (`request_device_approval`, `AdbDriver::request_approval`) is the
  only connection that may offer the public key. On Devices it opens a pop-up
  (`ApprovalDialog.svelte`) with the TV's name, a pulsing TV and the instruction; the pop-up
  closes by itself once the TV connects, and closing it never cancels (only Cancel request
  does). Under platform-tools the same row and pop-up ask the adb server to reconnect the TV
  and then watch that one transport (`transport_state`: `adb devices` alone, no property
  reads) until it is authorized, for the same 10 minutes, re-reading the device list only
  once it changes. It keeps that connection open for up to
  10 minutes (`APPROVAL_WAIT` in `src-tauri/src/adb/direct.rs`) for the person to choose
  Allow, and ends sooner on **Cancel** (`cancel_device_approval`), Forget, an engine
  switch or quit. The command returns once the key was written to the TV, and only then
  does the row say to choose Allow (`awaiting_authorization`). The request stays in
  `requesting` until its outcome is recorded (the TV listed, a refusal in `not_allowed`, or a
  failure), so the window never reads the moment between Allow and the TV being listed as a
  refusal; a transport the list folds into another row by hardware id is found through
  `device_profile`, never by its address. If the TV hasn't been
  asked within 20 seconds (`APPROVAL_START_WAIT`: an adbd that accepts the socket but
  never answers, or a handshake that times out first), the request stops and reports a
  connection failure with its reason; it never turns into "not allowed". A wait that ends
  without approval after the TV was asked, including a Deny that closes the connection,
  quietly returns the row to not allowed yet with the button available; it is never
  reported as a transport error. A Cancel or Forget before the TV was asked returns
  `canceled`, which shows nothing. A Cancel that races the person's Allow checks how the
  request ended: a connection that authenticated is published as the TV, anything else
  is closed. Android 14 (the Android TV emulator) does not close the connection on Deny:
  the framework sends adbd `NO` and adbd sends the host nothing, so the row keeps
  waiting until Cancel or the limit, and its help says so. Closing the connection
  (Cancel) does not take the prompt off the TV there either; choosing Allow with "Always
  allow" on that leftover prompt still stores the key, and the next quiet scan connects.
  A scan or Add by IP during the wait does not dial that TV again: a quiet attempt that
  finds the request holding the TV reports it as not allowed at once. Add by IP waits at
  most 20 seconds (`FOREGROUND_LOCK_WAIT`) behind another connection to the same address,
  then says so.
- **TLS** (Wireless debugging after PIN pairing) never shows a prompt, so it connects the
  same way in both modes.
- **Platform-tools** is unchanged: its adb server offers its public key whenever a TV
  refuses its signatures, so under that engine any connection can show the prompt.
- **The Android app** is unchanged: it still offers the key on its own connect
  (`WirelessAdb::connect`, 30 seconds) and on its dedicated connections. Moving it to the
  same quiet model is a follow-up.

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

## Keys

The built-in engine offers two keys, in order: Google platform-tools' own key, when
there is a usable one, then the app's key (`<data folder>/native-adb/identity.pkcs8.pem`).

- **Where.** `.android/adbkey` in the home folder, found exactly as adb finds it
  (`adb_get_homedir_path` in AOSP `adb_utils.cpp`, `get_user_key_path` in
  `client/auth.cpp`): `$HOME` as it is (even empty), or the user database's home when
  `HOME` is unset, on macOS and Linux; the user profile folder on Windows. adb reads no
  other variable for it (`ANDROID_USER_HOME` and `ANDROID_SDK_HOME` do not move it).
- **Read only.** It is read when the built-in engine starts or is switched to, never per
  command, and is never created, written, copied, re-permissioned or logged. It must be a
  regular file of at most 32 KiB holding an unencrypted RSA 2048 key in PKCS#8 (what adb
  writes) or PKCS#1 PEM. Anything else, or no file, is skipped with a debug log and the
  engine starts with the app key alone, which stays required. `ATV_OPTIMIZER_DIRECT_KEY`
  replaces the app key and is then the only key offered.
- **Legacy AUTH (port 5555).** Each AUTH token is signed with the next key, as adb signs
  with each of its keys. Once both are refused, only Request approval offers a public
  key (see **Approval**): the platform-tools key's, so one approval on the TV covers both
  engines.
- **TLS (Wireless debugging).** The client certificate is the key the TV names in its CA
  hints (`O=AdbKey-0, CN=<SHA-256 of the key's SubjectPublicKeyInfo>`, AOSP
  `tls/adb_ca_list.cpp`), else the first key, as adb's `adb_tls_set_certificate` does. A
  refused certificate is tried again once with the other key, on a fresh connection under
  the same deadline.
- **PIN pairing** registers the first key, so platform-tools trusts the pairing too.
- **The other direction.** Under platform-tools every adb command the app runs names the
  app key in `ADB_VENDOR_KEYS`, after any value already set, while that file exists. adb
  loads vendor keys (files, PKCS#8 included, or the `*.adb_key` files of a folder) only
  in the server when it starts (`adb_auth_init`), so this helps only when one of the
  app's commands starts the server, including the restart the macOS server repair makes.
  A server another tool started keeps its keys; nothing restarts one for this.
- The Android app is unchanged: it offers only its own key.

## Limits

Built-in is network-only: a TV connected by USB needs Google platform-tools, and the
connection help on Devices says so. Fast Remote uses the scrcpy control channel over its
own verified connections to the TV, and shell input only if the channel fails before any
input (see `NATIVE-REMOTE-ACCEPTANCE.md`); Android's keymap drops CJK and emoji text on
both engines. A TV that allows neither key approves the platform-tools key once, through Request
approval (the app key when there is no platform-tools key); neither key is ever imported or
replaced. Nothing the built-in engine opens by itself, including the dedicated connections
for downloads, installs, shells and Remote, ever offers a key; a TV that stops allowing it
fails those with a message pointing at Request approval. The TV does not tell the host about
a Deny, so a denied request waits until Cancel or its 10-minute limit. Physical acceptance is `docs/DEVICE-TEST-SCRIPT.md`
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
| Remote | Existing shell-input fallback remains in this backend. Desktop fast Remote is explicitly declined before pushing a jar; forwarding/spawn are unavailable (since 2026-11-06 fast Remote runs natively; see `NATIVE-REMOTE-ACCEPTANCE.md`) |
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

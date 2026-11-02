# Native desktop transport acceptance

This inventory covers the desktop integration on PR #268. The intended destination
is Rust-native network ADB with platform-tools as an explicitly selected backup.
The released default remains platform-tools until these gates have evidence.
This inventory records source capabilities, not TV compatibility certification.

| Area | Current evidence and remaining gate |
| --- | --- |
| Ownership and fallback | Exact-endpoint session pool, operation leases and cancellation have host tests. There is no subprocess fallback inside direct mode. The `native-adb-default` build feature selects native mode for Finder launches; the Devices label identifies the backend. Normal builds still select platform-tools. |
| Identity | Explicit Rust mode now initializes a private app-owned RSA-2048 PKCS#8 identity on Unix first run and validates/reuses it on later runs. Publication is atomic and never overwrites an existing identity; corrupt or unsafe storage fails visibly. Standard ADB keys are never imported or replaced. Packaged macOS identity creation and reuse have been exercised. Windows secure storage and approval of a previously unapproved physical TV remain pending. |
| Legacy AUTH / STLS | Shared mobile implementation is reused. Real loopback RSA challenge/signature/public-key fixtures pass; the packaged macOS build reconnected to two previously approved Shields. STLS interoperability and Windows/Linux packaged hardware acceptance remain pending. |
| TLS/PIN pairing | Loopback tests now cover TLS, exporter-bound SPAKE2 and encrypted peer-info exchange, including wrong-code rejection. Both simulated peers share the implementation; this is not Android interoperability evidence. Refused, silent and slow-trickle peers test the total deadline. Actual TV pairing, reconnect and distinct pair/connect ports remain pending. |
| Discovery | Scan LAN combines the existing 5555 subnet sweep with DNS-SD observations, preserves separate pairing/connect ports and starts bounded connections as endpoints arrive. Unapproved/failed endpoints retain their cards. Unknown aliases are never merged without a verified hardware ID. Packaged macOS LAN discovery has been exercised; other routers/platforms and USB discovery remain unverified or unsupported. |
| Refresh and multiple TVs | Pool isolation, offline retention, Forget and cancellation have host tests. Two Shields reconnected concurrently in the macOS test app; pending authorization and explicit retry were observed. DHCP changes, approval on the TV and sleep/wake recovery remain pending. |
| Bounded shell and Device Logs | The dedicated shell-v2 stream verifies owned/dedicated hardware identity before command OPEN and has a 30-second total deadline and 256 KiB per-stream limit, partial timeout output and cancellation cleanup. It requires shell-v2; unsupported peers fail without replaying the command as shell-v1, which cannot preserve separate stderr/exit semantics. Continuous output, truncation, filters and cleanup have simulated coverage; hardware acceptance remains pending. |
| Transfers | Push/pull on the owned connection reuse sync services. Dedicated limited downloads now verify the owned and dedicated hardware identity before stat or destination creation. Owned sync transfers and limited downloads now have fifteen-minute total deadlines with cancellation and continuous/stalled-I/O fixtures. Permissions and hardware acceptance remain pending. |
| APK installation | Single local APK and replacement choice now use the native streaming package service. Bounded responses preserve rejection codes and uncertain outcomes without replay. Verified hardware IDs from the owned control session and dedicated install connection must agree before install OPEN. Split installs, real package-manager compatibility and hardware acceptance remain pending. |
| Remote | Native helper upload, resident/control streams, generation ownership, held-key release and per-TV isolation are implemented behind a disabled production gate and exercised with injected drivers. Desktop queues now fence hidden/stale generations; uncertain input is never replayed. Startup and I/O are bounded, and only explicit control-service refusal is retried. Physical helper termination, cancellation behavior, text injection and packaged/hardware acceptance remain unverified. |
| Quit and diagnostics | No shared server is stopped by the direct driver. Leases and backend status have host tests. Packaged macOS launch and driver labeling were checked; shutdown during active transfers remains pending. |

At `b9bed05`, all six CI checks passed (Linux/macOS/Windows Rust, desktop and
mobile frontend, and simulated E2E). The local macOS observation authenticated two
Shields in 1.77 seconds, displayed four IPv4 endpoints by the first 5.127-second UI
observation, and completed discovery in 7.30 seconds. These are one-LAN observations,
not performance guarantees. Physical TV approval, TLS pairing, transfers/install,
sleep/wake and release signing still need acceptance. CI compilation does not prove
that first-run native identity works on Windows.

## Consolidated hardening limits

Ordinary owned-connection shell output is capped at 512 KiB per stream, including
lossy UTF-8 expansion; screenshots at 64 MiB plus bounded stderr. Limit failures
are errors rather than successful truncation. Unbounded framebuffer fallback is
refused explicitly, not replayed. Ordinary shell and screenshots now use total budgets including queue wait and
command I/O on the existing owned connection (30 seconds; slow shell 180 seconds). Generic sync and limited
transfers use fifteen minutes; dedicated limited downloads include authentication
in that budget. Reboot acknowledgement uses thirty seconds.
Cancellation closes only the admitted socket. Blocking local filesystem work can
outlive the caller deadline and retains its lease until completion.

Generic dedicated services now use the identity-bound path too. This is an
explicit Android compatibility change: authoritative shell-v2 identity is
required, streams have a fifteen-minute lifetime, and the original two-second response
inactivity budget is preserved for generic services. Android
Rust libraries compile offline for aarch64 Android using the existing API-24
toolchain. Hardware, JNI and packaged behavior remain unverified; this is not
transparent parity certification.

The existing E2E server already installed fixture subnet sweeps. Earlier caution
about the full suite potentially probing the LAN missed that initialization.
The new offline aggregate verifies the provider, refuses absent-provider real
probes, restricts browser origins and classifies scenarios explicitly.

### Scan and readiness integration

The prior Connect/Restart, failed-endpoint retry and Scan readiness candidates are
included in this integration. Native discovery itself only observes advertisements;
the desktop Scan LAN command combines those observations with the legacy subnet sweep
and starts bounded connection jobs. A discovered endpoint is not a verified TV.
Only the canonical profiling path and a reported hardware serial can merge aliases.

Connection cancellation now interrupts the handshake socket, including the interval
between authentication and publication. One 30-second deadline covers the handshake;
a successful publication detaches that deadline from the persistent connection.
Regression fixtures exercise silent peers, pending AUTH, blocked publication and
post-handshake liveness. Hardware approval and sleep/wake remain separate gates.

No candidate or simulated result establishes the cause of issue #235's reported
routing failure, and #228's pairing report remains separate.

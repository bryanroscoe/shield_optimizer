# Native desktop transport acceptance

This local follow-up starts at PR #236 (`812f132`). The intended destination is
Rust-native network ADB with platform-tools as an explicitly selected backup.
The released default remains platform-tools until these gates have evidence.
This inventory records source capabilities, not TV compatibility certification.

| Area | Current evidence and remaining gate |
| --- | --- |
| Ownership and fallback | Exact-endpoint session pool, operation leases and cancellation have host tests. There is no subprocess fallback inside direct mode. Selection currently requires a build feature and environment variables; normal settings UX remains pending. |
| Identity | Desktop requires an existing app-owned PKCS#8 key. No automatic creation, standard-key import or rotation is implemented. Product approval and an explicit identity/authorization flow are required before general availability. |
| Legacy AUTH / STLS | Shared mobile implementation is reused. Full scripted wire acceptance and packaged macOS/Windows/Linux plus TV acceptance remain pending. Pure framing tests do not establish AUTH acceptance. |
| TLS/PIN pairing | Loopback tests now cover TLS, exporter-bound SPAKE2 and encrypted peer-info exchange, including wrong-code rejection. Both simulated peers share the implementation; this is not Android interoperability evidence. Refused, silent and slow-trickle peers test the total deadline. Actual TV pairing, reconnect and distinct pair/connect ports remain pending. |
| Discovery | Explicit native DNS-SD observation supports legacy, pairing and connect services with separate ports and scoped addresses. Selection is manual, stale/unusable rows are disabled, and no physical identity is inferred. No subnet sweep or USB discovery. Multicast/router/permission and cross-platform acceptance remain pending. |
| Refresh and multiple TVs | Pool isolation, offline retention, Forget and cancellation have host tests. Hardware reconnect, DHCP changes and delayed authorization remain unverified. |
| Bounded shell and Device Logs | The dedicated shell-v2 stream verifies owned/dedicated hardware identity before command OPEN and has a 30-second total deadline and 256 KiB per-stream limit, partial timeout output and cancellation cleanup. It requires shell-v2; unsupported peers fail without replaying the command as shell-v1, which cannot preserve separate stderr/exit semantics. Continuous output, truncation, filters and cleanup have simulated coverage; hardware acceptance remains pending. |
| Transfers | Push/pull on the owned connection reuse sync services. Dedicated limited downloads now verify the owned and dedicated hardware identity before stat or destination creation. Truncation, interruption, total transfer deadlines, permissions and hardware acceptance remain pending. |
| APK installation | Single local APK and replacement choice now use the native streaming package service. Bounded responses preserve rejection codes and uncertain outcomes without replay. Verified hardware IDs from the owned control session and dedicated install connection must agree before install OPEN. Split installs, real package-manager compatibility and hardware acceptance remain pending. |
| Remote | Native helper upload, resident/control streams, generation ownership, held-key release and per-TV isolation are implemented behind a disabled production gate and exercised with injected drivers. Desktop queues now fence hidden/stale generations; uncertain input is never replayed. Startup and I/O are bounded, and only explicit control-service refusal is retried. Physical helper termination, cancellation behavior, text injection and packaged/hardware acceptance remain unverified. |
| Quit and diagnostics | No shared server is stopped by the direct driver. Leases and backend status have host tests; packaged app lifecycle acceptance remains pending. |

The local Connect/Restart readiness (`7edaf1d`), failed-endpoint retry (`f57bc84`)
and Scan readiness (`e8f4384`) candidates are separate. They are not part of
PR #236 or this branch's starting point. Neither the native transport nor those
reporting corrections establish the cause of issue #235's real routing failure.
Issue #228's Sony pairing report is a separate unresolved observation.

No real host ADB server, LAN scan, TV, APK installation or persistent identity
creation is needed for the local protocol and fake-driver gates. Those tests do
not substitute for the explicitly pending hardware and packaged-app gates.

## Consolidated hardening limits

Ordinary owned-connection shell output is capped at 512 KiB per stream, including
lossy UTF-8 expansion; screenshots at 64 MiB plus bounded stderr. Limit failures
are errors rather than successful truncation. Unbounded framebuffer fallback is
refused explicitly, not replayed. Ordinary shell/screenshot/limited-transfer
inactivity limits are not total wall-clock deadlines; that remains implementation
work before a primary-driver default change.

Generic dedicated services now use the identity-bound path too. This is an
explicit Android compatibility change: authoritative shell-v2 identity is
required, streams have a fifteen-minute lifetime, and the original two-second response
inactivity budget is preserved for generic services. Android
hardware behavior is unverified; this is not transparent parity certification.

The existing E2E server already installed fixture subnet sweeps. Earlier caution
about the full suite potentially probing the LAN missed that initialization.
The new offline aggregate verifies the provider, refuses absent-provider real
probes, restricts browser origins and classifies scenarios explicitly.

### Scan and readiness overlap

| Path | Relationship to the separate candidates |
| --- | --- |
| Native discovery | Observation only, separate pairing/connect ports and manual selection; it does not run the legacy subnet sweep or automatically connect. |
| Native connection pool | The native driver probes readiness before publishing its own connection; this is not evidence of real routing or authorization stability. |
| Shared platform-tools Connect/Restart | Existing command-result parsing is unchanged here. The separate `7edaf1d` readiness and `f57bc84` failed-endpoint recovery candidates remain unmerged into this stack. |
| Legacy Scan | Text-based connect classification and its diagnostic limitations remain separate; `e8f4384` is the isolated Scan readiness candidate, not included here. |
| Hardware identity | New dedicated streams pin verified identity; observations/related IP addresses are never treated as physical identity or merged on that basis. |

No candidate or simulated result establishes the cause of issue #235's reported
routing failure, and #228's pairing report remains separate. The approved combined
preview and all older candidates are untouched.

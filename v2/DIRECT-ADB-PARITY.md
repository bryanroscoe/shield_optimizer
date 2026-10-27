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
| Discovery | Manual numeric endpoints only. Scan/mDNS and USB remain explicitly unavailable; replacing discovery must preserve unknown identity and unauthorized rows. |
| Refresh and multiple TVs | Pool isolation, offline retention, Forget and cancellation have host tests. Hardware reconnect, DHCP changes and delayed authorization remain unverified. |
| Bounded shell and Device Logs | This follow-up adds a dedicated shell-v2 stream with a 30-second total deadline and 256 KiB per-stream limit, partial timeout output and cancellation cleanup. It requires shell-v2; unsupported peers fail without replaying the command as shell-v1, which cannot preserve separate stderr/exit semantics. Continuous output, truncation, filters and cleanup have simulated coverage; hardware acceptance remains pending. |
| Transfers | Push, pull and limited downloads reuse sync services. Truncation, interruption, permissions and large-file hardware acceptance remain pending. |
| APK installation | Single local APK and replacement choice now use the native streaming package service. Bounded responses preserve rejection codes and uncertain outcomes without replay. Verified hardware IDs from the owned control session and dedicated install connection must agree before install OPEN. Split installs, real package-manager compatibility and hardware acceptance remain pending. |
| Remote | Basic shell input is available; fast Remote is declined before jar push. Direct service lifetime, helper launch and control-channel parity remain pending. |
| Quit and diagnostics | No shared server is stopped by the direct driver. Leases and backend status have host tests; packaged app lifecycle acceptance remains pending. |

The local Connect/Restart readiness (`7edaf1d`), failed-endpoint retry (`f57bc84`)
and Scan readiness (`e8f4384`) candidates are separate. They are not part of
PR #236 or this branch's starting point. Neither the native transport nor those
reporting corrections establish the cause of issue #235's real routing failure.
Issue #228's Sony pairing report is a separate unresolved observation.

No real host ADB server, LAN scan, TV, APK installation or persistent identity
creation is needed for the local protocol and fake-driver gates. Those tests do
not substitute for the explicitly pending hardware and packaged-app gates.

# Native discovery acceptance slice

The PR #268 desktop integration retains the existing Devices flow. The shipped
backend remains platform-tools. In native builds, Scan LAN combines a port-5555
subnet sweep with DNS-SD observations and bounded background connection jobs. The
existing first-run Devices scan also uses this path. The DNS-SD provider itself
only observes advertisements; neither path starts a host ADB server or invokes adb.

## Scope

Recognize `_adb._tcp`, `_adb-tls-pairing._tcp` and `_adb-tls-connect._tcp` as
different advertisement kinds. Preserve their exact ports. Discovery does not
prove that a TV is online, paired, authorized or an Android TV. An advertised name,
host address or service instance is not a verified hardware identity and cannot
merge physical-device rows. Actual connected-device profiling remains canonical.

The subnet sweep starts connections before the mDNS observation window finishes.
Fresh usable connect endpoints are admitted to bounded jobs; stale, pairing-only
or unusable advertisements are never dialed. Discovered endpoints appear before
approval, and failures remain visible with retry/Forget. Authorization guidance
requires a successfully sent public-key AUTH packet. After pairing, the app waits for
the connect service of the TV that named itself while pairing (matched on its serial,
never its address) and otherwise asks for the separate connect port. Manual IP/port controls remain usable when multicast
is unavailable; the 5555 sweep covers non-advertising legacy network debugging.

## Cache and scope requirements

- Accept initial resolved/cache records without requiring a preceding Found event.
  Repeated events replace one exact advertisement rather than duplicating it;
  address/port changes remove superseded endpoints. Pair and connect services are
  never collapsed just because they share an address.
- Apply removal/goodbye/expiry notifications while browsing and a bounded local
  observation-age cap. `mdns-sd` 0.19.2's resolved client records do not expose DNS
  remaining TTL; publisher-side TTL defaults must not be presented as that data.
  Cached records not re-observed in the current window remain explicitly cached
  and ineligible for automatic connection. A local age cap is not proof of live DNS freshness.
- Preserve interface metadata and numeric IPv6 link-local scope. Reject unusable,
  missing-scope or ambiguous routes instead of dropping scope or inventing an
  interface. No OS/network permission or routing setting is changed.
- Bound cache size, field lengths, observation window and cleanup. Discovery starts
  lazily; each browse window requests stop/shutdown. Unconfirmed cleanup must not
  be reported as completed or permit another overlapping daemon.

## Evidence and remaining gates

Tests inject advertisements, time, removals and provider failures. They must not
construct a real multicast daemon, scan the LAN, query TVs or change permissions.
Protocol/loopback fixtures are not router, hardware or cross-OS acceptance. The
`b9bed05` packaged macOS app was separately checked on one LAN with two approved
Shields and visible unapproved endpoints. Approval on the TV and other host/router
combinations remain pending. Simulated UI evidence must be labeled as such.

Manual addressing, explicit platform-tools selection and honest failure details
are fallbacks for the user, not automatic cross-driver replay. Native-primary
shipping remains gated on discovery interoperability, packaged-app behavior and
the existing transport/install/identity acceptance requirements.

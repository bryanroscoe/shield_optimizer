# Native discovery acceptance slice

This local stack starts at reviewed native-install head `8dc1d2a`. The shipped
backend remains platform-tools. Native discovery is explicitly requested, uses
DNS-SD directly and never starts an ADB server, shells out to adb or sweeps a
subnet. Entering the native UI alone must not start multicast discovery.

## Scope

Recognize `_adb._tcp`, `_adb-tls-pairing._tcp` and `_adb-tls-connect._tcp` as
different advertisement kinds. Preserve their exact ports. Discovery does not
prove that a TV is online, paired, authorized or an Android TV. An advertised name,
host address or service instance is not a verified hardware identity and cannot
merge physical-device rows. Actual connected-device profiling remains canonical.

Native Scan returns observations for explicit manual selection. Selecting an
address only prepares the existing Connect or Pair form; it never dials or guesses
a PIN. Pairing success still requires the separate connect port. Existing manual
IP/port controls remain usable when multicast is unavailable or observes nothing.
Legacy network debugging may work without advertising any service.

## Cache and scope requirements

- Accept initial resolved/cache records without requiring a preceding Found event.
  Repeated events replace one exact advertisement rather than duplicating it;
  address/port changes remove superseded endpoints. Pair and connect services are
  never collapsed just because they share an address.
- Apply removal/goodbye/expiry notifications while browsing and a bounded local
  observation-age cap. `mdns-sd` 0.19.2's resolved client records do not expose DNS
  remaining TTL; publisher-side TTL defaults must not be presented as that data.
  Cached records not re-observed in the current window remain explicitly cached
  and nonselectable. A local age cap is not proof of live DNS freshness.
- Preserve interface metadata and numeric IPv6 link-local scope. Reject unusable,
  missing-scope or ambiguous routes instead of dropping scope or inventing an
  interface. No OS/network permission or routing setting is changed.
- Bound cache size, field lengths, observation window and cleanup. Discovery starts
  lazily; each browse window requests stop/shutdown. Unconfirmed cleanup must not
  be reported as completed or permit another overlapping daemon.

## Evidence and remaining gates

Tests inject advertisements, time, removals and provider failures. They must not
construct a real multicast daemon, scan the LAN, query TVs or change permissions.
Protocol/loopback fixtures are not macOS permission, router, hardware or cross-OS
acceptance. Simulated UI evidence must be labeled as such.

Manual addressing, explicit platform-tools selection and honest failure details
are fallbacks for the user, not automatic cross-driver replay. Native-primary
shipping remains gated on discovery interoperability, packaged-app behavior and
the existing transport/install/identity acceptance requirements.

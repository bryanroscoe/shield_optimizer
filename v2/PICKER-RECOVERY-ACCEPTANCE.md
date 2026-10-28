# Picker and Home recovery candidate

This local candidate combines the mobile-style native discovery picker with a
visible Report a bug action and conservative Home recovery. It has not shipped.
It builds on the experimental native transport; platform-tools remains the
shipped default and native fast Remote remains gated.

## Home recovery

When a stock-disable attempt ends on Setup Wraith or Settings/FallbackHome after
bounded verification, it attempts to restore the same stock package. It does not
disable the setup helper before confirming a real replacement. Android's valid
Home role confirmation remains sufficient when the resolver lags behind it.
The generic disable and snapshot paths require the original enabled Home
inventory before changing a paired stock/helper combination.

Restoration reads the exact stock package's enabled state independently of the
enable command's acknowledgment. Failed or unreadable recovery remains an error
with Emergency Recovery guidance. A previously disabled helper is not enabled.
If role information is unavailable and the resolver stays stale beyond the
verification window, conservative recovery can undo a genuinely working handoff.
This is a safety tradeoff, not evidence that every affected firmware needs it.

The issue #239 version, firmware and TCL causality are unconfirmed. The local
regressions reproduce controlled states, not that reporter's physical device.
Unknown delivery is covered for takeover and generic disable. Snapshot
`disable_from_plan` still records a transport error without establishing whether
the disable landed; that pre-existing recovery gap is not claimed fixed here.

## Repeatable driver comparison

From `v2`, run:

```
CARGO_BUILD_JOBS=1 cargo test -p shield-optimizer-v2 --features direct-adb adb::direct::comparison
```

The same scenario matrix exercises the actual desktop native adapter with
injected sessions and the actual subprocess adapter with a fake executable:

| Controlled scenario | Expected observation |
| --- | --- |
| Two target addresses | Exact per-TV routing and one command per operation |
| Successful shell | Same stdout, stderr and exit status |
| Nonzero shell, including stdout-only error | Same failure content and exit code |
| Deadline | Both report timeout without implicit replay |
| Bounded nonzero shell | Both preserve partial streams, exit code and termination |
| Binary response | Exact bytes, including non-UTF-8 data |
| Offline target | Error stays visible; transport and process error types remain distinct |

These are adapter-contract simulations. They do not test the native wire
decoder against a real platform-tools executable, AUTH/STLS/PIN, multicast on a
real network, or packaged Windows/Linux behavior. No identity is created and no
TV is contacted by this comparison. A physical comparison must use the same
already-authorized endpoint and read-only operations; modifying actions must
never be replayed automatically across backends.

## UI evidence and acceptance

Delayed/cached advertisements, exact scoped IPv6 selection, separate pairing
ports, repeated actions, expired rows and enlarged text are tested with injected
fixtures. Screenshots and the short picker/report videos are labeled simulation.
An actual Mac window capture proves the embedded app launched, not that discovery
or connection succeeds on hardware. Bryan performs those actions himself.

Real network discovery, TV authorization/connection, firmware-specific Home
recovery and cross-platform packaged acceptance remain separate gates. No
launcher mutation, helper upload, APK installation or server reset is part of
this local launch.

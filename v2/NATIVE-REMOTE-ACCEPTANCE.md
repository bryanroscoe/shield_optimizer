# Native Remote prerequisites

This local stack follows native discovery at `c44f8b7`. Platform-tools remains the
shipped default, and native fast Remote remains refused before helper push. This
is a prerequisite slice, not a claim of working native remote control.

## Input delivery

A channel startup failure before input can still use shell input through the
same selected driver. Explicit compatibility mode skips channel startup. Once a
key or text write is attempted, failure closes the session and reports an unknown
outcome without automatically resending. The warning asks users to inspect the
TV before retrying. A failed flush or key-up cannot prove that nothing arrived.

Injected dispatch tests cover startup, explicit compatibility, successful channel
input and failed partial text/write/flush/key-up outcomes. The simulated browser
shows the warning after one failed gesture and verifies that no automatic second
invocation occurs. These tests do not prove physical delivery or cancellation of
already queued later gestures.

## Dedicated-service prerequisite

The service API binds new dedicated connections to a verified hardware identity
from the currently owned control connection. A new connection must report the
same identity before the requested service is opened. Lifetime and cancellation
are explicit; cancellation interrupts only that dedicated socket and does not
confirm termination of a remote process. Logical session generations must still
be managed by a future Remote orchestrator.

No existing Android raw-service path is silently replaced. No external adb
fallback, connection retry, service replay, key creation or import is introduced.
The local tests use existing in-memory identities and self-owned loopback peers.
Six protocol fixtures cover mismatched/unknown identity, exact bytes, lost OPEN
acknowledgement, cancellation during OPEN or read, total deadline, stream drop
and output bounds. Shared bridge tests cover rejected requests, late worker
results after cancellation and work-lease ownership.

## Remaining enablement and release gates

- Helper launch must use generation-scoped cleanup rather than a broad process
  pattern; cancelled startup cannot publish a late session.
- Stop, reopen, disconnect and quit must release owned streams and retain leases
  until physical cleanup. A stalled TV must not block another TV's registry.
- Held keys need best-effort release without claiming acknowledgement. Current UI
  holds repeat full key presses, and tab hiding does not destroy the component.
- Pending UI sends need target/generation and visibility cancellation. Emoji/IME
  live typing and keyboard D-pad activation still need focused acceptance; UTF-8
  framing tests alone do not prove Android text injection support.
- Real helper/control interoperability, packaged macOS/Windows/Linux behavior,
  physical TV authorization and user-selected identity onboarding remain untested.

The normal gallery has no changed layout. Targeted failure-state screenshots and
video are labeled simulation and retained outside Git. No real ADB server, TV,
LAN scan, permissions or credentials are changed by this work.

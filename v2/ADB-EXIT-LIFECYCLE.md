# Desktop ADB exit lifecycle foundation

This draft is an unwired protocol, not a shipped exit feature. The desktop still leaves its
shared ADB server running. It adds no quit listener, process launch, daemon detection, cleanup
call, preference-file write, or settings UI. Nothing from external PR168 is used.

`src-tauri/src/exit_lifecycle.rs` models the reviewed safety boundaries. Its tests use fake
drivers/child registrations and threads, never ADB or a TV. They establish controller behavior,
not operating-system process termination or real Tauri lifecycle correctness.

## Policy and prompt

The preference schema contains only version, policy and whether the user remembered the choice.
Missing, malformed, unknown-policy, extra-field and future-version input defaults to unremembered
Always leave running. Encoding is provided; disk persistence is deliberately not wired yet.

Three choices are represented: Auto, Always stop and Always leave running. Auto says shared ADB
ownership cannot be verified and therefore leaves running. A startup message, version, binary
path or PID does not establish exclusive use. AOSP's server-status reports metadata, not an
ownership nonce or a census of other applications. There is no automatic owned-server stop here. The protocol is documented in
[AOSP adb_host.proto](https://android.googlesource.com/platform/packages/modules/adb/+/refs/heads/main/proto/adb_host.proto).

A normal quit first yields a prompt ticket. Repeated requests coalesce. Cancel leaves admission
and preferences untouched; stale callbacks cannot confirm a later prompt. Confirmation commits
the selected policy and closes new-operation admission under one lock. Only an explicit remembered
choice returns a preference to persist. A new app instance restores only this preference, never
old pending operations or a shutdown result. Updater restart latches Restarting, closing admission and invalidating pending prompt, drain and
teardown continuations. Later normal exit events cannot apply a remembered Always stop choice.
If the final stop capability has already been delegated, a distinct RestartAfterStopDelegated
result makes no promise that an external stop can be canceled; no additional stop is authorized.
The real Tauri adapter must serialize this point of no return with updater requests.

## Admission, draining and local teardown

An Operation permit spans the entire accepted logical operation, including waits between ADB
steps and a pending platform-tools install/replacement. Replacements and old driver clones must
use the same Controller. Merely adding a permit around each subprocess is insufficient.

Each child is registered atomically before spawn. A ChildLease outlives the initiating operation
and must be explicitly completed after failed spawn or successful reaping. Dropping a lease does
not assert that the child ended; it blocks final stop. The model relies on a future process adapter
to report these facts truthfully, retain the lease under cancellation, and reap actual children.

After logical operations drain, new ADB launches are rejected while the coordinator performs
local socket closure and child termination/reaping. There is no privileged general-cleanup lane.
The future remote-session adapter must suppress its ordinary Drop-launched ADB calls and must not
use shell pkill or forward removal during committed shutdown: either can auto-start a missing
daemon. Cleanup of device-side state may therefore be incomplete; it must not be claimed as done.

Drain/local-teardown deadline expiry never authorizes a stop. Late work cannot move a failed
attempt back to ready, and choosing leave after failure does not reopen admission. The actual
coordinator must surface the failure and await the user's exit decision. No rollback/resume of
committed shutdown is implemented.

Only Always stop, after drain and local child completion, yields a single-use StopAttempt.
It is a capability for a future bounded final kill-server operation, not an executable or generic
ADB bypass. A reported success is explicitly only the caller's result; it does not prove another
application cannot immediately start a new daemon. Abandonment is a failure, not success; the server state remains unknown even if the user then exits.

## Why this draft stops at the foundation

Safe runtime integration crosses the shared command entry points, driver replacement and the
remote-session child API. Tauri's generated invoke handler returns before asynchronous command
completion, so wrapping that boolean-returning handler is not a logical-operation lifetime guard.
Every ADB-capable logical command needs an audited whole-future adapter; the core/mobile consumers
and direct test callers must remain compatible. The current spawn trait returns a bare Child,
and RemoteInputSession::close/Drop launches additional ADB work. These are not fixed by this module.

Follow-up integration must therefore include all of:

- a complete command/driver admission audit, including installation, restart, timers and old handles;
- actual managed child leases through wait/reap and cancellation, plus local-only remote teardown;
- a narrowly scoped final stop with frozen, verified local server targeting; unknown/nonlocal
  targeting leaves running, without modifying network or security configuration;
- a native preference store, three-choice setting and first-quit prompt, wired to pinned Tauri
  CloseRequested/ExitRequested prevention before final Exit, with updater handling kept separate;
- process-adapter race/timeout tests and real macOS/Windows/Linux close, menu quit, updater and
  shared-client acceptance. Simulated UI video belongs to that runtime/UI follow-up.

Until those adapters are reviewed, this module must not be connected to kill-server or advertised
as implemented exit cleanup. There is no new private ADB daemon, port, key or ownership heuristic.

## Local validation

On the trusted main base b74aee6, `cargo test --offline --locked -p shield-optimizer-v2 --lib`
passes 114 tests, including 26 new controller tests; the existing live-device test remains ignored.
Strict desktop `cargo clippy --offline --locked -p shield-optimizer-v2 --all-targets -- -D warnings`,
workspace formatting and diff checks pass. These are macOS host results, not cross-platform
Tauri event-loop or real child-process adapter acceptance. No product UI changed, so no UI video
or gallery change is claimed.

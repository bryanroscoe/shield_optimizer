# Unwired native exit coordinator contract

This slice stacks on PR176 head `4df768ce1c888e6e39d066d98c9b5ac48fbf54f0`. The coordinator is a private child module of `exit_lifecycle` compiled only under `cfg(test)`. Release and ordinary development applications cannot construct it. There are no new IPC commands, production event hooks, preferences, process launches, updater calls, device calls or stop executors. Existing preview and PR heads are preserved.

The executable contract uses the existing Controller with an injected synchronous preference store and asynchronous local-resource/stop fakes. Proposed restart-drain, stop suppression and unknown-result extensions manipulate the parent model's private state only within this test module; they are not production APIs or approval of those transitions for runtime activation. A future independently reviewed controller API must replace these test-only bridges.

## Ordering

Confirmation runs validate → persist (only if remembered) → controller commitment under one coordinator lock in an owned blocking worker. Canceling the IPC waiter drops its join handle, not that worker: persistence and commitment finish together, and later observers read the resulting state. A store error leaves prompt/admission unchanged. Updater intent serializes with this transaction but can acquire the lock throughout subsequent asynchronous drain/teardown. A genuinely hung filesystem write remains an adapter/recovery limitation; a timeout must not falsely report unchanged preferences.

Commitment closes new logical-operation admission. Already-admitted operations can register their later children while Draining. Only after logical operations finish does the coordinator initiate local session closure and reap resident children. Local cleanup is independently owned and survives the deadline waiter; it has no ADB launch capability. Failure/deadline never authorizes a stop or reopens admission. Exit without another stop attempt is an explicit action, not automatic success or Continue working.

Updater intent is a stop veto, not early Restarting. During a slow download it preserves accepted work. Restart commitment enters Draining with Leave selected and closes new admission while preserving accepted permits. If coordination is already running, a second restart commitment is rejected while the veto remains latched. An intent arriving during drain/teardown suppresses undelegated stop. After capability delegation, it cannot promise cancellation or authorize another attempt. Failed/paused updates advertise Exit or clean restart; there is no automatic resume. Actual updater download/install/restart ordering is not wired here.

The final-stop fake receives at most one capability. Success, observed nonzero-command failure and unknown outcome remain distinct. A nonzero command does not prove the daemon was untouched; timeout does not become success because local child leases were reaped. Auto always leaves running. The supported-target boolean is an injected test decision, NOT implemented endpoint verification.

Final-exit authorization matches a session-scoped, single-use token carried by the coordinator's own programmatic-event abstraction. An unrelated window/menu event cannot consume it. This does NOT prove that a real Tauri RunEvent carries such a token: the native adapter must establish correlation without a global "allow next exit" boolean before activation. Existing macOS window-close semantics remain unchanged.

## Hermetic checks and limits

Run from `v2/`:

```
cargo test --offline --locked -p shield-optimizer-v2 --lib exit_lifecycle::coordinator
cargo test --offline --locked -p shield-optimizer-core -p shield-optimizer-v2 --features shield-optimizer-v2/e2e --lib
cargo clippy --offline --locked -p shield-optimizer-core -p shield-optimizer-v2 --all-targets --features shield-optimizer-v2/e2e -- -D warnings
cargo fmt --all --check
```

The tests use in-memory storage, blocking/async synchronization, controller child leases and injected fake results. They do not prove disk durability, OS child termination, a native menu/window event loop, actual updater behavior or daemon state. No UI changed, so no new visual evidence is claimed.

Runtime release gates remain: native event correlation and existing close semantics on macOS/Windows/Linux; updater installer-driven exits and recovery; real-store replacement/durability; full local-resource registry/reaping; and endpoint/executable freezing shared with ordinary drivers. Unknown/custom unsupported targeting must skip stop. Sanitizing only the final command to localhost is insufficient when ordinary operations used a custom endpoint. Exact argv/environment and process/reaper behavior need their own review and separately authorized acceptance. No merge, tag, release or PR168 execution is part of this slice.

Local macOS validation for this slice: all 16 coordinator contracts passed within the complete backend run (455 core tests and 159 desktop tests passed; one live-device test ignored). Strict Clippy with the E2E feature, formatting and diff checks passed. Frontend files are unchanged; no new browser or native-window acceptance is claimed. Cross-platform CI and independent review remain separate checks.

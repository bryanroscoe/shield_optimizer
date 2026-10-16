# Desktop process-lifetime integration — shutdown activation disabled

This is the stacked adapter follow-up to the unwired exit controller at 2eb5fef. It does not
implement the user's setting yet. Normal close/menu quit/updater behavior is unchanged. There is
no new final kill-server executor, quit listener, private daemon/port, ownership detection,
preference persistence or settings UI. DriverFactory's controller is private; its accessor used
by tests is compiled only under cfg(test).

## Actual integration

Desktop startup and platform-tools replacement use one managed DriverFactory. Its Controller is
shared by both factory clones and all old/replacement subprocess drivers. The standalone
SubprocessAdb constructor remains useful for isolated callers/tests; production construction is
through the shared factory. No PID, executable output, path or version is treated as ownership.

AppState::with_adb_operation holds a scoped driver through a whole async command future. Nested
commands on the same AppState reuse that scope, including awaits between subprocesses. The task
local is keyed to the borrowed AppState identity; a different state cannot borrow its driver.
Tasks spawned separately do not implicitly inherit this scope. Existing desktop command bodies
use same-future joins, while asynchronous remote Drop cleanup deliberately obtains fresh admission.

The source-based dispatch audit classifies all **88** current desktop entries (the earlier count
of 89 accidentally included the generate_handler macro's namespace). **77** have whole-future
adapters; **11** are explicitly non-ADB. Additional public reusable implementations are guarded
so direct native/test callers get the same scope. Snapshot/driver-use helpers retain the admitted
handle rather than accidentally reacquiring admission between steps.

| Dispatch group | Guarded / total |
| --- | --- |
| devices | 9 / 9 |
| scan | 1 / 2 |
| health | 5 / 5 |
| install | 3 / 3 |
| launcher | 8 / 8 |
| apps | 18 / 20 |
| screenshot | 1 / 1 |
| input | 3 / 3 |
| sideload | 2 / 3 |
| backup | 2 / 2 |
| files | 6 / 6 |
| snapshot | 6 / 6 |
| recovery, reboot, shell, logcat | 4 / 4 |
| tuning | 6 / 6 |
| optimize | 2 / 2 |
| update, app_report | 0 / 2 |
| diagnostics | 1 / 6 |

The exemptions are local_address_for (host network routing), safety_info/process_safety_info
(catalog rules), list_apks_in_folder (host files), check_for_update (HTTP metadata), save_app_report
(host file), and the five diagnostic logging/path/UI-event commands. The committed audit checks
the actual invoke table and each guarded function's adapter; a new unclassified command fails.
This is an omission detector, not a semantic proof that future edits to an exempt body remain safe.

## Child ownership and cancellation

All subprocess paths in the real desktop driver use AdbChild: ordinary raw/shell, transfers,
raw bytes, bounded shell and resident spawn. Registration precedes spawn. Spawn failure completes
only the registration; successful completion requires an actual successful wait/try_wait. Caller
cancellation requests local termination and transfers both Child and completion lease to a Tokio
reaper. The lease is not released merely because the caller or a process-output future disappears.
A failed wait or missing runtime cannot claim successful completion; an unreaped lease leaves
controller completion unknown and prevents stop eligibility.

The bounded-shell collector retains its existing output/timeout limits. Other output readers
retain their caller timeout even when descendants inherit open pipes. This tracks the local
client only: daemon/grandchild ownership is deliberately not inferred or controlled.

RemoteInputSession holds a detached driver, so a resident session does not hold the initiating
logical operation open forever; its managed child lease remains outstanding. Explicit ordinary
cleanup uses the caller's accepted driver and disarms Drop only after forward-removal was
attempted. Cancellation while Running preserves the best-effort fallback, which uses a detached
driver and fresh admission rather than retaining the canceled operation's permit. A separate desktop
close_local path disarms generic cleanup synchronously (even before its future is polled), closes
the socket and terminates/reaps the local child without shell pkill or forward removal. Background
Drop checks admission, and actual spawning checks again atomically:
queued late cleanup cannot bypass a committed gate. No broad privileged cleanup lane exists.

Platform-tools installation holds a factory operation before download/replacement. Its blocking
ZIP worker owns an Arc of that lease, so canceling the async waiter does not make a still-running
extraction disappear from the drain. Tests inject an installer; validation never invokes the real
download or writes the platform-tools install directory.

## Evidence and remaining gates

On macOS, the core suite passes 455 tests and the desktop suite with the e2e feature passes 131,
with its existing live-device test ignored. The desktop results include 16 new adapter tests;
core adds eight child/remote-session tests. Native fixtures use an explicitly supplied inert shell
executable, not binary discovery or a real ADB. Local fake control sockets are ephemeral test
pairs, not an ADB server. Mobile host compatibility tests pass 26; plugin host compilation passes.
The installed ARM64 Android target also passes offline cargo check with NDK API 24. This is not an
APK build, signing test or phone/TV validation.

Tests cover every finite driver path, output limits, timeout, pre-poll cancellation, caller abort,
spawn failure, inherited pipes, resident-child wait/Drop, old clones/replacement, nested operation
steps, pending/aborted installation and blocking extraction after cancellation. A mock-Tauri test
runs the actual connect_device IPC handler and keeps its operation outstanding through the async
response; the existing E2E CI job now runs these feature-enabled process/IPC tests too. Remote tests execute real local socket/child teardown and assert no cleanup calls, or
exactly the ordinary caller cleanup once. Cancellation tests cover Running fallback, gate closure
before Drop and gate closure while fallback work is queued. Failed-wait notification is tested with an injected
error at the shared notification branch; an actual OS wait failure is not reproduced.

Destructive activation remains inaccessible. Before adding a real Stop executor, independently
review the adapters and verify real macOS/Windows/Linux cancellation/reaping, updater/quit event
ordering and shared-client behavior. Unix fixture-process tests do not establish Windows process
acceptance; that platform remains a release gate. If a Tokio runtime is already shutting down,
reaping can become unknown. Generic Tokio filesystem operations other than the leased extraction
worker can finish after caller cancellation; they cannot launch ADB, but this slice does not claim
all host filesystem effects have been drained. Host route/ip subprocesses also remain outside the
ADB-client registry; they cannot start ADB. This slice does not claim every host process is tracked.

The next UI/native-event slice still needs a native preference store, three-choice settings,
first-quit prompt, event coalescing and pinned Tauri request/prevention flow. It must await logical
operations and local children, retain timeout failure states and suppress ordinary cleanup before
any final stop. No automatic owned-server cleanup or exclusive-use guarantee is enabled here.

Validation commands (from v2; all live-device tests stay ignored):

```sh
cargo test --offline --locked -p shield-optimizer-core -p shield-optimizer-v2 --features shield-optimizer-v2/e2e --lib
cargo clippy --offline --locked -p shield-optimizer-core -p shield-optimizer-v2 --all-targets --features shield-optimizer-v2/e2e -- -D warnings
cargo test --offline --locked -p atv-optimizer-mobile -p tauri-plugin-atv-adb --lib
cargo fmt --all -- --check
```

ARM64 compatibility used `cargo check --offline --locked -p atv-optimizer-mobile --target
aarch64-linux-android` with the installed NDK 28.2/API-24 compiler environment; no SDK component
installation or Android signing task was invoked.

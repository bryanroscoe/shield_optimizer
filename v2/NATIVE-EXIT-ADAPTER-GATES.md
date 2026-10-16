# Native exit adapters and pinned framework boundary

This slice stacks on approved PR177 head `10497a0cece9b884149e076a8a7621b22bdb3012`. It extends the existing coordinator's Store and Resources adapters, not a second coordinator. All new code remains under the parent's `cfg(test)` gate. No production event hook, real stop, updater installation, process exit or restart is enabled. Existing macOS window-close behavior is untouched.

The diagnostic regression drops a registered child specifically during ClosingLocal. The controller has already latched Failed(UnreapedChild), so finish_local_teardown reports WrongPhase. The coordinator now recognizes that exact state as ChildUnknown instead of returning Stale; it still grants no automatic exit or stop.

## Native adapters exercised

The preference adapter canonicalizes a native-supplied data directory and uses only `exit-settings-v1.json`, separate from the preview store. Reads are limited to 4,097 bytes with a 4,096-byte acceptance limit; invalid schema/version/policy or unreadable input returns unremembered Leave with a warning and does not overwrite input. Writes use a same-directory NamedTempFile, file sync, then replacement through the existing owned validate/persist/commit transaction. Existing non-file/symlink destinations are rejected. This is not protection against a concurrent hostile filesystem actor, nor proof of power-loss durability/directory fsync across platforms. No web-provided path is accepted. Those remain real-store release gates.

The native resource registry accepts at most 64 local-close futures and seals registration atomically at teardown. Its remote-session entry point uses the existing core RemoteInputSession::close_local API, which synchronously disables ADB cleanup before returning its future. All closes start in independently owned tasks before their completions are awaited. The coordinator retains ownership across waiter cancellation/deadline and verifies child completion before stop delegation. The new integration fixture uses a real loopback TCP pair and controller lease; it does not construct a real remote-input session or launch a process. Actual session behavior is covered by the pre-existing core local-close fixture tests, not newly claimed here.

An E2E-feature-gated pinned Tauri MockRuntime test invokes public cleanup_before_exit and verifies a managed Resource is dropped, without invoking exit, restart or any Tauri API afterward. This proves managed-resource cleanup in the mock, not real native-window delivery or full OS shutdown.

## Exact framework limitations

Cargo.lock pins tauri and tauri-runtime-wry 2.11.2, tauri-plugin-updater 2.10.1, and tauri-plugin-process 2.3.1. The local cached primary sources establish:

- `tauri-2.11.2/src/app.rs`, ExitRequestApi::prevent_exit: prevention is ignored for RESTART_EXIT_CODE. AppHandle::exit requests runtime exit but falls back to cleanup plus std::process::exit on delivery failure. Neither ExitRequested nor Exit exposes a coordinator nonce.
- `tauri-runtime-wry-2.11.2/src/lib.rs`, Message::RequestExit: delivers only `code: Some(code)`; last-window destruction delivers `code: None`. This does not justify consuming whichever exit event arrives next. A marker exit code would also become the process status with the current Builder::run path.
- `tauri-2.11.2/src/test/mock_runtime.rs`, RuntimeHandle::request_exit: unimplemented. Calling AppHandle::exit on this mock cannot establish real correlation safely; no such call is made.
- `tauri-2.11.2/src/app.rs`, cleanup_before_exit: public API; requires immediate process exit and no subsequent Tauri calls. run_return is public but restart still exits/relaunches without returning, so it is not a universal restart-interception remedy.
- `tauri-plugin-updater-2.10.1/src/updater.rs`, download_and_install: downloads then calls install synchronously. The Windows installer invokes a synchronous on_before_exit hook and then std::process::exit(0). A late ExitRequested listener cannot supply asynchronous drain/teardown there. The default UpdaterExt builder already uses on_before_exit for cleanup_before_exit; replacing that hook must preserve the cleanup.

## Supported alternative requiring review before activation

Use coordinator-owned final delivery: consume the exact correlated token in the owned native executor, call public cleanup_before_exit, then immediately perform the injected process-exit effect with status zero. This avoids asking an untagged ExitRequested event to carry a token. It bypasses ordinary final run-event delivery, so plugin/application exit callbacks must be audited, and real desktop acceptance is still required. No process-exit implementation or event bypass is added in this slice. Do not substitute a global allow-next-exit flag or arbitrary marker exit code.

For updater integration, the supported API is a native-owned split flow: latch stop-suppression intent before starting download; retain accepted operation rights during download; after download, commit restart/drain/local teardown; only then call Update::install. On Windows, that call can exit immediately, so all asynchronous work must already be done. On other platforms, request_restart is deferred until cleanup is complete. Preserve the default updater cleanup hook, or explicitly reproduce it at the reviewed final boundary. Installation must be owned natively rather than a cancelable frontend waiter, and failed/paused update must offer explicit exit or clean restart, not reopen closed admission. The current frontend downloadAndInstall/relaunch path is unchanged pending review of this replacement.

Frozen endpoint/executable selection shared with ordinary ADB drivers, actual stop argv/environment, native exit/menu correlation or replacement semantics, installer behavior, real remote-session registry wiring, crash durability and physical shared-client tests remain unimplemented/release gates. Custom or unknown server targets must fail closed; sanitizing only the final stop to localhost is not valid when normal operations used another endpoint.

Local macOS checks: 25 focused coordinator/adapter checks passed with the E2E feature, including seven new native-adapter checks and the ClosingLocal diagnostic regression. The complete backend run passed 455 core and 168 desktop tests (one live-device test ignored). Strict all-targets Clippy with E2E enabled, formatting and diff checks passed. No real native-window, installer, exit/restart, ADB or TV acceptance is claimed. Platform CI and independent adapter review remain pending.

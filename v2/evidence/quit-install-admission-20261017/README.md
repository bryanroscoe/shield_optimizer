# Installer admission regression evidence — 2026-10-17

Base: merged PR168, `40ba983b76e37891f439d5ce19c8364acdf53698`.
Failing reproduction commit: `2516d7f` (fixture seams and regression tests, before the lease fix).

## Failing before, passing after

[before.txt](before.txt) records three failures against the unchanged admission behavior:

- A pending injected installation allowed mock quit to return `Ran(Stopped)` instead of `OperationsRunning(1)`.
- Canceling the extraction waiter allowed the same mock stop while its blocking worker remained active.
- Installation after admission closed invoked the injected installer instead of returning an error.

The old-snapshot/replacement quit case already passed. The added inherited-pipe and shared process-admission cases also passed before the fix; they preserve additional coverage rather than claim another production bug.

The fix acquires an existing AppState lease before invoking the installer and shares it with the owned blocking extraction worker. The command retains its lease through driver replacement. Cancellation before extraction releases admission, while canceling a live extraction waiter does not release the worker's lease. Closed admission rejects an installation before its installer callback is invoked. No separate controller or driver factory is introduced.

[after.txt](after.txt) records the final feature-enabled library suites: **459 core passed; 114 desktop passed, one live-device test ignored**. This includes the additional cancellation-before-extraction case. [clippy.txt](clippy.txt) records strict all-targets Clippy with E2E enabled. [mobile-host.txt](mobile-host.txt) records **26 mobile host tests passed** and successful plugin library compilation (zero plugin tests). Formatting and `git diff --check` pass.

## Commands

From `v2/`, with a task-owned target directory and the cached dependency registry:

```sh
# At 2516d7f, reproduces the three expected failures without any real process fixtures:
cargo test --offline --locked -p shield-optimizer-v2 --lib commands::quit::tests -- --skip with_a_real_process

# Final source:
cargo test --offline --locked -p shield-optimizer-core -p shield-optimizer-v2 --features shield-optimizer-v2/e2e --lib -- --test-threads=1
cargo clippy --offline --locked -p shield-optimizer-core -p shield-optimizer-v2 --features shield-optimizer-v2/e2e --all-targets -- -D warnings
cargo test --offline --locked -p atv-optimizer-mobile -p tauri-plugin-atv-adb --lib -- --test-threads=1
cargo fmt --all --check
git diff --check
```

The default core/desktop library run and default strict Clippy also passed before the final cancellation case was added: 459 core and 113 desktop tests, one live-device ignore. The final E2E-enabled run above includes that case.

## Fixture and platform limits

No real ADB executable, server, device, download, platform-tools install directory, updater installation, native quit or process-exit effect was exercised. Stop calls use the in-memory StopCounter or explicitly supplied temporary shell stand-ins in existing process tests. The new inherited-pipe fixture uses `/bin/sleep`; its descendant expires naturally after one second. All fixture filesystem effects are temporary.

The first parallel process-suite run hit PID-startup failures in three existing 300 ms fixtures; see [process-initial.txt](process-initial.txt). The two newly added process cases passed. A serial rerun passed all eight process cases ([process-serial.txt](process-serial.txt)); the final complete serial run passed too. No production behavior or old test timeout was changed to hide that startup sensitivity.

Evidence is from macOS host tests. Linux/Windows CI and independent review remain gates; these logs do not establish physical shared-client, updater or Android-device acceptance. Mobile-main integration remains a separate task.

There is no visible UI change, so screenshots/video are not applicable. Historical simulated settings evidence and all six draft PR links are preserved in [ADB-QUIT-HARDENING.md](../../ADB-QUIT-HARDENING.md); those drafts and branches remain intact.

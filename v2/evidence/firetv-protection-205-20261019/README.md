# Issue 205: protected stock-launcher rejection

Main base: `ae0436e4e2809e4fa677542b41d0e09b29532f60`. Current mobile reference when this slice began: `d388cad1e3da9c17e0a5c0bdeec52022fd8d9970` (after PR204). Work is isolated in `fix/firetv-protected-launcher-205`; PR180 and the original mobile checkout are untouched.

## Evidence and scope

[Issue #205](https://github.com/bryanroscoe/shield_optimizer/issues/205) reports a Toshiba AFTHA004 / 55C350LU rejecting both stock disable and precautionary enable for `com.amazon.tv.launcher`, with exit 255, explicit SecurityException/protected-package text, and final Amazon Home readback. [The existing support response](https://github.com/bryanroscoe/shield_optimizer/issues/205#issuecomment-6254437708) requests Optimizer/platform and Fire OS/build details plus redacted diagnostics. No reporter reply was present when inspected. This is reporter evidence, not independently reproduced Fire TV behavior; the issue remains open pending those details.

The README now qualifies firmware/build compatibility, stock-disable and recovery/reversibility promises. Fire TV support is not removed. Recognizing Projectivy does not prove the firmware permits Home takeover, and no bypass or security-setting change is suggested.

The shared command layer adds guidance only when an already-failed command contains both a SecurityException/security-exception marker and an explicit Cannot disable/enable a protected package message naming the exact affected stock package. Exit 255 alone, unrelated permission/transport errors, or a different/prefix-related package do not establish protection. The matcher is generic; no production package list or safety classification changes.

Both stock-disable paths keep their existing guard decisions, shell calls, precautionary restoration, diagnostics and final Home reads. Guidance is prepended to the existing error rather than replacing the raw disable/restore details. Restore failure is not evidence that disable succeeded. Current Home remains its own observed result, including unavailable when unreadable. The action remains a failure and does not offer another takeover based on that failure.

## Final automated validation

- Workspace `cargo fmt --all --check` passes.
- Workspace all-target Clippy with warnings denied passes (`clippy.txt`).
- Complete serial workspace tests pass (`workspace-tests.txt`): core 463, desktop 114 with one preexisting live test ignored, mobile Rust 26, license 2 and registry 9; doc tests pass.
- Four new Rust tests cover narrow evidence matching, structured output and wrapped ADB errors, rejection during disable and/or restore, restore success/failure, exact command order, raw diagnostics, unrelated exit 255, and the already-active-target path. Existing takeover/rollback/Home tests remain passing. An initial added mock fixture used exit 0 with an unrecognized abbreviated error; it was corrected to the reporter's explicit SecurityException marker without changing production behavior.
- Message-rendering fixture passes exact text, separate restore/Home result and overflow checks at 980px, 384px and 384px/200% text (`message-test.txt`).

Rust ran offline using existing dependencies, two jobs, incremental off, and dev/test debug info off. This is macOS host validation, not a new Linux/Windows CI or Android-device pass. Frontend `npm run check`/`npm run build` and the full browser/E2E suite could not run: the existing Svelte/Vite cache is offloaded and its read/copy stalled; no dependencies were installed. The partial task-local cache copy was removed. Exact-lock CI and independent review remain gates.

## Simulated visual evidence and limitations

[`result-simulated.json`](result-simulated.json) is serialized by the mocked Rust regression, not a captured TV response. The [message fixture test](../../tests/launcher-protection-message.mjs) reuses the production message wrapping rule and exact result text in an explicitly labeled HTML fixture. It does not run the Svelte app or emulate native Android behavior.

- [Desktop-width message](message-980-100-simulated.png).
- [Narrow message](message-384-100-simulated.png) and [200% text](message-384-200-simulated.png).
- [Expanded raw diagnostics](raw-diagnostics-simulated.png).
- [Short simulated interaction video](message-simulated.webm).

Screenshots and a sampled video frame were inspected. The test used existing cached Playwright 1.62.1 and cached Chromium headless shell 1223 via explicit paths; no browser was installed. No production frontend markup/style or any state captured in the normal gallery changed, so regenerating those galleries is not applicable. Full-app visual/native acceptance remains unverified.

Reproduce the Rust receipt from `v2/` with `SHIELD_205_EVIDENCE="$PWD/evidence/firetv-protection-205-20261019/result-simulated.json" cargo test -p shield-optimizer-core protected_stock_rejection_preserves_errors_restore_and_home_readback`. With existing project Playwright/browser dependencies, run `LAUNCHER_PROTECTION_EVIDENCE=1 node tests/launcher-protection-message.mjs`; `PLAYWRIGHT_MODULE` and `PLAYWRIGHT_EXECUTABLE_PATH` can select an already available module/browser without installation.

## Main and mobile applicability

The PR targets main, which already includes PR168/200. Both desktop and Android call this shared core launcher code; no desktop-server behavior is introduced on mobile. Mobile's separate merged polish and PR204 are preserved on its own branch. Its launcher production code matches this main base; its extra setup-helper regression remains independent. Integrating this reviewed fix into mobile is a later step, not performed here. No build/release/version/signing or device-security change is included.

No TV/phone queries, app disable/enable/removal on hardware, installations, paid calls, release publication or merge was performed. Real firmware/build details and physical acceptance remain unavailable.

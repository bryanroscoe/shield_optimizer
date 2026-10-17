# Reviewed main integration into mobile

The original integration draft combined mobile `60a1934c5e4c575a81bd124698b9a9f6203e5fe8` with reviewed main `ae0436e4e2809e4fa677542b41d0e09b29532f60`, including PR168 desktop quit behavior and PR200 installer admission hardening. Original contributor commits remain in the ancestry. No release, device installation, signing configuration change, or physical device operation occurred.

The first normal merge is `13973d823c162815b46321442e9b1de636c7858d`, with parents mobile `60a1934` and main `40ba983`. Its only textual conflict was the Unreleased changelog: both branches' entries were retained, and the quit setting was explicitly labeled Desktop. The second normal merge of `ae0436e4` was automatic. Installer and quit production sources match reviewed main exactly. The mobile tracked tree remains unchanged from its parent.

Shared driver compatibility was inspected: leased drivers retain mobile byte-stream forwarding; unsupported spawn remains the default; WirelessAdb inherits the no-server stop default. A core regression checks both state and snapshot stop paths skip dispatch for a driver without a server. Desktop owns the quit hook and setting; Android has no shared-server stop hook.

## Historical local validation

On the combined source, workspace formatting and clippy with warnings denied passed. Full serial workspace tests passed: core 489, desktop 109 (one preexisting live test ignored), mobile Rust 26, license 2, registry 9. Mobile Svelte check had zero errors/warnings, production build passed, and all 326 tests passed. Desktop Svelte check had zero errors/warnings, production build passed, and all 24 frontend test scripts passed. All 34 real-command-layer E2E scenarios passed against SimulatedAdb only; opener/install/update effects are stubbed. No physical devices were queried.

The ARM64 Android debug APK compiled with existing JDK 17, SDK, NDK 28.2.13676358 and offline cached dependencies. APK SHA256: `9688e6a76902bca3bf0bef445693ba3396c341b72906c0c730e56da554c8d139`. The ignored local APK is not published. Upstream Kotlin/Gradle deprecation warnings remain.

Rust used two build jobs, incremental compilation off, and dev/test debug info disabled after the initial parallel build exhausted disk. Only this checkout's generated target cache was removed. Two existing fake-process PID preconditions failed under initial parallel load; the final combined workspace passed unchanged serially, including all eight `adb::driver::tests::process_lifetimes` fixtures recorded in [`rust.txt`](evidence/mobile-main-integration-2026-10-17/rust.txt). No deadlines were relaxed. Vendor tests passed 54 with one ignored after allowing their loopback mock listeners; vendor code is unchanged by the final main merge.

Desktop validation used an existing installed cache copied locally without installing dependencies: Kit 2.61.1 versus locked 2.70.3 and Vite 6.4.2 versus locked 6.4.4. Svelte 5.55.9 and TypeScript 5.6.3 match. Mobile cache versions match its lock. At the time of local validation, exact-lock CI and cross-platform checks remained required; these local results alone did not establish Windows/Linux success. The subsequent exact-head CI receipt below closes that gate without changing the local environment receipt.

Both gallery GIFs were regenerated (16 frames per theme) for the desktop setting. Representative [dark Devices](evidence/mobile-main-integration-2026-10-17/devices-dark.png), [light Devices](evidence/mobile-main-integration-2026-10-17/devices-light.png), and [Logs](evidence/mobile-main-integration-2026-10-17/logs-dark.png) frames use simulated data. PR200 changes no frontend source, so these remain current. [Validation logs](evidence/mobile-main-integration-2026-10-17/) contain the combined checks; inherited PR200 raw evidence is preserved verbatim, including trailing blank lines.

## Subsequent review and merge receipt

The preceding local validation is a historical receipt for integration head `93e9d46bed9935cd60ac84f0f0535fae320f58ea`, not a fresh run on later mobile commits. [Final independent source review](https://github.com/bryanroscoe/shield_optimizer/pull/201#issuecomment-6233713669) approved that exact head; [all six exact-lock/platform CI jobs](https://github.com/bryanroscoe/shield_optimizer/actions/runs/39030576353) passed. [PR201](https://github.com/bryanroscoe/shield_optimizer/pull/201) merged as `d14363bc76c05f2342289d9503e95175b2d44da2`. This closes its review/exact-lock gate while preserving the disclosed local-cache provenance. Documentation reconciliation does not rerun or replace those logs.

## Remaining acceptance gates

Independent review and exact-lock CI were completed for PR201 as linked above. Real phone/TV authentication, reconnect, remote and Android lifecycle checks remain physical-device gates. Desktop opt-in shutdown with real ADB, other clients, updater/restart, custom servers, and Windows/macOS/Linux remains a physical/platform acceptance gate. Simulated tests and a successful debug compile do not prove those behaviors.

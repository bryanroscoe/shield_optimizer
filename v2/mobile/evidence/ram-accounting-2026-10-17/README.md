# RAM accounting regression — 2026-10-17

Originally based on `dccfe99a805ffcb0c11f32a4fa9f227306ba1cf3`; now integrates mobile-parity-pass `5e80c0efd6620060bb03d5c354815990303628ab`, including independently reviewed and merged header PR191. The source-reviewed RAM implementation, tests and fixture are unchanged. Screenshot-retention PR193 remains separate.

The checked-in Shield TV Pro meminfo fixture reports Total 3,016,708K, Used 1,752,149K and Free 1,418,054K. The existing parser converts these to 2946/1711/1384 MB. Before this fix, `memoryUsage` returned null because Used + Free exceeded Total; the new regression failed with that exact result. It now preserves explicit Used and reports 58%, while retaining individual finite/range validation and the available complement only when Used is absent.

[AOSP accounting source](https://android.googlesource.com/platform/frameworks/base/+/master/services/core/java/com/android/server/am/AppProfiler.java): Free includes cached PSS, cached kernel and free memory; Used uses total PSS minus cached PSS plus kernel allocations. Lost separately reconciles swap and ZRAM. These are not a guaranteed complementary two-bucket partition. This does not diagnose the exact vendor-specific discrepancy in the historical capture.

Retained pre-header-integration simulated Playwright screenshots show the saved fixture at 384px, with the existing warning and units unchanged. `shield-ram-large-text.png` uses a 24px root font; existing fixed-pixel text is unaffected. The short MP4 shows the normal reading, enlarged root text and missing-data transition. No physical device, Android accessibility scaling or pinch gesture verification is claimed.

Validation: focused health presentation tests 12/12; mobile Svelte check 0 errors/0 warnings; production build passed. Full mobile suite result recorded in the PR handoff.

The RAM card content and fixtures are unchanged, so its existing visual evidence is retained rather than regenerated. The upstream header has changed: current header pixels are covered by [PR191 evidence](../diagnostics-header-2026-10-17/README.md), and the combined browser suite verifies its integration. Existing RAM screenshots are historical evidence from before that header integration, not new captures of this head.

Integration validation on base `5e80c0e`: all 300 mobile browser tests passed, including header and RAM fidelity regressions; Svelte check reports 0 errors/0 warnings and the production build passed. See [integration receipt](integration-validation.json). No Rust source changed on this RAM branch.

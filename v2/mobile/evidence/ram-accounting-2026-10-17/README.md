# RAM accounting regression — 2026-10-17

Based on mobile-parity-pass `dccfe99a805ffcb0c11f32a4fa9f227306ba1cf3`; separate from header PR191.

The checked-in Shield TV Pro meminfo fixture reports Total 3,016,708K, Used 1,752,149K and Free 1,418,054K. The existing parser converts these to 2946/1711/1384 MB. Before this fix, `memoryUsage` returned null because Used + Free exceeded Total; the new regression failed with that exact result. It now preserves explicit Used and reports 58%, while retaining individual finite/range validation and the available complement only when Used is absent.

[AOSP accounting source](https://android.googlesource.com/platform/frameworks/base/+/master/services/core/java/com/android/server/am/AppProfiler.java): Free includes cached PSS, cached kernel and free memory; Used uses total PSS minus cached PSS plus kernel allocations. Lost separately reconciles swap and ZRAM. These are not a guaranteed complementary two-bucket partition. This does not diagnose the exact vendor-specific discrepancy in the historical capture.

Simulated Playwright screenshots show the saved fixture at 384px, with the existing warning and units unchanged. `shield-ram-large-text.png` uses a 24px root font; existing fixed-pixel text is unaffected. The short MP4 shows the normal reading, enlarged root text and missing-data transition. No physical device, Android accessibility scaling or pinch gesture verification is claimed.

Validation: focused health presentation tests 12/12; mobile Svelte check 0 errors/0 warnings; production build passed. Full mobile suite result recorded in the PR handoff.

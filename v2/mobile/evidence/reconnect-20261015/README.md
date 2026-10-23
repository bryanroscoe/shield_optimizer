# Simulated reconnect safety evidence — 2026-10-15

These Playwright videos use mocked Tauri commands and synthetic TVs. No physical TV was
contacted or mutated. They demonstrate host browser behavior, not Android/native transport.

Base: PR163 `94079cc45c034fac4761f8fcee8dfc5fc01b3961`.

The three original new scenarios fail on the unchanged base (late takeover response survives,
wrong endpoint-cached original is retained, and a rotated port loses the restore target), and
pass with the fixes. Separate checks cover interrupted multi-writes and late list replies after
liveness loss. Existing launcher tests exercise successful actions and repeated switches.

Run from `v2/mobile`:

```sh
npm run check
npm run build
npm test
RECONNECT_VIDEO_DIR=/tmp/shield-reconnect-video node --test --test-concurrency=1 --test-name-pattern='same-endpoint|endpoint reuse|verified TV|between screensaver' tests/launcher.test.mjs tests/tweaks-tab.test.mjs
```

Videos are captured at 384×812, then transcoded from Playwright WebM to H.264 MP4. They are
short automated assertion runs. The test source provides exact state/command assertions.

- [a-reconnect-between-screensaver-writes-prevents-the-second-write](SIMULATED-a-reconnect-between-screensaver-writes-prevents-the-second-write.mp4)
- [endpoint-reuse-never-restores-another-TV-s-screensaver-and-interrupted-writes-stop](SIMULATED-endpoint-reuse-never-restores-another-TV-s-screensaver-and-interrupted-writes-stop.mp4)
- [same-endpoint-reconnect-cancels-confirmations-and-late-launcher-results](SIMULATED-same-endpoint-reconnect-cancels-confirmations-and-late-launcher-results.mp4)
- [verified-TV-retains-originals-across-a-rotated-port-replacement-hardware-does-not](SIMULATED-verified-TV-retains-originals-across-a-rotated-port-replacement-hardware-does-not.mp4)

No separate mobile lint script is configured; Svelte/TypeScript check and diff whitespace checks are used. Rust was unchanged. Desktop galleries are unchanged because no captured desktop screen changed.

Final validation: **217 mobile tests passed, 0 failed**; mobile check reports 0 errors / 0 warnings, build succeeds, and test-file syntax plus diff whitespace checks pass. The unchanged base fails all three original new scenarios. No Rust code changed or local multi-platform Rust validation was performed. Parent SvelteKit generated config is absent in this isolated checkout and Vite prints that existing parent-config warning; mobile svelte-check itself reports no diagnostics.

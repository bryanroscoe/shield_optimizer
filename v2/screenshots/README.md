# Screenshots

`gallery.gif` is the marketing walkthrough used at the top of [`../README.md`](../README.md). It's generated, not hand-captured — regenerate it after any UI change with:

```sh
npm run screenshots    # from v2/
```

That runs two steps:

1. **`screenshots/capture.mjs`** — boots the dev server with `VITE_DEMO=1` (fixture-backed `invoke()`, see [`../src/lib/demo-mock.ts`](../src/lib/demo-mock.ts) — no device needed), then drives a headless Chromium through all ten screens at a fixed retina viewport in **both themes**: dark frames to `frames/`, light frames to `frames-light/` (the light run sets `prefers-color-scheme: light`, which the app's Auto theme follows).
2. **`screenshots/build-gif.sh`** — stitches each frame set into a GIF with ffmpeg (two-pass palette): `frames/` → `gallery.gif`, `frames-light/` → `gallery-light.gif`.

## Requirements

- `npx playwright install chromium` once (Playwright is a devDependency; the browser binary is separate).
- `ffmpeg` on PATH (`brew install ffmpeg`).

## What's captured

Devices list → device Overview → Health → Launcher → App List → Optimize wizard → Tweaks → Install APK → Snapshot → global Snapshots page.

The demo data is a faithful Nvidia Shield — real package names, the real launcher catalog, and the real merged app list ([`../src/lib/demo-apps.json`](../src/lib/demo-apps.json), regenerated from `../data/app-lists/`). It is **not** a real device; it's fixtures so the capture is deterministic and hardware-free.

`frames/` and `frames-light/` are gitignored (regenerable). Only `gallery.gif` and `gallery-light.gif` are committed.

## Automatic regeneration on release

The `refresh-screenshots` job in [`../../.github/workflows/v2-release.yml`](../../.github/workflows/v2-release.yml) reruns this whole pipeline on every `v2-*` tag and commits the refreshed `gallery.gif` + `gallery-light.gif` back to the default branch — so a release never ships stale screenshots. Those captures render on Linux (font stack falls through to Roboto rather than macOS's `-apple-system`), so the release-generated GIF can look subtly different from one you regenerate locally on a Mac. Both are fine; it's the same UI.

## Tuning

- Screens / order: edit the capture sequence in `capture.mjs`.
- Per-screen hold time, output width: `SECONDS_PER` / `WIDTH` in `build-gif.sh`.
- Demo data (device, health, app states, launchers): `../src/lib/demo-mock.ts`.

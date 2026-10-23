# Mobile settings polish — simulated evidence

Branch base: `mobile-parity-pass` at `5f4fc141ceda0307cb86b68bfc50433b9ffcc49d`.

These are synthetic TV replies rendered in Chromium on macOS at 384×812. No phone or TV was connected; no DNS, background limits, display settings, installed applications, or permissions were changed on a real device. Fixtures contain no personal/device data.

- [Settings readback screenshot](settings-simulated.png): 1080p selected for an override of 1920×1080 at density 320; numerically equal animation values; cached cap 32 independently of legacy stored value 2.
- [DNS draft screenshot](dns-draft-simulated.png): opening Custom leaves Automatic selected until confirmed readback changes. The example hostname is reserved `.test` data.
- [Short settings interaction video](settings-simulated.webm): same mock fixture, opening/editing the Custom DNS draft and returning to Automatic.
- [Launcher optical comparison](icon-optical-comparison.png): current adaptive artwork versus a proposed 3dp downward translation inside its 108dp canvas, approximately 1.8 CSS pixels at 64px. Circle, rounded square and squircle are illustrative masks, not actual launcher captures. Existing artwork, safe-zone scale and generated icon assets are unchanged pending visual review.

Reproduce from `v2/mobile` with installed dependencies and Playwright Chromium:

```sh
MOBILE_POLISH_EVIDENCE=1 node --test tests/tweaks-tab.test.mjs
npm test
npm run check
npm run build
```

The opt-in evidence test is in `tests/tweaks-tab.test.mjs`; the ordinary suite does not record video. Raw temporary recordings are ignored; the named video is the reviewed deliverable.

The Android host now applies system-bar and cutout insets as native container padding and removes only those inset types before dispatching to web content. Keyboard insets remain available. This follows [Android's WebView inset handling guidance](https://developer.android.com/develop/ui/views/layout/webapps/understand-window-insets). Browser evidence cannot prove native inset dispatch, keyboard behavior, physical pinch zoom, or Android accessibility scaling. Pinch zoom and text scaling settings have not been disabled or overridden; the Remote gesture pad retains its existing localized touch handling.

The two supplied phone screenshots were not visually inspected: current Library helper materialization was unavailable. Root causes and fixes above are from code and reproducible mock readbacks, not assertions about screenshot pixels. A phone check remains necessary for status/navigation bars, keyboard, rotation, pinch/scroll interaction, accessibility text size, and actual launcher optical preference.

## Validation receipt

- Full mobile browser suite: **229 passed**, zero failures.
- Focused Tweaks suite with evidence: **19 passed**, zero failures.
- Svelte check: **0 errors, 0 warnings** (Vite separately prints the existing missing parent `.svelte-kit/tsconfig.json` notice in a fresh mobile-only worktree).
- Vite production build: passed.
- Native Android `tauri android build --debug --target aarch64 --apk --ci`: passed, local debug APK produced. Installed SDK/NDK/JDK used; offline Cargo dependencies. No APK installation or launch. Existing Gradle deprecation/signing-configuration notices remain; this is not a Play release build.
- `git diff --check`: passed.

Native build command used the existing JDK 17, Android SDK and NDK 28.2.13676358. Phone/TV acceptance is still open as listed above. The first Gradle task inspection could not run before Tauri generated its ignored settings; the supported Tauri build subsequently completed successfully.

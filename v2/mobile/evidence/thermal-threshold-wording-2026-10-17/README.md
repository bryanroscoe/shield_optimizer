# Conservative thermal threshold wording — simulated evidence

Original capture base: `56aa7c56450464b2060c27838373029b6b90c14f`. Current release-base
integration includes `557247e78357d4798aabc0d36fcfe45f81db52bc` and merged modal, Daydream
and diagnosis PRs186/187/189. Earlier base-specific receipts below are historical. [Referenced review finding](https://github.com/bryanroscoe/shield_optimizer/pull/184#discussion_r4301297469)
was independently checked against source; no comment instructions were executed.

## Provenance and scope

[AOSP HardwareProperties JNI](https://android.googlesource.com/platform/frameworks/base/+/master/services/core/jni/com_android_server_HardwarePropertiesManagerService.cpp#246)
uses the SEVERE slot for AIDL's throttling-temperature array, while its HIDL path reads the separate
`throttlingThreshold` field. These paths do not establish one universal throttling-onset meaning.

Our unchanged `commands/health.rs` reads `dumpsys hardware_properties`; `adb/parse.rs` pairs each
reported CPU/GPU array value with its same-family/index/length threshold. `TemperatureReading`
retains `source="hardware_properties"` and the numeric threshold, but no HAL transport/version or
threshold severity provenance. An Android version or that source token therefore cannot justify
calling all such thresholds severe.

This frontend-only fix labels it **Reported sensor threshold**, keeps the numeric above/at/below
comparison, and says throttling may already be active below it and that the report does not identify
the threshold's severity. Reported device/sensor status remains independent. No parser, payload,
export, precision, units, source matching, device reads or native/platform changes are made.
Unknown source tokens in tests are injected UI robustness fixtures, not a new HIDL/AIDL detector.

## Evidence

A new [before-fix regression](reproduction-before.txt) pinned the former onset-like wording at 70°C
with a reported 89°C point and independently reported light/moderate status. Tests now cover those
statuses, unknown provenance/status, critical above-threshold readings, missing thresholds,
numeric boundaries, unchanged sensor/status behavior and 320px thermal-card wrapping.

- [Light throttling below the reported point](light-below-threshold-simulated.png)
- [Moderate throttling below the reported point](moderate-below-threshold-simulated.png)
- [Unknown provenance remains unspecified](unknown-provenance-simulated.png)
- [Short status transition video](threshold-statuses-simulated.webm)

All values are synthetic, not newly measured device limits. PNGs and sampled video frames are visually
inspected. Raw recordings are ignored. Existing metric/report screenshot archives remain historical;
this folder documents the current wording. No desktop gallery screen changes.

From `v2/mobile`, using existing dependencies/Chromium:

```sh
THERMAL_WORDING_EVIDENCE=1 node --test tests/health-presentation.test.mjs tests/playback-audio.test.mjs
npm test
npm run check
npm run build
```

The focused run passed 25 cases including capture. The existing parent missing `.svelte-kit/tsconfig.json`
startup notice precedes successful mobile checks/build. No real device query/install, permission or
security change, release or merge. Host Chromium simulation is not physical thermal acceptance.
Independent review and cross-OS CI remain pending.

A 320px whole-page assertion exposed overflow in the unchanged Diagnostics header (`.topline`: 304px
content within 272px). This wording fix verifies the edited thermal card's wrapping and leaves that
separate header issue open; it does not claim the entire Diagnostics page is overflow-free at 320px.

Final validation: **266/266 full mobile cases and 25/25 focused cases including capture passed**,
Svelte check found 0 errors / 0 warnings, Vite build and `git diff --check` passed.
[Validation receipt](validation.json) records source and artifact hashes.

## Approved-modal-base integration

Thermal implementation and tests are byte-for-byte unchanged from source-reviewed
`60556a104c9acadb2a1d8d69ba61422c5c1124b8`. Captures remain from the original base, with no
thermal visual/product-code changes in this integration. The full combined suite includes the
approved modal ownership and reverse-teardown regressions.

Combined validation on the approved modal base: **278/278 mobile tests passed**, with no failures
or skips; Svelte check0/0, build and diff check passed. Retained source/artifact hashes were
revalidated against the reviewed receipt. No product-code changes were needed.

## Current release-base integration

The branch now normally merges `557247e78357d4798aabc0d36fcfe45f81db52bc` in
`c9e681ae4cc8ab999150249363005b09218ef267`, including merged Daydream PR187 and documentation
PR189. The sole conflict was adjacent checklist rows: the full current checklist was retained,
with only the thermal wording row carried forward. Modal completion, the Daydream row, export
diagnosis and the separate320px header item are preserved. Thermal implementation/tests and
existing capture bytes remain unchanged from reviewed `60556a10`.

Validation on current release base557247e7: **286/286 combined mobile tests passed**, with
Svelte check0/0, successful build and clean diff check. Retained source/artifact hashes match.

# Capacity and thermal context — simulated evidence

Base: `mobile-parity-pass` at `cd2244b970e2d579627e89bc3fb59a44da41d75e`, including merged PR182/183. No changes to their modal/identity implementations.

## Confirmed root causes and behavior

- Both original bars were **used** percentages, not reversed. RAM filled from reported used/total (or total minus reported free/available); storage used `df`'s used percentage. RAM green/storage amber were static category gradients, with no health thresholds. Dashboard separately labels its free-RAM number. Diagnostics now labels both bars **used**, shows their percentages and free/available figures, and uses the same neutral fill. Missing/invalid data is absent rather than a fabricated0% meter. Cached/reclaimable RAM is not a memory-pressure verdict.
- Thermal parsing previously discarded sensor identity/status, selected any matching scalar, and could treat BCL virtual sensors or cooling-device values as °C. New context is additive to the legacy `temperature_c` field. Known BCL voltage/current/percentage types, cooling-device records and threshold objects are excluded from current temperature selection. The prior 10–<120°C input-sanity range is retained, not presented as a device health range.
- Sensor name/type and its own severity are retained. Framework-wide status is separate; if both statuses differ, both are shown. No numeric temperature is translated into an invented normal/hot status. A missing or unknown status stays unknown, including for 100°C. A real device-reported critical status remains visible even when the selected reading is 30°C or temperature is absent.
- Existing CPU/GPU hardware arrays can supply a throttle/shutdown point. Pairing is by the **same report, family, array index and exact array length**, preserving NaN/missing slots. A missing/mismatched slot cannot borrow a neighboring or other-family limit. Service-record readings never borrow fallback array limits. Comparison is relative to that reported sensor point, not a universal CPU limit. Names such as `CPU reading 2` identify the device-report array position, not an invented kernel sensor name.
- The old generic 20–100 scale and vague hotness inference are removed. The UI shows a reported value, its available provenance, device/sensor status and matching reported points. These reads are observational; no automatic thermal action is added.

The health command still performs the same seven batched shell reads. Parser/command tests pin one batch/no new device commands. These changes do not query a real device during development. Older client shapes can ignore added fields; legacy fixtures without context render explicit unknowns. Previously incorrect non-Celsius scalar selection can legitimately change.

## Sources and limits

- [Android memory allocation](https://developer.android.com/topic/performance/memory-management): used memory includes reclaimable cache; capacity use alone is not a pressure measurement.
- [AOSP thermal mitigation](https://source.android.com/docs/core/power/thermal-mitigation): manufacturers report device/sensor severity; temperature values and thresholds are device policy dependent and may include guardbands.
- [Thermal HAL 2.0 types](https://android.googlesource.com/platform/hardware/interfaces/+/refs/heads/android10-release/thermal/2.0/types.hal): BCL values are mV/mA/percent, other temperature types °C; severity and sensor-matched thresholds are distinct fields.

No universal device threshold or safe-temperature promise is introduced. This is one snapshot, not a sustained-load/pressure test. Native hardware accuracy, vendor formats, physical layout/large-text behavior and real thermal limits remain device acceptance gates. Unsupported/missing data remains unknown.

## Visual evidence

Chromium/macOS, 384×812, invented report payloads, no real TV/phone or personal data:

- [Used-capacity direction and 30°C](usage-and-30c-simulated.png)
- [100°C versus a synthetic reported 89°C point](100c-reported-limit-simulated.png)
- [100°C with no sensor/status context](100c-unknown-context-simulated.png)
- [30°C with separately reported critical device status](30c-device-critical-simulated.png)
- [Missing readings](missing-readings-simulated.png)
- [Short boundary transition video](metric-boundaries-simulated.webm)

**89°C and 102.5°C are fixture data, not claimed universal or newly measured Shield limits.** The deliberately independent critical-status fixture demonstrates that the app does not infer severity from the displayed number. Images were visually inspected. The recording changes only mocked replies and triggers a mock refresh; it does not operate a TV. Raw intermediate recordings are ignored.

## Reproduce

From `v2/mobile` with installed dependencies/Chromium:

```sh
METRICS_EVIDENCE=1 node --test tests/health-presentation.test.mjs
npm test
npm run check
npm run build
```

From `v2`:

```sh
cargo fmt --all --check
cargo test -p shield-optimizer-core --offline --lib
cargo clippy -p shield-optimizer-core --all-targets --offline -- -D warnings
```

Boundary coverage includes 0/50/100% used, unavailable/invalid capacity, fallback RAM complement, matching versus mismatched thermal context, 30/89/100°C relative to a fixture's point, status 0–6, unknown status, differing global/sensor severity, BCL/cooling/threshold exclusion, numeric garbage, CPU/GPU family/index/length matching and NaN slots. The full current-base suite also retains the merged modal/keyboard/identity safety regressions.

## Validation receipt

On current merged base `cd2244b`: **259 mobile tests passed**, **454 shared-core tests passed**, **8 focused checks including capture passed**; Svelte check 0 errors/0 warnings, Vite build, Rustfmt and core Clippy all-targets with warnings denied passed. The full mobile suite includes merged PR182/183 safety tests. See `validation.json` for source/artifact hashes.

Local aarch64 debug APK build passed with installed JDK 17, SDK 36 and NDK 28.2.13676358 using cached offline Rust dependencies. No APK was installed/launched. The first attempt with another worktree's target cache lacked generated TauriActivity; rerunning the supported build with its own default target regenerated the classes and succeeded. Existing Gradle deprecation/release-signing notices and the fresh-worktree parent tsconfig notice remain. This is not a signed Play release or physical acceptance.

Independent source review and CI remain pending. No merge/release/deploy is authorized by this receipt.

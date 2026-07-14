# ATV Optimizer Mobile — Architecture Review (2026-07-04)

> Historical review. Most original P0 frontend findings were subsequently fixed. Current open
> correctness work is tracked in [`BACKLOG.md`](BACKLOG.md); do not treat unchecked prose in this
> document as current state without verifying the code.

Full review of the mobile app across three dimensions (transport/core, frontend, build/
distribution/security), each done by a dedicated pass. Findings are consolidated and
prioritized here. Companion to `HANDOFF.md` (state + playbook). Items marked **[device]**
need on-device verification (the test Pixel was unplugged when this was written).

## Guiding principle (from the user)
`crates/core` is **shared with the working desktop app** — mobile must stay aligned with it.
Several mobile-specific edits to shared core are band-aids for a transport problem and should
be reverted once the transport is fixed properly (see T1/T3 below). Keep only genuine
cross-platform improvements.

---

## P0 — Correctness / trust bugs (fix first)

### F1. `reboot_device` called with the wrong arg name — silently never reboots
`screens/Dashboard.svelte:125` passes `{ serial, rebootMode: "normal" }`; the core command
takes `mode` (Tauri maps `rebootMode`→`reboot_mode`, so `mode` is missing → deserialize
error). The UI shows "Reboot command sent!" regardless, then disconnects. **Fix:** `{ serial,
mode: "normal" }` (matches desktop `api.ts`). *Trivial.*

### F2. Failed health load renders as a *healthy* device (worst trust bug)
`Dashboard.svelte` sets `errorHealth` on failure but the template never renders it — it only
branches on `loadingHealth`. So when `health_report` throws, the dashboard shows
`healthScore = 100`, "System optimized", "0 active bloat apps". A total backend failure looks
like a perfectly clean TV. Also `healthScore`/`freedRamEst` are invented client-side
(`100 - activeBloatCount*3`, `activeBloatCount*45`), not backend values. **Fix:** render an
error/retry state on `errorHealth`; derive the score from a real signal or drop the number.

### F3. Diagnostics shows fabricated numbers as live metrics
`Diagnostics.svelte:38-49` hardcodes fallbacks indistinguishable from real data:
`storageTotal ?? "16G"`, `storageUsed ?? "9.8G"`, `used_percent ?? 63`, `tempC ?? 48`,
`audioOutput ?? "Atmos"`, `displayHz ?? "60Hz"`. When `health_report` returns null the screen
confidently shows "9.8G / 16G", "48°C", "Atmos". **Fix:** render `—`/"unavailable"/skeleton
when a field is null; never a plausible fake.

### F4. Optimize screen is 100% mock and discards the real plan
`Optimize.svelte` hardcodes the debloat list + "182 MB" sizes; `loadPlan()` calls
`prepare_optimize` only to swallow the `LOCKED:` error and throws the real `OptimizePlan`
away. Nothing reflects the connected device. **Fix:** show the real read-only plan preview on
Free, or clearly label it "example".

### F5. Diagnostics reimplements safety classification inline — violates the safety invariant
`Diagnostics.svelte:91-134` has its own `getSafetyTag` + package→name map, bypassing core's
audited `safety_info` + known-names loader. This is the "one detection function / do-not-
disable list is mandatory" invariant. **Fix:** call `safety_info`. (Note: the *destructive*
paths are still safe — B6 confirms core gates them — but the displayed tags can disagree with
the engine, which is misleading.)

### T4. Reboot reports a false failure **[device]**
`wireless_adb.rs:130` routes `reboot` over `exec:`; when the device reboots, adbd drops the
socket, the read hits the 8s timeout → `reboot_device` returns `ok:false` even though it
worked. Also `self.connected` still holds the dead device. **Fix:** treat a transport drop on
a `reboot` command as success; clear `self.connected`.

---

## P1 — Architecture

### F6. No shared session store; state prop-drilled as `any`; entitlement never threaded
`App.svelte` holds `host`/`connectPort`/`connectedDevice` and prop-drills a 4-field block into
all 7 screens (typed `any`). No caching (Dashboard + Diagnostics each re-fetch `health_report`).
Entitlement (Free/Pro) is never read on the client — `More.svelte` fakes it by string-matching
`"pro"`. Null-device transition can crash Dashboard (`connectedDevice.serial` with no guard).
**Fix:** a small runes store `src/lib/session.svelte.ts` (connectedDevice, host, entitlement +
connect/disconnect/reconnect actions); fetch entitlement once after connect; guard the null
transition.

### F7. Backend contract drift — inline types + raw `invoke()` duplicate desktop `api.ts`/`types.ts`
Every screen redefines partial types and raw `invoke("string")` calls; results typed `any`.
The desktop `v2/src/lib/{api.ts,types.ts}` are the canonical, "keep in sync with core"
wrappers. The F1 reboot bug is exactly what a typed wrapper catches. **Fix (plan M4):** extract
a shared typed `api`/`types` module (add the `wireless_*` + `find_remote` commands); replace
every inline `invoke`/`any`.

### T1. The global Kotlin mutex is the true root cause of the storage timeout **[device]**
`AdbService.kt` wraps every `shell()` in one global `Mutex`, so `health_report`'s 6 `dumpsys`/
`df` calls (fired in parallel by `tokio::join!`) execute strictly serially, each holding the
lock through its full read-to-EOF. `df` queued behind big dumpsys dumps blew its budget — the
reported regression. libadb's `AdbConnection` already multiplexes independent `AdbStream`s over
one socket, so concurrent `openStream` is supported by design. **Fix:** demote the global mutex
to a lifecycle-only lock (exclusive for connect/pair/disconnect; shared lease for shell/
screencap so they fan out). *Verify concurrent `exec:` streams demux correctly on the Shield.*
The interim band-aid (generous 10s core timeouts + `%`-anchored parse, already committed) keeps
storage working until this lands.

### T2. `exec:` hardcodes `exit_code = 0` — command-failure detection degraded **[device]**
The Kotlin `exec:` path returns `exitCode = 0` always (no stderr either). Any core logic
trusting `success()`/`exit_code` instead of `shell_reported_failure()` reads every mobile
command as succeeding. **Fix:** derive a real exit code (exec: doesn't carry one — either use
`shell,v2:` framing, or append `; echo $?` and parse, or make core rely only on
`shell_reported_failure()` on this transport). *Verify with a known-failing `pm`/`settings`.*

### T3. Revert the mobile band-aids in shared core once T1/T2 land
`health.rs` (per-command timeouts + `/proc/meminfo` fallback + error swallowing) and
`apps.rs::list_other_packages_impl` ("run sequentially to avoid Mutex queuing races" +
timeouts) are compensation for T1. They double the transport's own 8s cap. **Keep**
`parse.rs::parse_storage_info` `%`-anchor (genuine cross-platform robustness, tested). Revert
the rest to the desktop-clean version after the mutex is fixed.

---

## P1 — Missing flows the user explicitly asked for

### F8 / T6. Saved-TV chooser + auto-reconnect on launch — entirely absent
No persistence anywhere (no localStorage, no store, no reconnect). Onboarding always starts at
`scan`. The design has a "Reconnect" screen + "Saved TV" tile; the RSA key is already persisted
Kotlin-side so re-auth is silent. **Needed:** persist paired devices (host/port/last-name/
device_type) and add a launch screen listing saved TVs with an auto-reconnect attempt, falling
back to Scan. This is the intended first screen.

### F9 / T5. Real device info after connect
Symptom: Connected/Dashboard show generic "Android TV"/"unknown". **Root cause (corrected):**
`list_devices_impl` *does* enrich via `harvest_properties` (batched `getprop`), and Onboarding
*does* call `list_devices` after connect — but `harvest_properties` silently swallows a
transport error and returns defaults (`name = "Android TV"`, `model = "Unknown Device"`). Under
the old `shell:` hang the getprop degraded to defaults. **Likely resolves now that `exec:`
works — [device] verify** that the batched getprop returns real props post-connect. Also: log
the swallowed error instead of hiding it, and use one canonical `deviceLabel` derivation
(prefer `properties.friendly_name`) — Onboarding and Dashboard currently derive the name
differently.

### F10. `find_remote` is Nvidia-Shield-only but shown on every screen
`find_remote` runs `am start -n com.nvidia.remotelocator/...` (Shield-only); errors on Google
TV, and is silently swallowed on most screens. **Fix:** gate the button on
`device_type === "shield"`.

---

## P2 — Build / distribution / polish

### B1. `gen/` regeneration footgun (HIGH latent)
Hand-edits live in the regenerable `gen/android` tree: the icon set (mipmaps + adaptive vectors
+ anydpi-v26), `AndroidManifest` (App.kt name, leanback, cleartext, FileProvider), `strings.xml`,
`build.gradle.kts` (minSdk/compileSdk/applicationId). All tracked (survive in git) but a
`tauri android init` overwrites them. The libadb transport was correctly extracted to the
plugin because it *could* be; icon/manifest/strings are app-module-scoped and cannot. **Fix:**
inventory these hand-edits at the top of HANDOFF; keep the icon *source* outside gen and
regen with `tauri icon`; confirm `strings.xml` derives from `productName`.

### B2. Material Symbols font is 5.34 MB (full set) — only 35 glyphs used
Subset with `pyftsubset`/`glyphhanger` → ~20-40 KB. The 35 used icons (grep `class="msr">`):
`add_circle, arrow_back, arrow_forward, auto_fix_high, cast, check, expand_more, fast_forward,
fast_rewind, hdr_on, help, home, keyboard, keyboard_arrow_{down,left,right,up}, lan, lock,
memory, menu, notifications_active, power_settings_new, settings, settings_backup_restore,
settings_remote, star, surround_sound, tune, tv_gen, verified_user, volume_off, volume_up,
wifi_tethering`. (Verify `tv_gen` resolves before subsetting.)

### B3. No Android build in CI
CI (`v2-tests.yml`) covers Rust (all 4 crates via the desktop stub) + mobile svelte-check +
Vite build, but **never compiles the Kotlin/Gradle/APK**. A Kotlin error, bad JitPack coord, or
manifest-merge conflict passes green and only fails on a manual device build. **Fix:** add a
gated (paths/`workflow_dispatch`/nightly) `tauri android build --apk --target aarch64` job, or a
cheap Kotlin-compile-only job.

### B4. Release readiness (M6) — not shippable
`bundle.active: false`; no signing keystore/config (unsigned release APK can't install); no
versionCode bump process; no release workflow/distribution. Release buildType has
`isMinifyEnabled = false` (no R8/resource shrink) and ships a universal APK (all 4 ABIs).
**Fix:** keystore + `signingConfigs.release` from `keystore.properties` + CI secrets; a mobile
release script (monotonic versionCode); an Android release workflow; enable minify +
per-ABI splits (arm64 only for real devices) — verify JNI/reflection keep-rules for libadb.

### B5. Frontend polish (from the frontend pass)
Webview-blank-on-resume has no handler (add `visibilitychange` reload); `BottomTabs` lacks
`env(safe-area-inset-bottom)` so labels sit under the home indicator on gesture-nav devices;
missing empty/error states (Apps empty search, Dashboard retry); native `confirm()`/`alert()`
used (off-brand, blocking); overlay a11y (span-onclick close buttons, no focus trap/Esc, toggle
needs `role="switch"`); the screenshot toast claims "saved to device" but the result is
discarded.

### B6. Security / safety — sound (LOW, informational)
The do-not-disable gate is centralized in `engine::safety` and **inherited by every mobile
destructive path** (disable/uninstall/launcher/snapshot/optimize) because mobile reuses the same
core commands — no mobile path runs `pm disable-user`/`pm uninstall` directly, and the safety
classify runs *before* `require_pro`. Snapshot path-confinement is inherited. Entitlement is
client-side (crackable), which is acceptable for a low-cost offline unlock; the safety gate is
independent of entitlement, so a cracked-Pro build still can't brick a device. Only follow-up is
M5 signed-license validation (revenue, not safety). **The HANDOFF note that Pro commands aren't
registered/gated on mobile is STALE — they are registered and gated; there are core tests
asserting ADB is never called on Free.**

---

## Recommended order

1. **F1** reboot arg (trivial). **F2/F3/F4** stop showing fake data as real (trust). **F10** gate
   find_remote. **T4** reboot false-negative. — all safe, mostly type-checkable, no device needed.
2. **F8/T6** saved-TV persistence + reconnect launch screen — the headline missing flow.
3. **F6** shared session store (device + entitlement, null-guard); **F7/M4** shared typed api/types.
4. **T1** remove the global mutex → concurrent shells; then **T3** revert core band-aids;
   **T2** real exit codes. — [device] verify.
5. **F9/T5** confirm device-info enrichment works over exec:; log the swallowed getprop error.
6. **B2** font subset; **B1** gen/ inventory; **B3** CI Android job; **B4** release/signing (M6);
   **B5** polish.

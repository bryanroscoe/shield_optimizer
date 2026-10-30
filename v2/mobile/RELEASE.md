# Releasing ATV Optimizer for Android

End-to-end checklist for cutting an Android build. Nothing here is automated on a tag yet — the
desktop app's `v2-*` tag pipeline (`.github/workflows/v2-release.yml`) is a separate track and must
not be reused for mobile. `.github/workflows/v2-tests.yml` already runs the mobile gates on every change;
building and uploading is still a local, manual step.

## 0. Prerequisites (one time)

- JDK 17 and the Android SDK: platform 36, build-tools 36, and an NDK (r27+).
- `ANDROID_HOME` and `NDK_HOME` exported — the `rust` Gradle plugin in
  `src-tauri/gen/android/buildSrc` shells out to `cargo` with these.
- The four Android Rust targets:
  ```
  rustup target add aarch64-linux-android armv7-linux-androideabi i686-linux-android x86_64-linux-android
  ```
- `cargo install cargo-about --locked --features cli` (for the notices step).

## 1. Create the upload keystore (one time, then keep it forever)

Coordinate signing identity and backup with the owner before any creation. Distinguish the app
signing key from a Play upload key: Play App Signing supports upload-key reset and specific
app-signing-key upgrade procedures. Do not assume replacing a local key preserves sideload
updates. Keep credentials out of Git. The following is reference documentation, not authorization
to generate a key.

```
keytool -genkeypair -v \
  -keystore ~/keys/atv-optimizer-upload.jks \
  -alias atv-optimizer-upload \
  -keyalg RSA -keysize 4096 -validity 10000 \
  -storetype PKCS12
```

Then point the Gradle build at it with `src-tauri/gen/android/keystore.properties` (gitignored):

```properties
storeFile=/Users/you/keys/atv-optimizer-upload.jks
storePassword=…
keyAlias=atv-optimizer-upload
keyPassword=…
```

`app/build.gradle.kts` reads that file first, then falls back to the environment:

| Property | Environment variable |
| --- | --- |
| `storeFile` (or `path`) | `ATVOPT_KEYSTORE_PATH` |
| `storePassword` | `ATVOPT_KEYSTORE_PASSWORD` |
| `keyAlias` | `ATVOPT_KEYSTORE_ALIAS` |
| `keyPassword` | `ATVOPT_KEYSTORE_KEY_PASSWORD` |

A relative `storeFile` resolves against `src-tauri/gen/android/`. If neither source is complete the
release build still succeeds but is **unsigned**, and Gradle prints a warning saying so — Play will
reject the artifact, and `adb install` will refuse it. `keystore.properties`, `key.properties`,
`*.jks`, `*.keystore` and `*.p12` are ignored in both `mobile/.gitignore` and
`src-tauri/gen/android/.gitignore`.

**Use Play App Signing.** Enrol the app so Google holds the app signing key and the key above is
only the *upload* key. That makes a lost or compromised upload key recoverable (Play support can
reset it) instead of terminal for the listing. It also lets Play re-sign per-ABI splits from a
single AAB.

## 2. Bump the version

```
mobile/scripts/bump-version.sh 0.2.0
```

That writes `package.json`, `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml` and the workspace
`Cargo.lock`, then prints the Android `versionCode`.

`tauri android build` regenerates `src-tauri/gen/android/app/tauri.properties` (gitignored) from
`tauri.conf.json`; `app/build.gradle.kts` reads `tauri.android.versionName` and
`tauri.android.versionCode` out of it. `versionName` is the `tauri.conf.json` version verbatim.
`versionCode`, unless overridden, is

```
versionCode = major * 1000000 + minor * 1000 + patch      # 0.1.0 -> 1000
```

Pre-release identifiers are ignored by that formula, so `0.2.0-beta.1` and `0.2.0` both derive
`2000`. Play rejects a re-used `versionCode`, so for anything shipped on a test track set an
explicit code:

```json
"bundle": { "android": { "versionCode": 2001 } }
```

(`bundle.android.autoIncrementVersionCode` is the alternative, but it requires committing
`tauri.properties`, which is currently gitignored. Ceiling is 2,100,000,000.)

## 3. Run the gates

From `v2/`:

```
cargo fmt --all --check
cargo clippy -p shield-optimizer-core -p atv-optimizer-mobile -p tauri-plugin-atv-adb --all-targets -- -D warnings
cargo test -p shield-optimizer-core -p atv-optimizer-mobile
```

From `v2/mobile/`:

```
npm run check     # svelte-check: 0 errors / 0 warnings
npm run build
```

## 4. Regenerate third-party notices

```
mobile/scripts/gen-notices.sh
```

Rewrites `THIRD-PARTY-NOTICES.md` and `src/lib/notices.generated.ts` (the same text as a TS module
for in-app display; it is a few hundred KB, so import it dynamically to keep it in its own chunk).
The Cargo inventory comes from `cargo-about` + `about.toml`; the non-Cargo components (Geist fonts,
Material Symbols, the embedded scrcpy server, the vendored `adb_client` patch, npm runtime
packages) are hand-maintained in `scripts/notices-header.md`.

The script **fails** if any dependency is GPL / LGPL / AGPL. Treat that as a release blocker rather
than something to work around: it is the licence check that keeps the app sellable.

## 5. Build

Tauri drives Gradle, and the generated `RustPlugin` already defines one product flavor per ABI
(`universal`, `arm64`, `arm`, `x86`, `x86_64`), so per-ABI output needs CLI flags, not Gradle edits.

```
# Play upload artifact — one AAB, all ABIs
npm run tauri android build -- --aab

# Per-ABI AABs and APKs
npm run tauri android build -- --aab --apk --split-per-abi

# Sideloadable arm64 APK only (fast; what a real TV-side test needs)
npm run tauri android build -- --apk --target aarch64
```

Outputs land under `src-tauri/gen/android/app/build/outputs/`:

- `apk/universal/release/app-universal-release.apk`
- `apk/arm64/release/app-arm64-release.apk` (with `--split-per-abi`)
- `bundle/universalRelease/app-universal-release.aab`

Confirm the artifact is actually signed before uploading:

```
$ANDROID_HOME/build-tools/36.0.0/apksigner verify --print-certs \
  src-tauri/gen/android/app/build/outputs/apk/universal/release/app-universal-release.apk
```

## 6. Upload

1. Play Console → Internal testing → create release → upload the AAB.
2. Check the pre-launch report; `minSdk` is 24 and `targetSdk` 36.
3. Complete Data safety from the verified final artifact, actual data inventory and owner-approved
   disclosures; see `DATA-INVENTORY.md`. Do not reuse preset LAN-only/no-backend/no-analytics
   answers. Include user-initiated external navigation and review the resolved SDK graph.
4. Promote internal → closed → production once a real device has been exercised end to end.

## Testing-candidate gates

This is a preparation checklist, not approval to create credentials, upload or install a build.
No purchase, trial or pricing policy is selected. Release polish takes priority over new features.

- [ ] Record exact commit, lockfiles, SDK/NDK/JDK versions and selected ABIs.
- [ ] Run host Rust fmt/clippy/tests, mobile `npm test`, check and build. Host CI is not an Android build.
- [ ] Resolve and review the Android release runtime dependency graph and its notices (see
      `ANDROID-DEPENDENCIES.md`); notices for declaration sites are not a complete resolved graph.
- [ ] Confirm signing-key ownership and durable custody through the owner. Do not generate keys
      merely to make a build pass. Existing Gradle config can emit an unsigned release when absent.
- [ ] Choose a new, never-uploaded `versionCode` across all tracks and build the candidate locally.
- [ ] Verify the APK with `python3 scripts/verify-release-apk.py candidate.apk`, using SDK tool
      paths via `--apksigner`, `--zipalign`, `--apkanalyzer` as needed. All missing tools, signature,
      debug-certificate/debuggable, package-id, target/version, ZIP and architecture-aware native ELF alignment failures
      reject the artifact. ELF load segments require 16 KB alignment for arm64-v8a/x86_64;
      valid 4 KB armeabi-v7a/x86 libraries are accepted, with ABI/header consistency checked.
      The receipt certifies ELF LOAD alignment only, not full ELF/16 KB compatibility. It does
      not validate GNU_RELRO layout or overlap; retain those checks (including NDK r27-and-earlier
      hazards) and actual 16 KB runtime validation as separate gates. It never signs, installs or uploads. A successful check does not prove
      the signing fingerprint is the owner's, or that the versionCode exceeds Play's latest value.
- [ ] For Play, separately verify the AAB signature, bundletool output and generated APKs, then
      Play's pre-launch report. The APK checker deliberately does not accept AABs.
- [ ] Run on an Android 16 KB page-size environment; static alignment checks are not runtime tests.
- [ ] Perform the physical matrix in `PAIRING-PLAN.md` and `LIFECYCLE-EVIDENCE.md`: Shield legacy
      debugging; fresh Google TV code pairing; wrong code; rotated port; replaced endpoint;
      denied/unavailable discovery; reconnect; Home/background/Back/Recents; recreation/process
      death; fresh install and upgrade; restore/undo; free and legitimately licensed Pro behavior.
      Preserve #93 and #105 until their actual acceptance evidence exists.
- [ ] Review release disclosures, risk copy, accessibility, support contact and listing assets.
      `DATA-INVENTORY.md` is a code-grounded draft, not a published privacy policy.
- [ ] Owner selects distribution and monetization. No Play purchase/restore or self-service trial
      exists. Do not claim that offline key entry is store purchase restoration or that expiring
      manually issued licenses constitute a self-service trial. The paywall uses neutral feature copy; approve actual purchase terms before public sale.
- [ ] Obtain the account-specific testing/verification requirements from the actual store account;
      no account status or production-access eligibility has been verified here.

### Implemented, not fully physically validated

Android 11+ SPAKE2 code pairing is implemented in `src-tauri/src/wireless_adb.rs` and the vendored
transport. Signed Ed25519 offline licensing, persistence and debug-only development keys are
implemented; see `LICENSING.md`. Neither implementation establishes current phone/TV end-to-end readiness.

### Still missing or unresolved

SAF import/export, mobile release automation, verified signed APK/AAB evidence, complete resolved
Android dependency notices, privacy/store disclosures, and a selected commerce channel remain open.
The `mobile-release-preflight.yml` workflow tests the APK checker with synthetic artifacts/mocked SDK outcomes;
it does not build/sign an APK, validate the native Android transport, or publish anything.

### Current external requirements to verify at submission

The configuration targets API36 (the mobile requirement effective August 31, 2026), with minSdk24.
Check current rules again before submission:

- https://support.google.com/googleplay/android-developer/answer/11926878
- https://developer.android.com/guide/practices/page-sizes
- https://developer.android.com/google/play/billing/integrate
- https://support.google.com/googleplay/android-developer/answer/9858738
- https://support.google.com/googleplay/android-developer/answer/10144311

For local-network access, SDK36 and SDK37 have different requirements; do not add/request a
future permission speculatively: https://developer.android.com/privacy-and-security/local-network-permission

## Open issues to recheck before a candidate

Re-read the tracker before cutting a candidate; close an issue only after its actual acceptance
evidence is reviewed.

| Issue | Release disposition / exact remaining evidence |
|---|---|
| [#93 Saved-TV discovery/reconnect](https://github.com/bryanroscoe/shield_optimizer/issues/93) | Physical gate: Shield without mDNS, rotated Google TV port, changed address, cached identity/name isolation and honest reconnect results. Browser coverage does not close this. |
| [#105 Restart/lifecycle report](https://github.com/bryanroscoe/shield_optimizer/issues/105) | Physical gate: distinguish Home/background, Back, Recents, recreation and process death. Do not claim restored live state or replay mutations from browser reconstruction. |
| [#123 Glance/Daydream](https://github.com/bryanroscoe/shield_optimizer/issues/123) | Current screensaver controls are not proof that the reporter's server-delivered Glance variant is covered. Obtain exact package/device behavior and reporter confirmation; retain vendor restore/reconnect tests. |
| [#161 Launcher sources/default](https://github.com/bryanroscoe/shield_optimizer/issues/161) | Desktop shows source-site links for installed and current-default launchers, and mobile parity includes them. Confirm the requested case and release availability; do not reimplement or auto-close. |
| [#162 Logcat](https://github.com/bryanroscoe/shield_optimizer/issues/162) | Desktop has a read-only Logs tab; mobile does not expose it. Confirm the requested filtering/live-display acceptance and release availability; it is not a reason to add mobile features before release. |

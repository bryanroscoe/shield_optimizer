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

An Android package's signing identity is permanent: a key change means a new listing. Generate the
key once, back it up in the password manager **and** offline, and never commit it.

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
3. Data safety form: the app talks only to devices on the user's own LAN over wireless ADB; there
   is no backend and no analytics. Keep that answer accurate if telemetry is ever added.
4. Promote internal → closed → production once a real device has been exercised end to end.

## Not done yet — read before promising a release date

- **SPAKE2 pairing is missing.** `adb_client` implements legacy network debugging (`:5555`) but not
  the Android 11+ pairing-code handshake, so a Google TV device that has never been paired from a
  PC cannot be onboarded from the phone alone. Clean-room SPAKE2 is queued; see
  `HANDOFF.md` and `TRANSPORT-LICENSING-RESEARCH.md`.
- **SAF import/export is missing.** Backups and snapshots stay in app-private storage; there is no
  Storage Access Framework picker, so a user cannot move a bundle off the phone.
- **Licensing / Pro entitlement** is not release-ready — the current unlock path is a development
  key, not signed validation. See `LICENSING.md` for the current status.
- **No release automation.** There is no mobile tag namespace and no signed-build job; step 5 is
  manual. an APK/AAB CI job is not set up yet; it needs
  (SDK + NDK setup, the four Rust targets, keystore secrets).

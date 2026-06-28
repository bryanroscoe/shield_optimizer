# ATV Optimizer mobile

Android phone/tablet app for no-PC Android TV diagnostics and optimization.

## What is implemented

- Tauri Android scaffold (`mobile/src-tauri`) with package id `com.atvoptimizer.mobile`.
- Kotlin `AdbPlugin` backed by libadb-android for mDNS discovery, pairing, connect/disconnect, `shell:`, and screencap transport.
- Rust `WirelessAdb` implementing the shared `AdbDriver` seam for mobile-supported operations.
- Touch-first Svelte pairing/diagnostic UI.
- Free entitlement by default; shared Rust Pro gates protect paid side effects before ADB/filesystem work.

## Local validation

From `v2/`:

```bash
cargo check -p atv-optimizer-mobile
cargo check -p atv-optimizer-mobile --target aarch64-linux-android
```

From `v2/mobile/`:

```bash
npm run build
npx tauri android build --target aarch64 --apk
```

The current build produces an unsigned APK at:

```text
v2/mobile/src-tauri/gen/android/app/build/outputs/apk/universal/release/app-universal-release-unsigned.apk
```

## Hardware validation still required

These require a real Android phone and Android TV / Google TV device:

1. Runtime permission flow for Wi-Fi/mDNS discovery on Android 13+.
2. mDNS discovery of `_adb-tls-pairing._tcp` and `_adb-tls-connect._tcp`.
3. Pairing-code flow against Shield 11 and Google TV 13+.
4. Reconnect using the persisted app-private ADB keypair/certificate.
5. `shell:` command round-trip through shared commands (`list_devices`, `health_report`, `package_states`).
6. Base64 screencap bridge.

Release hardening still needs signing, R8/ProGuard tuning for Conscrypt, and store/developer-verification setup.

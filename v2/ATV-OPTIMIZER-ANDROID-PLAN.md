# ATV Optimizer Android App — Implementation Plan

## Summary

Build a separate Android phone/tablet app that acts as the ADB client for Android TV / Google TV devices over Wi-Fi. The mobile product name is **ATV Optimizer**; the desktop app keeps the existing Shield Optimizer name, identifier, updater, Homebrew tap, screenshots, and release pipeline until a separate desktop rebrand migration is planned.

This plan starts with a behavior-preserving Rust workspace extraction so the audited safety engine and driver-generic command layer can be shared by desktop and mobile.

## Architecture

- Keep the current desktop app at `v2/src-tauri`.
- Add a workspace rooted at `v2/Cargo.toml` with:
  - `crates/core` (`shield-optimizer-core`) for the pure engine, shared ADB trait/output parsers, shared command handlers, embedded app lists, and entitlement model.
  - `src-tauri` for the existing desktop Tauri app, subprocess ADB driver, updater, platform-tools installer, host-network scan, and desktop file-path commands.
  - Future `mobile/` for the Android Tauri app and mobile frontend.
- Mobile ADB transport will be a Kotlin Tauri mobile plugin backed by libadb-android, called by a Rust `WirelessAdb` implementation.
- `AdbDriver` is intentionally partial on mobile: `shell`, `raw_bytes`, and synthesized `raw(["devices"])` work; `spawn`, `forward`, `push`, and `pull` are unsupported until mobile-specific equivalents are built.
- Scrcpy fast remote input remains desktop-only via `#[cfg(not(target_os = "android"))]`; mobile keeps the shared `adb shell input ...` fallback.
- Desktop UI remains untouched. A future mobile Svelte frontend should live separately and share only layout-agnostic TypeScript types/fixtures/components.

## Free / Pro Split

Free: pairing/connect, device profile, health/storage/RAM, risk-tiered audit, launcher view, screenshot, force-stop, trim caches, normal reboot, basic shell remote input, open Settings, and recovery/safety commands.

Pro: curated debloat/uninstall/optimize, snapshots save/apply/delete, launcher takeover, settings/tweaks writes, app permission/op writes, advanced reboot, multi-device, and later advanced remote/sideload/files/backup/clone.

Desktop remains always-Pro. Mobile starts Free and uses signed entitlement tokens to unlock Pro. Shared Rust gates return `LOCKED:<feature>` before side effects.

## Milestones

1. **M0 — Transport/permission/license spike:** prove phone-to-TV pair/connect/shell/screencap on real hardware through a Tauri Android plugin; audit libadb-android dependencies and Android network permissions.
2. **M1 — Workspace extraction:** create `shield-optimizer-core`, keep desktop behavior-identical, update workflows/scripts for workspace paths, and correct stale handoff facts.
3. **M2 — Mobile app scaffold + `WirelessAdb`:** add Android Tauri app, Kotlin `AdbService`, Rust `WirelessAdb`, and mobile handler list.
4. **M3 — Guided pairing + Free diagnostic UX:** add mDNS/manual pairing flow and free diagnostic screens.
5. **M4 — Touch-first mobile frontend:** build the separate mobile UI with demo fixtures.
6. **M5 — Pro gates + licensing:** implement license activation/refresh/offline grace and gate every Pro command in Rust.
7. **M6 — Distribution beta:** signed sideload-first APK/AAB, developer verification/package registration, and optional store-channel decisions.

## Validation

After M1, run from `v2/`:

```bash
cargo fmt --all --check
cargo clippy -p shield-optimizer-core -p shield-optimizer-v2 -p atv-optimizer-mobile --all-targets -- -D warnings
cargo test -p shield-optimizer-core -p shield-optimizer-v2 -p atv-optimizer-mobile
npm run check
npm run build
```

For the mobile scaffold, run from `v2/mobile/`:

```bash
npm run build
npx tauri android build --target aarch64 --apk
```

From M2 onward, add hardware validation for pairing, reconnect, screencap, shell command success/failure, mDNS fallback, persisted ADB keypair, and Android network permissions.

## Assumptions

- Desktop rebrand is out of scope.
- Mobile MVP excludes APK sideload, files, backup/clone, screen mirror, and mouse mode.
- Existing untracked root files (`atv-optimizer-android-strategy.html`, root `package.json`, root `package-lock.json`, root `node_modules/`) are not part of this implementation unless explicitly added later.

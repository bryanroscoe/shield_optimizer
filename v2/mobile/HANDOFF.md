# ATV Optimizer (mobile) — HANDOFF

Read this top-to-bottom before doing any mobile work. It is the authoritative description of the
current state. Companion docs in this directory:

- **[`BACKLOG.md`](BACKLOG.md)** — the ordered queue of remaining work.
- [`LIFECYCLE-EVIDENCE.md`](LIFECYCLE-EVIDENCE.md) — the app-restart report (#105): host matrix and
  the physical evidence still needed.
- [`PAIRING-PLAN.md`](PAIRING-PLAN.md) — the Android 11+ code-pairing protocol and its manual test.
- [`FAST-REMOTE-PLAN.md`](FAST-REMOTE-PLAN.md) — the scrcpy control channel and its open device gates.
- [`RELEASE.md`](RELEASE.md), [`LICENSING.md`](LICENSING.md),
  [`ANDROID-DEPENDENCIES.md`](ANDROID-DEPENDENCIES.md), [`DATA-INVENTORY.md`](DATA-INVENTORY.md) —
  signing, offline license keys, Android notices and the privacy-disclosure inventory.
- [`BILLING-IMPLEMENTATION-PLAN.md`](BILLING-IMPLEMENTATION-PLAN.md),
  [`BILLING-COMMAND-MATRIX.md`](BILLING-COMMAND-MATRIX.md),
  [`FREE-SETTINGS-UNDO-DESIGN.md`](FREE-SETTINGS-UNDO-DESIGN.md) — planned Play billing, the current
  Free/Pro gate of every command, and the proposed Free undo.
- [`EXPORT-DISCOVERABILITY.md`](EXPORT-DISCOVERABILITY.md) — where every saved file and report
  actually goes, and the proposed Android export scope.

Cross-session agent memory also lives in
`~/.claude/projects/-Users-bryanroscoe-Developer-shield-optimizer/memory/`.

## Status

The app is unreleased (`package.json` version `0.1.0`) and builds an unsigned debug APK.

**Nothing in the current `v2/mobile` code has been verified on a phone or TV.** Earlier device
sessions (a Pixel 10 Pro against two Nvidia Shields) proved the transport, the fast-remote channel
and legacy network-debugging connect, but they predate most of today's code. Everything else
described here rests on host evidence: Rust unit tests, and a browser suite that drives the real
Svelte screens against a mocked Tauri layer. That is worth a lot, but it is not a device. Never
describe a mobile behavior as device-verified unless a physical session actually exercised it.

The open physical gates are GitHub [#93](https://github.com/bryanroscoe/shield_optimizer/issues/93)
(saved-TV discovery and reconnect), [#105](https://github.com/bryanroscoe/shield_optimizer/issues/105)
(the app-restart report), code pairing against a real adbd, and fast-remote Phase 4. See
`BACKLOG.md` P0.

## 1. What this is

A phone/tablet app (**Tauri 2 + Rust + Svelte 5**, in `v2/mobile/`) that drives an **Android TV
over wireless ADB** with no PC: debloat, optimize, launcher, tweaks, snapshots, files, APK
backups, remote and diagnostics. Product name **ATV Optimizer**. It is sold **freemium as a
closed-source product**, which constrains the transport (§2). It shares the audited engine
(`v2/crates/core`) with the desktop app (`v2/src-tauri` + `v2/src`); the rule is *reuse the
desktop, keep core aligned, never fork it*.

Mobile has desktop parity for Optimize (search, app sheet, desktop's recommendation defaults),
Diagnostics (Suggestion column; a process counts as an app only once the TV confirms the package is
installed), Snapshots (marked Beta; Preview restore with a Now column), Apps (Recognised section,
storage details, Report this app, long-press menu), Launcher (Setup Wraith card, set any app as
Home, a separate confirmed Disable stock launcher step, re-read after every action), Tweaks, and
Files (find app backups via shared core `commands::files`, copy to phone, TV-side delete under
`/sdcard`). Links open in the phone's browser through `tauri-plugin-opener`, scoped to http/https.

Not on mobile yet: phone→TV upload (needs an Android document picker), install APK / sideload, the
bug-report bundle (`collect_diagnostics`), and a patch-notes viewer.

## 2. Transport

- **`WirelessAdb`** (`src-tauri/src/wireless_adb.rs`) implements the shared `AdbDriver` trait on top
  of the vendored, MIT-licensed, pure-Rust **`adb_client`** (`v2/vendor/adb_client`, upstream by
  Corentin Liaud / `cocool97`; every local change is listed in its `SHIELD-OPTIMIZER-PATCH.md`).
  It talks straight to the TV's adbd: no adb binary, no adb server. Blocking calls run on
  `spawn_blocking`.
- Capabilities: `shell` (shell-v2 framing for real stdout/stderr/exit status, shell-v1 fallback
  for older devices), `raw_bytes`, synthesized `devices`/`disconnect`/`reboot`, push/pull through
  `raw_transfer`, and dedicated byte-stream services (used by the fast remote). `forward`,
  `connect`, `pair` and `spawn` through `raw` are unsupported.
- A persistent RSA key is generated in `app_data_dir` (`ensure_adb_key`, PKCS#8 PEM). The first
  connect prompts "Allow debugging" on the TV; later connects are silent.
- Every read is bounded: 30 s shell inactivity (180 s for installs/clears), a 120 s safety net
  inside the vendored crate, a 5 s TCP connect. Only socket-level errors evict the connection; a
  missing remote file does not. Readers skip a bounded number of stray packets from finished
  streams, locks recover from poisoning, and the device identity is mirrored outside the device
  mutex so status calls never wait behind a running command.
- Hot read paths (health, apps, optimize, launcher, display) are one sentinel-batched shell each
  (`crates/core/src/adb/batch.rs`).
- **The Kotlin plugin (`tauri-plugin-atv-adb/`) is mDNS discovery only** (Android `NsdManager`, no
  third-party native libraries).
- **Legacy network debugging (`:5555`, no code) is the proven path.** Android 11+ code pairing is
  implemented in the vendored crate (`vendor/adb_client/src/message_devices/tcp/pairing/`) and wired
  through `WirelessAdb::pair`, but has never run against a real adbd. Onboarding keeps pointing
  users at Network debugging as the fallback until `PAIRING-PLAN.md`'s manual test passes.
  BoringSSL's SPAKE2 is a bespoke variant that is wire-incompatible with the RustCrypto `spake2`
  crate, so it is implemented directly on `curve25519-dalek`.
- **No GPL code, anywhere in the transport.** The app is sold closed-source, so GPLv3 libraries
  such as `libadb-android` are out. Google's prebuilt platform-tools `adb` is not an option
  either: it is a host binary (no Android arm64 build), and the Android SDK License Agreement
  (§3.4) bars redistributing it. Pairing was written clean-room from Apache-2.0 sources (AOSP
  `packages/modules/adb`, BoringSSL `spake25519.cc`) and permissive crates. `cargo about`
  generates the notices (`scripts/gen-notices.sh`); rerun it when adding a crate and keep the GPL
  family out. These licensing conclusions are engineering judgment, not legal advice; have counsel
  confirm them before public sale.
- **Fast remote**: a scrcpy control-only channel over its own authenticated connection, with
  transparent same-request fallback to `adb shell input`. Android keeps the channel through a
  30-second background grace. Details and open gates: `FAST-REMOTE-PLAN.md`.

## 3. Behavior that must not regress

- **Saved-TV identity.** A verified hardware id is the only identity that crosses endpoints;
  never merge, rename or forget on IP evidence alone. `normalizeHardwareId` in `lib/identity.ts`
  (trim; empty or `unknown` in any casing is no id, matching desktop's `idKey`) is the one rule.
  Scan rows are one per stored TV, never per address. An id-less row gets its own random `localId`
  and matches only its exact endpoint. When two or more id-less rows share the connecting endpoint,
  `rememberDevice` writes nothing, because which one reconnected is unknowable. Id-less rows carry a
  soft `fingerprint` (model/manufacturer/codename/friendly name); a clear model or manufacturer
  disagreement records the connection as a new row and raises `session.identityNote` ("A different
  device is now at this address."), shown globally from `App.svelte`. A fingerprint can only rule a
  match out, never verify one, and a missing field is unknown rather than a mismatch.
  `savedDeviceIsLiveConnection` is the one check for "this saved row is the live connection". The
  silent one-shot recovery runs the same identity check as an explicit reconnect.
- **Connection lifecycle.** Any error carrying the backend's "Connection to the TV was lost"
  prefix flips the session (`lib/connectionEvents.ts`): one silent redial of the same TV, then the
  global `ConnectionBanner` (Retry / Switch TV). A 45 s heartbeat probes while the app is visible.
  Launch shows a saved-TV picker and auto-dials only when exactly one TV is saved and the last
  session wasn't ended by an explicit Disconnect (`atv.autoConnect.v1`), always naming the target
  with Cancel. "Add a TV" is a separate `addtv` route that never dials. The Android back button
  keeps exactly one spare history entry.
- **Reconnect safety.** The session generation is reactive. Launcher, app sheets and file actions
  discard reads, picker intent, polling and continuations when the generation or liveness changes,
  including a reconnect to the same endpoint. Tweaks keys screensaver originals by verified
  hardware id (falling back to connection generation), and multi-setting writes stop before the
  next write when the connection changes. A write already dispatched cannot be recalled.
- **Never fake data.** Failed loads show an error with retry, never a fabricated healthy device or
  placeholder metrics. Safety tags come only from core `safety_info`; Unknown (uncatalogued) is
  distinct from unavailable (the lookup failed), and Unknown is never auto-selected.
- **Entitlement fails closed.** The default is Free; desktop opts into Pro explicitly. Shared Rust
  gates return `LOCKED:<feature>` before side effects, and `require_pro` runs after the safety
  check.
- Screenshots, pulled files, APK bundles and snapshots all land in app-private storage with no
  export (`EXPORT-DISCOVERABILITY.md`). Copy text never claims to have saved a file.
- **Support tiers claim only what the TV reported.** A tier comes from the connected TV's profile
  (`Device.support`), never from a saved row's address. Unreadable means `unknown` ("not verified"),
  and no tier relaxes a safety gate or blocks enable/restore/Recovery.
- **Anonymous reports are opt-out and Rust-built.** The frontend only says what happened; Rust
  decides whether to send and redacts. Never add a field that carries a serial, address, TV name,
  package list or path. Data-safety answers follow `DATA-INVENTORY.md`.

## 4. Licensing and commerce

- Implemented: signed offline license keys (`ATVOPT-<payload>-<signature>`, Ed25519, verified
  against public keys compiled in; `crates/core/src/license.rs`, `tools/atvopt-license`). The dev
  key `ATVOPT-PRO-2025` works in **debug builds only**. Private signing keys live in `~/.atvopt/`
  outside the repo; back up `~/.atvopt/license-signing-key.prod`. See `LICENSING.md`.
- Planned, not built: one-time Play Pro with free recovery (`BILLING-IMPLEMENTATION-PLAN.md`) and
  bounded Free undo (`FREE-SETTINGS-UNDO-DESIGN.md`). `EntitlementPreview` is an isolated
  test-only reducer and does not authorize anything. `BILLING-COMMAND-MATRIX.json` is checked by
  `tests/entitlement-preview.test.mjs` against the registered commands.

## 5. Repo layout

- `v2/mobile/src/` — Svelte 5 frontend: `screens/` (14), `components/`, and `lib/` (typed
  `api.ts`/`types.ts` mirroring desktop and core, the runes `session.svelte.ts` and
  `router.svelte.ts`, `savedDevices.ts`/`identity.ts`/`discoveryRows.ts`, `log.ts` debug ring
  buffer, `safety.ts`), `app.css` (design tokens + offline `@font-face`).
- `v2/mobile/src-tauri/src/` — `lib.rs` (builder + `generate_handler!`, license persistence),
  `wireless_adb.rs` (transport), `wireless_commands.rs` (`wireless_*`), `file_commands.rs`
  (remote listing, pull, find/delete via core, APK backup/restore), `remote_lifecycle.rs` (Android
  background grace), `scrcpy_resource.rs`.
- `v2/mobile/src-tauri/gen/android/` — the generated Android project, tracked **with hand edits**
  (icons, `AndroidManifest.xml`, `strings.xml`, `build.gradle.kts` signing config). Don't rerun
  `tauri android init`; it would overwrite them.
- `v2/mobile/tauri-plugin-atv-adb/` — the mDNS discovery plugin (Kotlin `NsdManager` + thin Rust).
- `v2/vendor/adb_client/` — vendored MIT transport, outside the Cargo workspace.
- `v2/tools/atvopt-license/` — license key generator/issuer/verifier CLI.
- `v2/crates/core/` — the shared engine and commands. **Keep `engine/` pure; keep it aligned with
  desktop.**
- Design source of truth: `~/Downloads/Shield Optimizer mobile design (1)/ATV Optimizer Mobile.dc.html`
  (20 screen sections; lime `#C9F24E`, Geist/Geist Mono). The `claude_design` MCP is not available
  here; work from the downloaded folder.
- Launcher icon and in-app BrandMark are the designer's exact glyph (`src-tauri/icons/icon.svg`).

## 6. Validation

From `v2/`:

```
cargo fmt --check
cargo clippy -p shield-optimizer-core -p shield-optimizer-v2 -p atv-optimizer-mobile \
  -p tauri-plugin-atv-adb -p atvopt-license --all-targets -- -D warnings
cargo test -p shield-optimizer-core -p atv-optimizer-mobile -p atvopt-license
cd vendor/adb_client && cargo test --lib      # the vendored crate is outside the workspace
```

From `v2/mobile/`: `npm run check` (0 errors / 0 warnings), `npm run build`, `npm test` (the
browser suite: real screens, mocked Tauri). `node --test evidence/lifecycle/reproduce.mjs` runs the
lifecycle matrix. CI (`.github/workflows/v2-tests.yml`) runs the Rust workspace, the vendored
tests and the mobile check/build/test; **it never compiles the Kotlin or builds an APK**, so a
Gradle or manifest error only shows up in a local Android build.

Android-target clippy works with the NDK clang exported as `CC_aarch64_linux_android` /
`CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER`. If you add a Material Symbols icon name, rerun
`scripts/subset-material-symbols.py` (needs `fonttools` + `brotli` in a venv) or the icon renders as
literal text. If you add a crate, rerun `scripts/gen-notices.sh`.

Hot files that parallel agents must not edit at the same time: `wireless_adb.rs`,
`session.svelte.ts`, `App.svelte`, `router.svelte.ts`, `api.ts`, `types.ts`, mobile `lib.rs`. One
lead owns those while parallel agents take disjoint screens.

## 7. Device playbook

Device sessions need the owner's go-ahead. A session must first identify the installed build and
rediscover current wireless endpoints: wireless-debugging ports rotate, so never reuse a recorded
one. Don't mark pairing or fast-remote Phase 4 complete from host tests or logs alone.

Env: `ANDROID_HOME=~/Android/sdk`, NDK `28.2.13676358`, tauri-cli 2.11.x, the four Android Rust
targets installed. Test phone: **Pixel 10 Pro**. A phone or TV emulator can stand in for parts of
this; see `v2/tools/emulator/README.md`.

- **Deploy to the phone over wireless ADB, never USB** (the owner's rule). It sometimes
  auto-connects through mDNS (`adb-<serial>._adb-tls-connect._tcp`); otherwise
  `adb pair <phone-ip>:<pairport> <6-digit code>` (the owner reads the code and port off the phone's
  Wireless debugging screen), then `adb connect <phone-ip>:<connectport>`. The phone sleeps off USB
  power and drops the connection; when it goes offline, wait or ask the owner to wake it.
- **Build the APK** (2-4 min, run it backgrounded): `cd v2/mobile && PATH="$ANDROID_HOME/platform-tools:$PATH"
  NDK_HOME="$ANDROID_HOME/ndk/28.2.13676358" npx tauri android build --apk --debug --target aarch64`.
  Run `gradlew clean` first if stale native libs linger. Kotlin only compiles in this build.
- **Install/launch**: `adb -s <serial> install -r -d <apk>`; `adb -s <serial> shell am start -n
  com.atvoptimizer.mobile/.MainActivity`. The first launch after install may need a second `am start`.
- **Screenshots**: `adb -s <serial> exec-out screencap -p > shot.png`. Nothing gets past the secure
  lock screen, so ask the owner to unlock. The WebView is opaque to `uiautomator`, so blind
  `input tap` by pixel coordinates is unreliable; let the owner drive flows that need the TV's
  "Allow" prompt.
- **Debug logging**: Rust `tracing` goes to logcat tag `RustStdoutStderr` and to `debug.log` in the
  app data dir (pull with `run-as com.atvoptimizer.mobile`); the More screen shows it through
  `read_debug_log`. macOS has no `timeout`; quote logcat tag filters in zsh (`'Tag:V' '*:S'`).

## 8. Invariants and conventions

From the repo `CLAUDE.md`: the engine (`crates/core/src/engine/`) stays pure. One `AdbDriver` seam
(desktop `SubprocessAdb`, mobile `WirelessAdb`). `engine::safety` gates every disable path. App lists
live in JSON, not code. Tauri commands return `Result<T, String>`. Svelte 5 runes only. Command
argument names are camelCase in TS and Tauri maps them to snake_case (`pkg` → `package`).

## 9. What remains

`BACKLOG.md` is the ordered list. In short:

1. The physical device pass: #93, #105 (`LIFECYCLE-EVIDENCE.md`), and the stability and UI checks
   in `BACKLOG.md` P0.
2. Code pairing against a real adbd (`PAIRING-PLAN.md`).
3. Fast-remote Phase 4 device gates (`FAST-REMOTE-PLAN.md`).
4. Android SAF export/import, which also unblocks phone→TV upload; then sideload, the bug-report
   bundle and a patch-notes viewer.
5. Product decisions for the owner: a free read-only Optimize plan, and the unused
   `Feature::FileManager` / `Feature::BackupClone` gates.
6. Release: signing key and Play App Signing, a signed build, resolved Android dependency notices,
   privacy and store disclosures, and the commerce channel (`RELEASE.md`).

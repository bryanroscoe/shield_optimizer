# ATV Optimizer (mobile) — handoff & operations guide

This is the continuation guide for the **ATV Optimizer Android app**: a phone/tablet app
that drives an Android TV over **wireless ADB** (no PC), monetized freemium. It lives in the
`shield_optimizer` monorepo under `v2/mobile/`. Read this end-to-end before continuing — it
carries the current state, the build/test/iterate playbook, the known blockers, and the
remaining roadmap. Pair it with the top-level plan at
`~/.claude/plans/i-m-fine-to-release-bright-cascade.md` and `v2/ATV-OPTIMIZER-ANDROID-PLAN.md`.

Working branch: **`feat/atv-optimizer-mobile`** (based off `main`). Nothing pushed yet.

> **See also `ARCHITECTURE-REVIEW.md`** (same dir) — a full 3-pass review (transport/core,
> frontend, build/distribution/security) with prioritized, file-cited findings. It supersedes
> a few stale notes below. Corrections it established: **Pro commands ARE registered and gated
> on the mobile handler** (§6's old "not registered yet" was wrong — there are core tests
> asserting ADB isn't called on Free); the **global Kotlin shell mutex** (not the parse) is the
> real root cause of the storage/health starvation; and **device-info enrichment already exists**
> in `list_devices_impl` (`harvest_properties` getprop) — the generic "Android TV/unknown" was
> that getprop silently degraded under the old `shell:` hang, so it likely resolves now that
> `exec:` works (verify on device). Storage itself is fixed (commit `3ef29de`: %-anchored parse
> kept, df timeout starvation eased); the deeper fix is removing the mutex (T1 in the review).

---

## 1. What it is / product intent

- Phone is the **ADB client**; the TV is the target. The phone talks to the TV's `adbd`
  directly over Wi-Fi using **libadb-android** bundled in the APK — this is a *separate* ADB
  stack from any desktop `adb`. None of it shows up in a computer's `adb devices`.
- Rebrand: **ATV Optimizer** (covers all Android TV, not just Shield).
- Freemium: free = no-PC connect + curated diagnostic + safe reversible quick-wins; Pro
  (one-time ~$4–6) = debloat / launcher / snapshots / tweaks / remote / multi-device.
- Reuses the desktop v2 engine + commands verbatim through the `AdbDriver` seam; only the
  transport is new.

## 2. Architecture (as built)

Cargo workspace at `v2/` (`v2/Cargo.toml` members): `crates/core`, `src-tauri` (desktop),
`mobile/src-tauri` (mobile app), `mobile/tauri-plugin-atv-adb` (transport plugin).

- **`crates/core`** — platform-neutral: pure `engine/`, the `AdbDriver` trait +
  `AdbError`/`AdbOutput`, `commands/*` (devices, health, apps, launcher, optimize, snapshot,
  tuning, reboot, recovery, screenshot, input), `AppState` (holds `entitlement`), and
  `license.rs` (`Entitlement::{Free,Pro}`, `require_pro(Feature)` → `Err("LOCKED:<feature>")`).
  Pro command paths call `require_pro` AFTER the do-not-disable safety check.
- **`mobile/tauri-plugin-atv-adb`** — the Tauri plugin that IS the transport. This is the
  regen-safe home for the Android native code (a `tauri android init` regenerates
  `mobile/src-tauri/gen/` and would wipe anything custom there — so it must NOT live in gen).
  - `src/{lib.rs,mobile.rs,desktop.rs,models.rs,error.rs}` — Rust: `init()` + `AdbExt`
    (`app.adb()`) + `Adb<R>` handle (blocking `run_mobile_plugin`; desktop stub for host/CI).
  - `android/` — Android library project: `build.gradle.kts` (declares libadb-android 3.1.1,
    conscrypt-android 2.5.3, bcpkix-jdk15to18 1.81, its OWN jitpack repo), `AndroidManifest.xml`
    (INTERNET / NEARBY_WIFI_DEVICES etc. — merged into the app), and Kotlin in
    `src/main/java/app/tauri/atvadb/`: `AdbPlugin.kt` (@TauriPlugin @Command discover/pair/
    connect/disconnect/shell/screencap + Conscrypt/PRNG init in `load()`), `AdbService.kt`
    (libadb calls, mDNS discovery, stream reads), `AtvAdbConnectionManager.kt`
    (AbsAdbConnectionManager: persisted RSA key + self-signed cert).
  - Tauri auto-wires the plugin's android project by its tracked path via the generated
    (gitignored) `gen/android/tauri.settings.gradle` + `app/tauri.build.gradle.kts`.
- **`mobile/src-tauri`** — the mobile app crate: `lib.rs` (builder, plugin, `AppState` as
  `Entitlement::Free`, `app_data_dir()` for the data dir, `generate_handler!` registering the
  wireless_* commands + the reused core commands), `wireless_adb.rs` (`WirelessAdb impl
  AdbDriver` over the plugin via `spawn_blocking`; synthesizes `raw(["devices"])`; `shell`/
  `raw_bytes` through the bridge; forward/push/pull/spawn = Unsupported), `wireless_commands.rs`
  (`wireless_discover/pair/connect/disconnect`).
- **`mobile/src`** — the Svelte 5 (runes, plain Vite, NOT SvelteKit) frontend. Currently a
  single `App.svelte` implementing the onboarding flow. `app.css` holds the design tokens +
  offline `@font-face` (Geist, Geist Mono, Material Symbols Rounded in `src/fonts/`). `main.ts`
  mounts via Svelte 5 `mount()`.

## 3. Design source of truth

`~/Downloads/Shield Optimizer mobile design/` — `ATV Optimizer Mobile.dc.html` (full multi-screen
spec) + `screenshots/`. Tokens: accent lime `#C9F24E` (+ `--accent-ink #0B0D10`), canvas
`#0B0D10`, surface `#16191E` / `#1E2229`, text `#F2F5F8` / soft `#C7CDD6` / muted `#8A929E` /
dim `#5B626D`, teal `#35D6A5` (connected/safe), amber `#F5B544` (storage/review), danger
`#FB6B5F`, purple `#A78BFA`. Fonts: **Geist** (UI) + **Geist Mono** (all machine values:
IP:port, MB, serials) + **Material Symbols Rounded** (icons via `<span class="msr">name</span>`,
`.fill` for filled). Phone-frame/status-bar/home-indicator in the mockup are CANVAS CHROME —
do NOT render them; the real app uses safe-area insets.

Screens in the spec: §1 Onboarding (1.0 Reconnect, 1.1 Scan+radar, 1.2 Pair-code, 1.3
Connected) — **done**; §2 Home dashboard (health-score ring, 3 stat tiles, quick-actions grid,
bottom tab bar) + Diagnostics (memory/storage/temp bars, top memory consumers with risk tags);
§3 Optimize wizard (per-app risk tiers, Pro); plus more below in the file (apps, remote,
settings, paywall — read the rest of the .dc.html for these).

## 4. Current state — what works (verified on a Pixel 10 Pro)

- **Onboarding UI** matches the mockup on-device: Scan (radar, `tv_gen` core) → device cards
  (cast icon, friendly label, mono IP, "No code needed" teal tag for legacy) → Pair-code
  (6 segmented boxes + privacy callout) OR no-code straight to connect → Connected (teal check).
  Safe areas correct; Geist + Material Symbols render from the offline bundle.
- **Discovery** finds all LAN TVs: scans `_adb-tls-pairing._tcp`, `_adb-tls-connect._tcp`
  (Android-11 wireless debugging) AND legacy `_adb._tcp` (:5555 network debugging), folds
  services per host into one card, filters out the phone itself (local-interface addresses),
  early-exits ~1.5s.
- **Connect + pair** work: cert-generation bug fixed (see §6), RSA-auth connect to a legacy
  Shield reaches the Connected screen (`connected=true`).
- **Free/Pro gating** scaffolded in core (`require_pro`), desktop stays Pro (unchanged).
- Gates green: `cargo fmt/clippy/test` across all four crates, mobile `npm run check` 0/0 +
  build, aarch64 debug APK builds and installs.

## 5. THE current blocker (in progress)

**libadb-android's stream read hangs for command output on the legacy Shield.**
`manager.connect()` succeeds, but `manager.openStream("exec:<cmd>")` → read-to-EOF never
returns (observed 87s+). This blocks device-info enrichment (Connected screen shows generic
"Android TV" / "unknown" instead of "Bedroom Shield" / Android 11) and ALL diagnostics/shell
features.

**Proven it is NOT the command/device/exec: service:** the exact same commands return instantly
via native `adb exec-out` from a Mac connected to the same Shield (`getprop`, `settings get`,
the compound device-info command, and `dumpsys meminfo` 31 KB). So the hang is inside
libadb-android's read/close/demux path. The `withTimeout`/`runInterruptible` guard did NOT abort
the blocked read (interrupt not surfaced by libadb's `AdbStream.read()` `Object.wait()`).

**Investigated (Opus, against libadb-android source) — `setApi` hypothesis DISPROVEN:** libadb's
CNXN banner is the fixed minimal `"host::\0"` with NO feature list (no `shell_v2`/`cmd`), and
`setApi` only picks the protocol version (`0x01000001` at API ≥ 28) + max payload (1 MB) — both
byte-identical to what native `adb` (which works against this same Shield) sends. So negotiation
is not the cause. The real mechanism: `AdbStream.read()` reaches EOF only via `notifyClose()`,
which the reader thread calls only on a peer `A_CLSE`; for this Shield's `exec:` stream over
libadb's demux, the `A_WRTE`/`A_CLSE` packets never reach the stream, so `read()` parks forever.

**Done so far (in `AdbService.kt`, committed but NOT yet device-verified):** (a) a timeout that genuinely aborts — `readStream`
runs in an `async(IO)` job inside a `supervisorScope`, `withTimeout(await())`, and on timeout
calls `stream.close()` FIRST (sends CLSE, wakes the parked read) then cancels + throws
`IOException`; **lowered to 8s** so it fails fast instead of hanging. (b) First-read diagnostic
logging so the NEXT device run categorizes the failure:
- no `first read` line + `no bytes before timeout` → `A_WRTE` never routed to the stream
  (reader-thread / local-id demux issue).
- `first read returned {N}B` then `{N} bytes arrived but stream never closed` → bytes flow but
  the peer `A_CLSE` never arrives/routes (CLSE/EOF handling).
- `first read returned -1 (immediate EOF, 0B)` → clean empty (not a hang).

**NEXT STEP: device build → connect to `.196` → read the `AtvAdb` log to categorize**, then fix
accordingly. Likely endgame is a small libadb-android patch/fork (vendor the demux/close path)
or a protocol workaround; if command output proves unreliable via libadb on legacy `:5555`
devices generally, reconsider the transport for legacy TVs. Device-info enrichment + all
diagnostics remain blocked until command output returns.

## 6. Bugs already fixed this effort (don't re-introduce)

- **Cert generation blocked every connect/pair** (`bb472f3`): `JcaX509CertificateConverter()
  .setProvider("BC")` throws `NoSuchAlgorithmException: X.509 for provider BC` — Android's
  built-in BouncyCastle is stripped and has no X.509 factory. Fixed by converting with the
  platform default X.509 factory (Conscrypt), same as `readCertificate()`.
- **screencap wrong service**: was `shell:exec-out screencap -p` (exec-out is a host-client
  subcommand, not a device binary, and pty mangles binary). Now `exec:screencap -p`.
- **data dir unwritable on Android**: `dirs::data_local_dir()` returns None → `./` (EACCES).
  Now `app.path().app_data_dir()` with dirs as host-dev fallback.
- **blocking JNI on the async runtime**: every `WirelessAdb` call now goes through
  `spawn_blocking`.
- **connect error clobbered**: `connect()` used to run `refreshDevices()` unconditionally,
  wiping the error to blank ("flashes and vanishes"). Now only refreshes on `ok`.
- **invisible device text**: device cards are `<button>`s and inherited the WebView's default
  black text; base `button { color: … }` fixed it.
- **notch cutoff**: `viewport-fit=cover` + `env(safe-area-inset-*)`.

## 7. Known issues / follow-ups (not yet fixed)

- **Webview blank on resume**: after the app is backgrounded, the Tauri webview process is
  frozen and can repaint blank; a cold start fixes it. Add an onResume reload/handler.
- **Material Symbols font is 5.1 MB** (full set). Subset to the ~30 icons actually used
  (fonttools/glyphhanger) before release.
- **Diagnostic screen not built** — blocked by §5 anyway.
- **Global Kotlin `Mutex`** serializes all shell streams (kept for correctness; libadb streams
  may not be concurrency-safe). Revisit only if verified safe.
- **`exec:` returns stdout only** (no stderr) — fine for diagnostics; `AdbOutput
  .shell_reported_failure()` scans stdout+stderr but pm/settings write errors to stdout. Note
  in `AdbService.shell`.
- **Mobile frontend has its own inlined types/invoke wrappers** — duplicates desktop
  `src/lib/{types,api}.ts`. Plan M4 = extract a shared package. Until then they can drift.
- **Pro commands are not registered in the mobile handler yet** — the `require_pro` gates
  aren't reachable on mobile until they are.

## 8. Remaining roadmap (phased)

1. **Fix §5 transport read hang** (current). Then re-test device info + a real diagnostic.
2. **§2 Dashboard + Diagnostics screens** — health-score ring, stat tiles, quick actions,
   bottom tab bar; wire to `health_report` / `report_all` / `device_profile` / `trim_caches` /
   `reboot` / `take_screenshot`. Introduce a router / component-per-screen structure
   (`src/screens/*.svelte`) so screens can be built in parallel without `App.svelte` conflicts.
3. **§3 Optimize wizard** — per-app risk tiers from the engine; Pro-gated apply; register the
   Pro commands in the mobile handler behind `require_pro`.
4. **Apps / Remote / Settings** screens.
5. **M5 licensing** — real key validation + signed-token offline grace + educational paywall
   on `LOCKED:` (Lemon Squeezy/Gumroad).
6. **M6 distribution** — release keystore/signing, own-site APK; subset the icon font.

## 9. OPERATIONS PLAYBOOK — how to build, test, iterate

Environment (this machine): `ANDROID_HOME=~/Android/sdk`, NDK `28.2.13676358`, tauri-cli
2.11.x, four android Rust targets installed, `adb` at `/opt/homebrew/bin/adb` (also
`$ANDROID_HOME/platform-tools/adb` — identical build).

### Fast UI iteration (sub-second, NO device build) — use this for all pure-frontend work
```
cd v2/mobile && npm run build                 # vite build → build/
(cd build && python3 -m http.server 8899 &)   # serve it
```
Then a Playwright headless screenshot (chromium is installed at ~/Library/Caches/ms-playwright).
A harness that stubs the Tauri IPC so `invoke()` resolves canned data (drive scan→found→pair→
connected without a device) is at
`/private/tmp/.../scratchpad/flow.mjs` (recreate from §here if gone): set
`window.__TAURI_INTERNALS__ = { invoke: (cmd)=>Promise.resolve(mock(cmd)), transformCallback:(c)=>c, ipc:()=>{} }`
via `page.addInitScript`, viewport 384×812 dpr 2. This validates layout/fonts/flow fast; safe-area
insets read as 0 in the browser, so still spot-check on device.

### Device build (~2–4 min; needed for Kotlin/Rust changes + safe-area + real transport)
```
cd v2/mobile
PATH="$ANDROID_HOME/platform-tools:$PATH" NDK_HOME="$ANDROID_HOME/ndk/28.2.13676358" \
  npx tauri android build --apk --debug --target aarch64
```
APK → `mobile/src-tauri/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk`
(~280 MB debug). Gradle caches Rust/Kotlin, so a frontend-only rebuild is faster. Run this in
the background and wait for the completion notification — do NOT start it while an agent is
mid-editing a Kotlin/Rust file (you'll compile a half-written file).

### Install / launch / screenshot / drive on device
```
S=58040DLCH005YV      # the Pixel 10 Pro's serial (USB). `adb devices` to reconfirm.
adb -s $S install -r -d <apk>
adb -s $S shell svc power stayon true          # stop it auto-locking mid-test
adb -s $S shell am force-stop com.atvoptimizer.mobile   # cold start avoids the resume-blank bug
adb -s $S shell am start -n com.atvoptimizer.mobile/.MainActivity
adb -s $S exec-out screencap -p > shot.png     # then Read the png
```
Driving the UI blind via `adb shell input tap X Y`: the WebView is OPAQUE to `uiautomator`
(one node), so you can't get element bounds — you tap by pixel coordinates read off a
screenshot. Coordinates are the device's native 1080×2410; the Read tool shows the image at
896×2000 with a "×1.21" note, so multiply displayed coords by 1.21. The layout SCROLLS, so
re-screenshot before each tap; taps drift otherwise. **The secure lock screen blocks all of
this** — if `dumpsys window | grep isKeyguardShowing` is true you can't get past the PIN; ask
the user to unlock. Prefer having the user drive flows that need the TV's "Allow" prompt (it
appears on the TV, not the phone) or the lock screen.

### Logs
The Rust side logs to logcat as `RustStdoutStderr`; the Kotlin transport logs as tag `AtvAdb`
(connect/shell start+done, errors). Dump (non-blocking): `adb -s $S logcat -d | grep -aE
"AtvAdb|RustStdoutStderr"`. Note: `timeout` is NOT on macOS (use a background+kill loop). In
zsh, QUOTE logcat tag filters (`'AtvAdb:V' '*:S'`) or the `*` globs and the command aborts.

### Gates (must pass; CI runs equivalents)
```
cd v2 && cargo fmt --check && cargo clippy -p shield-optimizer-core -p shield-optimizer-v2 \
  -p atv-optimizer-mobile -p tauri-plugin-atv-adb --all-targets -- -D warnings && \
  cargo test -p shield-optimizer-core
cd v2/mobile && npm run check   # 0 errors 0 warnings
```
Kotlin CANNOT be compiled standalone — it only compiles in the gradle android build, so
review Kotlin by eye and rely on the device build to validate it.

### The test network (this user's LAN)
Shields / TVs on legacy network debugging `:5555`: `192.168.42.71`, `192.168.42.196`
(model SHIELD_Android_TV, "Bedroom Shield" — **already authorizes this phone's key, so it
reconnects with no TV prompt**), `192.168.42.143`, `192.168.42.25`. The Pixel itself advertises
its own TLS service at `192.168.42.211:<random>` — filtered out by discovery. First connect to
a NOT-yet-authorized TV pops an "Allow debugging?" dialog ON THE TV (accept with the remote);
after that the key is remembered.

## 10. How to work here (learned preferences)

- The user wants **aggressive parallelism**: spin up multiple **Opus** implementers at once for
  independent files (e.g., one on Kotlin transport, one on the Svelte UI) — they're the fastest
  path. Keep agents on non-overlapping files to avoid conflicts. Reserve the on-device
  screenshot/verify loop for the main thread (agents can't drive the phone).
- Bias hard to **action over asking**; move fast, verify on device, iterate.
- Commit checkpoints as you go (no `Co-Authored-By` per repo convention; new commit each time).
- Follow the design spec's own copy/direction; don't regress to a generic look. The client
  chose the lime accent + Geist deliberately.
- Legacy Shields = network debugging, **no pairing code** — the user just taps Allow/OK on the
  TV; only newer Google TV shows a 6-digit code.

## 11. Commit history on `feat/atv-optimizer-mobile` (newest first)
```
ac9c3cd  Mobile UI: implement the ATV Optimizer onboarding design
bb472f3  Mobile: fix cert generation that blocked every connect/pair
b90e6d5  Mobile UI: legacy network-debugging discovery + fix invisible device text
c0d0668  Mobile UI: redesign pairing screen, fix notch cutoff, sharpen scan
27f2e1a  Mobile: fix on-device transport bugs from code review
948460b  Mobile: extract wireless-ADB transport into a regen-safe Tauri plugin
2a9fcb6  ATV Optimizer mobile: wireless-ADB transport, Free/Pro gating, guided pairing
16e5c42  Extract v2 shared core workspace
```
The libadb read-hang mitigation from §5 (fast aborting 8s timeout + categorized first-read
logging) is committed but **not yet device-verified** — the next device build must confirm the
Kotlin compiles and read the categorized `AtvAdb` log against `.196`.
Other open branches (unrelated, from the desktop track): PR #84 (release tooling), PR #85
(restart button), PR #70 (dead-code, conflicting/stale).

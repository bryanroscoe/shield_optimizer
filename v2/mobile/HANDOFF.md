# ATV Optimizer (mobile) — HANDOFF

Read this top-to-bottom before doing any mobile work. It is the authoritative, current handoff.
Companion deep-dives (all in this dir): `ARCHITECTURE-REVIEW.md` (findings audit),
`FEATURES.md` (screen ↔ command map), `TRANSPORT-LICENSING-RESEARCH.md` (why the transport is
what it is), `CLOUD-TASK.md` (brief for the nightly cloud agent). Cross-session memory also lives
in `~/.claude/projects/-Users-bryanroscoe-Developer-shield-optimizer/memory/`.

Branch: **`feat/atv-optimizer-mobile`** (pushed to origin, in sync). Working tree clean.
Last commit: `bd6cbd8`. Not merged to `main` and no PR yet.

---

## 1. What this is
A phone/tablet app (**Tauri 2 + Rust + Svelte 5**, in `v2/mobile/`) that drives an **Android TV
over wireless ADB** — no PC. Debloat / optimize / launcher / tweaks / snapshots / remote, etc.
Monetized **freemium**; the owner (Bryan) **wants to sell it** (this drove the transport rewrite —
see §3). Product name **ATV Optimizer**. It shares the audited engine (`v2/crates/core`) with the
mature **desktop** app (`v2/src-tauri` + `v2/src`); the rule is *reuse the desktop, keep core
aligned, never fork it*.

## 2. Current state — what's DONE (all committed + pushed, gate-green)
- **Full frontend re-architecture** (`b7c3d75`): shared typed `api.ts`/`types.ts` (mirror desktop
  + core), a central `invoke` wrapper with a debug-log ring buffer (`lib/log.ts`), a runes
  **session store** (`lib/session.svelte.ts` — device/host/entitlement/liveness + health/bloat
  cache), a real **router with a screen stack + Android hardware-back** (`lib/router.svelte.ts`,
  fixed the Home-tab→blank bug), and the desktop fast-load patterns (lazy per-screen load,
  invalidate-on-mutation, optimistic UI, progressive enrichment).
- **Never-fake-data + trust fixes**: Dashboard shows a real error+retry on a failed health load
  (no fabricated "healthy" device); Diagnostics/Optimize render `—`/skeleton/error, never fake
  numbers; safety tags come only from core `safety_info` (no inline classifier).
- **Transport swapped to pure-Rust `adb_client`** (`3e8bcfc`) — see §3. This is the biggest change.
- **All screens built** (14 total): Onboarding (+Reconnect/saved-TV), Dashboard, Diagnostics,
  Optimize, Apps (+AppDetailSheet), Remote, More, Launcher, Tweaks, Snapshots, Devices, RiskGuide,
  Files, Backups. Components: BottomTabs, Toast, ConfirmDialog, FindRemoteButton (one consistent
  "ring the remote?" affordance, Shield-gated), BrandMark, PaywallSheet, AppDetailSheet.
- **Real Pro unlock**: `activate_license` flips + persists backend entitlement; **test key
  `ATVOPT-PRO-2025`** (case-insensitive). `LOCKED:<feature>` errors route to PaywallSheet.
- **Icon + branding**: launcher icon and in-app BrandMark are the designer's exact glyph — a
  TV/monitor on a stand with three **vertical faders** (exact SVG in `src-tauri/icons/icon.svg`).
- **On-device validated**: connected to a real Shield over the new transport; the app is fully
  usable; the connection-lost banner + real device name resolve. (Pixel 10 Pro test device.)

## 3. THE transport story (most important context)
Originally the transport was **libadb-android (GPLv3)**, a Kotlin lib called over a JNI plugin.
Two fatal problems: (a) **GPLv3 blocks selling** a closed-source product; (b) **unreliable** — its
`exec:` streams hung on an older Nvidia Shield's adbd waiting for a CLSE that never came.
**We replaced it with `adb_client`** — a **third-party MIT-licensed pure-Rust crate** (Corentin
Liaud / `cocool97`, crates.io v3.2.2). We did NOT write it. It reimplements the ADB client
protocol (connect, RSA auth, stream multiplexing, shell/exec/push/pull) in pure Rust. Proven:
a spike ran every previously-hanging command cleanly on the real Shield; it cross-compiles to
aarch64-Android; the clean APK has **zero GPL native libs** (only our `libatv_optimizer_mobile_lib.so`).
- `WirelessAdb` (`src-tauri/src/wireless_adb.rs`) now owns an `adb_client::tcp::ADBTcpDevice`
  (blocking calls via `spawn_blocking`), implementing the core `AdbDriver` trait. Real exit codes
  + stderr now flow through. A persistent **RSA key is generated/persisted in `app_data_dir`**
  (`ensure_adb_key`, PKCS#8 PEM) — first connect prompts "Allow debugging" on the TV, then silent.
- The **Kotlin plugin (`tauri-plugin-atv-adb`) is now mDNS-discovery ONLY** (NsdManager, a pure
  Android framework, no GPL). libadb/Conscrypt/BouncyCastle Gradle deps + the jitpack repo are gone.
- **Legacy network-debugging (`:5555`, no pairing code) works now** — that's the proven path.
- **Gap:** `adb_client` does NOT do the Android-11 **SPAKE2 pairing** for *new* Google-TV devices
  (the 6-digit-code flow). `wireless_pair` returns a clear "use network debugging for now" error.
  Clean-room SPAKE2 (referencing Apache-2.0 AOSP) is a queued follow-up. Licensing note: for a
  commercial release, add a third-party-licenses acknowledgment (MIT for adb_client + rustls/rsa/
  rcgen); generate with `cargo about`.

## 4. Repo layout (mobile)
- `v2/mobile/src/` — Svelte 5 frontend. `lib/{api,types,session.svelte,router.svelte,log,savedDevices}.ts`,
  `screens/*.svelte` (14), `components/*.svelte` (7), `app.css` (design tokens + offline @font-face).
- `v2/mobile/src-tauri/src/` — mobile Tauri app: `lib.rs` (builder + `generate_handler!`),
  `wireless_adb.rs` (adb_client transport = `AdbDriver`), `wireless_commands.rs` (wireless_*),
  `file_commands.rs` (list_remote_dir/pull_file/backup_apk/list_backups).
- `v2/mobile/tauri-plugin-atv-adb/` — mDNS discovery plugin (Kotlin NsdManager + thin Rust).
- `v2/crates/core/` — SHARED engine+commands (pure `engine/`, `commands/*`, `adb/{driver,parse}`,
  `license.rs`). Desktop and mobile both register from here. **Keep `engine/` pure; keep aligned
  with desktop.**
- Design source of truth: `~/Downloads/Shield Optimizer mobile design (1)/ATV Optimizer Mobile.dc.html`
  (20 screen sections; lime `#C9F24E`, Geist/Geist Mono). The `claude_design` MCP is NOT available
  in this environment — work from the downloaded folder.

## 5. What REMAINS (in priority order; also in CLOUD-TASK.md)
1. **Fast remote** — the remote works but in slow "compat" mode (`adb shell input`, ~690ms/press).
   The desktop's fast path (scrcpy control channel) is `#[cfg(not(android))]` and can't port.
   Needs a mobile equivalent (scrcpy-server over an `adb_client` stream). **Spike first** — I was
   mid-check on whether `adb_client` can open the `localabstract:` stream a scrcpy control channel
   needs; `adb_client` has `Forward/Reverse` local commands (server-mode) but the direct
   `ADBTcpDevice` (no-server) generic-stream support is unconfirmed. If it can't, options: a
   persistent-shell input pipe (won't fix the JVM-cold-start latency), `sendevent`, or bundle
   scrcpy differently. Write `FAST-REMOTE-PLAN.md` if you can't validate without a device.
2. **SPAKE2 pairing** — for new Google-TV devices (legacy Shields don't need it). Clean-room from
   Apache-2.0 AOSP pairing sources; plan first if risky.
3. **SAF push picker + Google Drive sync** — Files/Backups are app-scoped-storage only for now.
4. **Polish** — subset the **5.3 MB Material Symbols font** to the ~40 icons actually used
   (`grep 'class="msr"'` → pyftsubset); the debug APK is ~386 MB (release strips + should minify +
   per-ABI split). Disable autocorrect/autocapitalize on the **license-key input** (More screen) —
   autocorrect fights the key entry.
5. **Release prep** — signing keystore, versionCode process, own-site APK distribution (M6), the
   third-party-licenses screen. `ARCHITECTURE-REVIEW.md` §B4 has details.
6. **Desktop rebrand** to ATV Optimizer is a separate, later migration (MSI UpgradeCode risk) —
   out of scope for mobile.

## 6. OPERATIONS PLAYBOOK (how to build / deploy / test)
Env: `ANDROID_HOME=~/Android/sdk`, NDK `28.2.13676358`, tauri-cli 2.11.x, the 4 android Rust
targets installed. Test device: **Pixel 10 Pro**.

- **Deploy to the phone over WIRELESS ADB, never USB** (Bryan's rule). Phone at `192.168.42.211`;
  its adb-connect port is random per session. To connect: it sometimes auto-connects via mDNS
  (`adb-58040...._adb-tls-connect._tcp`); otherwise `adb pair 192.168.42.211:<pairport> <6-digit>`
  (Bryan reads the code+port off the phone's Wireless-debugging screen), then `adb connect
  192.168.42.211:<connectport>`. The phone auto-locks/sleeps off USB power and drops the
  connection — this is a recurring friction; when it's offline just wait/ask Bryan to nudge it.
- **Build APK** (~2-4 min, run backgrounded): `cd v2/mobile && PATH="$ANDROID_HOME/platform-tools:$PATH"
  NDK_HOME="$ANDROID_HOME/ndk/28.2.13676358" npx tauri android build --apk --debug --target aarch64`.
  Do a `gradlew clean` first if stale native libs linger. Kotlin only compiles in this build.
- **Install/launch**: `adb -s <serial> install -r -d <apk>`; `adb -s <serial> shell am start -n
  com.atvoptimizer.mobile/.MainActivity`. First launch after install may need a second `am start`.
- **Screenshots**: `adb -s <serial> exec-out screencap -p > shot.png` then Read it. Can't get past
  the secure lock — ask Bryan to unlock. WebView is opaque to `uiautomator`, so blind `input tap`
  by pixel coords is unreliable (device 1080×2410; the Read tool shows images at 896×2000 → ×1.21);
  prefer having Bryan drive flows that need the TV's "Allow" prompt.
- **Debug logging (built)**: Rust `tracing` → logcat tag `RustStdoutStderr` + a `debug.log` file in
  the app data dir (pull via `run-as com.atvoptimizer.mobile`), surfaced via `read_debug_log` in the
  More screen. macOS has NO `timeout`; quote logcat tag filters in zsh (`'Tag:V' '*:S'`).
- **Fast device-less UI loop**: `npm run build`, serve `build/` on a port, Playwright headless
  (chromium installed) at 384×812, stub `window.__TAURI_INTERNALS__.invoke`. Validates layout/flows
  without a device (safe-area reads 0 in browser, so still spot-check on device).
- **GATES (all must pass)**: from `v2/`: `cargo fmt --check`, `cargo clippy -p shield-optimizer-core
  -p shield-optimizer-v2 -p atv-optimizer-mobile -p tauri-plugin-atv-adb --all-targets -- -D warnings`,
  `cargo test -p shield-optimizer-core`; from `v2/mobile/`: `npm run check` (0/0) + `npm run build`.

## 7. Invariants / conventions (from CLAUDE.md + the review)
Engine (`crates/core/src/engine/`) stays pure (no I/O). One `AdbDriver` seam (desktop `SubprocessAdb`,
mobile `WirelessAdb`). One detection/safety function — `engine::safety` gates every disable path;
`require_pro(Feature)` runs AFTER the safety check. App lists in JSON, not code. Tauri commands
return `Result<T,String>`. Svelte 5 runes only. **Never fake data.** Commits: new commit each time,
no `Co-Authored-By`. Command arg names: camelCase in TS → Tauri maps to snake_case; `pkg`→`package`.

## 8. How Bryan wants work done
Aggressive **parallel Opus** sub-agents for independent work; action over asking; commit/push
checkpoints. But sequence work that shares hot files (the transport `wireless_adb.rs` and the shared
frontend `api.ts`/`router`/`App.svelte`) — parallel agents corrupt those. On-device screenshot/verify
stays on the main thread (agents can't drive the phone). There is a **nightly cloud routine** intended
(midnight = cron `0 5 * * *` UTC) that runs `CLOUD-TASK.md` against this branch — note the RemoteTrigger
tool has a low body-size limit, so its prompt is a short pointer; set it up via https://claude.ai/code/routines.

## 9. Commit history (this effort, newest first)
```
bd6cbd8 CLOUD-TASK: mark transport swap + Phase 6 done; queue remaining
44579c1 File transfer + Backups screens (adb_client push/pull)
fdeb62d Phase 6 screens (Launcher, Tweaks, Snapshots, Devices, App detail, Risk guide, Reconnect)
3e8bcfc Transport: replace GPL libadb-android with pure-Rust adb_client (MIT)
e6dd7f5 in-app BrandMark, soft-EOF workaround, transport/licensing research
b7c3d75 Re-architecture: reliability, shared foundation, real Pro, icon
8bb6b17 architecture review + reboot/stream-close fixes
3ef29de storage regression fix (%-anchored df parse)
88f7123 / 7c17c41 / 9efde3d icon iterations
41d1dc7 Apps cutoff fix
ac9c3cd onboarding design
16e5c42 (earlier) extract shared core workspace
```
Immediate next action: resume the **fast-remote spike** (§5.1).

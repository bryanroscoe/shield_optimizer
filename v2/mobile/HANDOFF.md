# ATV Optimizer (mobile) — HANDOFF

Read this top-to-bottom before doing any mobile work. It is the authoritative, current handoff
(lifecycle/current-authority corrections 2026-09-05 and local-integration review addendum
2026-09-08 America/Chicago; older checkpoints are historical).
Companion deep-dives (all in this dir): `ARCHITECTURE-REVIEW.md` (historical findings audit),
`FEATURES.md` (historical screen ↔ command map), **`BACKLOG.md` (current ordered queue)**,
`TRANSPORT-LICENSING-RESEARCH.md` (why the transport is what it is), `CLOUD-TASK.md` (brief for the
nightly cloud agent). Cross-session memory also lives
in `~/.claude/projects/-Users-bryanroscoe-Developer-shield-optimizer/memory/`.

Current lifecycle evidence baseline: **`48cba23`**, Navigator workspace `crew/navigator`.
The original mobile effort used `feat/atv-optimizer-mobile`; do not infer current
branch or release state from that historical name. Bryan's later feedback records
an installed debug build from `main` at `44d2d66` (see `USER-FEEDBACK-2026-09-05.md`).

**Current authority (`so-vtm.8`): host-only review/evidence/docs, no device attachment
or mutation, no speculative persistence, no commit/push/deploy.** This supersedes
older phone-interaction and shipping instructions below. Mechanic owns integration
and Sol file reservations; Navigator owns mobile UX/lifecycle evidence and docs.
See **[`LIFECYCLE-EVIDENCE.md`](LIFECYCLE-EVIDENCE.md)** for the reproducible browser
matrix and the owner-driven physical gate. Feedback #7 remains physically unresolved.

**Pending local navigation candidate (`so-vtm.3`):** mechanic accepted the separate
three-file Onboarding/session-test/feedback patch with 20 passing worker browser
tests. Saved-TV success opens Dashboard directly after profile resolution;
first-time/add-TV still uses the confirmation screen. Navigator's lifecycle matrix
was run on baseline `48cba23`, not that candidate; its saved-TV Open dashboard
observations must not be described as candidate behavior. No prior-screen
persistence, physical validation, or publication is implied. See the evidence
report for the preserved patch and provenance.

---

## 1. What this is
A phone/tablet app (**Tauri 2 + Rust + Svelte 5**, in `v2/mobile/`) that drives an **Android TV
over wireless ADB** — no PC. Debloat / optimize / launcher / tweaks / snapshots / remote, etc.
Monetized **freemium**; the owner (Bryan) **wants to sell it** (this drove the transport rewrite —
see §3). Product name **ATV Optimizer**. It shares the audited engine (`v2/crates/core`) with the
mature **desktop** app (`v2/src-tauri` + `v2/src`); the rule is *reuse the desktop, keep core
aligned, never fork it*.

## 2. Implementation checkpoints (validation applies to the cited checkpoint)
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
- **Licensing**: `activate_license` persists then flips backend entitlement;
  `LOCKED:<feature>` errors route to PaywallSheet. Signed offline validation is
  implemented (see the 2026-09-04 checkpoint below); `ATVOPT-PRO-2025` is a
  **debug-build-only** test key. Checkout/key issuance remains release work.
- **Icon + branding**: launcher icon and in-app BrandMark are the designer's exact glyph — a
  TV/monitor on a stand with three **vertical faders** (exact SVG in `src-tauri/icons/icon.svg`).
- **Historical device evidence**: earlier Pixel/Shield sessions connected through the new
  transport and exercised specific flows. Bryan later reported usability issues on the
  debug build from `main` at `44d2d66`. Neither establishes complete current physical
  validation; see `USER-FEEDBACK-2026-09-05.md` and the remaining device gates.
- **Fast remote Phases 1–3**: a pinned generic raw ADB service stream, embedded scrcpy v3.1 server,
  Android session registry/lifecycle, and transparent same-request shell fallback are implemented.
  The Bedroom Shield gate measured 30 warm channel presses at 75.2 ms p95 and app force-stop left
  no resident server/socket. Full results and remaining gates are in `FAST-REMOTE-PLAN.md`.
- **Correctness checkpoint (2026-07-13)**: serial-bound ADB operations, serialized connection
  lifecycle, finite liveness-probe reads, acknowledged reboot semantics, non-evicting local file
  failures, disconnect/auto-reconnect and stale-cache race fixes, fail-closed app safety UI,
  snapshot cross-device warnings, secret/remote-text log redaction, transactional live entitlement,
  and a working Optimize apply/progress path. Exact scope and remaining findings are in
  `BACKLOG.md`.
- **Follow-up correctness checkpoint**: direct ADB uses tested shell-v2 stderr/exit framing;
  SmartTube backup discovery covers its current Documents export path (GitHub #86); desktop file
  browsing discards stale responses; and Android fast-remote failure cleanup plus hold-repeat
  backpressure are hardened.
- **State-isolation checkpoint**: malformed saved TVs are normalized safely; Diagnostics labels
  stale metrics after a failed refresh and isolates safety lookups; Tweaks requests cannot overwrite
  another device, mutations capture their target serial, and partial failures force a real reload.
- **APK backup correctness**: new backups contain every `pm path` APK in a versioned bundle and can
  be restored together; restore paths are confined to app storage, remote staging is always cleaned,
  and legacy base-only files remain visible but are explicitly not restorable.
- **APK size polish**: `scripts/subset-material-symbols.py` derives the bundled icon font from Svelte
  references and validates retained ligatures (5.34 MB to about 111 KB). The license-key field also
  disables mobile autocorrect, spellcheck, and capitalization.
- **Background remote lifecycle checkpoint (2026-08-01)**: Android keeps the fast-remote channel for
  a 30-second background grace, cancels stale cleanup on resume, and serializes session startup with
  teardown. Host/frontend gates, the aarch64 APK build, and Android-target clippy are green; the
  physical two-Shield grace/concurrency matrix is still required.
- **Connection and device-history polish (2026-08-01)**: first-time authorization now waits up to
  30 seconds and turns transport failures into guidance to approve the TV's "Allow debugging?"
  prompt. Saved TVs retain their last real friendly name when ADB ports rotate, keep 16 recent
  hosts, and can be selected directly from the Dashboard device menu. Diagnostics now charts used
  RAM so the bar fills as memory usage increases.
- **Focused UI validation and cached-label follow-up** (`b293b66`, `3eccc34`): a 384×812
  Playwright/WebView pass exercised the real Svelte screens with Tauri invokes stubbed. It verified
  the previous-TV menu, a successful TV switch, the authorization-prompt guidance, and a
  `3072 / 4096 MB used` RAM bar at 75%. That pass caught one real gap: after switching, the header
  could fall back to the generic model name. `3eccc34` makes the session label prefer a reported
  friendly name, then the durable cached name. `npm run check` remained 0/0 and `npm run build`
  passed after the fix.

- **Stability reset (2026-09-04)** — a four-track code audit (feature parity vs v1/desktop,
  connection lifecycle, screen-by-screen UX, Rust backend) followed by a fix sweep. Root causes of
  the reported "looks connected but isn't / laggy / reconnects too often" symptoms were in the
  transport, not the UI:
  - **Infinite reads.** The vendored `adb_client` default read timeout was `u64::MAX`, and every
    normal shell call used it while holding the single connection mutex. A silently dropped
    socket (AP roam, TV asleep) froze every later command and the liveness probe behind it. Now:
    30 s inactivity per shell response (180 s for installs/clears), a 120 s safety net inside the
    vendored crate, a 5 s TCP connect timeout, and only socket-level errors evict the connection
    (a missing remote file used to disconnect the TV).
  - **Identity behind the device mutex.** `wireless_status`/`list_devices` blocked a tokio worker
    while any command ran. The identity is now mirrored outside the mutex.
  - **Poisoned mutex = brick.** A short sync packet could panic under the lock; connect then failed
    forever with "lock poisoned". Locks recover, and the two slices are bounds-checked.
  - **Stray packets killed the connection.** A late CLSE/WRTE from a finished stream failed the
    next command; readers now skip a bounded number of foreign-stream messages.
  - **Probe race.** The status probe issued a second disconnect after eviction, which could drop a
    reconnect to the same host that had just succeeded. Removed.
  - **Serialized fan-out.** health/apps/optimize/launcher/display reads are one sentinel-batched
    shell each (`crates/core/src/adb/batch.rs`), 19 → 6 round-trips on the hot paths.
  - **Frontend liveness.** Any command error carrying the backend's "Connection to the TV was
    lost" prefix flips the session (`lib/connectionEvents.ts`), which silently redials the same TV
    once and only then shows the global `ConnectionBanner` (Retry / Switch TV). A 45 s heartbeat
    probes while the app is visible.
  - **No surprise auto-connect.** Launch shows a saved-TV picker; it auto-dials only when exactly
    one TV is saved and the last session wasn't ended by an explicit Disconnect (persisted in
    `atv.autoConnect.v1`), always naming the target with a Cancel button. "Add a TV" from Devices is
    a separate `addtv` route that never dials. The Android back button keeps exactly one spare
    history entry, so it no longer eats dead presses at the root.
  - **Honest state.** Dashboard shows the real "N of M recommended-bloat apps active" instead of a
    floored invented score; screenshots preview inline; Settings (was "More Options") gained
    Emergency recovery (`panic_recovery`, previously registered but unreachable), reboot to
    recovery/bootloader, and confirmations on Disconnect.
  - Entitlement default is now Free (fail-closed); desktop opts into Pro explicitly.
  Screen-level fixes from the same sweep are listed in `BACKLOG.md` under "Stability reset".

- **Backlog pass (2026-09-04, same day)** — device-less P1/P2 items:
  - **Signed licensing.** `ATVOPT-<payload>-<signature>` Ed25519 keys (Crockford base32,
    case-insensitive), verified offline against embedded public keys; the dev key
    `ATVOPT-PRO-2025` works in **debug builds only**. `tools/atvopt-license` (`keygen` / `issue` /
    `verify`); private keys live in `~/.atvopt/` outside the repo. Settings shows licensee + term
    via `license_info`. See `LICENSING.md`.
  - **Release plumbing.** Gradle release signing from `keystore.properties` or `ATVOPT_KEYSTORE_*`
    env, `scripts/bump-version.sh`, `cargo about` notices (no GPL family in 293 crates) shown under
    Settings › About. See `RELEASE.md`.
  - **Follow-ups.** `remote_warm` starts the fast-remote channel when the Remote tab opens;
    snapshot reads batched; `pull_file` capped at 2 GiB with de-duplicated names; device profile
    harvests `ro.serialno`. In the Mechanic-local `so-fb3.1.2` integration, equal hardware IDs are
    the only cross-endpoint saved-TV identity and ID-less rows match only exact host+port. Conflicting
    or newly identified histories remain separate, so two rows can share an address and single-TV
    auto-dial is then suppressed. The integrated-local `so-fb3.1.3` follow-up now keys progress and
    errors by saved identity, filters the current TV by verified hardware ID or exact ID-less endpoint,
    shows host:port plus neutral shared-address guidance, and forgets the selected identity. Mechanic's
    combined saved-device/diagnostics tests passed 25/25, mobile check reported 0 errors/0 warnings,
    and the build passed; browser, device, publication, and release remain unverified.
    Closing out `so-fb3.1`, the scan-row reconciliation (`lib/discoveryRows.ts`) now has its own
    pure test suite (11 cases: repeated advertisements, mixed saved/new hosts, live-endpoint
    identification, address and port changes, pairing-only hosts, colliding saved identities,
    malformed records, `adb-…` instance names, legacy vs TLS ports). A saved endpoint whose address
    answered the scan on a different port reports `saved-other-port` ("This address answered on
    another port") instead of claiming it was not found — a rotated wireless-debugging port is not
    evidence the TV is offline. Desktop was checked and is not affected: it enumerates devices
    through the adb binary's own serials and keeps no saved-network-device list. Mobile gates:
    tests 78/78, check 0 errors/0 warnings, build passed. Physical device verification of the
    reconnect flow is still the open gate on the parent bead.
  - **Code pairing (SPAKE2) — implemented, unverified on a device.** Clean-room Rust in the
    vendored crate (`vendor/adb_client/src/message_devices/tcp/pairing/`), wired through
    `WirelessAdb::pair` and the Onboarding pair step. Key finding: BoringSSL's SPAKE2 is a bespoke
    variant (32-byte messages, SHA-512 transcript, its own M/N seeds, cofactor tricks) that is
    wire-incompatible with the RustCrypto `spake2` crate, so the variant is implemented directly on
    `curve25519-dalek` and checked against an independent Python model plus a full loopback.
    Every protocol constant is cited to an AOSP file in `PAIRING-PLAN.md`, which also lists the
    exact manual test (the Pixel's own pairing service first, then a Google TV). Also fixed a
    pre-existing encoder bug: `android_pubkey_encode` dropped a zero top byte (~1 key in 256
    rejected by adbd for AUTH and pairing).

- **Navigator-reviewed local integrations (2026-09-08; not released):**
  - **Canonical Unknown safety (`so-fb3.2.1`–`.2.4`).** Mechanic combined the accepted shared
    contract, mobile rev3 consumers, desktop round-three consumers, and Navigator documentation in
    `crew/mechanic`. Canonical safety now preserves Protected and Caution precedence and otherwise
    reports Unknown with a reason; malformed, failed, or stale lookup remains unavailable rather than
    becoming Unknown. Mobile and desktop removal paths require current installed-package identity and
    canonical evidence, do not auto-select Unknown, and keep unresolved memory/process rows inspect-only.
    Mechanic's integrated gates passed: core 199 tests plus clean clippy, desktop Tauri 36 tests, mobile
    tests 25/25 with check 0/0 and build, desktop check 0/0 and build, and independent mobile 13/13 and
    desktop 25/25 harnesses. `so-fb3.2.1` through `.2.4` are closed; the parent remains open for
    actual process attribution. Browser sessions, desktop gallery regeneration, cross-OS CI, device,
    publication, and release remain unverified. Exact hashes are recorded in
    `mayor/artifacts/hq-b1t/shield-so-fb3.2-integration-record.txt`.
  - **Unknown app diagnostics (`so-fb3.6.1`).** The mobile Apps screen records completed,
    current-session uncatalogued-package results in a bounded local-only collector. Settings can
    review a refreshable JSON snapshot, copy it explicitly, and clear it after confirmation; the
    copy states that nothing is sent automatically and identifies the fields included before
    sharing. The collector allowlists fields, rejects address/path/control-like tokens,
    deduplicates observations, caps records/bytes/count, and makes clear win over queued saves.
    Mechanic's focused collector tests passed 9/9; isolated TypeScript, Svelte compile, mobile
    check (0 errors/0 warnings), and build passed. Runtime unresolved-process collection remains
    deferred until the parent `so-fb3.2` process-attribution scope supplies typed identity;
    `registry_version` is currently unavailable.
    No browser, device, automatic upload, physical, or release claim.
  - **Frame-rate presentation (`so-vtm.5`).** The misleading numeric `30fps_select` decoration is
    replaced by the already-retained nonnumeric `sync_alt` glyph and marked `aria-hidden`.
    Subordinate copy reports the current Never / Seamless only / Always / device-default policy
    and says matching depends on TV, app, and content support; it does not present measured FPS.
    Mechanic's mobile check passed with 0 errors/0 warnings and build passed. No 384 px render,
    device verification, installed-build confirmation, physical matching claim, or release claim.

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
  (blocking calls via `spawn_blocking`), implementing the core `AdbDriver` trait. The pinned direct
  TCP path now uses shell-v2 framing for real stdout/stderr/exit status, with shell-v1 fallback for
  older devices; the Bedroom Shield live test returned separate output streams and exit code 7.
  A persistent **RSA key is generated/persisted in `app_data_dir`**
  (`ensure_adb_key`, PKCS#8 PEM) — first connect prompts "Allow debugging" on the TV, then silent.
- The **Kotlin plugin (`tauri-plugin-atv-adb`) is now mDNS-discovery ONLY** (NsdManager, a pure
  Android framework, no GPL). libadb/Conscrypt/BouncyCastle Gradle deps + the jitpack repo are gone.
- **Legacy network-debugging (`:5555`, no pairing code) works now** — that's the proven path.
- **Code pairing:** the vendored crate now implements the Android-11 SPAKE2 flow and
  `wireless_pair` is wired to it. Host model/loopback checks exist; real adbd pairing remains
  unverified. Use `PAIRING-PLAN.md` for the physical gate and Network debugging as the
  fallback. Generated third-party notices are available in Settings › About.

## 4. Repo layout (mobile)
- `v2/mobile/src/` — Svelte 5 frontend. `lib/{api,types,session.svelte,router.svelte,log,savedDevices}.ts`,
  `screens/*.svelte` (14), `components/*.svelte` (7), `app.css` (design tokens + offline @font-face).
- `v2/mobile/src-tauri/src/` — mobile Tauri app: `lib.rs` (builder + `generate_handler!`),
  `wireless_adb.rs` (adb_client transport = `AdbDriver`), `wireless_commands.rs` (wireless_*),
  `file_commands.rs` (list_remote_dir/pull_file/backup_apk/list_backups).
- `v2/mobile/tauri-plugin-atv-adb/` — mDNS discovery plugin (Kotlin NsdManager + thin Rust).
- `v2/vendor/adb_client/` — vendored MIT transport; `SHIELD-OPTIMIZER-PATCH.md` lists every local
  change (raw service stream, finite timeouts, stray-stream tolerance, `tcp/pairing/`).
- `v2/tools/atvopt-license/` — license key generator/issuer/verifier CLI.
- `v2/crates/core/src/adb/batch.rs` — sentinel-batched shell helper; `crates/core/src/license.rs`
  — signed license verification.
- Mobile docs: `HANDOFF.md` (this), `BACKLOG.md`, `FAST-REMOTE-PLAN.md`, `PAIRING-PLAN.md`,
  `LICENSING.md`, `RELEASE.md`, `THIRD-PARTY-NOTICES.md` (generated), `CLOUD-TASK.md`.
- `v2/crates/core/` — SHARED engine+commands (pure `engine/`, `commands/*`, `adb/{driver,parse}`,
  `license.rs`). Desktop and mobile both register from here. **Keep `engine/` pure; keep aligned
  with desktop.**
- Design source of truth: `~/Downloads/Shield Optimizer mobile design (1)/ATV Optimizer Mobile.dc.html`
  (20 screen sections; lime `#C9F24E`, Geist/Geist Mono). The `claude_design` MCP is NOT available
  in this environment — work from the downloaded folder.

## 5. What REMAINS

Use **[`BACKLOG.md`](BACKLOG.md)** as the ordered source of truth. Earlier host/APK
checkpoints are recorded in `CORRECTNESS-AUDIT-2026-09-05.md`; Bryan's later
`44d2d66` install is owner-reported, not a completed physical stability matrix.
The current next steps are:

The 2026-09-08 integrations above exist only in Mechanic's local integration workspace.
Do not describe any as shipped. The diagnostics report has no automatic network path; explicit
clipboard JSON is the current export. Keep unmatched-process collection pending until canonical
typed identity exists instead of inferring process/package identity in the frontend.

1. **Keep restart feedback #7 open on `so-vtm.8`.** Navigator's 13 focused browser
   lifecycle cases, 9 existing regressions, mobile check/build, and 3 pure lifecycle
   tests passed at `48cba23`. Visibility-only resume retained the screen; JavaScript
   reconstruction reset it and required a fresh connection/Connected confirmation.
   Use `LIFECYCLE-EVIDENCE.md` for the precise limits and minimal owner report.
   Physical investigation is gated; do not pair, install, or interact with a device
   under the current host-only assignment.
2. **Verify code pairing on a real device** (BACKLOG P1 #2). Follow `PAIRING-PLAN.md` exactly:
   the Pixel's own `_adb-tls-pairing` service from a host binary first, then a Google TV, including
   the wrong-code and silent-reconnect checks. Until this passes, treat pairing as unverified and
   keep the Onboarding copy pointing users at Network debugging as the fallback.
3. Finish the fast-remote Phase 4 device gates (`FAST-REMOTE-PLAN.md`).
4. SAF import/export for screenshots, pulled files and APK bundles (they land in app-private
   storage today), then Drive.
5. Product decisions for Bryan, listed under BACKLOG P1 #6: free read-only Optimize plan; the
   unused `Feature::FileManager` / `Feature::BackupClone` gates.
6. Release: create the real keystore (or enroll Play App Signing), a signed-AAB CI job, and a
   checkout/store that issues license keys with `tools/atvopt-license` (`RELEASE.md`,
   `LICENSING.md`). Back up `~/.atvopt/license-signing-key.prod`.

Desktop rebranding remains a separate migration because of the MSI UpgradeCode risk.

### Next-agent start checklist (updated 2026-09-05)

- Record the actual branch, HEAD, and dirty paths; preserve existing work and respect
  mechanic's file reservations. Read `BACKLOG.md` (P0 first), `LIFECYCLE-EVIDENCE.md`,
  `PAIRING-PLAN.md` if touching pairing, and the Phase 4 section of `FAST-REMOTE-PLAN.md`.
- Gates (all must pass; run from `v2/`): `cargo fmt --check`; `cargo clippy -p shield-optimizer-core
  -p shield-optimizer-v2 -p atv-optimizer-mobile -p tauri-plugin-atv-adb -p atvopt-license
  --all-targets -- -D warnings`; `cargo test -p shield-optimizer-core -p atv-optimizer-mobile
  -p atvopt-license` (179 / 21 / 2 at handoff); `cd vendor/adb_client && cargo test --lib` (54;
  the vendored crate is outside the workspace). From `v2/mobile`: `npm run check` (0/0) and
  `npm run build`. Android-target clippy works with the NDK clang exported as
  `CC_aarch64_linux_android` / `CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER`.
- Reproducible device-less UI loop from `v2/mobile/`: `npm test` runs the existing browser
  regressions; `node --test evidence/lifecycle/reproduce.mjs` runs the focused lifecycle matrix.
  Both use mocked Tauri on Vite-compiled source. Run `npm run check` and `npm run build`
  separately. These do not verify native Android callbacks or an installed APK.
- If you add a Material Symbols icon name, rerun `scripts/subset-material-symbols.py` (needs
  `fonttools` + `brotli` in a venv) or the icon renders as literal text. If you add a crate,
  rerun `scripts/gen-notices.sh`.
- The dev license key `ATVOPT-PRO-2025` unlocks Pro in **debug builds only**. For a release build
  issue a real key: `cargo run -p atvopt-license -- issue --key ~/.atvopt/license-signing-key.prod
  --licensee "you"`.
- Hot files that must not be edited by parallel agents at the same time: `wireless_adb.rs`,
  `session.svelte.ts`, `App.svelte`, `router.svelte.ts`, `api.ts`, `types.ts`, mobile `lib.rs`.
  The 2026-09-04 pattern that worked: one lead owns those, parallel agents own disjoint screens.
- Preserve the unrelated untracked root files (`atv-optimizer-android-strategy.html`, root
  `node_modules/`, `package.json`, and `package-lock.json`); they are user-owned and not part of the
  mobile commits.
- A future authorized physical session must identify the installed build and use current
  wireless endpoints. **No device interaction is authorized by the current assignment.**
  Do not reuse recorded ports or treat this historical playbook as permission.
- Do not mark Phase 4 or pairing complete from host tests or logs alone.

## 6. OPERATIONS PLAYBOOK (historical device commands; authorization required)

The device commands below are reference only during `so-vtm.8`; do not execute them
under its host-only scope. Recorded addresses, tool versions, and pairing state are
historical observations, not current device discovery.

Env: `ANDROID_HOME=~/Android/sdk`, NDK `28.2.13676358`, tauri-cli 2.11.x, the 4 android Rust
targets installed. Test device: **Pixel 10 Pro**.

- **Deploy to the phone over WIRELESS ADB, never USB** (Bryan's rule). Phone at `192.168.42.211`;
  its adb-connect port is random per session. To connect: it sometimes auto-connects via mDNS
  (`adb-58040...._adb-tls-connect._tcp`); otherwise `adb pair 192.168.42.211:<pairport> <6-digit>`
  (Bryan reads the code+port off the phone's Wireless-debugging screen), then `adb connect
  192.168.42.211:<connectport>`. The phone auto-locks/sleeps off USB power and drops the
  connection — this is a recurring friction; when it's offline just wait/ask Bryan to nudge it.
  The Pixel was successfully re-paired on 2026-08-16 and was reachable then, but both pairing and
  connect ports rotate; always rediscover rather than reusing a recorded endpoint.
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

## 8. Current coordination

Persistent Navigator owns mobile UX, independent lifecycle/regression evidence,
and mobile documentation. Mechanic owns device architecture and integration;
Sol implements bounded changes with explicit file reservations. Preserve existing
work. This assignment permits no device attachment/mutation, commit, push, deploy,
or speculative session persistence. Earlier parallel-Opus/cloud-routine and
commit/push guidance is historical and does not expand current authority.

## 9. Commit history (this effort, newest first)
```
fae340c Mobile: document the licensing, release, and pairing backlog pass
5e76b24 Mobile: Android 11 wireless-debugging code pairing (SPAKE2)   ← unverified on device
aa4e589 Mobile: release signing config, version tooling, third-party notices
5d52e51 Mobile: warm the remote channel, batch snapshot reads, key saved TVs by serial
78bb09d Licensing: Ed25519-signed offline license keys
0c24761 Mobile: document the stability reset and next physical checks
f3031c6 Mobile: screen correctness and honesty sweep
3e7cb3b Mobile: explicit reconnect, live connection state, honest dashboard
e51fc3f Mobile: make the ADB transport fail fast and honestly
334fda3 Vendor adb_client: bound every read and tolerate stray stream packets
bb33ecc Mobile: refresh agent handoff and device plan                   ← last APK on the Pixel
3eccc34 Mobile: retain cached TV labels after switching
b293b66 Mobile: clarify TV connections and device switching
5070840 Mobile: log remote lifecycle transitions
00aa8e6 Mobile: hand off background lifecycle checkpoint
53c975e Mobile: expire remote sessions after background grace
48b3574 Mobile: add fast scrcpy remote channel
988facc Mobile: add raw ADB service stream for fast remote
a9d2c8b HANDOFF: comprehensive current-state handoff for the next agent
3e8bcfc Transport: replace GPL libadb-android with pure-Rust adb_client (MIT)
b7c3d75 Re-architecture: reliability, shared foundation, real Pro, icon
16e5c42 (earlier) extract shared core workspace
```
Immediate next action: review `LIFECYCLE-EVIDENCE.md`, keep `so-vtm.8` open for the
owner-driven physical reproduction gate, and advance independently authorized host
work. Do not re-pair or install on the Pixel under this assignment.

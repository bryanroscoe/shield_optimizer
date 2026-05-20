# v2 Porting Plan

Phased roadmap for bringing v2 to feature parity with v1, then beyond. Each phase produces a shippable artifact and is gated by parity tests against v1 behavior.

Section references (§X.Y) point at [`../docs/FEATURES.md`](../docs/FEATURES.md).

## Guiding principles

- **One PR per feature**, not per phase. Phases are organizational, not all-or-nothing.
- **Parity tests precede ship.** Before claiming a feature is ported, the engine has a test (mock ADB driver, fixed inputs, asserted plan); the UI has a manual exercise pass against the real Shield at 192.168.42.71.
- **Don't fork before parity.** v1 stays alive and accepts patches until v2 is at parity. The behavior spec is shared; if v1 evolves, v2 follows.
- **The catalog is the contract.** When v2 does X differently from v1, update [`docs/FEATURES.md`](../docs/FEATURES.md) in the same PR that introduces the change.

## Phase status legend

| Status | Meaning |
|---|---|
| ⏳ Pending | Not started |
| 🛠 Active | In progress |
| 🧪 Review | Code complete, awaiting parity verification |
| ✅ Done | Parity confirmed against v1 |

---

## Phase 1 — Foundation

**Goal:** Project compiles, opens a window, can talk to a Shield via ADB, has a working test harness.

| # | Item | v1 ref | Status |
|---|---|---|---|
| 1.1 | Tauri 2 scaffold + frontend (Svelte default) | — | ⏳ |
| 1.2 | Cargo workspace layout: `engine/` crate, `adb/` crate, `src-tauri/` (commands), `data/app-lists/` | §0, §16 | ⏳ |
| 1.3 | ADB driver: subprocess wrapper, structured output type, error decoding | §16.1, §16.6 | ⏳ |
| 1.4 | Platform detection (Windows / macOS / Linux) for ADB binary path resolution | §0.2 | ⏳ |
| 1.5 | Bundled `platform-tools` for each target + download path for `-ForceAdbDownload` equivalent | §1.5 | ⏳ |
| 1.6 | Test harness: mock ADB driver | — | ⏳ |
| 1.7 | **Fixture capture pass** — checked-in dumpsys / settings / pm-list outputs from the real Shield at 192.168.42.71, used as the test corpus throughout the rest of the phases | — | ⏳ |
| 1.8 | CI: GitHub Actions builds on ubuntu/macos/windows, runs tests | — | ⏳ |
| 1.9 | **Pin frontend framework choice** (default Svelte) before shipping the first 3 views — switching frameworks after that is the expensive moment | — | ⏳ |

Ship target: developers can run `npm run tauri dev`, see a window, click a button that runs `adb devices` against a real Shield and renders the result. Fixture corpus is checked in so subsequent phases can write engine tests without device access.

---

## Phase 2 — Discovery & profile (read-only)

**Goal:** Display a usable device list. No write operations yet.

| # | Item | v1 ref | Status |
|---|---|---|---|
| 2.1 | `adb devices` polling → typed `Device { serial, status, connection_type }` | §1.6 | ⏳ |
| 2.2 | Batched property query (5 props in one shell call), build full `Device` | §1.6 | ⏳ |
| 2.3 | Device type detection — **single** function consolidating v1's two paths | §13.1 | ⏳ |
| 2.4 | Shield codename → friendly model map (mdarcy/sif/darcy/foster) | §1.6, §13.2 | ⏳ |
| 2.5 | Frontend: device list with `[NET]`/`[USB]` tags, UNAUTHORIZED handling | §2.2, §1.7 | ⏳ |
| 2.6 | Profile view: getprop dump + app-list breakdown | §13.2 | ⏳ |
| 2.7 | Connect by IP form with validation | §1.2 | ⏳ |
| 2.8 | PIN pairing (Android 11+) wizard | §1.3 | ⏳ |

Ship target: parity with v1's main menu + device profile.

---

## Phase 3 — Network discovery

**Goal:** Auto-find devices on the LAN.

| # | Item | v1 ref | Status |
|---|---|---|---|
| 3.1 | Default gateway detection per OS | §1.1 | ⏳ |
| 3.2 | Parallel /24 ping scan | §1.1 | ⏳ |
| 3.3 | ARP table parse per OS | §1.1 | ⏳ |
| 3.4 | `adb connect` per candidate, classify | §1.1 | ⏳ |
| 3.5 | `--subnet` override (CLI flag or settings entry) | §0.1 | ⏳ |
| 3.6 | **Stretch:** mDNS discovery (`_androidtvremote2._tcp`, `_adb-tls-pairing._tcp`) to skip IP entry on Android 11+ | (new feature, see research pass) | ⏳ |

Ship target: parity with v1 Scan Network + the mDNS improvement that v1 doesn't have.

---

## Phase 4 — Health report

**Goal:** Read-only diagnostic view including the v0.75.0 Display & Audio additions.

| # | Item | v1 ref | Status |
|---|---|---|---|
| 4.1 | Batched dumpsys query (thermal + meminfo + storage + props + settings + packages) | §5.1 | ⏳ |
| 4.2 | Parse thermal output (multiple format variants) | §16.7 | ⏳ |
| 4.3 | Parse meminfo (free/used/total/swap MB) | §16.7 | ⏳ |
| 4.4 | Top memory users (system-wide map, base-package summing) | §5.1, §16 | ⏳ |
| 4.5 | Vital color thresholds (temp/RAM/storage/AppMemory) | §16.7 | ⏳ |
| 4.6 | Display mode (resolution, refresh, HDR types) | §5.3 | ⏳ |
| 4.7 | Audio device | §5.3 | ⏳ |
| 4.8 | Bloat check table | §5.1 | ⏳ |
| 4.9 | Live monitor refresh loop | §5.2 | ⏳ |

Ship target: parity with v1 Health Report and Live Monitor.

---

## Phase 5 — App lists & optimize/restore (first write paths)

**Goal:** Core debloat functionality.

| # | Item | v1 ref | Status |
|---|---|---|---|
| 5.1 | App list JSON schema + bundled defaults (CommonApp/Shield/GoogleTV) | §15.1-§15.4 | ⏳ |
| 5.2 | Runtime fetch of latest app lists from `gh-pages` or similar | (new) | ⏳ |
| 5.3 | App-list filter by device type | §13.2 | ⏳ |
| 5.4 | Optimize plan generation (no I/O): for each app, determine action | §4.1 | ⏳ |
| 5.5 | Optimize execution: `pm disable-user --user 0`, `pm uninstall --user 0` | §4.1 | ⏳ |
| 5.6 | Per-app memory annotation from cached map | §4.1, §5.1 | ⏳ |
| 5.7 | Risk tiers + default action UI surfacing | §4.1 | ⏳ |
| 5.8 | Restore mode (`pm enable`, `cmd package install-existing`) | §4.1 | ⏳ |
| 5.9 | Play Store fallback for missing APKs | §16.5 | ⏳ |
| 5.10 | Animation triple setting | §4.1 | ⏳ |
| 5.11 | Task summary | §4.2 | ⏳ |
| 5.12 | Recovery (panic re-enable all disabled) | §12 | ⏳ |

Ship target: parity with v1 Optimize / Restore / Recovery — the core value of the tool.

---

## Phase 6 — Launcher management

**Goal:** Full launcher wizard with the v0.75.0 robustness improvements.

| # | Item | v1 ref | Status |
|---|---|---|---|
| 6.1 | Custom launcher catalog (preset + Custom entry) | §6.1 | ⏳ |
| 6.2 | Stock launcher list + safe-handler fallbacks | §6.1 | ⏳ |
| 6.3 | `Get-CurrentLauncher` equivalent (resolve-activity) | §6.3 | ⏳ |
| 6.4 | Launcher activity discovery (`query-activities --components`) | §6.3 | ⏳ |
| 6.5 | `Set-DefaultLauncher` multi-strategy: `pm enable` → role API → set-home-activity (cmd/pm aliases) → HOME-intent kick | §6.3 | ⏳ |
| 6.6 | "Unknown command" detection for unsupported role API | §6.3 | ⏳ |
| 6.7 | Verification by re-resolve after each attempt | §6.3 | ⏳ |
| 6.8 | Captured ADB error surfacing for failures | §6.3 | ⏳ |
| 6.9 | Disable stock launchers wizard (per-launcher prompt) | §6.4 | ⏳ |
| 6.10 | Restore stock launchers | §6.5 | ⏳ |
| 6.11 | Channel-dependency warning (`com.android.providers.tv` check) | §6.6 | ⏳ |

Ship target: parity with v1 Launcher Setup including the hard-won Android-11-on-Shield fixes.

---

## Phase 7 — Tunables & display

**Goal:** Display Scaling and Tweaks.

| # | Item | v1 ref | Status |
|---|---|---|---|
| 7.1 | Display scaling presets (4K / 1080p / Reset) | §8 | ⏳ |
| 7.2 | HDMI-CEC sub-toggles (4 settings) | §7 | ⏳ |
| 7.3 | `match_content_frame_rate` (0/1/2) | §7 | ⏳ |
| 7.4 | `long_press_timeout` (presets) | §7 | ⏳ |
| 7.5 | Generic Bool/Enum/Int setting editor pattern (engine + UI) | §7 | ⏳ |

Ship target: parity with v1 Display Scaling + Tweaks.

---

## Phase 8 — Snapshots

**Goal:** Save / apply state across factory resets and OTAs.

| # | Item | v1 ref | Status |
|---|---|---|---|
| 8.1 | Snapshot schema (v1 / `schemaVersion=1`) | §9.1 | ⏳ |
| 8.2 | Snapshot dir resolution per platform (XDG / `%APPDATA%` / `~/Library/Application Support`) | (new — v1 used `./snapshots/`, v2 should use proper OS dirs) | ⏳ |
| 8.3 | Save: enumerate disabled, current launcher, tracked settings | §9.2 | ⏳ |
| 8.4 | Apply: re-disable from list, set launcher, write settings, summary | §9.3 | ⏳ |
| 8.5 | Cross-device-type warning | §9.3 | ⏳ |
| 8.6 | Snapshot UI: list + preview + confirm | §9.4 | ⏳ |

Ship target: parity with v1 Snapshot/Restore.

---

## Phase 9 — APK sideload & reboot

| # | Item | v1 ref | Status |
|---|---|---|---|
| 9.1 | APK file picker + bulk install | §10 | ⏳ |
| 9.2 | Install error decoding | §16.6 | ⏳ |
| 9.3 | Reboot modes (normal / recovery / bootloader) | §11 | ⏳ |

---

## Phase 10 — Packaging & auto-update

**Goal:** Real distributable artifacts with seamless updates.

| # | Item | v1 ref | Status |
|---|---|---|---|
| 10.1 | `tauri.conf.json` bundler config (icons, identifiers, signing metadata) | (new) | ⏳ |
| 10.2 | **Code signing decision** — macOS: Apple Developer Program ($99/yr) + notarization, no shortcut. Windows: choose one of (a) EV certificate from DigiCert/Sectigo etc. ($300-500/yr, immediate SmartScreen reputation), (b) standard OV certificate ($100-300/yr, ~30k-installs to build SmartScreen reputation), or (c) ship unsigned with documented SmartScreen-bypass instructions. **(c) is risky for the debloater audience** — they already mistrust the tool; SmartScreen "this looks like malware" warnings will tank adoption. Lean (a) or (b); decide before Phase 10 begins. Linux: no signing required, can sign with personal GPG key for repo trust. | (new) | ⏳ |
| 10.3 | Tauri Updater plugin wired against `latest.json` hosted in GitHub Releases | (new) | ⏳ |
| 10.4 | Release pipeline: tag → CI builds installers for 5 platforms → uploads to release. **Pin toolchain versions** (rustc, node, tauri-cli) in CI for reproducibility — a privileged tool's supply chain warrants the discipline | (new) | ⏳ |
| 10.5 | **Update key generation + private-key handling docs.** Critical: Tauri Updater uses Ed25519 signatures, and the private signing key must NEVER rotate without bricking auto-update for the entire installed base on old versions. Document key custody (1Password / HSM), backup strategy, and a one-way "we lost the key" recovery plan (notify users via README, force manual reinstall). | (new) | ⏳ |

Ship target: v2.0.0 desktop GA with auto-update working.

---

## Phase 11 — Mobile (phone client)

**Goal:** Same app on Android, distributed outside Play Store.

> **Reality check before scoping:** an Android app **cannot bundle or exec an `adb` binary** — SELinux + the app sandbox forbid fork-exec of arbitrary binaries from third-party APKs. The Tauri `shell` plugin reflects this; its desktop-flavored subprocess APIs are not available on Android targets. "ADB-over-network from phone" therefore means **reimplementing the ADB wire protocol in Rust** (TLS handshake against the device's `adbd`, RSA key auth, and the sync / shell sub-protocols). That's a non-trivial library, not a flag-flip. Either commit to the protocol implementation or accept that v2.1 is a much larger effort than v2.0. **Do a research spike (11.0 below) before committing to v2.1 timeline.**

| # | Item | v1 ref | Status |
|---|---|---|---|
| 11.0 | **Research spike** — evaluate existing Rust ADB crates (e.g. `adb_client`, `forensic-adb`) for protocol completeness and license fit. Decide: vendor a crate, fork one, or write our own. Land a written decision before opening 11.1. | (new) | ⏳ |
| 11.1 | Tauri 2 Android target builds | (new) | ⏳ |
| 11.2 | Responsive frontend (existing layout works on phone-portrait) | (new) | ⏳ |
| 11.3 | **Pure-Rust ADB client implementation** — TLS connection to `<device>:5555`, Ed25519/RSA key auth (matching desktop ADB's key format so a paired desktop can hand off to mobile), shell exec, output streaming. Plugs into the same `Adb` trait the desktop subprocess driver implements so the engine doesn't know which is which. | (new) | ⏳ |
| 11.4 | APK distribution via GitHub Releases + F-Droid + Obtainium hint | (new) | ⏳ |
| 11.5 | Update flow: check + manual-confirm install (Google's APK install constraint) | (new) | ⏳ |
| 11.6 | iOS build (stretch — App Store also rejects this kind of tool; would be sideload-only) | (new) | ⏳ |

Ship target: v2.1.0 with phone client. **Realistic timeline depends on 11.0 outcome** — if a usable crate exists, 11.x is weeks; if we write our own ADB client, it's months. Treat v2.1 as a separate planning effort once 11.0 lands.

---

## Phase 5 addenda — data additions (folded in from the research pass)

These are *data changes* to the app lists shipping with v2, not separate features. They live in `data/app-lists/*.json` and are picked up by the runtime loader (commitment #2). Lumping them with Phase 5 because the loader is what unlocks them.

- **"Disable Nvidia telemetry" preset** — curated bundle of Nvidia telemetry packages exposed as a one-click preset. Implementation: a new `data/presets/*.json` schema lets the engine compose multiple package-actions into a single named preset; Phase 5 ships at least this one preset to validate the schema.
- **More Shield bloat from florisse.nl** — additional packages catalogued in the [florisse Shield-debloat guide](https://florisse.nl/shield-debloat/) that v1 doesn't have. Update `data/app-lists/shield.json` rather than touching code.

## Out-of-scope ideas (parking lot)

Captured from the research pass; revisit after v2.1:

- **Per-process CPU view** in Live Monitor (`top -n 1 -m 10`)
- **Wi-Fi RSSI / link speed diagnostic** in Health Report (`dumpsys wifi`)
- **Audio passthrough format detection** (multi-format `dumpsys audio` parser)
- **Profiles** — "Privacy" / "Performance" / "Defunct services" preset bundles (the preset *schema* lands in Phase 5; richer profile UX is post-v2.1)
- **Multi-device targeting** — apply same Optimize to a group of devices in one click

---

## Definition of done for v2.0

Before tagging v2.0.0:

- [ ] Every section of [`docs/FEATURES.md`](../docs/FEATURES.md) §0-§16 has a corresponding v2 implementation
- [ ] Each implementation has at least one engine-level test against checked-in dumpsys fixtures from Phase 1.7
- [ ] **Snapshot reader has a rejection-fixture test** — a snapshot with an unknown `schemaVersion` produces a clear, non-crashing error (honoring commitment #5)
- [ ] **v1 → v2 snapshot migration path documented** — does v2 read v1's `./snapshots/*.json`? If yes, write the import logic; if no, document the manual migration steps
- [ ] Manual exercise pass on real Shield at 192.168.42.71 for every menu action
- [ ] Auto-updater verified end-to-end (publish a test release, install, confirm prompt + apply)
- [ ] **Update signing-key custody plan committed** (where the private key lives, who has access, what happens if it's lost — see 10.5)
- [ ] Installer signed for all five desktop targets, or each unsigned target has a documented user-impact tradeoff
- [ ] **Third-party attribution surface in-app** — a "Credits" / "About" view listing crate licenses (cargo-about or equivalent)
- [ ] **Accessibility baseline** — keyboard-only navigation works for every action; tab order is sensible; high-contrast theme renders correctly
- [ ] **No telemetry** confirmed via build inspection — no Sentry, no analytics SDKs, no outbound calls except the app-list / updater endpoints
- [ ] v1 README updated to note v2 is the new recommended download for desktop; v1 marked as maintenance-only

Mobile is **not** a v2.0 gating requirement. v2.0 desktop ships, v2.1 adds mobile (timeline depends on Phase 11.0 spike outcome).

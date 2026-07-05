# ATV Optimizer — Transport Licensing & Architecture Research

Question: how should the phone/tablet app drive an Android TV over **Android-11 wireless
debugging** (SPAKE2 pairing-code + TLS-1.3 + RSA auth + shell/exec) **without shipping GPLv3
code**, so the product can be sold closed-source? Current transport is
`io.github.muntashirakon:libadb-android` (GPL-3.0), which (a) hangs on `exec:` reads against an
older Nvidia Shield's adbd and (b) is license-incompatible with a commercial closed-source app.

Date: 2026-07-04. Author: research agent. **Not legal advice — get counsel to sign off on the
licensing conclusions before shipping.**

Confidence legend: **[CONFIRMED]** = verified against primary source; **[LIKELY]** = strong
indirect evidence; **[UNCERTAIN]** = needs verification.

---

## Executive summary (recommendation)

1. **Drop libadb-android.** It is GPL-3.0(-or-later) per its own repo — a hard blocker for a
   closed-source paid app. **[CONFIRMED as blocker; license per repo — verify exact SPDX.]**

2. **The single most useful finding:** the Rust crate **`adb_client` (MIT)** already implements
   the *hard, reliability-sensitive* half in pure Rust — direct-to-device **TLS-1.3 connect + RSA
   AUTH + OPEN/OKAY/WRTE/CLSE stream multiplexing + shell / exec / install / push / pull** — with
   **no adb binary and no adb server** (`adb_client::tcp::ADBTcpDevice`). It uses `rustls` +
   `rcgen`. This is exactly the layer where libadb-android is buggy, and it drops cleanly into the
   Tauri **Rust** side, matching the app's "logic in Rust, thin Kotlin" architecture. **[CONFIRMED
   by reading the crate source.]**

3. **The one gap:** `adb_client` does **NOT** implement the Android-11 **SPAKE2 pairing** handshake
   in pure Rust. Its `pair()` only proxies `host:pair:<code>:<addr>` to a *running adb server*
   (`server/commands/pair.rs`) — useless without an adb binary. Direct `ADBTcpDevice` assumes the
   device is **already paired/trusted**. So a first-run pairing step must be built. **[CONFIRMED by
   source: the only pairing code path goes through the server proxy.]**

4. **Recommended architecture (primary):** `adb_client` (MIT) for connect + streaming, **plus a
   small clean-room SPAKE2 pairing module** (one-time flow), referencing the **Apache-2.0** AOSP
   `packages/modules/adb/pairing_*` sources. Pairing is the only real net-new work (~1–2 weeks;
   the hard part is BoringSSL-compatible SPAKE2 over Ed25519 + the TLS keying-material export).

5. **Fastest-to-ship fallback:** bundle an **aarch64 `adb` binary built from AOSP source
   (Apache-2.0)**, exec it from the APK's `nativeLibraryDir`, run its local server, and drive it
   (pair/connect/shell) via `adb_client`'s server mode. This gets **native, battle-tested SPAKE2
   pairing for free**, at the cost of a ~5–10 MB per-arch binary, a server-process lifecycle, and
   text-output parsing. Termux proves adb runs on unrooted Android. **[CONFIRMED feasible.]**

6. **Do NOT redistribute Google's prebuilt platform-tools `adb`:** (a) those are *host* binaries
   (x86-64 Linux/macOS/Windows), not Android-arm, so they won't even run on the phone; and (b) the
   **Android SDK License Agreement §3.4 forbids redistribution** of the packaged SDK. Building
   `adb` from AOSP source sidesteps *both* problems and is Apache-2.0 clean. **[CONFIRMED.]**

7. **atvtools reality (previous guess was wrong):** it is a **phone/tablet-side app** (both
   **iOS** and **Android** editions) that talks to the TV over **wireless ADB across the LAN** —
   *not* Shizuku, not an accessibility service, not loopback, not a TV-side app. It is
   **closed-source / proprietary**, sold freemium on Google Play (and App Store). Its iOS edition
   proves a from-scratch (non-libadb) wireless-ADB client is viable and shippable on stores.
   **[CONFIRMED it's phone-side over wireless ADB; iOS+Android; closed-source. Exact ADB impl
   not public — UNCERTAIN.]**

8. **Feasibility verdict:** the app connects phone→TV **outbound over the LAN** — it does *not*
   need loopback, root, or Shizuku, and needs no elevated privilege on the phone. That makes the
   pure-Rust path (#4) clean and the only genuinely novel code the SPAKE2 pairing handshake.

---

## Q1 — License of Google's / AOSP `adb`; can a commercial app ship the prebuilt binary?

### AOSP `adb` source: Apache-2.0 **[CONFIRMED]**
- AOSP's preferred license is **Apache-2.0**, and the `adb` module
  (`platform/packages/modules/adb`, historically `system/core/adb`) carries the standard "The
  Android Open Source Project" Apache-2.0 headers (SPDX `Apache-2.0`).
  Source: <https://source.android.com/license>
- Apache-2.0 permits commercial, closed-source redistribution of source/derivatives provided you
  keep the license/NOTICE and attribution. So **building `adb` yourself from AOSP source and
  shipping it in a paid closed-source product is permitted.**

### Google's *prebuilt* platform-tools package: redistribution NOT permitted **[CONFIRMED]**
- The downloadable "SDK Platform-Tools" package (which contains the prebuilt `adb`/`fastboot`) is
  governed by the **Android Software Development Kit License Agreement**. **§3.4** states you may
  not "copy (except for backup purposes), modify, adapt, redistribute, decompile, reverse
  engineer, disassemble, or create derivative works of the SDK or any part of the SDK" — *except
  as required by applicable third-party licenses.*
  Source: <https://developer.android.com/studio/terms>
- The "third-party licenses" carve-out is why people say the underlying Apache-2.0 `adb` is
  redistributable — but the safe, unambiguous route is to **rebuild from AOSP source** (governed
  purely by Apache-2.0) and comply with its NOTICE/attribution, rather than reship Google's ToS-
  bound package.

### Extra reason not to use the prebuilt: wrong architecture **[CONFIRMED]**
- Google's platform-tools `adb` binaries are **host** binaries (x86-64 desktop OSes). There is no
  official Google-shipped **Android arm64** `adb`. To run `adb` *on the phone* you must build it
  for `aarch64` from AOSP source (or use a community build — see Q2).
  Sources: <https://developer.android.com/tools/releases/platform-tools>,
  <https://github.com/qhuyduong/arm_adb>, <https://archlinuxarm.org/packages/aarch64/android-tools>

**Bottom line:** Bundling `adb` is legally fine **if built from AOSP source (Apache-2.0)** for
arm64. Reshipping Google's prebuilt package is both technically wrong (host arch) and ToS-barred.

---

## Q2 — Feasibility: running an `adb` binary inside an unrooted Android app

**Verdict: feasible and proven (Termux does it), but with real packaging friction.** Note the ATV
Optimizer's own connection is phone→TV *over the LAN*, so it does **not** need the loopback/root
trick that most "on-device adb" guides describe — it just needs adb to make outbound TLS
connections and (optionally) run a local server on 127.0.0.1.

Key facts:
- **W^X (Android 10+):** apps cannot `exec()` from writable dirs. The supported path is to place
  the executable in the **APK's native-lib dir** and exec from `getApplicationInfo().nativeLibraryDir`.
  Requirements: name it `lib*.so` (only that pattern is extracted), set
  **`android:extractNativeLibs="true"`** so it lands on disk, and set `LD_LIBRARY_PATH` if it needs
  non-system `.so`s. **[CONFIRMED]**
  Sources: <https://medium.com/android-knowledge-store/execute-native-executable-on-android-d6e897e71898>,
  <https://www.androidbugfix.com/2022/01/android-can-execute-process-for-android.html>
- **Local server / localhost bind:** `adb` starts a server on `127.0.0.1:5037` by default; binding a
  loopback TCP port from an app sandbox is allowed (no special permission). **[LIKELY — standard
  sandbox behavior; Termux relies on it.]**
- **`adb pair` / `adb connect` from on-device:** works. Termux users routinely run
  `adb pair 127.0.0.1:<port>` / `adb connect …` on unrooted phones. For the ATV Optimizer the target
  is the TV's LAN IP rather than loopback, which is strictly simpler (outbound only).
  Sources: <https://xdaforums.com/t/termux-adb-running-adb-within-android-using-termux-wireless-adb.4724780/>,
  <https://gist.github.com/kairusds/1d4e32d3cf0d6ca44dc126c1a383a48d>,
  <https://www.howtogeek.com/you-dont-need-a-pc-for-adb-anymoreheres-how-to-do-it-in-termux/>
- **Getting an arm64 build:** community build scripts / prebuilts exist
  (`bonnyfone/adb-arm`, `qhuyduong/arm_adb`, Arch Linux ARM `android-tools`, Termux
  `android-tools`), or build from AOSP.
  Sources: <https://github.com/bonnyfone/adb-arm>, <https://github.com/qhuyduong/arm_adb>

**Pitfalls:** ~5–10 MB binary per ABI (arm64 + maybe armeabi-v7a); `extractNativeLibs=true`
inflates install size and is otherwise-discouraged; you own the adb-server process lifecycle
(start/kill, port conflicts, `adb kill-server`); you parse human-oriented text output; and you must
build/track an arm64 adb across Android versions. Workable, but heavier than a library.

---

## Q3 — How atvtools (`dev.vodik7.atvtools`) actually works

**Corrected mechanism [CONFIRMED at the architecture level]:**
- It is a **phone/tablet-side controller app**, published by developer **tvDev** for **both Android
  (`dev.vodik7.atvtools`, 100k+ installs) and iOS**. It manages Android TV / Fire TV / Nvidia Shield
  devices remotely.
- It connects to the TV over **wireless ADB across the local network** — you enable Developer
  options + (wireless/USB) debugging on the TV, then the app connects to the TV's IP. It then runs
  ADB services: **sideload APKs, push files, remote/mouse input, and shell commands (e.g. disabling
  preinstalled apps)** — i.e. the same `pm`/`dumpsys`/shell surface ATV Optimizer uses.
  Sources: <https://play.google.com/store/apps/details?id=dev.vodik7.atvtools>,
  <https://tvdevinfo.com/atvtools> (describes the iOS edition: "same Wi-Fi network as the Android
  TV/Fire TV", enable Developer options + USB debugging, connect by IP, shell commands to disable
  preinstalled apps).
- **It is NOT Shizuku, NOT an accessibility service, NOT a loopback/on-device-adb trick, and NOT a
  TV-side app.** It is a networked wireless-ADB client to a *separate* device — the same category as
  ATV Optimizer.
- **Distribution & license:** closed-source / proprietary, freemium (free tier + paid premium), via
  Google Play and the App Store. No public source repo or OSS license found.
  Source: <https://chrome-stats.com/d/dev.vodik7.atvtools>

**Why this matters:** the **iOS** edition cannot use libadb-android (Java/Android-only), so tvDev
necessarily has a non-libadb wireless-ADB implementation (their own client, or a
C/Swift/portable library). That is direct precedent that a from-scratch wireless-ADB client is
shippable on app stores as a commercial product. **[The exact library/implementation they use is
not public — UNCERTAIN.]**

---

## Q4 — Permissively-licensed libraries for the Android-11 wireless-debugging flow

### `adb_client` (Rust) — **MIT** — the strongest option **[CONFIRMED by source review]**
Repo: <https://github.com/cocool97/adb_client> · crates.io: <https://crates.io/crates/adb_client>
(v3.2.2, actively maintained — 279+ commits, 41 releases).

What it *does* provide, **pure-Rust, no adb binary/server** (`adb_client::tcp::ADBTcpDevice`):
- Direct TCP connect to a device, **TLS-1.3 upgrade via `rustls`**, self-signed cert generated from
  the RSA keypair via **`rcgen`** (`certificate_from_pk`, `PKCS_RSA_SHA256`) — i.e. the ADB
  **STLS/AUTH** path.
- **Stream multiplexing + `shell`, `shell_command` (exec-style), `install`, `push`, `pull`, `stat`,
  `reboot`, `framebuffer`, etc.** via the `ADBDeviceExt` trait.
- mDNS discovery (`adb_client::mdns`), useful for finding the TV's dynamic TLS connect port.
- Evidence of active TLS work: PR #202 "fix: incorrect private key used for TLS upgrade" (2026-05),
  PR #21 "Add pairing and connecting features over tcp", PR #30 "Fix pairing".

**Critical limitation:** its **pairing** (`server/commands/pair.rs`) is implemented **only** as a
proxy command (`host:pair:<code>:<addr>`) to a **running adb server** — there is **no pure-Rust
SPAKE2 pairing**. `ADBTcpDevice` (the no-server path) therefore requires the device to be
**already paired**. So `adb_client` covers connect+streaming brilliantly but leaves the **first-run
SPAKE2 pairing** to you. **[CONFIRMED: SPAKE2 not present; only server-proxy pairing exists.]**

This limitation is *exactly the opposite* of libadb-android's problem: libadb does pairing but
streams unreliably; `adb_client` streams reliably but doesn't pair. That asymmetry drives the
recommended hybrid.

### `spake2-java` (MuntashirAkon) — for the pairing piece
Repo: <https://github.com/MuntashirAkon/spake2-java>. **BoringSSL-compatible SPAKE2 in Java**,
explicitly tested against Android-11 wireless-debugging pairing. Self-described **unaudited**. It is
what libadb-android uses. License is MuntashirAkon's (**likely GPL-family — VERIFY**; if GPL it's a
blocker, but it's a useful *reference* for a clean-room port). **[UNCERTAIN license.]**

### Rust `spake2` crate (RustCrypto/Brian Warner) — NOT drop-in compatible **[LIKELY]**
A general Rust `spake2` crate exists, but ADB uses **BoringSSL's `spake25519`** variant with
specific M/N generator points; a generic SPAKE2 crate uses different constants and will **not
interoperate** on the wire without matching BoringSSL's exact parameters. Treat pairing as a
custom/ported implementation, not "add a crate."
Source (compatibility caveat): <https://crates.io/crates/adb-wireless>,
<https://www.androidauthority.com/android-17-adb-wi-fi-2-0-3678411/> (notes Google itself rewrote
ADB Wi-Fi internals in a ~4,000-line Rust lib in Android 17 — confirms the space is Rust-friendly,
but that lib handles reconnection/network monitoring, not a reusable public SPAKE2 pairing API).

### Other crates (weaker)
`adb-wireless`, `adbqr`, `radb`, `Rust-ADB` — smaller/less proven; `adb_client` is the mature one.
No **Apache/MIT Kotlin/Java** library was found that does the full pairing flow *and* is
permissively licensed (the Java options in this space, incl. libadb-android and spake2-java, are
GPL-family). **[CONFIRMED no permissive Kotlin/Java full-flow lib surfaced.]**

---

## Q5 — Clean-room reimplementation scope

Pieces of the ADB client protocol and their difficulty:

| Piece | What it is | Difficulty | Covered by `adb_client`? |
|---|---|---|---|
| CNXN banner + protocol version | Initial handshake, feature negotiation | Easy | Yes |
| RSA key AUTH (`AUTH` token/signature/pubkey) | Trust establishment on connect | Easy–Med | Yes |
| Stream mux OPEN/OKAY/WRTE/CLSE | Multiplexed logical streams; **CLSE/EOF handling is the libadb bug** | Med | **Yes** (this is the win) |
| `shell:` / `exec:` / `shell,v2:` services | Command execution + stdout/stderr/exit | Med | Yes |
| TLS-1.3 wrap of the connect stream (STLS) | Conscrypt/rustls mutual TLS after CNXN on Android 11+ | Med | Yes (`rustls`) |
| **Android-11 pairing: SPAKE2 over TLS-1.3 pairing service** | 6-digit code → SPAKE2 (BoringSSL `spake25519`) key exchange over a TLS channel using the **exported keying material** as context, then exchange/enroll the RSA public key | **HARD** | **No** |

**The only hard, uncovered part is the pairing handshake.** Its sharp edges:
1. **BoringSSL-exact SPAKE2** (`spake25519`, Ed25519 group, specific M/N seed points). Must match
   byte-for-byte or the device rejects it. Reference: AOSP `pairing_auth` and MuntashirAkon's
   `spake2-java` (which ports BoringSSL). **[CONFIRMED this is the crux.]**
2. **TLS exported-keying-material binding:** the pairing runs *inside* a TLS-1.3 session and mixes
   the TLS export key into the SPAKE2 password/context — you need a TLS stack that exposes
   `export_keying_material` (rustls does).
3. **Pairing packet framing** (`pairing_connection` / `pairing_packet` state machine) — mechanical
   once SPAKE2 + TLS export are right.

You **can** reference the **Apache-2.0** AOSP `packages/modules/adb/pairing_auth/`,
`pairing_connection/` sources to build this cleanly (Apache-2.0 permits derivative closed-source
work with attribution) — so it need not even be a strict clean-room; it can be a licensed port.
**[CONFIRMED Apache-2.0 permits this.]**

**Effort estimate [UNCERTAIN — engineering judgment]:**
- Connect + shell/exec/streaming: **~0** (use `adb_client`, MIT).
- SPAKE2 pairing module in Rust (port BoringSSL spake25519 + TLS-export binding + packet framing,
  test against a real Shield): **~1–2 weeks** for someone comfortable with crypto/protocol work;
  add buffer for device-specific quirks. Full from-scratch reimplementation of *everything*
  (ignoring adb_client) would be **~4–8 weeks** and is not recommended given adb_client exists.

---

## Ranked recommendation

1. **`adb_client` (MIT) for connect/stream + clean-room-or-ported SPAKE2 pairing (Apache-2.0
   reference).** Best fit: pure Rust in the Tauri layer, no GPL, the reliability-critical stream
   code is maintained upstream, and the buggy CLSE/EOF path leaves libadb behind. Only novel work is
   the one-time pairing handshake. **Recommended primary.**

2. **Bundle an AOSP-built (Apache-2.0) arm64 `adb`; run its server; drive via `adb_client` server
   mode (or shell-out).** Gets native, proven SPAKE2 pairing "for free"; licensing is clean if built
   from source. Downsides: per-arch binary size, `extractNativeLibs=true`, server-process lifecycle,
   text parsing, arm64-adb maintenance. **Recommended fallback / fastest prototype**, or a hybrid:
   bundle adb *only* to perform the one-time pairing, then use pure-Rust `adb_client` for all
   ongoing connect/shell traffic.

3. **Full clean-room reimplementation of the entire ADB client** referencing Apache-2.0 AOSP. Only
   if #1's crate dependency is undesirable. Larger effort, little marginal benefit over #1.

4. **Redistribute Google's prebuilt platform-tools `adb`.** ✗ Rejected — wrong architecture (host,
   not Android-arm) *and* SDK ToS §3.4 bars redistribution.

5. **Keep libadb-android.** ✗ Rejected — GPL-3.0 blocks a closed-source paid product, and it's the
   source of the current exec-stream hang.

---

## What is confirmed vs uncertain

**Confirmed:** AOSP `adb` is Apache-2.0; SDK ToS §3.4 bars redistributing the prebuilt package;
Google ships no arm64 host adb; adb runs on unrooted Android (Termux); Android 10+ requires exec
from nativeLibraryDir with `extractNativeLibs=true`; `adb_client` is MIT and implements TLS connect
+ RSA auth + shell/exec/stream but only server-proxy pairing (no pure-Rust SPAKE2); atvtools is a
closed-source phone-side (iOS+Android) wireless-ADB LAN client, not Shizuku/accessibility/loopback.

**Uncertain / verify before committing:** exact SPDX of libadb-android and spake2-java (assumed
GPL-family); whether any Rust SPAKE2 crate can be parameterized to BoringSSL `spake25519`
compatibility (assessed: not off-the-shelf); atvtools' exact ADB implementation/library (not
public); the pairing-module effort estimate; and — because this gates the ability to *sell* the
product — **all licensing conclusions should be reviewed by legal counsel.**

---

## Sources
- AOSP content license (Apache-2.0 preferred): https://source.android.com/license
- Android SDK License Agreement (§3.4 redistribution): https://developer.android.com/studio/terms
- Platform-tools release notes (host binaries): https://developer.android.com/tools/releases/platform-tools
- `adb_client` repo (MIT): https://github.com/cocool97/adb_client
- `adb_client` crates.io: https://crates.io/crates/adb_client
- `adb_client` docs.rs: https://docs.rs/adb_client
- spake2-java (BoringSSL-compatible, unaudited): https://github.com/MuntashirAkon/spake2-java
- libadb-android: https://github.com/MuntashirAkon/libadb-android
- atvtools Play listing: https://play.google.com/store/apps/details?id=dev.vodik7.atvtools
- atvtools (iOS) dev site: https://tvdevinfo.com/atvtools
- atvtools metadata (proprietary, freemium): https://chrome-stats.com/d/dev.vodik7.atvtools
- Execute native binary on Android 10+: https://medium.com/android-knowledge-store/execute-native-executable-on-android-d6e897e71898
- W^X / nativeLibraryDir exec: https://www.androidbugfix.com/2022/01/android-can-execute-process-for-android.html
- Termux wireless ADB on-device: https://xdaforums.com/t/termux-adb-running-adb-within-android-using-termux-wireless-adb.4724780/
- Termux adb pairing gist: https://gist.github.com/kairusds/1d4e32d3cf0d6ca44dc126c1a383a48d
- arm64 adb build/prebuilt: https://github.com/bonnyfone/adb-arm , https://github.com/qhuyduong/arm_adb , https://archlinuxarm.org/packages/aarch64/android-tools
- Android 17 ADB Wi-Fi 2.0 / Rust rewrite: https://www.androidauthority.com/android-17-adb-wi-fi-2-0-3678411/

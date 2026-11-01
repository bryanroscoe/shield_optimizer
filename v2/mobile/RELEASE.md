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

A relative `storeFile` resolves against `src-tauri/gen/android/`. If neither source is complete, any
release `assemble*`/`bundle*`/`package*` task **fails** with a message naming the missing values;
debug builds are unaffected. For a deliberate local unsigned release (for example to exercise
`scripts/verify-release-apk.py`), set `ATVOPT_ALLOW_UNSIGNED_RELEASE=1`; never upload that
artifact. A release AAB also fails unless `ATVOPT_DISTRIBUTION=play` is set, because an AAB is the
Play upload format and a non-Play build carries offline key entry. `keystore.properties`, `key.properties`,
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
cargo clippy -p shield-optimizer-core -p atv-optimizer-mobile -p tauri-plugin-atv-adb -p tauri-plugin-atv-billing --all-targets -- -D warnings
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

### Build-time inputs for a Play build

| Variable | Value | Effect |
| --- | --- | --- |
| `ATVOPT_DISTRIBUTION` | `play` | Play build: offline license-key entry hidden, `activate_license` refused. Unset or anything else = sideload/dev build. |
| `ATVOPT_PLAY_LICENSE_KEY` | Play Console → the app → **Monetize with Play → Monetization setup → Licensing** → "Base64-encoded RSA public key" | Compiled in; verifies every purchase signature. Public, not a secret, but keep it out of Git so a fork can't ship with it by accident. Without it, purchases show "unavailable in this build" and Play never grants Pro. A `play` **release** build without a plausible key fails in `src-tauri/build.rs`. |
| signing (`keystore.properties` or `ATVOPT_KEYSTORE_*`) | the upload key | Required for any release task (see §1). |

Both `ATVOPT_*` billing variables are read by Cargo at compile time (`option_env!`), so export them
in the same shell that runs `tauri android build`; changing either triggers a rebuild of the app
crate.

```
export ATVOPT_DISTRIBUTION=play
export ATVOPT_PLAY_LICENSE_KEY='MIIBIjANBgkqh…'   # one line, from Play Console
npm run tauri android build -- --aab
```

The product id is the constant `PRO_PRODUCT_ID = "atvopt_pro"` in `src-tauri/src/play_billing.rs`.

Tauri drives Gradle, and the generated `RustPlugin` already defines one product flavor per ABI
(`universal`, `arm64`, `arm`, `x86`, `x86_64`), so per-ABI output needs CLI flags, not Gradle edits.

```
# Play upload artifact — one AAB, all ABIs (runs Gradle bundleUniversalRelease)
ATVOPT_DISTRIBUTION=play ATVOPT_PLAY_LICENSE_KEY=… npm run tauri android build -- --aab

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

Verify the AAB signature before uploading (an AAB is a JAR-signed ZIP):

```
jarsigner -verify -verbose -certs \
  src-tauri/gen/android/app/build/outputs/bundle/universalRelease/app-universal-release.aab | tail -5
```

## Telemetry endpoint

Anonymous reports are compiled in only when the build environment sets `ATVOPT_TELEMETRY_URL` to
the owner-operated Worker's `https://` base URL (no trailing path; non-HTTPS URLs other than
loopback are ignored). Without it the app never sends reports or
fetches support tiers, and More shows the setting as unavailable. Set it for the release build:

```bash
ATVOPT_DISTRIBUTION=play ATVOPT_PLAY_LICENSE_KEY=… \
  ATVOPT_TELEMETRY_URL=https://telemetry.example.workers.dev npm run tauri android build -- --aab
```

`option_env!` bakes the value in at compile time, so changing it needs a rebuild of the shared
core (Cargo tracks the variable). Check the Data safety answers against `DATA-INVENTORY.md` before
uploading a build that has it set. Purchase records (order id, token, price, product) are never
included in reports; a Buy attempt reports only its outcome (see `DATA-INVENTORY.md`).

## 6. Owner setup in Play Console for Pro (one time)

**Click-by-click version:** [`PLAY-CONSOLE-WALKTHROUGH.md`](PLAY-CONSOLE-WALKTHROUGH.md) covers the
whole launch in critical-path order: account verification, payments profile, store listing, Data
safety answers, content rating, closed test, promo codes and production access. Its links were
checked against Google's pages on 2026-11-01. Where it differs from the summary below, follow the
walkthrough.

The code is ready for these; none of it can be done from the repo. Order matters: Play only lets
you create the in-app product after an AAB that declares `com.android.vending.BILLING` has been
uploaded to some track.

1. **Upload key.** Create it with §1's `keytool` command, store the `.jks` and both passwords in
   1Password with an offline backup. It is the upload key only.
2. **Create the app** in Play Console with package `com.atvoptimizer.mobile` (permanent after the
   first upload), category Tools, free download with in-app purchases.
3. **Play App Signing.** Accept Google-managed app signing on the first upload (default for new
   apps). Record the app-signing and upload certificate SHA-256 fingerprints from **Protected with
   Play → Play Store protection → Manage Play app signing** in 1Password (new apps get hybrid
   signing, which shows three app-signing fingerprints).
4. **Payments profile.** Manage account → Play apps settings → Monetization settings →
   **Payments profile**: create or link the merchant/payments profile, bank account and tax
   details. Selling the product is blocked until it is verified, which can take days, so start
   it first.
5. **License key.** Monetize with Play → Monetization setup → copy the Base64 RSA public key into
   1Password as `ATVOPT_PLAY_LICENSE_KEY`. It is per-app and never rotates.
6. **First upload.** Build the AAB above (with the key and `ATVOPT_DISTRIBUTION=play`), choose a
   new `versionCode` (§2), upload it to **Internal testing**.
7. **Product.** Monetize with Play → Products → **One-time products** → create product id
   **`atvopt_pro`** (exactly; it is permanent), name "ATV Optimizer Pro", one purchase option, buy
   (not rent), US price **$4.99** and let Play derive other countries' prices. Activate it. The app
   shows Play's localized price, never a hard-coded one.
8. **License testers.** Settings → **License testing**: add the Google accounts of everyone testing
   purchases: the owner and closed-track testers who will try the purchase flow, so they can
   test-purchase Pro without being charged. Leave Ko-fi supporters who get a promo code (step 10)
   off this list, so their redemption is a normal, permanent purchase. License testers' purchases
   use test instruments ("Test instrument, always approves", "Slow test card, approves after a few
   minutes" for PENDING, "…always declines" for errors) and are never charged. Set the license
   response to RESPOND_NORMALLY.
9. **Closed testing track.** The developer account is personal and recently created, so the
   12-tester / 14-day rule applies: at least 12 testers must stay opted in for 14 consecutive days
   before you can apply for production access (then about a week of review). Test and release →
   Testing → **Closed testing**: create a track, add a Google Group or email list, and roll the
   same AAB out to it. **Invite past Ko-fi supporters first**; they are the natural first 12+.
   Share the opt-in link. Start this as early as possible: it is the critical path to
   production.
10. **Ko-fi supporters get Pro free.** Monetize with Play → **Promo codes** → Create promo code
    for `atvopt_pro` (at most 500 codes per quarter) and send each past Ko-fi supporter one
    code. A redeemed code arrives as a normal purchase (no order id) and goes through the same verification
    and restore. Supporters who are also closed testers can redeem their code on the closed track.
    No separate key, APK or bypass. Keep the supporter list and codes out of this repo.

### Purchase test script (license tester, closed or internal track build installed from Play)

`atvopt_pro` is non-consumable, so an account that owns it gets "already owned" instead of a new
payment sheet. Run the cases **in this order on one license-tester account**; each starts from
the state the previous one leaves. To rerun a case that needs Free, first refund the test order
with the entitlement removed (Order management) and reopen the app. Prove acknowledgment in Order
management plus a relaunch, not by waiting inside the app. See
[`PLAY-CONSOLE-WALKTHROUGH.md`](PLAY-CONSOLE-WALKTHROUGH.md) §4.3.

Starting state: Free (the account doesn't own `atvopt_pro`).

1. Fresh install, Free: More shows Play's price in local currency; Optimize shows the lock card
   with no "Already have a key?" link.
2. Cancel the sheet: no message, still Free. Buy with "always declines": an error, still Free.
3. Buy with "Slow test card, approves after a few minutes": "Purchase pending" and still Free;
   after it completes, returning to the app unlocks Pro. **Now owned.**
4. Acknowledgment: 5 minutes later Order management shows the order still charged, not refunded
   (unacknowledged tester purchases are refunded after 3 minutes); force-stop and relaunch, still
   Pro, and More shows "Source: Google Play purchase".
5. Uninstall/reinstall, or a second phone on the same account: Restore purchases (or just launch)
   restores Pro without charging.
6. Airplane mode while owned: Pro stays (up to 7 days offline); then Free until Play is reachable.
7. Refund the order with "Remove entitlement" in Order management, reopen the app: back to Free;
   More → Emergency recovery and Apps → re-enable still work. **Now Free.**
8. Buy with "always approves": Pro unlocks at once; repeat the step 4 acknowledgment check.
9. Promo code, on an account that doesn't own Pro (a supporter account that isn't a license
   tester, or this one after another refund as in step 7): redeem in the Play Store app, then open
   the app: Pro unlocks.

## 7. Upload

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
- [ ] Owner completes §6 (payments profile, `atvopt_pro`, license testers) and the purchase test
      script passes on a Play-installed build. There is no trial. Offline key entry is not offered
      in Play builds. Listing and paywall copy may promise only what is true: one-time purchase,
      no subscription, and "re-enabling apps and emergency recovery are always free". Do not
      promise free undo of settings, Home app or snapshots (`FREE-SETTINGS-UNDO-DESIGN.md` is
      unbuilt).
- [ ] Obtain the account-specific testing/verification requirements from the actual store account;
      no account status or production-access eligibility has been verified here.

### Implemented, not fully physically validated

Android 11+ SPAKE2 code pairing is implemented in `src-tauri/src/wireless_adb.rs` and the vendored
transport. Signed Ed25519 offline licensing, persistence and debug-only development keys are
implemented; see `LICENSING.md`. Neither implementation establishes current phone/TV end-to-end readiness.

### Still missing or unresolved

SAF import/export, mobile release automation, verified signed APK/AAB evidence, complete resolved
Android dependency notices (now including the Play Billing Library) and privacy/store disclosures
remain open. Play billing v1 is implemented but has not run against a real Play account.
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

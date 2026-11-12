# Releasing ATV Optimizer for Android

End-to-end checklist for cutting an Android build. The build, verification and Play upload are
automated on `mobile-v*` tags by `.github/workflows/mobile-play-release.yml` (see
[Automated release (CI)](#automated-release-ci)); the desktop app's `v2-*` tag pipeline
(`.github/workflows/v2-release.yml`) is a separate track and must not be reused for mobile.
`.github/workflows/v2-tests.yml` runs the mobile gates on every change. The local commands below
remain the reference and the fallback.

## Automated release (CI)

Push a tag `mobile-vX.Y.Z[-pre]` on a commit that is on `main`:

```sh
mobile/scripts/bump-version.sh 0.1.2   # then commit, PR and merge as usual
git tag -a mobile-v0.1.2 -m "ATV Optimizer for Android 0.1.2" <merged commit> && git push origin mobile-v0.1.2
```

The `release` job of `mobile-play-release.yml` then, in the `google-play` environment:

1. refuses a revision that is not on `main`, the `production` track and a tag that disagrees with
   `tauri.conf.json`, resolves the versionCode (`bundle.android.versionCode` if set, else derived
   from the version, see §2) and checks that every secret and variable below is set;
2. installs the Android toolchain and npm dependencies before writing any credential, then checks
   the keystore's certificate against the recorded upload certificate;
3. asks Play for the highest versionCode it holds across every bundle, APK and track release; a
   code that is not above it fails here, before anything is built;
4. builds with `scripts/build-play-release.py` (all four ABIs, Play mode, telemetry URL);
5. runs `scripts/verify-play-bundle.py`: AAB JAR signature and upload certificate, `bundletool
   validate`, manifest package and versionCode, then the universal APK bundletool generates and the
   Gradle universal APK through `verify-release-apk.py` with the same certificate check;
6. keeps the AAB, APK, build receipt and verification JSON as a 30-day workflow artifact;
7. uploads the AAB with `scripts/play-publish.py` and creates a **draft** release on the
   **internal** track. Nothing reaches testers until you open Play Console → Test and release →
   Internal testing → the draft → **Review release** → **Start rollout**.

**Run workflow** (Actions → Mobile Play release) offers the same job by hand: mode `build` (signed
and verified, no upload), `publish` (pick the track, e.g. `alpha` for the default Closed testing
track, or a custom closed track id, and `draft` or `completed`) or `smoke`. The `production`
track is refused: promote a tested release to production in Play Console. `completed` is refused
by Play while the app has never been published. A versionCode override is there for re-spinning
one version; it must still be above everything on Play.

The `smoke` job runs on PRs and pushes to `main` that touch mobile sources or their shared
inputs, as well as the workflow and toolchain action. It performs the same four-ABI build and
verification with a throwaway keystore and a random license key, no secrets and no upload.
Both smoke and real release jobs run the Kotlin discovery/network JVM tests and require
nonempty passing JUnit reports. The reports are retained as workflow artifacts. This covers
Android compilation and JVM behavior before a real tag depends on them; it is not device QA.

Release CI covers the build, signature and static artifact gates and the versionCode check.
The separate manual [16 KB runtime workflow](#retained-candidate-16-kb-runtime-qa) can record
x86_64 startup evidence for an existing signed candidate. Neither workflow covers the Play
pre-launch report, ARM64 runtime, the phone/TV matrix or the purchase script; those stay in
[Testing-candidate gates](#testing-candidate-gates).

### One-time setup

1. **First upload by hand.** The Play Developer API cannot create the first release of a new app.
   If Play Console shows no upload yet, do §5 and §7 once locally.
2. **Service account.** Google Cloud console (any project) → enable the *Google Play Android
   Developer API* → IAM → Service accounts → create one (no Cloud roles) → Keys → Add key → JSON.
   Save it as `~/.config/atv-optimizer/play-service-account.json` (mode 600) and in 1Password.
   Then Play Console → **Users and permissions** → Invite new users → the service account's email
   → App permissions → ATV Optimizer → grant *View app information* and *Release apps to testing
   tracks*. CI never releases to production, so it does not need *Release to production*. Access
   can take a few minutes to apply.
3. **Load the environment.** `mobile/scripts/setup-play-ci.sh --dry-run` prints what it will set;
   without `--dry-run` it creates the `google-play` environment (deploys only from `main` and
   `mobile-v*` tags) and sets, from the same local files `build-play-release.py` reads:

   | Name | Kind | Value |
   | --- | --- | --- |
   | `ATVOPT_UPLOAD_KEYSTORE_B64` | secret | base64 of the upload keystore |
   | `ATVOPT_UPLOAD_KEYSTORE_PASSWORD` / `ATVOPT_UPLOAD_KEY_PASSWORD` | secret | its passwords |
   | `PLAY_SERVICE_ACCOUNT_JSON` | secret | the service-account key |
   | `ATVOPT_UPLOAD_KEY_ALIAS` | variable | the key alias |
   | `ATVOPT_UPLOAD_CERT_SHA256` | variable | SHA-256 of the upload certificate; CI refuses any other keystore |
   | `ATVOPT_PLAY_LICENSE_KEY` | variable | the Play licensing public key (public, kept out of Git) |

   Re-run it after rotating any of them. Optionally add yourself as a required reviewer on the
   environment so every run waits for an approval click; re-running the script keeps the
   environment's reviewers, wait timer and self-review setting.

If a run fails, read the step that failed: credential and versionCode problems stop before the
build, and a rejected artifact is never uploaded. Delete a wrong tag with
`git push origin :refs/tags/mobile-vX.Y.Z` before re-tagging; a versionCode Play has already
accepted can never be reused, even if its release is discarded.

## Retained-candidate 16 KB runtime QA

`mobile-16kb-runtime.yml` tests the existing signed Gradle universal APK without rebuilding,
signing, uploading to Play or touching a physical device. Dispatch it from `main` with a
successful manual `Mobile Play release` run ID, its full source SHA and the APK SHA-256 copied
from its verified build receipt. Example for the retained build-only candidate 1003:

```sh
gh workflow run mobile-16kb-runtime.yml --ref main \
  -f source_run_id=42518527075 \
  -f source_sha=5e067e23c6319fc0b33b80e229c8a6573f2943c3 \
  -f apk_sha256=8a53d0bcfdedb5208d7314700e4638bf2d95716ba4f66d1e1c11434b94e02b3b
```

This requires an unexpired release artifact from this repository and source still in main's
history. The helper rejects PR/smoke/failed/foreign runs, ambiguous artifacts, changed bytes or
receipt disagreements. It compares the APK's x86_64 native library byte-for-byte with the AAB,
then repeats signature/certificate, manifest and static APK verification before installation.

The job uses one standard `ubuntu-24.04` runner, a 35-minute limit and seven-day evidence
retention. It is manual-only and restricted to the public repository: GitHub documents standard
public-repository runners as free. It uses only read permissions and the ephemeral GitHub token,
never the `google-play` environment or signing/Play credentials. It requires existing usable KVM
access, sufficient disk and accepted Android SDK licenses. Missing prerequisites fail clearly;
it does not change device permissions, accept new agreements or fall back to software emulation.

Google's official `system-images;android-36;google_apis_ps16k;x86_64` image must report
`getconf PAGE_SIZE` **16384**, API 36 and x86_64. A fresh emulator installs the pinned APK and
checks its installed hash, native-loader success, foreground activity, rendered onboarding and
stable process on both cold launch and restart. Review the retained screenshot/UI XML, bounded
logs, image revision and runtime receipt; a successful install or emulator image name alone
does not pass. A skipped or failed job never qualifies the candidate.

This is **x86_64 emulator startup evidence**, not ARM64 runtime, Play-generated split APKs,
TV pairing, purchases or the physical matrix. No runtime pass is implied merely by merging
this workflow. Run it for the exact candidate and inspect its results before recording the gate.

References: [Android 16 KB testing](https://developer.android.com/guide/practices/page-sizes)
and [GitHub-hosted runner capabilities/costs](https://docs.github.com/en/actions/reference/runners/github-hosted-runners).

## First install through Google Play

The owner's first Play Store install uses **Internal testing**. It does not have to wait for
public production access or the 12-tester / 14-day closed test. Use the same package and billing
implementation as the eventual public release; do not substitute a debug or fixture build.

1. Create the free app shell and configure the real per-app Play licensing public key. Set up
   the merchant profile for the Pro product; Google may require owner identity, bank/tax details
   or agreements. Finish the signed-artifact gates below.
2. Test and release → Testing → Internal testing: add the owner's phone Google account to the
   tester email list, upload the verified AAB, and roll out the internal release. Record the
   versionCode, artifact SHA-256, app-signing certificate fingerprints and opt-in URL.
3. On the phone, sign in with that tester account, open the opt-in link, join, then install from
   Google Play. Verify the installed version and that Free access and recovery work.
4. Add the owner's account to License testing before purchase tests; use Google's test payment
   instruments, not a real charge. Complete the purchase/restore/pending/refund matrix in §6.
5. Complete the physical phone/TV matrix and fix any failures. Internal availability alone is
   not product acceptance. Then finish listing/declarations and roll out Closed testing to
   start the account's required production-access clock. Send invited testers
   [`TESTER-GUIDE.md`](TESTER-GUIDE.md).

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
cargo clippy -p atv-optimizer-core -p atv-optimizer-mobile -p tauri-plugin-atv-adb -p tauri-plugin-atv-billing --all-targets -- -D warnings
cargo test -p atv-optimizer-core -p atv-optimizer-mobile
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

For the owner-authorized Play rollout, `scripts/build-play-release.py` reads signing inputs from
`~/.config/atv-optimizer/play-upload.json` and the app's real Play Console RSA public key from
`~/.config/atv-optimizer/play-license-key`. Both stay outside Git. It verifies the key parses,
the upload keystore opens, the privacy page is live, source is clean, capacity is available and
the proposed `versionCode` exceeds the operator-confirmed highest code on every track. It
refuses fixture mode and unsigned-release overrides, keeps build jobs at one and sets Play mode.

```sh
python3 scripts/build-play-release.py --version-code 1000 --previous-version-code 0 --preflight-only
python3 scripts/build-play-release.py --version-code 1000 --previous-version-code 0
```

Use `--previous-version-code 0` only when the Console confirms no upload exists. Set
`JAVA_HOME`, `ANDROID_HOME` and `NDK_HOME` first. The default builds all four Android targets;
`--target aarch64` narrows a deliberately arm64-only candidate. The build receipt records source,
targets, artifact hashes, policy hash and public-key/certificate fingerprints; it deliberately
leaves signature/bundletool/APK verification, 16 KiB runtime, Play pre-launch and device/purchase
acceptance open. The command never uploads or installs anything.
`scripts/verify-play-bundle.py` closes the signature, bundletool and APK gates for that output
(CI runs it with the same arguments; see the workflow's Verify step).

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

The product ids are constants in `src-tauri/src/play_billing.rs`: `PRO_PRODUCT_ID = "atvopt_pro"`
and the two pay-more tiers in `SUPPORT_PRODUCT_IDS` (`atvopt_pro_support_1`, `atvopt_pro_support_2`).

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
   shows Play's localized price, never a hard-coded one. Then create the two optional **Support me**
   tiers the same way: `atvopt_pro_support_1` at **$9.99** and `atvopt_pro_support_2` at **$19.99**.
   They unlock exactly the same Pro, so their names and descriptions must not promise anything
   extra. See [`PLAY-CONSOLE-WALKTHROUGH.md`](PLAY-CONSOLE-WALKTHROUGH.md) §3.3.
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
9. Support tier: refund again as in step 7, then tap the $9.99 **Support me** price: Play's sheet
   names the supporter product, Pro unlocks, and the Support me row and Buy Pro disappear. Repeat
   the step 4 acknowledgment check for this order.
10. Promo code, on an account that doesn't own Pro (a supporter account that isn't a license
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

SAF import/export, verified signed APK/AAB evidence, complete resolved
Android dependency notices (now including the Play Billing Library) and privacy/store disclosures
remain open. Play billing v1 is implemented but has not run against a real Play account.
The `mobile-release-preflight.yml` workflow unit-tests the release scripts with synthetic artifacts and
mocked SDK/Play responses. `mobile-play-release.yml` builds, signs, verifies and uploads (see
[Automated release (CI)](#automated-release-ci)); neither validates the native Android transport.

### Current external requirements to verify at submission

The configuration targets API 36 (the mobile requirement effective August 31, 2026), with minSdk24.
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

## Promote an already tested candidate without rebuilding

`Promote verified mobile candidate` (`mobile-play-promote.yml`) is a manual, Internal-only
handoff from the existing signed-build and 16 KB runtime workflows. It never compiles, signs,
changes runner permissions, or uploads to production. It reuses the existing `google-play`
environment and service account; no additional credentials are needed.

1. Run `Mobile Play release` in `build` mode on the selected merged source and retain its
   successful run ID and exact APK/AAB hashes. Its native build/signature checks must pass.
2. Require the exact source's main `v2 Tests` run, including all six substantive jobs and its
   aggregate verdict. A PR merge-ref test or a skipped job is not the exact main-source gate.
3. Run `Mobile 16 KB runtime QA` on that retained APK; inspect the successful receipt for
   the same source, artifact, version and byte hashes. Any permission opt-in still requires
   separate approval for the concrete runtime job; promotion never supplies that approval.
4. Dispatch `Promote verified mobile candidate` on main with `build_run_id`, `source_sha`,
   `apk_sha256`, `aab_sha256`, `runtime_run_id` and `ci_run_id`. Choose `draft` (the default)
   to leave an Internal draft or `completed` only when rollout to existing Internal testers
   is authorized. This workflow has no arbitrary track input.

The promotion rejects missing/expired/ambiguous artifacts, foreign or unsuccessful runs,
non-main source/QA revisions, incomplete current-attempt CI jobs, mismatched receipts/hashes,
and a certificate different from the configured upload certificate. The explicit AAB hash
is checked again inside the uploader before opening a Play edit. The uploader rechecks Play's
highest versionCode, requires Google's returned bundle hash to match, validates, then commits.
The release concurrency group is shared with the existing build/publish workflow, so those
jobs cannot race one another. External Console edits can still conflict; do not retry an
uncertain upload/commit until Play's actual state is checked. A code already accepted by Play
cannot be reused. No workflow retries publication automatically.

Promotion records source/QA/CI identities, qualified-byte hashes and the actual upload receipt
for 30 days. A draft is not a rollout, and a committed rollout can still take time to appear
on the phone. Verify Console and the enrolled account's Store view before saying Update is
available. This path establishes automated candidate gates, not physical phone/TV, ARM64,
Play-generated split, billing or pre-launch acceptance; those requirements remain separate.

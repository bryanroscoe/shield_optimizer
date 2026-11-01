# Google Play Console walkthrough: launching ATV Optimizer with paid Pro

This is a click-by-click guide for the owner to publish the Android app `com.atvoptimizer.mobile`
on Google Play as a free download with a one-time in-app purchase, **ATV Optimizer Pro**
(`atvopt_pro`, US $4.99). It is a companion to [`RELEASE.md`](RELEASE.md), which covers building
and signing. The build inputs it refers to (`ATVOPT_DISTRIBUTION=play`, `ATVOPT_PLAY_LICENSE_KEY`,
`ATVOPT_TELEMETRY_URL`) are described in RELEASE.md §5.

Every Google help link below was loaded on **2026-11-01** and checked against the claim it is cited
for. Play Console pages for a specific app have the developer id and app id in their URLs, which
this repo doesn't know. So console steps start from <https://play.google.com/console>, pick the
app, then follow the menu path. Google moves menu items around. If a label doesn't match, the
app's **Dashboard** still lists every outstanding setup task with a direct link to its page.

Who does each step: **OWNER** means only the account holder can do it (identity, money, Console
clicks). **AGENT** means a coding agent does it in the repo or on the build machine.
`<<OWNER: …>>` marks a decision that hasn't been made yet.

---

## One-page checklist

Critical path: **payments profile verification** and **12 closed testers opted in for 14
consecutive days**. Both run on Google's clock, so start them first.

**Phase 0: today (owner, about 1 hour, plus waiting on Google)**
- [ ] 0.1 Account checks done on the Console Home page: device verification, identity, contact email and phone
- [ ] 0.2 Payments profile created, bank account and tax info added (verification can take days)
- [ ] 0.3 Decisions recorded: support email `<<OWNER: support email>>`, public developer name `<<OWNER: developer name>>`, privacy-policy host, and who generates the upload key
- [ ] 0.4 Recruitment message sent to Ko-fi supporters and other likely testers; collecting Google account emails (target 15-20, minimum 12)

**Phase 1: app shell (owner, about 30 minutes)**
- [ ] 1.1 Upload key created and stored in 1Password with an offline backup
- [ ] 1.2 App created: name "ATV Optimizer", package `com.atvoptimizer.mobile`, App, Free
- [ ] 1.3 Licensing public key copied to 1Password as `ATVOPT_PLAY_LICENSE_KEY` and handed to the agent

**Phase 2: compliance pages (agent drafts, owner pastes, about 1-2 hours)**
- [ ] 2.1 Privacy policy published at a public URL and linked inside the app (agent code change)
- [ ] 2.2 App content: privacy policy, ads (No), app access, content rating, target audience (18+), Data safety
- [ ] 2.3 Store listing: name, short and full description, 512 icon, 1024×500 feature graphic, 4+ phone screenshots
- [ ] 2.4 Store settings: category **Tools**, contact email, website

**Phase 3: first build and product (agent builds, owner uploads, about 1 hour)**
- [ ] 3.1 Agent builds the signed Play AAB with a fresh `versionCode` and verifies the signature
- [ ] 3.2 AAB uploaded to **Internal testing** and rolled out; Play App Signing fingerprints copied to 1Password
- [ ] 3.3 One-time product `atvopt_pro` created, one Buy option, US $4.99, **Activated**
- [ ] 3.4 License testers added (owner and non-supporter testers who will try purchases)

**Phase 4: closed test, which starts the 14-day clock (owner)**
- [ ] 4.1 Closed testing track: tester email list, feedback email, same AAB, rolled out, release published
- [ ] 4.2 Opt-in link sent to every tester; 12 or more confirmed opted in. Day 1 = `<<date>>`
- [ ] 4.3 Purchase test script (RELEASE.md §6) passed on a Play-installed build

**Phase 5: supporters' Pro (owner)**
- [ ] 5.1 Promo codes created for `atvopt_pro` (500 per quarter at most) and one sent to each supporter

**Phase 6: production (owner, from day 15)**
- [ ] 6.1 Production access applied for from the Dashboard (review usually takes 7 days or less)
- [ ] 6.2 Production release rolled out after approval

---

## Requirements re-checked against Google on 2026-11-01

| Requirement | Current rule | Source |
| --- | --- | --- |
| Closed test before production | Personal accounts created after 2023-11-13 must run a closed test with **at least 12 testers opted in for the last 14 days continuously** before applying for production. The application asks about the closed test, the app and production readiness. Review "usually takes seven days or less". | <https://support.google.com/googleplay/android-developer/answer/14151465> |
| Device verification | New personal accounts must prove access to a real, non-rooted Android 10+ phone through the Play Console mobile app before the app can be made available. | <https://support.google.com/googleplay/android-developer/answer/14316361> |
| Identity verification | Personal accounts verify a contact email and phone. A government ID is needed if the linked Google payments profile isn't already verified. Device verification comes before phone verification. | <https://support.google.com/googleplay/android-developer/answer/10841920> |
| **What Google publishes about you** | Your legal name, country and developer email. **If you monetize, Google also displays your full address.** Your contact email and phone are not shown. | <https://support.google.com/googleplay/android-developer/answer/13628312> |
| Target API level | New apps and updates must target **API 36** (Android 16) from 2026-08-31. The optional extension ran to 2026-11-01, today. The app targets 36 (`build.gradle.kts`), so this is met. | <https://support.google.com/googleplay/android-developer/answer/11926878> |
| Billing Library | Version 8 or newer is required from 2026-08-31 (extension to 2026-11-01). The app uses 9.1.0, the version on Google's page. A purchase must be acknowledged within 3 days of reaching PURCHASED or Play refunds it. | <https://developer.android.com/google/play/billing/integrate> |
| Products need an uploaded build | You must publish a version that includes the Billing Library to some track (internal testing is fine) before product setup unlocks. | <https://developer.android.com/google/play/billing/getting-ready> |
| Payments profile | Required to sell in-app products: create it in Payments profile and link it to the developer account. | <https://support.google.com/googleplay/android-developer/answer/7161426>, <https://developer.android.com/google/play/billing/getting-ready> |
| Play billing only | In-app digital features must use Google Play billing, and the app "may not lead users to a payment method other than Google Play's billing system". The page's only carve-out is tax-exempt donations, which must *not* use Play billing. It says nothing that would allow a tip link, so Ko-fi links stay out of the Play build. | <https://support.google.com/googleplay/android-developer/answer/9858738> |
| Play App Signing | New apps are enrolled automatically in "quantum-ready, hybrid signing with Google-generated keys" (RSA 4096 + ML-DSA-65). The upload key must be RSA 2048 or larger. The signing key can only be changed before the first open-testing or production release. | <https://support.google.com/googleplay/android-developer/answer/9842756> |
| Privacy policy | Required for **every** app, both in the Console field and **inside the app**. It must be a public, non-geofenced, non-PDF web page. It must name the app or the listed entity, a contact, the data handled, security practices, and retention and deletion. | <https://support.google.com/googleplay/android-developer/answer/10144311>, <https://support.google.com/googleplay/android-developer/answer/9859455> |
| Data safety | Every app completes the form. "Collect" means sending data off the device. Transfers to a service provider (for example Cloudflare hosting) are not "sharing". "Optional" covers opt-out and must apply to all users. Location inferred from an IP address counts as approximate location. | <https://support.google.com/googleplay/android-developer/answer/10787469> |
| Content rating | Complete the IARC questionnaire for every app. Unrated apps are removed. | <https://support.google.com/googleplay/android-developer/answer/9898843>, <https://support.google.com/googleplay/android-developer/answer/9859655> |
| Target audience | Choose age groups. Targeting under-13s brings in the Families policy. | <https://support.google.com/googleplay/android-developer/answer/9867159> |
| Listing text | App name at most 30 characters, short description at most 80, full description at most 4,000. | <https://support.google.com/googleplay/android-developer/answer/9859152> |
| Graphics | Icon: 512×512 32-bit PNG with alpha, at most 1024 KB. Feature graphic: 1024×500 JPEG or 24-bit PNG, no alpha. Screenshots: JPEG or 24-bit PNG, 320-3840 px, long side no more than twice the short side, at least 2 to publish. Promotion-eligible: 4 or more at 1080 px or above (portrait 9:16, at least 1080×1920). Google advises against "Best"/"#1" text, device frames and third-party logos. | <https://support.google.com/googleplay/android-developer/answer/9866151> |
| Metadata and impersonation | No ranking, price or promotion claims, no unattributed testimonials. Don't imply a relationship with a company (for example Google) that doesn't exist. | <https://support.google.com/googleplay/android-developer/answer/9898842>, <https://support.google.com/googleplay/android-developer/answer/9888374> |
| Promo codes | One-time use codes only, at most **500 per quarter per app** across one-time products. A promotion lasts at most one year. Unused quota doesn't carry over. Codes need an **active Buy** purchase option. Users redeem in the purchase sheet, in the Play Store's **Redeem code**, or at `https://play.google.com/redeem?code=…`. | <https://support.google.com/googleplay/android-developer/answer/6321495>, <https://developer.android.com/google/play/billing/promo> |
| License testing | **Settings > License testing**, by email list (up to 2,000 per list) or Google Group. Testers buy without being charged. Test purchases left unacknowledged are refunded after 3 minutes. | <https://support.google.com/googleplay/android-developer/answer/6062777>, <https://developer.android.com/google/play/billing/test> |
| One-time products (2025 model) | **Monetize with Play > Products > One-time products > Create one-time product**, then a purchase option (Buy or Rent), prices, **Activate**. | <https://support.google.com/googleplay/android-developer/answer/16430488> |
| 16 KB pages | Apps targeting API 35+ must support 16 KB pages on 64-bit devices. From **2027-02-01**, updates that don't can't be released. | <https://developer.android.com/guide/practices/page-sizes> |
| Android developer verification (2026) | Verification protections started 2026-09-30 in Brazil, Indonesia, Singapore and Thailand, and expand globally in 2027. "Google Play automatically registers 99% of apps." Apps distributed outside Play are registered in Play Console. Nothing to do for the Play listing. It matters later for any sideloaded APK. | <https://developer.android.com/developer-verification> |

### Where this differs from the 2026-10-31 readiness audit and from RELEASE.md

- **Privacy policy inside the app.** Google requires a link or text inside the app as well as in
  the Console. The app has a Privacy card (telemetry toggle) but no policy link. This is an agent
  code task (Phase 2.1).
- **Monetizing publishes your full address.** Google displays the full legal address of a personal
  account that monetizes. Neither the audit nor RELEASE.md mentions this. Decide before 0.2 which
  address goes on the payments profile.
- **Play App Signing location and key type.** RELEASE.md §6.3 said "Test and release → App
  integrity". Google now documents **Protected with Play > Play Store protection > Manage Play app
  signing**. New apps get quantum-ready hybrid signing, so there are **three** app-signing
  fingerprints to record, not one.
- **Payments profile location.** Google documents **Manage account > Play apps settings >
  Monetization settings > Payments profile**, not "Settings → Payments profile".
- **License-test payment names.** The test page now says "Test instrument, always approves" for
  one-time products. The "Test card, …" wording is for subscriptions. The slow-card names are
  unchanged.
- **Approximate location in Data safety.** The Worker stores the Cloudflare-derived **country**.
  Google says location inferred from an IP address counts as approximate location. The audit left
  this open. This guide declares it.
- **Install id in Data safety.** The random install id is a "Device or other ID" in Google's sense
  (Google's own example is a Firebase installation ID), even though it is random and not tied to
  the phone.
- **Deletion.** The Worker README says a user can read their install id from the app. They can't:
  More never displays it, so a deletion request can't name its data. See the Data safety deletion
  answer.
- **License testers versus Ko-fi supporters.** RELEASE.md §6.8 said to add every closed tester as
  a license tester. This guide keeps **supporters out of the license-tester list** so their
  promo-code Pro is a normal, permanent entitlement. Google doesn't document how promo redemption
  behaves for a license tester. RELEASE.md now points here.
- **16 KB pages.** The hard stop is 2027-02-01 for updates, later than the audit implied. The
  RELEASE.md gate to verify the artifact still stands.
- The audit's 12 testers / 14 days, API 36, and Billing Library 8+ facts are confirmed unchanged.

---

## Phase 0: today, because Google's clock starts here

### 0.1 Account checks. OWNER

1. Open <https://play.google.com/console> and sign in with the developer account.
2. On **Home**, finish every verification task shown. For a new personal account these are:
   - **"Verify that you have access to an Android mobile device"** > **View details** > scan the QR
     code with a non-rooted Android 10+ phone (the Pixel works) > sign in to the Play Console app
     as the account owner > choose the developer account > **Verify**.
     Help: <https://support.google.com/googleplay/android-developer/answer/14316361>
   - Identity, contact email and contact phone (a 6-digit code by SMS or voice after device
     verification). Help: <https://support.google.com/googleplay/android-developer/answer/10841920>
3. In the account-level **Settings > Developer account** pages, set the public developer
   name `<<OWNER: developer name, for example "Bryan Roscoe" or "ATV Optimizer">>`. Don't use a
   name that suggests Google or NVIDIA (<https://support.google.com/googleplay/android-developer/answer/9888374>).

**Done when:** Home shows no outstanding verification tasks.
**Copy back to the agent:** account type (personal) and creation date, for the record.

### 0.2 Payments profile, bank and tax. OWNER (start now, verification takes days)

Before you start: a personal account that monetizes has its **full legal address shown on Google
Play** (<https://support.google.com/googleplay/android-developer/answer/13628312>). The address on
the payments profile is the one that applies. `<<OWNER: which address to use>>`

1. <https://play.google.com/console> > **Manage account > Play apps settings > Monetization
   settings > Payments profile** (Google's documented path).
2. **Create payments profile**. Under **Payments profile**, open the drop-down and choose **Create
   payments profile**.
3. Enter the legal name and legal address (no PO box), the primary contact name and email, and the
   public business info: website, product category, customer support email
   `<<OWNER: support email>>`, and the card-statement name (for example `ATV OPTIMIZER`). Click
   **Submit**.
4. Add the bank account (it must be in the payments profile's country) and the tax information
   (US: W-9) in the same payments profile. You can also manage it at <https://payments.google.com/>.
5. Complete any verification Google asks for, such as ID, address or a test deposit to the bank.

Help: <https://support.google.com/googleplay/android-developer/answer/7161426>

**Done when:** the payments profile shows as verified and linked, and the bank account is verified.
You can create the product before this, but nobody can buy until it's done.

### 0.3 Decisions to record. OWNER (5 minutes)

| Decision | Options | Why it matters |
| --- | --- | --- |
| Support and privacy contact | `<<OWNER: support email>>`, `<<OWNER: contact name shown in the privacy policy>>` | Required in store settings, the payments profile and the privacy policy. Testers see it as the feedback address. |
| Privacy-policy host | (a) GitHub Pages from this public repo, (b) a static page on the telemetry Worker's domain, (c) a personal site. `<<OWNER: choose>>` | Must be a public, non-PDF page reachable from every country. The agent drafts the text from `DATA-INVENTORY.md`. |
| Upload key custody | (a) **Agent generates**: runs RELEASE.md §1's `keytool` with random passwords, saves the `.jks` and both passwords to 1Password (item "ATV Optimizer Play upload key"), and the owner confirms the vault and makes the offline backup. (b) **Owner generates**: runs the command below and stores everything in 1Password. `<<OWNER: a or b>>` | The upload key is resettable through Play support if lost, but losing it means days of delay. |
| Telemetry deletion | See the deletion answer in Phase 2.2. `<<OWNER: choose>>` | It decides one Data safety answer. |

Option (b), run by the owner (from RELEASE.md §1; RSA 4096 is above Google's RSA 2048 minimum):

```
mkdir -p ~/keys && keytool -genkeypair -v \
  -keystore ~/keys/atv-optimizer-upload.jks \
  -alias atv-optimizer-upload \
  -keyalg RSA -keysize 4096 -validity 10000 \
  -storetype PKCS12
```

Then store `~/keys/atv-optimizer-upload.jks`, the store password and the key password in
1Password, and tell the agent where the item is. Never commit any of it.

### 0.4 Recruit testers. OWNER (send today, collect while Phases 1-3 run)

You need **at least 12 people** opted in for **14 consecutive days**. Aim for 15-20, because
anyone who leaves early resets their own count and Google may ask for more testing if engagement
is thin (<https://support.google.com/googleplay/android-developer/answer/14151465>). Each tester
needs an Android phone and the Google account they use in the Play Store.

Message to Ko-fi supporters (paste into Ko-fi messages or email; fill the placeholders):

> Subject: Early access to ATV Optimizer for Android, with Pro free for you
>
> Hi `<<name>>`,
>
> Thank you for supporting ATV Optimizer on Ko-fi. The phone version is almost ready for Google
> Play, and I'd like you to be one of its first testers.
>
> **What you get:** ATV Optimizer Pro free, permanently, through a Google Play promo code. It is
> tied to your Google account like a normal purchase, so it carries over to the public release.
>
> **What I'm asking:**
> 1. Reply with the Google account email you use in the Play Store on your Android phone.
> 2. When I send the test link (within a few days), open it on that account, tap to join, and
>    install the app from the Play Store.
> 3. **Stay opted in for at least 14 days in a row.** Google requires 12 testers for 14 straight
>    days before the app can go public. Leaving and rejoining restarts your count.
> 4. Use it with your own Android TV or Google TV a few times: connect, look around, try a
>    change and undo it. Send anything odd to `<<OWNER: support email>>`.
> 5. Redeem the promo code I send you. Don't tap "Buy" in the app, or Google will charge you.
>
> The app needs Developer options and network or wireless debugging turned on on the TV. The
> app shows where the setting is. It sends anonymous device and error reports by default (TV model, Android version,
> whether an action worked; no serial numbers, IP addresses, TV names or app lists). You can turn
> that off in More.
>
> Thanks,
> `<<OWNER: name>>`

For testers who aren't supporters, use the same message without the promo-code paragraph and
item 5. Tell them they can try purchases with Google's test payment methods and won't be charged
(they go on the license-tester list in Phase 3.4).

Keep the tester list and emails **out of this repo** (store them in 1Password or a private note).

**Done when:** you have 15 or more Google account emails.
**Copy back to the agent:** only the count, never the addresses.

---

## Phase 1: create the app shell

### 1.1 Upload key. AGENT or OWNER (per 0.3)

**Done when:** a 1Password item holds the `.jks`, the store password, the alias
`atv-optimizer-upload` and the key password, and an offline backup exists.
**Copy back to the agent:** the 1Password item name. The agent writes the gitignored
`src-tauri/gen/android/keystore.properties` locally (RELEASE.md §1).

### 1.2 Create the app. OWNER

1. <https://play.google.com/console> > **Home** > **Create app**.
2. Fill in:
   - **App name:** `ATV Optimizer` (13 of 30 characters)
   - **Package name** (if the form asks): `com.atvoptimizer.mobile`. This is permanent and must
     match `applicationId` in `src-tauri/gen/android/app/build.gradle.kts`.
   - **Default language:** English (United States), en-US
   - **App or game:** App
   - **Free or paid:** **Free**. Pro is an in-app product, not a paid download.
   - **Contact email:** `<<OWNER: support email>>`
3. Tick the declarations: Developer Program Policies, US export laws, and the Play App Signing
   Terms of Service.
4. **Create app**.

Help: <https://support.google.com/googleplay/android-developer/answer/9859152>

**Done when:** the app's Dashboard opens.

### 1.3 Licensing key, which goes into every Play build. OWNER

1. In the app: **Monetize with Play > Monetization setup**. Google documents this page as
   "Monetize > Monetization setup" (<https://developer.android.com/google/play/billing/getting-ready>).
2. Find the **Licensing** section and copy the **Base64-encoded RSA public key**, a single line
   starting `MIIB…`.
   Caveat: Google's own licensing doc still calls this location "Services & APIs"
   (<https://developer.android.com/google/play/licensing/setting-up>). If no Licensing section
   shows before the first upload, tell the agent: a Play release build fails without this key
   (RELEASE.md §5).
3. Save it in 1Password as `ATVOPT_PLAY_LICENSE_KEY`. It's public, per-app and permanent, but keep
   it out of Git.

**Copy back to the agent:** the key (or its 1Password item name) → `ATVOPT_PLAY_LICENSE_KEY`.
**Done when:** the agent confirms a `play` release build accepts the key.

---

## Phase 2: compliance pages (do these while testers reply)

If **Create new release** is greyed out on a track, the Dashboard setup tasks below aren't done
yet (<https://support.google.com/googleplay/android-developer/answer/9859348>). The **Dashboard >
Set up your app** list links to each page.

### 2.1 Privacy policy. AGENT drafts and wires up; OWNER approves and publishes

- **AGENT:** draft the policy from `DATA-INVENTORY.md` and `v2/telemetry-worker/README.md`. It
  must cover the operator and contact, data on the phone, data sent to the TV, the anonymous
  reports and their fields, the 90-day raw retention, what is never sent, Google Play billing
  (payment handled by Google; the purchase record stays on the phone), and how to turn reports off
  and request deletion. Label it "Privacy Policy" and name the app. Add a **Privacy policy** link
  in More (it opens in the browser) through a normal PR. Both are required by
  <https://support.google.com/googleplay/android-developer/answer/10144311>.
- **OWNER:** approve the text, publish it at `<<OWNER: privacy policy URL>>`, and check that it
  loads in a private window with no login.

**Done when:** the URL loads publicly and a build with the in-app link exists.

### 2.2 App content declarations. OWNER (paste the answers below)

Path: in the app, **Policy and programs > App content**. Google's articles currently label it
both "Monitor and improve > Policy and programs > App content" and "Policy > App content". Each
item has **Start** (or **Manage**). Help: <https://support.google.com/googleplay/android-developer/answer/9859455>

**Privacy policy:** paste `<<OWNER: privacy policy URL>>` > **Save**.

**Ads:** "No, my app does not contain ads" > **Save**.

**App access** (Google's help now calls it **Sign-in details**). No login exists, but a reviewer
can't use the core features without a TV. Choose the option that lets you add instructions (the
"restricted" or "+ Add new instructions" path) and paste:

> No account or login is needed. The app manages an Android TV or Google TV on the same Wi-Fi
> network. To try it: on the TV, turn on Developer options (usually Settings > Device
> Preferences > About, then tap Build repeatedly; the menu varies by TV maker). Then turn on
> Settings > System > Developer options > Wireless debugging (Android 11+), or Network debugging
> on SHIELD and older TVs. Open the app; it finds the TV and the TV asks you to allow the
> connection. Without a TV, the app opens to onboarding, More (settings, privacy, licenses) and
> the purchase screen. Demo video: `<<OWNER: unlisted video URL>>`.

The menu path matches the app's onboarding text (`src/screens/Onboarding.svelte`).

**Content rating:** **Start**. Enter `<<OWNER: email for IARC>>`, pick the category and click
**Next**. The category list is only visible in the Console. A device-management utility normally
fits the generic "all other app types" choice; pick the closest one and don't choose Game,
Social or Reference. Answers:

| Topic | Answer | Why |
| --- | --- | --- |
| Violence, fear, sexuality, profanity/crude humor, drugs, alcohol, tobacco | No | None in the app. |
| Gambling or simulated gambling | No | None. |
| Users interact or exchange content with each other | No | No chat, accounts or user-generated content. |
| Shares the user's current physical location with other users | No | The country is derived server-side for telemetry and never shown to anyone. |
| Allows users to purchase digital goods | **Yes** | ATV Optimizer Pro, through Google Play. |
| Unrestricted internet or web browser access | No | No in-app browser. External links open the system browser on user tap. |

Then **Save draft** > review the **Summary** > **Submit**. The likely result is a low age rating (Everyone / PEGI 3 or
similar) with an "In-App Purchases" notice. IARC decides, not us.
Help: <https://support.google.com/googleplay/android-developer/answer/9859655>, <https://support.google.com/googleplay/android-developer/answer/9898843>

**Target audience and content:** target age **18 and over** only. It's a developer-options tool
that changes system apps on a TV, and leaving out under-13s keeps it out of the Families policy.
Appeal to children: No. Help: <https://support.google.com/googleplay/android-developer/answer/9867159>

**Other declarations** (News, Health, Government, Financial features, and similar, if listed):
the app is none of these. The permissions declaration form doesn't apply: the manifest has only
INTERNET, ACCESS_NETWORK_STATE, ACCESS_WIFI_STATE and CHANGE_WIFI_MULTICAST_STATE, plus BILLING
from the library.

**Data safety:** see the next section.

**Done when:** App content shows every item complete.

### 2.2a Data safety answers (from `DATA-INVENTORY.md` and the Worker code)

Help: <https://support.google.com/googleplay/android-developer/answer/10787469>.
These answers assume the release build **sets `ATVOPT_TELEMETRY_URL`**. Without it nothing
leaves the phone for the developer, and the answer to the first question would be No.

**Data collection and security**

| Question | Answer | Basis |
| --- | --- | --- |
| Does your app collect or share any of the required user data types? | **Yes** | Anonymous reports go to the owner's Cloudflare Worker. |
| Is all of the user data collected by your app encrypted in transit? | **Yes** | HTTPS only. Non-HTTPS URLs other than loopback are ignored at build time (RELEASE.md, Telemetry endpoint). |
| Do you provide a way for users to request that their data is deleted? | **See "Deletion" below** | Owner decision. |

**Data types.** Each one: Collected **Yes**, Shared **No** (Cloudflare is a service provider
processing on the owner's behalf, which Google excludes from sharing), Processed ephemerally
**No** (stored), Required or optional **Optional** (users can turn reports off in More > Privacy,
in every region), Purpose **Analytics**.

| Category > data type | What is actually sent | Notes |
| --- | --- | --- |
| Location > **Approximate location** | Country only, from Cloudflare's `request.cf.country`, stored per event | Google counts location inferred from an IP address. No city or region is stored, and the IP itself is never stored. |
| App activity > **App interactions** | Action kind (for example `disable`) and whether it succeeded | Not which app it ran on. |
| App info and performance > **Crash logs** | Error events (`panic`, `uncaught_error` and others) with a short code and a redacted message of at most 500 characters | Redacted on the phone and again on the server. |
| App info and performance > **Diagnostics** | App version, phone OS release, and the TV's manufacturer, brand, model, codename, product, Android release, SDK level, detected family and support tier | TV build properties only. No serial. |
| Device or other IDs > **Device or other IDs** | Random UUID v4 install id | Not derived from hardware. Regenerated when reports are turned back on. Google's example of this type is a Firebase installation ID. |

**Not collected** (answer No or leave unticked): Personal info (name, email, address, phone),
Financial info including purchase history (Play handles payment; the signed purchase record is
verified and stored only on the phone, and reports never include order id, token, price or
product), Messages, Photos and videos (TV screenshots stay on the phone), Audio, Files and docs
(pulled files stay on the phone), Calendar, Contacts, Web browsing, Health and fitness, Precise
location, and Installed apps (TV package lists are never sent). Saved TVs, ADB keys and snapshots
never leave the phone, so they aren't "collected".

**Deletion.** Be honest; this is an owner decision. What exists today:
- Raw events are deleted automatically 90 days after receipt (daily cron).
- Aggregate tables keep the install id per device group (`agg_device_installs`) indefinitely.
  That means Google's "automatically deleted or anonymized within 90 days" description isn't fully
  met.
- An operator can delete one install's rows with SQL (Worker README, Retention), but the app
  **doesn't show the install id**, so a user has no way to say which rows are theirs.
- Turning reports off stops sending and discards anything unsent. Turning them back on starts a
  new id, so old and new data aren't linked.

`<<OWNER: choose one>>`
1. **Answer No for now.** It's truthful, and there are no accounts, so no deletion mechanism is
   required. The privacy policy explains the opt-out and the 90-day raw retention.
2. **Make it Yes (agent work, about half a day):** show a copyable install id in More > Privacy,
   name the support email in the privacy policy as the deletion channel, and have the owner run
   the README's `DELETE` statements on request. Optionally also purge or anonymize
   `agg_device_installs.install_id` after 90 days, so the automatic-deletion description fits.

**Disclosure note (review risk, not a confirmed violation):** reports are on before the user taps
anything, with a one-time, non-blocking first-launch notice. Google requires prominent in-app
disclosure plus affirmative consent before collecting *personal and sensitive* data that users
wouldn't expect (<https://support.google.com/googleplay/android-developer/answer/10144311>).
These reports are designed to be anonymous, but they carry a pseudonymous id and a country. If
review objects, the fix is a first-launch choice screen. Keep the "Optional" answer either way.

**Done when:** Data safety shows a preview that matches the table above, saved and submitted.

### 2.3 Store listing. AGENT prepares assets and text; OWNER pastes and uploads

Path: **Grow users > Store presence > Store listings** > the default (main) listing.
Help: <https://support.google.com/googleplay/android-developer/answer/9859152>, assets <https://support.google.com/googleplay/android-developer/answer/9866151>

**App name** (30 maximum): `ATV Optimizer`

**Short description** (80 maximum; this one is 77):

```
Manage apps and settings on your own Android TV or Google TV from your phone.
```

**Full description** (4,000 maximum; this one is about 2,200). No speed or "boost" claims, no
rankings, and no implied endorsement:

```
ATV Optimizer manages your own Android TV or Google TV from your phone, over your home Wi-Fi, using the TV's built-in developer debugging connection. No computer is needed.

See what is on your TV
• Device details: model, Android version, storage and memory use
• Every installed app, with its storage and memory footprint and what is safe to change
• Screenshots of the TV screen and a basic remote control

Free, always
• Connect to TVs on your network and check whether yours is supported before you pay
• Diagnostics and the full app list
• Re-enable apps, and emergency recovery that turns disabled apps back on

ATV Optimizer Pro, a one-time purchase
• Curated clean-up: turn off or remove preinstalled apps you do not use, from a reviewed list that refuses to touch apps the TV needs
• Optimize: review a plan before anything changes, then apply it
• Launcher: set a different Home app and turn off the stock one
• Tweaks: change system settings such as HDMI-CEC, frame-rate matching, display scaling and Private DNS
• Snapshots: record your TV's configuration and re-apply it later
Pro is a one-time purchase through Google Play. No subscription. Undoing settings, Home app or snapshot changes uses Pro features.

What you need
• An Android TV or Google TV device on the same Wi-Fi network as your phone
• Developer options turned on, with network debugging (older TVs and SHIELD) or wireless debugging (Android 11 and newer). The app shows you where to find it.
• Fire TV is not supported: Fire OS blocks the changes this app makes.

Be careful
Turning off system apps can change how your TV behaves. The app warns you before risky changes, and every disabled app can be turned back on for free.

Privacy
Your saved TVs, app lists and files stay on your phone. The app sends anonymous device and error reports by default (TV model, Android version, whether an action worked). It never sends serial numbers, IP addresses, TV names or app lists. Turn reports off in More.

ATV Optimizer is an independent app. It is not affiliated with, endorsed by or sponsored by Google or NVIDIA. Android TV and Google TV are trademarks of Google LLC. SHIELD is a trademark of NVIDIA Corporation.
```

`<<AGENT: before the owner pastes this, check every bullet against the build being shipped. Drop
"wireless debugging (Android 11 and newer)" if code pairing hasn't passed the physical test
(PAIRING-PLAN.md). Drop "basic remote control" if it is not in the release.>>`

**Graphics**

| Asset | Spec (Google) | Source in repo / status |
| --- | --- | --- |
| App icon | 512×512, 32-bit PNG with alpha, at most 1024 KB | `v2/mobile/src-tauri/icons/icon.png` is 512×512 RGBA. AGENT checks the size is at most 1024 KB. |
| Feature graphic | 1024×500, JPEG or 24-bit PNG, no alpha. Avoid pure white, black or dark gray backgrounds and device images. | **Missing.** AGENT renders one from the brand assets (wordmark and icon on a brand color, no device frame, no Google or NVIDIA logos). OWNER approves. |
| Phone screenshots | 2-8; JPEG or 24-bit PNG; 320-3840 px; aspect at most 2:1. Use 4 or more at 1080×1920 or larger (9:16) for promotion eligibility. No device frames or "Best"/"#1" text. | AGENT runs `cd v2/mobile && npm run screenshots` (demo data, offline) with a scale that gives 1080 px or more width, then picks 4-8 frames (Dashboard, Apps, Optimize plan, Launcher, Tweaks, More). Captures are 412×915 at the chosen scale (about 2.2:1), so **crop or pad to 9:16** before upload. Add alt text of 140 characters or less. |

**Done when:** the listing saves with no errors.

### 2.4 Store settings. OWNER

Path: **Grow users > Store presence > Store settings** (listed under Store presence in Google's
help).

- **App or game:** App. **Category:** **Tools**. Tags: pick the closest offered (for example
  device management or utilities), up to 5.
- **Contact details:** email `<<OWNER: support email>>` (required); website
  `https://github.com/bryanroscoe/shield_optimizer` or the privacy-policy site; phone optional
  (leave blank).

**Done when:** saved, and the Dashboard ticks off the listing tasks.

---

## Phase 3: first build, internal track, product

### 3.1 Build the AAB. AGENT

Follow RELEASE.md §2-5 and its testing-candidate gates. In particular:

- a new explicit `bundle.android.versionCode`, never used before on any track
- `ATVOPT_DISTRIBUTION=play`, `ATVOPT_PLAY_LICENSE_KEY=<from 1.3>`,
  `ATVOPT_TELEMETRY_URL=<deployed Worker https base>` exported in the same shell
- `npm run tauri android build -- --aab`, then `jarsigner -verify` on the AAB, and the bundletool
  and 16 KB checks
- the build must include the privacy-policy link from 2.1

Follow the build-machine limits (one compiler job; check disk space first).

**Done when:** a signed `app-universal-release.aab` exists and the agent reports its SHA-256 and
`versionCode`.

### 3.2 Upload to internal testing. OWNER

1. **Test and release > Testing > Internal testing** > **Testers** tab: create an email list with
   your own account, then **Save changes**.
2. **Create new release**. Under **Add your app bundles**, upload the AAB.
3. Play App Signing: accept Google's default (Google-generated, quantum-ready hybrid key). Don't
   choose to upload your own app signing key.
4. **Name your release:** `0.x.y (versionCode)`. Release notes:
   ```
   <en-US>
   First test build.
   </en-US>
   ```
5. **Next** > fix anything under the errors summary > **Save** > open **Publishing overview** and
   send the changes for review (or click **Start rollout** / **Publish** when the page offers it).
   Help: <https://support.google.com/googleplay/android-developer/answer/9859348>
6. Record the fingerprints: **Protected with Play > Play Store protection > Manage Play app
   signing**. Copy the SHA-256 of **each** app-signing key shown (hybrid signing shows three) and
   the **Upload key certificate** SHA-256 into 1Password.
   Help: <https://support.google.com/googleplay/android-developer/answer/9842756>

**Copy back to the agent:** the SHA-256 fingerprints (not secret). The agent checks that the
upload-key fingerprint matches the keystore it signed with.
**Done when:** the internal release is available and installs from the Play Store link on your
phone.

### 3.3 Create `atvopt_pro`. OWNER

Unlocked only after 3.2 (<https://developer.android.com/google/play/billing/getting-ready>).

1. **Monetize with Play > Products > One-time products > Create one-time product**.
2. **Product ID:** `atvopt_pro`, exactly. It's permanent and can never be reused. It must match
   `PRO_PRODUCT_ID` in `src-tauri/src/play_billing.rs`.
   **Name:** `ATV Optimizer Pro`.
   **Description:** `Unlocks curated clean-up, Optimize, Launcher, Tweaks and Snapshots. One-time purchase.`
3. **Tax, compliance, and programs:** mark it as digital content (an app feature unlock, not a
   service) > **Next**.
4. **Purchase option:** ID `buy` (lowercase), type **Buy**. Leave quantities at 1 (no
   multi-quantity).
5. **Set prices**: United States **USD 4.99**. Let Play fill the other regions (bulk edit if
   prompted: **Set prices > Bulk edit pricing**, all regions, **Apply**). Leave every region
   available.
6. **Activate** both the purchase option and the product. Only Active options reach the app and
   can carry promo codes.

Help: <https://support.google.com/googleplay/android-developer/answer/16430488>

**Done when:** the product shows Active, and the app (installed from Play) shows the local price
in More and on the paywall.

### 3.4 License testers. OWNER

1. Open <https://play.google.com/console> at the account level (not inside the app) >
   **Settings > License testing**.
2. Add an email list with **your account and any non-supporter testers who will try purchases**.
   **Leave Ko-fi supporters off this list**, because their Pro comes from a promo code (Phase 5),
   which should behave like a real, permanent purchase.
3. **Save changes**.

Help: <https://support.google.com/googleplay/android-developer/answer/6062777>,
<https://developer.android.com/google/play/billing/test>

**Done when:** a license tester sees test payment methods ("Test instrument, always approves",
"Slow test card, …") in the purchase sheet.

---

## Phase 4: closed testing, the 14-day clock

### 4.1 Create the closed track. OWNER

1. **Test and release > Testing > Closed testing** > the default track > **Manage track**.
2. **Testers** tab: **Create email list** named `ko-fi + early testers`, paste every email from
   0.4 (comma-separated or CSV) > **Save changes** > **Create**. Tick the list.
3. **Feedback URL or email address:** `<<OWNER: support email>>`. It shows on the opt-in page.
4. **Countries/regions** (if asked): all. Country choices apply to every track.
5. **Releases** tab: **Create new release** > **Add from library** > the same AAB as internal >
   release notes > **Next** > **Save** > send for review from **Publishing overview**.
6. Wait for review. The opt-in link only appears once the track is **Published**.
7. Back on **Testers**, copy the shareable opt-in link.

Help: <https://support.google.com/googleplay/android-developer/answer/9845334>

**Copy back to the agent:** the date the release was published.
**Done when:** the track status is Published and the opt-in link opens a "Become a tester" page.

### 4.2 Get 12+ opted in. OWNER

Send each tester the opt-in link, with the reminder to join **on the same Google account** whose
email they gave, install from the Play Store, and stay in for 14 days. Day 1 is the first day you
have 12 opted in. Keep your own count of who has joined. Google doesn't document a progress
counter, so if the Dashboard shows one, treat it as a bonus.

**Done when:** 12 or more opted in. Write the day-1 date in this file's checklist (outside Git if
you prefer).

### 4.3 Purchase test. OWNER (with the agent watching logs)

Run the **Purchase test script** in RELEASE.md §6 on a Play-installed build as a license tester,
plus a promo-code redemption (Phase 5). If a purchase isn't acknowledged, Play refunds a license
tester's test purchase after 3 minutes, so "still Pro after 5 minutes" is a good check.

**Done when:** every line of the script passes. Report failures to the agent with the time and
the More > Debug log.

---

## Phase 5: Pro for Ko-fi supporters. OWNER

1. **Monetize with Play > Promo codes > Create promo code**.
2. Type: one-time-use codes for a one-time product. Product **`atvopt_pro`**, purchase option
   **`buy`** (it must be an active Buy option).
3. Name it `ko-fi-supporters-2026Q4`, set the start date to today and the end date to the
   longest offered (up to 1 year), and the quantity to the number of supporters plus a few spare.
   The limit is 500 per quarter across all one-time products. Unused quota doesn't carry over.
4. **Create**, then download the codes. Keep them in 1Password, never in this repo.
5. Send each supporter one code with the redeem link
   `https://play.google.com/redeem?code=<CODE>`, or tell them Play Store > profile menu >
   **Redeem code**, or the **Redeem** link in the app's purchase sheet.

Help: <https://support.google.com/googleplay/android-developer/answer/6321495>,
<https://developer.android.com/google/play/billing/promo>

The app re-queries owned purchases at launch and on return to the foreground, so a code redeemed
in the Play Store unlocks Pro the next time the app opens.

**Done when:** one supporter confirms Pro, and **Order management** shows the redemption.

---

## Phase 6: production

### 6.1 Apply for production access. OWNER (day 15 or later)

1. **Dashboard** > the production-access task. Google says you apply "from the Dashboard" once
   the criteria are met; the exact button label isn't documented.
2. Answer the three sections: **About your closed test** (how testers were recruited, i.e. Ko-fi
   supporters and issue reporters, how they used it, the feedback received), **About your app**
   (it manages the user's own Android TV from a phone; the audience is Android TV owners
   comfortable with developer options), and **About your production readiness** (what changed
   because of testing; list the real fixes).

Help: <https://support.google.com/googleplay/android-developer/answer/14151465>

**Done when:** Google approves (usually within 7 days). If it asks for more testing, keep the
closed track running.

### 6.2 Release to production. OWNER, with an AGENT build

1. AGENT: build a fresh AAB with a new `versionCode` if anything changed (RELEASE.md).
2. **Test and release > Production > Create new release** > add the AAB > notes > **Next** >
   **Save** > send for review. Choose a staged rollout percentage if offered. For a first
   production release the button reads **Start rollout to production**.
3. Watch the pre-launch report and **Order management** for the first real orders.

**Done when:** the listing is live and a non-tester account can buy Pro.

---

## Later: sideloaded APKs and Android developer verification

Play automatically registers the Play listing's package
(<https://developer.android.com/developer-verification>). If you later distribute a sideloaded
APK outside Play, for example on GitHub, it has to be registered in Play Console as distributed
outside Google Play, before the protections reach users' regions (Brazil, Indonesia, Singapore
and Thailand since 2026-09-30, then globally in 2027). Also, Play re-signs installs with Google's
app-signing key, so a sideloaded APK signed with the upload key can't update a Play install, or
the reverse.

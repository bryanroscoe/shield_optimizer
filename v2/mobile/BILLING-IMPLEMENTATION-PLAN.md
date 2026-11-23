# Android commerce implementation plan

Status: **v1 built (on-device verification, no backend); not yet exercised against a real Play
account or device.** See "Implemented v1" below. The product ids (`atvopt_pro`, the retired
`atvopt_pro_support_1` and `atvopt_pro_support_2`, and the tips `atvopt_tip_2`, `atvopt_tip_5`,
`atvopt_tip_10`, `atvopt_tip_20`) are in `src-tauri/src/play_billing.rs`; no Play account,
credential, payment or external tip link is configured in the repo. Sideload/dev builds keep the signed offline license key in
[`LICENSING.md`](LICENSING.md). Nothing here authorizes a store submission or a real charge.

## Implemented v1 (2026-10-31)

What exists, and how it deviates from the server design below:

- **Native adapter**: `tauri-plugin-atv-billing` (Kotlin, Play Billing Library 9.1.0). Connect,
  `ProductDetails` for each product, purchase sheet, `queryPurchasesAsync(INAPP)`, acknowledge and
  consume. It makes no entitlement decision and consumes only the purchase tokens Rust hands it
  (verified tips, never Pro); it returns Play's signed `originalJson` and `signature` untouched.
  No JS-invokable plugin commands.
- **Verification is on the phone**, in Rust (`src-tauri/src/play_billing.rs`): SHA1withRSA against
  the app's Play license public key compiled in from `ATVOPT_PLAY_LICENSE_KEY`, then package name,
  product id (one of the Pro products below) and `purchaseState == 0` from the signed JSON. No key in the build means purchases
  show as unavailable and Play never grants Pro. A patched APK can bypass on-device checks; accepted
  for a one-time unlock at this price. A server verifier can later sit behind the same source.
- **Entitlement**: `billing_commands.rs` keeps two independent sources (Play, license key) and
  pushes their union into `AppState`, so every existing `require_pro` / `LOCKED:` gate applies
  unchanged. Neither source converts into or revokes the other.
- **Lifecycle**: owned purchases are queried at launch, whenever the app becomes visible again
  (rate-limited to once per 30 s), on Restore purchases and after every purchase flow. PENDING is
  shown and never granted or acknowledged. A verified PURCHASED purchase is granted, cached, then
  acknowledged; a failed acknowledgment retries at the next query (Play refunds unacknowledged
  purchases after three days). A successful query without the purchase (refund, revocation,
  another account) drops Play Pro and clears the cache.
- **Offline policy**: the last verified purchase is stored in `play_purchase.json` and re-verified
  on every read. When Play cannot be asked, it keeps Pro for at most 7 days after Play last
  confirmed it (`OFFLINE_GRACE_MS`); a clock more than a day behind that confirmation does not count.
  So a refund can go unnoticed for up to 7 days only while Play is unreachable. A fresh install
  never gets Pro offline.
- **Policy**: Play builds (`ATVOPT_DISTRIBUTION=play`) hide key entry in the UI and the backend
  refuses `activate_license`. Ko-fi supporters get Pro through Play promo codes; redemption arrives
  as a normal purchase (verified tests cover a promo-shaped purchase with no `orderId`).
- **Retired support tiers** (in the app 2026-11-08 to 2026-11-23, never created in Play Console,
  so never sold): `atvopt_pro_support_1` ($9.99) and `atvopt_pro_support_2` ($19.99) were
  non-consumable products that unlocked exactly the same Pro.
  The tip jar replaced them (owner decision 2026-11-23): the purchase UI now sells only
  `atvopt_pro`, and `billing_purchase` refuses any other id. Both ids stay in `PRO_PRODUCT_IDS`
  forever, so verification, restore and `purchase_gate` still treat them as Pro. Don't create them
  in Play Console, and never reuse the ids. Play refuses a second copy of
  one product but not a different Pro product, so `billing_purchase` still queries owned
  purchases first and starts a purchase only when Play confirms the account owns no Pro product
  (`purchase_gate`). Every unacknowledged Pro purchase is acknowledged, not just the first.
- **Tip jar** (owner decision 2026-11-23): four **consumable** products, `atvopt_tip_2` ($2),
  `atvopt_tip_5` ($5), `atvopt_tip_10` ($10) and `atvopt_tip_20` ($20), buyable any number of
  times. A tip grants nothing: no Pro, badge, feature or stored tip count. Play builds only
  (`ATVOPT_DISTRIBUTION=play` with working billing), and only for Pro owners: a quiet "Leave a
  tip" link on More's Licensing card opens the tips Play returned a price for, at Play's localized
  price, with one line saying a tip doesn't unlock anything and a one-line thank-you afterwards.
  No popups, prompts or nags. `billing_tip` (gated the same way) opens Play's sheet for one tip.
  - Verification is the same signed-JSON check as Pro, through `PlayVerifier::verify_tip`: a
    purchase is a tip only when every product it names is a tip id. `TIP_PRODUCT_IDS` and
    `PRO_PRODUCT_IDS` share no id (a test holds them apart), a purchase that names Pro and a tip
    together counts as Pro, and `verify` never accepts a tip, so a tip can never grant Pro or
    satisfy `purchase_gate`.
  - A verified PURCHASED tip is consumed (Play's `consumeAsync`, which also acknowledges it).
    Only `reconcile_tips` makes the `ConsumableTip` tokens the consume call accepts, so a Pro
    purchase can never be consumed. PENDING tips are shown as pending and never consumed until
    Play reports them PURCHASED. Every owned-purchase query (launch, resume, Restore purchases,
    `purchases-updated`) consumes any verified, unconsumed PURCHASED tip, before Play's three-day
    refund of unacknowledged purchases. `ITEM_ALREADY_OWNED` on a tip means an earlier one of the
    same amount was never consumed; the app runs a tip-only owned-purchase query
    (`apply_tip_query`), which consumes it, and asks the user to try again. The same tip-only query
    runs when a tip flow returns OK without the tip. It never touches Pro's state, cache,
    acknowledgment or refresh clock, so a tip can't change the entitlement even when Play's list
    is missing Pro or the query fails past offline grace.
  - A consume Play doesn't confirm is retried in the background while the app runs: after 30 s,
    doubling to at most 30 min, for 8 scheduled tries (about 1.5 hours), under the same Play
    operation lock as every other call. Every later owned-purchase query tries again too, and a
    retried tip the owned list no longer shows is dropped. At most one native consume runs per
    purchase token: a call Rust stopped waiting for (30 s timeout) keeps its in-flight mark until
    it actually returns, and `ITEM_NOT_OWNED` counts as already consumed. While a tip is pending
    the frontend re-checks every 30 s, as it does for a pending Pro purchase, and the pending note
    follows the backend's `tip_pending`, so it clears once Play settles the tip.
  - A purchase-flow callback answers the open sheet only with purchases of the product it was
    opened for. Play also sends unrelated updates through the same listener (an earlier pending
    tip clearing while another tip's sheet is open); the Kotlin adapter routes those to the
    `purchases-updated` watch, whose full owned-purchase refresh consumes verified tips and grants
    Pro as usual. Rust checks the same thing on the verified JSON: only purchases naming the
    requested tip decide what the user is told, other verified tips in the callback are consumed,
    and anything else re-emits `purchases-updated`.
  - `billing_tip` checks the tip gate (Play build, billing available, Pro unlocked) before and
    again after taking the Play operation lock, so a refresh that removed Pro meanwhile stops a
    queued tip before its sheet opens. Tip prices have their own 10-minute retry clock, separate
    from Pro's price lookup, so a newly unlocked Pro shows the tip jar on its next refresh.
  - Tips never change `PlayState`, `play_purchase.json` or the entitlement. Tips are not reported
    to telemetry. Sideload and dev builds show nothing and never query or consume tips.
  - Play has no buyer-entered price, so "pay what you want" is these fixed amounts.
- **Ko-fi stays out of the Play app.** Offering Pro codes for an outside donation inside the app
  would steer purchases of a digital unlock away from Play billing. The Ko-fi link and the
  "donate and get a Pro code" offer live on the GitHub README only; codes are Play promo codes
  for `atvopt_pro`.
- **Not built**: server verification, RTDN, Voided Purchases reconciliation, account linking.
  Free settings/Home/snapshot undo is still unbuilt, so all copy promises only that re-enabling
  apps and emergency recovery are always free.

## Approved product direction

- Free compatibility checks before purchase; one-time non-consumable Pro, intended US price $4.99 (owner decision 2026-10-31; was $2.99). The $9.99 and $19.99 same-Pro support tiers (2026-11-08) were retired for the tip jar on 2026-11-23.
- No timed trial, subscription or recurring charge.
- Complimentary Pro for existing Ko-fi supporters uses the normal Google Play one-time-product
  promo redemption flow and the same verification/restore logic. No separate APK, client-only
  unlock, hard-coded supporter key or alternate entitlement bypass. No codes issued or outreach
  performed by this plan.
- Optional pure support tips confer no app functionality, entitlement, digital item, badge,
  priority support or other digital benefit. In the Play app they go through Play billing (the
  tip jar above). External tips (Ko-fi) stay out of the app with no URL.
- Preserve free recovery/undo even after payment failure, refund, revocation or loss of access.
- Display Google's localized ProductDetails price and terms, not a hard-coded universal price.
  USD intent does not establish other currencies, taxes or regional price settings.

## Current gate audit (no gates changed)

| Existing path | Current evidence / implication |
|---|---|
| Discovery/connect/device status, health and package reads | Free API paths; keep enough real-device evidence visible to assess compatibility before purchase. Never claim compatibility from fabricated/unknown responses. |
| `apps::enable_package_impl`, `apps::reinstall_existing` | No require_pro gate in these recovery paths; retain free access and existing package/identity safeguards. |
| `recovery::panic_recovery`, More emergency recovery | Not Pro-gated. Keep reachable without billing service, entitlement or network verification. It re-enables packages; it is not a complete settings rollback. |
| `apps::disable_package_impl`, uninstall | CuratedDebloat Pro gate; preserve safety/last-home checks independently of entitlement. |
| `optimize::prepare_optimize_impl` | OptimizeWizard gate currently blocks plan preparation. Compatibility UI must not claim a free preview exists until separately implemented/tested. |
| Launcher set Home / disable stock | LauncherTakeover gates; re-enabling a package remains free. Audit UI reachability after downgrade and safe restoration of the prior Home choice. |
| `tuning::write_setting`, scaling and DNS setters | TweaksWrite gates also cover restoring old values. Thus current implementation does NOT prove all settings undo is free. Resolve this before paid release. |
| `apps::set_app_permission_impl`, app-op writes | AppPermissionWrite gates; distinguish new changes from narrowly authorized undo. |
| Snapshot list/delete/save/plan/apply | Snapshot gates apply even to list/delete. Existing snapshots re-apply a disabled/settings configuration; they are NOT complete undo and can perform new destructive changes. Do not simply make arbitrary apply free. |
| `Feature::FileManager` / BackupClone | Enum entries alone do not establish enforcement; backlog notes ungated paths. Inventory actual call sites before changing pricing gates. |
| `activate_license`, startup license.json | Offline signed-key flow, not Play purchase or restore. Keep legacy entitlement provenance separate; do not convert unverified local JSON into a Play purchase. |

Phase 0 must finish the full command/UI matrix and add Free-state tests for every recovery path.
For reversible settings/permissions/Home changes, propose a bounded recorded-before-value undo
mechanism: exact verified device identity, operation provenance, expected-current-value guard,
finite allowed keys/packages, explicit confirmation, and no arbitrary shell/settings bypass.
Persist before mutation; preserve journal/snapshots through downgrade; never auto-replay TV writes
on billing callbacks. Details and migration need separate review before implementation. Do not
advertise universal undo until this gate is met. Scope does not promise to recover uninstalled
third-party APKs or missing data.

## Sales (Play discount offers)

A temporary price cut on Pro is a discount offer on `atvopt_pro`'s purchase option `pro-unlock` in
Play Console (Monetize › Products › One-time products), with a start and end time. No app release
is needed to start or stop one. Play lists only the offers this buyer may use now, in no
documented order, so the plugin charges and shows the cheapest one and reports Play's full price,
percentage and end time; More and the Pro sheet show them through `SaleNote`. Pro's price is
asked for again on every refresh until Pro is unlocked, and product details are queried again
just before the purchase sheet, so a sale that starts or ends is the one charged. The app never
computes or hard-codes a sale price.

## Supporter promo redemption and later roadmap

Plan Play one-time-product promo codes for owner-selected existing Ko-fi supporters. Eligibility,
code availability, expiry, quotas and account requirements must be verified in the existing Play
account against the official promo documentation before issuance. Do not invent numeric limits.
Redeemed purchases must follow the same PURCHASED verification, idempotent delivery,
acknowledgment and owned-purchase restore path; promo purchases may lack a normal order ID, so
purchase tokens remain the processing key. Test redemption inside/outside the app, app installed
later, already-owned Pro, expired/invalid/used codes, repeated restore and refund/revocation
where applicable. Do not require supporters to buy again merely because a promo purchase lacks
an order ID. No supporter list, promo code or private purchase data belongs in source control.
Owner selection, code creation and distribution/outreach remain separate authorized actions.

Keep the current repository for Android launch. Root `LICENSE` states all rights reserved and
publishes source for transparency/review; source availability is not a permissive reuse license.
No visibility, license or repository split is proposed or performed here. Native client code must
never contain publisher credentials or private verification/signing secrets regardless of visibility.

Record iOS as **post-Android launch** roadmap work, not this implementation: separately assess
platform transport feasibility, App Store policy, StoreKit verification/restore and licensing,
privacy and physical-device validation. Do not assume Android Play entitlements automatically
transfer to Apple purchases or promise iOS feature parity/date before that assessment.

## Planned architecture

1. Android-native Google Play Billing adapter behind a narrow Tauri plugin; choose/pin a currently
   supported Billing Library at implementation preflight and add its resolved notices. Connection,
   product query, launch, updates and owned-purchase query are separate operations. Browser tests
   use an injected fake; unsupported hosts return unavailable, never fabricated Pro.
2. Pure Rust entitlement state/reducer with explicit source (Play verified / legacy signed key),
   verification status and retryable errors; retain one canonical backend enforcement layer.
   A frontend boolean or successful billing-sheet launch never grants Pro. Coalesce duplicate
   launches and fence asynchronous results by purchase/account/request identity, not TV endpoint.
3. A narrowly scoped verification service interface, mocked until hosting/authentication is
   reviewed. Sensitive Google publisher credentials stay server-side. Verify purchase token,
   package, exact approved product/quantity and PURCHASED state with Google before entitlement.
   Store token processing idempotently: repeated delivery/restore of the same valid purchase must
   not double-grant, double-acknowledge or be mistaken for fraud. An order ID is not the token key.
4. Authenticating the caller and proving token ownership across restore/reinstall needs an exact
   design before exposing the service. Do not ship a public token-to-Pro oracle, assume a local
   device ID authenticates a purchase, or invent accounts/credentials. Account requirements,
   service hosting, retention and any signed offline-assertion key custody remain owner/security
   gates; this plan creates none of them.
5. Verified entitlement must be durably committed before acknowledgment. Prefer server-side
   acknowledgment and a durable idempotent retry queue, so app termination does not lose delivery.
   Do not consume this non-consumable (only tips are consumed). Acknowledge promptly and monitor Google's deadline; pending
   purchases are not acknowledged or granted until PURCHASED. Duplicate acknowledged deliveries
   safely reconcile. No real refund API calls are authorized by this plan.
6. Proposed UX: compatibility result → localized Pro offer → Play purchase sheet → pending /
   verifying / purchased. Provide Restore purchases, retry, and support paths without hiding free
   tools. No provider or release product ID is guessed or created. Purchase unavailable remains
   an honest unavailable state, not an invitation to an unapproved external checkout.

## Required state transitions and recovery

| Event | Required behavior |
|---|---|
| Product query unavailable / disconnected billing | Explain unavailability; keep compatibility and recovery usable; bounded reconnect/retry, no duplicate purchase launch. |
| User cancels | Return quietly to Free/current verified entitlement; cancellation is not a charge or a TV failure. |
| PENDING | Show pending; no Pro grant, no acknowledgment; reconcile purchase updates and foreground owned purchases. |
| PURCHASED but verification times out | Show verification pending, persist retry intent, no first-time Pro grant based only on client callback. |
| Verified PURCHASED | Persist entitlement and idempotent delivery, then acknowledge; expose failures/retries without double-charging. |
| Already owned | Reconcile owned purchases and restore rather than launching another charge. |
| Restore / reinstall / second eligible phone | Query owned non-consumable purchases, reverify through the same path, restore idempotently; no charge, no requirement to retain a copied license key. |
| Process death during purchase/ack | Recover from Play-owned queries and durable service state; never rely on one callback. |
| Known revoked / canceled entitlement | Remove Play-derived Pro for new premium actions; preserve independently valid legacy source if applicable, customer artifacts and free recovery. |
| Unknown network error | Do not equate temporary inability to query with confirmed revocation or claim refunds are observed instantly offline. |

## Refunds, revocation and offline behavior

Reconcile foreground owned-purchase queries with verified server state. Use authenticated,
idempotent RTDN handling and the appropriate Google API lookup; a notification is a wake-up,
not an independently trusted entitlement assertion. Cover voided/refunded/revoked one-time
purchases and delayed/reordered notifications; do not revoke on a notification type unrelated
to the approved product. Backfill gaps with bounded reconciliation/Voided Purchases handling.

No design can discover a new refund while completely offline. Proposals for a cached verified
entitlement must explicitly define freshness, tamper/clock handling and the maximum offline
revocation delay. That policy and any key/service needed to enforce it remain unresolved; it is
not a customer trial and cannot silently become one. Never grant an initial purchase offline.
On fresh install offline, compatibility and free recovery work; purchase restoration cannot be
claimed complete until ownership is reverified. Do not promise lifetime offline irrevocability.

## Bounded implementation sequence and tests

1. Free recovery gate audit and failing-before/passing-after regression PR. Verify downgrade
   cannot trap the user with app-written settings, lose snapshots, or trigger unsolicited undo.
2. Pure entitlement reducer, fake store adapter and UI state PR. No live payment calls; exhaustive
   pending/cancel/error/duplicate/reorder/late response/account switch/process-death tests.
3. Native billing plugin and verifier contract PR, disabled without approved runtime configuration.
   Mock package/product/token validation, replay resistance and legitimate repeated restores.
4. Verification/ack/revocation service implementation only after a separately approved exact
   hosting/authentication/key/retention proposal. Test transactional delivery, acknowledgment
   failure/retry, RTDN duplicates and refunds; never place publisher secrets in client bundles.
5. Store sandbox test only after product/account authorization: pending-to-paid/canceled, offline
   restart, already-owned, restore, refund/revoke, same account across phones and account switch.
   Capture simulated videos first; real test purchases and physical-device operations need their
   own bounded execution scope. No real customer charges during verification.
6. Refresh privacy/Data safety, notices, localized offer/restore/support copy and release checklist.
   Tips are built through Play billing (consumables, see "Tip jar"); tests prove no
   benefit/entitlement change when tipped. A tip exception is not assumed merely because a
   button says "support".

## Official references

- Purchase integration: https://developer.android.com/google/play/billing/integrate
- Verification/ack/voided purchases: https://developer.android.com/google/play/billing/security
- One-time lifecycle: https://developer.android.com/google/play/billing/lifecycle/one-time
- Notifications: https://developer.android.com/google/play/billing/rtdn-reference
- Current product lookup: https://developers.google.com/android-publisher/api-ref/rest/v3/purchases.productsv2/getproductpurchasev2
- Promo reference (recheck in the account before issuance): https://support.google.com/googleplay/android-developer/answer/6321495
- Payment/tip applicability: https://support.google.com/googleplay/android-developer/answer/9858738

Check API/library versions and policy again when implementing. These references are technical
inputs, not proof of account eligibility or a completed policy review.

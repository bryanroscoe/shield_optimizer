//! Google Play billing commands and the combined entitlement.
//!
//! Two independent entitlement sources feed `AppState`'s single gate
//! (`require_pro` / `LOCKED:`): the offline Ed25519 license key (sideload and
//! dev builds) and a verified Google Play purchase. [`BillingCore`] holds both
//! and recomputes the effective entitlement whenever either changes. Neither
//! source is ever converted into the other, and losing one never revokes the
//! other.
//!
//! The owned-purchase query runs at launch, whenever the WebView becomes
//! visible again (`billing_refresh` from the frontend), when Play reports a
//! purchase made outside the app such as a redeemed promo code (the billing
//! plugin's `purchases-updated` event, which the frontend answers with a forced
//! `billing_refresh`) and after every purchase flow. It is what restores Pro on a new install and what notices refunds and
//! revocations.
//!
//! The same query also finishes tips (Play builds only): every verified,
//! PURCHASED tip it lists is consumed, which acknowledges it before Play's
//! three-day refund deadline. Tips are kept apart from Pro end to end: they
//! never change `PlayState`, the cache or the entitlement. A tip flow that
//! needs the owned list runs a tip-only query that leaves Pro alone, and a
//! consume Play doesn't confirm is retried on a bounded backoff while the app
//! runs (and again by every later query), with at most one native consume in
//! flight per purchase token.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use atv_optimizer_core::commands::AppState;
use atv_optimizer_core::license::Entitlement;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_atv_billing::{
    response_code, BillingExt, ProductInfo, RawPurchase, SaleInfo, PURCHASES_UPDATED_EVENT,
};

use crate::play_billing::{
    cached_owned_state, effective_entitlement, is_tip_product, reconcile, reconcile_flow,
    reconcile_tips, response_message, CacheAction, CachedPurchase, ConsumableTip, Distribution,
    PlayState, PlayVerifier, QueryOutcome, TipsReconciled, PLAY_LICENSE_KEY, PRO_PRODUCT_ID,
    TIP_PRODUCT_IDS,
};

/// Automatic refreshes (launch, resume) closer together than this reuse the
/// last answer. Restore purchases and purchase flows always ask Play.
const AUTO_REFRESH_MIN_INTERVAL_MS: i64 = 30_000;
/// A tip price Play didn't return (inactive in Play Console, say) is asked for
/// again at most this often. A missing standard Pro price is always retried.
const OFFER_RETRY_MS: i64 = 10 * 60 * 1000;
/// A tip consume Play didn't confirm is retried after this, doubling each time
/// up to [`TIP_RETRY_MAX_MS`], for at most [`TIP_RETRY_ATTEMPTS`] scheduled
/// tries per run. Every later owned-purchase query tries again too, well
/// inside Play's three-day acknowledgment deadline.
const TIP_RETRY_BASE_MS: i64 = 30_000;
const TIP_RETRY_MAX_MS: i64 = 30 * 60 * 1000;
const TIP_RETRY_ATTEMPTS: u32 = 8;
const QUERY_TIMEOUT: Duration = Duration::from_secs(30);
/// The purchase sheet stays open while the user decides; this only bounds an
/// abandoned flow (for instance an activity destroyed mid-sheet).
const PURCHASE_TIMEOUT: Duration = Duration::from_secs(15 * 60);

const NO_KEY_REASON: &str = "Purchases aren't available in this build of the app.";
const NOT_ANDROID_REASON: &str = "Google Play purchases are only available in the Android app.";
const UNREACHABLE: &str = "Couldn't reach Google Play. Check your connection and try again.";
/// Buy found Pro already on the account: it unlocks, and nothing is charged.
const OWNED_RESTORED: &str = "Pro restored from Google Play. Nothing was charged.";
/// Play's sheet refused a second copy, but its owned-purchase list on this
/// phone doesn't have the first yet (a code redeemed on the website, say).
const OWNED_NOT_SENT: &str = "Google Play has this purchase on your account but hasn't sent it to this phone yet, so nothing was charged. Wait a minute, then tap Restore purchases.";

const TIP_THANKS: &str = "Thank you for the tip!";
const TIPS_PLAY_ONLY: &str = "Tips are only available in the Google Play version of the app.";
const TIPS_NEED_PRO: &str = "Tips are available once Pro is unlocked.";

/// Play's localized offer for one product (Pro or a tip), for display only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Offer {
    pub product_id: String,
    pub formatted_price: String,
    pub title: String,
    pub name: String,
    pub description: String,
    /// Present while Play's offer is a discount (a Play Console sale).
    pub sale: Option<Sale>,
}

/// How Play describes a running discount: the full price it replaces, the
/// percentage off and when it ends. Never computed or hard-coded here; only
/// the full price's formatting is the phone's, as Play gives just its amount.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Sale {
    pub full_formatted_price: Option<String>,
    pub percent_off: Option<u32>,
    pub ends_at_ms: Option<i64>,
}

impl Sale {
    fn from_play(s: SaleInfo, price_micros: i64) -> Option<Self> {
        if s.full_price_micros <= price_micros {
            return None;
        }
        let percent_off = s.percentage_discount.filter(|p| (1..100).contains(p));
        let full_formatted_price = s.full_formatted_price.filter(|p| !p.trim().is_empty());
        if full_formatted_price.is_none() && percent_off.is_none() {
            return None;
        }
        Some(Sale {
            full_formatted_price,
            percent_off,
            ends_at_ms: s.end_time_millis.filter(|t| *t > 0),
        })
    }
}

impl From<ProductInfo> for Offer {
    fn from(p: ProductInfo) -> Self {
        let sale = p
            .sale
            .and_then(|s| Sale::from_play(s, p.price_amount_micros));
        Offer {
            product_id: p.product_id,
            formatted_price: p.formatted_price,
            title: p.title,
            name: p.name,
            description: p.description,
            sale,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct BillingStatus {
    pub entitlement: Entitlement,
    /// Which sources currently grant Pro: `"play"`, `"license"`, both or none.
    pub sources: Vec<&'static str>,
    pub distribution: Distribution,
    /// Whether this build accepts offline license keys (never in Play builds).
    pub license_key_entry: bool,
    /// Whether Buy / Restore purchases can work in this build on this device.
    pub available: bool,
    pub play: PlayState,
    /// The standard Pro product, the only one the app sells for Pro.
    pub offer: Option<Offer>,
    /// Whether this build may offer tips: a Play build where billing works.
    pub tips_available: bool,
    /// Tips Play returned a price for, in display order. Asked for only while
    /// Pro is unlocked, the only time the app shows them. A tip unlocks nothing.
    pub tip_offers: Vec<Offer>,
    /// A tip is waiting on payment. It is finished once Play confirms it.
    pub tip_pending: bool,
    /// A granted purchase Play has not yet confirmed as acknowledged; retried
    /// on the next refresh.
    pub ack_pending: bool,
    pub product_id: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PurchaseResult {
    Purchased,
    Pending,
    Canceled,
    Restored,
    AlreadyOwned,
    Unavailable,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
pub struct PurchaseOutcome {
    pub outcome: PurchaseResult,
    pub message: String,
    pub status: BillingStatus,
}

#[derive(Debug)]
struct Inner {
    license_pro: bool,
    play: PlayState,
    cache: Option<CachedPurchase>,
    /// Offers Play returned: the standard Pro product and tips.
    offers: Vec<Offer>,
    /// When tip prices were last asked for. Pro's price has no cooldown.
    tip_offers_checked_ms: Option<i64>,
    last_query_ms: Option<i64>,
    ack_pending: bool,
    tip_pending: bool,
    /// Verified tips Play hasn't confirmed consuming, with their retry plan.
    tip_retries: Vec<TipRetry>,
    /// Tokens with a native consume call still running. A call Rust stopped
    /// waiting for keeps running, so this clears only when the call returns.
    consuming: HashSet<String>,
    /// Whether the background tip retry task is running.
    retry_task: bool,
}

#[derive(Debug)]
struct TipRetry {
    tip: ConsumableTip,
    attempts: u32,
    /// When to try again; `None` once the scheduled tries are used up (later
    /// owned-purchase queries still try).
    next_ms: Option<i64>,
}

fn tip_retry_backoff_ms(attempts: u32) -> i64 {
    let doublings = attempts.saturating_sub(1).min(16);
    (TIP_RETRY_BASE_MS << doublings).min(TIP_RETRY_MAX_MS)
}

/// A purchase-flow result for a tip, split into the tip that was asked for
/// and whatever else Play delivered alongside it.
#[derive(Debug, Default)]
pub struct TipFlow {
    /// Purchases of the requested tip. Only these decide what the user is told.
    pub requested: TipsReconciled,
    /// Other verified, PURCHASED tips in the same callback, consumed as usual.
    pub other_tips: Vec<ConsumableTip>,
    /// Something in the callback that isn't a verified tip (a Pro purchase
    /// completing, say). The full owned-purchase refresh handles it.
    pub other_purchases: bool,
}

/// Billing state for one app run. Pure apart from its locks, so the state
/// transitions are unit-tested without Tauri or Play.
pub struct BillingCore {
    verifier: Option<PlayVerifier>,
    distribution: Distribution,
    inner: Mutex<Inner>,
    /// Serializes every Play operation (query, purchase, acknowledge), so a
    /// resume refresh can never interleave with a purchase flow.
    op: tokio::sync::Mutex<()>,
}

/// What a reconciliation asks the caller to do outside the state lock.
#[derive(Debug, PartialEq, Eq)]
pub struct Applied {
    pub cache: CacheAction,
    pub acknowledge: Vec<String>,
    /// Verified tips to consume. Never a Pro purchase (see [`ConsumableTip`]).
    pub consume: Vec<ConsumableTip>,
}

impl BillingCore {
    pub fn new(
        key: Option<&str>,
        package_name: &str,
        distribution: Distribution,
        on_android: bool,
        cache: Option<CachedPurchase>,
        license_pro: bool,
        now_ms: i64,
    ) -> Self {
        let verifier = match key {
            Some(k) => match PlayVerifier::from_base64(k, package_name) {
                Ok(v) => Some(v),
                Err(e) => {
                    tracing::error!(error = %e, "ATVOPT_PLAY_LICENSE_KEY is unusable; Play billing disabled");
                    None
                }
            },
            None => None,
        };
        let play = match (&verifier, on_android) {
            (None, _) => PlayState::Unavailable {
                reason: NO_KEY_REASON.into(),
            },
            (Some(_), false) => PlayState::Unavailable {
                reason: NOT_ANDROID_REASON.into(),
            },
            (Some(v), true) => cache
                .as_ref()
                .and_then(|c| cached_owned_state(v, c, now_ms))
                .unwrap_or(PlayState::Unknown),
        };
        let verifier = verifier.filter(|_| on_android);
        Self {
            verifier,
            distribution,
            inner: Mutex::new(Inner {
                // A Play build never counts an offline key, even one left in
                // app data by an earlier sideload install.
                license_pro: license_pro && distribution.allows_license_keys(),
                play,
                cache,
                offers: Vec::new(),
                tip_offers_checked_ms: None,
                last_query_ms: None,
                ack_pending: false,
                tip_pending: false,
                tip_retries: Vec::new(),
                consuming: HashSet::new(),
                retry_task: false,
            }),
            op: tokio::sync::Mutex::new(()),
        }
    }

    /// The production configuration: compiled-in key and distribution flag.
    pub fn from_build(
        package_name: &str,
        cache: Option<CachedPurchase>,
        license_pro: bool,
        now_ms: i64,
    ) -> Self {
        Self::new(
            PLAY_LICENSE_KEY,
            package_name,
            Distribution::current(),
            cfg!(target_os = "android"),
            cache,
            license_pro,
            now_ms,
        )
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn available(&self) -> bool {
        self.verifier.is_some()
    }

    pub fn distribution(&self) -> Distribution {
        self.distribution
    }

    /// Tips are a Google Play build feature, and need working billing.
    pub fn tips_enabled(&self) -> bool {
        self.verifier.is_some() && self.distribution == Distribution::Play
    }

    /// Take the Play operation lock for a tip, then check the tip gate again:
    /// a refresh that held the lock meanwhile may have removed Pro.
    pub async fn lock_for_tip(&self) -> Result<tokio::sync::MutexGuard<'_, ()>, String> {
        let op = self.op.lock().await;
        match tip_gate(self) {
            Some(reason) => Err(reason),
            None => Ok(op),
        }
    }

    pub fn effective(&self) -> Entitlement {
        let inner = self.lock();
        effective_entitlement(inner.license_pro, &inner.play)
    }

    pub fn play(&self) -> PlayState {
        self.lock().play.clone()
    }

    pub fn set_license_pro(&self, pro: bool) {
        self.lock().license_pro = pro && self.distribution.allows_license_keys();
    }

    /// Products whose price Play hasn't returned yet this run: the standard
    /// Pro product, and the tips while Pro is unlocked in a build with tips.
    pub fn missing_offers(&self) -> Vec<&'static str> {
        let tips = self.tips_enabled() && self.effective() == Entitlement::Pro;
        let inner = self.lock();
        std::iter::once(PRO_PRODUCT_ID)
            .chain(TIP_PRODUCT_IDS.into_iter().filter(|_| tips))
            .filter(|id| !inner.offers.iter().any(|o| o.product_id == *id))
            .collect()
    }

    /// The prices worth asking Play for now. Pro's price is asked for on
    /// every refresh until Pro is unlocked, so a sale starting or ending
    /// shows up on the next launch or resume; missing tip prices at most
    /// every [`OFFER_RETRY_MS`], on a clock that starts only when tips are
    /// first asked for, so Pro unlocking shows the tip jar on the next refresh.
    pub fn offers_to_query(&self, now_ms: i64) -> Vec<&'static str> {
        let mut missing = self.missing_offers();
        if self.effective() != Entitlement::Pro && !missing.contains(&PRO_PRODUCT_ID) {
            missing.insert(0, PRO_PRODUCT_ID);
        }
        let mut inner = self.lock();
        let tips_due = match inner.tip_offers_checked_ms {
            Some(last) => now_ms - last >= OFFER_RETRY_MS || now_ms < last,
            None => true,
        };
        let wanted: Vec<_> = missing
            .into_iter()
            .filter(|id| *id == PRO_PRODUCT_ID || tips_due)
            .collect();
        if wanted.iter().any(|id| is_tip_product(id)) {
            inner.tip_offers_checked_ms = Some(now_ms);
        }
        wanted
    }

    /// Record Play's offer for the Pro product or a tip. Anything else
    /// (including the retired support tiers) is dropped.
    pub fn set_offer(&self, offer: Offer) {
        if offer.product_id != PRO_PRODUCT_ID && !is_tip_product(&offer.product_id) {
            return;
        }
        let mut inner = self.lock();
        inner.offers.retain(|o| o.product_id != offer.product_id);
        inner.offers.push(offer);
    }

    pub fn should_auto_refresh(&self, now_ms: i64) -> bool {
        match self.lock().last_query_ms {
            Some(last) => now_ms - last >= AUTO_REFRESH_MIN_INTERVAL_MS || now_ms < last,
            None => true,
        }
    }

    pub fn apply_query(&self, outcome: QueryOutcome<'_>, now_ms: i64) -> Applied {
        let Some(verifier) = &self.verifier else {
            return Applied {
                cache: CacheAction::Keep,
                acknowledge: Vec::new(),
                consume: Vec::new(),
            };
        };
        let succeeded = matches!(outcome, QueryOutcome::Owned(_));
        let tips = match &outcome {
            QueryOutcome::Owned(purchases) if self.tips_enabled() => {
                Some(reconcile_tips(verifier, purchases))
            }
            _ => None,
        };
        let mut inner = self.lock();
        let r = reconcile(verifier, inner.cache.as_ref(), outcome, now_ms);
        if r.rejected > 0 {
            tracing::warn!(
                rejected = r.rejected,
                "Play returned purchases that failed verification"
            );
        }
        inner.play = r.state;
        if succeeded {
            inner.last_query_ms = Some(now_ms);
            inner.ack_pending = !r.acknowledge.is_empty();
        }
        // A complete owned list is the whole truth about pending tips too.
        if let Some(t) = &tips {
            Self::note_owned_tips(&mut inner, t);
        }
        Self::apply_cache(&mut inner, &r.cache);
        Applied {
            cache: r.cache,
            acknowledge: r.acknowledge,
            consume: tips.map(|t| t.consume).unwrap_or_default(),
        }
    }

    /// Apply a purchase-flow result. `None` means it said nothing about Pro and
    /// a full owned-purchase query should decide.
    pub fn apply_flow(&self, purchases: &[RawPurchase], now_ms: i64) -> Option<Applied> {
        let verifier = self.verifier.as_ref()?;
        let r = reconcile_flow(verifier, purchases, now_ms)?;
        let mut inner = self.lock();
        inner.play = r.state;
        inner.ack_pending = !r.acknowledge.is_empty();
        Self::apply_cache(&mut inner, &r.cache);
        Some(Applied {
            cache: r.cache,
            acknowledge: r.acknowledge,
            consume: Vec::new(),
        })
    }

    /// Apply a tip purchase flow's result. Only purchases naming `product`
    /// count as this flow's; any other tip in the callback is consumed but
    /// never reported as this one. It never touches Pro: the Play state, the
    /// cache and the entitlement stay exactly as they were.
    pub fn apply_tip_flow(&self, purchases: &[RawPurchase], product: &str) -> TipFlow {
        let Some(verifier) = self.verifier.as_ref().filter(|_| self.tips_enabled()) else {
            return TipFlow::default();
        };
        let (mine, others): (Vec<_>, Vec<_>) = purchases
            .iter()
            .cloned()
            .partition(|p| verifier.names_product(p, product));
        let requested = reconcile_tips(verifier, &mine);
        let other = reconcile_tips(verifier, &others);
        let other_purchases = others.iter().any(|p| verifier.verify_tip(p).is_err());
        if requested.pending || other.pending {
            self.lock().tip_pending = true;
        }
        TipFlow {
            requested,
            other_tips: other.consume,
            other_purchases,
        }
    }

    /// Fold a tip-only owned-purchase query: the verified tips to consume.
    /// Pro's state, cache, acknowledgment and refresh clock are left exactly
    /// as they were, whatever the list says about Pro; a failed query changes
    /// nothing at all.
    pub fn apply_tip_query(&self, outcome: QueryOutcome<'_>) -> Vec<ConsumableTip> {
        let (Some(verifier), QueryOutcome::Owned(purchases)) = (
            self.verifier.as_ref().filter(|_| self.tips_enabled()),
            outcome,
        ) else {
            return Vec::new();
        };
        let tips = reconcile_tips(verifier, purchases);
        Self::note_owned_tips(&mut self.lock(), &tips);
        tips.consume
    }

    /// A complete owned list settles pending tips, and any retried tip it no
    /// longer lists has been consumed (or refunded): stop retrying it.
    fn note_owned_tips(inner: &mut Inner, tips: &TipsReconciled) {
        inner.tip_pending = tips.pending;
        inner.tip_retries.retain(|r| tips.consume.contains(&r.tip));
    }

    /// Claim a tip for a native consume call. `false` while another call for
    /// the same token is still running, so Play never sees two at once.
    pub fn begin_consume(&self, tip: &ConsumableTip) -> bool {
        self.lock().consuming.insert(tip.token().to_string())
    }

    /// Record how a native consume call ended (`done`: Play consumed it, or
    /// no longer has it to consume). Returns whether a retry is now scheduled.
    pub fn consume_finished(&self, tip: &ConsumableTip, done: bool, now_ms: i64) -> bool {
        let mut inner = self.lock();
        inner.consuming.remove(tip.token());
        if done {
            inner.tip_retries.retain(|r| r.tip != *tip);
            return false;
        }
        let index = match inner.tip_retries.iter().position(|r| r.tip == *tip) {
            Some(i) => i,
            None => {
                inner.tip_retries.push(TipRetry {
                    tip: tip.clone(),
                    attempts: 0,
                    next_ms: None,
                });
                inner.tip_retries.len() - 1
            }
        };
        let retry = &mut inner.tip_retries[index];
        retry.attempts += 1;
        retry.next_ms = (retry.attempts <= TIP_RETRY_ATTEMPTS)
            .then(|| now_ms + tip_retry_backoff_ms(retry.attempts));
        retry.next_ms.is_some()
    }

    /// Start the retry task unless it is already running.
    fn claim_retry_task(&self) -> bool {
        !std::mem::replace(&mut self.lock().retry_task, true)
    }

    /// How long until the next scheduled tip retry. `None` ends the retry
    /// task (in the same lock, so a later failure starts a new one).
    fn next_tip_retry_in(&self, now_ms: i64) -> Option<i64> {
        let mut inner = self.lock();
        let next = inner
            .tip_retries
            .iter()
            .filter(|r| !inner.consuming.contains(r.tip.token()))
            .filter_map(|r| r.next_ms)
            .min();
        if next.is_none() {
            inner.retry_task = false;
        }
        next.map(|t| (t - now_ms).max(0))
    }

    /// Tips whose scheduled retry is due and that have no call in flight.
    pub fn due_tip_retries(&self, now_ms: i64) -> Vec<ConsumableTip> {
        let inner = self.lock();
        inner
            .tip_retries
            .iter()
            .filter(|r| r.next_ms.is_some_and(|t| t <= now_ms))
            .filter(|r| !inner.consuming.contains(r.tip.token()))
            .map(|r| r.tip.clone())
            .collect()
    }

    fn apply_cache(inner: &mut Inner, action: &CacheAction) {
        match action {
            CacheAction::Keep => {}
            CacheAction::Store(c) => inner.cache = Some(c.clone()),
            CacheAction::Clear => inner.cache = None,
        }
    }

    pub fn acknowledged(&self, ok: bool) {
        let mut inner = self.lock();
        inner.ack_pending = !ok;
        if ok {
            if let PlayState::Owned { acknowledged, .. } = &mut inner.play {
                *acknowledged = true;
            }
        }
    }

    pub fn status(&self) -> BillingStatus {
        let inner = self.lock();
        let mut sources = Vec::new();
        if inner.play.grants_pro() {
            sources.push("play");
        }
        if inner.license_pro {
            sources.push("license");
        }
        BillingStatus {
            entitlement: effective_entitlement(inner.license_pro, &inner.play),
            sources,
            distribution: self.distribution,
            license_key_entry: self.distribution.allows_license_keys(),
            available: self.verifier.is_some(),
            play: inner.play.clone(),
            offer: inner
                .offers
                .iter()
                .find(|o| o.product_id == PRO_PRODUCT_ID)
                .cloned(),
            tips_available: self.tips_enabled(),
            tip_offers: TIP_PRODUCT_IDS
                .iter()
                .filter(|_| self.tips_enabled())
                .filter_map(|id| inner.offers.iter().find(|o| o.product_id == *id).cloned())
                .collect(),
            tip_pending: inner.tip_pending,
            ack_pending: inner.ack_pending,
            product_id: PRO_PRODUCT_ID,
        }
    }
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn cache_path(data_dir: &Path) -> PathBuf {
    data_dir.join("play_purchase.json")
}

/// Read the last verified purchase. Anything unreadable is simply absent; the
/// contents are re-verified before they can count.
pub fn read_cache(data_dir: &Path) -> Option<CachedPurchase> {
    let text = std::fs::read_to_string(cache_path(data_dir)).ok()?;
    match serde_json::from_str(&text) {
        Ok(cache) => Some(cache),
        Err(e) => {
            tracing::warn!(error = %e, "play_purchase.json unreadable; ignoring");
            None
        }
    }
}

/// Persist a cache decision. A failure only shortens offline grace: Play
/// remains the durable record of the purchase, so it is logged, not fatal, and
/// never blocks acknowledgment (an unacknowledged purchase is refunded).
pub fn persist_cache(data_dir: &Path, action: &CacheAction) {
    let path = cache_path(data_dir);
    let result = match action {
        CacheAction::Keep => Ok(()),
        CacheAction::Clear => match std::fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
            _ => Ok(()),
        },
        CacheAction::Store(cache) => std::fs::create_dir_all(data_dir)
            .map_err(|e| e.to_string())
            .and_then(|_| serde_json::to_string(cache).map_err(|e| e.to_string()))
            .and_then(|json| {
                crate::write_replacing(&path, json.as_bytes()).map_err(|e| e.to_string())
            }),
    };
    if let Err(e) = result {
        tracing::warn!(error = %e, "could not update play_purchase.json");
    }
}

/// Push the combined entitlement into the one gate every command checks.
pub fn sync_entitlement(state: &AppState, core: &BillingCore) {
    state.set_entitlement(core.effective());
}

async fn plugin_call<T, F>(app: &AppHandle, timeout: Duration, f: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce(
            &tauri_plugin_atv_billing::Billing<tauri::Wry>,
        ) -> tauri_plugin_atv_billing::Result<T>
        + Send
        + 'static,
{
    let app = app.clone();
    let task = tokio::task::spawn_blocking(move || f(app.billing()).map_err(|e| e.to_string()));
    match tokio::time::timeout(timeout, task).await {
        Ok(Ok(result)) => result,
        Ok(Err(join)) => Err(join.to_string()),
        Err(_) => Err("Google Play didn't respond in time.".into()),
    }
}

/// Acknowledge every listed purchase; one failure leaves `ack_pending` set so
/// the next refresh retries whatever Play still reports unacknowledged.
async fn acknowledge(app: &AppHandle, core: &BillingCore, tokens: Vec<String>) {
    if tokens.is_empty() {
        return;
    }
    let mut all_ok = true;
    for token in tokens {
        let result = plugin_call(app, QUERY_TIMEOUT, move |b| b.acknowledge(&token)).await;
        if !matches!(&result, Ok(r) if r.response_code == response_code::OK) {
            tracing::warn!(?result, "acknowledgment failed; will retry on next refresh");
            all_ok = false;
        }
    }
    core.acknowledged(all_ok);
}

/// Consume verified tips (Play's consume also acknowledges). Only a
/// [`ConsumableTip`] can reach the native consume call, so a Pro purchase never
/// does. A failure schedules a bounded retry and never touches Pro. Returns
/// whether Play confirmed every tip within the wait.
async fn consume_tips(app: &AppHandle, tips: Vec<ConsumableTip>) -> bool {
    let mut all_ok = true;
    for tip in tips {
        all_ok &= consume_one(app, tip).await;
    }
    all_ok
}

/// One native consume. The call's own thread records its result, so a call
/// Rust stopped waiting for still clears its in-flight mark (and schedules a
/// retry if it failed) when it finally returns.
async fn consume_one(app: &AppHandle, tip: ConsumableTip) -> bool {
    let core = app.state::<BillingCore>();
    if !core.begin_consume(&tip) {
        tracing::info!("tip consume already in flight; not starting another");
        return false;
    }
    let call_app = app.clone();
    let task = tokio::task::spawn_blocking(move || {
        let result = call_app.billing().consume(tip.token());
        // ITEM_NOT_OWNED: nothing left to consume (an earlier call finished it).
        let done = matches!(&result, Ok(r) if r.response_code == response_code::OK
            || r.response_code == response_code::ITEM_NOT_OWNED);
        if !done {
            tracing::warn!(?result, "tip consume failed; retrying later");
        }
        let core = call_app.state::<BillingCore>();
        if core.consume_finished(&tip, done, now_ms()) {
            schedule_tip_retries(&call_app);
        }
        done
    });
    match tokio::time::timeout(QUERY_TIMEOUT, task).await {
        Ok(Ok(done)) => done,
        Ok(Err(join)) => {
            tracing::warn!(error = %join, "tip consume task failed");
            false
        }
        Err(_) => {
            tracing::warn!("tip consume is taking long; it finishes in the background");
            false
        }
    }
}

/// Run scheduled tip retries in the background until none are left. Each
/// retry takes the Play operation lock like every other Play call.
fn schedule_tip_retries(app: &AppHandle) {
    if !app.state::<BillingCore>().claim_retry_task() {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let core = app.state::<BillingCore>();
        while let Some(delay) = core.next_tip_retry_in(now_ms()) {
            tokio::time::sleep(Duration::from_millis(delay as u64)).await;
            let _op = core.op.lock().await;
            let due = core.due_tip_retries(now_ms());
            consume_tips(&app, due).await;
        }
    });
}

/// Ask Play for owned purchases and finish only the tips in them. Unlike
/// [`refresh_locked`], it never changes Pro's state, cache or entitlement.
/// Caller holds `core.op`.
async fn tip_recovery_locked(app: &AppHandle, core: &BillingCore) {
    let response = plugin_call(app, QUERY_TIMEOUT, |b| b.query_purchases()).await;
    let consume = match &response {
        Ok(r) if r.response_code == response_code::OK => {
            core.apply_tip_query(QueryOutcome::Owned(&r.purchases))
        }
        other => {
            tracing::info!(?other, "tip-only owned-purchase query failed");
            Vec::new()
        }
    };
    consume_tips(app, consume).await;
}

fn finish(app: &AppHandle, core: &BillingCore, cache: &CacheAction) {
    if let Ok(dir) = app.path().app_data_dir() {
        persist_cache(&dir, cache);
    }
    sync_entitlement(&app.state::<AppState>(), core);
}

/// Query owned purchases and fold them in. Caller holds `core.op`.
async fn refresh_locked(app: &AppHandle, core: &BillingCore) {
    let response = plugin_call(app, QUERY_TIMEOUT, |b| b.query_purchases()).await;
    let applied = match &response {
        Ok(r) if r.response_code == response_code::OK => {
            core.apply_query(QueryOutcome::Owned(&r.purchases), now_ms())
        }
        Ok(r) => core.apply_query(
            QueryOutcome::Failed(response_message(r.response_code, &r.debug_message)),
            now_ms(),
        ),
        Err(e) => {
            tracing::warn!(error = %e, "owned-purchase query failed");
            core.apply_query(QueryOutcome::Failed(UNREACHABLE.into()), now_ms())
        }
    };
    finish(app, core, &applied.cache);
    acknowledge(app, core, applied.acknowledge).await;
    consume_tips(app, applied.consume).await;
    for id in core.offers_to_query(now_ms()) {
        match plugin_call(app, QUERY_TIMEOUT, move |b| b.query_product(id)).await {
            Ok(r) if r.response_code == response_code::OK => {
                if let Some(product) = r.product.filter(|p| p.product_id == id) {
                    core.set_offer(Offer::from(product));
                }
            }
            Ok(r) => tracing::info!(id, code = r.response_code, "product details unavailable"),
            Err(e) => tracing::info!(id, error = %e, "product details unavailable"),
        }
    }
}

/// Launch/resume/restore entry point. Never fails: problems land in the
/// returned status, and a failure never grants anything.
pub async fn refresh(app: &AppHandle, force: bool) -> BillingStatus {
    let core = app.state::<BillingCore>();
    if !core.available() {
        return core.status();
    }
    let _op = core.op.lock().await;
    if force || core.should_auto_refresh(now_ms()) {
        refresh_locked(app, &core).await;
    }
    core.status()
}

/// `billing_status` — current entitlement, its sources and the Play state,
/// without contacting Play.
#[tauri::command]
pub async fn billing_status(core: State<'_, BillingCore>) -> Result<BillingStatus, String> {
    Ok(core.status())
}

/// `billing_refresh` — ask Play for owned purchases (launch, resume and the
/// Restore purchases button with `force`).
#[tauri::command]
pub async fn billing_refresh(app: AppHandle, force: Option<bool>) -> Result<BillingStatus, String> {
    Ok(refresh(&app, force.unwrap_or(false)).await)
}

/// Whether a purchase may start, given what Play just said about ownership.
/// Only a confirmed "not owned" may: anything less certain could charge an
/// existing Pro owner again for another Pro product. An owner who taps Buy
/// (say after redeeming a code elsewhere) is told Pro is restored, never sent
/// to Play's "you already own this" sheet.
pub fn purchase_gate(play: &PlayState) -> Option<(PurchaseResult, String)> {
    match play {
        PlayState::NotOwned => None,
        PlayState::Owned { .. } => Some((
            PurchaseResult::AlreadyOwned,
            OWNED_RESTORED.into(),
        )),
        PlayState::Pending => Some((
            PurchaseResult::Pending,
            "A purchase is already pending. Pro unlocks as soon as Google Play confirms the payment.".into(),
        )),
        PlayState::Unavailable { reason } => Some((PurchaseResult::Unavailable, reason.clone())),
        PlayState::Error { .. } | PlayState::Unknown => Some((
            PurchaseResult::Failed,
            "Couldn't check your existing purchases with Google Play, so nothing was charged. Try again.".into(),
        )),
    }
}

/// The product `billing_purchase` may sell: only the standard Pro product.
/// The retired support tiers still unlock Pro when owned but are no longer
/// sold, and a tip is never bought through the Pro flow.
pub fn pro_purchase_product(product_id: Option<&str>) -> Result<&'static str, String> {
    match product_id {
        None => Ok(PRO_PRODUCT_ID),
        Some(id) if id == PRO_PRODUCT_ID => Ok(PRO_PRODUCT_ID),
        Some(id) => Err(format!("Unknown product: {id}")),
    }
}

/// `billing_purchase` — open Play's purchase sheet for Pro and verify the
/// result. Pro is granted only from a verified PURCHASED purchase.
#[tauri::command]
pub async fn billing_purchase(
    app: AppHandle,
    product_id: Option<String>,
) -> Result<PurchaseOutcome, String> {
    let product = pro_purchase_product(product_id.as_deref())?;
    let core = app.state::<BillingCore>();
    let reply = |outcome, message: &str, core: &BillingCore| PurchaseOutcome {
        outcome,
        message: message.to_string(),
        status: core.status(),
    };
    if !core.available() {
        let reason = match core.play() {
            PlayState::Unavailable { reason } => reason,
            _ => NO_KEY_REASON.to_string(),
        };
        return Ok(reply(PurchaseResult::Unavailable, &reason, &core));
    }
    let _op = core.op.lock().await;
    // Play refuses a second copy of the same product, but not a different Pro
    // product, so ownership is confirmed with Play before every purchase.
    refresh_locked(&app, &core).await;
    if let Some((outcome, message)) = purchase_gate(&core.play()) {
        return Ok(reply(outcome, &message, &core));
    }
    let response = match plugin_call(&app, PURCHASE_TIMEOUT, |b| b.launch_purchase(product)).await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "purchase flow failed");
            return Ok(reply(PurchaseResult::Failed, UNREACHABLE, &core));
        }
    };
    match response.response_code {
        response_code::OK => {
            match core.apply_flow(&response.purchases, now_ms()) {
                Some(applied) => {
                    finish(&app, &core, &applied.cache);
                    acknowledge(&app, &core, applied.acknowledge).await;
                }
                None => refresh_locked(&app, &core).await,
            }
            Ok(match core.play() {
                PlayState::Owned { .. } => reply(
                    PurchaseResult::Purchased,
                    "Pro unlocked. Thank you!",
                    &core,
                ),
                PlayState::Pending => reply(
                    PurchaseResult::Pending,
                    "Your purchase is pending. Pro unlocks as soon as Google Play confirms the payment.",
                    &core,
                ),
                PlayState::Error { message } => reply(PurchaseResult::Failed, &message, &core),
                _ => reply(
                    PurchaseResult::Failed,
                    "Google Play didn't confirm the purchase. If you were charged, tap Restore purchases.",
                    &core,
                ),
            })
        }
        response_code::USER_CANCELED => Ok(reply(PurchaseResult::Canceled, "", &core)),
        response_code::ITEM_ALREADY_OWNED => {
            refresh_locked(&app, &core).await;
            Ok(if core.play().grants_pro() {
                reply(PurchaseResult::Restored, OWNED_RESTORED, &core)
            } else {
                let message = match core.play() {
                    PlayState::Error { message } => message,
                    _ => OWNED_NOT_SENT.into(),
                };
                reply(PurchaseResult::Failed, &message, &core)
            })
        }
        code => Ok(reply(
            PurchaseResult::Failed,
            &response_message(code, &response.debug_message),
            &core,
        )),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TipResult {
    Thanked,
    Pending,
    Canceled,
    Unavailable,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
pub struct TipOutcome {
    pub outcome: TipResult,
    pub message: String,
    pub status: BillingStatus,
}

/// The tip product `billing_tip` may sell: one of [`TIP_PRODUCT_IDS`], never
/// a Pro product.
pub fn tip_product(product_id: &str) -> Result<&'static str, String> {
    TIP_PRODUCT_IDS
        .into_iter()
        .find(|id| *id == product_id)
        .ok_or_else(|| format!("Unknown tip: {product_id}"))
}

/// Whether a tip may start. Tips are for Play builds with working billing,
/// and only once Pro is unlocked, so a tip can never be mistaken for buying Pro.
pub fn tip_gate(core: &BillingCore) -> Option<String> {
    if !core.available() {
        return Some(match core.play() {
            PlayState::Unavailable { reason } => reason,
            _ => NO_KEY_REASON.to_string(),
        });
    }
    if !core.tips_enabled() {
        return Some(TIPS_PLAY_ONLY.into());
    }
    if core.effective() != Entitlement::Pro {
        return Some(TIPS_NEED_PRO.into());
    }
    None
}

/// What a finished tip flow tells the user. `consumed` is whether Play
/// confirmed consuming every verified tip in the flow.
pub fn tip_flow_result(tips: &TipsReconciled, consumed: bool) -> Option<(TipResult, &'static str)> {
    if !tips.consume.is_empty() {
        return Some(if consumed {
            (TipResult::Thanked, TIP_THANKS)
        } else {
            (
                TipResult::Thanked,
                "Thank you for the tip! Google Play is still finishing it. This retries automatically.",
            )
        });
    }
    if tips.pending {
        return Some((
            TipResult::Pending,
            "Your tip is pending. Google Play finishes it once the payment clears.",
        ));
    }
    None
}

/// `billing_tip` — open Play's purchase sheet for one tip, verify the result
/// and consume it. A tip unlocks nothing and never changes Pro.
#[tauri::command]
pub async fn billing_tip(app: AppHandle, product_id: String) -> Result<TipOutcome, String> {
    let product = tip_product(&product_id)?;
    let core = app.state::<BillingCore>();
    let reply = |outcome, message: &str, core: &BillingCore| TipOutcome {
        outcome,
        message: message.to_string(),
        status: core.status(),
    };
    if let Some(reason) = tip_gate(&core) {
        return Ok(reply(TipResult::Unavailable, &reason, &core));
    }
    let _op = match core.lock_for_tip().await {
        Ok(op) => op,
        Err(reason) => return Ok(reply(TipResult::Unavailable, &reason, &core)),
    };
    let response = match plugin_call(&app, PURCHASE_TIMEOUT, |b| b.launch_purchase(product)).await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "tip flow failed");
            return Ok(reply(TipResult::Failed, UNREACHABLE, &core));
        }
    };
    match response.response_code {
        response_code::OK => {
            let flow = core.apply_tip_flow(&response.purchases, product);
            let consumed = consume_tips(&app, flow.requested.consume.clone()).await;
            consume_tips(&app, flow.other_tips).await;
            if flow.other_purchases {
                // Not this tip's business: the frontend answers with the full
                // owned-purchase refresh, as for any update outside a flow.
                let _ = app.emit(PURCHASES_UPDATED_EVENT, ());
            }
            if let Some((outcome, message)) = tip_flow_result(&flow.requested, consumed) {
                return Ok(reply(outcome, message, &core));
            }
            // The flow didn't hand back the tip; the owned list finishes any
            // verified one, without touching Pro.
            tip_recovery_locked(&app, &core).await;
            Ok(reply(
                TipResult::Failed,
                "Google Play didn't confirm the tip. If you were charged, the app finishes it automatically.",
                &core,
            ))
        }
        response_code::USER_CANCELED => Ok(reply(TipResult::Canceled, "", &core)),
        response_code::ITEM_ALREADY_OWNED => {
            // An earlier tip of the same amount wasn't consumed yet. The owned
            // list consumes it, after which the same tip can be bought again.
            tip_recovery_locked(&app, &core).await;
            Ok(reply(
                TipResult::Failed,
                "Google Play was still finishing an earlier tip. Try again in a moment.",
                &core,
            ))
        }
        response_code::ITEM_UNAVAILABLE => Ok(reply(
            TipResult::Failed,
            "That tip isn't available right now.",
            &core,
        )),
        code => Ok(reply(
            TipResult::Failed,
            &response_message(code, &response.debug_message),
            &core,
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atv_optimizer_core::commands::test_support::{state_with, MockAdb};
    use atv_optimizer_core::license::Feature;
    use serde::Deserialize;

    const PACKAGE: &str = "com.atvoptimizer.mobile";
    const NOW: i64 = 1_761_900_000_000;

    #[derive(Deserialize)]
    struct Vector {
        original_json: String,
        signature: String,
    }

    #[derive(Deserialize)]
    struct Vectors {
        play_public_key: String,
        purchases: std::collections::HashMap<String, Vector>,
    }

    fn vectors() -> Vectors {
        serde_json::from_str(include_str!("../test-fixtures/play-billing-vectors.json")).unwrap()
    }

    fn raw(name: &str) -> RawPurchase {
        let v = &vectors().purchases[name];
        RawPurchase {
            original_json: v.original_json.clone(),
            signature: v.signature.clone(),
        }
    }

    fn core(distribution: Distribution, license_pro: bool) -> BillingCore {
        BillingCore::new(
            Some(&vectors().play_public_key),
            PACKAGE,
            distribution,
            true,
            None,
            license_pro,
            NOW,
        )
    }

    #[test]
    fn build_without_a_key_is_unavailable_and_free() {
        let c = BillingCore::new(None, PACKAGE, Distribution::Play, true, None, false, NOW);
        let s = c.status();
        assert!(!s.available);
        assert_eq!(s.entitlement, Entitlement::Free);
        assert_eq!(
            s.play,
            PlayState::Unavailable {
                reason: NO_KEY_REASON.into()
            }
        );
        // Nothing Play-shaped can unlock a build that cannot verify.
        let a = c.apply_query(QueryOutcome::Owned(&[raw("acked")]), NOW);
        assert!(a.acknowledge.is_empty());
        assert_eq!(c.effective(), Entitlement::Free);
    }

    #[test]
    fn unusable_key_is_unavailable() {
        let c = BillingCore::new(
            Some("junk"),
            PACKAGE,
            Distribution::Play,
            true,
            None,
            false,
            NOW,
        );
        assert!(!c.available());
    }

    #[test]
    fn host_build_is_unavailable_even_with_a_key() {
        let c = BillingCore::new(
            Some(&vectors().play_public_key),
            PACKAGE,
            Distribution::Play,
            false,
            None,
            false,
            NOW,
        );
        assert!(!c.available());
        assert_eq!(
            c.status().play,
            PlayState::Unavailable {
                reason: NOT_ANDROID_REASON.into()
            }
        );
    }

    #[test]
    fn play_build_hides_license_key_entry() {
        assert!(!core(Distribution::Play, false).status().license_key_entry);
        assert!(
            core(Distribution::Sideload, false)
                .status()
                .license_key_entry
        );
    }

    #[test]
    fn play_build_ignores_a_leftover_offline_license() {
        let c = core(Distribution::Play, true);
        assert_eq!(c.effective(), Entitlement::Free);
        assert!(c.status().sources.is_empty());
        c.set_license_pro(true);
        assert_eq!(c.effective(), Entitlement::Free);
        assert_eq!(
            core(Distribution::Sideload, true).effective(),
            Entitlement::Pro
        );
    }

    #[test]
    fn purchase_then_refund_transitions() {
        let c = core(Distribution::Play, false);
        assert_eq!(c.status().play, PlayState::Unknown);
        assert_eq!(c.effective(), Entitlement::Free);

        let a = c.apply_flow(&[raw("pending")], NOW).unwrap();
        assert!(a.acknowledge.is_empty());
        assert_eq!(c.status().play, PlayState::Pending);
        assert_eq!(c.effective(), Entitlement::Free);

        let a = c.apply_query(QueryOutcome::Owned(&[raw("purchased")]), NOW);
        assert_eq!(a.acknowledge, vec!["tok-purchased-1"]);
        assert!(c.status().ack_pending);
        assert_eq!(c.effective(), Entitlement::Pro);
        assert_eq!(c.status().sources, vec!["play"]);
        c.acknowledged(true);
        assert!(!c.status().ack_pending);
        assert!(matches!(
            c.status().play,
            PlayState::Owned {
                acknowledged: true,
                ..
            }
        ));

        let a = c.apply_query(QueryOutcome::Owned(&[]), NOW + 1);
        assert_eq!(a.cache, CacheAction::Clear);
        assert_eq!(c.status().play, PlayState::NotOwned);
        assert_eq!(c.effective(), Entitlement::Free);
    }

    fn offer(id: &str, price: &str) -> Offer {
        Offer {
            product_id: id.into(),
            formatted_price: price.into(),
            title: String::new(),
            name: String::new(),
            description: String::new(),
            sale: None,
        }
    }

    fn tip_vectors() -> Vectors {
        let all: serde_json::Value =
            serde_json::from_str(include_str!("../test-fixtures/play-billing-vectors.json"))
                .unwrap();
        serde_json::from_value(all["tips"].clone()).unwrap()
    }

    fn tip(name: &str) -> RawPurchase {
        let v = &tip_vectors().purchases[name];
        RawPurchase {
            original_json: v.original_json.clone(),
            signature: v.signature.clone(),
        }
    }

    fn tip_core(distribution: Distribution) -> BillingCore {
        BillingCore::new(
            Some(&tip_vectors().play_public_key),
            PACKAGE,
            distribution,
            true,
            None,
            false,
            NOW,
        )
    }

    fn owned_pro_with_tips(c: &BillingCore) {
        c.apply_query(QueryOutcome::Owned(&[tip("pro_purchased")]), NOW);
        assert_eq!(c.effective(), Entitlement::Pro);
    }

    #[test]
    fn offers_keep_pro_and_ordered_tips_and_drop_the_retired_tiers() {
        let c = tip_core(Distribution::Play);
        assert_eq!(c.missing_offers(), vec![PRO_PRODUCT_ID]);
        c.set_offer(offer(crate::play_billing::SUPPORT_PRODUCT_IDS[0], "$9.99"));
        c.set_offer(offer("someone_elses_item", "$1.00"));
        c.set_offer(offer(PRO_PRODUCT_ID, "$4.99"));
        c.set_offer(offer("atvopt_tip_20", "$20.00"));
        c.set_offer(offer("atvopt_tip_2", "$2.00"));
        let s = c.status();
        assert_eq!(s.offer.unwrap().formatted_price, "$4.99");
        let tips: Vec<_> = s.tip_offers.iter().map(|o| o.product_id.as_str()).collect();
        assert_eq!(tips, vec!["atvopt_tip_2", "atvopt_tip_20"]);
        let json = serde_json::to_value(c.status()).unwrap();
        assert!(json.get("support_offers").is_none());
        assert!(!json.to_string().contains("atvopt_pro_support"));

        owned_pro_with_tips(&c);
        assert_eq!(c.missing_offers(), vec!["atvopt_tip_5", "atvopt_tip_10"]);
    }

    fn play_product(price: &str, micros: i64, sale: Option<SaleInfo>) -> ProductInfo {
        ProductInfo {
            product_id: PRO_PRODUCT_ID.into(),
            name: String::new(),
            title: String::new(),
            description: String::new(),
            formatted_price: price.into(),
            price_amount_micros: micros,
            price_currency_code: "USD".into(),
            sale,
        }
    }

    fn play_sale(full: i64, shown: Option<&str>, percent: Option<u32>) -> SaleInfo {
        SaleInfo {
            full_price_micros: full,
            full_formatted_price: shown.map(Into::into),
            percentage_discount: percent,
            end_time_millis: Some(NOW + 1_000),
        }
    }

    #[test]
    fn a_play_sale_shows_the_full_price_and_discount_play_reported() {
        let o = Offer::from(play_product(
            "$1.00",
            1_000_000,
            Some(play_sale(4_990_000, Some("$4.99"), Some(80))),
        ));
        assert_eq!(o.formatted_price, "$1.00");
        assert_eq!(
            o.sale,
            Some(Sale {
                full_formatted_price: Some("$4.99".into()),
                percent_off: Some(80),
                ends_at_ms: Some(NOW + 1_000),
            })
        );
        let json = serde_json::to_value(&o).unwrap();
        assert_eq!(json["sale"]["full_formatted_price"], "$4.99");
        assert_eq!(json["sale"]["percent_off"], 80);
    }

    #[test]
    fn anything_that_isnt_a_real_discount_shows_no_sale() {
        assert_eq!(
            Offer::from(play_product("$4.99", 4_990_000, None)).sale,
            None
        );
        // Full price not above the price charged.
        let same = play_sale(4_990_000, Some("$4.99"), Some(80));
        assert_eq!(
            Offer::from(play_product("$4.99", 4_990_000, Some(same))).sale,
            None
        );
        // Nothing Play gave us to show.
        let bare = play_sale(4_990_000, None, None);
        assert_eq!(
            Offer::from(play_product("$1.00", 1_000_000, Some(bare))).sale,
            None
        );
        // A nonsense percentage is dropped, the full price still shows.
        let odd = play_sale(4_990_000, Some("$4.99"), Some(100));
        let sale = Offer::from(play_product("$1.00", 1_000_000, Some(odd)))
            .sale
            .unwrap();
        assert_eq!(sale.percent_off, None);
        assert_eq!(sale.full_formatted_price.as_deref(), Some("$4.99"));
    }

    #[test]
    fn tip_prices_are_asked_for_only_while_pro_is_unlocked_in_a_play_build() {
        let c = tip_core(Distribution::Play);
        assert_eq!(c.offers_to_query(NOW), vec![PRO_PRODUCT_ID]);
        c.set_offer(offer(PRO_PRODUCT_ID, "$4.99"));
        // Until Pro is unlocked its price is asked for again, for sales.
        assert_eq!(c.offers_to_query(NOW + 1), vec![PRO_PRODUCT_ID]);
        owned_pro_with_tips(&c);
        // Pro's price lookup didn't start the tips' cooldown: newly unlocked
        // Pro asks for tip prices on its very next refresh.
        assert_eq!(c.offers_to_query(NOW + 2), TIP_PRODUCT_IDS.to_vec());
        // Play returned none of them: they are asked for again only sparingly.
        assert!(c.offers_to_query(NOW + 3).is_empty());
        assert_eq!(
            c.offers_to_query(NOW + 2 + OFFER_RETRY_MS),
            TIP_PRODUCT_IDS.to_vec()
        );
        for id in TIP_PRODUCT_IDS {
            c.set_offer(offer(id, "$1"));
        }
        assert!(c.offers_to_query(NOW + 10 * OFFER_RETRY_MS).is_empty());
        assert_eq!(c.status().tip_offers.len(), 4);

        // A missing Pro price never waits on the tips' cooldown.
        let fresh = tip_core(Distribution::Play);
        owned_pro_with_tips(&fresh);
        let mut first = vec![PRO_PRODUCT_ID];
        first.extend(TIP_PRODUCT_IDS);
        assert_eq!(fresh.offers_to_query(NOW), first);
        assert_eq!(fresh.offers_to_query(NOW + 1), vec![PRO_PRODUCT_ID]);

        let sideload = BillingCore::new(
            Some(&tip_vectors().play_public_key),
            PACKAGE,
            Distribution::Sideload,
            true,
            None,
            false,
            NOW,
        );
        owned_pro_with_tips(&sideload);
        assert_eq!(sideload.missing_offers(), vec![PRO_PRODUCT_ID]);
        sideload.set_offer(offer("atvopt_tip_5", "$5.00"));
        let s = sideload.status();
        assert!(!s.tips_available);
        assert!(s.tip_offers.is_empty(), "sideload builds never show tips");
    }

    #[test]
    fn a_tip_never_grants_pro_or_counts_as_owning_it() {
        let c = tip_core(Distribution::Play);
        let a = c.apply_query(
            QueryOutcome::Owned(&[tip("tip_purchased"), tip("tip_listed")]),
            NOW,
        );
        assert_eq!(c.status().play, PlayState::NotOwned);
        assert_eq!(c.effective(), Entitlement::Free);
        assert!(a.acknowledge.is_empty());
        assert_eq!(a.cache, CacheAction::Clear, "a tip is never cached as Pro");
        let tokens: Vec<_> = a.consume.iter().map(ConsumableTip::token).collect();
        assert_eq!(tokens, vec!["tok-tip-5", "tok-tip-20"]);
        // Buy Pro is still offered: the gate sees no Pro purchase.
        assert_eq!(purchase_gate(&c.play()), None);

        let flow = c.apply_tip_flow(&[tip("tip_purchased")], "atvopt_tip_5");
        assert_eq!(flow.requested.consume.len(), 1);
        assert!(c.apply_flow(&[tip("tip_purchased")], NOW).is_none());
        assert_eq!(c.effective(), Entitlement::Free);
        assert_eq!(purchase_gate(&c.play()), None);
    }

    #[test]
    fn a_pending_tip_is_shown_pending_and_never_consumed() {
        let c = tip_core(Distribution::Play);
        owned_pro_with_tips(&c);
        let flow = c.apply_tip_flow(&[tip("tip_pending")], "atvopt_tip_2");
        assert!(flow.requested.consume.is_empty());
        assert!(c.status().tip_pending);
        assert_eq!(
            tip_flow_result(&flow.requested, true).unwrap().0,
            TipResult::Pending
        );
        let a = c.apply_query(
            QueryOutcome::Owned(&[tip("pro_purchased"), tip("tip_pending")]),
            NOW + 1,
        );
        assert!(a.consume.is_empty());
        assert!(c.status().tip_pending);
        // Once Play no longer lists it pending, the flag clears.
        c.apply_query(QueryOutcome::Owned(&[tip("pro_purchased")]), NOW + 2);
        assert!(!c.status().tip_pending);
        assert_eq!(c.effective(), Entitlement::Pro);
    }

    #[test]
    fn pro_purchases_are_never_consumed_and_tips_leave_pro_alone() {
        let c = tip_core(Distribution::Play);
        let a = c.apply_query(
            QueryOutcome::Owned(&[
                tip("pro_and_tip"),
                tip("support_1_unacked"),
                tip("pro_purchased"),
                tip("tip_purchased"),
            ]),
            NOW,
        );
        let tokens: Vec<_> = a.consume.iter().map(ConsumableTip::token).collect();
        assert_eq!(tokens, vec!["tok-tip-5"], "only the tip is consumed");
        assert_eq!(
            a.acknowledge,
            vec!["tok-pro-and-tip", "tok-support-with-tips"]
        );
        assert_eq!(c.effective(), Entitlement::Pro);
        for name in ["pro_and_tip", "support_1_unacked", "pro_purchased"] {
            for id in TIP_PRODUCT_IDS {
                let flow = c.apply_tip_flow(&[tip(name)], id);
                assert!(flow.requested.consume.is_empty() && flow.other_tips.is_empty());
            }
        }
        assert_eq!(c.effective(), Entitlement::Pro);
    }

    #[test]
    fn a_failed_query_keeps_pro_and_consumes_nothing() {
        let c = tip_core(Distribution::Play);
        owned_pro_with_tips(&c);
        let a = c.apply_query(QueryOutcome::Failed("offline".into()), NOW + 1000);
        assert!(a.consume.is_empty());
        assert_eq!(c.effective(), Entitlement::Pro);
    }

    #[test]
    fn sideload_builds_neither_sell_nor_consume_tips() {
        let c = BillingCore::new(
            Some(&tip_vectors().play_public_key),
            PACKAGE,
            Distribution::Sideload,
            true,
            None,
            false,
            NOW,
        );
        let a = c.apply_query(QueryOutcome::Owned(&[tip("tip_purchased")]), NOW);
        assert!(a.consume.is_empty());
        let flow = c.apply_tip_flow(&[tip("tip_purchased")], "atvopt_tip_5");
        assert!(flow.requested.consume.is_empty() && flow.other_tips.is_empty());
        owned_pro_with_tips(&c);
        assert_eq!(tip_gate(&c).as_deref(), Some(TIPS_PLAY_ONLY));
    }

    #[test]
    fn tips_start_only_for_pro_owners_in_play_builds() {
        let c = tip_core(Distribution::Play);
        assert_eq!(tip_gate(&c).as_deref(), Some(TIPS_NEED_PRO));
        owned_pro_with_tips(&c);
        assert_eq!(tip_gate(&c), None);
        let host = BillingCore::new(
            Some(&tip_vectors().play_public_key),
            PACKAGE,
            Distribution::Play,
            false,
            None,
            false,
            NOW,
        );
        assert_eq!(tip_gate(&host).as_deref(), Some(NOT_ANDROID_REASON));
    }

    #[test]
    fn product_ids_are_kept_to_their_own_flow() {
        assert_eq!(pro_purchase_product(None), Ok(PRO_PRODUCT_ID));
        assert_eq!(
            pro_purchase_product(Some(PRO_PRODUCT_ID)),
            Ok(PRO_PRODUCT_ID)
        );
        for retired in crate::play_billing::SUPPORT_PRODUCT_IDS {
            assert!(
                pro_purchase_product(Some(retired)).is_err(),
                "{retired} is no longer sold"
            );
        }
        for id in TIP_PRODUCT_IDS {
            assert!(pro_purchase_product(Some(id)).is_err());
            assert_eq!(tip_product(id), Ok(id));
        }
        assert!(tip_product(PRO_PRODUCT_ID).is_err());
        assert!(tip_product("atvopt_pro_support_1").is_err());
        assert!(tip_product("atvopt_tip_3").is_err());
    }

    #[test]
    fn a_tip_says_thanks_once_play_finishes_it() {
        let c = tip_core(Distribution::Play);
        let tips = c
            .apply_tip_flow(&[tip("tip_purchased")], "atvopt_tip_5")
            .requested;
        assert_eq!(
            tip_flow_result(&tips, true),
            Some((TipResult::Thanked, TIP_THANKS))
        );
        let (outcome, message) = tip_flow_result(&tips, false).unwrap();
        assert_eq!(outcome, TipResult::Thanked);
        assert!(message.contains("retries automatically"));
        assert_eq!(tip_flow_result(&TipsReconciled::default(), true), None);
    }

    fn consumable(name: &str) -> ConsumableTip {
        let c = tip_core(Distribution::Play);
        let mut tips = c.apply_tip_query(QueryOutcome::Owned(&[tip(name)]));
        assert_eq!(tips.len(), 1, "{name} is a consumable tip");
        tips.remove(0)
    }

    #[test]
    fn tip_recovery_query_never_touches_pro() {
        // Cached Pro, past offline grace for a failed query: the full refresh
        // would drop it, the tip-only query must not.
        let r = tip("pro_purchased");
        let cache = CachedPurchase {
            original_json: r.original_json,
            signature: r.signature,
            verified_at_ms: NOW,
        };
        let c = BillingCore::new(
            Some(&tip_vectors().play_public_key),
            PACKAGE,
            Distribution::Play,
            true,
            Some(cache),
            false,
            NOW,
        );
        assert_eq!(c.effective(), Entitlement::Pro);
        let before = c.status();
        let consume = c.apply_tip_query(QueryOutcome::Failed("offline".into()));
        assert!(consume.is_empty());
        // A complete list with no Pro in it still leaves Pro and its cache alone.
        let consume = c.apply_tip_query(QueryOutcome::Owned(&[
            tip("tip_purchased"),
            tip("tip_pending"),
        ]));
        let tokens: Vec<_> = consume.iter().map(ConsumableTip::token).collect();
        assert_eq!(tokens, vec!["tok-tip-5"]);
        let after = c.status();
        assert_eq!(after.play, before.play);
        assert_eq!(after.entitlement, Entitlement::Pro);
        assert!(after.tip_pending);
        assert!(c.lock().cache.is_some(), "cached Pro purchase kept");
        assert!(
            c.should_auto_refresh(NOW + 1),
            "Pro refresh clock untouched"
        );
        // Sideload builds never consume tips, even through this path.
        let sideload = BillingCore::new(
            Some(&tip_vectors().play_public_key),
            PACKAGE,
            Distribution::Sideload,
            true,
            None,
            false,
            NOW,
        );
        assert!(sideload
            .apply_tip_query(QueryOutcome::Owned(&[tip("tip_purchased")]))
            .is_empty());
    }

    #[test]
    fn a_tip_flow_reports_only_the_tip_that_was_asked_for() {
        let c = tip_core(Distribution::Play);
        owned_pro_with_tips(&c);
        // The $5 sheet is open when an earlier $2 tip's payment clears.
        let flow = c.apply_tip_flow(&[tip("tip_listed")], "atvopt_tip_5");
        assert_eq!(flow.requested, TipsReconciled::default());
        assert_eq!(tip_flow_result(&flow.requested, true), None);
        let others: Vec<_> = flow.other_tips.iter().map(ConsumableTip::token).collect();
        assert_eq!(
            others,
            vec!["tok-tip-20"],
            "the other tip is still consumed"
        );
        assert!(!flow.other_purchases);
        // A Pro purchase arriving in a tip flow is handed to the full refresh.
        let flow = c.apply_tip_flow(
            &[tip("tip_purchased"), tip("pro_purchased")],
            "atvopt_tip_5",
        );
        assert_eq!(flow.requested.consume.len(), 1);
        assert!(flow.other_tips.is_empty());
        assert!(flow.other_purchases);
        // Another tip still pending shows as pending, but isn't this flow's result.
        let flow = c.apply_tip_flow(&[tip("tip_pending")], "atvopt_tip_5");
        assert_eq!(tip_flow_result(&flow.requested, true), None);
        assert!(c.status().tip_pending);
        assert_eq!(c.effective(), Entitlement::Pro);
    }

    #[test]
    fn a_failed_tip_consume_is_retried_on_a_bounded_backoff() {
        let c = tip_core(Distribution::Play);
        let t = consumable("tip_purchased");
        assert!(c.begin_consume(&t));
        assert!(
            !c.begin_consume(&t),
            "one native consume per token at a time"
        );
        assert!(c.due_tip_retries(NOW).is_empty());
        assert!(c.consume_finished(&t, false, NOW));
        assert_eq!(c.next_tip_retry_in(NOW), Some(TIP_RETRY_BASE_MS));
        assert!(c.due_tip_retries(NOW + TIP_RETRY_BASE_MS - 1).is_empty());
        assert_eq!(c.due_tip_retries(NOW + TIP_RETRY_BASE_MS), vec![t.clone()]);

        // While a retry's call is in flight it is neither due nor scheduled.
        assert!(c.begin_consume(&t));
        assert!(c.due_tip_retries(NOW + TIP_RETRY_MAX_MS).is_empty());
        assert_eq!(c.next_tip_retry_in(NOW), None);
        assert!(c.claim_retry_task(), "None ended the retry task");
        assert!(!c.claim_retry_task(), "only one retry task runs");

        // Backoff doubles up to the cap, then the scheduled tries stop.
        let mut now = NOW;
        let mut delays = vec![TIP_RETRY_BASE_MS];
        for _ in 2..=TIP_RETRY_ATTEMPTS {
            now += 1;
            assert!(c.consume_finished(&t, false, now));
            delays.push(c.lock().tip_retries[0].next_ms.unwrap() - now);
            assert!(c.begin_consume(&t));
        }
        assert_eq!(
            delays,
            vec![30_000, 60_000, 120_000, 240_000, 480_000, 960_000, 1_800_000, 1_800_000]
        );
        assert!(
            !c.consume_finished(&t, false, now + 1),
            "attempts are capped"
        );
        assert_eq!(c.next_tip_retry_in(now + 1), None);
        assert!(c.due_tip_retries(now + 10 * TIP_RETRY_MAX_MS).is_empty());
        // Still tracked, so a later query's success or absence settles it.
        assert_eq!(c.lock().tip_retries.len(), 1);
        assert!(c.begin_consume(&t));
        assert!(!c.consume_finished(&t, true, now + 2));
        assert!(c.lock().tip_retries.is_empty());
        assert!(c.lock().consuming.is_empty());
    }

    #[test]
    fn an_owned_list_without_a_retried_tip_stops_retrying_it() {
        let c = tip_core(Distribution::Play);
        let five = consumable("tip_purchased");
        let twenty = consumable("tip_listed");
        for t in [&five, &twenty] {
            assert!(c.begin_consume(t));
            assert!(c.consume_finished(t, false, NOW));
        }
        // Play still lists the $20 tip unconsumed; the $5 one is gone.
        let a = c.apply_query(
            QueryOutcome::Owned(&[tip("pro_purchased"), tip("tip_listed")]),
            NOW + 1,
        );
        assert_eq!(a.consume, vec![twenty.clone()]);
        assert_eq!(
            c.due_tip_retries(NOW + TIP_RETRY_BASE_MS),
            vec![twenty.clone()]
        );
        // A failed query settles nothing.
        c.apply_query(QueryOutcome::Failed("offline".into()), NOW + 2);
        assert_eq!(c.lock().tip_retries.len(), 1);
        c.apply_tip_query(QueryOutcome::Owned(&[tip("pro_purchased")]));
        assert!(c.lock().tip_retries.is_empty());
        assert_eq!(c.effective(), Entitlement::Pro);
    }

    #[tokio::test]
    async fn a_queued_tip_rechecks_the_pro_gate_once_it_holds_the_lock() {
        let c = std::sync::Arc::new(tip_core(Distribution::Play));
        owned_pro_with_tips(&c);
        assert_eq!(tip_gate(&c), None, "the early check passes");
        let refresh = c.op.lock().await;
        let queued = {
            let c = c.clone();
            tokio::spawn(async move { c.lock_for_tip().await.map(|_| ()) })
        };
        tokio::task::yield_now().await;
        // The refresh holding the lock finds Pro refunded.
        c.apply_query(QueryOutcome::Owned(&[]), NOW + 1);
        drop(refresh);
        assert_eq!(queued.await.unwrap(), Err(TIPS_NEED_PRO.to_string()));
        // With Pro still owned the lock is handed over.
        owned_pro_with_tips(&c);
        assert!(c.lock_for_tip().await.is_ok());
    }

    #[test]
    fn a_purchase_starts_only_when_play_confirms_pro_is_not_owned() {
        assert_eq!(purchase_gate(&PlayState::NotOwned), None);
        let owned = PlayState::Owned {
            acknowledged: true,
            verified_at_ms: NOW,
            cached: true,
        };
        let (outcome, message) = purchase_gate(&owned).unwrap();
        assert_eq!(outcome, PurchaseResult::AlreadyOwned);
        // An owner who taps Buy hears that Pro is restored, not "you already own".
        assert_eq!(message, OWNED_RESTORED);
        assert!(!message.to_lowercase().contains("already own"));
        assert_eq!(
            purchase_gate(&PlayState::Pending).unwrap().0,
            PurchaseResult::Pending
        );
        for unsure in [
            PlayState::Unknown,
            PlayState::Error {
                message: "offline".into(),
            },
        ] {
            assert_eq!(purchase_gate(&unsure).unwrap().0, PurchaseResult::Failed);
        }
    }

    #[test]
    fn query_error_never_unlocks_a_fresh_install() {
        let c = core(Distribution::Play, false);
        c.apply_query(QueryOutcome::Failed("offline".into()), NOW);
        assert_eq!(c.effective(), Entitlement::Free);
        assert!(matches!(c.status().play, PlayState::Error { .. }));
    }

    #[test]
    fn query_error_after_purchase_keeps_pro_from_the_verified_cache() {
        let c = core(Distribution::Play, false);
        c.apply_query(QueryOutcome::Owned(&[raw("acked")]), NOW);
        c.apply_query(QueryOutcome::Failed("offline".into()), NOW + 1000);
        assert!(matches!(
            c.status().play,
            PlayState::Owned { cached: true, .. }
        ));
        assert_eq!(c.effective(), Entitlement::Pro);
    }

    #[test]
    fn startup_restores_a_recent_cached_purchase_and_rejects_an_old_one() {
        let r = raw("acked");
        let cache = CachedPurchase {
            original_json: r.original_json,
            signature: r.signature,
            verified_at_ms: NOW,
        };
        let key = vectors().play_public_key;
        let fresh = BillingCore::new(
            Some(&key),
            PACKAGE,
            Distribution::Play,
            true,
            Some(cache.clone()),
            false,
            NOW + 1000,
        );
        assert_eq!(fresh.effective(), Entitlement::Pro);
        let stale = BillingCore::new(
            Some(&key),
            PACKAGE,
            Distribution::Play,
            true,
            Some(cache),
            false,
            NOW + crate::play_billing::OFFLINE_GRACE_MS + 1,
        );
        assert_eq!(stale.effective(), Entitlement::Free);
    }

    #[test]
    fn license_and_play_sources_stay_independent() {
        let c = core(Distribution::Sideload, true);
        c.apply_query(QueryOutcome::Owned(&[raw("acked")]), NOW);
        assert_eq!(c.status().sources, vec!["play", "license"]);
        c.apply_query(QueryOutcome::Owned(&[]), NOW + 1);
        assert_eq!(c.status().sources, vec!["license"]);
        assert_eq!(c.effective(), Entitlement::Pro);
        c.set_license_pro(false);
        assert_eq!(c.effective(), Entitlement::Free);
    }

    #[test]
    fn auto_refresh_is_rate_limited_but_never_stuck() {
        let c = core(Distribution::Play, false);
        assert!(c.should_auto_refresh(NOW));
        c.apply_query(QueryOutcome::Owned(&[]), NOW);
        assert!(!c.should_auto_refresh(NOW + 1000));
        assert!(c.should_auto_refresh(NOW + AUTO_REFRESH_MIN_INTERVAL_MS));
        assert!(c.should_auto_refresh(NOW - 1));
    }

    #[test]
    fn cache_file_round_trips_and_clears() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read_cache(dir.path()), None);
        let r = raw("acked");
        let cache = CachedPurchase {
            original_json: r.original_json,
            signature: r.signature,
            verified_at_ms: NOW,
        };
        persist_cache(dir.path(), &CacheAction::Store(cache.clone()));
        assert_eq!(read_cache(dir.path()), Some(cache));
        persist_cache(dir.path(), &CacheAction::Clear);
        assert_eq!(read_cache(dir.path()), None);
        persist_cache(dir.path(), &CacheAction::Clear);
        std::fs::write(cache_path(dir.path()), "{not json").unwrap();
        assert_eq!(read_cache(dir.path()), None);
    }

    #[tokio::test]
    async fn refund_downgrades_pro_gates_but_recovery_stays_free() {
        let mock = MockAdb::default()
            .on_shell("pm list packages -d", "package:com.example.one\n")
            // Recovery confirms each enable by reading it back.
            .on_shell(
                "pm list packages -e --user 0 com.example.one",
                &format!(
                    "package:com.example.one\n{}0\n",
                    atv_optimizer_core::adb::BATCH_STATUS
                ),
            );
        let log = mock.shell_log();
        let state = state_with(mock);
        let c = core(Distribution::Play, false);

        c.apply_query(QueryOutcome::Owned(&[raw("acked")]), NOW);
        sync_entitlement(&state, &c);
        assert!(state.require_pro(Feature::TweaksWrite).is_ok());

        c.apply_query(QueryOutcome::Owned(&[]), NOW + 1);
        sync_entitlement(&state, &c);
        assert_eq!(state.entitlement(), Entitlement::Free);
        assert_eq!(
            state.require_pro(Feature::TweaksWrite),
            Err("LOCKED:tweaks_write".to_string())
        );

        let recovered = atv_optimizer_core::commands::recovery::panic_recovery_impl(&state, "TV")
            .await
            .expect("emergency recovery stays free");
        assert_eq!(recovered.restored, vec!["com.example.one"]);
        let enabled = atv_optimizer_core::commands::apps::enable_package_impl(
            &state,
            "TV",
            "com.example.two",
        )
        .await
        .expect("re-enabling an app stays free");
        assert!(!enabled.message.starts_with("LOCKED:"));
        let calls = log.lock().unwrap();
        assert!(calls
            .iter()
            .any(|c| c == "pm enable --user 0 com.example.one"));
        assert!(calls.iter().any(|c| c == "pm enable com.example.two"));
    }
}

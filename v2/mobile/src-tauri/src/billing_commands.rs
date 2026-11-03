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
//! visible again (`billing_refresh` from the frontend) and after every purchase
//! flow. It is what restores Pro on a new install and what notices refunds and
//! revocations.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use shield_optimizer_core::commands::AppState;
use shield_optimizer_core::license::Entitlement;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_atv_billing::{response_code, BillingExt, ProductInfo, RawPurchase};

use crate::play_billing::{
    cached_owned_state, effective_entitlement, reconcile, reconcile_flow, response_message,
    CacheAction, CachedPurchase, Distribution, PlayState, PlayVerifier, QueryOutcome,
    PLAY_LICENSE_KEY, PRO_PRODUCT_ID,
};

/// Automatic refreshes (launch, resume) closer together than this reuse the
/// last answer. Restore purchases and purchase flows always ask Play.
const AUTO_REFRESH_MIN_INTERVAL_MS: i64 = 30_000;
const QUERY_TIMEOUT: Duration = Duration::from_secs(30);
/// The purchase sheet stays open while the user decides; this only bounds an
/// abandoned flow (for instance an activity destroyed mid-sheet).
const PURCHASE_TIMEOUT: Duration = Duration::from_secs(15 * 60);

const NO_KEY_REASON: &str = "Purchases aren't available in this build of the app.";
const NOT_ANDROID_REASON: &str = "Google Play purchases are only available in the Android app.";
const UNREACHABLE: &str = "Couldn't reach Google Play. Check your connection and try again.";

/// Play's localized offer for Pro, for display only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Offer {
    pub formatted_price: String,
    pub title: String,
    pub name: String,
    pub description: String,
}

impl From<ProductInfo> for Offer {
    fn from(p: ProductInfo) -> Self {
        Offer {
            formatted_price: p.formatted_price,
            title: p.title,
            name: p.name,
            description: p.description,
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
    pub offer: Option<Offer>,
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
    offer: Option<Offer>,
    last_query_ms: Option<i64>,
    ack_pending: bool,
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
    pub acknowledge: Option<String>,
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
                offer: None,
                last_query_ms: None,
                ack_pending: false,
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

    pub fn has_offer(&self) -> bool {
        self.lock().offer.is_some()
    }

    pub fn set_offer(&self, offer: Option<Offer>) {
        self.lock().offer = offer;
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
                acknowledge: None,
            };
        };
        let succeeded = matches!(outcome, QueryOutcome::Owned(_));
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
            inner.ack_pending = r.acknowledge.is_some();
        }
        Self::apply_cache(&mut inner, &r.cache);
        Applied {
            cache: r.cache,
            acknowledge: r.acknowledge,
        }
    }

    /// Apply a purchase-flow result. `None` means it said nothing about Pro and
    /// a full owned-purchase query should decide.
    pub fn apply_flow(&self, purchases: &[RawPurchase], now_ms: i64) -> Option<Applied> {
        let verifier = self.verifier.as_ref()?;
        let r = reconcile_flow(verifier, purchases, now_ms)?;
        let mut inner = self.lock();
        inner.play = r.state;
        inner.ack_pending = r.acknowledge.is_some();
        Self::apply_cache(&mut inner, &r.cache);
        Some(Applied {
            cache: r.cache,
            acknowledge: r.acknowledge,
        })
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
            offer: inner.offer.clone(),
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
            .and_then(|json| std::fs::write(&path, json).map_err(|e| e.to_string())),
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

async fn acknowledge(app: &AppHandle, core: &BillingCore, token: String) {
    let result = plugin_call(app, QUERY_TIMEOUT, move |b| b.acknowledge(&token)).await;
    let ok = matches!(&result, Ok(r) if r.response_code == response_code::OK);
    if !ok {
        tracing::warn!(?result, "acknowledgment failed; will retry on next refresh");
    }
    core.acknowledged(ok);
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
    if let Some(token) = applied.acknowledge {
        acknowledge(app, core, token).await;
    }
    if !core.has_offer() {
        match plugin_call(app, QUERY_TIMEOUT, |b| b.query_product(PRO_PRODUCT_ID)).await {
            Ok(r) if r.response_code == response_code::OK => {
                core.set_offer(r.product.map(Offer::from));
            }
            Ok(r) => tracing::info!(code = r.response_code, "Pro product details unavailable"),
            Err(e) => tracing::info!(error = %e, "Pro product details unavailable"),
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

/// `billing_purchase` — open Play's purchase sheet for Pro and verify the
/// result. Pro is granted only from a verified PURCHASED purchase.
#[tauri::command]
pub async fn billing_purchase(app: AppHandle) -> Result<PurchaseOutcome, String> {
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
    if matches!(core.play(), PlayState::Owned { cached: false, .. }) {
        return Ok(reply(
            PurchaseResult::AlreadyOwned,
            "You already own ATV Optimizer Pro.",
            &core,
        ));
    }
    let response = match plugin_call(&app, PURCHASE_TIMEOUT, |b| {
        b.launch_purchase(PRO_PRODUCT_ID)
    })
    .await
    {
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
                    if let Some(token) = applied.acknowledge {
                        acknowledge(&app, &core, token).await;
                    }
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
                reply(
                    PurchaseResult::Restored,
                    "You already own Pro. It's restored on this phone.",
                    &core,
                )
            } else {
                let message = match core.play() {
                    PlayState::Error { message } => message,
                    _ => "Google Play says you own Pro but didn't return the purchase. Try Restore purchases.".into(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use shield_optimizer_core::commands::test_support::{state_with, MockAdb};
    use shield_optimizer_core::license::Feature;

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
        assert_eq!(a.acknowledge, None);
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
        assert_eq!(a.acknowledge, None);
        assert_eq!(c.status().play, PlayState::Pending);
        assert_eq!(c.effective(), Entitlement::Free);

        let a = c.apply_query(QueryOutcome::Owned(&[raw("purchased")]), NOW);
        assert_eq!(a.acknowledge.as_deref(), Some("tok-purchased-1"));
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
        let mock = MockAdb::default().on_shell("pm list packages -d", "package:com.example.one\n");
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

        let recovered =
            shield_optimizer_core::commands::recovery::panic_recovery_impl(&state, "TV")
                .await
                .expect("emergency recovery stays free");
        assert_eq!(recovered.restored, vec!["com.example.one"]);
        let enabled = shield_optimizer_core::commands::apps::enable_package_impl(
            &state,
            "TV",
            "com.example.two",
        )
        .await
        .expect("re-enabling an app stays free");
        assert!(!enabled.message.starts_with("LOCKED:"));
        let calls = log.lock().unwrap();
        assert!(calls.iter().any(|c| c == "pm enable com.example.one"));
        assert!(calls.iter().any(|c| c == "pm enable com.example.two"));
    }
}

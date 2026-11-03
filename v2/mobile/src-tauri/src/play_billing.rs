//! Google Play purchase verification and the Play entitlement source.
//!
//! Pure logic only: no Tauri, no Play calls, no file I/O. The commands in
//! `billing_commands.rs` feed it what the Kotlin adapter returned and persist
//! what it decides.
//!
//! Trust model (v1, no backend): Play signs every purchase's JSON with the
//! app's RSA key (SHA1withRSA). The public half is compiled in from
//! `ATVOPT_PLAY_LICENSE_KEY`. A purchase counts only if that signature verifies
//! and the signed JSON names this package, the Pro product and the PURCHASED
//! state. A patched APK can bypass on-device checks; that is the accepted
//! trade-off for a one-time unlock at this price, and a server verifier can
//! later sit behind the same source without changing the UX.
//!
//! This source never touches the Ed25519 offline license. The two are kept
//! separate and combined only by [`effective_entitlement`].

use base64::Engine as _;
use rsa::pkcs8::DecodePublicKey;
use rsa::{Pkcs1v15Sign, RsaPublicKey};
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use shield_optimizer_core::license::Entitlement;
use tauri_plugin_atv_billing::{response_code, RawPurchase};

/// The one Play product that unlocks Pro: a non-consumable one-time product.
/// The owner creates it in Play Console with exactly this id.
pub const PRO_PRODUCT_ID: &str = "atvopt_pro";

/// The app's Play licensing public key (Play Console → Monetization setup),
/// base64 DER, supplied at build time. Absent means this build cannot verify
/// purchases, so billing reports itself unavailable and Play never grants Pro.
pub const PLAY_LICENSE_KEY: Option<&str> = option_env!("ATVOPT_PLAY_LICENSE_KEY");

/// How long a previously verified purchase keeps Pro when Google Play cannot be
/// asked (no Play services, no connection). This is also the longest a refund
/// can go unnoticed while Play is unreachable. A successful query always wins:
/// a refunded or revoked purchase drops Pro at the next one.
pub const OFFLINE_GRACE_MS: i64 = 7 * 24 * 60 * 60 * 1000;

/// A device clock this far *behind* the last verification is treated as
/// tampered or broken, and the cached purchase is not honored.
pub const CLOCK_SKEW_MS: i64 = 24 * 60 * 60 * 1000;

/// Where this build is distributed, fixed at compile time by
/// `ATVOPT_DISTRIBUTION`. Play builds hide offline license-key entry
/// (Payments policy: in-app digital unlocks go through Play billing).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Distribution {
    Play,
    Sideload,
}

impl Distribution {
    pub fn from_flag(flag: Option<&str>) -> Self {
        match flag.map(str::trim) {
            Some(value) if value.eq_ignore_ascii_case("play") => Distribution::Play,
            _ => Distribution::Sideload,
        }
    }

    pub fn current() -> Self {
        Self::from_flag(option_env!("ATVOPT_DISTRIBUTION"))
    }

    /// Offline license keys are a sideload/dev unlock only.
    pub fn allows_license_keys(self) -> bool {
        self == Distribution::Sideload
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PurchaseState {
    Purchased,
    Pending,
    /// Any other value. Never unlocks.
    Other(i64),
}

/// A purchase whose signature verified and whose signed JSON names this app
/// and the Pro product.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedPurchase {
    pub purchase_token: String,
    pub state: PurchaseState,
    pub acknowledged: bool,
    pub purchase_time_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifyError {
    SignatureEncoding,
    BadSignature,
    MalformedJson,
    WrongPackage,
    /// Signed and genuine, but for another product. Ignored, not suspicious.
    OtherProduct,
    MissingToken,
}

/// Verifies Play purchase signatures for one package.
#[derive(Clone)]
pub struct PlayVerifier {
    key: RsaPublicKey,
    package_name: String,
}

impl std::fmt::Debug for PlayVerifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlayVerifier")
            .field("package_name", &self.package_name)
            .finish_non_exhaustive()
    }
}

impl PlayVerifier {
    /// Parse Play Console's base64 X.509 SubjectPublicKeyInfo. Whitespace (a key
    /// pasted across lines) is ignored.
    pub fn from_base64(key_b64: &str, package_name: &str) -> Result<Self, String> {
        let compact: String = key_b64.chars().filter(|c| !c.is_whitespace()).collect();
        if compact.is_empty() {
            return Err("Play license key is empty".into());
        }
        let der = base64::engine::general_purpose::STANDARD
            .decode(compact.as_bytes())
            .map_err(|e| format!("Play license key is not valid base64: {e}"))?;
        let key = RsaPublicKey::from_public_key_der(&der)
            .map_err(|e| format!("Play license key is not an RSA public key: {e}"))?;
        Ok(Self {
            key,
            package_name: package_name.to_string(),
        })
    }

    pub fn verify(&self, raw: &RawPurchase) -> Result<VerifiedPurchase, VerifyError> {
        let signature: String = raw
            .signature
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        let signature = base64::engine::general_purpose::STANDARD
            .decode(signature.as_bytes())
            .map_err(|_| VerifyError::SignatureEncoding)?;
        let digest = Sha1::digest(raw.original_json.as_bytes());
        self.key
            .verify(Pkcs1v15Sign::new::<Sha1>(), &digest, &signature)
            .map_err(|_| VerifyError::BadSignature)?;

        // Only now is the JSON trusted, and only the fields read from it here.
        let json: serde_json::Value =
            serde_json::from_str(&raw.original_json).map_err(|_| VerifyError::MalformedJson)?;
        if json.get("packageName").and_then(|v| v.as_str()) != Some(self.package_name.as_str()) {
            return Err(VerifyError::WrongPackage);
        }
        let single = json.get("productId").and_then(|v| v.as_str());
        let listed = json
            .get("productIds")
            .and_then(|v| v.as_array())
            .map(|ids| ids.iter().any(|id| id.as_str() == Some(PRO_PRODUCT_ID)))
            .unwrap_or(false);
        if single != Some(PRO_PRODUCT_ID) && !listed {
            return Err(VerifyError::OtherProduct);
        }
        let purchase_token = json
            .get("purchaseToken")
            .and_then(|v| v.as_str())
            .filter(|t| !t.is_empty())
            .ok_or(VerifyError::MissingToken)?
            .to_string();
        // Play's JSON uses 0 = purchased and 4 = pending. A missing state is
        // treated as unknown rather than guessed.
        let state = match json.get("purchaseState").and_then(|v| v.as_i64()) {
            Some(0) => PurchaseState::Purchased,
            Some(4) => PurchaseState::Pending,
            Some(other) => PurchaseState::Other(other),
            None => PurchaseState::Other(-1),
        };
        Ok(VerifiedPurchase {
            purchase_token,
            state,
            acknowledged: json
                .get("acknowledged")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            purchase_time_ms: json
                .get("purchaseTime")
                .and_then(|v| v.as_i64())
                .unwrap_or(0),
        })
    }
}

/// The last verified Pro purchase, persisted so Pro survives a restart while
/// Play is briefly unreachable. It holds Play's signed blob, so it is
/// re-verified on every read; editing it only ever makes it invalid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CachedPurchase {
    pub original_json: String,
    pub signature: String,
    /// When Play last confirmed this purchase (Unix ms).
    pub verified_at_ms: i64,
}

/// The Play entitlement source, as shown to the user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum PlayState {
    /// This build or device cannot use Play billing.
    Unavailable { reason: String },
    /// Not yet checked with Play this launch, and nothing usable cached.
    Unknown,
    /// Play answered and this account does not own Pro (never bought,
    /// refunded or revoked).
    NotOwned,
    /// Play reports the purchase as pending payment. Not Pro until it completes.
    Pending,
    /// A verified Pro purchase. `cached` means Play could not be asked this
    /// time and a purchase verified within the offline grace window is honored.
    Owned {
        acknowledged: bool,
        verified_at_ms: i64,
        cached: bool,
    },
    /// Play could not be asked and there is no recent verified purchase.
    Error { message: String },
}

impl PlayState {
    pub fn grants_pro(&self) -> bool {
        matches!(self, PlayState::Owned { .. })
    }
}

/// Combine the two independent sources. Neither can revoke the other.
pub fn effective_entitlement(license_pro: bool, play: &PlayState) -> Entitlement {
    if license_pro || play.grants_pro() {
        Entitlement::Pro
    } else {
        Entitlement::Free
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CacheAction {
    Keep,
    Store(CachedPurchase),
    Clear,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reconciled {
    pub state: PlayState,
    pub cache: CacheAction,
    /// A verified, granted purchase Play still needs acknowledged (within three
    /// days, or Play refunds it). Only ever set alongside `Owned`.
    pub acknowledge: Option<String>,
    /// Purchases that claimed to be ours but failed verification.
    pub rejected: usize,
}

/// What Play's owned-purchase query produced.
pub enum QueryOutcome<'a> {
    /// The query succeeded: this is the complete owned list for the account.
    Owned(&'a [RawPurchase]),
    /// The query failed; the message is user-facing.
    Failed(String),
}

/// Honor a cached purchase only if it still verifies, is PURCHASED and was
/// confirmed by Play within the grace window of a sane clock.
pub fn cached_owned_state(
    verifier: &PlayVerifier,
    cache: &CachedPurchase,
    now_ms: i64,
) -> Option<PlayState> {
    if now_ms < cache.verified_at_ms - CLOCK_SKEW_MS
        || now_ms - cache.verified_at_ms > OFFLINE_GRACE_MS
    {
        return None;
    }
    let raw = RawPurchase {
        original_json: cache.original_json.clone(),
        signature: cache.signature.clone(),
    };
    match verifier.verify(&raw) {
        Ok(p) if p.state == PurchaseState::Purchased => Some(PlayState::Owned {
            acknowledged: p.acknowledged,
            verified_at_ms: cache.verified_at_ms,
            cached: true,
        }),
        _ => None,
    }
}

/// Fold one owned-purchase query into the Play source. A failed query never
/// grants anything new; it can only keep a recent verified purchase.
pub fn reconcile(
    verifier: &PlayVerifier,
    previous: Option<&CachedPurchase>,
    outcome: QueryOutcome<'_>,
    now_ms: i64,
) -> Reconciled {
    match outcome {
        QueryOutcome::Owned(purchases) => reconcile_purchases(verifier, purchases, now_ms, true),
        QueryOutcome::Failed(message) => {
            let state = previous
                .and_then(|cache| cached_owned_state(verifier, cache, now_ms))
                .unwrap_or(PlayState::Error { message });
            Reconciled {
                state,
                cache: CacheAction::Keep,
                acknowledge: None,
                rejected: 0,
            }
        }
    }
}

/// Fold purchases from a purchase flow (`onPurchasesUpdated`). Unlike a
/// query, this list is not the complete owned set, so it returns `None` when
/// it says nothing about Pro and the caller should run a full query.
pub fn reconcile_flow(
    verifier: &PlayVerifier,
    purchases: &[RawPurchase],
    now_ms: i64,
) -> Option<Reconciled> {
    let result = reconcile_purchases(verifier, purchases, now_ms, false);
    match result.state {
        PlayState::Owned { .. } | PlayState::Pending | PlayState::Error { .. } => Some(result),
        _ => None,
    }
}

fn reconcile_purchases(
    verifier: &PlayVerifier,
    purchases: &[RawPurchase],
    now_ms: i64,
    complete: bool,
) -> Reconciled {
    let mut rejected = 0;
    let mut owned: Option<(&RawPurchase, VerifiedPurchase)> = None;
    let mut pending = false;
    for raw in purchases {
        match verifier.verify(raw) {
            Ok(p) => match p.state {
                PurchaseState::Purchased => {
                    if owned.is_none() {
                        owned = Some((raw, p));
                    }
                }
                PurchaseState::Pending => pending = true,
                PurchaseState::Other(_) => {}
            },
            Err(VerifyError::OtherProduct) => {}
            Err(_) => rejected += 1,
        }
    }
    if let Some((raw, p)) = owned {
        return Reconciled {
            state: PlayState::Owned {
                acknowledged: p.acknowledged,
                verified_at_ms: now_ms,
                cached: false,
            },
            cache: CacheAction::Store(CachedPurchase {
                original_json: raw.original_json.clone(),
                signature: raw.signature.clone(),
                verified_at_ms: now_ms,
            }),
            acknowledge: (!p.acknowledged).then_some(p.purchase_token),
            rejected,
        };
    }
    let clear = if complete {
        CacheAction::Clear
    } else {
        CacheAction::Keep
    };
    let state = if pending {
        PlayState::Pending
    } else if rejected > 0 {
        PlayState::Error {
            message: "Google Play returned a purchase this app could not verify, so Pro was not unlocked. Contact support if you were charged.".into(),
        }
    } else {
        PlayState::NotOwned
    };
    Reconciled {
        state,
        cache: clear,
        acknowledge: None,
        rejected,
    }
}

/// User-facing text for a Play `BillingResponseCode` that is not success.
pub fn response_message(code: i32, debug_message: &str) -> String {
    let base = match code {
        response_code::USER_CANCELED => "Purchase canceled.",
        response_code::SERVICE_UNAVAILABLE
        | response_code::NETWORK_ERROR
        | response_code::SERVICE_TIMEOUT
        | response_code::SERVICE_DISCONNECTED => {
            "Couldn't reach Google Play. Check your connection and try again."
        }
        response_code::BILLING_UNAVAILABLE => {
            "Google Play purchases aren't available on this phone or account."
        }
        response_code::ITEM_UNAVAILABLE => "ATV Optimizer Pro isn't available to buy right now.",
        response_code::DEVELOPER_ERROR => {
            "This copy of the app can't make purchases. Install it from Google Play."
        }
        response_code::FEATURE_NOT_SUPPORTED => {
            "This version of Google Play doesn't support purchases."
        }
        response_code::ITEM_ALREADY_OWNED => "You already own ATV Optimizer Pro.",
        response_code::ITEM_NOT_OWNED => "Google Play says this account doesn't own Pro.",
        _ => "Google Play reported an error. Try again.",
    };
    tracing::debug!(code, debug_message, "billing response");
    base.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const PACKAGE: &str = "com.atvoptimizer.mobile";
    const DAY_MS: i64 = 24 * 60 * 60 * 1000;
    const NOW: i64 = 1_761_900_000_000;

    #[derive(Deserialize)]
    struct Vector {
        original_json: String,
        signature: String,
    }

    #[derive(Deserialize)]
    struct Vectors {
        play_public_key: String,
        other_public_key: String,
        purchases: std::collections::HashMap<String, Vector>,
        purchased_signed_by_other_key: String,
    }

    fn vectors() -> Vectors {
        serde_json::from_str(include_str!("../test-fixtures/play-billing-vectors.json"))
            .expect("vectors parse")
    }

    fn verifier() -> PlayVerifier {
        PlayVerifier::from_base64(&vectors().play_public_key, PACKAGE).expect("test key")
    }

    fn raw(name: &str) -> RawPurchase {
        let v = &vectors().purchases[name];
        RawPurchase {
            original_json: v.original_json.clone(),
            signature: v.signature.clone(),
        }
    }

    #[test]
    fn genuine_purchase_verifies_and_reads_signed_fields() {
        let p = verifier().verify(&raw("purchased")).expect("verifies");
        assert_eq!(p.state, PurchaseState::Purchased);
        assert_eq!(p.purchase_token, "tok-purchased-1");
        assert!(!p.acknowledged);
        assert_eq!(p.purchase_time_ms, 1_761_900_000_000);
    }

    #[test]
    fn key_with_line_breaks_still_parses() {
        let key = vectors().play_public_key;
        let wrapped: String = key
            .as_bytes()
            .chunks(64)
            .map(|c| std::str::from_utf8(c).unwrap())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(PlayVerifier::from_base64(&wrapped, PACKAGE).is_ok());
    }

    #[test]
    fn bad_keys_are_rejected() {
        assert!(PlayVerifier::from_base64("", PACKAGE).is_err());
        assert!(PlayVerifier::from_base64("not base64 !!", PACKAGE).is_err());
        assert!(PlayVerifier::from_base64("AAAA", PACKAGE).is_err());
    }

    #[test]
    fn tampered_json_fails_signature() {
        let mut p = raw("purchased");
        p.original_json = p.original_json.replace("tok-purchased-1", "tok-forged-1");
        assert_eq!(verifier().verify(&p), Err(VerifyError::BadSignature));
        let mut pending_to_purchased = raw("pending");
        pending_to_purchased.original_json = pending_to_purchased
            .original_json
            .replace("\"purchaseState\":4", "\"purchaseState\":0");
        assert_eq!(
            verifier().verify(&pending_to_purchased),
            Err(VerifyError::BadSignature)
        );
    }

    #[test]
    fn signature_from_another_key_fails() {
        let mut p = raw("purchased");
        p.signature = vectors().purchased_signed_by_other_key;
        assert_eq!(verifier().verify(&p), Err(VerifyError::BadSignature));
        let other = PlayVerifier::from_base64(&vectors().other_public_key, PACKAGE).unwrap();
        assert_eq!(
            other.verify(&raw("purchased")),
            Err(VerifyError::BadSignature)
        );
    }

    #[test]
    fn garbage_signature_is_rejected() {
        let mut p = raw("purchased");
        p.signature = "%%%".into();
        assert_eq!(verifier().verify(&p), Err(VerifyError::SignatureEncoding));
        p.signature = String::new();
        assert_eq!(verifier().verify(&p), Err(VerifyError::BadSignature));
    }

    #[test]
    fn genuine_purchase_for_another_package_is_rejected() {
        assert_eq!(
            verifier().verify(&raw("otherpkg")),
            Err(VerifyError::WrongPackage)
        );
    }

    #[test]
    fn genuine_purchase_of_another_product_is_ignored() {
        assert_eq!(
            verifier().verify(&raw("otherproduct")),
            Err(VerifyError::OtherProduct)
        );
        let r = reconcile(
            &verifier(),
            None,
            QueryOutcome::Owned(&[raw("otherproduct")]),
            NOW,
        );
        assert_eq!(r.state, PlayState::NotOwned);
        assert_eq!(r.rejected, 0);
    }

    #[test]
    fn promo_redemption_without_order_id_uses_product_ids_list() {
        let p = verifier().verify(&raw("promo")).expect("promo verifies");
        assert_eq!(p.state, PurchaseState::Purchased);
        assert!(p.acknowledged);
        let r = reconcile(&verifier(), None, QueryOutcome::Owned(&[raw("promo")]), NOW);
        assert!(r.state.grants_pro());
        assert_eq!(r.acknowledge, None);
    }

    #[test]
    fn purchased_grants_pro_caches_and_asks_for_acknowledgment() {
        let r = reconcile(
            &verifier(),
            None,
            QueryOutcome::Owned(&[raw("purchased")]),
            NOW,
        );
        assert_eq!(
            r.state,
            PlayState::Owned {
                acknowledged: false,
                verified_at_ms: NOW,
                cached: false
            }
        );
        assert_eq!(r.acknowledge.as_deref(), Some("tok-purchased-1"));
        assert!(matches!(r.cache, CacheAction::Store(ref c) if c.verified_at_ms == NOW));
        assert_eq!(effective_entitlement(false, &r.state), Entitlement::Pro);
    }

    #[test]
    fn acknowledged_purchase_is_not_acknowledged_again() {
        let r = reconcile(&verifier(), None, QueryOutcome::Owned(&[raw("acked")]), NOW);
        assert!(r.state.grants_pro());
        assert_eq!(r.acknowledge, None);
    }

    #[test]
    fn pending_is_free_and_never_acknowledged() {
        let r = reconcile(
            &verifier(),
            None,
            QueryOutcome::Owned(&[raw("pending")]),
            NOW,
        );
        assert_eq!(r.state, PlayState::Pending);
        assert_eq!(r.acknowledge, None);
        assert_eq!(r.cache, CacheAction::Clear);
        assert_eq!(effective_entitlement(false, &r.state), Entitlement::Free);
    }

    #[test]
    fn refund_or_revocation_drops_play_pro_and_clears_cache() {
        let first = reconcile(&verifier(), None, QueryOutcome::Owned(&[raw("acked")]), NOW);
        let CacheAction::Store(cache) = first.cache else {
            panic!("expected cache")
        };
        // Play no longer lists the purchase once it is refunded or revoked.
        let after = reconcile(
            &verifier(),
            Some(&cache),
            QueryOutcome::Owned(&[]),
            NOW + DAY_MS,
        );
        assert_eq!(after.state, PlayState::NotOwned);
        assert_eq!(after.cache, CacheAction::Clear);
        assert_eq!(
            effective_entitlement(false, &after.state),
            Entitlement::Free
        );
    }

    #[test]
    fn refund_keeps_an_independent_license_key() {
        let after = reconcile(&verifier(), None, QueryOutcome::Owned(&[]), NOW);
        assert_eq!(effective_entitlement(true, &after.state), Entitlement::Pro);
    }

    #[test]
    fn unverifiable_purchase_never_unlocks() {
        let mut forged = raw("purchased");
        forged.signature = vectors().purchased_signed_by_other_key;
        let r = reconcile(&verifier(), None, QueryOutcome::Owned(&[forged]), NOW);
        assert!(matches!(r.state, PlayState::Error { .. }));
        assert_eq!(r.rejected, 1);
        assert_eq!(r.acknowledge, None);
        assert_eq!(r.cache, CacheAction::Clear);
        assert_eq!(effective_entitlement(false, &r.state), Entitlement::Free);
    }

    #[test]
    fn query_error_without_cache_is_free() {
        let r = reconcile(
            &verifier(),
            None,
            QueryOutcome::Failed("offline".into()),
            NOW,
        );
        assert_eq!(
            r.state,
            PlayState::Error {
                message: "offline".into()
            }
        );
        assert_eq!(effective_entitlement(false, &r.state), Entitlement::Free);
    }

    fn cache_of(name: &str, verified_at_ms: i64) -> CachedPurchase {
        let r = raw(name);
        CachedPurchase {
            original_json: r.original_json,
            signature: r.signature,
            verified_at_ms,
        }
    }

    #[test]
    fn query_error_keeps_a_recent_verified_purchase_within_grace() {
        let cache = cache_of("acked", NOW);
        let r = reconcile(
            &verifier(),
            Some(&cache),
            QueryOutcome::Failed("offline".into()),
            NOW + 6 * DAY_MS,
        );
        assert_eq!(
            r.state,
            PlayState::Owned {
                acknowledged: true,
                verified_at_ms: NOW,
                cached: true
            }
        );
        assert_eq!(r.cache, CacheAction::Keep);
    }

    #[test]
    fn query_error_after_grace_is_free() {
        let cache = cache_of("acked", NOW);
        let r = reconcile(
            &verifier(),
            Some(&cache),
            QueryOutcome::Failed("offline".into()),
            NOW + OFFLINE_GRACE_MS + 1,
        );
        assert!(!r.state.grants_pro());
    }

    #[test]
    fn clock_set_far_back_does_not_extend_grace() {
        let cache = cache_of("acked", NOW);
        assert_eq!(
            cached_owned_state(&verifier(), &cache, NOW - 2 * DAY_MS),
            None
        );
        assert!(cached_owned_state(&verifier(), &cache, NOW - DAY_MS / 2).is_some());
    }

    #[test]
    fn edited_cache_is_not_honored() {
        let mut cache = cache_of("acked", NOW);
        cache.original_json = cache.original_json.replace("tok-purchased-1", "tok-x");
        assert_eq!(cached_owned_state(&verifier(), &cache, NOW), None);
        let pending = cache_of("pending", NOW);
        assert_eq!(cached_owned_state(&verifier(), &pending, NOW), None);
    }

    #[test]
    fn purchase_flow_result_only_speaks_for_what_it_contains() {
        let v = verifier();
        assert!(reconcile_flow(&v, &[], NOW).is_none());
        assert!(reconcile_flow(&v, &[raw("otherproduct")], NOW).is_none());
        let pending = reconcile_flow(&v, &[raw("pending")], NOW).unwrap();
        assert_eq!(pending.state, PlayState::Pending);
        assert_eq!(pending.cache, CacheAction::Keep);
        let bought = reconcile_flow(&v, &[raw("purchased")], NOW).unwrap();
        assert!(bought.state.grants_pro());
        assert_eq!(bought.acknowledge.as_deref(), Some("tok-purchased-1"));
    }

    #[test]
    fn owned_wins_over_a_pending_second_purchase() {
        let r = reconcile(
            &verifier(),
            None,
            QueryOutcome::Owned(&[raw("pending"), raw("acked")]),
            NOW,
        );
        assert!(r.state.grants_pro());
    }

    #[test]
    fn distribution_flag_parsing() {
        assert_eq!(Distribution::from_flag(Some("play")), Distribution::Play);
        assert_eq!(Distribution::from_flag(Some(" PLAY ")), Distribution::Play);
        assert_eq!(
            Distribution::from_flag(Some("sideload")),
            Distribution::Sideload
        );
        assert_eq!(Distribution::from_flag(None), Distribution::Sideload);
        assert!(!Distribution::Play.allows_license_keys());
        assert!(Distribution::Sideload.allows_license_keys());
    }

    #[test]
    fn cancel_is_quiet_and_errors_are_user_facing() {
        assert_eq!(
            response_message(response_code::USER_CANCELED, ""),
            "Purchase canceled."
        );
        assert!(response_message(response_code::NETWORK_ERROR, "x").contains("Couldn't reach"));
        assert!(response_message(999, "").contains("Try again"));
    }
}

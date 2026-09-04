//! Entitlement model plus offline license verification, shared by the desktop
//! and mobile frontends.
//!
//! Desktop constructs `AppState` as Pro. Mobile starts Free and later replaces
//! the entitlement from a validated, signed license token.
//!
//! # License format
//!
//! `ATVOPT-<payload>-<signature>`, both halves Crockford base32 (case
//! insensitive, and `I`/`L` decode as `1`, `O` as `0`, so a phone keyboard and
//! a hand-copied key can't produce a false rejection).
//!
//! The payload is compact binary, big-endian:
//!
//! | offset | size | field                                        |
//! |--------|------|----------------------------------------------|
//! | 0      | 1    | format version (`PAYLOAD_VERSION`)           |
//! | 1      | 1    | plan (`PLAN_PRO`)                            |
//! | 2      | 1    | signing key id (see [`PUBLIC_KEYS`])         |
//! | 3      | 2    | issued, days since 1970-01-01                |
//! | 5      | 2    | expires, days since 1970-01-01 (0 = perpetual) |
//! | 7      | 1    | licensee byte length (1..=`MAX_LICENSEE_LEN`) |
//! | 8      | n    | licensee, UTF-8                              |
//!
//! Dates are `u16` days rather than a timestamp: it keeps the typed key short
//! and still reaches the year 2149.
//!
//! The signature is Ed25519 over the exact payload bytes (64 bytes). There is
//! no server: verification is a pure function of the key plus the public keys
//! embedded below.

use chrono::{Duration, NaiveDate, Utc};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};

/// Documented test license key that unlocks Pro in debug builds only, so the
/// activation flow and its gates can be exercised without issuing a real key.
/// Release builds reject it — see [`validate_license_key`].
pub const TEST_LICENSE_KEY: &str = "ATVOPT-PRO-2025";

/// Every license string starts with this.
pub const LICENSE_PREFIX: &str = "ATVOPT-";

/// Current payload format version.
pub const PAYLOAD_VERSION: u8 = 1;

/// Plan byte for a Pro license. No other plan is issued today.
const PLAN_PRO: u8 = 1;

/// Header bytes before the licensee string.
const PAYLOAD_HEADER_LEN: usize = 8;

/// Longest licensee string that fits the length byte's documented range.
pub const MAX_LICENSEE_LEN: usize = 40;

/// Dev signing key (`key_id` 0). Compiled in for debug builds only so a
/// dev-issued license never unlocks a shipped build. The matching private key
/// lives outside the repo at `~/.atvopt/license-signing-key.dev`.
#[cfg(debug_assertions)]
const DEV_PUBLIC_KEY: [u8; 32] = [
    0x3b, 0x13, 0x6d, 0x69, 0xd8, 0x2c, 0x5f, 0xc7, 0xb3, 0x00, 0xda, 0xcb, 0x22, 0x21, 0x7b, 0x03,
    0x32, 0x06, 0xfc, 0x79, 0x7a, 0x32, 0x58, 0x8c, 0x64, 0xcc, 0x38, 0x4f, 0x6e, 0xd7, 0x24, 0x99,
];

/// Production signing key (`key_id` 1). The matching private key lives outside
/// the repo at `~/.atvopt/license-signing-key.prod` — never in version
/// control. Rotate by generating a new pair, appending it here under the next
/// id, and issuing with `--key-id <new>`; leave the old entry in place so
/// already-sold keys keep working. See `mobile/LICENSING.md`.
const PROD_PUBLIC_KEY: [u8; 32] = [
    0x27, 0x7b, 0xfb, 0x0a, 0xf0, 0x33, 0xac, 0x89, 0xdd, 0x80, 0x40, 0x3e, 0x3e, 0xd2, 0xce, 0xb7,
    0xd9, 0x43, 0xa9, 0x45, 0x0b, 0x98, 0xb6, 0x67, 0xbc, 0xfa, 0xac, 0x95, 0xea, 0xc8, 0xa4, 0xe0,
];

/// Public keys trusted by this build, newest last. `key_id` is carried in the
/// payload so a compromised key can be dropped from this list without
/// invalidating licenses signed by the others.
pub const PUBLIC_KEYS: &[(u8, [u8; 32])] = &[
    #[cfg(debug_assertions)]
    (0, DEV_PUBLIC_KEY),
    (1, PROD_PUBLIC_KEY),
];

/// A verified license's contents. Serialized to the frontend by the
/// `license_info` command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LicenseInfo {
    pub plan: Entitlement,
    pub licensee: String,
    pub issued: NaiveDate,
    /// `None` for a perpetual license.
    pub expires: Option<NaiveDate>,
    pub key_id: u8,
}

/// Why a license string was rejected. `Display` strings are user-facing — they
/// are surfaced verbatim in the mobile activation card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LicenseError {
    /// Not a license string at all: wrong prefix, wrong shape, bad characters,
    /// truncated payload.
    Malformed,
    /// Well-formed but the signature doesn't match the payload.
    BadSignature,
    /// Signed by a key this build doesn't trust.
    UnknownKey(u8),
    /// Payload format newer than this build understands.
    UnsupportedVersion(u8),
    /// Valid signature, but the plan byte isn't one we sell.
    UnknownPlan(u8),
    /// Valid signature, but the term is over.
    Expired(NaiveDate),
    /// Issuing only: licensee empty or longer than [`MAX_LICENSEE_LEN`] bytes.
    InvalidLicensee,
    /// Issuing only: a date outside the representable `u16` day range.
    DateOutOfRange(NaiveDate),
}

impl std::fmt::Display for LicenseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LicenseError::Malformed => write!(
                f,
                "That doesn't look like an ATV Optimizer license key. Check for missing characters and try again."
            ),
            LicenseError::BadSignature => write!(
                f,
                "That license key failed its signature check — it may have been mistyped or altered."
            ),
            LicenseError::UnknownKey(id) => write!(
                f,
                "That license key was signed by an unknown key (id {id}). Update the app, or contact support for a replacement."
            ),
            LicenseError::UnsupportedVersion(v) => write!(
                f,
                "That license key uses a newer format (version {v}). Update the app to activate it."
            ),
            LicenseError::UnknownPlan(p) => {
                write!(f, "That license key is for an unknown plan (code {p}).")
            }
            LicenseError::Expired(on) => write!(
                f,
                "That license expired on {on}. Contact support to renew it."
            ),
            LicenseError::InvalidLicensee => write!(
                f,
                "The licensee must be 1 to {MAX_LICENSEE_LEN} bytes of text."
            ),
            LicenseError::DateOutOfRange(d) => {
                write!(f, "{d} is outside the range a license can encode.")
            }
        }
    }
}

impl std::error::Error for LicenseError {}

/// Validate a license key. True for a correctly signed, unexpired Pro license
/// — and, in debug builds only, for [`TEST_LICENSE_KEY`].
pub fn validate_license_key(key: &str) -> bool {
    matches!(
        parse_license(key),
        Ok(LicenseInfo {
            plan: Entitlement::Pro,
            ..
        })
    )
}

/// Verify a license key and return its contents, or a user-facing reason it was
/// rejected. Expiry is judged against today's UTC date.
pub fn parse_license(key: &str) -> Result<LicenseInfo, LicenseError> {
    parse_license_at(key, Utc::now().date_naive())
}

/// [`parse_license`] with an injected "today", so expiry behavior is testable.
pub fn parse_license_at(key: &str, today: NaiveDate) -> Result<LicenseInfo, LicenseError> {
    #[cfg(debug_assertions)]
    if normalize(key) == TEST_LICENSE_KEY {
        return Ok(LicenseInfo {
            plan: Entitlement::Pro,
            licensee: "Debug build test key".to_string(),
            issued: today,
            expires: None,
            key_id: 0,
        });
    }
    verify_with_keys(key, PUBLIC_KEYS, today)
}

/// Uppercase and strip whitespace: license keys are case-insensitive and are
/// routinely pasted with stray spaces or a trailing newline.
fn normalize(key: &str) -> String {
    key.chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_uppercase)
        .collect()
}

/// The real verifier, parameterized over the trusted key list so tests can
/// supply their own pair instead of shipping a test key in [`PUBLIC_KEYS`].
fn verify_with_keys(
    key: &str,
    keys: &[(u8, [u8; 32])],
    today: NaiveDate,
) -> Result<LicenseInfo, LicenseError> {
    let normalized = normalize(key);
    let body = normalized
        .strip_prefix(LICENSE_PREFIX)
        .ok_or(LicenseError::Malformed)?;
    let mut parts = body.split('-');
    let (payload_b32, signature_b32) = match (parts.next(), parts.next(), parts.next()) {
        (Some(p), Some(s), None) if !p.is_empty() && !s.is_empty() => (p, s),
        _ => return Err(LicenseError::Malformed),
    };

    let payload = base32_decode(payload_b32).ok_or(LicenseError::Malformed)?;
    let signature: [u8; 64] = base32_decode(signature_b32)
        .ok_or(LicenseError::Malformed)?
        .try_into()
        .map_err(|_| LicenseError::Malformed)?;

    if payload.len() < PAYLOAD_HEADER_LEN {
        return Err(LicenseError::Malformed);
    }
    let version = payload[0];
    if version != PAYLOAD_VERSION {
        return Err(LicenseError::UnsupportedVersion(version));
    }
    let key_id = payload[2];
    let public_key = keys
        .iter()
        .find(|(id, _)| *id == key_id)
        .map(|(_, bytes)| bytes)
        .ok_or(LicenseError::UnknownKey(key_id))?;

    VerifyingKey::from_bytes(public_key)
        .map_err(|_| LicenseError::UnknownKey(key_id))?
        .verify_strict(&payload, &Signature::from_bytes(&signature))
        .map_err(|_| LicenseError::BadSignature)?;

    // Past this point the payload is authentic, so its fields can be trusted.
    let plan = match payload[1] {
        PLAN_PRO => Entitlement::Pro,
        other => return Err(LicenseError::UnknownPlan(other)),
    };
    let issued = date_from_days(u16::from_be_bytes([payload[3], payload[4]]))
        .ok_or(LicenseError::Malformed)?;
    let expires = match u16::from_be_bytes([payload[5], payload[6]]) {
        0 => None,
        days => Some(date_from_days(days).ok_or(LicenseError::Malformed)?),
    };
    let licensee_len = payload[7] as usize;
    if licensee_len == 0
        || licensee_len > MAX_LICENSEE_LEN
        || payload.len() != PAYLOAD_HEADER_LEN + licensee_len
    {
        return Err(LicenseError::Malformed);
    }
    let licensee = std::str::from_utf8(&payload[PAYLOAD_HEADER_LEN..])
        .map_err(|_| LicenseError::Malformed)?
        .to_string();

    if let Some(expires) = expires {
        if today > expires {
            return Err(LicenseError::Expired(expires));
        }
    }

    Ok(LicenseInfo {
        plan,
        licensee,
        issued,
        expires,
        key_id,
    })
}

/// Sign a Pro license. Lives here (not in the CLI) so the issuer and the
/// verifier can never disagree about the payload layout.
pub fn issue_license(
    signing_key: &[u8; 32],
    key_id: u8,
    licensee: &str,
    issued: NaiveDate,
    expires: Option<NaiveDate>,
) -> Result<String, LicenseError> {
    let licensee = licensee.trim();
    if licensee.is_empty() || licensee.len() > MAX_LICENSEE_LEN {
        return Err(LicenseError::InvalidLicensee);
    }
    let issued_days = days_from_date(issued).ok_or(LicenseError::DateOutOfRange(issued))?;
    let expires_days = match expires {
        None => 0,
        Some(d) => match days_from_date(d) {
            Some(0) | None => return Err(LicenseError::DateOutOfRange(d)),
            Some(days) => days,
        },
    };

    let mut payload = Vec::with_capacity(PAYLOAD_HEADER_LEN + licensee.len());
    payload.push(PAYLOAD_VERSION);
    payload.push(PLAN_PRO);
    payload.push(key_id);
    payload.extend_from_slice(&issued_days.to_be_bytes());
    payload.extend_from_slice(&expires_days.to_be_bytes());
    payload.push(licensee.len() as u8);
    payload.extend_from_slice(licensee.as_bytes());

    let signature = SigningKey::from_bytes(signing_key).sign(&payload);
    Ok(format!(
        "{LICENSE_PREFIX}{}-{}",
        base32_encode(&payload),
        base32_encode(&signature.to_bytes())
    ))
}

/// The public key matching a private signing key — used by the CLI's `keygen`
/// to print the literal that gets pasted into [`PUBLIC_KEYS`].
pub fn public_key_for(signing_key: &[u8; 32]) -> [u8; 32] {
    SigningKey::from_bytes(signing_key)
        .verifying_key()
        .to_bytes()
}

fn epoch() -> NaiveDate {
    NaiveDate::from_ymd_opt(1970, 1, 1).expect("1970-01-01 is a valid date")
}

fn days_from_date(date: NaiveDate) -> Option<u16> {
    u16::try_from(date.signed_duration_since(epoch()).num_days()).ok()
}

fn date_from_days(days: u16) -> Option<NaiveDate> {
    epoch().checked_add_signed(Duration::days(days as i64))
}

/// Crockford base32 alphabet — no `I`, `L`, `O`, or `U`.
const CROCKFORD: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

fn base32_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity((data.len() * 8).div_ceil(5));
    let mut acc: u32 = 0;
    let mut bits: u32 = 0;
    for &byte in data {
        acc = (acc << 8) | byte as u32;
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(CROCKFORD[((acc >> bits) & 0x1f) as usize] as char);
        }
    }
    if bits > 0 {
        out.push(CROCKFORD[((acc << (5 - bits)) & 0x1f) as usize] as char);
    }
    out
}

/// Decode Crockford base32. `None` on an invalid character, a stray trailing
/// symbol, or non-zero padding bits — all of which mean the key was mistyped.
fn base32_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() * 5 / 8);
    let mut acc: u32 = 0;
    let mut bits: u32 = 0;
    for ch in s.chars() {
        acc = (acc << 5) | crockford_value(ch)? as u32;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push(((acc >> bits) & 0xff) as u8);
        }
    }
    if bits >= 5 || acc & ((1 << bits) - 1) != 0 {
        return None;
    }
    Some(out)
}

fn crockford_value(ch: char) -> Option<u8> {
    let c = ch.to_ascii_uppercase();
    match c {
        'O' => Some(0),
        'I' | 'L' => Some(1),
        '0'..='9' => Some(c as u8 - b'0'),
        'A'..='H' => Some(c as u8 - b'A' + 10),
        'J' => Some(18),
        'K' => Some(19),
        'M' => Some(20),
        'N' => Some(21),
        'P'..='T' => Some(c as u8 - b'P' + 22),
        'V'..='Z' => Some(c as u8 - b'V' + 27),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Entitlement {
    Free,
    Pro,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Feature {
    CuratedDebloat,
    OptimizeWizard,
    Snapshot,
    LauncherTakeover,
    TweaksWrite,
    AppPermissionWrite,
    AdvancedReboot,
    MultiDevice,
    AdvancedRemote,
    Sideload,
    FileManager,
    BackupClone,
}

impl Feature {
    pub fn code(self) -> &'static str {
        match self {
            Feature::CuratedDebloat => "curated_debloat",
            Feature::OptimizeWizard => "optimize_wizard",
            Feature::Snapshot => "snapshot",
            Feature::LauncherTakeover => "launcher_takeover",
            Feature::TweaksWrite => "tweaks_write",
            Feature::AppPermissionWrite => "app_permission_write",
            Feature::AdvancedReboot => "advanced_reboot",
            Feature::MultiDevice => "multi_device",
            Feature::AdvancedRemote => "advanced_remote",
            Feature::Sideload => "sideload",
            Feature::FileManager => "file_manager",
            Feature::BackupClone => "backup_clone",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A deterministic key pair for tests — never trusted by a real build.
    const TEST_SEED_A: [u8; 32] = [7u8; 32];
    const TEST_SEED_B: [u8; 32] = [9u8; 32];

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    /// Verify against a test key list instead of the compiled-in
    /// [`PUBLIC_KEYS`], exercising the real verifier.
    fn parse_with(
        license: &str,
        keys: &[(u8, [u8; 32])],
        today: NaiveDate,
    ) -> Result<LicenseInfo, LicenseError> {
        verify_with_keys(license, keys, today)
    }

    fn keys_a() -> Vec<(u8, [u8; 32])> {
        vec![(1, public_key_for(&TEST_SEED_A))]
    }

    #[test]
    fn issue_then_verify_round_trips() {
        let license = issue_license(
            &TEST_SEED_A,
            1,
            "buyer@example.com",
            day(2026, 1, 15),
            Some(day(2027, 1, 15)),
        )
        .unwrap();
        assert!(license.starts_with(LICENSE_PREFIX));
        let info = parse_with(&license, &keys_a(), day(2026, 6, 1)).unwrap();
        assert_eq!(info.plan, Entitlement::Pro);
        assert_eq!(info.licensee, "buyer@example.com");
        assert_eq!(info.issued, day(2026, 1, 15));
        assert_eq!(info.expires, Some(day(2027, 1, 15)));
        assert_eq!(info.key_id, 1);
    }

    #[test]
    fn perpetual_license_has_no_expiry() {
        let license = issue_license(&TEST_SEED_A, 1, "Owner", day(2026, 1, 1), None).unwrap();
        let info = parse_with(&license, &keys_a(), day(2099, 1, 1)).unwrap();
        assert_eq!(info.expires, None);
    }

    #[test]
    fn license_survives_lowercase_and_confusable_characters() {
        let license = issue_license(&TEST_SEED_A, 1, "Owner", day(2026, 1, 1), None).unwrap();
        // Crockford maps I/L -> 1 and O -> 0, so a hand-copied key that swaps
        // them must still decode to the same bytes.
        let mangled = license.to_lowercase().replace('1', "l").replace('0', "o");
        let normalized: String = mangled.chars().flat_map(char::to_uppercase).collect();
        assert_eq!(
            parse_with(&license, &keys_a(), day(2026, 6, 1)).unwrap(),
            parse_with(&normalized, &keys_a(), day(2026, 6, 1)).unwrap()
        );
    }

    #[test]
    fn tampered_payload_is_rejected() {
        let license =
            issue_license(&TEST_SEED_A, 1, "buyer@example.com", day(2026, 1, 1), None).unwrap();
        let body = license.strip_prefix(LICENSE_PREFIX).unwrap();
        let (payload, signature) = body.split_once('-').unwrap();
        let mut bytes = base32_decode(payload).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 0x01;
        let forged = format!("{LICENSE_PREFIX}{}-{signature}", base32_encode(&bytes));
        assert_eq!(
            parse_with(&forged, &keys_a(), day(2026, 6, 1)),
            Err(LicenseError::BadSignature)
        );
    }

    #[test]
    fn signature_from_another_key_is_rejected() {
        let license =
            issue_license(&TEST_SEED_B, 1, "buyer@example.com", day(2026, 1, 1), None).unwrap();
        assert_eq!(
            parse_with(&license, &keys_a(), day(2026, 6, 1)),
            Err(LicenseError::BadSignature)
        );
    }

    #[test]
    fn unknown_key_id_is_rejected() {
        let license =
            issue_license(&TEST_SEED_A, 9, "buyer@example.com", day(2026, 1, 1), None).unwrap();
        assert_eq!(
            parse_with(&license, &keys_a(), day(2026, 6, 1)),
            Err(LicenseError::UnknownKey(9))
        );
        assert!(!validate_license_key(&license));
    }

    #[test]
    fn expired_license_is_rejected() {
        let license = issue_license(
            &TEST_SEED_A,
            1,
            "buyer@example.com",
            day(2025, 1, 1),
            Some(day(2026, 1, 1)),
        )
        .unwrap();
        assert!(
            parse_with(&license, &keys_a(), day(2026, 1, 1)).is_ok(),
            "valid through the expiry date itself"
        );
        assert_eq!(
            parse_with(&license, &keys_a(), day(2026, 1, 2)),
            Err(LicenseError::Expired(day(2026, 1, 1)))
        );
        assert!(!validate_license_key(&license));
    }

    #[test]
    fn malformed_strings_are_rejected() {
        for bad in [
            "",
            "nope",
            "ATVOPT",
            "ATVOPT-",
            "ATVOPT-ONLYONEPART",
            "ATVOPT-AAAA-BBBB-CCCC",
            "ATVOPT-!!!!-????",
            "ATVOPT-AAAAAAAA-AAAAAAAA",
            "PRO-AAAA-BBBB",
        ] {
            assert_eq!(
                parse_license_at(bad, day(2026, 6, 1)),
                Err(LicenseError::Malformed),
                "{bad} should be malformed"
            );
            assert!(!validate_license_key(bad));
        }
    }

    #[test]
    fn issuing_rejects_bad_licensee() {
        assert_eq!(
            issue_license(&TEST_SEED_A, 1, "   ", day(2026, 1, 1), None),
            Err(LicenseError::InvalidLicensee)
        );
        let long = "x".repeat(MAX_LICENSEE_LEN + 1);
        assert_eq!(
            issue_license(&TEST_SEED_A, 1, &long, day(2026, 1, 1), None),
            Err(LicenseError::InvalidLicensee)
        );
    }

    #[cfg(debug_assertions)]
    #[test]
    fn dev_test_key_is_accepted_in_debug_builds() {
        assert!(validate_license_key(TEST_LICENSE_KEY));
        assert!(validate_license_key("  atvopt-pro-2025  "));
        assert!(validate_license_key("ATVOpt-Pro-2025"));
        assert_eq!(parse_license(TEST_LICENSE_KEY).unwrap().key_id, 0);
        assert!(PUBLIC_KEYS.iter().any(|(id, _)| *id == 0));
    }

    #[cfg(not(debug_assertions))]
    #[test]
    fn dev_test_key_and_dev_signing_key_are_rejected_in_release_builds() {
        assert!(!validate_license_key(TEST_LICENSE_KEY));
        assert!(!PUBLIC_KEYS.iter().any(|(id, _)| *id == 0));
    }

    #[test]
    fn production_key_id_is_trusted() {
        assert!(PUBLIC_KEYS.iter().any(|(id, _)| *id == 1));
    }

    #[test]
    fn base32_round_trips_arbitrary_bytes() {
        for len in 0..40usize {
            let bytes: Vec<u8> = (0..len).map(|i| (i as u8).wrapping_mul(37)).collect();
            assert_eq!(
                base32_decode(&base32_encode(&bytes)).as_deref(),
                Some(&bytes[..])
            );
        }
    }

    #[test]
    fn issued_license_length_stays_typeable() {
        let license =
            issue_license(&TEST_SEED_A, 1, "buyer@example.com", day(2026, 1, 1), None).unwrap();
        assert!(license.len() < 200, "license was {} chars", license.len());
    }
}

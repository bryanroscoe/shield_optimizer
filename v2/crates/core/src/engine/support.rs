//! Device support tiers: how well this app is known to work on a TV.
//!
//! Pure. The rules are data (`crates/core/data/device-support.json`, or a
//! validated copy fetched by the host layer); this module only parses and
//! matches them against what a device reported about itself.
//!
//! A tier is informational. Nothing here feeds `engine::safety`: a
//! `fully_supported` TV gets exactly the same do-not-disable gates as an
//! `unknown` one, and `not_supported` never blocks a recovery or enable path.
//!
//! The identity rule applies: only properties the device actually reported
//! can match. An unauthorized or unreadable device has no properties, so it
//! is `unknown` and the app claims nothing about it.

use serde::{Deserialize, Serialize};

use super::types::DeviceProperties;

/// The only rules schema this build understands.
pub const SUPPORT_SCHEMA: u32 = 1;
/// A note is one short line in a badge tooltip or banner.
pub const MAX_NOTE_CHARS: usize = 200;
/// Generous for a hand-maintained list; bounds a hostile or broken download.
pub const MAX_RULES: usize = 500;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupportTier {
    #[default]
    Unknown,
    NotSupported,
    Beta,
    Supported,
    FullySupported,
}

impl SupportTier {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::NotSupported => "not_supported",
            Self::Beta => "beta",
            Self::Supported => "supported",
            Self::FullySupported => "fully_supported",
        }
    }
}

/// Every present field must match. Unknown fields are rejected so a newer
/// rule file using a matcher this build doesn't know fails validation (and
/// falls back to the embedded rules) instead of matching more TVs than its
/// author meant.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SupportMatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manufacturer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brand: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_prefix: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupportRule {
    #[serde(rename = "match")]
    pub matcher: SupportMatch,
    pub tier: SupportTier,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupportRules {
    pub schema: u32,
    #[serde(default)]
    pub updated: String,
    pub rules: Vec<SupportRule>,
}

/// What the UI shows for one device.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupportInfo {
    pub tier: SupportTier,
    /// The matching rule's note. `None` for `unknown`: there is nothing to
    /// say yet, and the UI supplies its own "not verified" line.
    #[serde(default)]
    pub note: Option<String>,
}

fn present(field: &Option<String>) -> Option<&str> {
    field.as_deref().map(str::trim).filter(|s| !s.is_empty())
}

impl SupportMatch {
    fn is_empty(&self) -> bool {
        present(&self.manufacturer).is_none()
            && present(&self.brand).is_none()
            && present(&self.model_prefix).is_none()
            && present(&self.device).is_none()
    }

    fn matches(&self, props: &DeviceProperties) -> bool {
        // `ro.*` props that read as empty or the literal "unknown" were not
        // reported, and a field that wasn't reported can't satisfy a rule.
        fn reported(value: &str) -> Option<&str> {
            let v = value.trim();
            (!v.is_empty() && !v.eq_ignore_ascii_case("unknown")).then_some(v)
        }
        let exact = |want: &Option<String>, got: &str| match present(want) {
            None => true,
            Some(w) => reported(got).is_some_and(|g| g.eq_ignore_ascii_case(w)),
        };
        let prefix_ok = match present(&self.model_prefix) {
            None => true,
            Some(p) => reported(&props.model).is_some_and(|m| {
                m.len() >= p.len()
                    && m.is_char_boundary(p.len())
                    && m[..p.len()].eq_ignore_ascii_case(p)
            }),
        };
        !self.is_empty()
            && exact(&self.manufacturer, &props.manufacturer)
            && exact(&self.brand, &props.brand)
            && exact(&self.device, &props.device_codename)
            && prefix_ok
    }
}

/// Parse and validate a rules document. Any problem rejects the whole
/// document; the caller keeps whatever rules it already had.
pub fn parse_support_rules(json: &str) -> Result<SupportRules, String> {
    let rules: SupportRules =
        serde_json::from_str(json).map_err(|e| format!("support rules: {e}"))?;
    if rules.schema != SUPPORT_SCHEMA {
        return Err(format!(
            "support rules: schema {} is not {SUPPORT_SCHEMA}",
            rules.schema
        ));
    }
    if rules.rules.len() > MAX_RULES {
        return Err(format!("support rules: more than {MAX_RULES} rules"));
    }
    for (i, rule) in rules.rules.iter().enumerate() {
        if rule.matcher.is_empty() {
            return Err(format!("support rules: rule {i} matches every device"));
        }
        let note = rule.note.trim();
        if note.is_empty() || note.chars().count() > MAX_NOTE_CHARS {
            return Err(format!(
                "support rules: rule {i} note must be 1-{MAX_NOTE_CHARS} characters"
            ));
        }
    }
    Ok(rules)
}

/// First matching rule wins. `None` properties (the device couldn't be
/// queried) are always `unknown`.
pub fn classify_support(props: Option<&DeviceProperties>, rules: &SupportRules) -> SupportInfo {
    let Some(props) = props else {
        return SupportInfo::default();
    };
    rules
        .rules
        .iter()
        .find(|rule| rule.matcher.matches(props))
        .map(|rule| SupportInfo {
            tier: rule.tier,
            note: Some(rule.note.trim().to_string()),
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    const EMBEDDED: &str = include_str!("../../data/device-support.json");

    fn embedded() -> SupportRules {
        parse_support_rules(EMBEDDED).expect("embedded rules parse")
    }

    fn props(manufacturer: &str, brand: &str, model: &str, device: &str) -> DeviceProperties {
        DeviceProperties {
            manufacturer: manufacturer.into(),
            brand: brand.into(),
            model: model.into(),
            device_codename: device.into(),
            ..Default::default()
        }
    }

    #[test]
    fn shield_fixture_is_fully_supported() {
        // Strings from tests/fixtures/devices/shield-tv-pro/getprop.txt.
        let p = props("NVIDIA", "NVIDIA", "SHIELD Android TV", "mdarcy");
        let info = classify_support(Some(&p), &embedded());
        assert_eq!(info.tier, SupportTier::FullySupported);
        assert!(info.note.is_some());
    }

    #[test]
    fn tcl_is_beta() {
        let p = props("TCL", "TCL", "QM7L Pro", "x");
        assert_eq!(
            classify_support(Some(&p), &embedded()).tier,
            SupportTier::Beta
        );
    }

    #[test]
    fn fire_tv_is_not_supported_with_its_reason() {
        let p = props("Amazon", "Amazon", "AFTMM", "mantis");
        let info = classify_support(Some(&p), &embedded());
        assert_eq!(info.tier, SupportTier::NotSupported);
        assert!(info.note.unwrap().contains("Fire OS"));
    }

    #[test]
    fn amazon_non_fire_tv_model_is_not_claimed() {
        let p = props("Amazon", "Amazon", "KFTRWI", "trona");
        assert_eq!(
            classify_support(Some(&p), &embedded()).tier,
            SupportTier::Unknown
        );
    }

    #[test]
    fn pixel_phone_fixture_is_unknown() {
        let p = props("Google", "google", "Pixel 10 Pro", "blazer");
        assert_eq!(
            classify_support(Some(&p), &embedded()),
            SupportInfo::default()
        );
    }

    #[test]
    fn unreadable_device_claims_nothing() {
        assert_eq!(classify_support(None, &embedded()), SupportInfo::default());
        // Read but blank: nothing was reported, so even a manufacturer-only
        // rule can't match.
        let blank = props("", "", "", "");
        assert_eq!(
            classify_support(Some(&blank), &embedded()).tier,
            SupportTier::Unknown
        );
        let literal_unknown = props("unknown", "unknown", "unknown", "unknown");
        assert_eq!(
            classify_support(Some(&literal_unknown), &embedded()).tier,
            SupportTier::Unknown
        );
    }

    #[test]
    fn matching_is_case_insensitive_and_first_rule_wins() {
        let doc = r#"{"schema":1,"rules":[
            {"match":{"brand":"acme","device":"boxy"},"tier":"supported","note":"a"},
            {"match":{"brand":"ACME"},"tier":"beta","note":"b"}
        ]}"#;
        let rules = parse_support_rules(doc).unwrap();
        let boxy = props("x", "Acme", "M1", "BOXY");
        assert_eq!(
            classify_support(Some(&boxy), &rules).tier,
            SupportTier::Supported
        );
        let other = props("x", "acme", "M1", "other");
        assert_eq!(
            classify_support(Some(&other), &rules).tier,
            SupportTier::Beta
        );
    }

    #[test]
    fn model_prefix_never_panics_on_multibyte_models() {
        let doc =
            r#"{"schema":1,"rules":[{"match":{"model_prefix":"ab"},"tier":"beta","note":"n"}]}"#;
        let rules = parse_support_rules(doc).unwrap();
        let p = props("", "", "é", "");
        assert_eq!(
            classify_support(Some(&p), &rules).tier,
            SupportTier::Unknown
        );
    }

    #[test]
    fn invalid_documents_are_rejected_whole() {
        for doc in [
            "not json",
            r#"{"schema":2,"rules":[]}"#,
            r#"{"schema":0,"rules":[]}"#,
            r#"{"schema":1,"rules":[{"match":{},"tier":"beta","note":"n"}]}"#,
            r#"{"schema":1,"rules":[{"match":{"brand":"  "},"tier":"beta","note":"n"}]}"#,
            r#"{"schema":1,"rules":[{"match":{"brand":"a"},"tier":"great","note":"n"}]}"#,
            r#"{"schema":1,"rules":[{"match":{"brand":"a"},"tier":"beta","note":""}]}"#,
            r#"{"schema":1,"rules":[{"match":{"brand":"a","sdk_min":30},"tier":"beta","note":"n"}]}"#,
        ] {
            assert!(parse_support_rules(doc).is_err(), "accepted: {doc}");
        }
        let long_note = format!(
            r#"{{"schema":1,"rules":[{{"match":{{"brand":"a"}},"tier":"beta","note":"{}"}}]}}"#,
            "x".repeat(MAX_NOTE_CHARS + 1)
        );
        assert!(parse_support_rules(&long_note).is_err());
    }

    #[test]
    fn tier_wire_names_match_the_contract() {
        for (tier, name) in [
            (SupportTier::Unknown, "unknown"),
            (SupportTier::NotSupported, "not_supported"),
            (SupportTier::Beta, "beta"),
            (SupportTier::Supported, "supported"),
            (SupportTier::FullySupported, "fully_supported"),
        ] {
            assert_eq!(tier.as_str(), name);
            assert_eq!(serde_json::to_string(&tier).unwrap(), format!("\"{name}\""));
        }
    }
}

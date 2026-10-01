//! Loader — fetches app-list JSON from disk-embedded defaults (commitment #2).
//!
//! Honors architectural commitment #1: the engine does NOT load files; this
//! lives in the host layer next to the command bridge. The engine receives
//! the resulting `AppListBundle` as an input.

use std::collections::HashMap;
use std::sync::LazyLock;

use serde::Deserialize;

use crate::engine::{AppListBundle, LauncherCatalog};

/// Embedded JSON for the three default app lists. Loaded at compile time so
/// the binary works offline. Future versions will additionally check a
/// versioned URL and prefer fresher copies; that goes here.
const COMMON_JSON: &str = include_str!("../../data/app-lists/common.json");
const SHIELD_JSON: &str = include_str!("../../data/app-lists/shield.json");
const GOOGLETV_JSON: &str = include_str!("../../data/app-lists/googletv.json");
const KNOWN_NAMES_JSON: &str = include_str!("../../data/app-lists/known-names.json");
const LAUNCHERS_JSON: &str = include_str!("../../data/app-lists/launchers.json");

/// Load the bundled defaults. Returns a useful error string if any of the
/// embedded JSON files fail to parse — that's a build-time mistake worth
/// surfacing on startup.
pub fn load_embedded_app_lists() -> Result<AppListBundle, String> {
    let common =
        serde_json::from_str(COMMON_JSON).map_err(|e| format!("common.json parse error: {e}"))?;
    let shield =
        serde_json::from_str(SHIELD_JSON).map_err(|e| format!("shield.json parse error: {e}"))?;
    let googletv = serde_json::from_str(GOOGLETV_JSON)
        .map_err(|e| format!("googletv.json parse error: {e}"))?;
    Ok(AppListBundle {
        common,
        shield,
        googletv,
    })
}

/// Parse the embedded launcher catalog. Separate from the accessor so a test
/// can assert on the shipped file's contents and report a parse error rather
/// than a panic.
pub fn load_embedded_launchers() -> Result<LauncherCatalog, String> {
    serde_json::from_str(LAUNCHERS_JSON).map_err(|e| format!("launchers.json parse error: {e}"))
}

/// The launcher catalog, parsed once. Unlike the app lists it has no
/// per-device variant, so every caller shares one copy rather than threading it
/// through `AppState`. The JSON is embedded at compile time and its parse is
/// asserted by the tests below, so the failure this expects on is a build-time
/// mistake that cannot reach a shipped binary.
pub fn launchers() -> &'static LauncherCatalog {
    static CATALOG: LazyLock<LauncherCatalog> =
        LazyLock::new(|| load_embedded_launchers().expect("embedded launchers.json must parse"));
    &CATALOG
}

/// One entry of `known-names.json`: a friendly name and, optionally, a
/// one-line description of what the app is. Display only — it never feeds a
/// safety verdict, so a described package still reads Unknown until a
/// reviewed list says otherwise.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(from = "KnownNameEntry")]
pub struct KnownName {
    pub name: String,
    pub description: Option<String>,
}

/// The JSON accepts either a bare name string or `{ name, description }`, so
/// entries without a description stay one line.
#[derive(Deserialize)]
#[serde(untagged)]
enum KnownNameEntry {
    Name(String),
    Full {
        name: String,
        #[serde(default)]
        description: Option<String>,
    },
}

impl From<KnownNameEntry> for KnownName {
    fn from(entry: KnownNameEntry) -> Self {
        match entry {
            KnownNameEntry::Name(name) => KnownName {
                name,
                description: None,
            },
            KnownNameEntry::Full { name, description } => KnownName {
                name,
                description: description.filter(|d| !d.trim().is_empty()),
            },
        }
    }
}

impl From<String> for KnownName {
    fn from(name: String) -> Self {
        KnownName {
            name,
            description: None,
        }
    }
}

/// Load the curated package→friendly-name map for popular sideloads. Display
/// only, so a parse error is non-fatal — log it and carry on with an empty map
/// (rows just fall back to showing the package id).
pub fn load_known_names() -> HashMap<String, KnownName> {
    match serde_json::from_str(KNOWN_NAMES_JSON) {
        Ok(map) => map,
        Err(e) => {
            tracing::error!(error = %e, "known-names.json parse error; using empty map");
            HashMap::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::engine::{classify_with_catalog, CatalogVerdict, Safety, SafetySource};

    /// The shipped lists, not a fixture. This is the assertion that would have
    /// caught the regression where 82 of 89 curated apps reported "role and
    /// effects unknown" beside their own description.
    #[test]
    fn every_shipped_catalog_entry_gets_a_reviewed_verdict() {
        let bundle = load_embedded_app_lists().expect("embedded lists parse");
        let mut unknown = Vec::new();
        for entry in bundle
            .common
            .iter()
            .chain(bundle.shield.iter())
            .chain(bundle.googletv.iter())
        {
            let verdict = classify_with_catalog(
                &entry.package,
                Some(CatalogVerdict {
                    risk: entry.risk,
                    description: &entry.optimize_description,
                }),
            );
            if let Safety::Unknown { .. } = verdict {
                unknown.push(entry.package.clone());
            }
        }
        assert!(
            unknown.is_empty(),
            "curated apps must never report Unknown; these did: {unknown:?}"
        );
    }

    /// A curated entry must never be able to claim a package is safe when the
    /// protected list says it bricks the device.
    #[test]
    fn no_shipped_entry_overrides_the_protected_list() {
        let bundle = load_embedded_app_lists().expect("embedded lists parse");
        for entry in bundle
            .common
            .iter()
            .chain(bundle.shield.iter())
            .chain(bundle.googletv.iter())
        {
            let verdict = classify_with_catalog(
                &entry.package,
                Some(CatalogVerdict {
                    risk: entry.risk,
                    description: &entry.optimize_description,
                }),
            );
            if crate::engine::is_never_disable(&entry.package) {
                assert!(
                    matches!(verdict, Safety::NeverDisable { .. }),
                    "{} is protected but classified {verdict:?}",
                    entry.package
                );
            }
        }
    }

    /// Every curated entry needs a sentence a person can act on. An empty
    /// description leaves the verdict saying nothing useful.
    #[test]
    fn every_shipped_entry_has_a_usable_description() {
        let bundle = load_embedded_app_lists().expect("embedded lists parse");
        for entry in bundle
            .common
            .iter()
            .chain(bundle.shield.iter())
            .chain(bundle.googletv.iter())
        {
            assert!(
                entry.optimize_description.trim().len() > 10,
                "{} has no usable optimize_description: {:?}",
                entry.package,
                entry.optimize_description
            );
            assert!(
                !entry.name.trim().is_empty(),
                "{} has no display name",
                entry.package
            );
        }
    }

    /// The lookup `safety_info` relies on must find every shipped package, and
    /// the lists must stay disjoint — two entries for one package would make
    /// the answer depend on iteration order.
    #[test]
    fn find_resolves_every_package_exactly_once() {
        let bundle = load_embedded_app_lists().expect("embedded lists parse");
        let mut seen = std::collections::HashSet::new();
        for entry in bundle
            .common
            .iter()
            .chain(bundle.shield.iter())
            .chain(bundle.googletv.iter())
        {
            assert!(
                seen.insert(entry.package.clone()),
                "{} appears in more than one list",
                entry.package
            );
            assert!(
                bundle.find(&entry.package).is_some(),
                "{} is not findable",
                entry.package
            );
        }
        assert!(bundle.find("com.definitely.not.here").is_none());
        let _ = SafetySource::NoRecord;
    }

    /// Device families a catalog entry may scope itself to — the per-device
    /// lists `AppListBundle::for_device` knows about.
    const DEVICE_SCOPES: &[&str] = &["shield", "googletv"];

    /// Safe entries with no `reviewed_at`. Every entry was dated from git
    /// history when the field was introduced (#101), so this is empty; a new
    /// safe entry goes in with a date, not on this list.
    const SAFE_WITHOUT_REVIEW_DATE: &[&str] = &[];

    fn named_lists(bundle: &AppListBundle) -> [(&'static str, &[crate::engine::AppEntry]); 3] {
        [
            ("common", &bundle.common),
            ("shield", &bundle.shield),
            ("googletv", &bundle.googletv),
        ]
    }

    #[test]
    fn review_dates_parse_and_are_not_in_the_future() {
        let bundle = load_embedded_app_lists().expect("parse");
        // Dates come from local commit dates, which can run a day ahead of UTC.
        let latest = chrono::Utc::now().date_naive() + chrono::Days::new(1);
        for (list, entries) in named_lists(&bundle) {
            for e in entries {
                if let Some(raw) = &e.reviewed_at {
                    let date =
                        chrono::NaiveDate::parse_from_str(raw, "%Y-%m-%d").unwrap_or_else(|err| {
                            panic!(
                                "{} in {list}.json: bad reviewed_at {raw:?}: {err}",
                                e.package
                            )
                        });
                    assert_eq!(
                        date.format("%Y-%m-%d").to_string(),
                        *raw,
                        "{} in {list}.json: reviewed_at must be zero-padded YYYY-MM-DD",
                        e.package
                    );
                    assert!(
                        date <= latest,
                        "{} in {list}.json: reviewed_at {raw} is in the future",
                        e.package
                    );
                }
                for source in &e.sources {
                    assert!(
                        !source.trim().is_empty(),
                        "{} in {list}.json has an empty source",
                        e.package
                    );
                }
            }
        }
    }

    #[test]
    fn device_scope_uses_known_families_and_agrees_with_its_list() {
        let bundle = load_embedded_app_lists().expect("parse");
        for (list, entries) in named_lists(&bundle) {
            for e in entries {
                let mut seen = std::collections::HashSet::new();
                for scope in &e.device_scope {
                    assert!(
                        DEVICE_SCOPES.contains(&scope.as_str()),
                        "{} in {list}.json: unknown device_scope {scope:?} (known: {DEVICE_SCOPES:?})",
                        e.package
                    );
                    assert!(
                        seen.insert(scope.as_str()),
                        "{} in {list}.json repeats device_scope {scope:?}",
                        e.package
                    );
                }
                // A per-device list only reaches its own family, so a scope
                // naming any other one would be a claim the app never acts on.
                if list != "common" {
                    assert!(
                        e.device_scope.iter().all(|s| s == list),
                        "{} in {list}.json is scoped to {:?}",
                        e.package,
                        e.device_scope
                    );
                }
            }
        }
    }

    /// A Safe verdict is the strongest claim the catalog makes, so it must say
    /// when it was made. Gaps are listed here rather than hidden: the list has
    /// to match exactly, so a newly undated entry fails and so does a gap that
    /// got fixed without being removed from the allowlist.
    #[test]
    fn safe_entries_carry_a_review_date_or_are_listed_as_gaps() {
        use crate::engine::types::RiskTier;
        let bundle = load_embedded_app_lists().expect("parse");
        let mut gaps: Vec<&str> = named_lists(&bundle)
            .into_iter()
            .flat_map(|(_, entries)| entries.iter())
            .filter(|e| e.risk == RiskTier::Safe && e.reviewed_at.is_none())
            .map(|e| e.package.as_str())
            .collect();
        gaps.sort_unstable();
        let mut allowed = SAFE_WITHOUT_REVIEW_DATE.to_vec();
        allowed.sort_unstable();
        assert_eq!(
            gaps, allowed,
            "safe entries without reviewed_at must match SAFE_WITHOUT_REVIEW_DATE"
        );
    }
    use pretty_assertions::assert_eq;

    #[test]
    fn embedded_json_loads_and_parses() {
        let bundle = load_embedded_app_lists().expect("bundled JSON must parse");
        assert!(bundle.common.len() >= 10, "common list looks thin");
        assert!(bundle.shield.len() >= 5, "shield list looks thin");
        assert!(!bundle.googletv.is_empty(), "googletv list empty");
    }

    #[test]
    fn embedded_data_includes_known_defunct_apps() {
        let bundle = load_embedded_app_lists().expect("parse");
        for pkg in [
            "com.Funimation.FunimationNow.androidtv",
            "com.google.stadia.android",
            "com.quibi.qlient",
            "com.hbo.hbonow",
        ] {
            assert!(
                bundle.common.iter().any(|e| e.package == pkg),
                "missing defunct app: {pkg}"
            );
        }
    }

    /// The shipped launcher catalog, not a fixture — the engine takes the
    /// catalog as an argument now, so this file is the only place the real
    /// entries are checked.
    #[test]
    fn embedded_launchers_parse_with_every_shipped_entry() {
        let cat = load_embedded_launchers().expect("launchers.json must parse");
        assert_eq!(cat.custom.len(), 7, "custom launcher count");
        assert_eq!(cat.stock.len(), 4, "stock launcher count");

        // Critical correctness: the Dispatch package name change from v1's
        // launcher selection fix.
        let dispatch = cat
            .custom
            .iter()
            .find(|e| e.name == "Dispatch Launcher")
            .expect("Dispatch entry");
        assert_eq!(dispatch.package, "com.spauldhaliwal.dispatch");
        assert!(cat
            .custom
            .iter()
            .any(|e| e.package == "com.spocky.projengmenu"));
        // GitHub #121.
        assert!(
            cat.custom.iter().any(|e| e.package == "com.klevico.monet"),
            "Monet Launcher is missing"
        );
        assert!(cat
            .home_handler_name("com.google.android.tungsten.setupwraith")
            .is_some());
    }

    /// A custom launcher with no source is a row whose "Get" link cannot be
    /// rendered — the only way to install one the device's Play Store lacks.
    #[test]
    fn every_custom_launcher_has_a_source_url_and_no_package_repeats() {
        let cat = load_embedded_launchers().expect("parse");
        let mut seen = std::collections::HashSet::new();
        for entry in cat.custom.iter().chain(cat.stock.iter()) {
            assert!(
                !entry.name.trim().is_empty(),
                "{} has no display name",
                entry.package
            );
            assert!(
                seen.insert(entry.package.as_str()),
                "duplicate launcher package {:?}",
                entry.package
            );
        }
        for entry in &cat.custom {
            let url = entry
                .source_url
                .as_deref()
                .unwrap_or_else(|| panic!("{} has no source_url", entry.package));
            assert!(
                url.starts_with("https://"),
                "{} source_url must be https: {url}",
                entry.package
            );
        }
    }

    #[test]
    fn known_names_map_parses_and_has_expected_entries() {
        let names = load_known_names();
        assert!(!names.is_empty(), "known-names map should not be empty");
        assert_eq!(
            names.get("ca.devmesh.overseerrtv").map(|k| k.name.as_str()),
            Some("Overseerr (TV)"),
            "a known non-catalog sideload must map to its friendly name"
        );
    }

    #[test]
    fn known_names_accept_a_bare_name_or_a_described_entry() {
        let parsed: HashMap<String, KnownName> = serde_json::from_str(
            r#"{
                "a.bare": "Bare",
                "a.full": { "name": "Full", "description": "Does a thing" },
                "a.blank": { "name": "Blank", "description": "  " }
            }"#,
        )
        .expect("both shapes parse");
        assert_eq!(parsed["a.bare"].description, None);
        assert_eq!(parsed["a.full"].name, "Full");
        assert_eq!(
            parsed["a.full"].description.as_deref(),
            Some("Does a thing")
        );
        assert_eq!(parsed["a.blank"].description, None);
    }

    #[test]
    fn shipped_known_names_are_display_only() {
        // Descriptions say what an app is, never whether it is safe to remove.
        for (pkg, known) in load_known_names() {
            assert!(!known.name.trim().is_empty(), "{pkg} has an empty name");
            if let Some(desc) = &known.description {
                let lower = desc.to_lowercase();
                for claim in ["safe to", "harmless", "bloat", "can be removed", "remove"] {
                    assert!(
                        !lower.contains(claim),
                        "{pkg}'s description makes a removal claim ({claim:?}): {desc}"
                    );
                }
            }
        }
    }

    #[test]
    fn known_names_do_not_duplicate_catalog_entries() {
        // Catalog members never reach "Everything else", so a known-name for one
        // is dead data — keep the two sets disjoint.
        let names = load_known_names();
        let bundle = load_embedded_app_lists().expect("parse");
        let catalog: std::collections::HashSet<&str> = bundle
            .common
            .iter()
            .chain(bundle.shield.iter())
            .chain(bundle.googletv.iter())
            .map(|e| e.package.as_str())
            .collect();
        for pkg in names.keys() {
            assert!(
                !catalog.contains(pkg.as_str()),
                "{pkg} is in both the catalog and known-names; drop it from one"
            );
        }
    }

    #[test]
    fn non_reinstallable_uninstall_entries_are_gated_to_disable() {
        // The safety guarantee: any catalog app whose method is uninstall but
        // that isn't reinstallable must resolve to disable via the gate, so the
        // wizard can never recommend an unrecoverable removal. (Preinstalled
        // bloat like the Walmart app lands here and is safely disabled instead.)
        use crate::engine::types::ActionMethod;
        let bundle = load_embedded_app_lists().expect("parse");
        for list in [&bundle.common, &bundle.shield, &bundle.googletv] {
            for e in list {
                if !e.reinstallable() {
                    assert_eq!(
                        e.safe_method(),
                        ActionMethod::Disable,
                        "{} is not reinstallable, so safe_method must be disable",
                        e.package
                    );
                }
            }
        }
    }

    #[test]
    fn review_apps_are_never_auto_selected() {
        // The "remove if unused" tier is the user's call — it must never be
        // pre-checked by the wizard.
        let bundle = load_embedded_app_lists().expect("parse");
        for list in [&bundle.common, &bundle.shield, &bundle.googletv] {
            for e in list {
                if e.review {
                    assert!(
                        !e.default_optimize,
                        "{} is both review and default_optimize — pick one",
                        e.package
                    );
                }
            }
        }
    }

    #[test]
    fn no_duplicate_packages_within_any_bundled_list() {
        // A repeated package blanks the App List + Optimize tables (Svelte throws
        // on a duplicate `{#each}` key). Catch it here instead of in the field.
        let bundle = load_embedded_app_lists().expect("parse");
        for (name, list) in [
            ("common", &bundle.common),
            ("shield", &bundle.shield),
            ("googletv", &bundle.googletv),
        ] {
            let mut seen = std::collections::HashSet::new();
            for e in list {
                assert!(
                    seen.insert(e.package.as_str()),
                    "duplicate package {:?} in {name}.json",
                    e.package
                );
            }
        }
    }

    #[test]
    fn channel_provider_entry_has_high_risk() {
        let bundle = load_embedded_app_lists().expect("parse");
        let entry = bundle
            .common
            .iter()
            .find(|e| e.package == "com.android.providers.tv")
            .expect("providers.tv entry");
        assert_eq!(
            entry.risk,
            crate::engine::types::RiskTier::High,
            "providers.tv must be flagged High Risk so users see the cost"
        );
        assert!(
            !entry.default_optimize,
            "providers.tv must not default-disable in Optimize mode"
        );
    }
}

//! Loader — fetches app-list JSON from disk-embedded defaults (commitment #2).
//!
//! Honors architectural commitment #1: the engine does NOT load files; this
//! lives in the host layer next to the command bridge. The engine receives
//! the resulting `AppListBundle` as an input.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock};

use serde::Deserialize;

use crate::engine::{parse_support_rules, AppListBundle, LauncherCatalog, SupportRules};

/// Embedded JSON for the three default app lists. Loaded at compile time so
/// the binary works offline.
const COMMON_JSON: &str = include_str!("../../data/app-lists/common.json");
const SHIELD_JSON: &str = include_str!("../../data/app-lists/shield.json");
const GOOGLETV_JSON: &str = include_str!("../../data/app-lists/googletv.json");
const KNOWN_NAMES_JSON: &str = include_str!("../../data/app-lists/known-names.json");
const LAUNCHERS_JSON: &str = include_str!("../../data/app-lists/launchers.json");
const DEVICE_SUPPORT_JSON: &str = include_str!("../../data/device-support.json");

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

/// The device support rules this build shipped with. A fetched copy can
/// replace them at runtime (`crate::telemetry::refresh_support_rules`); these
/// are what every failure falls back to. The parse is asserted by the tests
/// below, so the expect can only trip on a build-time mistake.
pub fn embedded_support_rules() -> Arc<SupportRules> {
    static RULES: LazyLock<Arc<SupportRules>> = LazyLock::new(|| {
        Arc::new(
            parse_support_rules(DEVICE_SUPPORT_JSON)
                .expect("embedded device-support.json must parse"),
        )
    });
    RULES.clone()
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

    #[test]
    fn embedded_support_rules_parse_and_seed_the_three_families() {
        let rules = embedded_support_rules();
        assert_eq!(rules.schema, crate::engine::support::SUPPORT_SCHEMA);
        let tiers: Vec<_> = rules.rules.iter().map(|r| r.tier.as_str()).collect();
        for want in ["fully_supported", "beta", "not_supported"] {
            assert!(tiers.contains(&want), "missing {want} in {tiers:?}");
        }
    }

    #[test]
    fn tvrecommendations_is_not_a_default_removal_and_explains_remote_risk() {
        let bundle = load_embedded_app_lists().expect("embedded lists parse");
        let entry = bundle
            .shield
            .iter()
            .find(|e| e.package == "com.google.android.tvrecommendations")
            .expect("SHIELD recommendations entry exists");
        assert!(
            !entry.default_optimize,
            "phone-remote dependency must not be selected automatically"
        );
        assert!(entry.default_restore);
        let verdict = classify_with_catalog(
            &entry.package,
            Some(CatalogVerdict {
                risk: entry.risk,
                description: &entry.optimize_description,
            }),
        );
        match verdict {
            Safety::Caution { reason, source } => {
                assert_eq!(source, SafetySource::ReviewedCatalog);
                for evidence in [
                    "phone app",
                    "reboot",
                    "custom launcher",
                    "darcy",
                    "foster_e",
                    "Android 11",
                    "unverified",
                ] {
                    assert!(
                        reason.contains(evidence),
                        "missing consequence or evidence limit: {evidence}"
                    );
                }
            }
            other => panic!("expected Caution, got {other:?}"),
        }
        assert!(entry
            .sources
            .iter()
            .any(|s| s.contains("30626a8e4714f86bf837384b109b5c73ee736917")));
        assert!(!entry.optimize_description.contains("No effect"));
    }

    /// The shipped lists, not a fixture: no curated app may report "role and
    /// effects unknown" beside its own description.
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

    /// Safe entries with no `reviewed_at` (#101). Empty: a new safe entry goes
    /// in with a date, not on this list.
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
                // The field records applicability; it does not decide it.
                // `for_device` picks entries by list, so a scope must name the
                // list the entry is in. A common entry that only applies to one
                // family belongs in that family's list instead.
                let ok = if list == "common" {
                    e.device_scope.is_empty()
                } else {
                    e.device_scope == [list]
                };
                assert!(
                    ok,
                    "{} in {list}.json is scoped to {:?}; move it to that list instead",
                    e.package, e.device_scope
                );
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
    /// catalog as an argument, so this file is the only place the real
    /// entries are checked.
    #[test]
    fn embedded_launchers_parse_with_every_shipped_entry() {
        let cat = load_embedded_launchers().expect("launchers.json must parse");
        assert_eq!(cat.custom.len(), 7, "custom launcher count");
        assert_eq!(cat.stock.len(), 4, "stock launcher count");

        // Dispatch's real package id; a wrong one breaks launcher selection.
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
        assert_eq!(
            cat.home_handler_name("com.google.android.tungsten.setupwraith"),
            Some("Google TV setup helper — not a launcher")
        );
        assert!(cat.is_transient_home_holder("com.google.android.tungsten.setupwraith"));
        // #122: only Google TV Home pairs with Setup Wraith; Shield stock pairs with nothing.
        assert_eq!(
            cat.disable_with_for("com.google.android.apps.tv.launcherx"),
            ["com.google.android.tungsten.setupwraith".to_string()]
        );
        for entry in cat.stock.iter().filter(|e| !e.disable_with.is_empty()) {
            assert_eq!(entry.package, "com.google.android.apps.tv.launcherx");
            assert!(entry
                .disable_with
                .iter()
                .all(|p| cat.is_transient_home_holder(p)));
        }
    }

    #[test]
    fn uncurated_shared_package_remains_a_generic_home_handler() {
        let catalog = load_embedded_launchers().expect("parse");
        let package = "com.gathin.tvhome".to_string();
        assert!(!catalog.contains(&package));
        let missing = crate::engine::launcher_rows(&catalog, &[], &[], &[], &[]);
        assert!(!missing.iter().any(|row| row.entry.package == package));
        for disabled in [false, true] {
            let installed = vec![package.clone()];
            let disabled_packages = if disabled { installed.clone() } else { vec![] };
            let handlers = if disabled {
                vec![]
            } else {
                vec![package.clone(), package.clone()]
            };
            let rows = crate::engine::launcher_rows(
                &catalog,
                &installed,
                &disabled_packages,
                &handlers,
                &installed,
            );
            let matching: Vec<_> = rows
                .iter()
                .filter(|row| row.entry.package == package)
                .collect();
            assert_eq!(matching.len(), 1);
            let row = matching[0];
            assert_eq!(row.entry.name, package);
            assert_eq!(row.entry.source_url, None);
            assert!(row.installed && row.other);
            assert!(!row.stock && !row.setup_helper);
            assert_eq!(row.enabled, !disabled);
        }
    }

    /// A custom launcher with no source is a row whose "Get" link cannot be
    /// rendered — the only way to install one the device's Play Store lacks.
    #[test]
    fn every_custom_launcher_has_a_source_url_and_no_package_repeats() {
        let cat = load_embedded_launchers().expect("parse");
        let projectivy = cat
            .custom
            .iter()
            .find(|entry| entry.package == "com.spocky.projengmenu")
            .expect("Projectivy");
        assert_eq!(
            projectivy.source_url.as_deref(),
            Some("https://github.com/spocky/miproja1/releases")
        );
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
    fn reported_streaming_names_preserve_catalog_safety_fields() {
        let changes: Vec<serde_json::Value> = serde_json::from_str(include_str!(
            "../../../../tests/fixtures/reported-package-renames.json"
        ))
        .unwrap();
        let lists = load_embedded_app_lists().unwrap();
        assert_eq!(changes.len(), 2);
        for change in changes {
            let package = change["package"].as_str().unwrap();
            let entry = lists.find(package).unwrap();
            assert_eq!(entry.name, change["name"].as_str().unwrap());
            let actual = serde_json::to_value(entry).unwrap();
            for (key, expected) in change["preserved_fields"].as_object().unwrap() {
                assert_eq!(&actual[key], expected, "{package}: {key}");
            }
        }
    }

    #[test]
    fn reported_packages_have_reviewed_safety_and_review_without_default_removal() {
        let evidence: Vec<serde_json::Value> = serde_json::from_str(include_str!(
            "../../../../tests/fixtures/reported-package-names.json"
        ))
        .unwrap();
        let names = load_known_names();
        let lists = load_embedded_app_lists().unwrap();
        let mut unique = std::collections::HashSet::new();
        let mut safe_count = 0;
        let mut medium_count = 0;
        assert_eq!(evidence.len(), 15);
        for item in evidence {
            let package = item["package"].as_str().unwrap();
            assert!(unique.insert(package.to_owned()));
            let expected = item["expected_safety"].as_str().unwrap();
            if expected == "safe" || expected == "medium" {
                let entry = lists.find(package).unwrap();
                assert_eq!(entry.name, item["name"].as_str().unwrap());
                assert!(!names.contains_key(package));
                // Apps people install by choice: remove only if unused, never by default.
                assert!(!entry.default_optimize && !entry.default_restore && entry.review);
                assert_eq!(entry.method, crate::engine::ActionMethod::Disable);
                assert!(entry.reviewed_at.is_some());
                assert!(!entry.sources.is_empty());
                assert!(entry.optimize_description.starts_with("Disabling stops"));
                assert!(entry.restore_description.contains("if still installed"));
                let verdict = classify_with_catalog(
                    package,
                    Some(CatalogVerdict {
                        risk: entry.risk,
                        description: &entry.optimize_description,
                    }),
                );
                if expected == "safe" {
                    safe_count += 1;
                    assert!(matches!(
                        verdict,
                        Safety::Safe {
                            source: SafetySource::ReviewedCatalog,
                            ..
                        }
                    ));
                } else {
                    medium_count += 1;
                    assert_eq!(entry.risk, crate::engine::types::RiskTier::Medium);
                    assert!(matches!(
                        verdict,
                        Safety::Caution {
                            source: SafetySource::ReviewedCatalog,
                            ..
                        }
                    ));
                }
            } else {
                assert!(lists.find(package).is_none());
                assert_eq!(names[package].name, item["name"].as_str().unwrap());
                let verdict = crate::engine::classify_with_sources(
                    package,
                    None,
                    item["expected_safety"] == "caution",
                );
                assert!(matches!(
                    (item["expected_safety"].as_str(), verdict),
                    (Some("unknown"), Safety::Unknown { .. })
                        | (
                            Some("caution"),
                            Safety::Caution {
                                source: SafetySource::LauncherList,
                                ..
                            }
                        )
                ));
            }
        }
        assert_eq!(safe_count, 13);
        assert_eq!(medium_count, 1);
    }

    #[test]
    fn reviewed_source_leads_expand_plans_without_safe_or_default_actions() {
        use crate::engine::{
            compute_plan, ActionMethod, DeviceType, OptimizeAction, OptimizeInputs, OptimizeMode,
            RiskTier, SkipReason,
        };
        let reviewed: Vec<serde_json::Value> = serde_json::from_str(include_str!(
            "../../../../tests/fixtures/reviewed-optimize-packages.json"
        ))
        .unwrap();
        let lists = load_embedded_app_lists().unwrap();
        let packages: std::collections::HashSet<String> = reviewed
            .iter()
            .map(|p| p["package"].as_str().unwrap().to_owned())
            .collect();
        let empty = std::collections::HashSet::new();
        let memory = HashMap::new();
        for family in [
            DeviceType::Shield,
            DeviceType::GoogleTv,
            DeviceType::Unknown,
        ] {
            let apps = lists.for_device(family);
            for (mode, installed, disabled, expected) in [
                (
                    OptimizeMode::Optimize,
                    &packages,
                    &empty,
                    OptimizeAction::Disable,
                ),
                (
                    OptimizeMode::Optimize,
                    &packages,
                    &packages,
                    OptimizeAction::Skip {
                        reason: SkipReason::AlreadyDisabled,
                    },
                ),
                (
                    OptimizeMode::Optimize,
                    &empty,
                    &empty,
                    OptimizeAction::Skip {
                        reason: SkipReason::NotInstalled,
                    },
                ),
                (
                    OptimizeMode::Restore,
                    &packages,
                    &packages,
                    OptimizeAction::Enable,
                ),
                (
                    OptimizeMode::Restore,
                    &packages,
                    &empty,
                    OptimizeAction::Skip {
                        reason: SkipReason::AlreadyEnabled,
                    },
                ),
                (
                    OptimizeMode::Restore,
                    &empty,
                    &empty,
                    OptimizeAction::Skip {
                        reason: SkipReason::NotInstalled,
                    },
                ),
            ] {
                let plan = compute_plan(
                    &apps,
                    mode,
                    &OptimizeInputs {
                        installed_packages: installed,
                        disabled_packages: disabled,
                        memory_map: &memory,
                        user_installed_packages: None,
                    },
                );
                for record in &reviewed {
                    let package = record["package"].as_str().unwrap();
                    let item = plan
                        .items
                        .iter()
                        .find(|i| i.entry.package == package)
                        .unwrap();
                    assert_eq!(item.action, expected, "{package} {family:?} {mode:?}");
                    let entry = &item.entry;
                    assert_eq!(entry.name, record["name"].as_str().unwrap());
                    assert_eq!(entry.method, ActionMethod::Disable);
                    assert_eq!(entry.risk, RiskTier::Medium);
                    assert!(entry.review && !entry.default_optimize && !entry.default_restore);
                    assert!(entry.reviewed_at.is_some() && entry.sources.len() >= 3);
                    assert!(matches!(
                        classify_with_catalog(
                            package,
                            Some(CatalogVerdict {
                                risk: entry.risk,
                                description: &entry.optimize_description
                            })
                        ),
                        Safety::Caution {
                            source: SafetySource::ReviewedCatalog,
                            ..
                        }
                    ));
                }
            }
        }
        let imported: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../catalogs/cleanyourtv/catalog.json"
        ))
        .unwrap();
        let held: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../catalogs/cleanyourtv/research-ledger.json"
        ))
        .unwrap();
        assert_eq!(imported["actionable"], false);
        for record in held["packages"].as_array().unwrap() {
            let package = record["package"].as_str().unwrap();
            assert!(
                lists.find(package).is_none(),
                "unreviewed lead {package} entered the action catalog"
            );
        }
        let progress: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../tools/catalog-review/progress.json"
        ))
        .unwrap();
        for (package, record) in progress["packages"].as_object().unwrap() {
            if record["status"] == "held" {
                assert!(
                    lists.find(package).is_none(),
                    "held research lead {package} entered the action catalog"
                );
            }
        }
    }

    #[test]
    fn reported_content_clients_have_exact_evidence_and_keep_defaults() {
        use crate::engine::{ActionMethod, DeviceType, RiskTier};
        let fixture: Vec<serde_json::Value> = serde_json::from_str(include_str!(
            "../../../../tests/fixtures/reviewed-optimize-packages.json"
        ))
        .unwrap();
        let reports: Vec<_> = fixture
            .iter()
            .filter(|e| {
                e["issue"]
                    .as_u64()
                    .is_some_and(|n| (301..=304).contains(&n))
            })
            .collect();
        assert_eq!(reports.len(), 4);
        let lists = load_embedded_app_lists().unwrap();
        for report in reports {
            let package = report["package"].as_str().unwrap();
            let entry = lists.find(package).unwrap();
            assert_eq!(entry.name, report["name"].as_str().unwrap());
            assert_eq!(entry.risk, RiskTier::Medium);
            assert_eq!(entry.method, ActionMethod::Disable);
            assert!(entry.review && !entry.default_optimize && !entry.default_restore);
            assert_eq!(entry.reviewed_at.as_deref(), Some("2026-11-05"));
            assert_eq!(entry.sources[0], report["source_url"].as_str().unwrap());
            assert!(entry.sources[0].ends_with(&format!("id={package}")));
            assert!(entry.sources.iter().any(|s| s.contains("Device/firmware")));
            assert!(entry.restore_description.contains("if still installed"));
            for family in [
                DeviceType::Shield,
                DeviceType::GoogleTv,
                DeviceType::Unknown,
            ] {
                assert_eq!(
                    lists
                        .for_device(family)
                        .iter()
                        .filter(|e| e.package == package)
                        .count(),
                    1
                );
            }
        }
        assert!(lists.find("com.amcplus.amcfiretv").is_some());
        assert!(!load_known_names().contains_key("com.amazon.music.tv"));
    }

    #[test]
    fn followup_apps_are_optional_and_directv_stays_unreviewed() {
        use crate::engine::{ActionMethod, RiskTier};
        let lists = load_embedded_app_lists().unwrap();
        let fixture: Vec<serde_json::Value> = serde_json::from_str(include_str!(
            "../../../../tests/fixtures/reviewed-optimize-packages.json"
        ))
        .unwrap();
        let reports: Vec<_> = fixture
            .iter()
            .filter(|e| {
                e["issue"]
                    .as_u64()
                    .is_some_and(|n| [305, 306, 308].contains(&n))
            })
            .collect();
        assert_eq!(reports.len(), 3);
        for report in reports {
            let package = report["package"].as_str().unwrap();
            let entry = lists.find(package).unwrap();
            assert_eq!(entry.name, report["name"].as_str().unwrap());
            assert_eq!(entry.risk, RiskTier::Medium);
            assert_eq!(entry.method, ActionMethod::Disable);
            assert!(entry.review && !entry.default_optimize && !entry.default_restore);
            assert_eq!(entry.reviewed_at.as_deref(), Some("2026-11-06"));
            assert_eq!(entry.sources[0], report["source_url"].as_str().unwrap());
            assert!(entry.sources[0].ends_with(&format!("id={package}")));
            assert!(entry.sources.iter().any(|s| s.contains("Device/firmware")));
            assert!(entry.restore_description.contains("if still installed"));
        }
        assert!(lists
            .find("com.aetnd.svod.historyvault.androidtv")
            .is_some());
        assert!(lists.find("com.britbox.us").is_some());
        assert!(lists.find("com.att.tv").is_none());
        assert!(matches!(
            classify_with_catalog("com.att.tv", None),
            Safety::Unknown { .. }
        ));
        let analiti = lists.find("com.analiti.fastest.android").unwrap();
        assert!(analiti
            .optimize_description
            .contains("background connection tracking"));
        assert!(analiti
            .optimize_description
            .contains("embedded test servers"));
    }

    #[test]
    fn latest_reports_keep_exact_variants_and_never_default_to_removal() {
        use crate::engine::{ActionMethod, RiskTier};
        let lists = load_embedded_app_lists().unwrap();
        let fixture: Vec<serde_json::Value> = serde_json::from_str(include_str!(
            "../../../../tests/fixtures/reviewed-optimize-packages.json"
        ))
        .unwrap();
        let reports: Vec<_> = fixture
            .iter()
            .filter(|e| {
                e["issue"]
                    .as_u64()
                    .is_some_and(|n| (310..=312).contains(&n))
            })
            .collect();
        assert_eq!(reports.len(), 3);
        for report in reports {
            let package = report["package"].as_str().unwrap();
            let entry = lists.find(package).unwrap();
            assert_eq!(entry.name, report["name"].as_str().unwrap());
            assert_eq!(entry.risk, RiskTier::Medium);
            assert_eq!(entry.method, ActionMethod::Disable);
            assert!(entry.review && !entry.default_optimize && !entry.default_restore);
            assert_eq!(entry.reviewed_at.as_deref(), Some("2026-11-06"));
            assert_eq!(entry.sources[0], report["source_url"].as_str().unwrap());
            assert!(entry.sources[0].ends_with(&format!("id={package}")));
            assert!(entry.sources.iter().any(|s| s.contains("Device/firmware")));
            assert!(entry.restore_description.contains("if still installed"));
        }
        let tidal = lists.find("com.aspiro.tidal").unwrap();
        assert_eq!(tidal.risk, RiskTier::Medium);
        assert_eq!(tidal.method, ActionMethod::Disable);
        assert!(tidal.review && !tidal.default_optimize && !tidal.default_restore);
        assert_eq!(tidal.reviewed_at.as_deref(), Some("2026-11-05"));
        assert_eq!(
            lists
                .common
                .iter()
                .filter(|e| e.package == "com.aspiro.tidal")
                .count(),
            1
        );
        assert_eq!(
            lists.find("com.starz.starzplay.android").unwrap().name,
            "LIONSGATE+"
        );
        assert_eq!(lists.find("com.cbs.ott").unwrap().name, "Paramount+");
        assert!(lists.find("com.cbsnews.ott").is_some());
        assert!(lists.find("com.att.tv").is_none());
        assert!(matches!(
            classify_with_catalog("com.att.tv", None),
            Safety::Unknown { .. }
        ));
    }

    #[test]
    fn radio_and_discovery_reports_preserve_variants_and_keep_defaults() {
        use crate::engine::{ActionMethod, RiskTier};
        let lists = load_embedded_app_lists().unwrap();
        let fixture: Vec<serde_json::Value> = serde_json::from_str(include_str!(
            "../../../../tests/fixtures/reviewed-optimize-packages.json"
        ))
        .unwrap();
        let reports: Vec<_> = fixture
            .iter()
            .filter(|e| {
                e["issue"]
                    .as_u64()
                    .is_some_and(|n| (314..=317).contains(&n))
            })
            .collect();
        assert_eq!(reports.len(), 4);
        for report in reports {
            let package = report["package"].as_str().unwrap();
            let entry = lists.find(package).unwrap();
            assert_eq!(entry.name, report["name"].as_str().unwrap());
            assert_eq!(entry.risk, RiskTier::Medium);
            assert_eq!(entry.method, ActionMethod::Disable);
            assert!(entry.review && !entry.default_optimize && !entry.default_restore);
            assert_eq!(entry.reviewed_at.as_deref(), Some("2026-11-06"));
            assert_eq!(entry.sources[0], report["source_url"].as_str().unwrap());
            assert!(entry.sources[0].ends_with(&format!("id={package}")));
            assert!(entry.sources.iter().any(|s| s.contains("Device/firmware")));
            assert!(entry.restore_description.contains("if still installed"));
        }
        let news = lists.find("com.cbsnews.ott").unwrap();
        assert_eq!(news.risk, RiskTier::Medium);
        assert_eq!(news.method, ActionMethod::Disable);
        assert!(news.review && !news.default_optimize && !news.default_restore);
        assert_eq!(
            lists
                .common
                .iter()
                .filter(|e| e.package == "com.cbsnews.ott")
                .count(),
            1
        );
        assert!(lists
            .find("com.discovery.discoveryplus.androidtv")
            .is_some());
        assert!(lists.find("com.discovery.discoveryplus.mobile").is_some());
        assert!(lists.find("com.att.tv").is_none());
        assert!(matches!(
            classify_with_catalog("com.att.tv", None),
            Safety::Unknown { .. }
        ));
    }

    #[test]
    fn media_reports_preserve_legacy_identities_and_system_holds() {
        use crate::engine::{ActionMethod, RiskTier};
        let lists = load_embedded_app_lists().unwrap();
        let fixture: Vec<serde_json::Value> = serde_json::from_str(include_str!(
            "../../../../tests/fixtures/reviewed-optimize-packages.json"
        ))
        .unwrap();
        let reports: Vec<_> = fixture
            .iter()
            .filter(|e| {
                e["issue"]
                    .as_u64()
                    .is_some_and(|n| (319..=322).contains(&n))
            })
            .collect();
        assert_eq!(reports.len(), 3);
        for report in reports {
            let package = report["package"].as_str().unwrap();
            let entry = lists.find(package).unwrap();
            assert_eq!(entry.name, report["name"].as_str().unwrap());
            assert_eq!(entry.risk, RiskTier::Medium);
            assert_eq!(entry.method, ActionMethod::Disable);
            assert!(entry.review && !entry.default_optimize && !entry.default_restore);
            assert_eq!(entry.reviewed_at.as_deref(), Some("2026-11-06"));
            assert_eq!(entry.sources[0], report["source_url"].as_str().unwrap());
            assert!(entry.sources[0].ends_with(&format!("id={package}")));
            assert!(entry.sources.iter().any(|s| s.contains("Device/firmware")));
            assert!(entry.restore_description.contains("if still installed"));
        }
        assert!(lists.find("com.epix.epixnow.google").unwrap().defunct);
        for held in [
            "com.disney.datg.videoplatforms.android.watchdc",
            "com.google.android.tv",
        ] {
            assert!(lists.find(held).is_none());
            assert!(matches!(
                classify_with_catalog(held, None),
                Safety::Unknown { .. }
            ));
        }
        assert!(lists.find("com.att.tv").is_none());
        assert!(matches!(
            classify_with_catalog("com.att.tv", None),
            Safety::Unknown { .. }
        ));
        // Live Channels keeps its descriptive name, and only that.
        assert!(load_known_names().contains_key("com.google.android.tv"));
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

    /// Components that ship with the TV and that nobody installs, so they are
    /// neither default bloat nor "remove only if unused". Each one is listed in
    /// docs/OPTIMIZE-CATALOG-REVIEW-2026-11-08.md with its reason.
    const PREINSTALLED_NOT_REVIEW: &[&str] = &[
        "com.android.printspooler",
        "com.android.gallery3d",
        "com.android.dreams.basic",
        "com.google.android.tv.remote.service",
        "com.google.android.play.games",
        "com.google.android.backdrop",
        "com.nvidia.osc",
        "com.nvidia.shieldtech.hooks",
        "com.nvidia.nvgamecast",
        "com.google.android.tungsten.overscan",
        "com.droidlogic.launcher.provider",
    ];

    #[test]
    fn chosen_apps_outside_the_default_bloat_are_review() {
        // A Safe, Medium or Advanced entry that is not default bloat is an app
        // someone installed or uses by choice (Surfshark is Medium, Plex Media
        // Server is Advanced), so it must read "remove only if unused". High is
        // for components the TV needs, which are never a person's choice.
        use crate::engine::types::RiskTier;
        let bundle = load_embedded_app_lists().expect("parse");
        for (list, entries) in named_lists(&bundle) {
            for e in entries {
                if matches!(
                    e.risk,
                    RiskTier::Safe | RiskTier::Medium | RiskTier::Advanced
                ) && !e.default_optimize
                    && !PREINSTALLED_NOT_REVIEW.contains(&e.package.as_str())
                {
                    assert!(
                        e.review,
                        "{} in {list}.json is {:?}, not default bloat and not a listed preinstalled component, so it needs review: true",
                        e.package, e.risk
                    );
                }
            }
        }
    }

    #[test]
    fn preinstalled_exceptions_exist_and_are_not_review() {
        // The exception list must not outlive its entries, and an exception that
        // has since become review or default bloat no longer needs to be one.
        let bundle = load_embedded_app_lists().expect("parse");
        let all: Vec<&crate::engine::AppEntry> = named_lists(&bundle)
            .into_iter()
            .flat_map(|(_, entries)| entries.iter())
            .collect();
        for pkg in PREINSTALLED_NOT_REVIEW {
            let matches: Vec<_> = all.iter().filter(|e| e.package == *pkg).collect();
            assert!(
                !matches.is_empty(),
                "{pkg} is listed in PREINSTALLED_NOT_REVIEW but is in no app list; remove it"
            );
            for e in matches {
                assert!(
                    !e.review,
                    "{pkg} is review: true, so it is not a preinstalled exception; remove it from PREINSTALLED_NOT_REVIEW"
                );
                assert!(
                    !e.default_optimize,
                    "{pkg} is default bloat, which the rule already skips; remove it from PREINSTALLED_NOT_REVIEW"
                );
                assert!(
                    !e.defunct,
                    "{pkg} is defunct, which is not a preinstalled component; remove it from PREINSTALLED_NOT_REVIEW"
                );
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

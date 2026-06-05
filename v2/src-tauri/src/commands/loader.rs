//! Loader — fetches app-list JSON from disk-embedded defaults (commitment #2).
//!
//! Honors architectural commitment #1: the engine does NOT load files; this
//! lives in the host layer next to the command bridge. The engine receives
//! the resulting `AppListBundle` as an input.

use std::collections::HashMap;

use crate::engine::AppListBundle;

/// Embedded JSON for the three default app lists. Loaded at compile time so
/// the binary works offline. Future versions will additionally check a
/// versioned URL and prefer fresher copies; that goes here.
const COMMON_JSON: &str = include_str!("../../../data/app-lists/common.json");
const SHIELD_JSON: &str = include_str!("../../../data/app-lists/shield.json");
const GOOGLETV_JSON: &str = include_str!("../../../data/app-lists/googletv.json");
const KNOWN_NAMES_JSON: &str = include_str!("../../../data/app-lists/known-names.json");

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

/// Load the curated package→friendly-name map for popular sideloads. Display
/// only, so a parse error is non-fatal — log it and carry on with an empty map
/// (rows just fall back to showing the package id).
pub fn load_known_names() -> HashMap<String, String> {
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

    #[test]
    fn known_names_map_parses_and_has_expected_entries() {
        let names = load_known_names();
        assert!(!names.is_empty(), "known-names map should not be empty");
        assert_eq!(
            names.get("ca.devmesh.overseerrtv").map(String::as_str),
            Some("Overseerr (TV)"),
            "a known non-catalog sideload must map to its friendly name"
        );
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

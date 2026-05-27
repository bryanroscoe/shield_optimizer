//! Loader — fetches app-list JSON from disk-embedded defaults (commitment #2).
//!
//! Honors architectural commitment #1: the engine does NOT load files; this
//! lives in the host layer next to the command bridge. The engine receives
//! the resulting `AppListBundle` as an input.

use crate::engine::AppListBundle;

/// Embedded JSON for the three default app lists. Loaded at compile time so
/// the binary works offline. Future versions will additionally check a
/// versioned URL and prefer fresher copies; that goes here.
const COMMON_JSON: &str = include_str!("../../../data/app-lists/common.json");
const SHIELD_JSON: &str = include_str!("../../../data/app-lists/shield.json");
const GOOGLETV_JSON: &str = include_str!("../../../data/app-lists/googletv.json");

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

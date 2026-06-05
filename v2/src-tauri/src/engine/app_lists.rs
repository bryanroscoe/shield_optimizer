//! App list loading — engine-side type definitions and merging logic.
//! No I/O here; the host layer loads JSON from disk (or remote) and passes it in.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use super::detection::DeviceType;
use super::types::AppEntry;

/// A named app list — one of common / shield / googletv (or any user/community add).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppList {
    pub name: String,
    pub entries: Vec<AppEntry>,
}

/// All loaded app lists, indexed for lookup.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppListBundle {
    pub common: Vec<AppEntry>,
    pub shield: Vec<AppEntry>,
    pub googletv: Vec<AppEntry>,
}

impl AppListBundle {
    /// Return the merged app list applicable to `device_type` — common is always
    /// included; device-specific extras are appended.
    ///
    /// Duplicate packages (same package in common AND device-specific) prefer
    /// the device-specific entry so per-device overrides work. The result is
    /// guaranteed unique by package — see the dedup note below.
    pub fn for_device(&self, device_type: DeviceType) -> Vec<AppEntry> {
        let device_specific: &[AppEntry] = match device_type {
            DeviceType::Shield => &self.shield,
            DeviceType::GoogleTv => &self.googletv,
            DeviceType::Unknown => &[],
        };

        // Device-specific entries win over common, so reserve their packages.
        let overrides: HashSet<&str> = device_specific.iter().map(|e| e.package.as_str()).collect();

        // Guarantee each package appears exactly once. The UI renders a keyed
        // `{#each ... (package)}`; Svelte throws on a duplicate key and blanks
        // the whole table (header counts still show), so a stray repeat in the
        // data — even within a single list — must never reach it.
        let mut seen: HashSet<&str> = HashSet::new();
        let mut out: Vec<AppEntry> = Vec::with_capacity(self.common.len() + device_specific.len());
        for entry in self
            .common
            .iter()
            .filter(|e| !overrides.contains(e.package.as_str()))
            .chain(device_specific.iter())
        {
            if seen.insert(entry.package.as_str()) {
                out.push(entry.clone());
            }
        }
        out
    }

    /// Total app count across all lists (for diagnostics).
    pub fn total(&self) -> usize {
        self.common.len() + self.shield.len() + self.googletv.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::types::{ActionMethod, RiskTier};
    use pretty_assertions::assert_eq;

    fn entry(pkg: &str, name: &str) -> AppEntry {
        AppEntry {
            package: pkg.to_string(),
            name: name.to_string(),
            method: ActionMethod::Uninstall,
            risk: RiskTier::Safe,
            optimize_description: String::new(),
            restore_description: String::new(),
            default_optimize: false,
            default_restore: false,
            play_store: false,
        }
    }

    #[test]
    fn shield_merges_common_and_shield() {
        let bundle = AppListBundle {
            common: vec![entry("c1", "Common1"), entry("c2", "Common2")],
            shield: vec![entry("s1", "Shield1")],
            googletv: vec![],
        };
        let result = bundle.for_device(DeviceType::Shield);
        assert_eq!(result.len(), 3);
        assert!(result.iter().any(|e| e.package == "c1"));
        assert!(result.iter().any(|e| e.package == "s1"));
    }

    #[test]
    fn googletv_excludes_shield_entries() {
        let bundle = AppListBundle {
            common: vec![entry("c1", "Common1")],
            shield: vec![entry("s1", "ShieldOnly")],
            googletv: vec![entry("g1", "GTVOnly")],
        };
        let result = bundle.for_device(DeviceType::GoogleTv);
        assert!(result.iter().any(|e| e.package == "g1"));
        assert!(!result.iter().any(|e| e.package == "s1"));
    }

    #[test]
    fn unknown_returns_common_only() {
        let bundle = AppListBundle {
            common: vec![entry("c1", "Common1")],
            shield: vec![entry("s1", "ShieldOnly")],
            googletv: vec![entry("g1", "GTVOnly")],
        };
        let result = bundle.for_device(DeviceType::Unknown);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].package, "c1");
    }

    #[test]
    fn device_specific_overrides_common() {
        // If a package is in both common and shield, the shield entry wins.
        let mut shield_override = entry("dup", "Shield Override");
        shield_override.risk = RiskTier::High;
        let bundle = AppListBundle {
            common: vec![entry("dup", "Common Default"), entry("c1", "Common1")],
            shield: vec![shield_override],
            googletv: vec![],
        };
        let result = bundle.for_device(DeviceType::Shield);
        let dup = result.iter().find(|e| e.package == "dup").unwrap();
        assert_eq!(dup.name, "Shield Override");
        assert_eq!(dup.risk, RiskTier::High);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn duplicate_packages_within_a_list_are_collapsed() {
        // A stray repeat inside one list (the catalog regression that blanked the
        // App List + Optimize tables) must dedup, keeping the first occurrence.
        let bundle = AppListBundle {
            common: vec![
                entry("tv.pluto.android", "Pluto TV"),
                entry("c1", "Common1"),
                entry("tv.pluto.android", "Pluto TV (dup)"),
            ],
            shield: vec![],
            googletv: vec![],
        };
        let result = bundle.for_device(DeviceType::Shield);
        let plutos: Vec<_> = result
            .iter()
            .filter(|e| e.package == "tv.pluto.android")
            .collect();
        assert_eq!(plutos.len(), 1, "duplicate package must collapse to one");
        assert_eq!(plutos[0].name, "Pluto TV", "first occurrence wins");
        assert_eq!(result.len(), 2);
    }
}

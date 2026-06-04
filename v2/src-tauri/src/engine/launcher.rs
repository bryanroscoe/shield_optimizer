//! Custom launcher catalog + plan helpers.

use serde::{Deserialize, Serialize};

/// One supported custom launcher.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LauncherEntry {
    pub name: String,
    pub package: String,
}

/// The preset launcher catalog — direct port of v1's `$Script:Launchers`.
pub fn launcher_catalog() -> Vec<LauncherEntry> {
    [
        ("Projectivy Launcher", "com.spocky.projengmenu"),
        ("FLauncher", "me.efesser.flauncher"),
        ("ATV Launcher", "com.sweech.launcher"),
        ("Wolf Launcher", "com.wolf.firelauncher"),
        ("AT4K Launcher", "com.overdevs.at4k"),
        ("Dispatch Launcher", "com.spauldhaliwal.dispatch"),
    ]
    .iter()
    .map(|(n, p)| LauncherEntry {
        name: n.to_string(),
        package: p.to_string(),
    })
    .collect()
}

/// Stock launchers, with friendly names. These get disabled when activating a
/// custom launcher — and shown in the launcher list so users can switch back.
pub fn stock_launcher_catalog() -> Vec<LauncherEntry> {
    [
        (
            "Android TV Launcher (Stock)",
            "com.google.android.tvlauncher",
        ),
        (
            "Google TV Home (Stock)",
            "com.google.android.apps.tv.launcherx",
        ),
        (
            "Leanback Launcher (Stock)",
            "com.google.android.leanbacklauncher",
        ),
        ("Amazon TV Launcher (Stock)", "com.amazon.tv.launcher"),
    ]
    .iter()
    .map(|(n, p)| LauncherEntry {
        name: n.to_string(),
        package: p.to_string(),
    })
    .collect()
}

/// Friendly names for known HOME-capable apps that aren't launchers.
pub fn home_handler_name(pkg: &str) -> Option<&'static str> {
    match pkg {
        "com.google.android.tungsten.setupwraith" => Some("Setup Wraith (HOME)"),
        "com.droidlogic.launcher.provider" => Some("Droidlogic Launcher Provider"),
        _ => None,
    }
}

/// One row of the Launchers list: a catalog entry plus its on-device state.
#[derive(Debug, Clone, Serialize)]
pub struct LauncherStatus {
    pub entry: LauncherEntry,
    pub installed: bool,
    pub enabled: bool,
    /// True for the device's preinstalled launcher(s) — rendered with a STOCK
    /// badge and no Install button, since they aren't on the Play Store.
    pub stock: bool,
    /// True for HOME-capable apps outside both catalogs (e.g. Setup Wraith,
    /// a sideloaded HOME app) — rendered with a HOME APP badge.
    pub other: bool,
}

/// Build the Launchers list: stock launchers actually present on the device
/// first (so "back to stock" is always one click away), then the full custom
/// catalog, then any other HOME-capable app. Custom launchers are listed even
/// when missing (they get an Install button); stock ones only when installed —
/// a Shield shouldn't show a "missing" Amazon launcher row.
///
/// `home_handler_pkgs` is the device's enabled HOME handlers (disabled
/// packages don't answer the HOME intent query, which is why callers pass
/// `tracked_disabled_pkgs` — handlers we disabled ourselves and remembered).
/// Safe fallbacks (Settings) are deliberately absent: they must never be
/// disabled, so we don't render them at all.
pub fn launcher_rows(
    installed_pkgs: &[String],
    disabled_pkgs: &[String],
    home_handler_pkgs: &[String],
    tracked_disabled_pkgs: &[String],
) -> Vec<LauncherStatus> {
    let is_disabled = |pkg: &str| disabled_pkgs.iter().any(|d| d == pkg);
    let stock_catalog = stock_launcher_catalog();
    let custom_catalog = launcher_catalog();

    let stock = stock_catalog
        .iter()
        .filter(|e| installed_pkgs.iter().any(|p| p == &e.package))
        .map(|entry| LauncherStatus {
            enabled: !is_disabled(&entry.package),
            installed: true,
            stock: true,
            other: false,
            entry: entry.clone(),
        });

    let custom = custom_catalog.iter().map(|entry| {
        let installed = installed_pkgs.iter().any(|p| p == &entry.package);
        LauncherStatus {
            installed,
            enabled: installed && !is_disabled(&entry.package),
            stock: false,
            other: false,
            entry: entry.clone(),
        }
    });

    let in_catalogs = |pkg: &str| {
        stock_catalog.iter().any(|e| e.package == pkg)
            || custom_catalog.iter().any(|e| e.package == pkg)
    };
    let mut seen_other = std::collections::HashSet::new();
    let other = home_handler_pkgs
        .iter()
        .chain(tracked_disabled_pkgs.iter())
        .filter(|pkg| {
            !in_catalogs(pkg)
                && !safe_home_handlers().contains(&pkg.as_str())
                && seen_other.insert(pkg.to_string())
        })
        .map(|pkg| LauncherStatus {
            entry: LauncherEntry {
                name: home_handler_name(pkg).unwrap_or(pkg).to_string(),
                package: pkg.clone(),
            },
            installed: true,
            enabled: !is_disabled(pkg),
            stock: false,
            other: true,
        });

    stock.chain(custom).chain(other).collect()
}

/// True when disabling `target` would leave the device without a single
/// enabled HOME handler the user can actually land on. Safe fallbacks
/// (Settings) don't count — they're a recovery hatch, not a launcher.
pub fn is_last_enabled_home_handler(target: &str, enabled_handler_pkgs: &[String]) -> bool {
    let target_is_enabled = enabled_handler_pkgs.iter().any(|h| h == target);
    let remaining = enabled_handler_pkgs
        .iter()
        .filter(|h| h.as_str() != target && !safe_home_handlers().contains(&h.as_str()))
        .count();
    target_is_enabled && remaining == 0
}

/// HOME-capable packages that we must NEVER disable — fallback safety net.
pub fn safe_home_handlers() -> &'static [&'static str] {
    &["com.android.tv.settings", "com.android.settings"]
}

/// Validate a user-supplied custom launcher package name (Setup-Launcher's
/// "Custom..." option in v1). Matches the regex v1 uses.
pub fn is_valid_package_name(pkg: &str) -> bool {
    if pkg.is_empty() {
        return false;
    }
    // Must look like `name(.name)+` where each segment starts with a letter.
    let segments: Vec<&str> = pkg.split('.').collect();
    if segments.len() < 2 {
        return false;
    }
    segments.iter().all(|seg| {
        !seg.is_empty()
            && seg.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
            && seg.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn catalog_has_six_entries() {
        let cat = launcher_catalog();
        assert_eq!(cat.len(), 6);
        // Critical correctness: the Dispatch package name change from v1's
        // launcher selection fix.
        let dispatch = cat.iter().find(|e| e.name == "Dispatch Launcher").unwrap();
        assert_eq!(dispatch.package, "com.spauldhaliwal.dispatch");
    }

    #[test]
    fn projectivy_present() {
        let cat = launcher_catalog();
        assert!(cat.iter().any(|e| e.package == "com.spocky.projengmenu"));
    }

    fn pkgs(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn launcher_rows_put_installed_stock_first() {
        let rows = launcher_rows(
            &pkgs(&["com.google.android.tvlauncher", "com.spocky.projengmenu"]),
            &[],
            &[],
            &[],
        );
        assert_eq!(rows[0].entry.package, "com.google.android.tvlauncher");
        assert!(rows[0].stock);
        assert!(rows[0].installed);
        assert!(rows[0].enabled);
        // Stock + all six custom catalog entries.
        assert_eq!(rows.len(), 7);
        assert!(rows[1..].iter().all(|r| !r.stock));
    }

    #[test]
    fn launcher_rows_omit_stock_not_on_device() {
        // A Shield shouldn't get an Amazon (or any "missing stock") row.
        let rows = launcher_rows(&pkgs(&["com.spocky.projengmenu"]), &[], &[], &[]);
        assert!(rows.iter().all(|r| !r.stock));
        assert_eq!(rows.len(), 6);
    }

    #[test]
    fn launcher_rows_mark_disabled_stock() {
        // The post-cleanup state: stock disabled, custom active. The stock
        // row must still appear — it's the path back.
        let rows = launcher_rows(
            &pkgs(&["com.google.android.tvlauncher", "com.spocky.projengmenu"]),
            &pkgs(&["com.google.android.tvlauncher"]),
            &[],
            &[],
        );
        assert!(rows[0].stock);
        assert!(rows[0].installed);
        assert!(!rows[0].enabled);
    }

    #[test]
    fn launcher_rows_include_other_home_handlers_but_never_safe_fallbacks() {
        let rows = launcher_rows(
            &pkgs(&["com.spocky.projengmenu"]),
            &[],
            &pkgs(&[
                "com.spocky.projengmenu",                  // catalog — already a row
                "com.android.tv.settings",                 // safe fallback — never shown
                "com.google.android.tungsten.setupwraith", // genuinely "other"
            ]),
            &[],
        );
        let others: Vec<_> = rows.iter().filter(|r| r.other).collect();
        assert_eq!(others.len(), 1);
        assert_eq!(
            others[0].entry.package,
            "com.google.android.tungsten.setupwraith"
        );
        assert_eq!(others[0].entry.name, "Setup Wraith (HOME)");
        assert!(others[0].enabled);
        assert!(!rows
            .iter()
            .any(|r| r.entry.package == "com.android.tv.settings"));
    }

    #[test]
    fn launcher_rows_keep_tracked_disabled_handlers_visible() {
        // A disabled handler doesn't answer the HOME query — the tracked list
        // is what keeps its row (and its Enable path) alive.
        let rows = launcher_rows(
            &pkgs(&["com.spocky.projengmenu"]),
            &pkgs(&["com.example.sideloaded.home"]),
            &[],
            &pkgs(&["com.example.sideloaded.home"]),
        );
        let row = rows
            .iter()
            .find(|r| r.entry.package == "com.example.sideloaded.home")
            .unwrap();
        assert!(row.other);
        assert!(!row.enabled);
    }

    #[test]
    fn last_enabled_home_handler_guard() {
        let enabled = pkgs(&["com.spocky.projengmenu", "com.android.tv.settings"]);
        // Projectivy is the only real launcher left — Settings doesn't count.
        assert!(is_last_enabled_home_handler(
            "com.spocky.projengmenu",
            &enabled
        ));

        let two = pkgs(&["com.spocky.projengmenu", "com.google.android.tvlauncher"]);
        assert!(!is_last_enabled_home_handler(
            "com.spocky.projengmenu",
            &two
        ));

        // Target already disabled (absent from the enabled list) — nothing to guard.
        assert!(!is_last_enabled_home_handler(
            "com.example.gone",
            &pkgs(&["com.spocky.projengmenu"])
        ));
    }

    #[test]
    fn package_name_validation_accepts_valid() {
        for valid in &[
            "com.example.launcher",
            "tv.projectivy.launcher",
            "com.google.android.tvlauncher",
            "org.example.app123",
            "com.a.b",
            "com.Example_App.test",
        ] {
            assert!(is_valid_package_name(valid), "should accept {}", valid);
        }
    }

    #[test]
    fn package_name_validation_rejects_invalid() {
        for invalid in &[
            "",
            "   ",
            "com",
            "com.",
            ".com.example",
            "123.example.app",
            "com..example",
            "com.example.",
            "com.123.app",
            "-com.example.app",
            "com.example.app with spaces",
        ] {
            assert!(
                !is_valid_package_name(invalid),
                "should reject {:?}",
                invalid
            );
        }
    }
}

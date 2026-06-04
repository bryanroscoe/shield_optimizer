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

/// One row of the Launchers list: a catalog entry plus its on-device state.
#[derive(Debug, Clone, Serialize)]
pub struct LauncherStatus {
    pub entry: LauncherEntry,
    pub installed: bool,
    pub enabled: bool,
    /// True for the device's preinstalled launcher(s) — rendered with a STOCK
    /// badge and no Install button, since they aren't on the Play Store.
    pub stock: bool,
}

/// Build the Launchers list: stock launchers actually present on the device
/// first (so "back to stock" is always one click away), then the full custom
/// catalog. Custom launchers are listed even when missing (they get an
/// Install button); stock ones only when installed — a Shield shouldn't show
/// a "missing" Amazon launcher row.
pub fn launcher_rows(installed_pkgs: &[String], disabled_pkgs: &[String]) -> Vec<LauncherStatus> {
    let is_disabled = |pkg: &str| disabled_pkgs.iter().any(|d| d == pkg);
    let stock = stock_launcher_catalog()
        .into_iter()
        .filter(|e| installed_pkgs.iter().any(|p| p == &e.package))
        .map(|entry| LauncherStatus {
            enabled: !is_disabled(&entry.package),
            installed: true,
            stock: true,
            entry,
        });
    let custom = launcher_catalog().into_iter().map(|entry| {
        let installed = installed_pkgs.iter().any(|p| p == &entry.package);
        LauncherStatus {
            installed,
            enabled: installed && !is_disabled(&entry.package),
            stock: false,
            entry,
        }
    });
    stock.chain(custom).collect()
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
        let rows = launcher_rows(&pkgs(&["com.spocky.projengmenu"]), &[]);
        assert!(rows.iter().all(|r| !r.stock));
        assert_eq!(rows.len(), 6);
    }

    #[test]
    fn launcher_rows_mark_disabled_stock() {
        // The post-wizard state: stock disabled, custom active. The stock row
        // must still appear — it's the path back.
        let rows = launcher_rows(
            &pkgs(&["com.google.android.tvlauncher", "com.spocky.projengmenu"]),
            &pkgs(&["com.google.android.tvlauncher"]),
        );
        assert!(rows[0].stock);
        assert!(rows[0].installed);
        assert!(!rows[0].enabled);
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

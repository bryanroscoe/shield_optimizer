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

/// Stock launcher packages — these get disabled when activating a custom launcher.
pub fn stock_launchers() -> &'static [&'static str] {
    &[
        "com.google.android.tvlauncher",
        "com.google.android.apps.tv.launcherx",
        "com.google.android.leanbacklauncher",
        "com.amazon.tv.launcher",
    ]
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

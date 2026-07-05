//! Entitlement model shared by desktop and mobile frontends.
//!
//! Desktop constructs `AppState` as Pro. Mobile starts Free and later replaces
//! the entitlement from a validated, signed license token.

use serde::{Deserialize, Serialize};

/// Documented test license key that unlocks Pro. This is a placeholder for the
/// real signed-token validation (M5) — it lets the mobile activation flow and
/// its gates be exercised end-to-end today. Matching is case-insensitive and
/// trims surrounding whitespace (see [`validate_license_key`]).
pub const TEST_LICENSE_KEY: &str = "ATVOPT-PRO-2025";

/// Validate a license key. Currently accepts only [`TEST_LICENSE_KEY`],
/// compared case-insensitively after trimming. Replace with signed-token
/// verification when M5 licensing lands.
pub fn validate_license_key(key: &str) -> bool {
    key.trim().eq_ignore_ascii_case(TEST_LICENSE_KEY)
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

    #[test]
    fn validates_test_key_case_insensitively_and_trimmed() {
        assert!(validate_license_key(TEST_LICENSE_KEY));
        assert!(validate_license_key("  atvopt-pro-2025  "));
        assert!(validate_license_key("ATVOpt-Pro-2025"));
    }

    #[test]
    fn rejects_bad_keys() {
        assert!(!validate_license_key(""));
        assert!(!validate_license_key("nope"));
        assert!(!validate_license_key("ATVOPT-PRO-2024"));
    }
}

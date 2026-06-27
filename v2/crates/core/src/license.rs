//! Entitlement model shared by desktop and mobile frontends.
//!
//! Desktop constructs `AppState` as Pro. Mobile starts Free and later replaces
//! the entitlement from a validated, signed license token.

use serde::{Deserialize, Serialize};

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

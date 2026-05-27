//! Engine — pure logic with no I/O.
//!
//! Per architectural commitment #1 (see `v2/README.md`): nothing in this module
//! should call ADB, read files, or make network requests. The engine returns
//! plans that the host layer (Tauri commands) is responsible for executing
//! through the ADB driver.

pub mod app_lists;
pub mod detection;
pub mod launcher;
pub mod snapshot;
pub mod types;

pub use app_lists::{AppList, AppListBundle};
pub use detection::{detect_device_type, DeviceType};
pub use launcher::{launcher_catalog, LauncherEntry};
pub use snapshot::{Snapshot, SnapshotApplyPlan, SnapshotError, SCHEMA_VERSION};
pub use types::{
    ActionMethod, AppEntry, Device, DeviceProperties, DeviceStatus, OptimizeAction, OptimizeMode,
    OptimizePlanItem, RiskTier, SkipReason,
};

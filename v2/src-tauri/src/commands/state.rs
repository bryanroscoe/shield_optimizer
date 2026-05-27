//! Shared application state held across Tauri command invocations.

use std::path::PathBuf;
use std::sync::Arc;

use crate::adb::{AdbDriver, SubprocessAdb};
use crate::engine::AppListBundle;

/// State managed by Tauri's state store. Held by `tauri::Builder::manage`.
pub struct AppState {
    /// The driver for ADB calls. `Arc<dyn>` so commands can clone cheaply.
    pub adb: Arc<dyn AdbDriver>,
    /// Loaded app-list bundle (common + shield + googletv).
    pub app_lists: AppListBundle,
    /// Directory where snapshots are read from / written to.
    pub snapshot_dir: PathBuf,
}

impl AppState {
    pub fn new(adb: Arc<dyn AdbDriver>, app_lists: AppListBundle, snapshot_dir: PathBuf) -> Self {
        Self {
            adb,
            app_lists,
            snapshot_dir,
        }
    }

    /// Build the standard state — subprocess ADB, embedded app lists, OS
    /// snapshot dir. Falls back to a no-op driver if ADB isn't on PATH so the
    /// frontend can still render an error rather than crashing on startup.
    pub fn default_for_runtime(app_lists: AppListBundle, snapshot_dir: PathBuf) -> Self {
        let adb: Arc<dyn AdbDriver> = match SubprocessAdb::from_path() {
            Some(driver) => Arc::new(driver),
            None => Arc::new(SubprocessAdb::new(PathBuf::from("adb"))),
        };
        Self::new(adb, app_lists, snapshot_dir)
    }
}

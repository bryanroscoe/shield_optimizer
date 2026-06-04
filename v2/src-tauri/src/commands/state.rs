//! Shared application state held across Tauri command invocations.

use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::RwLock;

use crate::adb::driver::discover_adb_binary;
use crate::adb::{AdbDriver, AdbError, AdbOutput, AdbResult, SubprocessAdb};
use crate::engine::AppListBundle;

/// State managed by Tauri's state store. Held by `tauri::Builder::manage`.
pub struct AppState {
    /// The driver for ADB calls. Wrapped so we can hot-swap it after a
    /// successful platform-tools download (no app restart required).
    pub adb: RwLock<Arc<dyn AdbDriver>>,
    /// Loaded app-list bundle (common + shield + googletv).
    pub app_lists: AppListBundle,
    /// App data root (parent of `snapshot_dir`) — small bookkeeping files
    /// like the disabled-HOME-handler tracker live here.
    pub data_dir: PathBuf,
    /// Directory where snapshots are read from / written to.
    pub snapshot_dir: PathBuf,
}

impl AppState {
    pub fn new(adb: Arc<dyn AdbDriver>, app_lists: AppListBundle, data_dir: PathBuf) -> Self {
        Self {
            adb: RwLock::new(adb),
            app_lists,
            snapshot_dir: data_dir.join("snapshots"),
            data_dir,
        }
    }

    /// Build the standard runtime state. If no adb binary can be found, we
    /// still construct an `AppState` so the GUI can render — but every ADB
    /// call returns `AdbError::BinaryNotFound`, which renders as an
    /// actionable error in the device list. The user can then trigger a
    /// download via the `install_adb` command.
    pub fn default_for_runtime(app_lists: AppListBundle, data_dir: PathBuf) -> Self {
        let adb: Arc<dyn AdbDriver> = match discover_adb_binary() {
            Some(path) => {
                tracing::info!(adb = %path.display(), "adb located");
                Arc::new(SubprocessAdb::new(path))
            }
            None => {
                tracing::warn!("no adb binary located; commands will return BinaryNotFound");
                Arc::new(NoAdbDriver)
            }
        };
        Self::new(adb, app_lists, data_dir)
    }

    /// Snapshot the current driver `Arc` — cheap clone for command bodies.
    pub async fn adb_snapshot(&self) -> Arc<dyn AdbDriver> {
        self.adb.read().await.clone()
    }

    /// Swap the driver — used by `install_adb` after a successful download.
    pub async fn replace_adb(&self, new_driver: Arc<dyn AdbDriver>) {
        *self.adb.write().await = new_driver;
    }
}

/// Driver used when no adb binary could be discovered at startup. Every call
/// returns the actionable `BinaryNotFound` error so the UI tells the user
/// exactly what to do.
struct NoAdbDriver;

#[async_trait::async_trait]
impl AdbDriver for NoAdbDriver {
    async fn raw(&self, _args: &[&str]) -> AdbResult<AdbOutput> {
        Err(AdbError::BinaryNotFound)
    }
    async fn shell(&self, _serial: &str, _command: &str) -> AdbResult<AdbOutput> {
        Err(AdbError::BinaryNotFound)
    }
}

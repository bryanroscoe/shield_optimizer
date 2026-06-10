//! Shared application state held across Tauri command invocations.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::sync::{Mutex, RwLock};

use crate::adb::driver::discover_adb_binary;
use crate::adb::remote_input::SERVER_JAR_RESOURCE_PATH;
use crate::adb::{AdbDriver, AdbError, AdbOutput, AdbResult, RemoteInputSession, SubprocessAdb};
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
    /// package → friendly name for popular sideloads that aren't in the curated
    /// catalog (Artemis, Overseerr, …). Display-only: lets the App List show and
    /// search "Everything else" by a recognizable name instead of a bare package
    /// ID. There's no cheap way to read an app's label over adb, so this is a
    /// curated map loaded from `data/app-lists/known-names.json`.
    pub known_names: HashMap<String, String>,
    /// Live scrcpy control sessions, keyed by device serial. Lazily started on
    /// the first remote key and held open for the Remote tab's lifetime.
    pub remote_sessions: Mutex<HashMap<String, RemoteInputSession>>,
}

impl AppState {
    pub fn new(adb: Arc<dyn AdbDriver>, app_lists: AppListBundle, data_dir: PathBuf) -> Self {
        Self {
            adb: RwLock::new(adb),
            app_lists,
            snapshot_dir: data_dir.join("snapshots"),
            data_dir,
            known_names: HashMap::new(),
            remote_sessions: Mutex::new(HashMap::new()),
        }
    }

    /// Attach the curated package→name map. Builder-style so the existing
    /// constructors (and their test callers) stay unchanged.
    pub fn with_known_names(mut self, known_names: HashMap<String, String>) -> Self {
        self.known_names = known_names;
        self
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

    /// Get-or-start the scrcpy control session for `serial`. The slow `start()`
    /// (push + forward + spawn + connect) runs OUTSIDE the registry lock so a
    /// cold start can't block other commands; the lock is only taken for the
    /// fast presence check and the final insert. If two callers race, the loser
    /// tears its extra session down.
    pub async fn ensure_remote_session(
        &self,
        adb: Arc<dyn AdbDriver>,
        jar_path: &Path,
        serial: &str,
    ) -> Result<(), String> {
        if self.remote_sessions.lock().await.contains_key(serial) {
            return Ok(());
        }
        let session = RemoteInputSession::start(adb, jar_path, serial).await?;
        let mut guard = self.remote_sessions.lock().await;
        if guard.contains_key(serial) {
            drop(guard);
            session.close().await;
        } else {
            guard.insert(serial.to_string(), session);
        }
        Ok(())
    }

    /// Inject a single key-down / key-up via the live session. Errors if no
    /// session exists — Phase 3 calls `ensure_remote_session` first, and on a
    /// write error should `drop_remote_session` and fall back to `input`.
    pub async fn remote_send_key(
        &self,
        serial: &str,
        keycode: u32,
        down: bool,
    ) -> Result<(), String> {
        let mut guard = self.remote_sessions.lock().await;
        let session = guard
            .get_mut(serial)
            .ok_or_else(|| "no active remote session".to_string())?;
        session.send_key(keycode, down).await
    }

    /// Inject a full key press (down + up) via the live session.
    pub async fn remote_send_key_press(&self, serial: &str, keycode: u32) -> Result<(), String> {
        let mut guard = self.remote_sessions.lock().await;
        let session = guard
            .get_mut(serial)
            .ok_or_else(|| "no active remote session".to_string())?;
        session.send_key_press(keycode).await
    }

    /// Inject UTF-8 text via the live session.
    pub async fn remote_send_text(&self, serial: &str, text: &str) -> Result<(), String> {
        let mut guard = self.remote_sessions.lock().await;
        let session = guard
            .get_mut(serial)
            .ok_or_else(|| "no active remote session".to_string())?;
        session.send_text(text).await
    }

    /// Tear down and forget the session for `serial`, if any. Removes it from
    /// the registry first, then closes outside the lock.
    pub async fn drop_remote_session(&self, serial: &str) {
        let session = self.remote_sessions.lock().await.remove(serial);
        if let Some(session) = session {
            session.close().await;
        }
    }
}

/// Resolve the on-disk path of the bundled scrcpy server jar. Prefers the
/// Tauri resource directory (production install); falls back to the
/// crate-relative `resources/` dir for `cargo run` / `cargo test`.
///
/// Phase 3: the remote-input command calls this with its `AppHandle` to get the
/// jar path, then hands it to `AppState::ensure_remote_session`.
pub fn resolve_scrcpy_server_jar(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    use tauri::Manager;
    if let Ok(p) = app.path().resolve(
        SERVER_JAR_RESOURCE_PATH,
        tauri::path::BaseDirectory::Resource,
    ) {
        if p.is_file() {
            return Ok(p);
        }
    }
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(SERVER_JAR_RESOURCE_PATH);
    if dev.is_file() {
        return Ok(dev);
    }
    Err(format!(
        "scrcpy server jar not found (looked in the Tauri resource dir and {})",
        dev.display()
    ))
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

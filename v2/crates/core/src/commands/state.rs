//! Shared application state held across Tauri command invocations.

use std::collections::HashMap;
#[cfg(not(target_os = "android"))]
use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

#[cfg(not(target_os = "android"))]
use tokio::sync::Mutex;
use tokio::sync::RwLock;

#[cfg(not(target_os = "android"))]
use crate::adb::remote_input::SERVER_JAR_RESOURCE_PATH;
use crate::adb::AdbDriver;
#[cfg(not(target_os = "android"))]
use crate::adb::RemoteInputSession;
use crate::engine::AppListBundle;
use crate::license::{Entitlement, Feature};

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
    /// curated map loaded from `crates/core/data/app-lists/known-names.json`.
    pub known_names: HashMap<String, String>,
    /// Current product entitlement, stored atomically so it can be flipped at
    /// runtime (mobile's `activate_license`) while `require_pro` stays a cheap
    /// synchronous read. Desktop constructs this as Pro; mobile starts Free and
    /// replaces it after license validation. Encoded via [`entitlement_to_u8`].
    entitlement: AtomicU8,
    /// Live scrcpy control sessions, keyed by device serial. Lazily started on
    /// the first remote key and held open for the Remote tab's lifetime.
    #[cfg(not(target_os = "android"))]
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
            entitlement: AtomicU8::new(entitlement_to_u8(Entitlement::Pro)),
            #[cfg(not(target_os = "android"))]
            remote_sessions: Mutex::new(HashMap::new()),
        }
    }

    /// Attach the curated package→name map. Builder-style so the existing
    /// constructors (and their test callers) stay unchanged.
    pub fn with_known_names(mut self, known_names: HashMap<String, String>) -> Self {
        self.known_names = known_names;
        self
    }

    pub fn with_entitlement(self, entitlement: Entitlement) -> Self {
        self.set_entitlement(entitlement);
        self
    }

    /// Read the live entitlement.
    pub fn entitlement(&self) -> Entitlement {
        entitlement_from_u8(self.entitlement.load(Ordering::Relaxed))
    }

    /// Flip the live entitlement at runtime (mobile `activate_license`).
    pub fn set_entitlement(&self, entitlement: Entitlement) {
        self.entitlement
            .store(entitlement_to_u8(entitlement), Ordering::Relaxed);
    }

    pub fn require_pro(&self, feature: Feature) -> Result<(), String> {
        match self.entitlement() {
            Entitlement::Pro => Ok(()),
            Entitlement::Free => Err(format!("LOCKED:{}", feature.code())),
        }
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
    #[cfg(not(target_os = "android"))]
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
    #[cfg(not(target_os = "android"))]
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
    #[cfg(not(target_os = "android"))]
    pub async fn remote_send_key_press(&self, serial: &str, keycode: u32) -> Result<(), String> {
        let mut guard = self.remote_sessions.lock().await;
        let session = guard
            .get_mut(serial)
            .ok_or_else(|| "no active remote session".to_string())?;
        session.send_key_press(keycode).await
    }

    /// Inject UTF-8 text via the live session.
    #[cfg(not(target_os = "android"))]
    pub async fn remote_send_text(&self, serial: &str, text: &str) -> Result<(), String> {
        let mut guard = self.remote_sessions.lock().await;
        let session = guard
            .get_mut(serial)
            .ok_or_else(|| "no active remote session".to_string())?;
        session.send_text(text).await
    }

    /// Tear down and forget the session for `serial`, if any. Removes it from
    /// the registry first, then closes outside the lock.
    #[cfg(not(target_os = "android"))]
    pub async fn drop_remote_session(&self, serial: &str) {
        let session = self.remote_sessions.lock().await.remove(serial);
        if let Some(session) = session {
            session.close().await;
        }
    }
}

/// Encode an [`Entitlement`] into the byte stored in the atomic cell.
const fn entitlement_to_u8(entitlement: Entitlement) -> u8 {
    match entitlement {
        Entitlement::Free => 0,
        Entitlement::Pro => 1,
    }
}

/// Decode a stored byte back into an [`Entitlement`]. Any unexpected value maps
/// to the safe default (`Free`).
fn entitlement_from_u8(value: u8) -> Entitlement {
    match value {
        1 => Entitlement::Pro,
        _ => Entitlement::Free,
    }
}

/// Resolve the on-disk path of the bundled scrcpy server jar. Prefers the
/// Tauri resource directory (production install); falls back to the
/// crate-relative `resources/` dir for `cargo run` / `cargo test`.
///
/// Phase 3: the remote-input command calls this with its `AppHandle` to get the
/// jar path, then hands it to `AppState::ensure_remote_session`.
#[cfg(not(target_os = "android"))]
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
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../src-tauri")
        .join(SERVER_JAR_RESOURCE_PATH);
    if dev.is_file() {
        return Ok(dev);
    }
    Err(format!(
        "scrcpy server jar not found (looked in the Tauri resource dir and {})",
        dev.display()
    ))
}

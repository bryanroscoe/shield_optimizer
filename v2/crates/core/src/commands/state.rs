//! Shared application state held across Tauri command invocations.

use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

use tokio::sync::RwLock;

use crate::adb::lifetimes::{ClosedAdb, LeasedAdb};
#[cfg(not(target_os = "android"))]
use crate::adb::remote_input::SERVER_JAR_RESOURCE_PATH;
use crate::adb::RemoteInputSession;
use crate::adb::{AdbDriver, AdbError, Lease, Leases, ServerStop};
use crate::engine::AppListBundle;
use crate::license::{Entitlement, Feature, LicenseInfo};

/// State managed by Tauri's state store. Held by `tauri::Builder::manage`.
pub struct AppState {
    /// The driver for ADB calls. Wrapped so we can hot-swap it after a
    /// successful platform-tools download (no app restart required). Private
    /// so every use goes through `adb_snapshot`, which counts it.
    adb: RwLock<Arc<dyn AdbDriver>>,
    /// Every live driver snapshot holds one of these, across driver swaps.
    /// Quit closes it and waits for it to drain before stopping the server.
    adb_operations: Arc<Leases>,
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
    pub known_names: HashMap<String, crate::commands::loader::KnownName>,
    /// Current product entitlement, stored atomically so it can be flipped at
    /// runtime (mobile's `activate_license`) while `require_pro` stays a cheap
    /// synchronous read. Desktop constructs this as Pro; mobile starts Free and
    /// replaces it after license validation. Encoded via [`entitlement_to_u8`].
    entitlement: AtomicU8,
    /// Details of the license behind a Pro entitlement, recorded by the
    /// activation path so `license_info` can show the licensee and term
    /// without keeping the key itself around. `None` on Free.
    license_info: std::sync::Mutex<Option<LicenseInfo>>,
    remote_registry: crate::adb::remote_lifecycle::RemoteRegistry,
    remote_driver_swap: tokio::sync::Mutex<()>,
}

impl AppState {
    pub fn new(adb: Arc<dyn AdbDriver>, app_lists: AppListBundle, data_dir: PathBuf) -> Self {
        let adb_operations = adb.operation_leases().unwrap_or_default();
        Self {
            adb: RwLock::new(adb),
            adb_operations,
            app_lists,
            snapshot_dir: data_dir.join("snapshots"),
            data_dir,
            known_names: HashMap::new(),
            // Fail closed: a construction path that forgets `with_entitlement`
            // must not silently grant Pro.
            entitlement: AtomicU8::new(entitlement_to_u8(Entitlement::Free)),
            license_info: std::sync::Mutex::new(None),
            remote_registry: crate::adb::remote_lifecycle::RemoteRegistry::default(),
            remote_driver_swap: tokio::sync::Mutex::new(()),
        }
    }

    /// Attach the curated package→name map.
    pub fn with_known_names<T: Into<crate::commands::loader::KnownName>>(
        mut self,
        known_names: HashMap<String, T>,
    ) -> Self {
        self.known_names = known_names
            .into_iter()
            .map(|(pkg, known)| (pkg, known.into()))
            .collect();
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

    /// Record (or clear, with `None`) the verified license behind the current
    /// entitlement. Called from the activation path and from startup restore.
    pub fn set_license_info(&self, info: Option<LicenseInfo>) {
        if let Ok(mut guard) = self.license_info.lock() {
            *guard = info;
        }
    }

    /// Read back the recorded license, if any.
    pub fn license_info(&self) -> Option<LicenseInfo> {
        self.license_info.lock().ok().and_then(|g| g.clone())
    }

    pub fn require_pro(&self, feature: Feature) -> Result<(), String> {
        match self.entitlement() {
            Entitlement::Pro => Ok(()),
            Entitlement::Free => Err(format!("LOCKED:{}", feature.code())),
        }
    }

    /// Snapshot the current driver for one operation. The snapshot counts as
    /// running ADB work until its last clone is dropped. Once quit has closed
    /// admission, it refuses every call instead.
    pub async fn adb_snapshot(&self) -> Arc<dyn AdbDriver> {
        let driver = self.adb.read().await.clone();
        match self.adb_operations.try_acquire() {
            Some(lease) => Arc::new(LeasedAdb::new(driver, lease)),
            None => Arc::new(ClosedAdb),
        }
    }

    /// Count work such as installation that must finish before quit can stop
    /// the server, even though it does not use the current driver.
    pub fn try_adb_operation(&self) -> Result<Lease, AdbError> {
        self.adb_operations
            .try_acquire()
            .ok_or(AdbError::ShuttingDown)
    }

    /// Refuse every later snapshot. Snapshots already handed out keep working.
    pub fn close_adb_admission(&self) {
        self.remote_registry.refuse_new();
        self.adb_operations.close();
    }

    /// Wait until no snapshot is alive, up to `deadline`.
    pub async fn wait_adb_operations(&self, deadline: tokio::time::Instant) -> bool {
        self.adb_operations.wait_idle(deadline).await
    }

    pub fn adb_operations_active(&self) -> usize {
        self.adb_operations.active()
    }

    /// Stop the adb server through the live driver, outside admission. Only
    /// the quit path calls this, after admission has closed and drained.
    pub async fn stop_adb_server(&self, timeout: std::time::Duration) -> ServerStop {
        let driver = self.adb.read().await.clone();
        driver.stop_server(timeout).await
    }

    /// Swap the driver — used by `install_adb` after a successful download.
    pub async fn replace_adb(&self, new_driver: Arc<dyn AdbDriver>) {
        let _swap = self.remote_driver_swap.lock().await;
        struct Resume<'a>(&'a crate::adb::remote_lifecycle::RemoteRegistry);
        impl Drop for Resume<'_> {
            fn drop(&mut self) {
                self.0.resume();
            }
        }
        self.remote_registry.pause();
        let _resume = Resume(&self.remote_registry);
        self.remote_registry.close_all().await;
        *self.adb.write().await = new_driver;
    }

    pub async fn capture_remote_generation(&self, serial: &str) -> Result<u64, String> {
        let _admission = self
            .try_adb_operation()
            .map_err(|error| error.to_string())?;
        self.remote_registry.capture(serial)
    }
    pub async fn begin_remote_generation(&self, serial: &str) -> Result<u64, String> {
        let _admission = self
            .try_adb_operation()
            .map_err(|error| error.to_string())?;
        self.remote_registry.begin(serial)
    }
    pub async fn validate_remote_generation(
        &self,
        serial: &str,
        generation: u64,
    ) -> Result<(), String> {
        self.remote_registry.validate(serial, generation)
    }
    pub async fn lock_remote_generation_operation(
        &self,
        serial: &str,
        generation: Option<u64>,
    ) -> Result<tokio::sync::OwnedMutexGuard<()>, String> {
        self.remote_registry.operation(serial, generation).await
    }
    pub async fn close_remote_generation(
        &self,
        serial: &str,
        generation: u64,
    ) -> Result<(), String> {
        self.remote_registry.close(serial, Some(generation)).await
    }

    pub async fn ensure_remote_session(
        &self,
        adb: Arc<dyn AdbDriver>,
        jar: &Path,
        serial: &str,
    ) -> Result<(), String> {
        self.ensure_remote_session_for_generation(adb, jar, serial, None)
            .await
    }
    pub async fn ensure_remote_session_for_generation(
        &self,
        adb: Arc<dyn AdbDriver>,
        jar: &Path,
        serial: &str,
        generation: Option<u64>,
    ) -> Result<(), String> {
        #[cfg(not(target_os = "android"))]
        const NATIVE_FAST_REMOTE_ENABLED: bool = false;
        let jar = jar.to_owned();
        let serial_owned = serial.to_owned();
        self.remote_registry.ensure(serial,generation,move |cancel|async move {
            #[cfg(target_os = "android")]
            let _ = &cancel;
            #[cfg(not(target_os = "android"))]
            if !adb.uses_host_server() {
                if !NATIVE_FAST_REMOTE_ENABLED {return Err("Fast Remote is unavailable for direct ADB until its lifecycle acceptance gates pass.".into());}
                let session=crate::adb::native_remote::NativeRemoteSession::start(adb,&jar,&serial_owned,cancel).await?;
                return Ok(Box::new(session) as Box<dyn crate::adb::remote_lifecycle::RemoteSessionIo>);
            }
            RemoteInputSession::start(adb,&jar,&serial_owned).await.map(|session|Box::new(session) as Box<dyn crate::adb::remote_lifecycle::RemoteSessionIo>)
        }).await
    }
    pub async fn remote_send_key(
        &self,
        serial: &str,
        keycode: u32,
        down: bool,
    ) -> Result<(), String> {
        self.remote_send_key_for_generation(serial, keycode, down, None)
            .await
    }
    pub async fn remote_send_key_for_generation(
        &self,
        serial: &str,
        keycode: u32,
        down: bool,
        generation: Option<u64>,
    ) -> Result<(), String> {
        self.remote_registry
            .send(
                serial,
                generation,
                crate::adb::remote_lifecycle::RemoteAction::Key(keycode, down),
            )
            .await
    }
    pub async fn remote_send_key_press(&self, serial: &str, keycode: u32) -> Result<(), String> {
        self.remote_send_key_press_for_generation(serial, keycode, None)
            .await
    }
    pub async fn remote_send_key_press_for_generation(
        &self,
        serial: &str,
        keycode: u32,
        generation: Option<u64>,
    ) -> Result<(), String> {
        self.remote_registry
            .send(
                serial,
                generation,
                crate::adb::remote_lifecycle::RemoteAction::Press(keycode),
            )
            .await
    }
    pub async fn remote_send_text(&self, serial: &str, text: &str) -> Result<(), String> {
        self.remote_send_text_for_generation(serial, text, None)
            .await
    }
    pub async fn remote_send_text_for_generation(
        &self,
        serial: &str,
        text: &str,
        generation: Option<u64>,
    ) -> Result<(), String> {
        self.remote_registry
            .send(
                serial,
                generation,
                crate::adb::remote_lifecycle::RemoteAction::Text(text.into()),
            )
            .await
    }
    pub async fn drop_remote_session(&self, serial: &str) {
        if let Err(error) = self.remote_registry.close(serial, None).await {
            tracing::warn!(%error,"Remote cleanup incomplete");
        }
    }
    pub async fn drop_all_remote_sessions(&self) {
        self.remote_registry.close_all().await;
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
/// The remote-input command calls this with its `AppHandle` to get the jar
/// path, then hands it to `AppState::ensure_remote_session`.
pub fn resolve_scrcpy_server_jar<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<PathBuf, String> {
    use tauri::Manager;
    #[cfg(target_os = "android")]
    {
        let path = app
            .path()
            .app_data_dir()
            .map_err(|e| format!("resolve app data directory: {e}"))?
            .join("scrcpy-server-v3.1");
        if path.is_file() {
            return Ok(path);
        }
        Err(format!("scrcpy server jar not found at {}", path.display()))
    }
    #[cfg(not(target_os = "android"))]
    {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::test_support::MockAdb;

    fn state() -> AppState {
        AppState::new(
            Arc::new(MockAdb::default()),
            AppListBundle::default(),
            std::env::temp_dir(),
        )
    }

    struct NoopCancel;
    impl crate::adb::AdbServiceCancellation for NoopCancel {
        fn cancel(&self) {}
    }
    struct ClosingSession {
        started: Arc<tokio::sync::Notify>,
        release: Arc<tokio::sync::Notify>,
    }
    #[async_trait::async_trait]
    impl crate::adb::remote_lifecycle::RemoteSessionIo for ClosingSession {
        fn cancellation(&self) -> Arc<dyn crate::adb::AdbServiceCancellation> {
            Arc::new(NoopCancel)
        }
        async fn send_key(&mut self, _: u32, _: bool) -> Result<(), String> {
            Ok(())
        }
        async fn send_key_press(&mut self, _: u32) -> Result<(), String> {
            Ok(())
        }
        async fn send_text(&mut self, _: &str) -> Result<(), String> {
            Ok(())
        }
        async fn close(self: Box<Self>) -> Result<(), String> {
            self.started.notify_one();
            self.release.notified().await;
            Ok(())
        }
    }
    async fn blocking_session(
        state: &AppState,
    ) -> (Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>) {
        let started = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        let generation = state.remote_registry.begin("A").unwrap();
        let session = ClosingSession {
            started: started.clone(),
            release: release.clone(),
        };
        state
            .remote_registry
            .ensure("A", Some(generation), move |_| async move {
                Ok(Box::new(session) as Box<dyn crate::adb::remote_lifecycle::RemoteSessionIo>)
            })
            .await
            .unwrap();
        (started, release)
    }
    #[tokio::test]
    async fn cancelled_driver_swap_preserves_driver_and_resumes_remote_admission() {
        let state = Arc::new(state());
        let original = state.adb.read().await.clone();
        let (started, release) = blocking_session(&state).await;
        let swap = tokio::spawn({
            let state = state.clone();
            async move { state.replace_adb(Arc::new(MockAdb::default())).await }
        });
        started.notified().await;
        swap.abort();
        assert!(swap.await.unwrap_err().is_cancelled());
        assert!(Arc::ptr_eq(&original, &*state.adb.read().await));
        assert!(state.begin_remote_generation("B").await.is_ok());
        release.notify_one();
        state.drop_all_remote_sessions().await;
        state.close_adb_admission();
        state.replace_adb(Arc::new(MockAdb::default())).await;
        assert!(state.begin_remote_generation("closed").await.is_err());
    }
    #[tokio::test]
    async fn concurrent_driver_swaps_commit_in_serialized_order() {
        let state = Arc::new(state());
        let (started, release) = blocking_session(&state).await;
        let first: Arc<dyn AdbDriver> = Arc::new(MockAdb::default());
        let second: Arc<dyn AdbDriver> = Arc::new(MockAdb::default());
        let a = tokio::spawn({
            let state = state.clone();
            async move { state.replace_adb(first).await }
        });
        started.notified().await;
        let b = tokio::spawn({
            let state = state.clone();
            let second = second.clone();
            async move { state.replace_adb(second).await }
        });
        tokio::task::yield_now().await;
        assert!(!b.is_finished());
        release.notify_one();
        a.await.unwrap();
        b.await.unwrap();
        assert!(Arc::ptr_eq(&second, &*state.adb.read().await));
        assert!(state.begin_remote_generation("B").await.is_ok());
    }

    #[tokio::test]
    async fn a_driver_without_a_server_skips_stop_without_dispatching_commands() {
        let mock = MockAdb::default();
        let raw = mock.raw_log();
        let shell = mock.shell_log();
        let state = AppState::new(
            Arc::new(mock),
            AppListBundle::default(),
            std::env::temp_dir(),
        );
        assert!(matches!(
            state.stop_adb_server(std::time::Duration::ZERO).await,
            ServerStop::Skipped(_)
        ));
        assert!(matches!(
            state
                .adb_snapshot()
                .await
                .stop_server(std::time::Duration::ZERO)
                .await,
            ServerStop::Skipped(_)
        ));
        assert!(raw.lock().unwrap().is_empty());
        assert!(shell.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn every_snapshot_counts_until_its_last_clone_goes() {
        let state = state();
        let snapshot = state.adb_snapshot().await;
        let kept_by_a_session = Arc::clone(&snapshot);
        assert_eq!(state.adb_operations_active(), 1);
        drop(snapshot);
        assert_eq!(state.adb_operations_active(), 1);
        drop(kept_by_a_session);
        assert_eq!(state.adb_operations_active(), 0);
    }

    #[tokio::test]
    async fn snapshots_of_a_swapped_driver_share_one_count() {
        let state = state();
        let old = state.adb_snapshot().await;
        state.replace_adb(Arc::new(MockAdb::default())).await;
        let new = state.adb_snapshot().await;
        assert_eq!(state.adb_operations_active(), 2);
        drop((old, new));
        assert_eq!(state.adb_operations_active(), 0);
    }

    #[tokio::test]
    async fn closing_admission_refuses_new_snapshots_but_not_running_ones() {
        let mock = MockAdb::default();
        let calls = mock.raw_log();
        let state = AppState::new(
            Arc::new(mock),
            AppListBundle::default(),
            std::env::temp_dir(),
        );
        let running = state.adb_snapshot().await;
        state.close_adb_admission();
        assert!(running.raw(&["devices"]).await.is_ok());
        let late = state.adb_snapshot().await.raw(&["devices"]).await;
        assert!(matches!(late, Err(AdbError::ShuttingDown)));
        assert_eq!(calls.lock().unwrap().as_slice(), ["devices"]);
    }
}

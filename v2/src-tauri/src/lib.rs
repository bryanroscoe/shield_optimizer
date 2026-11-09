//! ATV Optimizer — desktop Tauri entry point.
//!
//! Shared engine and driver-generic commands live in `atv_optimizer_core`.
//! This crate keeps the desktop-only subprocess ADB driver, installer/updater,
//! host-network scan, and file-path based commands.

// clippy's `double_must_use` fires on the futures that
// `#[async_trait]` generates for every trait method. The attribute is the
// macro's, not ours, so there is nothing to change at the call sites.
#![allow(clippy::double_must_use)]
pub mod adb;
pub mod commands;
pub mod rename;
pub mod session;
pub use atv_optimizer_core::engine;

use std::path::PathBuf;
use std::sync::Arc;

use adb::SubprocessAdb;
use atv_optimizer_core::adb::{AdbDriver, AdbError, AdbOutput, AdbResult, Leases};
use atv_optimizer_core::commands::{
    apps, devices, health, input, launcher, loader, logcat, optimize, reboot, recovery, screenshot,
    shell, snapshot, telemetry, tuning,
};
use atv_optimizer_core::engine::telemetry::AppKind;
use atv_optimizer_core::license::Entitlement;
use atv_optimizer_core::telemetry::{
    install_panic_hook, load_cached_support_rules, spawn_support_refresh, Telemetry,
    TelemetryConfig,
};
use commands::adb_engine::{self, AdbEngineControl, EngineStarter};
use commands::quit::{self, AdbQuitControl};
use commands::{
    app_report, backup, diagnostics, files, install, local_network, scan, sideload, update,
    AppState,
};
use tauri::Manager;

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

fn subprocess_driver() -> Arc<dyn AdbDriver> {
    match adb::cached_adb_binary() {
        Some(path) => {
            tracing::info!(adb = %path.display(), "adb located");
            Arc::new(SubprocessAdb::new(path))
        }
        None => {
            tracing::warn!("no adb binary located; commands will return BinaryNotFound");
            Arc::new(NoAdbDriver)
        }
    }
}
struct ConfigurationError(String);

#[async_trait::async_trait]
impl AdbDriver for ConfigurationError {
    fn uses_host_server(&self) -> bool {
        false
    }
    async fn raw(&self, _args: &[&str]) -> AdbResult<AdbOutput> {
        Err(AdbError::Initialization(self.0.clone()))
    }
    async fn shell(&self, _serial: &str, _command: &str) -> AdbResult<AdbOutput> {
        Err(AdbError::Initialization(self.0.clone()))
    }
    async fn discover_network(&self) -> AdbResult<atv_optimizer_core::adb::NativeDiscoveryReport> {
        Err(AdbError::Initialization(self.0.clone()))
    }
    async fn pull_limited(&self, _: &str, _: &str, _: &std::path::Path, _: u64) -> AdbResult<()> {
        Err(AdbError::Initialization(self.0.clone()))
    }
    async fn shell_bounded(
        &self,
        _: &str,
        _: &str,
    ) -> AdbResult<atv_optimizer_core::adb::BoundedShellOutput> {
        Err(AdbError::Initialization(self.0.clone()))
    }
    async fn raw_bytes(&self, _: &[&str]) -> AdbResult<Vec<u8>> {
        Err(AdbError::Initialization(self.0.clone()))
    }
    async fn device_binding(
        &self,
        _: &str,
    ) -> AdbResult<atv_optimizer_core::adb::AdbDeviceBinding> {
        Err(AdbError::Initialization(self.0.clone()))
    }
    async fn open_bound_device_service(
        &self,
        _: &atv_optimizer_core::adb::AdbDeviceBinding,
        _: &str,
        _: std::time::Duration,
    ) -> AdbResult<atv_optimizer_core::adb::AdbOwnedService> {
        Err(AdbError::Initialization(self.0.clone()))
    }
    async fn open_device_service(
        &self,
        _: &str,
        _: &str,
    ) -> AdbResult<Box<dyn atv_optimizer_core::adb::AdbByteStream>> {
        Err(AdbError::Initialization(self.0.clone()))
    }
    async fn spawn(&self, _: &[&str]) -> AdbResult<atv_optimizer_core::adb::TrackedChild> {
        Err(AdbError::Initialization(self.0.clone()))
    }
}

/// The native identity lives in the session's data folder, which
/// `rename.rs` migrates with its `native-adb/` key. When the OS reports no
/// local data folder, `migrate_host` falls back to the working directory;
/// a key must never be created there.
fn identity_app_data(
    local_data: Option<PathBuf>,
    data_dir: &std::path::Path,
) -> Result<PathBuf, String> {
    match local_data {
        Some(root) if data_dir.starts_with(&root) => Ok(data_dir.to_path_buf()),
        _ => Err("Rust ADB could not find the app storage folder. Reopen the app after restoring access to your user profile. Existing identities were not replaced.".to_string()),
    }
}

/// Builds the desktop's two engines, at launch and on a Settings switch.
struct DesktopEngines {
    data_dir: PathBuf,
    /// The OS local data folder, read once at launch.
    local_data: Option<PathBuf>,
    /// `ATV_OPTIMIZER_DIRECT_KEY`: an existing identity to use instead, and
    /// then the only one the built-in engine offers.
    key_override: Option<PathBuf>,
    /// Where Google's adb keeps its key, located once at launch. The
    /// built-in engine reads it each time it starts, never writing it.
    platform_tools_key: Option<PathBuf>,
}

impl DesktopEngines {
    fn from_environment(data_dir: PathBuf) -> Self {
        Self {
            data_dir,
            local_data: dirs::data_local_dir(),
            key_override: std::env::var_os("ATV_OPTIMIZER_DIRECT_KEY").map(PathBuf::from),
            #[cfg(feature = "direct-adb")]
            platform_tools_key: adb::identity::platform_tools_key_path(),
            #[cfg(not(feature = "direct-adb"))]
            platform_tools_key: None,
        }
    }

    /// The key the built-in engine uses (or would create), which platform-tools
    /// started by this app also offers, so a TV that allowed only the
    /// built-in engine needs no new prompt there either.
    fn builtin_key_path(&self) -> Option<PathBuf> {
        match &self.key_override {
            Some(path) => Some(path.clone()),
            None => identity_app_data(self.local_data.clone(), &self.data_dir)
                .ok()
                .map(|root| root.join("native-adb").join("identity.pkcs8.pem")),
        }
    }
}

impl EngineStarter for DesktopEngines {
    fn platform_tools(&self) -> Arc<dyn AdbDriver> {
        subprocess_driver()
    }

    fn builtin(&self, operations: &Arc<Leases>) -> Result<Arc<dyn AdbDriver>, String> {
        #[cfg(feature = "direct-adb")]
        {
            let identity = match &self.key_override {
                Some(path) => {
                    adb::identity::validate_existing_identity(path).map(|()| path.clone())
                }
                None => identity_app_data(self.local_data.clone(), &self.data_dir)
                    .and_then(|root| adb::identity::ensure_app_identity(&root)),
            };
            // An explicit key override is a diagnostic choice: that key alone.
            let preferred = match &self.key_override {
                Some(_) => atv_direct_adb::PreferredKeys::none(),
                None => adb::identity::load_platform_tools_key(self.platform_tools_key.as_deref()),
            };
            match identity {
                Ok(key) => Ok(Arc::new(
                    adb::direct::DirectNetworkAdb::with_shared_operations_and_keys(
                        key,
                        preferred,
                        Arc::clone(operations),
                    ),
                )),
                Err(message) => Err(format!("The built-in engine couldn't set up its key, so this session uses Google platform-tools. Existing keys were not replaced. Details: {message}")),
            }
        }
        #[cfg(not(feature = "direct-adb"))]
        {
            let _ = (
                operations,
                &self.data_dir,
                &self.local_data,
                &self.key_override,
                &self.platform_tools_key,
            );
            Err(
                "This build doesn't include the built-in engine, so it uses Google platform-tools."
                    .into(),
            )
        }
    }
}

async fn default_state(
    app_lists: engine::AppListBundle,
    data_dir: PathBuf,
) -> (AppState, AdbEngineControl) {
    let env = std::env::var(adb::selection::TRANSPORT_ENV).ok();
    let engines = DesktopEngines::from_environment(data_dir.clone());
    adb::driver::offer_builtin_key_to_adb_server(engines.builtin_key_path());
    let (control, driver) =
        AdbEngineControl::start(data_dir.clone(), env.as_deref(), Arc::new(engines));
    let driver: Arc<dyn AdbDriver> = match driver {
        Ok(driver) => driver,
        Err(message) => Arc::new(ConfigurationError(message)),
    };
    let state = control.app_state(driver, app_lists, data_dir).await;
    // The desktop app has no paywall; every feature is unlocked.
    let state = state
        .with_entitlement(Entitlement::Pro)
        .with_cast_names(Arc::new(atv_optimizer_core::net::CastNames::live()));
    (state, control)
}

/// Every command the desktop frontend can invoke. One list, shared by the app
/// and the dev-only E2E server (`crates/e2e-server`), so the harness
/// dispatches through exactly the handler table the shipped app uses.
pub fn invoke_handler<R: tauri::Runtime>(
) -> impl Fn(tauri::ipc::Invoke<R>) -> bool + Send + Sync + 'static {
    tauri::generate_handler![
        devices::list_devices,
        devices::device_profile,
        devices::connect_device,
        devices::disconnect_device,
        devices::pair_device,
        devices::forget_device,
        devices::reprompt_device,
        devices::request_device_approval,
        devices::cancel_device_approval,
        devices::transport_state,
        devices::probe_paired_connect,
        devices::connect_paired,
        scan::local_address_for,
        devices::rename_device,
        health::health_report,
        health::media_report,
        health::resource_sample,
        health::app_list_for_device,
        health::report_all,
        install::adb_status,
        install::install_adb,
        install::restart_adb,
        adb_engine::adb_engine,
        adb_engine::set_adb_engine,
        local_network::local_network_access,
        local_network::open_local_network_settings,
        quit::get_stop_adb_on_quit,
        quit::set_stop_adb_on_quit,
        scan::scan_network,
        launcher::list_launchers,
        launcher::current_launcher,
        launcher::channel_provider_disabled,
        launcher::set_default_launcher,
        launcher::disable_launcher,
        launcher::set_home_any,
        launcher::disable_stock_launcher,
        launcher::disable_setup_helper,
        apps::disable_package,
        apps::enable_package,
        apps::force_stop,
        screenshot::take_screenshot,
        apps::uninstall_package,
        apps::reinstall_existing,
        apps::open_play_store,
        apps::package_states,
        apps::list_other_packages,
        apps::list_installed_packages,
        apps::app_memory_map,
        apps::app_storage_map,
        apps::app_apk_size,
        apps::app_usage_map,
        apps::safety_info,
        apps::process_safety_info,
        apps::trim_caches,
        apps::app_permission_state,
        apps::set_app_permission,
        apps::set_app_op,
        apps::get_app_op,
        input::remote_open,
        input::remote_close,
        input::send_text,
        input::send_key,
        input::open_settings,
        sideload::install_apk,
        sideload::list_apks_in_folder,
        sideload::inspect_apk,
        backup::backup_apk,
        backup::clone_app,
        files::list_dir,
        files::pull_file,
        files::push_file,
        files::delete_path,
        files::find_files,
        files::copy_file_to_device,
        snapshot::list_snapshots,
        snapshot::save_snapshot,
        snapshot::preview_apply,
        snapshot::apply_snapshot,
        snapshot::delete_snapshot,
        snapshot::snapshot_dir_path,
        recovery::panic_recovery,
        reboot::reboot_device,
        shell::run_shell,
        logcat::read_device_logs,
        tuning::get_tweaks,
        tuning::write_setting,
        tuning::set_display_scaling,
        tuning::get_display_scaling,
        tuning::get_private_dns,
        tuning::set_private_dns,
        optimize::prepare_optimize,
        optimize::apply_performance_settings,
        update::check_for_update,
        update::update_channel,
        app_report::save_app_report,
        diagnostics::collect_diagnostics,
        diagnostics::upload_diagnostics,
        diagnostics::set_debug_logging,
        diagnostics::get_debug_logging,
        diagnostics::log_dir_path,
        diagnostics::open_log_dir,
        diagnostics::log_ui_event,
        telemetry::telemetry_settings,
        telemetry::set_telemetry_enabled,
        telemetry::dismiss_telemetry_notice,
        telemetry::telemetry_action,
        telemetry::telemetry_error,
    ]
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Some Wayland/Mesa setups crash WebKitGTK's DMABUF renderer on startup
    // ("Could not create default EGL display: EGL_BAD_PARAMETER"), leaving a
    // blank window. Disabling that renderer falls back to a path that works
    // everywhere. Honor an existing value so power users can still force it on.
    #[cfg(target_os = "linux")]
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }

    // Before logging, so the log opens in the new data folder with the old
    // logs already beside it, and before any webview exists, so the copied
    // profile is the one it opens. The data folder is `ATVOptimizer` under
    // the local data dir (`~/Library/Application Support`, `%LOCALAPPDATA%`,
    // `~/.local/share`), or the old `ShieldOptimizer` for a session whose
    // copy failed, so the next launch tries again.
    let migration = rename::migrate_host();
    let data_dir = migration.data_dir.clone();
    rename::set_session_data_dir(data_dir.clone());

    // stdout *and* a rolling daily file under the data dir. A packaged app has
    // nowhere for stdout to go, so the file is the only copy that survives on
    // the machines where a bug actually happens.
    let log_control = diagnostics::init_logging(&data_dir);
    session::install(log_control.dir(), log_control.debug_enabled());
    migration.log();

    let app_lists = match loader::load_embedded_app_lists() {
        Ok(lists) => {
            tracing::info!(total = lists.total(), "app lists loaded");
            lists
        }
        Err(e) => {
            // Surface the build-time mistake but don't crash the GUI — let the
            // frontend show an empty state.
            tracing::error!(error = %e, "failed to load embedded app lists");
            engine::AppListBundle::default()
        }
    };

    // Anonymous reports: a complete no-op unless this build was compiled with
    // ATVOPT_TELEMETRY_URL, and the user can turn them off in Settings.
    let telemetry = Telemetry::new(
        TelemetryConfig::for_build(AppKind::Desktop, env!("CARGO_PKG_VERSION")),
        &data_dir,
    );
    install_panic_hook(telemetry.clone());
    // Failure and "found nothing" reports carry a redacted excerpt of the
    // adb and connection log lines, a few times a day at most.
    telemetry.set_excerpt_source(diagnostics::excerpt_source(log_control.dir().to_path_buf()));

    let (state, engine_control) =
        tauri::async_runtime::block_on(default_state(app_lists, data_dir));
    let state = state
        .with_known_names(loader::load_known_names())
        .with_telemetry(telemetry.clone());
    load_cached_support_rules(&state);

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(state)
        .manage(log_control)
        .manage(AdbQuitControl::default())
        .manage(engine_control)
        .setup(move |app| {
            let telemetry_app = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let state = telemetry_app.state::<AppState>();
                let adb = state.adb_snapshot().await;
                telemetry.set_adb_facts(adb_engine::adb_facts(adb.uses_host_server()));
                telemetry.start();
            });
            spawn_support_refresh(app.handle().clone());
            Ok(())
        })
        .invoke_handler(invoke_handler())
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| match event {
            // An update restart arrives as this exit code, ahead of `Exit`.
            tauri::RunEvent::ExitRequested {
                code: Some(tauri::RESTART_EXIT_CODE),
                ..
            } => app.state::<AdbQuitControl>().note_restart(),
            tauri::RunEvent::Exit => {
                let state = app.state::<AppState>();
                let control = app.state::<AdbQuitControl>();
                tauri::async_runtime::block_on(quit::stop_adb_server_on_quit(
                    state.inner(),
                    control.inner(),
                ));
                #[cfg(target_os = "macos")]
                if !control.is_restarting() {
                    rename_bundle_on_exit();
                }
                // The file writer is non-blocking; without this the last
                // lines (the quit stop, the bundle rename) can be lost.
                app.state::<diagnostics::LogControl>().flush_and_close();
            }
            _ => {}
        });
}

/// Give a bundle the updater left at `Shield Optimizer.app` the new name.
#[cfg(target_os = "macos")]
fn rename_bundle_on_exit() {
    let (Ok(exe), Some(home)) = (std::env::current_exe(), dirs::home_dir()) else {
        return;
    };
    let caskrooms: Vec<&std::path::Path> =
        rename::CASKROOMS.iter().map(std::path::Path::new).collect();
    match rename::rename_legacy_bundle(&exe, &home, &caskrooms) {
        rename::BundleRename::NotABundle | rename::BundleRename::NotLegacyName => {}
        outcome => tracing::info!(outcome = ?outcome, "legacy bundle rename"),
    }
}

#[cfg(test)]
mod initialization_tests {
    use super::*;

    fn engines(data_dir: PathBuf, local_data: Option<PathBuf>) -> DesktopEngines {
        DesktopEngines {
            data_dir,
            local_data,
            key_override: None,
            platform_tools_key: None,
        }
    }

    fn refusal(engines: &DesktopEngines) -> String {
        match engines.builtin(&Leases::new()) {
            Ok(_) => panic!("the built-in engine should not start here"),
            Err(reason) => reason,
        }
    }

    #[cfg(not(feature = "direct-adb"))]
    #[test]
    fn a_build_without_the_builtin_engine_says_so() {
        let reason = refusal(&engines(PathBuf::from("not-created"), None));
        assert!(reason.contains("doesn't include the built-in engine"));
    }

    #[cfg(feature = "direct-adb")]
    #[test]
    fn builtin_that_cannot_set_up_its_key_refuses_without_writing_one() {
        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path().join("ATVOptimizer");

        // No OS data folder: never the working-directory fallback.
        let reason = refusal(&engines(data_dir.clone(), None));
        assert!(reason.contains("couldn't set up its key"), "{reason}");
        assert!(reason.contains("could not find the app storage folder"));
        assert!(!data_dir.exists());

        // A data folder that cannot hold a key.
        std::fs::write(&data_dir, b"fixture file in place of the folder").unwrap();
        let reason = refusal(&engines(data_dir.clone(), Some(dir.path().to_path_buf())));
        assert!(
            reason.contains("Existing keys were not replaced"),
            "{reason}"
        );
        assert_eq!(
            std::fs::read(&data_dir).unwrap(),
            b"fixture file in place of the folder"
        );

        // A named identity that does not exist is never created.
        let missing = dir.path().join("missing.pem");
        let named = DesktopEngines {
            key_override: Some(missing.clone()),
            ..engines(dir.path().join("unused"), Some(dir.path().to_path_buf()))
        };
        refusal(&named);
        assert!(!missing.exists());
    }

    #[cfg(feature = "direct-adb")]
    #[tokio::test]
    async fn a_saved_builtin_choice_falls_back_to_platform_tools_when_its_key_fails() {
        let dir = tempfile::tempdir().unwrap();
        adb::selection::save(dir.path(), adb::selection::AdbEngine::Builtin).unwrap();
        let (control, driver) = AdbEngineControl::start(
            dir.path().to_path_buf(),
            None,
            Arc::new(engines(dir.path().to_path_buf(), None)),
        );
        let status = control.status().await;
        assert_eq!(status.selected, Some(adb::selection::AdbEngine::Builtin));
        assert_eq!(
            status.active,
            Some(adb::selection::AdbEngine::PlatformTools)
        );
        assert!(status
            .fallback_reason
            .unwrap()
            .contains("couldn't set up its key"));
        let Ok(driver) = driver else {
            panic!("a fallback is a working driver, not a configuration error")
        };
        assert!(driver.uses_host_server());
        assert!(!dir.path().join("native-adb").exists());
    }

    #[cfg(feature = "direct-adb")]
    #[test]
    fn missing_app_data_never_falls_back_to_the_working_directory() {
        let session = std::path::Path::new("/fixture-profile/ATVOptimizer");
        assert!(identity_app_data(None, session).is_err());
        assert_eq!(
            identity_app_data(Some(std::path::PathBuf::from("/fixture-profile")), session).unwrap(),
            session
        );
        let working_directory = std::path::Path::new(".").join("ATVOptimizer");
        assert!(identity_app_data(
            Some(std::path::PathBuf::from("/fixture-profile")),
            &working_directory
        )
        .is_err());
    }

    #[tokio::test]
    async fn initialization_failure_is_preserved_across_native_operations() {
        let driver = ConfigurationError("Rust ADB setup could not save its identity. Check app storage access and reopen the app. Existing identities were not replaced.".into());
        let expected = driver.0.clone();
        let binding = atv_optimizer_core::adb::AdbDeviceBinding {
            serial: "fixture".into(),
            hardware_id: "fixture-id".into(),
        };
        assert!(!driver.uses_host_server());
        assert_eq!(
            driver.raw(&["devices"]).await.unwrap_err().to_string(),
            expected
        );
        assert_eq!(
            driver
                .raw_transfer(&["push"])
                .await
                .unwrap_err()
                .to_string(),
            expected
        );
        assert_eq!(
            driver
                .shell("fixture", "ignored")
                .await
                .unwrap_err()
                .to_string(),
            expected
        );
        assert_eq!(
            driver.discover_network().await.unwrap_err().to_string(),
            expected
        );
        assert_eq!(
            driver
                .pull_limited("fixture", "ignored", std::path::Path::new("not-created"), 1)
                .await
                .unwrap_err()
                .to_string(),
            expected
        );
        assert_eq!(
            driver
                .shell_bounded("fixture", "ignored")
                .await
                .unwrap_err()
                .to_string(),
            expected
        );
        assert_eq!(
            driver.raw_bytes(&[]).await.unwrap_err().to_string(),
            expected
        );
        assert_eq!(
            driver
                .device_binding("fixture")
                .await
                .unwrap_err()
                .to_string(),
            expected
        );
        assert!(
            matches!(driver.open_device_service("fixture", "ignored").await, Err(AdbError::Initialization(message)) if message == driver.0)
        );
        assert!(
            matches!(driver.open_bound_device_service(&binding, "ignored", std::time::Duration::from_secs(1)).await, Err(AdbError::Initialization(message)) if message == driver.0)
        );
        assert!(
            matches!(driver.spawn(&[]).await, Err(AdbError::Initialization(message)) if message == driver.0)
        );
        assert!(matches!(
            driver.stop_server(std::time::Duration::from_secs(1)).await,
            atv_optimizer_core::adb::ServerStop::Skipped(_)
        ));
    }
}

/// The built-in engine started by the desktop offers platform-tools' key
/// before its own, end to end through a loopback adbd.
#[cfg(all(test, unix, feature = "direct-adb"))]
mod builtin_key_tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};

    fn send(socket: &mut TcpStream, command: &[u8; 4], arg0: u32, arg1: u32, payload: &[u8]) {
        let command = u32::from_le_bytes(*command);
        for field in [
            command,
            arg0,
            arg1,
            payload.len() as u32,
            payload.iter().map(|b| u32::from(*b)).sum(),
            command ^ u32::MAX,
        ] {
            socket.write_all(&field.to_le_bytes()).unwrap();
        }
        socket.write_all(payload).unwrap();
    }

    fn receive(socket: &mut TcpStream) -> ([u8; 4], u32, Vec<u8>) {
        let mut header = [0; 24];
        socket.read_exact(&mut header).unwrap();
        let size = u32::from_le_bytes(header[12..16].try_into().unwrap()) as usize;
        let mut payload = vec![0; size];
        socket.read_exact(&mut payload).unwrap();
        (
            header[..4].try_into().unwrap(),
            u32::from_le_bytes(header[4..8].try_into().unwrap()),
            payload,
        )
    }

    /// The AUTH signature `key` makes over `token` (PKCS#1 v1.5 over SHA-1's
    /// DigestInfo, which is deterministic).
    fn signature(key: &rsa::RsaPrivateKey, token: &[u8; 20]) -> Vec<u8> {
        let mut digest_info = vec![
            0x30, 0x21, 0x30, 0x09, 0x06, 0x05, 0x2b, 0x0e, 0x03, 0x02, 0x1a, 0x05, 0x00, 0x04,
            0x14,
        ];
        digest_info.extend_from_slice(token);
        key.sign(rsa::Pkcs1v15Sign::new_unprefixed(), &digest_info)
            .unwrap()
    }

    /// One adbd connection that wants `signers` to sign its AUTH tokens in
    /// that order, then answers the liveness `echo ok`.
    fn adbd(listener: TcpListener, signers: Vec<rsa::RsaPrivateKey>) {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(std::time::Duration::from_secs(10)))
            .unwrap();
        assert_eq!(&receive(&mut socket).0, b"CNXN");
        for (round, signer) in signers.iter().enumerate() {
            let token = [round as u8 + 1; 20];
            send(&mut socket, b"AUTH", 1, 0, &token);
            let (command, kind, payload) = receive(&mut socket);
            assert_eq!((&command, kind), (b"AUTH", 2));
            assert_eq!(
                payload,
                signature(signer, &token),
                "AUTH round {round}: wrong key"
            );
        }
        send(
            &mut socket,
            b"CNXN",
            0x0100_0000,
            1 << 20,
            b"device::features=shell_v2\0",
        );
        let (command, local, destination) = receive(&mut socket);
        assert_eq!(&command, b"OPEN");
        assert_eq!(destination, b"shell,v2,raw:echo ok\0");
        send(&mut socket, b"OKAY", 1, local, &[]);
        for packet in [
            [&[1, 3, 0, 0, 0][..], b"ok\n"].concat(),
            vec![3, 1, 0, 0, 0, 0],
        ] {
            send(&mut socket, b"WRTE", 1, local, &packet);
            assert_eq!(&receive(&mut socket).0, b"OKAY");
        }
        send(&mut socket, b"CLSE", 1, local, &[]);
        let _ = std::io::copy(&mut socket, &mut std::io::sink());
    }

    #[tokio::test]
    async fn builtin_offers_the_platform_tools_key_first_and_never_changes_it() {
        use rsa::pkcs1::EncodeRsaPrivateKey;
        use rsa::pkcs8::DecodePrivateKey;
        let dir = tempfile::tempdir().unwrap();
        let android = dir.path().join("home").join(".android");
        std::fs::create_dir_all(&android).unwrap();
        let adbkey = android.join("adbkey");
        let platform_tools = rsa::RsaPrivateKey::new(&mut rsa::rand_core::OsRng, 2048).unwrap();
        std::fs::write(
            &adbkey,
            platform_tools
                .to_pkcs1_pem(rsa::pkcs1::LineEnding::LF)
                .unwrap()
                .as_bytes(),
        )
        .unwrap();
        let adbkey_before = std::fs::read(&adbkey).unwrap();
        let engines = DesktopEngines {
            data_dir: dir.path().join("ATVOptimizer"),
            local_data: Some(dir.path().to_path_buf()),
            key_override: None,
            platform_tools_key: Some(adbkey.clone()),
        };
        let driver = engines.builtin(&Leases::new()).unwrap();
        let app_key_path = engines.builtin_key_path().unwrap();
        let app_key = rsa::RsaPrivateKey::from_pkcs8_pem(
            &std::fs::read_to_string(&app_key_path).expect("builtin_key_path names the app key"),
        )
        .unwrap();

        // The platform-tools key signs the first token, the app key the next.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("127.0.0.1:{}", listener.local_addr().unwrap().port());
        let signers = vec![platform_tools.clone(), app_key.clone()];
        let peer = std::thread::spawn(move || adbd(listener, signers));
        driver.raw(&["connect", &address]).await.unwrap();
        drop(driver);
        peer.join().unwrap();

        // An explicit key override is used alone.
        let engines = DesktopEngines {
            key_override: Some(app_key_path.clone()),
            ..engines
        };
        let driver = engines.builtin(&Leases::new()).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("127.0.0.1:{}", listener.local_addr().unwrap().port());
        let peer = std::thread::spawn(move || adbd(listener, vec![app_key]));
        driver.raw(&["connect", &address]).await.unwrap();
        drop(driver);
        peer.join().unwrap();

        assert_eq!(std::fs::read(&adbkey).unwrap(), adbkey_before);
        let names: Vec<_> = std::fs::read_dir(&android)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(names, vec![std::ffi::OsString::from("adbkey")]);
    }
}

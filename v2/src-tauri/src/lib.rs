//! Shield Optimizer v2 — desktop Tauri entry point.
//!
//! Shared engine and driver-generic commands live in `shield_optimizer_core`.
//! This crate keeps the desktop-only subprocess ADB driver, installer/updater,
//! host-network scan, and file-path based commands.

// clippy's `double_must_use` fires on the futures that
// `#[async_trait]` generates for every trait method. The attribute is the
// macro's, not ours, so there is nothing to change at the call sites.
#![allow(clippy::double_must_use)]
pub mod adb;
pub mod commands;
pub mod session;
pub use shield_optimizer_core::engine;

use std::path::PathBuf;
use std::sync::Arc;

use adb::SubprocessAdb;
use commands::quit::{self, AdbQuitControl};
use commands::{app_report, backup, diagnostics, files, install, scan, sideload, update, AppState};
use shield_optimizer_core::adb::{AdbDriver, AdbError, AdbOutput, AdbResult};
use shield_optimizer_core::commands::{
    apps, devices, health, input, launcher, loader, logcat, optimize, reboot, recovery, screenshot,
    shell, snapshot, telemetry, tuning,
};
use shield_optimizer_core::engine::telemetry::AppKind;
use shield_optimizer_core::license::Entitlement;
use shield_optimizer_core::telemetry::{
    install_panic_hook, load_cached_support_rules, spawn_support_refresh, Telemetry,
    TelemetryConfig,
};
use tauri::Manager;

/// Resolve the OS-appropriate app data root (snapshots live in a `snapshots`
/// subdirectory).
///
/// macOS: `~/Library/Application Support/ShieldOptimizer`
/// Linux: `~/.local/share/ShieldOptimizer`
/// Windows: `%LOCALAPPDATA%/ShieldOptimizer`
fn default_data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("ShieldOptimizer")
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
    async fn discover_network(
        &self,
    ) -> AdbResult<shield_optimizer_core::adb::NativeDiscoveryReport> {
        Err(AdbError::Initialization(self.0.clone()))
    }
    async fn pull_limited(&self, _: &str, _: &str, _: &std::path::Path, _: u64) -> AdbResult<()> {
        Err(AdbError::Initialization(self.0.clone()))
    }
    async fn shell_bounded(
        &self,
        _: &str,
        _: &str,
    ) -> AdbResult<shield_optimizer_core::adb::BoundedShellOutput> {
        Err(AdbError::Initialization(self.0.clone()))
    }
    async fn raw_bytes(&self, _: &[&str]) -> AdbResult<Vec<u8>> {
        Err(AdbError::Initialization(self.0.clone()))
    }
    async fn device_binding(
        &self,
        _: &str,
    ) -> AdbResult<shield_optimizer_core::adb::AdbDeviceBinding> {
        Err(AdbError::Initialization(self.0.clone()))
    }
    async fn open_bound_device_service(
        &self,
        _: &shield_optimizer_core::adb::AdbDeviceBinding,
        _: &str,
        _: std::time::Duration,
    ) -> AdbResult<shield_optimizer_core::adb::AdbOwnedService> {
        Err(AdbError::Initialization(self.0.clone()))
    }
    async fn open_device_service(
        &self,
        _: &str,
        _: &str,
    ) -> AdbResult<Box<dyn shield_optimizer_core::adb::AdbByteStream>> {
        Err(AdbError::Initialization(self.0.clone()))
    }
    async fn spawn(&self, _: &[&str]) -> AdbResult<shield_optimizer_core::adb::TrackedChild> {
        Err(AdbError::Initialization(self.0.clone()))
    }
}

#[cfg(feature = "direct-adb")]
fn identity_app_data(root: Option<PathBuf>) -> Result<PathBuf, String> {
    root.map(|path| path.join("ShieldOptimizer")).ok_or_else(||
        "Rust ADB could not find the app storage folder. Reopen the app after restoring access to your user profile. Existing identities were not replaced.".to_string())
}

fn default_state(app_lists: engine::AppListBundle, data_dir: PathBuf) -> AppState {
    let selection = adb::selection::parse_selection(
        std::env::var("SHIELD_OPTIMIZER_TRANSPORT").ok().as_deref(),
    );
    let adb: Arc<dyn AdbDriver> = match selection {
        Ok(adb::selection::TransportSelection::Subprocess) => subprocess_driver(),
        Ok(adb::selection::TransportSelection::Direct) => {
            #[cfg(feature = "direct-adb")]
            {
                let identity = match std::env::var_os("SHIELD_OPTIMIZER_DIRECT_KEY") {
                    Some(path) => {
                        let path = PathBuf::from(path);
                        adb::identity::validate_existing_identity(&path).map(|()| path)
                    }
                    None => identity_app_data(dirs::data_local_dir())
                        .and_then(|root| adb::identity::ensure_app_identity(&root)),
                };
                match identity {
                    Ok(key) => Arc::new(adb::direct::DirectNetworkAdb::new(key)),
                    Err(message) => Arc::new(ConfigurationError(format!("Rust ADB setup could not finish. Check app storage access and reopen the app. Existing identities were not replaced. Details: {message}"))),
                }
            }
            #[cfg(not(feature = "direct-adb"))]
            {
                Arc::new(ConfigurationError("Direct ADB was selected but this build lacks the experimental direct-adb feature.".into()))
            }
        }
        Err(message) => Arc::new(ConfigurationError(message)),
    };
    // The desktop app has no paywall; every feature is unlocked.
    AppState::new(adb, app_lists, data_dir).with_entitlement(Entitlement::Pro)
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
        app_report::save_app_report,
        diagnostics::collect_diagnostics,
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

    // stdout *and* a rolling daily file under the data dir. A packaged app has
    // nowhere for stdout to go, so the file is the only copy that survives on
    // the machines where a bug actually happens.
    let data_dir = default_data_dir();
    let log_control = diagnostics::init_logging(&data_dir);
    session::install(log_control.dir(), log_control.debug_enabled());

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

    let state = default_state(app_lists, data_dir)
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
        .setup(move |app| {
            telemetry.start();
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
            }
            _ => {}
        });
}

#[cfg(test)]
mod initialization_tests {
    use super::*;

    #[cfg(feature = "direct-adb")]
    #[test]
    fn missing_app_data_never_falls_back_to_the_working_directory() {
        assert!(identity_app_data(None).is_err());
        assert_eq!(
            identity_app_data(Some(std::path::PathBuf::from("/fixture-profile"))).unwrap(),
            std::path::PathBuf::from("/fixture-profile/ShieldOptimizer")
        );
    }

    #[tokio::test]
    async fn initialization_failure_is_preserved_across_native_operations() {
        let driver = ConfigurationError("Rust ADB setup could not save its identity. Check app storage access and reopen the app. Existing identities were not replaced.".into());
        let expected = driver.0.clone();
        let binding = shield_optimizer_core::adb::AdbDeviceBinding {
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
            shield_optimizer_core::adb::ServerStop::Skipped(_)
        ));
    }
}

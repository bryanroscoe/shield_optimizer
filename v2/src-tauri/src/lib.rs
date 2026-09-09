//! Shield Optimizer v2 — desktop Tauri entry point.
//!
//! Shared engine and driver-generic commands live in `shield_optimizer_core`.
//! This crate keeps the desktop-only subprocess ADB driver, installer/updater,
//! host-network scan, and file-path based commands.

pub mod adb;
pub mod commands;
pub use shield_optimizer_core::engine;

use std::path::PathBuf;
use std::sync::Arc;

use adb::SubprocessAdb;
use commands::{backup, files, install, scan, sideload, update, AppState};
use shield_optimizer_core::adb::{AdbDriver, AdbError, AdbOutput, AdbResult};
use shield_optimizer_core::commands::{
    apps, devices, health, input, launcher, loader, optimize, reboot, recovery, screenshot, shell,
    snapshot, tuning,
};
use shield_optimizer_core::license::Entitlement;

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

fn default_state(app_lists: engine::AppListBundle, data_dir: PathBuf) -> AppState {
    let adb: Arc<dyn AdbDriver> = match adb::discover_adb_binary() {
        Some(path) => {
            tracing::info!(adb = %path.display(), "adb located");
            Arc::new(SubprocessAdb::new(path))
        }
        None => {
            tracing::warn!("no adb binary located; commands will return BinaryNotFound");
            Arc::new(NoAdbDriver)
        }
    };
    // The desktop app has no paywall; every feature is unlocked.
    AppState::new(adb, app_lists, data_dir).with_entitlement(Entitlement::Pro)
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

    // Best-effort tracing setup; fall back silently if EnvFilter parse fails.
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init();

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

    let state =
        default_state(app_lists, default_data_dir()).with_known_names(loader::load_known_names());

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            devices::list_devices,
            devices::device_profile,
            devices::connect_device,
            devices::disconnect_device,
            devices::pair_device,
            devices::rename_device,
            health::health_report,
            health::media_report,
            health::resource_sample,
            health::app_list_for_device,
            health::report_all,
            install::adb_status,
            install::install_adb,
            install::restart_adb,
            scan::scan_network,
            launcher::list_launchers,
            launcher::current_launcher,
            launcher::channel_provider_disabled,
            launcher::set_default_launcher,
            launcher::disable_launcher,
            apps::disable_package,
            apps::enable_package,
            apps::force_stop,
            screenshot::take_screenshot,
            apps::uninstall_package,
            apps::reinstall_existing,
            apps::open_play_store,
            apps::package_states,
            apps::list_other_packages,
            apps::app_memory_map,
            apps::app_usage_map,
            apps::safety_info,
            apps::trim_caches,
            apps::app_permission_state,
            apps::set_app_permission,
            apps::set_app_op,
            apps::get_app_op,
            input::send_text,
            input::send_key,
            input::open_settings,
            sideload::install_apk,
            sideload::list_apks_in_folder,
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
            tuning::get_tweaks,
            tuning::write_setting,
            tuning::set_display_scaling,
            tuning::get_display_scaling,
            tuning::get_private_dns,
            tuning::set_private_dns,
            optimize::prepare_optimize,
            optimize::apply_performance_settings,
            update::check_for_update,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

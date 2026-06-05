//! Shield Optimizer v2 — Tauri entry point.
//!
//! Layout:
//! - `engine/` — pure logic (no I/O).
//! - `adb/`    — subprocess wrapper + output parsers.
//! - `commands/` — Tauri command handlers and shared state.

pub mod adb;
pub mod commands;
pub mod engine;

use std::path::PathBuf;

use commands::{
    apps, backup, devices, files, health, input, install, launcher, loader, optimize, reboot,
    recovery, scan, screenshot, sideload, snapshot, tuning, AppState,
};

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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
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
            crate::engine::AppListBundle::default()
        }
    };

    let state = AppState::default_for_runtime(app_lists, default_data_dir());

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            devices::list_devices,
            devices::device_profile,
            devices::connect_device,
            devices::disconnect_device,
            devices::pair_device,
            devices::rename_device,
            health::health_report,
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
            apps::safety_info,
            apps::trim_caches,
            input::send_text,
            input::send_key,
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
            tuning::get_tweaks,
            tuning::write_setting,
            tuning::set_display_scaling,
            tuning::get_display_scaling,
            optimize::prepare_optimize,
            optimize::apply_performance_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

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
    apps, devices, health, install, launcher, loader, scan, sideload, snapshot, AppState,
};

/// Resolve the OS-appropriate snapshot directory.
///
/// macOS: `~/Library/Application Support/ShieldOptimizer/snapshots`
/// Linux: `~/.local/share/shield-optimizer/snapshots`
/// Windows: `%APPDATA%/ShieldOptimizer/snapshots`
fn default_snapshot_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("ShieldOptimizer")
        .join("snapshots")
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

    let snapshot_dir = default_snapshot_dir();
    let state = AppState::default_for_runtime(app_lists, snapshot_dir);

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            devices::list_devices,
            devices::device_profile,
            devices::connect_device,
            devices::disconnect_device,
            health::health_report,
            health::app_list_for_device,
            install::adb_status,
            install::install_adb,
            scan::scan_network,
            launcher::list_launchers,
            launcher::current_launcher,
            launcher::channel_provider_disabled,
            launcher::set_default_launcher,
            apps::disable_package,
            apps::enable_package,
            apps::uninstall_package,
            apps::reinstall_existing,
            sideload::install_apk,
            snapshot::list_snapshots,
            snapshot::save_snapshot,
            snapshot::preview_apply,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

//! ATV Optimizer mobile Tauri entry point.
//!
//! This crate wires the shared core to an Android wireless-ADB transport. Host
//! builds compile the same code with a stub transport so CI can validate the
//! Rust command surface before Android hardware is available.

mod file_commands;
mod scrcpy_resource;
mod wireless_adb;
mod wireless_commands;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use shield_optimizer_core::commands::{
    apps, devices, health, input, launcher, loader, optimize, reboot, recovery, screenshot,
    snapshot, tuning, AppState,
};
use shield_optimizer_core::engine;
use shield_optimizer_core::license::{validate_license_key, Entitlement};
use tauri::{Manager, State};
use wireless_adb::WirelessAdb;
use wireless_commands::MobileState;

/// On-disk record of an activated license. Persisted to
/// `app_data_dir()/license.json` so Pro survives restarts.
#[derive(Debug, Serialize, Deserialize)]
struct LicenseFile {
    key: String,
    entitlement: Entitlement,
}

fn license_path(data_dir: &Path) -> PathBuf {
    data_dir.join("license.json")
}

fn persist_license(data_dir: &Path, file: &LicenseFile) -> Result<(), String> {
    std::fs::create_dir_all(data_dir).map_err(|e| format!("create license dir: {e}"))?;
    let json = serde_json::to_string(file).map_err(|e| format!("serialize license: {e}"))?;
    let path = license_path(data_dir);
    std::fs::write(&path, json).map_err(|e| format!("persist license: {e}"))
}

/// Read the persisted entitlement at startup. Returns `Free` when there is no
/// license file, it can't be read/parsed, or the stored key no longer
/// validates — so a tampered or stale file safely degrades to Free.
fn read_persisted_entitlement(data_dir: &Path) -> Entitlement {
    let path = license_path(data_dir);
    let Ok(contents) = std::fs::read_to_string(&path) else {
        return Entitlement::Free;
    };
    match serde_json::from_str::<LicenseFile>(&contents) {
        Ok(file) if validate_license_key(&file.key) => Entitlement::Pro,
        Ok(_) => {
            tracing::warn!("license.json present but key no longer validates; treating as Free");
            Entitlement::Free
        }
        Err(e) => {
            tracing::warn!(error = %e, "failed to parse license.json; treating as Free");
            Entitlement::Free
        }
    }
}

/// `activate_license` — validate a key and, if valid, flip the live entitlement
/// to Pro and persist it. Returns the resulting entitlement.
#[tauri::command]
async fn activate_license(state: State<'_, AppState>, key: String) -> Result<Entitlement, String> {
    let key = key.trim().to_string();
    if !validate_license_key(&key) {
        return Err("That license key isn't valid.".to_string());
    }
    let file = LicenseFile {
        key,
        entitlement: Entitlement::Pro,
    };
    // Persist before changing live state. A storage failure must not leave the
    // current process in Pro while the next launch silently falls back to Free.
    persist_license(&state.data_dir, &file)?;
    state.set_entitlement(Entitlement::Pro);
    tracing::info!("license activated; entitlement set to Pro");
    Ok(state.entitlement())
}

/// `get_entitlement` — the current live entitlement, for the frontend to gate UI.
#[tauri::command]
async fn get_entitlement(state: State<'_, AppState>) -> Result<Entitlement, String> {
    Ok(state.entitlement())
}

/// `read_debug_log` — return the last ~500 lines of the on-disk debug log for
/// the More screen. An absent log is not an error — returns an empty string.
#[tauri::command]
async fn read_debug_log(state: State<'_, AppState>) -> Result<String, String> {
    let path = state.data_dir.join("debug.log");
    let contents = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(String::new()),
        Err(e) => return Err(format!("read debug log: {e}")),
    };
    let lines: Vec<&str> = contents.lines().collect();
    let start = lines.len().saturating_sub(500);
    Ok(lines[start..].join("\n"))
}

/// Host-dev fallback when Tauri's app-scoped data dir is unavailable. On Android
/// `dirs::data_local_dir()` returns None (dirs-sys hard-codes a None home there),
/// so this is only used off-device where the current dir is writable.
fn default_data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("ATVOptimizer")
}

/// Install the tracing subscriber: logcat/stdout as before, PLUS a plain-text
/// file layer at `data_dir/debug.log` that the More screen can surface. Called
/// once from `setup`, after the data dir is known.
fn init_logging(data_dir: &Path) {
    use tracing_subscriber::prelude::*;

    let _ = std::fs::create_dir_all(data_dir);
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let file_appender = tracing_appender::rolling::never(data_dir, "debug.log");

    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .with(
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(file_appender),
        )
        .try_init();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_atv_adb::init())
        .setup(|app| {
            // On Android the dirs crate returns no writable location, so resolve
            // the app-scoped data dir through Tauri's path resolver (already
            // namespaced by the bundle identifier — no extra segment needed).
            // `app_data_dir()` failing here is fatal for logging/persistence, so
            // fall back to a dev-writable dir (logging isn't up yet, so this
            // pre-init failure can't be logged to file).
            let data_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| default_data_dir());
            init_logging(&data_dir);
            tracing::info!(data_dir = %data_dir.display(), "resolved data dir");
            match scrcpy_resource::materialize(&data_dir) {
                Ok(path) => tracing::info!(path = %path.display(), "scrcpy server materialized"),
                Err(e) => tracing::warn!(error = %e, "scrcpy server materialization failed; remote input will use shell fallback"),
            }

            let app_lists = match loader::load_embedded_app_lists() {
                Ok(lists) => {
                    tracing::info!(total = lists.total(), "app lists loaded");
                    lists
                }
                Err(e) => {
                    tracing::error!(error = %e, "failed to load embedded app lists");
                    engine::AppListBundle::default()
                }
            };

            // Restore Pro if a valid license was persisted; default Free.
            let entitlement = read_persisted_entitlement(&data_dir);
            tracing::info!(?entitlement, "startup entitlement");

            let wireless = Arc::new(WirelessAdb::new(
                app.handle().clone(),
                data_dir.join("adb_key"),
            ));
            let state = AppState::new(wireless.clone(), app_lists, data_dir)
                .with_known_names(loader::load_known_names())
                .with_entitlement(entitlement);
            app.manage(state);
            app.manage(MobileState { wireless });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            wireless_commands::wireless_discover,
            wireless_commands::wireless_pair,
            wireless_commands::wireless_connect,
            wireless_commands::wireless_disconnect,
            wireless_commands::wireless_status,
            wireless_commands::find_remote,
            activate_license,
            get_entitlement,
            read_debug_log,
            devices::list_devices,
            devices::device_profile,
            devices::rename_device,
            health::health_report,
            health::app_list_for_device,
            health::report_all,
            launcher::list_launchers,
            launcher::current_launcher,
            launcher::channel_provider_disabled,
            launcher::set_default_launcher,
            launcher::disable_launcher,
            apps::force_stop,
            screenshot::take_screenshot,
            apps::package_states,
            apps::list_other_packages,
            apps::app_memory_map,
            apps::app_usage_map,
            apps::safety_info,
            apps::trim_caches,
            apps::disable_package,
            apps::enable_package,
            apps::uninstall_package,
            apps::reinstall_existing,
            apps::set_app_permission,
            apps::app_permission_state,
            apps::set_app_op,
            apps::get_app_op,
            apps::open_play_store,
            input::send_text,
            input::send_key,
            input::open_settings,
            recovery::panic_recovery,
            reboot::reboot_device,
            optimize::prepare_optimize,
            optimize::apply_performance_settings,
            tuning::get_tweaks,
            tuning::write_setting,
            tuning::get_display_scaling,
            tuning::set_display_scaling,
            tuning::get_private_dns,
            tuning::set_private_dns,
            snapshot::delete_snapshot,
            snapshot::snapshot_dir_path,
            snapshot::list_snapshots,
            snapshot::save_snapshot,
            snapshot::apply_snapshot,
            snapshot::preview_apply,
            file_commands::list_remote_dir,
            file_commands::pull_file,
            file_commands::backup_apk,
            file_commands::restore_apk_backup,
            file_commands::list_backups,
        ])
        .run(tauri::generate_context!())
        .expect("error while running ATV Optimizer mobile application");
}

#[cfg(test)]
mod license_tests {
    use super::{persist_license, read_persisted_entitlement, LicenseFile};
    use shield_optimizer_core::license::{Entitlement, TEST_LICENSE_KEY};

    #[test]
    fn persisted_valid_license_round_trips() {
        let dir = tempfile::tempdir().expect("temp dir");
        let file = LicenseFile {
            key: TEST_LICENSE_KEY.to_string(),
            entitlement: Entitlement::Pro,
        };
        persist_license(dir.path(), &file).expect("persist license");
        assert_eq!(read_persisted_entitlement(dir.path()), Entitlement::Pro);
    }

    #[test]
    fn persistence_failure_is_reported() {
        let dir = tempfile::tempdir().expect("temp dir");
        let not_a_dir = dir.path().join("file");
        std::fs::write(&not_a_dir, "occupied").expect("write blocker");
        let file = LicenseFile {
            key: TEST_LICENSE_KEY.to_string(),
            entitlement: Entitlement::Pro,
        };
        assert!(persist_license(&not_a_dir, &file).is_err());
    }
}

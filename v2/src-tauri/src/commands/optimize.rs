//! Optimize / Restore — host bridge between the wizard UI and the
//! pure engine planner. The frontend calls `prepare_optimize` to fetch the
//! plan, then iterates the items and calls the existing per-package commands
//! (`disable_package` / `uninstall_package` / `enable_package`) one at a time
//! so the UI can show live progress.
//!
//! `apply_performance_settings` is the post-pass that mirrors v1's
//! Performance Settings step (animation triple at 0.5 for Optimize, 1.0 for
//! Restore).

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::adb::{
    parse_disabled_packages_output, parse_installed_packages_output, parse_total_pss_by_process,
};
use crate::engine::{compute_plan, OptimizeInputs, OptimizeMode, OptimizePlan};

use super::AppState;

/// `prepare_optimize` — fetch installed, disabled, and memory map for `serial`,
/// pick the device-appropriate app list, and run the engine planner.
#[tauri::command]
pub async fn prepare_optimize(
    state: State<'_, AppState>,
    serial: String,
    mode: OptimizeMode,
) -> Result<OptimizePlan, String> {
    let device = crate::commands::devices::device_profile_impl(state.inner(), &serial).await?;

    let apps = state.app_lists.for_device(device.device_type);

    let adb = state.adb_snapshot().await;
    let (installed_res, disabled_res, meminfo_res) = tokio::join!(
        adb.shell(&serial, "pm list packages"),
        adb.shell(&serial, "pm list packages -d"),
        adb.shell(&serial, "dumpsys meminfo"),
    );
    let installed = installed_res.map_err(|e| format!("pm list packages: {e}"))?;
    let disabled = disabled_res.map_err(|e| format!("pm list packages -d: {e}"))?;
    let meminfo = meminfo_res.map_err(|e| format!("dumpsys meminfo: {e}"))?;

    let installed_set: HashSet<String> = parse_installed_packages_output(&installed.stdout)
        .into_iter()
        .collect();
    let disabled_set: HashSet<String> = parse_disabled_packages_output(&disabled.stdout)
        .into_iter()
        .collect();
    let memory = parse_total_pss_by_process(&meminfo.stdout);

    let plan = compute_plan(
        &apps,
        mode,
        &OptimizeInputs {
            installed_packages: &installed_set,
            disabled_packages: &disabled_set,
            memory_map: &memory,
        },
    );
    Ok(plan)
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PerformanceProfile {
    /// v1's Optimize default — animation triple at 0.5×.
    Optimized,
    /// v1's Restore default — animation triple at 1.0× (system default).
    Default,
}

#[derive(Serialize)]
pub struct PerformanceResult {
    pub ok: bool,
    pub message: String,
}

/// `apply_performance_settings` — the post-wizard step that writes the
/// animation triple all at once. Optimize → 0.5, Restore → 1.0.
#[tauri::command]
pub async fn apply_performance_settings(
    state: State<'_, AppState>,
    serial: String,
    profile: PerformanceProfile,
) -> Result<PerformanceResult, String> {
    let value = match profile {
        PerformanceProfile::Optimized => "0.5",
        PerformanceProfile::Default => "1",
    };
    let cmd = format!(
        "settings put global window_animation_scale {value}; \
         settings put global transition_animation_scale {value}; \
         settings put global animator_duration_scale {value}"
    );
    let adb = state.adb_snapshot().await;
    let out = adb
        .shell(&serial, &cmd)
        .await
        .map_err(|e| format!("settings put: {e}"))?;
    let message = if out.stdout.is_empty() {
        if out.stderr.is_empty() {
            format!("Animations → {value}×")
        } else {
            out.stderr
        }
    } else {
        out.stdout
    };
    Ok(PerformanceResult {
        ok: !message.contains("Error") && !message.contains("Exception"),
        message,
    })
}

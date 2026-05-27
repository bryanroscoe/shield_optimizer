//! Health report and display-mode commands.

use serde::Serialize;
use tauri::State;

use crate::adb::{
    parse_display_mode, parse_meminfo_summary, parse_storage_info, parse_thermal_max_celsius,
    parse_total_pss_by_process, DisplayMode, RamInfo, StorageInfo,
};

use super::AppState;

/// Top-N memory consumer entry.
#[derive(Serialize)]
pub struct MemoryEntry {
    pub package: String,
    pub mb: f64,
}

/// Single payload for the Health Report view — everything the UI needs in one
/// round trip.
#[derive(Serialize)]
pub struct HealthReport {
    pub display: DisplayMode,
    pub ram: RamInfo,
    pub storage: StorageInfo,
    pub temperature_c: Option<f64>,
    pub top_memory: Vec<MemoryEntry>,
}

/// `health_report` — fetch display + meminfo + thermal + storage in parallel
/// and decode into a single payload.
#[tauri::command]
pub async fn health_report(
    state: State<'_, AppState>,
    serial: String,
) -> Result<HealthReport, String> {
    let adb = state.adb_snapshot().await;
    let (display_res, mem_res, thermal_res, df_res) = tokio::join!(
        adb.shell(&serial, "dumpsys display"),
        adb.shell(&serial, "dumpsys meminfo"),
        adb.shell(&serial, "dumpsys thermalservice"),
        adb.shell(&serial, "df -h /data"),
    );
    let display_out = display_res.map_err(|e| format!("dumpsys display: {e}"))?;
    let mem_out = mem_res.map_err(|e| format!("dumpsys meminfo: {e}"))?;
    // Thermal and storage are best-effort — some Android builds restrict
    // access; surfacing them as missing is better than failing the whole report.
    let thermal_text = thermal_res.map(|o| o.stdout).unwrap_or_default();
    let df_text = df_res.map(|o| o.stdout).unwrap_or_default();

    let display = parse_display_mode(&display_out.stdout);
    let ram = parse_meminfo_summary(&mem_out.stdout);
    let storage = parse_storage_info(&df_text);
    let temperature_c = parse_thermal_max_celsius(&thermal_text);

    let mut top_memory: Vec<MemoryEntry> = parse_total_pss_by_process(&mem_out.stdout)
        .into_iter()
        .map(|(package, mb)| MemoryEntry { package, mb })
        .collect();
    top_memory.sort_by(|a, b| b.mb.partial_cmp(&a.mb).unwrap_or(std::cmp::Ordering::Equal));
    top_memory.truncate(10);

    Ok(HealthReport {
        display,
        ram,
        storage,
        temperature_c,
        top_memory,
    })
}

/// `app_list_for_device` — return the merged app list for a given device type.
/// Read-only; doesn't touch the device. Used by the Profile view.
#[tauri::command]
pub async fn app_list_for_device(
    state: State<'_, AppState>,
    device_type: crate::engine::DeviceType,
) -> Result<Vec<crate::engine::types::AppEntry>, String> {
    Ok(state.app_lists.for_device(device_type))
}

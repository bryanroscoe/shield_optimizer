//! Health report and display-mode commands.

use serde::Serialize;
use tauri::State;

use crate::adb::{parse_display_mode, parse_total_pss_by_process, DisplayMode};

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
    pub top_memory: Vec<MemoryEntry>,
    pub raw_meminfo_first_lines: Option<String>,
}

/// `health_report` — fetch display mode + dumpsys meminfo, decode both.
#[tauri::command]
pub async fn health_report(
    state: State<'_, AppState>,
    serial: String,
) -> Result<HealthReport, String> {
    let display_fut = state.adb.shell(&serial, "dumpsys display");
    let mem_fut = state.adb.shell(&serial, "dumpsys meminfo");

    let display_out = display_fut
        .await
        .map_err(|e| format!("dumpsys display: {e}"))?;
    let mem_out = mem_fut.await.map_err(|e| format!("dumpsys meminfo: {e}"))?;

    let display = parse_display_mode(&display_out.stdout);
    let mem_map = parse_total_pss_by_process(&mem_out.stdout);

    let mut top_memory: Vec<MemoryEntry> = mem_map
        .into_iter()
        .map(|(package, mb)| MemoryEntry { package, mb })
        .collect();
    top_memory.sort_by(|a, b| b.mb.partial_cmp(&a.mb).unwrap_or(std::cmp::Ordering::Equal));
    top_memory.truncate(10);

    // First few lines of raw meminfo are useful for the totals (RAM free/used).
    let raw_meminfo_first_lines = Some(
        mem_out
            .stdout
            .lines()
            .take(20)
            .collect::<Vec<&str>>()
            .join("\n"),
    );

    Ok(HealthReport {
        display,
        top_memory,
        raw_meminfo_first_lines,
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

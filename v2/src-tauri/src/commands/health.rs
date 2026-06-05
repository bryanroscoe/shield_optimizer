//! Health report and display-mode commands.

use serde::Serialize;
use tauri::State;

use crate::adb::{
    parse_active_audio_device, parse_display_mode, parse_hardware_properties_temp,
    parse_meminfo_summary, parse_storage_info, parse_thermal_max_celsius,
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
    /// Current active audio output (e.g. "HDMI", "BUILTIN_SPEAKER"). Parsed
    /// from `dumpsys audio`; `None` when the section isn't present.
    pub audio_device: Option<String>,
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
    let (display_res, mem_res, thermal_res, df_res, audio_res, hwprops_res) = tokio::join!(
        adb.shell(&serial, "dumpsys display"),
        adb.shell(&serial, "dumpsys meminfo"),
        adb.shell(&serial, "dumpsys thermalservice"),
        adb.shell(&serial, "df -h /data"),
        adb.shell(&serial, "dumpsys audio"),
        adb.shell(&serial, "dumpsys hardware_properties"),
    );
    let display_out = display_res.map_err(|e| format!("dumpsys display: {e}"))?;
    let mem_out = mem_res.map_err(|e| format!("dumpsys meminfo: {e}"))?;
    // Thermal, storage, audio are best-effort — some Android builds restrict
    // access; surfacing them as missing is better than failing the whole report.
    let thermal_text = thermal_res.map(|o| o.stdout).unwrap_or_default();
    let df_text = df_res.map(|o| o.stdout).unwrap_or_default();
    let audio_text = audio_res.map(|o| o.stdout).unwrap_or_default();

    let display = parse_display_mode(&display_out.stdout);
    let ram = parse_meminfo_summary(&mem_out.stdout);
    let storage = parse_storage_info(&df_text);
    let temperature_c = parse_thermal_max_celsius(&thermal_text).or_else(|| {
        parse_hardware_properties_temp(
            &hwprops_res
                .as_ref()
                .map(|o| o.stdout.clone())
                .unwrap_or_default(),
        )
    });
    let audio_device = parse_active_audio_device(&audio_text);

    let mut top_memory: Vec<MemoryEntry> = parse_total_pss_by_process(&mem_out.stdout)
        .into_iter()
        .map(|(package, mb)| MemoryEntry { package, mb })
        .collect();
    top_memory.sort_by(|a, b| b.mb.partial_cmp(&a.mb).unwrap_or(std::cmp::Ordering::Equal));
    top_memory.truncate(20);

    Ok(HealthReport {
        display,
        ram,
        storage,
        temperature_c,
        audio_device,
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

#[derive(Serialize)]
pub struct DeviceReport {
    pub serial: String,
    pub name: String,
    /// `Some(report)` on success, `None` if the per-device health call failed.
    pub report: Option<HealthReport>,
    pub error: Option<String>,
}

/// `report_all` — iterate every authorized device and run a health report on
/// each. Mirrors v1's main-menu "Report All" (§2.1). Returns one entry per
/// device, including failures so the UI can show them inline.
#[tauri::command]
pub async fn report_all(state: State<'_, AppState>) -> Result<Vec<DeviceReport>, String> {
    let devices = crate::commands::devices::list_devices_impl(state.inner()).await?;
    let mut out = Vec::with_capacity(devices.len());
    for d in devices {
        if !matches!(d.status, crate::engine::types::DeviceStatus::Device) {
            out.push(DeviceReport {
                serial: d.serial.clone(),
                name: d.name.clone(),
                report: None,
                error: Some(format!("device not authorized (state: {:?})", d.status)),
            });
            continue;
        }
        // Run inline (not joined) to avoid hammering the same adb daemon with
        // many parallel dumpsys calls — keeps load and timeout risk low.
        match health_report_for(state.inner(), &d.serial).await {
            Ok(report) => out.push(DeviceReport {
                serial: d.serial,
                name: d.name,
                report: Some(report),
                error: None,
            }),
            Err(e) => out.push(DeviceReport {
                serial: d.serial,
                name: d.name,
                report: None,
                error: Some(e),
            }),
        }
    }
    Ok(out)
}

/// Internal helper — same as `health_report` but takes `&AppState`. Lets
/// `report_all` reuse the implementation without juggling Tauri's State
/// lifetime constraints.
async fn health_report_for(state: &AppState, serial: &str) -> Result<HealthReport, String> {
    let adb = state.adb_snapshot().await;
    let (display_res, mem_res, thermal_res, df_res, audio_res, hwprops_res) = tokio::join!(
        adb.shell(serial, "dumpsys display"),
        adb.shell(serial, "dumpsys meminfo"),
        adb.shell(serial, "dumpsys thermalservice"),
        adb.shell(serial, "df -h /data"),
        adb.shell(serial, "dumpsys audio"),
        adb.shell(serial, "dumpsys hardware_properties"),
    );
    let display_out = display_res.map_err(|e| format!("dumpsys display: {e}"))?;
    let mem_out = mem_res.map_err(|e| format!("dumpsys meminfo: {e}"))?;
    let thermal_text = thermal_res.map(|o| o.stdout).unwrap_or_default();
    let df_text = df_res.map(|o| o.stdout).unwrap_or_default();
    let audio_text = audio_res.map(|o| o.stdout).unwrap_or_default();

    let display = parse_display_mode(&display_out.stdout);
    let ram = parse_meminfo_summary(&mem_out.stdout);
    let storage = parse_storage_info(&df_text);
    let temperature_c = parse_thermal_max_celsius(&thermal_text).or_else(|| {
        parse_hardware_properties_temp(
            &hwprops_res
                .as_ref()
                .map(|o| o.stdout.clone())
                .unwrap_or_default(),
        )
    });
    let audio_device = parse_active_audio_device(&audio_text);

    let mut top_memory: Vec<MemoryEntry> = parse_total_pss_by_process(&mem_out.stdout)
        .into_iter()
        .map(|(package, mb)| MemoryEntry { package, mb })
        .collect();
    top_memory.sort_by(|a, b| b.mb.partial_cmp(&a.mb).unwrap_or(std::cmp::Ordering::Equal));
    top_memory.truncate(20);

    Ok(HealthReport {
        display,
        ram,
        storage,
        temperature_c,
        audio_device,
        top_memory,
    })
}

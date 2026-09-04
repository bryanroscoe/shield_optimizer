//! Health report and display-mode commands.

use serde::Serialize;
use tauri::State;

use crate::adb::{
    batch_command, parse_active_audio_device, parse_display_mode, parse_hardware_properties_temp,
    parse_meminfo_summary, parse_storage_info, parse_thermal_max_celsius,
    parse_total_pss_by_process, split_batch, DisplayMode, RamInfo, StorageInfo,
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

/// `health_report` — fetch display + meminfo + thermal + storage + audio in
/// one batched shell call and decode into a single payload.
#[tauri::command]
pub async fn health_report(
    state: State<'_, AppState>,
    serial: String,
) -> Result<HealthReport, String> {
    health_report_for(state.inner(), &serial).await
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

/// Shared implementation for `health_report` and `report_all`. Takes
/// `&AppState` so both callers avoid juggling Tauri's State lifetime.
async fn health_report_for(state: &AppState, serial: &str) -> Result<HealthReport, String> {
    use std::time::Duration;
    use tokio::time::timeout;

    let adb = state.adb_snapshot().await;

    // One round-trip for the whole report. This was six concurrent shells
    // plus a seventh `/proc/meminfo` call on the RAM fallback path; the mobile
    // transport serializes everything behind one connection, so the fan-out
    // bought no concurrency and cost 6-7 × Wi-Fi RTT. `/proc/meminfo` is tiny,
    // so it rides along unconditionally and serves as the fallback without an
    // extra round-trip.
    let cmd = batch_command(&[
        "dumpsys display",
        "dumpsys meminfo",
        "dumpsys thermalservice",
        "df -h /data",
        "dumpsys audio",
        "dumpsys hardware_properties",
        "cat /proc/meminfo",
    ]);

    // Generous: one timeout now covers what used to be seven calls, and the
    // transport caps each individual read on its own. A failure or timeout
    // still has to produce a report — every section degrades to its parser's
    // default, exactly as a single failing call did before, and a truncated
    // read only blanks the sections that never arrived.
    let batched = match timeout(Duration::from_secs(30), adb.shell(serial, &cmd)).await {
        Ok(Ok(out)) => out.stdout,
        Ok(Err(e)) => {
            tracing::warn!(error = %e, "health batch shell failed; reporting defaults");
            String::new()
        }
        Err(_) => {
            tracing::warn!("health batch shell timed out; reporting defaults");
            String::new()
        }
    };

    let sections = split_batch(&batched, 7);
    let display_text = &sections[0];
    let mem_text = &sections[1];
    let thermal_text = &sections[2];
    let df_text = &sections[3];
    let audio_text = &sections[4];
    let hwprops_text = &sections[5];
    let procmem_text = &sections[6];

    let display = parse_display_mode(display_text);
    let mut ram = parse_meminfo_summary(mem_text);

    // Fast, local fallback for RAM info when the dumpsys meminfo section is
    // missing or unparseable.
    if ram.total_mb.is_none() || ram.free_mb.is_none() {
        let mut total: Option<u64> = None;
        let mut free: Option<u64> = None;
        let mut avail: Option<u64> = None;
        for line in procmem_text.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                if parts[0] == "MemTotal:" {
                    total = parts[1].parse().ok().map(|kb: u64| kb / 1024);
                } else if parts[0] == "MemFree:" {
                    free = parts[1].parse().ok().map(|kb: u64| kb / 1024);
                } else if parts[0] == "MemAvailable:" {
                    avail = parts[1].parse().ok().map(|kb: u64| kb / 1024);
                }
            }
        }
        if total.is_some() {
            ram.total_mb = total;
            // Use MemAvailable as free RAM if reported, else MemFree
            ram.free_mb = avail.or(free);
            if let (Some(t), Some(f)) = (total, ram.free_mb) {
                ram.used_mb = Some(t - f);
            }
        }
    }

    let storage = parse_storage_info(df_text);
    let temperature_c = parse_thermal_max_celsius(thermal_text)
        .or_else(|| parse_hardware_properties_temp(hwprops_text));
    let audio_device = parse_active_audio_device(audio_text);

    let mut top_memory: Vec<MemoryEntry> = parse_total_pss_by_process(mem_text)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adb::BATCH_SEPARATOR;
    use crate::commands::test_support::{state_with, MockAdb};

    /// Device output for a batched shell: sections joined by the sentinel the
    /// device would echo between sub-commands.
    fn batched(sections: &[&str]) -> String {
        sections.join(&format!("\n{BATCH_SEPARATOR}\n"))
    }

    const MEMINFO: &str = "Total RAM: 3,072,000K\n\
                           Free RAM: 1,024,000K\n\
                           Used RAM: 2,048,000K\n\
                           Total PSS by process:\n\
                           243,712K: com.netflix.ninja (pid 2201)\n";
    const DF: &str = "Filesystem      Size  Used Avail Use% Mounted on\n\
                      /dev/block/dm-5  11G  8.4G  2.4G  78% /data\n";
    const THERMAL: &str = "Temperature{mValue=42.0, mType=0, mName=CPU}";
    const AUDIO: &str = "  Devices: hdmi\n";
    const PROC_MEMINFO: &str = "MemTotal:        3145728 kB\n\
                                MemFree:          524288 kB\n\
                                MemAvailable:    1048576 kB\n";

    #[tokio::test]
    async fn health_report_reads_every_section_from_one_batched_call() {
        let mock = MockAdb::default().on_shell(
            BATCH_SEPARATOR,
            &batched(&["", MEMINFO, THERMAL, DF, AUDIO, "", PROC_MEMINFO]),
        );
        let log = mock.shell_log();
        let state = state_with(mock);

        let report = health_report_for(&state, "serial")
            .await
            .unwrap_or_else(|e| panic!("report: {e}"));

        assert_eq!(report.ram.total_mb, Some(3000));
        assert_eq!(report.ram.free_mb, Some(1000));
        assert_eq!(report.storage.used_percent, Some(78));
        assert_eq!(report.storage.available.as_deref(), Some("2.4G"));
        assert_eq!(report.temperature_c, Some(42.0));
        assert_eq!(report.audio_device.as_deref(), Some("HDMI"));
        assert_eq!(report.top_memory.len(), 1);
        assert_eq!(report.top_memory[0].package, "com.netflix.ninja");

        let calls = log.lock().unwrap();
        assert_eq!(
            calls.len(),
            1,
            "the whole report must cost one round-trip: {calls:?}"
        );
        assert!(
            calls[0].contains("cat /proc/meminfo"),
            "the RAM fallback rides along in the same batch: {}",
            calls[0]
        );
        assert!(
            !calls[0].contains("&&"),
            "sub-commands must run even if an earlier one fails"
        );
    }

    #[tokio::test]
    async fn proc_meminfo_fallback_needs_no_extra_round_trip() {
        // `dumpsys meminfo` came back empty (restricted / failed); the
        // `/proc/meminfo` section already in the batch has to cover for it.
        let mock = MockAdb::default().on_shell(
            BATCH_SEPARATOR,
            &batched(&["", "", THERMAL, DF, AUDIO, "", PROC_MEMINFO]),
        );
        let log = mock.shell_log();
        let state = state_with(mock);

        let report = health_report_for(&state, "serial")
            .await
            .unwrap_or_else(|e| panic!("report: {e}"));

        assert_eq!(report.ram.total_mb, Some(3072));
        assert_eq!(
            report.ram.free_mb,
            Some(1024),
            "MemAvailable wins over MemFree"
        );
        assert_eq!(report.ram.used_mb, Some(2048));
        // The rest of the report is untouched by the missing section.
        assert_eq!(report.storage.used_percent, Some(78));
        assert_eq!(log.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn one_empty_section_does_not_blank_the_rest_of_the_report() {
        // No thermal, no hardware_properties, no audio, no /proc/meminfo —
        // each degrades to its parser's default and everything else survives.
        let state = state_with(MockAdb::default().on_shell(
            BATCH_SEPARATOR,
            &batched(&["", MEMINFO, "", DF, "", "", ""]),
        ));

        let report = health_report_for(&state, "serial")
            .await
            .unwrap_or_else(|e| panic!("report: {e}"));
        assert_eq!(report.temperature_c, None);
        assert_eq!(report.audio_device, None);
        assert_eq!(report.display.resolution, None);
        assert_eq!(report.ram.total_mb, Some(3000));
        assert_eq!(report.storage.used_percent, Some(78));
    }

    #[tokio::test]
    async fn truncated_batch_only_blanks_the_sections_that_never_arrived() {
        // The shell died after `dumpsys meminfo`: storage / thermal / audio
        // pad to empty instead of shifting into the wrong slots.
        let state =
            state_with(MockAdb::default().on_shell(BATCH_SEPARATOR, &batched(&["", MEMINFO])));

        let report = health_report_for(&state, "serial")
            .await
            .unwrap_or_else(|e| panic!("report: {e}"));
        assert_eq!(report.ram.total_mb, Some(3000));
        assert_eq!(report.storage.used_percent, None);
        assert_eq!(report.temperature_c, None);
        assert_eq!(report.audio_device, None);
    }

    #[tokio::test]
    async fn a_failed_batch_still_returns_a_default_report() {
        let state = state_with(MockAdb::default().on_shell_err(BATCH_SEPARATOR, "device offline"));

        let report = health_report_for(&state, "serial")
            .await
            .unwrap_or_else(|e| panic!("a failed shell must degrade, not error: {e}"));
        assert_eq!(report.ram.total_mb, None);
        assert_eq!(report.storage.used_percent, None);
        assert!(report.top_memory.is_empty());
    }
}

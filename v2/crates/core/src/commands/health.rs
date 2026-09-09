//! Health report and display-mode commands.

use serde::Serialize;
use tauri::State;

use crate::adb::{
    batch_command, parse_active_audio_device, parse_display_mode, parse_display_modes,
    parse_hardware_properties_temp, parse_meminfo_summary, parse_net_dev, parse_proc_stat,
    parse_storage_info, parse_thermal_max_celsius, parse_total_pss_by_process, split_batch,
    DisplayMode, RamInfo, StorageInfo,
};
use crate::engine::media::{
    build_capabilities, parse_media_codecs, surround_mode, MediaCapabilities,
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

/// Every `media_codecs*.xml` the platform might ship, in one `cat`.
///
/// The file is split across vendor / system / product partitions and its exact
/// path differs per build, so globbing all the plausible locations at once is
/// both cheaper and more portable than probing them in turn. Unmatched globs
/// and missing files fail silently — `batch_command` already discards stderr,
/// and `parse_media_codecs` tolerates the concatenation of several documents.
///
/// `/vendor/odm/etc` and `/odm/etc` are not optional extras: they are where a
/// Shield TV Pro (mdarcy, Android 11) actually keeps the file — `/vendor/etc`
/// holds only `media_profiles` there. Verified against hardware; dropping them
/// makes the whole report read as "decoder list unavailable" on the device
/// this app is named after.
const MEDIA_CODECS_GLOB: &str = "cat /vendor/etc/media_codecs*.xml \
                                 /vendor/odm/etc/media_codecs*.xml /odm/etc/media_codecs*.xml \
                                 /system/etc/media_codecs*.xml /etc/media_codecs*.xml \
                                 /product/etc/media_codecs*.xml";

/// `media_report` — what this device can actually decode and output.
///
/// `device_type` comes from the caller rather than a fresh `getprop` round
/// trip: the frontend already holds the profiled device, and re-deriving it
/// here would fork the single canonical detection path.
#[tauri::command]
pub async fn media_report(
    state: State<'_, AppState>,
    serial: String,
    device_type: crate::engine::DeviceType,
) -> Result<MediaCapabilities, String> {
    media_report_for(state.inner(), &serial, device_type).await
}

async fn media_report_for(
    state: &AppState,
    serial: &str,
    device_type: crate::engine::DeviceType,
) -> Result<MediaCapabilities, String> {
    use std::time::Duration;
    use tokio::time::timeout;

    let adb = state.adb_snapshot().await;
    let cmd = batch_command(&[
        "dumpsys display",
        MEDIA_CODECS_GLOB,
        "settings get global encoded_surround_output",
        "settings get global encoded_surround_output_enabled_formats",
        "settings get secure match_content_frame_rate",
    ]);

    let batched = timeout(Duration::from_secs(30), adb.shell(serial, &cmd))
        .await
        .map_err(|_| "Playback report timed out after 30 seconds.".to_string())?
        .map_err(|e| format!("media report: {e}"))?
        .stdout;

    let sections = split_batch(&batched, 5);
    if sections.iter().all(|section| section.trim().is_empty()) {
        return Err("The TV returned no playback data. Retry the report.".into());
    }

    // A `settings get` on an unset key prints the literal "null"; treat that
    // and an empty section as "not set" so the engine sees `None` either way.
    let setting = |raw: &str| {
        let v = raw.trim();
        (!v.is_empty() && v != "null").then(|| v.to_string())
    };

    Ok(build_capabilities(
        &parse_media_codecs(&sections[1]),
        parse_display_mode(&sections[0]).hdr_types,
        parse_display_modes(&sections[0]),
        surround_mode(
            setting(&sections[2]).as_deref(),
            setting(&sections[3]).as_deref(),
        ),
        setting(&sections[4]),
        device_type,
    ))
}

/// CPU load and network throughput over a short sampling window.
#[derive(Serialize)]
pub struct ResourceSample {
    /// Aggregate CPU busy time across the window, 0-100. `None` when
    /// `/proc/stat` was unreadable or the counters did not advance.
    pub cpu_percent: Option<f64>,
    pub rx_bytes_per_s: Option<u64>,
    pub tx_bytes_per_s: Option<u64>,
    /// The nominal sampling window, so the UI can label the reading.
    pub interval_ms: u64,
}

/// Device-side sampling window. Both samples and the sleep between them run
/// inside one shell, so the delta is measured on the device and Wi-Fi latency
/// never lands in the denominator.
const SAMPLE_INTERVAL_MS: u64 = 1000;

/// `resource_sample` — CPU % and network throughput.
///
/// Deliberately *not* folded into `health_report`: rate counters need two
/// reads a second apart, and charging every health refresh an extra second of
/// wall clock to carry two numbers would be a bad trade. The Health tab calls
/// this separately and can poll it without re-running the whole report.
#[tauri::command]
pub async fn resource_sample(
    state: State<'_, AppState>,
    serial: String,
) -> Result<ResourceSample, String> {
    resource_sample_for(state.inner(), &serial).await
}

async fn resource_sample_for(state: &AppState, serial: &str) -> Result<ResourceSample, String> {
    use std::time::Duration;
    use tokio::time::timeout;

    let adb = state.adb_snapshot().await;
    let cmd = batch_command(&[
        "cat /proc/stat",
        "cat /proc/net/dev",
        &format!("sleep {}", SAMPLE_INTERVAL_MS as f64 / 1000.0),
        "cat /proc/stat",
        "cat /proc/net/dev",
    ]);

    let batched = timeout(Duration::from_secs(30), adb.shell(serial, &cmd))
        .await
        .map_err(|_| "Resource sample timed out after 30 seconds.".to_string())?
        .map_err(|e| format!("resource sample: {e}"))?
        .stdout;

    let sections = split_batch(&batched, 5);

    // Counter deltas use saturating subtraction throughout: an interface going
    // down mid-window (or a 32-bit counter wrapping) would otherwise underflow
    // into a nonsense spike rather than reading as zero.
    let cpu_percent = match (parse_proc_stat(&sections[0]), parse_proc_stat(&sections[3])) {
        (Some(a), Some(b)) => {
            let total = b.total.saturating_sub(a.total);
            let busy = b.busy.saturating_sub(a.busy);
            (total > 0).then(|| ((busy as f64 / total as f64) * 1000.0).round() / 10.0)
        }
        _ => None,
    };

    let (rx_bytes_per_s, tx_bytes_per_s) =
        match (parse_net_dev(&sections[1]), parse_net_dev(&sections[4])) {
            (Some(a), Some(b)) => {
                let per_s = |later: u64, earlier: u64| {
                    later.saturating_sub(earlier) * 1000 / SAMPLE_INTERVAL_MS
                };
                (
                    Some(per_s(b.rx_bytes, a.rx_bytes)),
                    Some(per_s(b.tx_bytes, a.tx_bytes)),
                )
            }
            _ => (None, None),
        };

    Ok(ResourceSample {
        cpu_percent,
        rx_bytes_per_s,
        tx_bytes_per_s,
        interval_ms: SAMPLE_INTERVAL_MS,
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

    let batched = timeout(Duration::from_secs(30), adb.shell(serial, &cmd))
        .await
        .map_err(|_| "Health report timed out after 30 seconds.".to_string())?
        .map_err(|e| format!("health report: {e}"))?
        .stdout;

    let sections = split_batch(&batched, 7);
    if sections.iter().all(|section| section.trim().is_empty()) {
        return Err("The TV returned no health data. Retry the report.".into());
    }
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

    const DISPLAY: &str = "DisplayDeviceInfo{\"Built-in Screen\": 3840 x 2160, modeId 20, \
supportedModes [{id=1, width=3840, height=2160, fps=23.976023}, {id=20, width=3840, height=2160, \
fps=59.94006}], HdrCapabilities{mSupportedHdrTypes=[1, 2, 3]}}";
    const CODECS: &str = r#"<Decoders>
<MediaCodec name="OMX.Nvidia.h265.decode" type="video/hevc" />
<MediaCodec name="c2.android.av1.decoder" type="video/av01" />
</Decoders>"#;

    #[test]
    fn the_codec_glob_covers_the_path_a_real_shield_uses() {
        // Regression guard, found on hardware: a Shield TV Pro (mdarcy,
        // Android 11) keeps media_codecs.xml under /vendor/odm/etc — its
        // /vendor/etc has only media_profiles. Dropping this path makes the
        // whole report degrade to "decoder list unavailable" on the exact
        // device this app targets.
        assert!(MEDIA_CODECS_GLOB.contains("/vendor/odm/etc/media_codecs"));
        assert!(MEDIA_CODECS_GLOB.contains("/vendor/etc/media_codecs"));
    }

    #[tokio::test]
    async fn media_report_decodes_every_section_from_one_round_trip() {
        let mock = MockAdb::default().on_shell(
            BATCH_SEPARATOR,
            &batched(&[DISPLAY, CODECS, "3", "5,6,14", "2"]),
        );
        let log = mock.shell_log();
        let state = state_with(mock);

        let caps = media_report_for(&state, "serial", crate::engine::DeviceType::Shield)
            .await
            .unwrap_or_else(|e| panic!("media report: {e}"));

        assert_eq!(caps.hdr_types, ["Dolby Vision", "HDR10", "HLG"]);
        assert_eq!(caps.modes.len(), 2);
        assert!(caps.modes.iter().any(|m| m.is_film_rate()));
        assert_eq!(caps.match_content_frame_rate.as_deref(), Some("2"));
        assert_eq!(caps.audio.mode, crate::engine::SurroundMode::Manual);
        assert!(caps
            .audio
            .enabled_formats
            .iter()
            .any(|f| f.contains("TrueHD")));

        let hevc = caps.video.iter().find(|v| v.mime == "video/hevc").unwrap();
        assert!(hevc.hardware);
        let av1 = caps.video.iter().find(|v| v.mime == "video/av01").unwrap();
        assert!(!av1.hardware && av1.software);

        let calls = log.lock().unwrap();
        assert_eq!(
            calls.len(),
            1,
            "the report must cost one round-trip: {calls:?}"
        );
        assert!(calls[0].contains("media_codecs"));
    }

    #[tokio::test]
    async fn media_report_treats_a_null_setting_as_unset() {
        // `settings get` prints the literal "null" for an absent key — it must
        // not reach the engine as the string "null".
        let state = state_with(MockAdb::default().on_shell(
            BATCH_SEPARATOR,
            &batched(&[DISPLAY, CODECS, "null", "null", "null"]),
        ));
        let caps = media_report_for(&state, "serial", crate::engine::DeviceType::Shield)
            .await
            .unwrap();
        assert_eq!(caps.match_content_frame_rate, None);
        assert_eq!(caps.audio.mode, crate::engine::SurroundMode::Unset);
        assert_eq!(caps.audio.raw_formats, None);
    }

    #[tokio::test]
    async fn media_report_survives_an_unreadable_codec_file() {
        // Everything else still renders, and nothing is claimed about codecs.
        let state = state_with(
            MockAdb::default().on_shell(BATCH_SEPARATOR, &batched(&[DISPLAY, "", "0", "", "2"])),
        );
        let caps = media_report_for(&state, "serial", crate::engine::DeviceType::Shield)
            .await
            .unwrap();
        assert_eq!(caps.modes.len(), 2);
        assert!(caps
            .verdicts
            .iter()
            .any(|v| v.title == "Decoder list unavailable"));
        assert!(!caps.verdicts.iter().any(|v| v.title.contains("AV1")));
    }

    #[tokio::test]
    async fn media_report_errors_when_the_device_returns_nothing() {
        let state = state_with(MockAdb::default().on_shell(BATCH_SEPARATOR, ""));
        assert!(
            media_report_for(&state, "serial", crate::engine::DeviceType::Shield)
                .await
                .is_err()
        );
    }

    const STAT_A: &str = "cpu  100 0 100 800 0 0 0 0";
    const STAT_B: &str = "cpu  200 0 200 1600 0 0 0 0";
    const NET_A: &str = "  eth0: 1000 5 0 0 0 0 0 0 500 3 0 0 0 0 0 0";
    const NET_B: &str = "  eth0: 3000 9 0 0 0 0 0 0 1500 7 0 0 0 0 0 0";

    #[tokio::test]
    async fn resource_sample_derives_rates_from_two_device_side_reads() {
        let mock = MockAdb::default().on_shell(
            BATCH_SEPARATOR,
            &batched(&[STAT_A, NET_A, "", STAT_B, NET_B]),
        );
        let log = mock.shell_log();
        let state = state_with(mock);

        let sample = resource_sample_for(&state, "serial").await.unwrap();
        // busy delta 200, total delta 1000 → 20%.
        assert_eq!(sample.cpu_percent, Some(20.0));
        assert_eq!(sample.rx_bytes_per_s, Some(2000));
        assert_eq!(sample.tx_bytes_per_s, Some(1000));

        let calls = log.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert!(
            calls[0].contains("sleep 1"),
            "the window must be device-side: {}",
            calls[0]
        );
    }

    #[tokio::test]
    async fn resource_sample_reports_none_rather_than_a_bogus_spike() {
        // Counters that went backwards (interface reset / wrap) and identical
        // CPU samples must not produce negative or infinite rates.
        let state = state_with(MockAdb::default().on_shell(
            BATCH_SEPARATOR,
            &batched(&[STAT_A, NET_B, "", STAT_A, NET_A]),
        ));
        let sample = resource_sample_for(&state, "serial").await.unwrap();
        assert_eq!(
            sample.cpu_percent, None,
            "no elapsed jiffies means no reading"
        );
        assert_eq!(sample.rx_bytes_per_s, Some(0));
        assert_eq!(sample.tx_bytes_per_s, Some(0));
    }

    #[tokio::test]
    async fn resource_sample_degrades_when_proc_is_unreadable() {
        let state = state_with(MockAdb::default().on_shell(BATCH_SEPARATOR, &batched(&["", ""])));
        let sample = resource_sample_for(&state, "serial").await.unwrap();
        assert_eq!(sample.cpu_percent, None);
        assert_eq!(sample.rx_bytes_per_s, None);
        assert_eq!(sample.interval_ms, SAMPLE_INTERVAL_MS);
    }

    #[tokio::test]
    async fn a_failed_batch_preserves_the_transport_error() {
        let state = state_with(MockAdb::default().on_shell_err(BATCH_SEPARATOR, "device offline"));

        let error = health_report_for(&state, "serial")
            .await
            .err()
            .expect("must fail");
        assert!(error.contains("device offline"));
    }
}

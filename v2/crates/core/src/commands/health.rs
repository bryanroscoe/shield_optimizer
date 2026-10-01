//! Health report and display-mode commands.

use serde::Serialize;
use tauri::State;

use crate::adb::{
    batch_command, kb_to_mb, package_for_process, parse_active_audio_device, parse_display_mode,
    parse_display_modes, parse_hardware_properties_temp, parse_meminfo_summary, parse_net_dev,
    parse_proc_stat, parse_pss_by_process, parse_storage_info, parse_thermal_max_celsius,
    split_batch, DisplayMode, RamInfo, StorageInfo,
};
use crate::engine::media::{
    build_capabilities, parse_media_codecs, surround_mode, MediaCapabilities,
};

use super::AppState;

/// One process among the top memory consumers.
#[derive(Serialize)]
pub struct MemoryEntry {
    /// The full process name the device reported.
    pub process: String,
    pub pid: Option<u32>,
    /// The package this process name would belong to, if it has the shape of
    /// one. Unverified: the UI only treats the row as that app once the
    /// package is confirmed installed on the device.
    pub package: Option<String>,
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
/// and missing files must not discard readable entries from the other paths.
/// The trailing no-op preserves that partial output in a checked batch.
///
/// `/vendor/odm/etc` and `/odm/etc` are not optional extras: they are where a
/// Shield TV Pro (mdarcy, Android 11) actually keeps the file — `/vendor/etc`
/// holds only `media_profiles` there. Verified against hardware; dropping them
/// makes the whole report read as "decoder list unavailable" on the device
/// this app is named after.
const MEDIA_CODECS_GLOB: &str = "cat /vendor/etc/media_codecs*.xml \
                                 /vendor/odm/etc/media_codecs*.xml /odm/etc/media_codecs*.xml \
                                 /system/etc/media_codecs*.xml /etc/media_codecs*.xml \
                                 /product/etc/media_codecs*.xml; :";

/// Device-reported codec configuration, display modes, and audio settings.
#[tauri::command]
pub async fn media_report(
    state: State<'_, AppState>,
    serial: String,
) -> Result<MediaCapabilities, String> {
    media_report_for(state.inner(), &serial).await
}

async fn media_report_for(state: &AppState, serial: &str) -> Result<MediaCapabilities, String> {
    use std::time::Duration;
    use tokio::time::timeout;

    let adb = state.adb_snapshot().await;
    let cmd = crate::adb::batch::checked_batch_command(&[
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

    let sections = crate::adb::batch::parse_checked_batch(&batched, 5, &[2, 3, 4])?;

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
    ))
}

/// CPU load and network throughput over a short sampling window.
#[derive(Serialize)]
pub struct ResourceSample {
    /// Aggregate CPU busy time across the window, 0-100. `None` when
    /// `/proc/stat` was unreadable or the counters did not advance.
    pub cpu_percent: Option<f64>,
    pub interfaces: Vec<InterfaceRate>,
    pub interval_ms: Option<u64>,
}

#[derive(Serialize)]
pub struct InterfaceRate {
    pub name: String,
    pub rx_bytes_per_s: Option<u64>,
    pub tx_bytes_per_s: Option<u64>,
}

/// `resource_sample` — CPU % and network throughput.
///
/// Deliberately *not* folded into `health_report`: rate counters need two
/// reads a second apart, and charging every health refresh an extra second of
/// wall clock to carry two numbers would be a bad trade. The Health tab calls
/// this separately without re-running the whole report.
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
    let cmd = crate::adb::checked_batch_command(&[
        "cat /proc/uptime",
        "cat /proc/stat",
        "cat /proc/net/dev",
        "sleep 1",
        "cat /proc/uptime",
        "cat /proc/stat",
        "cat /proc/net/dev",
    ]);

    let batched = timeout(Duration::from_secs(30), adb.shell(serial, &cmd))
        .await
        .map_err(|_| "Resource sample timed out after 30 seconds.".to_string())?
        .map_err(|e| format!("resource sample: {e}"))?
        .stdout;

    let sections = crate::adb::parse_checked_batch(&batched, 7, &[3])?;
    let uptime = |raw: &str| raw.split_whitespace().next()?.parse::<f64>().ok();
    let elapsed = uptime(&sections[0])
        .zip(uptime(&sections[4]))
        .filter(|(a, b)| a.is_finite() && b.is_finite() && *a >= 0.0 && b > a)
        .map(|(a, b)| b - a);
    let cpu_percent = match (parse_proc_stat(&sections[1]), parse_proc_stat(&sections[5])) {
        (Some(a), Some(b)) => b
            .total
            .checked_sub(a.total)
            .zip(b.busy.checked_sub(a.busy))
            .filter(|(total, busy)| *total > 0 && busy <= total)
            .map(|(total, busy)| ((busy as f64 / total as f64) * 1000.0).round() / 10.0),
        _ => None,
    };

    let first = parse_net_dev(&sections[2]);
    let second = parse_net_dev(&sections[6]);
    let names: std::collections::BTreeSet<_> = first.keys().chain(second.keys()).collect();
    let interfaces = names
        .into_iter()
        .map(|name| {
            let rates = first.get(name).zip(second.get(name)).zip(elapsed);
            let rate = |later: u64, earlier: u64, seconds: f64| {
                later
                    .checked_sub(earlier)
                    .map(|delta| (delta as f64 / seconds).round() as u64)
            };
            InterfaceRate {
                name: name.clone(),
                rx_bytes_per_s: rates
                    .and_then(|((a, b), seconds)| rate(b.rx_bytes, a.rx_bytes, seconds)),
                tx_bytes_per_s: rates
                    .and_then(|((a, b), seconds)| rate(b.tx_bytes, a.tx_bytes, seconds)),
            }
        })
        .collect();

    Ok(ResourceSample {
        cpu_percent,
        interfaces,
        interval_ms: elapsed.map(|seconds| (seconds * 1000.0).round() as u64),
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

    let mut processes = parse_pss_by_process(mem_text);
    processes.sort_by_key(|p| std::cmp::Reverse(p.kb));
    processes.truncate(20);
    let top_memory: Vec<MemoryEntry> = processes
        .into_iter()
        .map(|p| MemoryEntry {
            package: package_for_process(&p.process).map(str::to_string),
            mb: kb_to_mb(p.kb),
            pid: p.pid,
            process: p.process,
        })
        .collect();

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
        assert_eq!(report.top_memory[0].process, "com.netflix.ninja");
        assert_eq!(report.top_memory[0].pid, Some(2201));
        assert_eq!(
            report.top_memory[0].package.as_deref(),
            Some("com.netflix.ninja")
        );

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
    async fn top_memory_keeps_each_process_whole() {
        let meminfo = "Total PSS by process:\n\
                       251,612K: vendor.nvidia.hardware.graphics.composer@2.0-service (pid 3405)\n\
                       175,433K: system (pid 3739 state 0 oom -900)\n\
                        85,413K: com.google.android.katniss:interactor (pid 15826 state 5 oom 150)\n\
                        40,112K: /system/bin/surfaceflinger (pid 312)\n\
                        33,000K: com.google.android.katniss (pid 15000 state 19 oom 900)\n";
        let state = state_with(MockAdb::default().on_shell(
            BATCH_SEPARATOR,
            &batched(&["", meminfo, THERMAL, DF, AUDIO, "", PROC_MEMINFO]),
        ));

        let report = health_report_for(&state, "serial")
            .await
            .unwrap_or_else(|e| panic!("report: {e}"));

        let rows: Vec<(&str, Option<u32>, Option<&str>)> = report
            .top_memory
            .iter()
            .map(|m| (m.process.as_str(), m.pid, m.package.as_deref()))
            .collect();
        assert_eq!(
            rows,
            vec![
                (
                    "vendor.nvidia.hardware.graphics.composer@2.0-service",
                    Some(3405),
                    None
                ),
                ("system", Some(3739), None),
                (
                    "com.google.android.katniss:interactor",
                    Some(15826),
                    Some("com.google.android.katniss")
                ),
                ("/system/bin/surfaceflinger", Some(312), None),
                (
                    "com.google.android.katniss",
                    Some(15000),
                    Some("com.google.android.katniss")
                ),
            ]
        );
        assert_eq!(report.top_memory[0].mb, 245.7);
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

    fn media_batch(sections: &[(&str, i32)]) -> String {
        sections
            .iter()
            .map(|(body, status)| format!("{body}\n{}{status}", crate::adb::batch::BATCH_STATUS))
            .collect::<Vec<_>>()
            .join(&format!("\n{BATCH_SEPARATOR}\n"))
    }

    #[tokio::test]
    async fn media_report_rejects_failed_and_truncated_settings_reads() {
        for index in 2..5 {
            let mut sections = [
                (DISPLAY, 0),
                (CODECS, 0),
                ("null", 0),
                ("null", 0),
                ("null", 0),
            ];
            sections[index] = ("", 1);
            let state =
                state_with(MockAdb::default().on_shell(BATCH_SEPARATOR, &media_batch(&sections)));
            assert!(media_report_for(&state, "serial")
                .await
                .unwrap_err()
                .contains("failed"));

            sections[index] = ("Error: settings unavailable", 0);
            let state =
                state_with(MockAdb::default().on_shell(BATCH_SEPARATOR, &media_batch(&sections)));
            assert!(media_report_for(&state, "serial")
                .await
                .unwrap_err()
                .contains("reported an error"));
        }
        let complete = media_batch(&[
            (DISPLAY, 0),
            (CODECS, 0),
            ("null", 0),
            ("null", 0),
            ("null", 0),
        ]);
        let truncated = complete
            .rsplit_once(crate::adb::batch::BATCH_STATUS)
            .unwrap()
            .0;
        let state = state_with(MockAdb::default().on_shell(BATCH_SEPARATOR, truncated));
        assert!(media_report_for(&state, "serial")
            .await
            .unwrap_err()
            .contains("did not complete"));
    }

    #[tokio::test]
    async fn media_report_keeps_codecs_when_optional_display_read_fails() {
        let state = state_with(MockAdb::default().on_shell(
            BATCH_SEPARATOR,
            &media_batch(&[("", 1), (CODECS, 0), ("null", 0), ("null", 0), ("null", 0)]),
        ));
        let caps = media_report_for(&state, "serial").await.unwrap();
        assert!(caps.modes.is_empty());
        assert!(caps.video.iter().any(|v| v.advertised));
    }

    #[cfg(unix)]
    #[test]
    fn codec_glob_preserves_readable_xml_among_missing_paths() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("readable.xml"), CODECS).unwrap();
        let command = MEDIA_CODECS_GLOB.replacen("cat ", "cat readable.xml ", 1);
        let output = std::process::Command::new("sh")
            .current_dir(directory.path())
            .args(["-c", &crate::adb::batch::checked_batch_command(&[&command])])
            .output()
            .unwrap();
        let text = String::from_utf8(output.stdout).unwrap();
        let sections = crate::adb::batch::parse_checked_batch(&text, 1, &[0]).unwrap();
        assert!(!parse_media_codecs(&sections[0]).is_empty());
    }

    #[tokio::test]
    async fn media_report_decodes_every_section_from_one_round_trip() {
        let mock = MockAdb::default().on_shell(
            BATCH_SEPARATOR,
            &media_batch(&[(DISPLAY, 0), (CODECS, 0), ("3", 0), ("5,6,14", 0), ("2", 0)]),
        );
        let log = mock.shell_log();
        let state = state_with(mock);

        let caps = media_report_for(&state, "serial")
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
        assert!(hevc.advertised && hevc.acceleration_unknown);
        let av1 = caps.video.iter().find(|v| v.mime == "video/av01").unwrap();
        assert!(av1.advertised && av1.software);

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
            &media_batch(&[
                (DISPLAY, 0),
                (CODECS, 0),
                ("null", 0),
                ("null", 0),
                ("null", 0),
            ]),
        ));
        let caps = media_report_for(&state, "serial").await.unwrap();
        assert_eq!(caps.match_content_frame_rate, None);
        assert_eq!(caps.audio.mode, crate::engine::SurroundMode::Unset);
        assert_eq!(caps.audio.raw_formats, None);
    }

    #[tokio::test]
    async fn media_report_survives_an_unreadable_codec_file() {
        // Everything else still renders, and nothing is claimed about codecs.
        let state = state_with(MockAdb::default().on_shell(
            BATCH_SEPARATOR,
            &media_batch(&[(DISPLAY, 0), ("", 0), ("0", 0), ("", 0), ("2", 0)]),
        ));
        let caps = media_report_for(&state, "serial").await.unwrap();
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
        assert!(media_report_for(&state, "serial").await.is_err());
    }

    const STAT_A: &str = "cpu  100 0 100 800 0 0 0 0";
    const STAT_B: &str = "cpu  200 0 200 1600 0 0 0 0";
    const NET_A: &str = "  eth0: 1000 5 0 0 0 0 0 0 500 3 0 0 0 0 0 0";
    const NET_B: &str = "  eth0: 3000 9 0 0 0 0 0 0 1500 7 0 0 0 0 0 0";

    fn resource_batch(parts: &[&str]) -> String {
        parts
            .iter()
            .map(|part| format!("{part}\n{}0", crate::adb::batch::BATCH_STATUS))
            .collect::<Vec<_>>()
            .join(&format!("\n{BATCH_SEPARATOR}\n"))
    }

    #[tokio::test]
    async fn resource_sample_derives_rates_from_two_device_side_reads() {
        let mock = MockAdb::default().on_shell(
            BATCH_SEPARATOR,
            &resource_batch(&["100.00 0", STAT_A, NET_A, "", "102.00 0", STAT_B, NET_B]),
        );
        let log = mock.shell_log();
        let state = state_with(mock);

        let sample = resource_sample_for(&state, "serial").await.unwrap();
        // busy delta 200, total delta 1000 → 20%.
        assert_eq!(sample.cpu_percent, Some(20.0));
        assert_eq!(sample.interval_ms, Some(2000));
        assert_eq!(sample.interfaces[0].name, "eth0");
        assert_eq!(sample.interfaces[0].rx_bytes_per_s, Some(1000));
        assert_eq!(sample.interfaces[0].tx_bytes_per_s, Some(500));

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
            &resource_batch(&["100 0", STAT_A, NET_B, "", "101 0", STAT_A, NET_A]),
        ));
        let sample = resource_sample_for(&state, "serial").await.unwrap();
        assert_eq!(
            sample.cpu_percent, None,
            "no elapsed jiffies means no reading"
        );
        assert_eq!(sample.interfaces[0].rx_bytes_per_s, None);
        assert_eq!(sample.interfaces[0].tx_bytes_per_s, None);
    }

    #[tokio::test]
    async fn resource_sample_degrades_when_proc_is_unreadable() {
        let response = resource_batch(&["", "", "", "", "", "", ""]).replacen(
            &format!("{}0", crate::adb::batch::BATCH_STATUS),
            &format!("{}1", crate::adb::batch::BATCH_STATUS),
            1,
        );
        let state = state_with(MockAdb::default().on_shell(BATCH_SEPARATOR, &response));
        let sample = resource_sample_for(&state, "serial").await.unwrap();
        assert_eq!(sample.cpu_percent, None);
        assert!(sample.interfaces.is_empty());
        assert_eq!(sample.interval_ms, None);
    }

    #[tokio::test]
    async fn resource_sample_keeps_tunnels_separate_and_missing_interfaces_unknown() {
        let net_a = format!("{NET_A}\ntun0: 1000 0 0 0 0 0 0 0 500\nwlan0: 100 0 0 0 0 0 0 0 100");
        let net_b = format!("{NET_B}\ntun0: 3000 0 0 0 0 0 0 0 1500\nwlan1: 100 0 0 0 0 0 0 0 100");
        let state = state_with(MockAdb::default().on_shell(
            BATCH_SEPARATOR,
            &resource_batch(&["100 0", STAT_A, &net_a, "", "101 0", STAT_B, &net_b]),
        ));
        let sample = resource_sample_for(&state, "serial").await.unwrap();
        assert_eq!(sample.interfaces.len(), 4);
        assert_eq!(sample.interfaces[0].rx_bytes_per_s, Some(2000));
        assert_eq!(sample.interfaces[1].rx_bytes_per_s, Some(2000));
        assert_eq!(sample.interfaces[2].rx_bytes_per_s, None);
        assert_eq!(sample.interfaces[3].rx_bytes_per_s, None);
    }

    #[tokio::test]
    async fn resource_sample_rejects_invalid_cpu_deltas_and_elapsed_time() {
        for (uptime, stat) in [
            ("99 0", STAT_A),
            ("NaN 0", "cpu 300 0 300 500 0 0 0 0"),
            ("", "cpu 0 0 0 0"),
        ] {
            let state = state_with(MockAdb::default().on_shell(
                BATCH_SEPARATOR,
                &resource_batch(&["100 0", STAT_A, NET_A, "", uptime, stat, NET_B]),
            ));
            let sample = resource_sample_for(&state, "serial").await.unwrap();
            assert_eq!(sample.cpu_percent, None);
            assert_eq!(sample.interval_ms, None);
            assert_eq!(sample.interfaces[0].rx_bytes_per_s, None);
        }
    }

    #[tokio::test]
    async fn resource_sample_rejects_truncated_batches() {
        let state = state_with(
            MockAdb::default().on_shell(BATCH_SEPARATOR, &resource_batch(&["100 0", STAT_A])),
        );
        assert!(resource_sample_for(&state, "serial").await.is_err());
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

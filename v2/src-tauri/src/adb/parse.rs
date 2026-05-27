//! Parsers for ADB output. These are pure functions (no I/O); the driver
//! fetches strings and the parsers turn them into typed values.
//!
//! Tests pin behavior against fixtures captured from real Shield devices
//! (see `tests/fixtures/`).

use std::collections::HashMap;

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

use crate::engine::types::{ConnectionType, DeviceStatus};

/// A row from `adb devices`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceListEntry {
    pub serial: String,
    pub status: DeviceStatus,
    pub connection: ConnectionType,
}

/// Parse `adb devices` output into a structured list.
///
/// Sample input (tab-separated columns in real output):
/// ```text
/// List of devices attached
/// 192.168.42.71:5555    device
/// 192.168.42.143:5555   unauthorized
/// emulator-5554         device
/// ```
pub fn parse_device_list(adb_devices_output: &str) -> Vec<DeviceListEntry> {
    static IP_PORT: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^\d+\.\d+\.\d+\.\d+:\d+$").unwrap());

    let mut entries = Vec::new();
    for line in adb_devices_output.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("List of devices") {
            continue;
        }
        // Each line: <serial>\t<status>
        let mut parts = line.split_whitespace();
        let Some(serial) = parts.next() else { continue };
        let Some(status_str) = parts.next() else {
            continue;
        };
        let Some(status) = DeviceStatus::from_adb_str(status_str) else {
            continue;
        };
        let connection = if IP_PORT.is_match(serial) {
            ConnectionType::Network
        } else {
            ConnectionType::Usb
        };
        entries.push(DeviceListEntry {
            serial: serial.to_string(),
            status,
            connection,
        });
    }
    entries
}

/// Parse the `package:<name>` lines that `pm list packages [-d|-e|-u]` emits.
pub fn parse_installed_packages_output(output: &str) -> Vec<String> {
    output
        .lines()
        .filter_map(|line| line.strip_prefix("package:"))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Same shape but kept as a distinct name for call-site clarity.
pub fn parse_disabled_packages_output(output: &str) -> Vec<String> {
    parse_installed_packages_output(output)
}

/// Parse the `Total PSS by process:` section of `dumpsys meminfo` into a
/// package → MB map. Sums multiple processes that share a base package.
///
/// Per v1's Get-AppMemoryMap learnings: per-process query (`dumpsys meminfo <pkg>`)
/// is unreliable across Android versions; the system-wide section is robust.
pub fn parse_dumpsys_meminfo(meminfo: &str) -> HashMap<String, f64> {
    static ROW: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^\s*([\d,]+)K:\s+([a-zA-Z0-9_.]+)").unwrap());

    let mut totals_kb: HashMap<String, u64> = HashMap::new();
    let mut in_section = false;
    for line in meminfo.lines() {
        if line.contains("Total PSS by process:") {
            in_section = true;
            continue;
        }
        if !in_section {
            continue;
        }
        if line.trim().is_empty() {
            // Empty line ends the section.
            break;
        }
        if let Some(caps) = ROW.captures(line) {
            let kb: u64 = caps[1].replace(',', "").parse().unwrap_or(0);
            let pkg = caps[2].to_string();
            *totals_kb.entry(pkg).or_insert(0) += kb;
        }
    }
    totals_kb
        .into_iter()
        .map(|(pkg, kb)| (pkg, (kb as f64 / 1024.0 * 10.0).round() / 10.0))
        .collect()
}

/// Stable alias for callers that want to be explicit about what they're getting.
pub fn parse_total_pss_by_process(meminfo: &str) -> HashMap<String, f64> {
    parse_dumpsys_meminfo(meminfo)
}

/// Free / used / total / swap MB parsed from the summary block at the bottom
/// of `dumpsys meminfo`. Returns `None` for fields the device didn't report.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct RamInfo {
    pub total_mb: Option<u64>,
    pub used_mb: Option<u64>,
    pub free_mb: Option<u64>,
    pub swap_mb: Option<u64>,
}

/// Parse the "Total RAM" / "Free RAM" / "Used RAM" / "ZRAM" lines from
/// `dumpsys meminfo` output. Values can be in KB (with commas) or MB
/// depending on Android version — we normalize to MB.
pub fn parse_meminfo_summary(meminfo: &str) -> RamInfo {
    static ROW: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^\s*(Total RAM|Free RAM|Used RAM|ZRAM):\s*([\d,]+)([KkMm])?").unwrap()
    });

    let mut info = RamInfo::default();
    for line in meminfo.lines() {
        let Some(caps) = ROW.captures(line) else {
            continue;
        };
        let label = &caps[1];
        let value: u64 = caps[2].replace(',', "").parse().unwrap_or(0);
        let unit = caps.get(3).map(|m| m.as_str()).unwrap_or("K"); // dumpsys defaults to K
        let mb = match unit {
            "M" | "m" => value,
            _ => value / 1024,
        };
        match label {
            "Total RAM" => info.total_mb = Some(mb),
            "Free RAM" => info.free_mb = Some(mb),
            "Used RAM" => info.used_mb = Some(mb),
            "ZRAM" => info.swap_mb = Some(mb),
            _ => {}
        }
    }
    info
}

/// Parse the highest temperature reading from `dumpsys thermalservice`. The
/// service emits a list of HardwareThrottlingService temps per zone; we want
/// the hottest CPU-class zone since that's the one users care about for
/// throttling. Returns `None` if no readable temp is present.
pub fn parse_thermal_max_celsius(dumpsys_thermalservice: &str) -> Option<f64> {
    static TEMP: LazyLock<Regex> = LazyLock::new(|| {
        // Common formats across Android 9-13:
        //   "Temperature{mValue=42.0, mType=0, mName=..."
        //   "  CPU: temp=42.0 type=CPU"
        Regex::new(r"mValue=([\d.]+)|temp=([\d.]+)").unwrap()
    });

    let mut max: Option<f64> = None;
    for caps in TEMP.captures_iter(dumpsys_thermalservice) {
        let raw = caps.get(1).or(caps.get(2)).map(|m| m.as_str())?;
        let Ok(t) = raw.parse::<f64>() else { continue };
        // Sanity check — drop obvious garbage like 0.0 or 999.0.
        if !(10.0..120.0).contains(&t) {
            continue;
        }
        max = Some(max.map_or(t, |m| m.max(t)));
    }
    max
}

/// Disk usage parsed from `df -h /data`.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct StorageInfo {
    /// Raw "size" column (e.g. "11G").
    pub total: Option<String>,
    pub used: Option<String>,
    pub available: Option<String>,
    /// Percentage as a u8 (0-100).
    pub used_percent: Option<u8>,
}

/// Parse `df -h /data` output. Expected shape:
/// ```text
/// Filesystem  Size  Used  Avail  Use%  Mounted on
/// /dev/...    11G   6.4G  4.6G   60%   /data
/// ```
pub fn parse_storage_info(df_output: &str) -> StorageInfo {
    let mut info = StorageInfo::default();
    for line in df_output.lines() {
        if !line.contains("/data") {
            continue;
        }
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 5 {
            continue;
        }
        // Layout: Filesystem Size Used Avail Use% Mounted-on
        info.total = Some(cols[1].to_string());
        info.used = Some(cols[2].to_string());
        info.available = Some(cols[3].to_string());
        info.used_percent = cols[4].trim_end_matches('%').parse::<u8>().ok();
        break;
    }
    info
}

/// Current display mode parsed from `dumpsys display`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DisplayMode {
    pub resolution: Option<String>,
    pub refresh_hz: Option<f64>,
    /// Decoded HDR types from `mSupportedHdrTypes=[…]`. Empty = SDR only.
    pub hdr_types: Vec<String>,
}

/// Parse `dumpsys display` for the active display's resolution + refresh rate +
/// HDR capabilities. The active mode id is in DisplayDeviceInfo; supportedModes
/// maps id → {width, height, fps}.
pub fn parse_display_mode(dumpsys_display: &str) -> DisplayMode {
    static MODE_ID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"modeId\s+(\d+)").unwrap());
    static MODE_ENTRY: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"id=(\d+),\s*width=(\d+),\s*height=(\d+),\s*fps=([\d.]+)").unwrap()
    });
    static HDR_TYPES: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"mSupportedHdrTypes=\[([\d,\s]*)\]").unwrap());

    let active_id = MODE_ID
        .captures(dumpsys_display)
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse::<u32>().ok());

    let mut resolution = None;
    let mut refresh_hz = None;
    if let Some(id) = active_id {
        for caps in MODE_ENTRY.captures_iter(dumpsys_display) {
            let mode_id: u32 = caps[1].parse().unwrap_or(0);
            if mode_id == id {
                let w: u32 = caps[2].parse().unwrap_or(0);
                let h: u32 = caps[3].parse().unwrap_or(0);
                let fps: f64 = caps[4].parse().unwrap_or(0.0);
                resolution = Some(format!("{w}x{h}"));
                refresh_hz = Some((fps * 100.0).round() / 100.0);
                break;
            }
        }
    }

    let mut hdr_types: Vec<String> = Vec::new();
    if let Some(caps) = HDR_TYPES.captures(dumpsys_display) {
        let raw = caps[1].trim();
        if !raw.is_empty() {
            for tok in raw.split(',') {
                let t = tok.trim();
                let name = match t {
                    "1" => Some("Dolby Vision"),
                    "2" => Some("HDR10"),
                    "3" => Some("HLG"),
                    "4" => Some("HDR10+"),
                    _ => None,
                };
                if let Some(n) = name {
                    hdr_types.push(n.to_string());
                }
            }
        }
    }

    DisplayMode {
        resolution,
        refresh_hz,
        hdr_types,
    }
}

/// Parse `dumpsys audio` for the first `Devices: <name>` row — the current
/// active output device. Returns the uppercased label (HDMI / BUILTIN_SPEAKER
/// / etc.) or `None` if the section isn't present.
pub fn parse_active_audio_device(dumpsys_audio: &str) -> Option<String> {
    static DEVICES: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?m)^\s*Devices:\s*([A-Za-z0-9_\-]+)").unwrap());
    DEVICES
        .captures(dumpsys_audio)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn parses_device_list_with_mixed_states() {
        let input = "List of devices attached\n\
            192.168.42.71:5555\tdevice\n\
            192.168.42.143:5555\tunauthorized\n\
            emulator-5554\tdevice\n\
            offline-host\toffline\n";
        let entries = parse_device_list(input);
        assert_eq!(entries.len(), 4);
        assert_eq!(entries[0].serial, "192.168.42.71:5555");
        assert_eq!(entries[0].status, DeviceStatus::Device);
        assert_eq!(entries[0].connection, ConnectionType::Network);
        assert_eq!(entries[1].status, DeviceStatus::Unauthorized);
        assert_eq!(entries[2].connection, ConnectionType::Usb);
        assert_eq!(entries[3].status, DeviceStatus::Offline);
    }

    #[test]
    fn ignores_header_and_blank_lines() {
        let input = "List of devices attached\n\n\n";
        assert!(parse_device_list(input).is_empty());
    }

    #[test]
    fn parses_pm_list_packages_output() {
        let input = "package:com.foo\npackage:com.bar\npackage:com.baz\n";
        let pkgs = parse_installed_packages_output(input);
        assert_eq!(pkgs, vec!["com.foo", "com.bar", "com.baz"]);
    }

    #[test]
    fn pm_list_packages_skips_garbage_lines() {
        let input = "package:com.foo\nnot-a-package\npackage:com.bar\n";
        let pkgs = parse_installed_packages_output(input);
        assert_eq!(pkgs, vec!["com.foo", "com.bar"]);
    }

    #[test]
    fn parses_total_pss_section_and_sums_per_package() {
        // Realistic-ish meminfo with multiple processes for one package.
        let input = "Total RAM: 3GB\n\n\
            Total PSS by process:\n\
              845,500K: com.plexapp.android (pid 1234)\n\
              193,300K: com.spauldhaliwal.dispatch:worker (pid 2345)\n\
              116,800K: com.spauldhaliwal.dispatch (pid 3456)\n\
              121,800K: com.Funimation.FunimationNow.androidtv (pid 4567)\n\n\
            Some other section we don't care about\n";
        let map = parse_dumpsys_meminfo(input);
        // Plex: single process
        assert!((map["com.plexapp.android"] - 825.7).abs() < 0.2);
        // Dispatch: sum of worker + main (193,300 + 116,800 = 310,100 K = ~302.8 MB)
        assert!((map["com.spauldhaliwal.dispatch"] - 302.8).abs() < 0.2);
        assert!(map.contains_key("com.Funimation.FunimationNow.androidtv"));
    }

    #[test]
    fn meminfo_returns_empty_when_section_missing() {
        assert!(parse_dumpsys_meminfo("nothing useful here").is_empty());
    }

    #[test]
    fn parses_display_mode_shield_4k60_hdr10() {
        // Distilled from a real Shield Android 11 dumpsys display.
        let input = r#"
DisplayDeviceInfo{"Built-in Screen": uniqueId="local:0", 3840 x 2160, modeId 20, defaultModeId 20, supportedModes [{id=1, width=3840, height=2160, fps=29.97003}, {id=20, width=3840, height=2160, fps=59.94006}], HdrCapabilities HdrCapabilities{mSupportedHdrTypes=[2], mMaxLuminance=500.0}, ...}
"#;
        let mode = parse_display_mode(input);
        assert_eq!(mode.resolution.as_deref(), Some("3840x2160"));
        assert_eq!(mode.refresh_hz, Some(59.94));
        assert_eq!(mode.hdr_types, vec!["HDR10"]);
    }

    #[test]
    fn parses_multiple_hdr_types() {
        let input = "modeId 1, supportedModes [{id=1, width=3840, height=2160, fps=60.0}], HdrCapabilities mSupportedHdrTypes=[1, 2, 4]";
        let mode = parse_display_mode(input);
        assert_eq!(mode.hdr_types, vec!["Dolby Vision", "HDR10", "HDR10+"]);
    }

    #[test]
    fn parses_meminfo_summary_kb() {
        let input = "\
            Total PSS by process:\n\
              123K: com.foo\n\n\
            Total RAM: 2,946,720K (status normal)\n\
            Free RAM: 770,512K\n\
            Used RAM: 2,176,208K\n\
            ZRAM: 524,288K\n";
        let info = parse_meminfo_summary(input);
        assert_eq!(info.total_mb, Some(2877));
        assert_eq!(info.free_mb, Some(752));
        assert_eq!(info.used_mb, Some(2125));
        assert_eq!(info.swap_mb, Some(512));
    }

    #[test]
    fn parses_thermal_max_temp() {
        let input = "Temperature{mValue=38.0, mType=0, mName=\"SKIN\"}\
                     Temperature{mValue=46.5, mType=0, mName=\"CPU\"}";
        assert_eq!(parse_thermal_max_celsius(input), Some(46.5));
    }

    #[test]
    fn parses_thermal_rejects_garbage_values() {
        let input = "mValue=999.0\nmValue=42.0";
        assert_eq!(parse_thermal_max_celsius(input), Some(42.0));
    }

    #[test]
    fn parses_df_data_storage() {
        let input = "\
            Filesystem    Size  Used  Avail  Use%  Mounted on\n\
            /dev/mmcblk0p35  11G  6.4G   4.6G  60%  /data\n";
        let info = parse_storage_info(input);
        assert_eq!(info.total.as_deref(), Some("11G"));
        assert_eq!(info.used.as_deref(), Some("6.4G"));
        assert_eq!(info.available.as_deref(), Some("4.6G"));
        assert_eq!(info.used_percent, Some(60));
    }

    #[test]
    fn display_mode_sdr_only_when_hdr_list_empty() {
        let input = "modeId 1, supportedModes [{id=1, width=1920, height=1080, fps=60.0}], HdrCapabilities mSupportedHdrTypes=[]";
        let mode = parse_display_mode(input);
        assert_eq!(mode.resolution.as_deref(), Some("1920x1080"));
        assert!(mode.hdr_types.is_empty());
    }

    #[test]
    fn parses_active_audio_device() {
        let input = "Audio routing:\n  Devices: hdmi\n  Streams: ...\n";
        assert_eq!(parse_active_audio_device(input).as_deref(), Some("HDMI"));
    }

    #[test]
    fn audio_device_missing_returns_none() {
        let input = "something completely unrelated";
        assert_eq!(parse_active_audio_device(input), None);
    }
}

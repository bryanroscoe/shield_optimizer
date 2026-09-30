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

/// A service advertised over mDNS and reported by `adb mdns services`.
///
/// Android 11+ wireless debugging listens on a *random* port that changes
/// every time it is toggled, and advertises that port over mDNS. Pairing and
/// connecting are separate services on separate ports — which is why guessing
/// `:5555` cannot reach a modern device (GitHub #88).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MdnsService {
    /// Service instance name, e.g. `adb-58040DLCH005YV-jBeCEe`.
    pub instance: String,
    /// Service type, e.g. `_adb-tls-connect._tcp`.
    pub service: String,
    pub host: String,
    pub port: u16,
}

/// `_adb._tcp` — legacy network debugging, no pairing code (the Shield path).
pub const MDNS_SERVICE_LEGACY: &str = "_adb._tcp";
/// `_adb-tls-connect._tcp` — the Android 11+ endpoint to `adb connect` to.
pub const MDNS_SERVICE_CONNECT: &str = "_adb-tls-connect._tcp";
/// `_adb-tls-pairing._tcp` — the Android 11+ endpoint to `adb pair` against.
pub const MDNS_SERVICE_PAIRING: &str = "_adb-tls-pairing._tcp";

impl MdnsService {
    /// `host:port`, ready to hand to `adb connect` / `adb pair`.
    ///
    /// `adb mdns services` prints an IPv6 host bare; this brackets it, which
    /// is the form adb accepts and the form it uses for the transport key, so
    /// this can be compared directly against `adb devices` serials.
    pub fn endpoint(&self) -> String {
        if self.host.contains(':') && !self.host.starts_with('[') {
            format!("[{}]:{}", self.host, self.port)
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }

    /// The hardware serial a Wireless debugging instance name embeds:
    /// `adb-<ro.serialno>-<random suffix>`. The pairing and connect services
    /// of one device carry different suffixes, so this, not the full name, is
    /// what ties them together. `None` for anything that does not have that
    /// shape, or whose serial is empty or `unknown`.
    pub fn instance_serial(&self) -> Option<&str> {
        instance_serial(&self.instance)
    }

    /// Can this be connected to directly? True for legacy `_adb._tcp` and for
    /// an already-paired `_adb-tls-connect._tcp`.
    pub fn is_connectable(&self) -> bool {
        self.service == MDNS_SERVICE_LEGACY || self.service == MDNS_SERVICE_CONNECT
    }

    /// Is this the pairing endpoint, which needs a code from the TV first?
    pub fn is_pairing(&self) -> bool {
        self.service == MDNS_SERVICE_PAIRING
    }
}

/// Is this adb transport key a network `host:port`? Dotted IPv4
/// (`192.168.1.5:5555`) or bracketed IPv6 (`[fe80::1]:41541`, the form adb
/// and [`MdnsService::endpoint`] use). A USB hardware serial is neither.
pub fn is_network_endpoint(serial: &str) -> bool {
    static IPV4_PORT: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^\d+\.\d+\.\d+\.\d+:\d+$").unwrap());
    if IPV4_PORT.is_match(serial) {
        return true;
    }
    let Some(rest) = serial.strip_prefix('[') else {
        return false;
    };
    let Some((host, port)) = rest.split_once("]:") else {
        return false;
    };
    // A scoped link-local address carries `%iface`, which Ipv6Addr rejects.
    let bare = host.split('%').next().unwrap_or(host);
    bare.parse::<std::net::Ipv6Addr>().is_ok() && port.parse::<u16>().is_ok_and(|p| p > 0)
}

/// See [`MdnsService::instance_serial`].
pub fn instance_serial(instance: &str) -> Option<&str> {
    let rest = instance.strip_prefix("adb-")?;
    let (serial, suffix) = rest.rsplit_once('-')?;
    let serial = serial.trim();
    if serial.is_empty() || suffix.is_empty() || serial.eq_ignore_ascii_case("unknown") {
        return None;
    }
    Some(serial)
}

/// Parse `adb mdns services` output.
///
/// Sample input (tab-separated in real output):
/// ```text
/// List of discovered mdns services
/// adb-58040DLCH005YV-jBeCEe   _adb-tls-connect._tcp   192.168.42.211:41541
/// adb-0323716101827           _adb._tcp               192.168.42.71:5555
/// ```
///
/// Rows that are malformed, carry no port, or name a service we do not
/// understand are dropped rather than guessed at — a wrong endpoint here is
/// indistinguishable to the user from a device that is not there.
pub fn parse_mdns_services(output: &str) -> Vec<MdnsService> {
    let mut out = Vec::new();
    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("List of discovered") {
            continue;
        }
        let mut cols = line.split_whitespace();
        let (Some(instance), Some(service), Some(endpoint)) =
            (cols.next(), cols.next(), cols.next())
        else {
            continue;
        };
        if !service.starts_with("_adb") {
            continue;
        }
        // rsplit: an IPv6 literal contains colons of its own.
        let Some((host, port)) = endpoint.rsplit_once(':') else {
            continue;
        };
        let Ok(port) = port.parse::<u16>() else {
            continue;
        };
        if host.is_empty() || port == 0 {
            continue;
        }
        out.push(MdnsService {
            instance: instance.to_string(),
            service: service.to_string(),
            host: host.to_string(),
            port,
        });
    }
    out
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
        // Network if it's an `ip:port` serial OR an mDNS wireless-debugging
        // serial (Android 11+ pairs over `_adb-tls-connect._tcp` etc.; those
        // never look like `ip:port` but always carry the `_tcp` service tag).
        // USB serials are plain hardware ids and contain neither.
        let connection = if is_network_endpoint(serial) || serial.contains("._tcp") {
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

/// One entry from `ls -lA` on the device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FileEntry {
    pub name: String,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub size_bytes: u64,
    /// `YYYY-MM-DD HH:MM`, as toybox prints it.
    pub modified: String,
}

/// Parse toybox `ls -lA` output into entries. Skips the `total N` header and
/// any line that doesn't match the 8-column shape (column counts are stable
/// across Android's toybox builds; names may contain spaces, so the name is
/// the regex tail rather than a whitespace split). Symlinks keep the link
/// name and drop the `-> target` part.
pub fn parse_ls_output(output: &str) -> Vec<FileEntry> {
    static ROW: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"^([a-zA-Z?-])[rwxsStT?-]{9}\s+\d+\s+\S+\s+\S+\s+(\d+)\s+(\d{4}-\d{2}-\d{2})\s+(\d{2}:\d{2})\s+(.+)$",
        )
        .unwrap()
    });
    let mut entries = Vec::new();
    for line in output.lines() {
        let Some(c) = ROW.captures(line.trim_end()) else {
            continue;
        };
        let kind = &c[1];
        let is_symlink = kind == "l";
        let raw_name = &c[5];
        let name = if is_symlink {
            raw_name.split(" -> ").next().unwrap_or(raw_name)
        } else {
            raw_name
        };
        entries.push(FileEntry {
            name: name.to_string(),
            is_dir: kind == "d",
            is_symlink,
            size_bytes: c[2].parse().unwrap_or(0),
            modified: format!("{} {}", &c[3], &c[4]),
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

/// When an app was last opened, from `dumpsys usagestats`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppUsage {
    /// `"YYYY-MM-DD HH:MM:SS"` of the last foreground use, or `None` if the app
    /// has never been opened (usagestats reports the 1969/1970 epoch for that).
    pub last_used: Option<String>,
    /// How many times the app has been launched (0 ⇒ never).
    pub launch_count: u32,
}

/// Parse per-package last-used + launch count from `dumpsys usagestats`. Each
/// package's usage appears across several stat buckets; we keep the most recent
/// `lastTimeUsed` and the highest `appLaunchCount` seen. A `1969`/`1970`
/// timestamp means "never opened" and maps to `last_used: None`.
pub fn parse_usage_stats(dumpsys: &str) -> HashMap<String, AppUsage> {
    // One package's stats sit on a single line:
    //   package=<id> totalTimeUsed="…" lastTimeUsed="YYYY-MM-DD HH:MM:SS" … appLaunchCount=N …
    static ROW: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"package=(\S+).*?lastTimeUsed="([^"]*)".*?appLaunchCount=(\d+)"#).unwrap()
    });

    let mut out: HashMap<String, AppUsage> = HashMap::new();
    for caps in ROW.captures_iter(dumpsys) {
        let package = caps[1].to_string();
        let raw_last = &caps[2];
        let count: u32 = caps[3].parse().unwrap_or(0);
        // Epoch 0 in any local timezone renders as 1969-12-31 or 1970-01-01.
        let last_used = if raw_last.starts_with("1969") || raw_last.starts_with("1970") {
            None
        } else {
            Some(raw_last.to_string())
        };

        let entry = out.entry(package).or_insert(AppUsage {
            last_used: None,
            launch_count: 0,
        });
        entry.launch_count = entry.launch_count.max(count);
        // "YYYY-MM-DD HH:MM:SS" sorts lexically == chronologically, so keep the
        // larger string. Any real date beats `None` (never used).
        match (&entry.last_used, &last_used) {
            (Some(existing), Some(candidate)) if candidate > existing => {
                entry.last_used = last_used;
            }
            (None, Some(_)) => entry.last_used = last_used,
            _ => {}
        }
    }
    out
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

/// Fallback temperature from `dumpsys hardware_properties`, used when
/// `thermalservice` reports nothing (older Shield firmware, e.g. 8.2.3,
/// formats it differently or restricts it). Reads the `CPU temperatures:`
/// and `GPU temperatures:` lines — `CPU temperatures: [32.0, 32.0, ...]` —
/// and returns the hottest in-range reading. Ignores the *throttling* /
/// *shutdown* / *vr* lines (those are limits, not the current temp).
pub fn parse_hardware_properties_temp(dumpsys_hardware_properties: &str) -> Option<f64> {
    static FLOAT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"-?\d+\.?\d*").unwrap());
    let mut max: Option<f64> = None;
    for line in dumpsys_hardware_properties.lines() {
        let l = line.trim();
        let is_current = (l.starts_with("CPU temperatures:") || l.starts_with("GPU temperatures:"))
            && !l.contains("throttling")
            && !l.contains("shutdown");
        if !is_current {
            continue;
        }
        for m in FLOAT.find_iter(l) {
            let Ok(t) = m.as_str().parse::<f64>() else {
                continue;
            };
            if (10.0..120.0).contains(&t) {
                max = Some(max.map_or(t, |cur: f64| cur.max(t)));
            }
        }
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
        // Anchor on the `Use%` column (the token ending in `%`) rather than a
        // fixed offset. `df -h /data` renders as
        //   Filesystem  Size  Used  Avail  Use%  Mounted-on
        // but a long filesystem name wraps the row onto its own line, so the
        // data columns can start at index 0 or 1 depending on the device. The
        // `%` token is unambiguous; Size/Used/Avail are the three before it.
        let pct_idx = cols.iter().position(|c| c.ends_with('%'));
        let Some(p) = pct_idx else { continue };
        if p < 3 {
            continue;
        }
        info.total = Some(cols[p - 3].to_string());
        info.used = Some(cols[p - 2].to_string());
        info.available = Some(cols[p - 1].to_string());
        info.used_percent = cols[p].trim_end_matches('%').parse::<u8>().ok();
        break;
    }
    info
}

/// Current display mode parsed from `dumpsys display`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DisplayMode {
    pub resolution: Option<String>,
    pub refresh_hz: Option<f64>,
    /// Decoded HDR types from `mSupportedHdrTypes=[…]`; empty may mean unavailable.
    pub hdr_types: Vec<String>,
}

fn selected_display_record(dumpsys_display: &str) -> Option<&str> {
    // DisplayDeviceInfo.toString emits one line, including nested mode/HDR objects.
    let records: std::collections::BTreeSet<_> = dumpsys_display
        .lines()
        .filter_map(|line| line.split_once("DisplayDeviceInfo{"))
        .map(|(_, record)| record.trim())
        .collect();
    if records.is_empty() {
        return Some(dumpsys_display);
    }
    if records.len() == 1 {
        return records.first().copied();
    }
    let mut defaults = records.into_iter().filter(|record| {
        record
            .split(',')
            .any(|field| field.trim().trim_end_matches('}') == "FLAG_DEFAULT_DISPLAY")
    });
    let selected = defaults.next()?;
    // ALLOWED_TO_BE_DEFAULT_DISPLAY only indicates eligibility, not selection.
    defaults.next().is_none().then_some(selected)
}

/// Parse `dumpsys display` for the active display's resolution + refresh rate +
/// HDR capabilities. The active mode id is in DisplayDeviceInfo; supportedModes
/// maps id → {width, height, fps}.
pub fn parse_display_mode(dumpsys_display: &str) -> DisplayMode {
    let dumpsys_display = selected_display_record(dumpsys_display).unwrap_or("");
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

/// Parse *every* mode from `dumpsys display`'s `supportedModes`, flagging the
/// active one.
///
/// `parse_display_mode` above answers "what is the panel doing right now" and
/// throws the rest away. The playback report needs the whole list instead —
/// whether a 23.976 Hz mode exists at all is what decides if 24p film can be
/// shown at its native cadence, and that mode is by definition not the active
/// one while the UI is on screen.
///
/// Duplicate (width, height, fps) triples are collapsed: a display commonly
/// advertises the same mode under several ids, and the caller cares about
/// distinct capabilities, not id count.
pub fn parse_display_modes(dumpsys_display: &str) -> Vec<crate::engine::media::DisplayModeEntry> {
    use crate::engine::media::DisplayModeEntry;
    let dumpsys_display = selected_display_record(dumpsys_display).unwrap_or("");

    static MODE_ID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"modeId\s+(\d+)").unwrap());
    static MODE_ENTRY: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"id=(\d+),\s*width=(\d+),\s*height=(\d+),\s*fps=([\d.]+)").unwrap()
    });

    let active_id = MODE_ID
        .captures(dumpsys_display)
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse::<u32>().ok());

    let mut out: Vec<DisplayModeEntry> = Vec::new();
    for caps in MODE_ENTRY.captures_iter(dumpsys_display) {
        let Ok(mode_id) = caps[1].parse::<u32>() else {
            continue;
        };
        let (Ok(width), Ok(height), Ok(fps)) = (
            caps[2].parse::<u32>(),
            caps[3].parse::<u32>(),
            caps[4].parse::<f64>(),
        ) else {
            continue;
        };
        let fps = (fps * 1000.0).round() / 1000.0;
        let active = Some(mode_id) == active_id;
        // Same capability under a second id: keep the entry, but let the
        // active flag win so the UI can still mark the running mode.
        if let Some(existing) = out
            .iter_mut()
            .find(|m| m.width == width && m.height == height && m.fps == fps)
        {
            existing.active |= active;
            continue;
        }
        out.push(DisplayModeEntry {
            width,
            height,
            fps,
            active,
        });
    }
    out.sort_by(|a, b| {
        (b.width, b.height).cmp(&(a.width, a.height)).then(
            b.fps
                .partial_cmp(&a.fps)
                .unwrap_or(std::cmp::Ordering::Equal),
        )
    });
    out
}

/// Aggregate CPU counters from one `/proc/stat` sample.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuSample {
    /// Jiffies spent doing anything other than idle/iowait.
    pub busy: u64,
    /// Jiffies across every state, idle included.
    pub total: u64,
}

/// Parse the aggregate `cpu` line of `/proc/stat`.
///
/// Fields are: user nice system idle iowait irq softirq steal guest guest_nice.
/// `idle` and `iowait` (indices 3 and 4) count as not-busy; everything else is
/// busy. `guest` time is already included in `user`, so summing every field
/// would double-count it — the total stops at `steal`.
pub fn parse_proc_stat(proc_stat: &str) -> Option<CpuSample> {
    let line = proc_stat
        .lines()
        .map(str::trim)
        .find(|l| l.starts_with("cpu ") || *l == "cpu")?;
    let values: Vec<u64> = line
        .split_whitespace()
        .skip(1)
        .take(8)
        .map(|v| v.parse::<u64>())
        .collect::<Result<_, _>>()
        .ok()?;
    if values.len() < 4 {
        return None;
    }
    let total = values
        .iter()
        .try_fold(0u64, |sum, value| sum.checked_add(*value))?;
    let idle = values[3].checked_add(values.get(4).copied().unwrap_or(0))?;
    Some(CpuSample {
        busy: total - idle,
        total,
    })
}

/// Byte counters for one network interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NetSample {
    pub rx_bytes: u64,
    pub tx_bytes: u64,
}

/// Keep interfaces separate: tunnel traffic can also appear on its physical link.
pub fn parse_net_dev(proc_net_dev: &str) -> std::collections::BTreeMap<String, NetSample> {
    let mut samples = std::collections::BTreeMap::new();
    for line in proc_net_dev.lines() {
        let Some((iface, counters)) = line.split_once(':') else {
            continue;
        };
        let iface = iface.trim();
        if iface.is_empty() || iface == "lo" || iface.contains(char::is_whitespace) {
            continue;
        }
        let fields: Vec<&str> = counters.split_whitespace().collect();
        // rx_bytes is field 0; tx_bytes is field 8.
        if fields.len() < 9 {
            continue;
        }
        let (Ok(rx), Ok(tx)) = (fields[0].parse::<u64>(), fields[8].parse::<u64>()) else {
            continue;
        };
        samples.insert(
            iface.to_string(),
            NetSample {
                rx_bytes: rx,
                tx_bytes: tx,
            },
        );
    }
    samples
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

/// Whether `permission` is currently granted to a package, read from
/// `dumpsys package <pkg>`. The grant lives on a line like
/// `android.permission.RECORD_AUDIO: granted=true, flags=[ ... ]`. Returns
/// `None` if the permission isn't listed (package missing, or it doesn't
/// declare the permission) so callers can distinguish "revoked" from "absent".
pub fn parse_permission_granted(dumpsys_package: &str, permission: &str) -> Option<bool> {
    let needle = format!("{permission}: granted=");
    dumpsys_package.lines().find_map(|line| {
        let rest = line.trim().strip_prefix(needle.as_str())?;
        Some(rest.starts_with("true"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn parses_permission_grant_state() {
        // Real `dumpsys package com.google.android.katniss` shape — the name
        // appears once on its own line, then again with the grant.
        let granted = "      android.permission.RECORD_AUDIO\n        \
            android.permission.RECORD_AUDIO: granted=true, flags=[ GRANTED_BY_DEFAULT ]";
        assert_eq!(
            parse_permission_granted(granted, "android.permission.RECORD_AUDIO"),
            Some(true)
        );
        let revoked =
            "        android.permission.RECORD_AUDIO: granted=false, flags=[ USER_FIXED ]";
        assert_eq!(
            parse_permission_granted(revoked, "android.permission.RECORD_AUDIO"),
            Some(false)
        );
        // Permission not present at all (or wrong package) → None.
        assert_eq!(
            parse_permission_granted("nothing here", "android.permission.RECORD_AUDIO"),
            None
        );
        // A bare name line without ": granted=" must not match.
        assert_eq!(
            parse_permission_granted(
                "      android.permission.RECORD_AUDIO",
                "android.permission.RECORD_AUDIO"
            ),
            None
        );
    }

    #[test]
    fn parses_usage_stats_last_used_and_never_opened() {
        let dump = r#"
      package=com.netflix.ninja totalTimeUsed="07:22" lastTimeUsed="2026-05-25 16:37:01" appLaunchCount=3 lastTimeStamp=1780669160342
      package=com.netflix.ninja totalTimeUsed="01:00" lastTimeUsed="2026-06-01 09:00:00" appLaunchCount=5 lastTimeStamp=1780669160342
      package=com.android.shell totalTimeUsed="00:00" lastTimeUsed="1969-12-31 18:00:00" appLaunchCount=0 lastTimeStamp=1780669160342
"#;
        let map = parse_usage_stats(dump);
        let netflix = map.get("com.netflix.ninja").expect("netflix present");
        // Keeps the most recent timestamp and the highest launch count across buckets.
        assert_eq!(netflix.last_used.as_deref(), Some("2026-06-01 09:00:00"));
        assert_eq!(netflix.launch_count, 5);
        let shell = map.get("com.android.shell").expect("shell present");
        assert_eq!(shell.last_used, None, "1969 epoch == never opened");
        assert_eq!(shell.launch_count, 0);
    }

    #[test]
    fn parses_ls_rows_including_spaced_names_and_symlinks() {
        let input = "total 64\n\
            drwxrwx--x 2 u0_a123 sdcard_rw 4096 2026-05-01 10:30 Download\n\
            -rw-rw---- 1 u0_a123 sdcard_rw 1048576 2026-05-02 11:00 movie trailer.mp4\n\
            lrwxrwxrwx 1 root root 11 2026-01-01 00:00 shortcut -> /sdcard/Download\n\
            ls: /sdcard/secret: Permission denied\n";
        let entries = parse_ls_output(input);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].name, "Download");
        assert!(entries[0].is_dir);
        assert_eq!(entries[1].name, "movie trailer.mp4");
        assert!(!entries[1].is_dir);
        assert_eq!(entries[1].size_bytes, 1_048_576);
        assert_eq!(entries[1].modified, "2026-05-02 11:00");
        assert_eq!(entries[2].name, "shortcut");
        assert!(entries[2].is_symlink);
    }

    #[test]
    fn ls_parse_returns_empty_on_error_output() {
        assert!(parse_ls_output("ls: /sdcard/nope: No such file or directory\n").is_empty());
        assert!(parse_ls_output("").is_empty());
    }

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
    fn parses_real_mdns_services_output() {
        // Captured verbatim from `adb mdns services` (platform-tools 37.0.0)
        // on a LAN with one Android 11+ TV and several legacy Shields.
        let input = "List of discovered mdns services\n\
            adb-58040DLCH005YV-jBeCEe\t_adb-tls-connect._tcp\t192.168.42.211:41541\n\
            adb-1321920044953\t_adb._tcp\t192.168.42.143:5555\n\
            adb-0323716101827\t_adb._tcp\t192.168.42.71:5555\n";
        let services = parse_mdns_services(input);

        assert_eq!(services.len(), 3);
        assert_eq!(services[0].instance, "adb-58040DLCH005YV-jBeCEe");
        assert_eq!(services[0].service, MDNS_SERVICE_CONNECT);
        assert_eq!(services[0].host, "192.168.42.211");
        // The whole point: a modern device is on a random port, not 5555.
        assert_eq!(services[0].port, 41541);
        assert_eq!(services[0].endpoint(), "192.168.42.211:41541");
        assert!(services[0].is_connectable());
        assert!(!services[0].is_pairing());
        assert_eq!(services[1].port, 5555);
        assert!(services[1].is_connectable());
    }

    #[test]
    fn pairing_service_is_not_offered_as_a_connect_endpoint() {
        // Connecting to the pairing port always fails. Keeping the two apart
        // is the whole of the #88 fix.
        let input = "List of discovered mdns services\n\
            adb-58040DLCH005YV-A1b2C3\t_adb-tls-pairing._tcp\t192.168.42.211:37199\n\
            adb-58040DLCH005YV-jBeCEe\t_adb-tls-connect._tcp\t192.168.42.211:41541\n";
        let services = parse_mdns_services(input);

        assert_eq!(services.len(), 2);
        assert!(services[0].is_pairing());
        assert!(!services[0].is_connectable());
        assert!(services[1].is_connectable());
        assert!(!services[1].is_pairing());
        // Same TV, different ports — neither may be substituted for the other.
        assert_eq!(services[0].host, services[1].host);
        assert_ne!(services[0].port, services[1].port);
        // Different suffixes, same embedded hardware serial.
        assert_ne!(services[0].instance, services[1].instance);
        assert_eq!(services[0].instance_serial(), Some("58040DLCH005YV"));
        assert_eq!(services[1].instance_serial(), Some("58040DLCH005YV"));
    }

    #[test]
    fn instance_serial_rejects_malformed_or_unknown_names() {
        assert_eq!(
            instance_serial("adb-58040DLCH005YV-jBeCEe"),
            Some("58040DLCH005YV")
        );
        assert_eq!(instance_serial("adb-AB-12-CD-x9"), Some("AB-12-CD"));
        assert_eq!(instance_serial("adb-unknown-jBeCEe"), None);
        assert_eq!(instance_serial("adb--jBeCEe"), None);
        assert_eq!(
            instance_serial("adb-1321920044953"),
            None,
            "legacy name has no suffix"
        );
        assert_eq!(instance_serial("adb-58040DLCH005YV-"), None);
        assert_eq!(instance_serial("Living Room TV"), None);
    }

    #[test]
    fn a_bracketed_ipv6_transport_is_a_network_connection() {
        let entries = parse_device_list(
            "List of devices attached\n\
             [fe80::1]:41541\tdevice\n\
             [fe80::1%en0]:41541\tunauthorized\n\
             0323220054321\tdevice\n",
        );
        assert_eq!(entries[0].connection, ConnectionType::Network);
        assert_eq!(entries[1].connection, ConnectionType::Network);
        assert_eq!(entries[2].connection, ConnectionType::Usb);
        assert!(is_network_endpoint("192.168.1.5:5555"));
        assert!(!is_network_endpoint("[not-an-ip]:5555"));
        assert!(!is_network_endpoint("[fe80::1]"));
    }

    #[test]
    fn ipv6_endpoint_is_bracketed_like_the_adb_transport_key() {
        let input = "List of discovered mdns services\n\
            adb-v6-x\t_adb-tls-connect._tcp\tfe80::1:41541\n";
        let services = parse_mdns_services(input);
        assert_eq!(services[0].endpoint(), "[fe80::1]:41541");
    }

    #[test]
    fn unparseable_mdns_rows_are_dropped_not_guessed() {
        let input = "List of discovered mdns services\n\
            \n\
            adb-missing-port\t_adb._tcp\t192.168.42.9\n\
            adb-bad-port\t_adb._tcp\t192.168.42.9:not-a-port\n\
            adb-zero-port\t_adb._tcp\t192.168.42.9:0\n\
            adb-no-host\t_adb._tcp\t:5555\n\
            adb-truncated\t_adb._tcp\n\
            some-printer\t_ipp._tcp\t192.168.42.50:631\n\
            adb-good\t_adb-tls-connect._tcp\t192.168.42.9:41541\n";
        let services = parse_mdns_services(input);

        assert_eq!(services.len(), 1);
        assert_eq!(services[0].instance, "adb-good");
        assert_eq!(services[0].port, 41541);
    }

    #[test]
    fn mdns_endpoint_keeps_an_ipv6_host_intact() {
        let input = "List of discovered mdns services\n\
            adb-v6\t_adb-tls-connect._tcp\tfe80::1c2d:3e4f:5a6b:7c8d:41541\n";
        let services = parse_mdns_services(input);

        assert_eq!(services.len(), 1);
        assert_eq!(services[0].host, "fe80::1c2d:3e4f:5a6b:7c8d");
        assert_eq!(services[0].port, 41541);
    }

    #[test]
    fn empty_mdns_output_is_not_an_error() {
        assert!(parse_mdns_services("List of discovered mdns services\n").is_empty());
        assert!(parse_mdns_services("").is_empty());
    }

    #[test]
    fn mdns_wireless_debugging_serial_is_network_not_usb() {
        // Android 11+ wireless debugging registers over mDNS; the serial is the
        // service name, not an ip:port. It must still classify as Network.
        let input = "List of devices attached\n\
            adb-58040DLCH005YV-jBeCEe._adb-tls-connect._tcp\tdevice\n";
        let entries = parse_device_list(input);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].connection, ConnectionType::Network);
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
    fn parses_every_display_mode_and_flags_the_active_one() {
        // Same shape as the real dumpsys, plus a 23.976 mode — the one the
        // playback report exists to find, and never the active one in practice.
        let input = r#"
DisplayDeviceInfo{"Built-in Screen": 3840 x 2160, modeId 20, defaultModeId 20, supportedModes [{id=1, width=3840, height=2160, fps=23.976023}, {id=2, width=3840, height=2160, fps=29.97003}, {id=20, width=3840, height=2160, fps=59.94006}], ...}
"#;
        let modes = parse_display_modes(input);
        assert_eq!(modes.len(), 3);
        // Sorted by resolution then fps, descending.
        assert_eq!(modes[0].fps, 59.94);
        assert_eq!(modes[2].fps, 23.976);
        assert!(modes[0].active);
        assert!(!modes[1].active && !modes[2].active);
        assert!(modes.iter().filter(|m| m.is_film_rate()).count() == 1);
    }

    #[test]
    fn duplicate_modes_collapse_but_keep_the_active_flag() {
        // The same capability advertised under two ids, the *second* of which
        // is the active one — the flag has to survive the merge.
        let input = "modeId 7, supportedModes [{id=3, width=1920, height=1080, fps=60.0}, \
                     {id=7, width=1920, height=1080, fps=60.0}]";
        let modes = parse_display_modes(input);
        assert_eq!(modes.len(), 1);
        assert!(modes[0].active);
    }

    #[test]
    fn display_modes_degrade_to_empty_rather_than_guessing() {
        assert!(parse_display_modes("").is_empty());
        assert!(parse_display_modes("no modes here").is_empty());
        // Modes present but no active id: every mode is reported, none active.
        let modes =
            parse_display_modes("supportedModes [{id=1, width=3840, height=2160, fps=24.0}]");
        assert_eq!(modes.len(), 1);
        assert!(!modes[0].active);
    }

    #[test]
    fn display_reports_scope_modes_and_hdr_to_the_same_default_record() {
        let input = concat!(
            "DisplayDeviceInfo{\"Auxiliary\": modeId 1, supportedModes [{id=1, width=1920, height=1080, fps=24.0}], HdrCapabilities{mSupportedHdrTypes=[1]}}\n",
            "  mInfo=DisplayDeviceInfo{\"Main\": modeId 1, supportedModes [{id=1, width=3840, height=2160, fps=60.0}], HdrCapabilities{mSupportedHdrTypes=[2]}, FLAG_DEFAULT_DISPLAY}\n",
            "Logical display modes [{id=1, width=1280, height=720, fps=24.0}]"
        );
        let active = parse_display_mode(input);
        assert_eq!(active.resolution.as_deref(), Some("3840x2160"));
        assert_eq!(active.refresh_hz, Some(60.0));
        assert_eq!(active.hdr_types, vec!["HDR10"]);
        let modes = parse_display_modes(input);
        assert_eq!(modes.len(), 1);
        assert!(modes[0].active);
        assert!(!modes[0].is_film_rate());
    }

    #[test]
    fn multiple_displays_without_a_unique_default_are_unavailable() {
        for flag in [
            "",
            ", FLAG_ALLOWED_TO_BE_DEFAULT_DISPLAY",
            ", FLAG_DEFAULT_DISPLAY",
        ] {
            let input = format!(
                "DisplayDeviceInfo{{\"One\": modeId 1, supportedModes [{{id=1, width=3840, height=2160, fps=60.0}}], HdrCapabilities{{mSupportedHdrTypes=[2]}}{flag}}}\n\
                 DisplayDeviceInfo{{\"Two\": modeId 1, supportedModes [{{id=1, width=1920, height=1080, fps=24.0}}]{flag}}}"
            );
            assert!(parse_display_modes(&input).is_empty());
            let active = parse_display_mode(&input);
            assert_eq!(active.resolution, None);
            assert_eq!(active.refresh_hz, None);
            assert!(active.hdr_types.is_empty());
        }
    }

    #[test]
    fn repeated_single_display_records_do_not_create_ambiguity() {
        let record = "DisplayDeviceInfo{\"Main\": modeId 1, supportedModes [{id=1, width=3840, height=2160, fps=60.0}], FLAG_ALLOWED_TO_BE_DEFAULT_DISPLAY}";
        let input = format!("{record}\n  mInfo={record}");
        assert_eq!(parse_display_modes(&input).len(), 1);
        assert_eq!(parse_display_mode(&input).refresh_hz, Some(60.0));
    }

    #[test]
    fn one_eligible_display_does_not_prove_the_current_default() {
        let input = concat!(
            "DisplayDeviceInfo{\"One\": modeId 1, supportedModes [{id=1, width=3840, height=2160, fps=60.0}], FLAG_ALLOWED_TO_BE_DEFAULT_DISPLAY}\n",
            "DisplayDeviceInfo{\"Two\": modeId 2, supportedModes [{id=2, width=1920, height=1080, fps=24.0}]}"
        );
        assert!(parse_display_modes(input).is_empty());
        assert_eq!(parse_display_mode(input).refresh_hz, None);
    }

    #[test]
    fn proc_stat_counts_iowait_as_idle() {
        // user nice system idle iowait irq softirq steal
        let sample =
            parse_proc_stat("cpu  100 20 30 700 50 5 5 0\ncpu0 1 2 3 4 5 6 7 8\n").unwrap();
        assert_eq!(sample.total, 910);
        // idle(700) + iowait(50) are not busy.
        assert_eq!(sample.busy, 160);
    }

    #[test]
    fn proc_stat_ignores_per_core_lines_and_missing_input() {
        // `cpu0` must not be mistaken for the aggregate `cpu` line.
        assert_eq!(parse_proc_stat("cpu0 1 2 3 4 5 6 7 8"), None);
        assert_eq!(parse_proc_stat(""), None);
        assert_eq!(parse_proc_stat("cpu 1 2"), None);
    }

    #[test]
    fn net_dev_preserves_interfaces_and_skips_loopback() {
        let input = "\
Inter-|   Receive                                                |  Transmit\n\
 face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed\n\
    lo: 999999    100    0    0    0     0          0         0  999999     100    0    0    0     0       0          0\n\
  eth0: 100000    500    0    0    0     0          0         0   20000     300    0    0    0     0       0          0\n\
 wlan0:  50000    250    0    0    0     0          0         0   10000     150    0    0    0     0       0          0\n";
        let samples = parse_net_dev(input);
        assert_eq!(samples.len(), 2);
        assert_eq!(samples["eth0"].rx_bytes, 100_000);
        assert_eq!(samples["wlan0"].tx_bytes, 10_000);
    }

    #[test]
    fn net_dev_with_no_usable_interfaces_is_empty() {
        // Header only, loopback only, and garbage all mean "no reading" —
        // distinct from a real zero, which would misreport as idle traffic.
        assert!(parse_net_dev("").is_empty());
        assert!(parse_net_dev("Inter-|   Receive  |  Transmit").is_empty());
        assert!(parse_net_dev("    lo: 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16").is_empty());
        assert!(parse_net_dev("eth0: invalid 2 3 4 5 6 7 8 9").is_empty());
    }

    #[test]
    fn proc_stat_rejects_invalid_or_overflowing_counters() {
        assert_eq!(parse_proc_stat("cpu 1 bad 3 4"), None);
        assert_eq!(parse_proc_stat("cpu 18446744073709551615 1 0 0"), None);
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
    fn parses_hardware_properties_temp_from_cpu_and_gpu() {
        // Real Shield 2019 dumpsys hardware_properties shape — current temps,
        // ignoring throttling/shutdown limit lines.
        let input = "CPU temperatures: [32.0, 40.5, 32.0, 32.0]\n\
                     CPU throttling temperatures: [89.0, 89.0, 89.0, 89.0]\n\
                     CPU shutdown temperatures: [102.5, 102.5, 102.5, 102.5]\n\
                     GPU temperatures: [33.0]\n\
                     GPU throttling temperatures: [90.5]\n";
        // Max current reading is the 40.5 CPU core; limits are excluded.
        assert_eq!(parse_hardware_properties_temp(input), Some(40.5));
    }

    #[test]
    fn hardware_properties_temp_none_when_empty() {
        assert_eq!(
            parse_hardware_properties_temp("Battery temperatures: []"),
            None
        );
        assert_eq!(parse_hardware_properties_temp(""), None);
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
    fn parses_real_shield_df_output() {
        // Verbatim from a live Nvidia Shield: mount point is /data/user/0 (still
        // contains "/data"); non-wrapped row.
        let input = "\
            Filesystem            Size  Used Avail Use% Mounted on\n\
            /dev/block/mmcblk0p32  12G  3.7G  7.7G  33% /data/user/0\n";
        let info = parse_storage_info(input);
        assert_eq!(info.total.as_deref(), Some("12G"));
        assert_eq!(info.used.as_deref(), Some("3.7G"));
        assert_eq!(info.available.as_deref(), Some("7.7G"));
        assert_eq!(info.used_percent, Some(33));
    }

    #[test]
    fn parses_df_data_storage_when_filesystem_name_wraps() {
        // A long filesystem path wraps the row onto its own line — the data
        // columns then start at index 0. The %-anchored parse must still work.
        let input = "\
            Filesystem                        Size  Used  Avail  Use%  Mounted on\n\
            /dev/block/dm-9\n\
            113G   98G   15G  87%  /data\n";
        let info = parse_storage_info(input);
        assert_eq!(info.total.as_deref(), Some("113G"));
        assert_eq!(info.used.as_deref(), Some("98G"));
        assert_eq!(info.available.as_deref(), Some("15G"));
        assert_eq!(info.used_percent, Some(87));
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

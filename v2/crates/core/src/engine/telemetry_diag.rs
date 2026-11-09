//! What the app saw when a scan found nothing or a TV gave no details, for
//! the anonymous reports.
//!
//! Pure. Each builder takes the raw text or values the host layer gathered,
//! which can hold serials, addresses, mDNS instance names, interface names
//! and paths, and keeps only what a [`Diag`] can carry: enums from fixed
//! lists, bounded counts, durations, version numbers and error codes from
//! [`classify_error_text`]. No field is free text, so there is no redaction
//! rule to miss; the tests feed hostile input to prove it.

use std::net::{IpAddr, Ipv4Addr};
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

use super::telemetry::TelemetryAction;
use super::types::{ConnectionType, DeviceProperties, DeviceStatus};

const MAX_COUNT: usize = 9_999;
const MAX_CODES: usize = 8;
const MAX_PREFIXES: usize = 8;
const MAX_MS: u64 = 3_600_000;

fn count(n: usize) -> u32 {
    n.min(MAX_COUNT) as u32
}

fn ms(n: u128) -> u64 {
    n.min(MAX_MS as u128) as u64
}

/// Why a diagnostic report was sent. Its code is the event's `error_code`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiagReason {
    /// Scan LAN found no device on the network.
    ScanEmpty,
    /// Scan LAN found devices but connected to none of them.
    ScanConnectFailed,
    /// Wireless debugging discovery (mobile) found no service.
    DiscoverEmpty,
    /// adb lists a device but it gave no usable details: unauthorized,
    /// offline, `getprop` failed, or make and model came back blank.
    DeviceDetailsEmpty,
}

impl DiagReason {
    pub fn code(self) -> &'static str {
        match self {
            Self::ScanEmpty => "scan_empty",
            Self::ScanConnectFailed => "scan_connect_failed",
            Self::DiscoverEmpty => "discover_empty",
            Self::DeviceDetailsEmpty => "device_details_empty",
        }
    }

    pub fn action(self) -> TelemetryAction {
        match self {
            Self::ScanEmpty | Self::ScanConnectFailed | Self::DiscoverEmpty => {
                TelemetryAction::Scan
            }
            Self::DeviceDetailsEmpty => TelemetryAction::Connect,
        }
    }
}

/// Where the adb binary came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdbSource {
    /// The platform-tools the app downloaded into its own data dir.
    Bundled,
    /// `ANDROID_HOME` / `ANDROID_SDK_ROOT`.
    Sdk,
    /// PATH or a well-known install location (Homebrew, /usr/bin…).
    System,
    /// `ATV_OPTIMIZER_ADB` (or its old name), set by the user.
    Custom,
    /// The phone app's in-process adb client; no binary or server.
    Builtin,
    /// No adb binary was found.
    None,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServerState {
    Reachable,
    Unreachable,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdbDiag {
    pub source: AdbSource,
    /// `1.0.41`: the adb protocol version line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// `35.0.2`: the platform-tools release.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server: Option<ServerState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_error: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransportCounts {
    pub device: u32,
    pub unauthorized: u32,
    pub offline: u32,
    pub other: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MdnsCheck {
    Ok,
    Unavailable,
    Failed,
    NotRun,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MdnsDiag {
    pub check: MdnsCheck,
    pub services: u32,
    pub connect: u32,
    pub pairing: u32,
    pub legacy: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Gateway {
    Detected,
    NotDetected,
    /// `ATV_OPTIMIZER_SUBNET` (or its old name) chose the subnet.
    Override,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetDiag {
    /// Usable IPv4 addresses (not loopback, not link-local).
    pub ipv4: u32,
    pub ipv6: u32,
    /// 169.254.x.x: an interface that never got a DHCP lease.
    pub link_local: u32,
    /// Interfaces whose name looks like a VPN or tunnel. The name itself is
    /// never sent.
    pub vpn: u32,
    /// Distinct IPv4 prefix lengths, e.g. `[24]` or `[22, 24]`.
    pub prefixes: Vec<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gateway: Option<Gateway>,
    /// The swept /24 lies inside a local interface's network. `false` means
    /// the scan probed a subnet this computer isn't on (a VPN default route,
    /// say).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subnet_local: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SweepDiag {
    pub probed: u32,
    pub open: u32,
    pub timeouts: u32,
    pub refused: u32,
    pub ms: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectDiag {
    pub attempted: u32,
    pub connected: u32,
    pub unauthorized: u32,
    pub failed: u32,
    pub pairing: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceDiag {
    /// `device`, `unauthorized`, `offline` or `other`.
    pub state: String,
    /// `usb` or `network`.
    pub transport: String,
    /// Property keys that came back empty, from [`PROP_KEYS`] only.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub empty_props: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveryDiag {
    pub timeout_ms: u64,
    pub found: u32,
}

/// The structured half of a diagnostic report. Every part is optional: a
/// report carries what the path that sent it could see.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diag {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adb: Option<AdbDiag>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transports: Option<TransportCounts>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mdns: Option<MdnsDiag>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub net: Option<NetDiag>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sweep: Option<SweepDiag>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connect: Option<ConnectDiag>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<DeviceDiag>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discovery: Option<DiscoveryDiag>,
    /// Underlying error codes, deduplicated.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
    /// How long the whole operation took.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ms: Option<u64>,
}

impl Diag {
    pub fn with_ms(mut self, elapsed: std::time::Duration) -> Self {
        self.ms = Some(ms(elapsed.as_millis()));
        self
    }
}

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).expect("static diag pattern")
}

/// Error text to a code from a fixed vocabulary, matching the codes
/// `shared/errors.ts` shows users where the two overlap. First match wins,
/// so the specific shapes come before the broad ones.
static ERROR_RULES: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    [
        (
            r"(?i)could not locate an adb binary|adb binary at .* is not a regular file",
            "adb_missing",
        ),
        (
            r"(?i)cannot connect to daemon|daemon not running|failed to start daemon|could not install \*smartsocket\*|adb server version .* doesn't match|server version .* doesn't match",
            "adb_server",
        ),
        // Before the network shapes: a bind failure can say "address in use".
        (
            r"(?i)mdns\b.*(failed to bind|address already in use|address in use|EADDRINUSE)|mdns daemon unavailable",
            "mdns_unavailable",
        ),
        (
            r"(?i)failed to authenticate|in state 'unauthorized'|\bunauthori[sz]ed\b",
            "device_unauthorized",
        ),
        (r"(?i)in state 'offline'|device offline|\boffline\b", "device_offline"),
        (
            r"(?i)no devices/emulators found|device .* not found|not connected to a device",
            "not_connected",
        ),
        (
            r"(?i)wrong password|incorrect (pin|code|password)|bad (pin|code)|code (didn't|didn’t|did not) match",
            "wrong_pin",
        ),
        (
            r"(?i)pairing (was )?(cancel|aborted|expired)|unable to start pairing client",
            "pairing_expired",
        ),
        (r"(?i)connection refused|actively refused", "connection_refused"),
        (r"(?i)connection reset|broken pipe", "connection_reset"),
        (r"(?i)timed out|timeout", "timeout"),
        (
            r"(?i)no route to host|network is unreachable|host is (down|unreachable)",
            "unreachable",
        ),
        (
            r"(?i)no such host|nodename nor servname|name or service not known|could not resolve",
            "dns_failure",
        ),
        (r"(?i)protocol fault|couldn't read status", "protocol_fault"),
        (
            r"(?i)could not connect|cannot connect|failed to connect|unable to connect",
            "connect_failed",
        ),
        (r"(?i)permission denied|access is denied|operation not permitted", "permission_denied"),
        (r"(?i)empty response|produced nothing|device profile unavailable", "empty_response"),
        (r"(?i)stale transport", "stale_transport"),
    ]
    .into_iter()
    .map(|(p, code)| (re(p), code))
    .collect()
});

/// The code for one error text, or `unknown`. Never returns any of the text.
pub fn classify_error_text(text: &str) -> &'static str {
    ERROR_RULES
        .iter()
        .find(|(pattern, _)| pattern.is_match(text))
        .map(|(_, code)| *code)
        .unwrap_or("unknown")
}

/// Codes for several error texts, deduplicated in first-seen order.
pub fn error_codes<'a>(texts: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for text in texts {
        let code = classify_error_text(text);
        if !out.iter().any(|c| c == code) {
            out.push(code.to_string());
        }
        if out.len() >= MAX_CODES {
            break;
        }
    }
    out
}

static ADB_VERSION: LazyLock<Regex> =
    LazyLock::new(|| re(r"Android Debug Bridge version (\d{1,3}(?:\.\d{1,4}){1,3})\b"));
static TOOLS_VERSION: LazyLock<Regex> =
    LazyLock::new(|| re(r"(?m)^\s*Version (\d{1,3}(?:\.\d{1,4}){1,3})\b"));

/// `adb version` prints the protocol version, the platform-tools release and
/// `Installed as <path>`. Only the two version numbers are kept.
pub fn adb_diag(
    source: AdbSource,
    version_output: Option<&str>,
    server: Option<Result<(), &str>>,
) -> AdbDiag {
    let capture = |pattern: &Regex| {
        version_output
            .and_then(|text| pattern.captures(text))
            .map(|c| c[1].to_string())
    };
    AdbDiag {
        source,
        version: capture(&ADB_VERSION),
        tools: capture(&TOOLS_VERSION),
        server: server.map(|r| {
            if r.is_ok() {
                ServerState::Reachable
            } else {
                ServerState::Unreachable
            }
        }),
        server_error: match server {
            Some(Err(text)) => Some(classify_error_text(text).to_string()),
            _ => None,
        },
    }
}

/// Count `adb devices` rows by state. The serial column is read past, never
/// kept.
pub fn transport_counts(devices_stdout: &str) -> TransportCounts {
    let mut c = [0usize; 4];
    for line in devices_stdout.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("List of devices") || line.starts_with('*') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let (Some(_serial), Some(state)) = (parts.next(), parts.next()) else {
            continue;
        };
        let i = match DeviceStatus::from_adb_str(state) {
            Some(DeviceStatus::Device) => 0,
            Some(DeviceStatus::Unauthorized) => 1,
            Some(DeviceStatus::Offline) => 2,
            None => 3,
        };
        c[i] += 1;
    }
    TransportCounts {
        device: count(c[0]),
        unauthorized: count(c[1]),
        offline: count(c[2]),
        other: count(c[3]),
    }
}

/// `check` is `adb mdns check`'s output (`Ok`) or error text (`Err`), or
/// `None` when it wasn't run. `service_types` are the advertised service
/// types (`_adb-tls-connect._tcp`…); instance names are not taken.
pub fn mdns_diag(check: Option<Result<&str, &str>>, service_types: &[&str]) -> MdnsDiag {
    let check = match check {
        None => MdnsCheck::NotRun,
        Some(Ok(text)) | Some(Err(text)) if text.to_ascii_lowercase().contains("unavailable") => {
            MdnsCheck::Unavailable
        }
        Some(Ok(text)) if text.to_ascii_lowercase().contains("version") => MdnsCheck::Ok,
        Some(_) => MdnsCheck::Failed,
    };
    let of = |kind: &str| service_types.iter().filter(|t| t.trim() == kind).count();
    MdnsDiag {
        check,
        services: count(service_types.len()),
        connect: count(of("_adb-tls-connect._tcp")),
        pairing: count(of("_adb-tls-pairing._tcp")),
        legacy: count(of("_adb._tcp")),
    }
}

/// One local interface address as the OS reported it. Only counts and
/// prefix lengths derived from these leave [`net_diag`].
#[derive(Debug, Clone)]
pub struct NetInterface<'a> {
    pub name: &'a str,
    pub addr: IpAddr,
    pub prefix: u8,
}

const VPN_NAME_PREFIXES: [&str; 12] = [
    "utun",
    "tun",
    "tap",
    "wg",
    "ppp",
    "ipsec",
    "tailscale",
    "zt",
    "nordlynx",
    "gpd",
    "proton",
    "mullvad",
];

fn vpn_like(name: &str) -> bool {
    let lower = name.trim().to_ascii_lowercase();
    lower.contains("vpn") || VPN_NAME_PREFIXES.iter().any(|p| lower.starts_with(p))
}

fn v4_mask(prefix: u8) -> u32 {
    if prefix == 0 {
        0
    } else {
        u32::MAX << (32 - u32::from(prefix.min(32)))
    }
}

/// Does `swept` (a /24) overlap the network of `addr/prefix`?
fn covers(addr: Ipv4Addr, prefix: u8, swept: [u8; 3]) -> bool {
    let probe = u32::from(Ipv4Addr::new(swept[0], swept[1], swept[2], 0));
    let own = u32::from(addr);
    let mask = v4_mask(prefix.min(24));
    own & mask == probe & mask
}

pub fn net_diag(
    interfaces: &[NetInterface],
    gateway: Option<Gateway>,
    swept: Option<[u8; 3]>,
) -> NetDiag {
    let mut ipv4 = 0;
    let mut ipv6 = 0;
    let mut link_local = 0;
    let mut vpn_names: Vec<&str> = Vec::new();
    let mut prefixes: Vec<u8> = Vec::new();
    let mut subnet_local = false;
    for iface in interfaces {
        if iface.addr.is_loopback() || iface.addr.is_unspecified() {
            continue;
        }
        if vpn_like(iface.name) && !vpn_names.contains(&iface.name) {
            vpn_names.push(iface.name);
        }
        match iface.addr {
            IpAddr::V4(v4) if v4.is_link_local() => link_local += 1,
            IpAddr::V4(v4) => {
                ipv4 += 1;
                let prefix = iface.prefix.min(32);
                if !prefixes.contains(&prefix) {
                    prefixes.push(prefix);
                }
                if swept.is_some_and(|s| covers(v4, prefix, s)) {
                    subnet_local = true;
                }
            }
            IpAddr::V6(_) => ipv6 += 1,
        }
    }
    prefixes.sort_unstable();
    prefixes.truncate(MAX_PREFIXES);
    NetDiag {
        ipv4: count(ipv4),
        ipv6: count(ipv6),
        link_local: count(link_local),
        vpn: count(vpn_names.len()),
        prefixes,
        gateway,
        subnet_local: swept.map(|_| subnet_local),
    }
}

pub fn sweep_diag(
    probed: usize,
    open: usize,
    timeouts: usize,
    refused: usize,
    elapsed: std::time::Duration,
) -> SweepDiag {
    SweepDiag {
        probed: count(probed),
        open: count(open),
        timeouts: count(timeouts),
        refused: count(refused),
        ms: ms(elapsed.as_millis()),
    }
}

/// Counts from a scan's connect stage, plus the codes behind its failures.
/// `failure_details` are the raw per-endpoint texts (`192.168.1.5:5555:
/// failed to connect…`); only their codes are kept.
pub fn connect_diag(
    attempted: usize,
    connected: usize,
    unauthorized: usize,
    failed: usize,
    pairing: usize,
    failure_details: &[&str],
) -> ConnectDiag {
    ConnectDiag {
        attempted: count(attempted),
        connected: count(connected),
        unauthorized: count(unauthorized),
        failed: count(failed),
        pairing: count(pairing),
        errors: error_codes(failure_details.iter().copied()),
    }
}

/// The property keys a report may name. A key outside this list is never
/// sent, whatever the caller passes.
pub const PROP_KEYS: [&str; 9] = [
    "ro.product.manufacturer",
    "ro.product.brand",
    "ro.product.model",
    "ro.product.device",
    "ro.build.version.release",
    "ro.build.version.sdk",
    "ro.build.characteristics",
    "ro.serialno",
    "android.software.leanback",
];

/// Which of the identity properties came back empty. Values are never read
/// past "is it blank".
pub fn empty_props(props: &DeviceProperties) -> Vec<String> {
    let blank = |v: &str| {
        let v = v.trim();
        v.is_empty() || v.eq_ignore_ascii_case("unknown")
    };
    let checks = [
        blank(&props.manufacturer),
        blank(&props.brand),
        blank(&props.model),
        blank(&props.device_codename),
        blank(&props.android_release),
        blank(&props.sdk_level),
        blank(&props.characteristics),
        blank(&props.serial_number),
        props.leanback.is_none(),
    ];
    PROP_KEYS
        .iter()
        .zip(checks)
        .filter(|(_, empty)| *empty)
        .map(|(key, _)| key.to_string())
        .collect()
}

/// A device that gave no usable details: blank make and model.
pub fn lacks_details(props: &DeviceProperties) -> bool {
    props.manufacturer.trim().is_empty() && props.model.trim().is_empty()
}

pub fn device_diag(
    status: Option<DeviceStatus>,
    connection: ConnectionType,
    props: Option<&DeviceProperties>,
    error: Option<&str>,
) -> DeviceDiag {
    DeviceDiag {
        state: match status {
            Some(DeviceStatus::Device) => "device",
            Some(DeviceStatus::Unauthorized) => "unauthorized",
            Some(DeviceStatus::Offline) => "offline",
            None => "other",
        }
        .to_string(),
        transport: match connection {
            ConnectionType::Usb => "usb",
            ConnectionType::Network => "network",
        }
        .to_string(),
        empty_props: props.map(empty_props).unwrap_or_default(),
        error: error.map(|e| classify_error_text(e).to_string()),
    }
}

pub fn discovery_diag(timeout_ms: u64, found: usize) -> DiscoveryDiag {
    DiscoveryDiag {
        timeout_ms: timeout_ms.min(MAX_MS),
        found: count(found),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    /// Everything identifying the hostile inputs below contain. None may
    /// appear anywhere in a serialized report.
    const SECRETS: [&str; 16] = [
        "1324619053514",
        "ABCDEFABCDEF",
        "192.168",
        "10.0.0",
        "fe80",
        "3c:a6:f6",
        "Living Room",
        "Bryans-MacBook",
        "jBeCEe",
        "/Users/",
        "bryan",
        "C:\\",
        "Jane",
        "en0",
        "utun3",
        "com.example",
    ];

    fn assert_clean(diag: &Diag) {
        let text = serde_json::to_string(diag).unwrap();
        for secret in SECRETS {
            assert!(
                !text.to_lowercase().contains(&secret.to_lowercase()),
                "{secret} leaked into {text}"
            );
        }
    }

    #[test]
    fn adb_version_keeps_only_the_numbers() {
        let out = "Android Debug Bridge version 1.0.41\nVersion 35.0.2-12147458\nInstalled as /Users/bryan/Library/Application Support/ShieldOptimizer/platform-tools/adb\nRunning on Darwin 25.6.0 (arm64)\n";
        let d = adb_diag(AdbSource::Bundled, Some(out), Some(Ok(())));
        assert_eq!(d.version.as_deref(), Some("1.0.41"));
        assert_eq!(d.tools.as_deref(), Some("35.0.2"));
        assert_eq!(d.server, Some(ServerState::Reachable));
        let failed = adb_diag(
            AdbSource::System,
            Some("Installed as C:\\Users\\Jane Doe\\adb.exe"),
            Some(Err("* daemon not running; starting now at tcp:5037\nADB server didn't ACK\n* failed to start daemon")),
        );
        assert_eq!(failed.version, None);
        assert_eq!(failed.server, Some(ServerState::Unreachable));
        assert_eq!(failed.server_error.as_deref(), Some("adb_server"));
        assert_clean(&Diag {
            adb: Some(d),
            ..Default::default()
        });
        assert_clean(&Diag {
            adb: Some(failed),
            ..Default::default()
        });
        assert_eq!(
            serde_json::to_value(adb_diag(AdbSource::None, None, None)).unwrap(),
            serde_json::json!({ "source": "none" })
        );
    }

    #[test]
    fn transports_are_counted_never_listed() {
        let out = "* daemon started successfully\nList of devices attached\n192.168.1.20:5555\tdevice product:mdarcy model:SHIELD_Android_TV\nadb-1324619053514-jBeCEe._adb-tls-connect._tcp\tunauthorized\nABCDEFABCDEF\toffline\n10.0.0.7:5555\tno permissions (Living Room)\n\nlonely-line\n";
        let counts = transport_counts(out);
        assert_eq!(
            counts,
            TransportCounts {
                device: 1,
                unauthorized: 1,
                offline: 1,
                other: 1
            }
        );
        assert_clean(&Diag {
            transports: Some(counts),
            ..Default::default()
        });
    }

    #[test]
    fn mdns_reports_kinds_and_the_check_not_instances() {
        let kinds = [
            "_adb-tls-connect._tcp",
            "_adb-tls-pairing._tcp",
            "_adb._tcp",
            "_adb-tls-connect._tcp",
        ];
        let d = mdns_diag(
            Some(Ok("mdns daemon version [Openscreen discovery 0.0.0]")),
            &kinds,
        );
        assert_eq!(
            d,
            MdnsDiag {
                check: MdnsCheck::Ok,
                services: 4,
                connect: 2,
                pairing: 1,
                legacy: 1
            }
        );
        assert_eq!(
            mdns_diag(Some(Err("ERROR: mdns daemon unavailable")), &[]).check,
            MdnsCheck::Unavailable
        );
        assert_eq!(
            mdns_diag(Some(Ok("usage: adb ...")), &[]).check,
            MdnsCheck::Failed
        );
        assert_eq!(mdns_diag(None, &[]).check, MdnsCheck::NotRun);
    }

    #[test]
    fn interfaces_become_counts_and_prefixes() {
        let ifaces = [
            NetInterface {
                name: "lo0",
                addr: "127.0.0.1".parse().unwrap(),
                prefix: 8,
            },
            NetInterface {
                name: "en0",
                addr: "192.168.42.17".parse().unwrap(),
                prefix: 24,
            },
            NetInterface {
                name: "en0",
                addr: "fe80::1c2:3ff:fe4d:5e6f".parse().unwrap(),
                prefix: 64,
            },
            NetInterface {
                name: "utun3",
                addr: "10.0.0.9".parse().unwrap(),
                prefix: 32,
            },
            NetInterface {
                name: "Bryans-MacBook VPN (Jane)",
                addr: "10.0.0.10".parse().unwrap(),
                prefix: 32,
            },
            NetInterface {
                name: "en7",
                addr: "169.254.3.4".parse().unwrap(),
                prefix: 16,
            },
        ];
        let d = net_diag(&ifaces, Some(Gateway::Detected), Some([192, 168, 42]));
        assert_eq!(
            d,
            NetDiag {
                ipv4: 3,
                ipv6: 1,
                link_local: 1,
                vpn: 2,
                prefixes: vec![24, 32],
                gateway: Some(Gateway::Detected),
                subnet_local: Some(true),
            }
        );
        // A VPN default route sends the sweep to a subnet nothing local is on.
        let off = net_diag(&ifaces, Some(Gateway::Detected), Some([172, 16, 5]));
        assert_eq!(off.subnet_local, Some(false));
        // A /22 covers each /24 inside it.
        let wide = [NetInterface {
            name: "eth0",
            addr: "10.1.2.3".parse().unwrap(),
            prefix: 22,
        }];
        assert_eq!(
            net_diag(&wide, None, Some([10, 1, 1])).subnet_local,
            Some(true)
        );
        assert_eq!(
            net_diag(&wide, None, Some([10, 1, 4])).subnet_local,
            Some(false)
        );
        assert_eq!(net_diag(&wide, None, None).subnet_local, None);
        assert_clean(&Diag {
            net: Some(d),
            ..Default::default()
        });
    }

    #[test]
    fn connect_failures_keep_only_codes() {
        let details = [
            "192.168.1.20:5555: failed to connect to '192.168.1.20:5555': Connection refused",
            "adb-1324619053514-jBeCEe._adb-tls-connect._tcp: failed to authenticate to Living Room",
            "10.0.0.7:5555: cannot connect to 10.0.0.7:5555: Operation timed out",
            "ABCDEFABCDEF: something /Users/bryan/x.txt said",
            "[fe80::1]:5555: No route to host",
            "192.168.1.21:5555: failed to connect to '192.168.1.21:5555': Connection refused",
        ];
        let d = connect_diag(6, 0, 1, 5, 0, &details);
        assert_eq!(
            d.errors,
            vec![
                "connection_refused",
                "device_unauthorized",
                "timeout",
                "unknown",
                "unreachable"
            ]
        );
        assert_clean(&Diag {
            connect: Some(d),
            ..Default::default()
        });
    }

    #[test]
    fn error_codes_cover_the_adb_failures_users_hit() {
        for (text, code) in [
            ("could not locate an adb binary. Tried PATH", "adb_missing"),
            ("cannot connect to daemon at tcp:5037: Connection refused", "adb_server"),
            ("adb server version (40) doesn't match this client (41); killing...", "adb_server"),
            ("error: device offline", "device_offline"),
            ("error: no devices/emulators found", "not_connected"),
            ("failed to connect to '10.0.0.5:5555': Connection refused", "connection_refused"),
            ("cannot connect to 10.0.0.5:5555: No connection could be made because the target machine actively refused it. (10061)", "connection_refused"),
            ("adb command timed out after 30s", "timeout"),
            ("failed to connect to tv.local:5555: nodename nor servname provided", "dns_failure"),
            ("protocol fault (couldn't read status): Connection reset by peer", "connection_reset"),
            ("device profile unavailable: ", "empty_response"),
            ("something new", "unknown"),
        ] {
            assert_eq!(classify_error_text(text), code, "{text}");
        }
    }

    /// Same codes `shared/errors.ts` shows for these texts, so a report and
    /// the message the user saw name the same thing.
    #[test]
    fn pairing_code_and_mdns_bind_failures_match_the_shared_error_table() {
        for (text, code) in [
            ("The pairing code didn't match", "wrong_pin"),
            ("The pairing code didn\u{2019}t match", "wrong_pin"),
            ("Pairing failed: code did not match", "wrong_pin"),
            ("mdns: failed to bind 0.0.0.0:5353", "mdns_unavailable"),
            ("mDNS responder: address already in use", "mdns_unavailable"),
            ("mdns socket: EADDRINUSE", "mdns_unavailable"),
            ("mdns daemon unavailable", "mdns_unavailable"),
            ("bind: address already in use", "unknown"),
        ] {
            assert_eq!(classify_error_text(text), code, "{text}");
        }
    }

    #[test]
    fn device_details_name_empty_keys_not_values() {
        let props = DeviceProperties {
            friendly_name: Some("Living Room".into()),
            manufacturer: String::new(),
            model: " ".into(),
            brand: "unknown".into(),
            serial_number: "1324619053514".into(),
            sdk_level: "30".into(),
            android_release: "11".into(),
            device_codename: "mdarcy".into(),
            characteristics: "tv".into(),
            leanback: None,
            ..Default::default()
        };
        assert!(lacks_details(&props));
        let d = device_diag(
            Some(DeviceStatus::Device),
            ConnectionType::Network,
            Some(&props),
            Some("device 1324619053514 not found"),
        );
        assert_eq!(
            d.empty_props,
            vec![
                "ro.product.manufacturer",
                "ro.product.brand",
                "ro.product.model",
                "android.software.leanback"
            ]
        );
        assert_eq!(d.error.as_deref(), Some("not_connected"));
        let unauthorized = device_diag(
            Some(DeviceStatus::Unauthorized),
            ConnectionType::Usb,
            None,
            None,
        );
        assert_eq!(
            serde_json::to_value(&unauthorized).unwrap(),
            serde_json::json!({ "state": "unauthorized", "transport": "usb" })
        );
        assert_clean(&Diag {
            device: Some(d),
            ..Default::default()
        });
    }

    #[test]
    fn counts_and_durations_are_bounded() {
        let s = sweep_diag(
            usize::MAX,
            3,
            250,
            1,
            std::time::Duration::from_secs(86_400),
        );
        assert_eq!(s.probed, MAX_COUNT as u32);
        assert_eq!(s.ms, MAX_MS);
        let many: Vec<String> = (0..50).map(|i| format!("error {i} timed out")).collect();
        assert!(error_codes(many.iter().map(String::as_str)).len() <= MAX_CODES);
        assert_eq!(discovery_diag(u64::MAX, 0).timeout_ms, MAX_MS);
        let d = Diag::default().with_ms(std::time::Duration::from_millis(1234));
        assert_eq!(
            serde_json::to_value(&d).unwrap(),
            serde_json::json!({ "ms": 1234 })
        );
    }

    #[test]
    fn reasons_have_stable_codes() {
        assert_eq!(DiagReason::ScanEmpty.code(), "scan_empty");
        assert_eq!(DiagReason::ScanConnectFailed.code(), "scan_connect_failed");
        assert_eq!(DiagReason::DiscoverEmpty.code(), "discover_empty");
        assert_eq!(
            DiagReason::DeviceDetailsEmpty.code(),
            "device_details_empty"
        );
        assert_eq!(
            DiagReason::DeviceDetailsEmpty.action(),
            TelemetryAction::Connect
        );
    }
}

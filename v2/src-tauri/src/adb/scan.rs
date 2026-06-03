//! Network discovery — finds Android TV devices on the local /24 subnet by
//! probing port 5555 in parallel. Matches v1's `Scan-Network` behavior.
//!
//! Implementation note: v1 used ICMP ping + ARP lookup to find any host, then
//! attempted `adb connect`. We skip the ICMP detour and TCP-probe :5555
//! directly — it's the only port we care about, it's faster, and many TVs
//! firewall ICMP but must expose 5555 for adb-over-network to work at all.

use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use futures_util::stream::{FuturesUnordered, StreamExt};
use tokio::net::TcpStream;
use tokio::process::Command;
use tokio::time::timeout;

/// How long to wait per TCP connect before giving up.
const PROBE_TIMEOUT: Duration = Duration::from_millis(250);

/// How many simultaneous probes to keep in flight.
const PROBE_CONCURRENCY: usize = 64;

/// ADB-over-network listening port.
pub const ADB_NETWORK_PORT: u16 = 5555;

/// Determine the local /24 subnet by reading the default gateway. Returns the
/// first three octets (e.g. `[192, 168, 42]`).
///
/// Honors the `SHIELD_OPTIMIZER_SUBNET` env var (e.g. `"192.168.42"`) which
/// matches v1's `-Subnet` CLI flag. Falls back to OS-specific subprocess
/// parsing when unset.
pub async fn local_subnet_prefix() -> Option<[u8; 3]> {
    if let Ok(s) = std::env::var("SHIELD_OPTIMIZER_SUBNET") {
        if let Some(prefix) = parse_prefix(&s) {
            return Some(prefix);
        }
    }
    let gateway = detect_default_gateway().await?;
    let octets = match gateway {
        IpAddr::V4(v4) => v4.octets(),
        IpAddr::V6(_) => return None, // ADB-over-network is IPv4-only in practice.
    };
    Some([octets[0], octets[1], octets[2]])
}

fn parse_prefix(s: &str) -> Option<[u8; 3]> {
    let parts: Vec<&str> = s.trim().split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    let a = parts[0].parse::<u8>().ok()?;
    let b = parts[1].parse::<u8>().ok()?;
    let c = parts[2].parse::<u8>().ok()?;
    Some([a, b, c])
}

/// Read the system's default gateway address. Per-OS subprocess parsing.
async fn detect_default_gateway() -> Option<IpAddr> {
    if cfg!(target_os = "macos") {
        gateway_from_macos_route().await
    } else if cfg!(target_os = "linux") {
        // /proc/net/route is the fast path; `ip route show default` is a
        // subprocess fallback when /proc isn't readable (containerized envs).
        if let Some(g) = gateway_from_linux_proc() {
            Some(g)
        } else {
            gateway_from_ip_route().await
        }
    } else if cfg!(windows) {
        gateway_from_windows_route().await
    } else {
        None
    }
}

async fn gateway_from_macos_route() -> Option<IpAddr> {
    let out = Command::new("route")
        .args(["-n", "get", "default"])
        .output()
        .await
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    // Line looks like: "    gateway: 192.168.42.1"
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("gateway:") {
            if let Ok(addr) = rest.trim().parse::<IpAddr>() {
                return Some(addr);
            }
        }
    }
    None
}

fn gateway_from_linux_proc() -> Option<IpAddr> {
    // /proc/net/route is whitespace-separated columns:
    //   Iface  Destination  Gateway  Flags  RefCnt  Use  Metric  Mask  ...
    // Destination/Gateway/Mask are little-endian hex IPs. We want the row
    // whose Destination is `00000000` (default route).
    let contents = std::fs::read_to_string("/proc/net/route").ok()?;
    for line in contents.lines().skip(1) {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 3 {
            continue;
        }
        if cols[1] == "00000000" {
            if let Some(addr) = parse_le_hex_ip(cols[2]) {
                return Some(addr);
            }
        }
    }
    None
}

fn parse_le_hex_ip(hex: &str) -> Option<IpAddr> {
    if hex.len() != 8 {
        return None;
    }
    let n = u32::from_str_radix(hex, 16).ok()?;
    let bytes = n.to_le_bytes();
    Some(IpAddr::V4(std::net::Ipv4Addr::new(
        bytes[0], bytes[1], bytes[2], bytes[3],
    )))
}

async fn gateway_from_ip_route() -> Option<IpAddr> {
    let out = Command::new("ip")
        .args(["route", "show", "default"])
        .output()
        .await
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    // "default via 192.168.42.1 dev eth0 ..."
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        if parts.next() == Some("default") && parts.next() == Some("via") {
            if let Some(ip) = parts.next() {
                if let Ok(addr) = ip.parse::<IpAddr>() {
                    return Some(addr);
                }
            }
        }
    }
    None
}

async fn gateway_from_windows_route() -> Option<IpAddr> {
    let mut cmd = Command::new("route");
    cmd.args(["print", "-4", "0.0.0.0"]);
    super::hide_console_window(&mut cmd);
    let out = cmd.output().await.ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    // Row: "    0.0.0.0    0.0.0.0    192.168.42.1    192.168.42.50    25"
    for line in text.lines() {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() >= 3 && cols[0] == "0.0.0.0" && cols[1] == "0.0.0.0" {
            if let Ok(addr) = cols[2].parse::<IpAddr>() {
                return Some(addr);
            }
        }
    }
    None
}

/// Result of scanning the local subnet. Each entry is an IP that responded
/// on the ADB port.
#[derive(Debug, Clone)]
pub struct ScanHit {
    pub ip: String,
}

/// Probe all 254 addresses in the given /24 in parallel. Returns IPs that
/// answered on the ADB port. Honors a global concurrency cap so we don't
/// flood the user's network stack.
pub async fn scan_subnet(prefix: [u8; 3]) -> Vec<ScanHit> {
    // Priority sweep mirroring v1: common DHCP range first, then the rest.
    let ordered_hosts: Vec<u8> = (100..=150).chain(2..=99).chain(151..=254).collect();

    let mut in_flight: FuturesUnordered<_> = FuturesUnordered::new();
    let mut iter = ordered_hosts.into_iter();
    let mut hits = Vec::new();

    // Prime the pump.
    for _ in 0..PROBE_CONCURRENCY {
        if let Some(host) = iter.next() {
            in_flight.push(probe(prefix, host));
        }
    }

    while let Some(result) = in_flight.next().await {
        if let Some(hit) = result {
            hits.push(hit);
        }
        if let Some(host) = iter.next() {
            in_flight.push(probe(prefix, host));
        }
    }

    hits
}

async fn probe(prefix: [u8; 3], host: u8) -> Option<ScanHit> {
    let addr = SocketAddr::from((
        std::net::Ipv4Addr::new(prefix[0], prefix[1], prefix[2], host),
        ADB_NETWORK_PORT,
    ));
    match timeout(PROBE_TIMEOUT, TcpStream::connect(addr)).await {
        Ok(Ok(_)) => Some(ScanHit {
            ip: addr.ip().to_string(),
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn parses_dotted_prefix() {
        assert_eq!(parse_prefix("192.168.42").unwrap(), [192, 168, 42]);
        assert_eq!(parse_prefix(" 10.0.0 ").unwrap(), [10, 0, 0]);
    }

    #[test]
    fn rejects_malformed_prefix() {
        assert!(parse_prefix("192.168").is_none());
        assert!(parse_prefix("192.168.42.1").is_none());
        assert!(parse_prefix("abc.def.ghi").is_none());
    }

    #[test]
    fn parses_linux_le_hex_ip() {
        // 192.168.42.1 → little-endian hex bytes: 0xC0,0xA8,0x2A,0x01 → "0102A8C0" (reversed).
        // Actually /proc/net/route stores it as the bytes appear in memory,
        // which on LE is host-low-byte first: 192.168.42.1 → 01 2A A8 C0 → "012AA8C0"
        let parsed = parse_le_hex_ip("012AA8C0").unwrap();
        assert_eq!(parsed.to_string(), "192.168.42.1");
    }

    #[test]
    fn rejects_bad_hex() {
        assert!(parse_le_hex_ip("nothex").is_none());
        assert!(parse_le_hex_ip("ABC").is_none());
    }
}

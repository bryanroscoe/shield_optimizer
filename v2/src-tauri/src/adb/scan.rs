//! Network discovery — finds Android TV devices on the local /24 subnet by
//! probing port 5555 in parallel. Matches v1's `Scan-Network` behavior.
//!
//! Implementation note: v1 used ICMP ping + ARP lookup to find any host, then
//! attempted `adb connect`. We skip the ICMP detour and TCP-probe :5555
//! directly — it's the only port we care about, it's faster, and many TVs
//! firewall ICMP but must expose 5555 for adb-over-network to work at all.

use std::net::IpAddr;
#[cfg(not(feature = "e2e"))]
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use futures_util::{stream, stream::BoxStream, StreamExt};
#[cfg(not(feature = "e2e"))]
use tokio::net::TcpStream;
use tokio::process::Command;
#[cfg(not(feature = "e2e"))]
use tokio::time::timeout;

/// How long to wait per TCP connect before giving up.
#[cfg(not(feature = "e2e"))]
const PROBE_TIMEOUT: Duration = Duration::from_millis(250);

/// How many simultaneous probes to keep in flight.
#[cfg(not(feature = "e2e"))]
const PROBE_CONCURRENCY: usize = 64;

/// ADB-over-network listening port.
pub const ADB_NETWORK_PORT: u16 = 5555;

/// Why the gateway's /24 was not swept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubnetSkip {
    /// No IPv4 default gateway could be read.
    NotDetected,
    /// The default route leads somewhere other than a LAN on a local
    /// interface — typically a full-tunnel VPN. Sweeping it would probe
    /// port 5555 across someone else's network.
    NotLocalLan,
}

/// Determine the local /24 subnet by reading the default gateway. Returns the
/// first three octets (e.g. `[192, 168, 42]`).
///
/// Honors the `ATV_OPTIMIZER_SUBNET` env var (or its old name
/// `SHIELD_OPTIMIZER_SUBNET`, e.g. `"192.168.42"`) which
/// matches v1's `-Subnet` CLI flag. Falls back to OS-specific subprocess
/// parsing when unset.
pub async fn local_subnet_prefix() -> Option<[u8; 3]> {
    local_subnet().await.ok()
}

/// [`local_subnet_prefix`], saying why nothing will be swept. The explicit
/// override is used as given; a detected gateway only counts when it sits
/// inside the subnet of an up, non-tunnel local interface, whatever its
/// address range. When the interface list cannot be read, the gateway's /24
/// is swept as before.
pub async fn local_subnet() -> Result<[u8; 3], SubnetSkip> {
    if let Some(s) = crate::rename::subnet_override() {
        if let Some(prefix) = parse_prefix(&s) {
            return Ok(prefix);
        }
    }
    let gateway = match detect_default_gateway().await {
        Some(IpAddr::V4(v4)) => v4,
        // ADB-over-network is IPv4-only in practice.
        Some(IpAddr::V6(_)) | None => return Err(SubnetSkip::NotDetected),
    };
    let interfaces = local_interfaces();
    if !gateway_is_on_local_lan(gateway, interfaces.as_deref()) {
        tracing::info!(%gateway, "default gateway is not on a local interface; subnet sweep skipped");
        return Err(SubnetSkip::NotLocalLan);
    }
    let octets = gateway.octets();
    Ok([octets[0], octets[1], octets[2]])
}

#[derive(Debug, Clone)]
struct LocalInterface {
    name: String,
    ip: std::net::Ipv4Addr,
    prefixlen: u8,
    up: bool,
    point_to_point: bool,
}

/// `None` when the interface list cannot be read.
fn local_interfaces() -> Option<Vec<LocalInterface>> {
    let list = match if_addrs::get_if_addrs() {
        Ok(list) => list,
        Err(e) => {
            tracing::warn!(error = %e, "could not read network interfaces; sweeping the gateway's /24");
            return None;
        }
    };
    let v4 = list
        .iter()
        .filter_map(|iface| match &iface.addr {
            if_addrs::IfAddr::V4(v4) => Some(LocalInterface {
                name: iface.name.clone(),
                ip: v4.ip,
                prefixlen: v4.prefixlen,
                up: iface.is_oper_up(),
                point_to_point: iface.is_p2p(),
            }),
            if_addrs::IfAddr::V6(_) => None,
        })
        .collect();
    Some(v4)
}

fn is_tunnel_name(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    [
        "utun",
        "tun",
        "tap",
        "ppp",
        "wg",
        "tailscale",
        "ipsec",
        "zt",
        "gif",
        "stf",
    ]
    .iter()
    .any(|prefix| name.starts_with(prefix))
}

/// Unreadable interfaces (`None`) say nothing, so the sweep goes ahead.
fn gateway_is_on_local_lan(
    gateway: std::net::Ipv4Addr,
    interfaces: Option<&[LocalInterface]>,
) -> bool {
    let Some(interfaces) = interfaces else {
        return true;
    };
    interfaces.iter().any(|iface| {
        if !iface.up
            || iface.point_to_point
            || iface.ip.is_loopback()
            || is_tunnel_name(&iface.name)
        {
            return false;
        }
        let bits = u32::from(iface.prefixlen.min(32));
        let mask = if bits == 0 {
            0
        } else {
            u32::MAX << (32 - bits)
        };
        u32::from(iface.ip) & mask == u32::from(gateway) & mask
    })
}

/// `route` / `ip` with a short deadline, from a fixed working directory and
/// without a console window, like every other helper the app spawns.
async fn route_helper(program: &str, args: &[&str]) -> Option<std::process::Output> {
    let mut cmd = Command::new(program);
    cmd.args(args).kill_on_drop(true);
    super::hide_console_window(&mut cmd);
    super::pin_working_directory(&mut cmd);
    let out = tokio::time::timeout(Duration::from_secs(3), cmd.output())
        .await
        .ok()?
        .ok()?;
    out.status.success().then_some(out)
}

/// Did `ATV_OPTIMIZER_SUBNET` choose the subnet, rather than the gateway?
pub fn subnet_overridden() -> bool {
    crate::rename::subnet_override()
        .and_then(|s| parse_prefix(&s))
        .is_some()
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
    let out = route_helper("route", &["-n", "get", "default"]).await?;
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
    let out = route_helper("ip", &["route", "show", "default"]).await?;
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
    let out = route_helper("route", &["print", "-4", "0.0.0.0"]).await?;
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

/// The E2E harness answers the port sweep from its simulated network rather
/// than opening sockets on the host's LAN. Compiled only with `e2e`.
#[cfg(feature = "e2e")]
#[allow(clippy::type_complexity)]
pub static SWEEP_OVERRIDE: std::sync::RwLock<
    Option<Box<dyn Fn([u8; 3]) -> Vec<String> + Send + Sync>>,
> = std::sync::RwLock::new(None);

/// How a port sweep went, for the "scan found nothing" report.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SweepStats {
    pub probed: usize,
    pub open: usize,
    pub timeouts: usize,
    pub refused: usize,
    pub elapsed: Duration,
}

#[cfg_attr(feature = "e2e", allow(dead_code))]
enum Probe {
    Open(ScanHit),
    Refused,
    TimedOut,
    Other,
}

/// Collect the same streaming sweep used by native discovery, with diagnostics.
pub async fn scan_subnet_with_stats(prefix: [u8; 3]) -> (Vec<ScanHit>, SweepStats) {
    let mut stats = SweepStats::default();
    let hits = scan_subnet_hits(prefix, &mut stats).collect().await;
    (hits, stats)
}

pub fn scan_subnet_hits(prefix: [u8; 3], stats: &mut SweepStats) -> BoxStream<'_, ScanHit> {
    #[cfg(feature = "e2e")]
    let probes = {
        let ips = SWEEP_OVERRIDE
            .read()
            .ok()
            .and_then(|guard| guard.as_ref().map(|sweep| sweep(prefix)))
            .expect("E2E fixture sweep is missing; live network probing was refused");
        stream::iter(ips.into_iter().map(|ip| Probe::Open(ScanHit { ip })))
    };
    #[cfg(not(feature = "e2e"))]
    let probes = stream::iter((100..=150).chain(2..=99).chain(151..=254))
        .map(move |host| probe(prefix, host))
        .buffer_unordered(PROBE_CONCURRENCY);
    tracked_hits(probes, stats)
}

fn tracked_hits<'a>(
    probes: impl futures_util::Stream<Item = Probe> + Send + 'a,
    stats: &'a mut SweepStats,
) -> BoxStream<'a, ScanHit> {
    let started = Instant::now();
    probes
        .filter_map(move |probe| {
            stats.probed += 1;
            stats.elapsed = started.elapsed();
            let hit = match probe {
                Probe::Open(hit) => {
                    stats.open += 1;
                    Some(hit)
                }
                Probe::Refused => {
                    stats.refused += 1;
                    None
                }
                Probe::TimedOut => {
                    stats.timeouts += 1;
                    None
                }
                Probe::Other => None,
            };
            futures_util::future::ready(hit)
        })
        .boxed()
}

#[cfg(not(feature = "e2e"))]
async fn probe(prefix: [u8; 3], host: u8) -> Probe {
    let addr = SocketAddr::from((
        std::net::Ipv4Addr::new(prefix[0], prefix[1], prefix[2], host),
        ADB_NETWORK_PORT,
    ));
    match timeout(PROBE_TIMEOUT, TcpStream::connect(addr)).await {
        Ok(Ok(_)) => Probe::Open(ScanHit {
            ip: addr.ip().to_string(),
        }),
        Ok(Err(e)) if e.kind() == std::io::ErrorKind::ConnectionRefused => Probe::Refused,
        Ok(Err(_)) => Probe::Other,
        Err(_) => Probe::TimedOut,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn iface(name: &str, ip: [u8; 4], prefixlen: u8, point_to_point: bool) -> LocalInterface {
        LocalInterface {
            name: name.into(),
            ip: ip.into(),
            prefixlen,
            up: true,
            point_to_point,
        }
    }

    #[test]
    fn a_gateway_on_a_real_local_interface_is_swept() {
        let on =
            |gw: [u8; 4], list: &[LocalInterface]| gateway_is_on_local_lan(gw.into(), Some(list));
        let wifi = iface("en0", [192, 168, 42, 50], 24, false);
        let vpn = iface("utun4", [10, 8, 0, 6], 24, true);
        assert!(on([192, 168, 42, 1], &[wifi.clone(), vpn.clone()]));
        // Full-tunnel VPN: the default route goes through the tunnel.
        assert!(!on([10, 8, 0, 1], &[wifi.clone(), vpn]));
        // A tunnel without the point-to-point flag is still recognised by name.
        for name in ["utun3", "wg0", "tailscale0", "tun0", "ppp0", "ipsec0"] {
            assert!(
                !on([10, 8, 0, 1], &[iface(name, [10, 8, 0, 6], 24, false)]),
                "{name}"
            );
        }
        // The address range does not matter: a home LAN on carrier-grade NAT
        // space or public addresses is still a LAN.
        assert!(on(
            [100, 64, 1, 1],
            &[iface("en0", [100, 64, 1, 20], 24, false)]
        ));
        assert!(on(
            [203, 0, 113, 1],
            &[iface("eth0", [203, 0, 113, 9], 24, false)]
        ));
        // ...but Tailscale's 100.64/10 address on its tunnel is not.
        assert!(!on(
            [100, 64, 1, 1],
            &[iface("utun5", [100, 64, 1, 20], 10, true)]
        ));
        assert!(on(
            [169, 254, 1, 1],
            &[iface("en0", [169, 254, 1, 20], 16, false)]
        ));
        assert!(on(
            [10, 0, 0, 1],
            &[iface("eth0", [10, 0, 5, 9], 16, false)]
        ));
        // A gateway outside every local subnet is not swept.
        assert!(!on([10, 9, 9, 1], std::slice::from_ref(&wifi)));
        // A down interface does not count.
        let mut down = wifi;
        down.up = false;
        assert!(!on([192, 168, 42, 1], &[down]));
    }

    #[test]
    fn an_unreadable_interface_list_still_sweeps() {
        assert!(gateway_is_on_local_lan([192, 168, 42, 1].into(), None));
    }

    #[cfg(feature = "e2e")]
    #[tokio::test]
    #[should_panic(expected = "live network probing was refused")]
    async fn missing_e2e_provider_never_falls_through_to_network_probes() {
        assert!(SWEEP_OVERRIDE.read().unwrap().is_none());
        scan_subnet_with_stats([192, 0, 2]).await;
    }

    #[tokio::test]
    async fn sweep_statistics_do_not_buffer_early_responders() {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let probes = stream::iter([
            Probe::Refused,
            Probe::Open(ScanHit {
                ip: "192.0.2.1".into(),
            }),
        ])
        .chain(stream::once(async move {
            rx.await.unwrap();
            Probe::TimedOut
        }))
        .chain(stream::iter([Probe::Other]));
        let mut stats = SweepStats::default();
        let mut hits = tracked_hits(probes, &mut stats);
        let first = tokio::time::timeout(Duration::from_secs(1), hits.next())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(first.ip, "192.0.2.1");
        tx.send(()).unwrap();
        assert!(hits.next().await.is_none());
        drop(hits);
        assert_eq!(
            (stats.probed, stats.open, stats.refused, stats.timeouts),
            (4, 1, 1, 1)
        );
    }

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
        // /proc/net/route stores the address as the bytes appear in memory,
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

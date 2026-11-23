//! Wake-on-LAN for saved devices (#411): the magic packet and where it goes.
//!
//! Pure: no sockets here. The desktop's `wake_device` command sends what this
//! builds. Waking is best effort; many TVs ignore it while asleep on Wi-Fi.

use std::net::{Ipv4Addr, SocketAddrV4};

/// 6 bytes of 0xFF, then the MAC 16 times.
pub const MAGIC_PACKET_LEN: usize = 6 + 16 * 6;

/// The discard port is the usual one; echo (7) is the other common choice.
pub const WAKE_PORTS: [u16; 2] = [9, 7];

/// A MAC address a wake packet can be sent for: six octets separated by `:`
/// or `-`, and a unicast address that isn't all zeros. Anything else is `None`.
pub fn parse_mac(text: &str) -> Option<[u8; 6]> {
    let text = text.trim();
    let sep = if text.contains('-') { '-' } else { ':' };
    let mut mac = [0u8; 6];
    let mut parts = text.split(sep);
    for octet in &mut mac {
        let part = parts.next()?;
        if part.len() != 2 || !part.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        *octet = u8::from_str_radix(part, 16).ok()?;
    }
    if parts.next().is_some() {
        return None;
    }
    let all_zero = mac == [0; 6];
    // The multicast bit also covers broadcast (ff:ff:ff:ff:ff:ff).
    let multicast = mac[0] & 0x01 != 0;
    (!all_zero && !multicast).then_some(mac)
}

pub fn magic_packet(mac: [u8; 6]) -> [u8; MAGIC_PACKET_LEN] {
    let mut packet = [0xFF; MAGIC_PACKET_LEN];
    for chunk in packet[6..].chunks_exact_mut(6) {
        chunk.copy_from_slice(&mac);
    }
    packet
}

/// The subnet's broadcast address. `None` for /0 (that is the limited
/// broadcast) and for /31 and /32, which have none.
pub fn directed_broadcast(ip: Ipv4Addr, prefixlen: u8) -> Option<Ipv4Addr> {
    if prefixlen == 0 || prefixlen > 30 {
        return None;
    }
    let host_bits = u32::MAX >> prefixlen;
    Some(Ipv4Addr::from(u32::from(ip) | host_bits))
}

fn same_subnet(a: Ipv4Addr, b: Ipv4Addr, prefixlen: u8) -> bool {
    let mask = u32::MAX.checked_shl(32 - u32::from(prefixlen)).unwrap_or(0);
    u32::from(a) & mask == u32::from(b) & mask
}

/// Where to send the packet: the limited broadcast, plus the directed
/// broadcast of each local interface (`ip`, prefix length) whose subnet holds
/// the saved `host`, on each of [`WAKE_PORTS`].
pub fn wake_targets(host: Option<Ipv4Addr>, interfaces: &[(Ipv4Addr, u8)]) -> Vec<SocketAddrV4> {
    let mut addresses = vec![Ipv4Addr::BROADCAST];
    if let Some(host) = host.filter(|h| !h.is_loopback() && !h.is_unspecified()) {
        for &(ip, prefixlen) in interfaces {
            if ip.is_loopback()
                || prefixlen == 0
                || prefixlen > 30
                || !same_subnet(ip, host, prefixlen)
            {
                continue;
            }
            if let Some(b) = directed_broadcast(ip, prefixlen) {
                if !addresses.contains(&b) {
                    addresses.push(b);
                }
            }
        }
    }
    WAKE_PORTS
        .iter()
        .flat_map(|&port| addresses.iter().map(move |&a| SocketAddrV4::new(a, port)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAC: [u8; 6] = [0x00, 0x04, 0x4b, 0x8c, 0x21, 0x5e];

    #[test]
    fn magic_packet_is_six_ff_then_the_mac_sixteen_times() {
        let packet = magic_packet(MAC);
        assert_eq!(packet.len(), 102);
        assert_eq!(&packet[..6], &[0xFF; 6]);
        for i in 0..16 {
            assert_eq!(&packet[6 + i * 6..12 + i * 6], &MAC, "repetition {i}");
        }
    }

    #[test]
    fn parses_colon_and_dash_macs_in_any_case() {
        assert_eq!(parse_mac("00:04:4b:8c:21:5e"), Some(MAC));
        assert_eq!(parse_mac(" 00-04-4B-8C-21-5E "), Some(MAC));
        assert_eq!(
            parse_mac("A4:5E:60:1D:42:0A"),
            Some([0xa4, 0x5e, 0x60, 0x1d, 0x42, 0x0a])
        );
    }

    #[test]
    fn rejects_malformed_macs() {
        for bad in [
            "",
            "00:04:4b:8c:21",
            "00:04:4b:8c:21:5e:77",
            "00:04:4b:8c:21:5g",
            "0:04:4b:8c:21:5e",
            "000:4:4b:8c:21:5e",
            "00:04-4b:8c:21:5e",
            "00044b8c215e",
            "+0:04:4b:8c:21:5e",
        ] {
            assert_eq!(parse_mac(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn rejects_zero_broadcast_and_multicast_macs() {
        assert_eq!(parse_mac("00:00:00:00:00:00"), None);
        assert_eq!(parse_mac("ff:ff:ff:ff:ff:ff"), None);
        assert_eq!(parse_mac("01:00:5e:00:00:fb"), None, "IPv4 multicast");
        assert_eq!(parse_mac("33:33:00:00:00:01"), None, "IPv6 multicast");
        // Locally administered (randomized Wi-Fi) addresses are unicast and fine.
        assert!(parse_mac("02:11:22:33:44:55").is_some());
    }

    #[test]
    fn directed_broadcast_per_prefix() {
        let ip = Ipv4Addr::new(192, 168, 1, 42);
        assert_eq!(
            directed_broadcast(ip, 24),
            Some(Ipv4Addr::new(192, 168, 1, 255))
        );
        assert_eq!(
            directed_broadcast(ip, 16),
            Some(Ipv4Addr::new(192, 168, 255, 255))
        );
        assert_eq!(
            directed_broadcast(ip, 22),
            Some(Ipv4Addr::new(192, 168, 3, 255))
        );
        assert_eq!(
            directed_broadcast(Ipv4Addr::new(10, 0, 7, 9), 8),
            Some(Ipv4Addr::new(10, 255, 255, 255))
        );
        assert_eq!(
            directed_broadcast(ip, 30),
            Some(Ipv4Addr::new(192, 168, 1, 43))
        );
        assert_eq!(directed_broadcast(ip, 31), None);
        assert_eq!(directed_broadcast(ip, 32), None);
        assert_eq!(directed_broadcast(ip, 0), None);
    }

    #[test]
    fn targets_add_the_subnet_holding_the_host() {
        let interfaces = [
            (Ipv4Addr::new(127, 0, 0, 1), 8),
            (Ipv4Addr::new(192, 168, 1, 10), 24),
            (Ipv4Addr::new(10, 8, 0, 2), 24),
            // A second address on the same subnet adds nothing new.
            (Ipv4Addr::new(192, 168, 1, 11), 24),
        ];
        let targets = wake_targets(Some(Ipv4Addr::new(192, 168, 1, 64)), &interfaces);
        assert_eq!(
            targets,
            vec![
                SocketAddrV4::new(Ipv4Addr::BROADCAST, 9),
                SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 255), 9),
                SocketAddrV4::new(Ipv4Addr::BROADCAST, 7),
                SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 255), 7),
            ]
        );
    }

    #[test]
    fn targets_fall_back_to_the_limited_broadcast() {
        let limited = vec![
            SocketAddrV4::new(Ipv4Addr::BROADCAST, 9),
            SocketAddrV4::new(Ipv4Addr::BROADCAST, 7),
        ];
        let lan = [(Ipv4Addr::new(192, 168, 1, 10), 24)];
        assert_eq!(wake_targets(None, &lan), limited, "no host");
        assert_eq!(
            wake_targets(Some(Ipv4Addr::new(192, 168, 2, 64)), &lan),
            limited,
            "host on another subnet"
        );
        assert_eq!(
            wake_targets(Some(Ipv4Addr::new(192, 168, 1, 64)), &[]),
            limited,
            "no interfaces"
        );
        assert_eq!(
            wake_targets(Some(Ipv4Addr::LOCALHOST), &[(Ipv4Addr::LOCALHOST, 8)]),
            limited,
            "loopback"
        );
        assert_eq!(
            wake_targets(
                Some(Ipv4Addr::new(192, 168, 1, 11)),
                &[(Ipv4Addr::new(192, 168, 1, 10), 31)]
            ),
            limited,
            "a /31 has no broadcast"
        );
    }

    #[test]
    fn same_subnet_respects_the_prefix() {
        let a = Ipv4Addr::new(192, 168, 1, 10);
        assert!(same_subnet(a, Ipv4Addr::new(192, 168, 3, 1), 22));
        assert!(!same_subnet(a, Ipv4Addr::new(192, 168, 4, 1), 22));
        assert!(same_subnet(a, Ipv4Addr::new(192, 168, 1, 200), 24));
        assert!(!same_subnet(a, Ipv4Addr::new(192, 168, 1, 200), 25));
    }
}

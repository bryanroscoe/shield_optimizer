//! `wake_device`: best-effort Wake-on-LAN for a saved device (#411).
//!
//! Sent only when the user chooses Connect on a saved row, never by a launch
//! or restart redial. The packet and the target list come from
//! `atv_optimizer_core::wol`; this is just the UDP send. Not ADB, so it
//! doesn't go through the ADB driver.

use std::io;
use std::net::{Ipv4Addr, SocketAddrV4, UdpSocket};
use std::time::Duration;

use atv_optimizer_core::wol;
use serde::Serialize;

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct WakeResult {
    /// Packets that went out. Zero when the MAC isn't usable or nothing
    /// could be sent; neither is an error, and neither says the TV woke.
    pub sent: u32,
}

const SEND_TIMEOUT: Duration = Duration::from_millis(500);

/// `wake_device` — `mac` is the saved record's MAC, `host` the IPv4 host of
/// its saved address (it picks the local subnet's directed broadcast).
#[tauri::command]
pub async fn wake_device(mac: String, host: Option<String>) -> Result<WakeResult, String> {
    let Some(mac) = wol::parse_mac(&mac) else {
        tracing::debug!("wake: no usable MAC; nothing sent");
        return Ok(WakeResult { sent: 0 });
    };
    let host = host
        .as_deref()
        .and_then(|h| h.trim().parse::<Ipv4Addr>().ok());
    let sent = tokio::task::spawn_blocking(move || {
        let targets = wol::wake_targets(host, &lan_interfaces());
        let socket = match open_socket(Ipv4Addr::UNSPECIFIED) {
            Ok(s) => s,
            Err(e) => {
                tracing::debug!(error = %e, "wake: could not open a UDP socket");
                return 0;
            }
        };
        send_all(&wol::magic_packet(mac), &targets, |buf, to| {
            socket.send_to(buf, to)
        })
    })
    .await
    .unwrap_or(0);
    Ok(WakeResult { sent })
}

/// Up, non-loopback, non-point-to-point IPv4 interfaces, as (address, prefix).
fn lan_interfaces() -> Vec<(Ipv4Addr, u8)> {
    match if_addrs::get_if_addrs() {
        Ok(list) => list
            .iter()
            .filter(|iface| iface.is_oper_up() && !iface.is_loopback() && !iface.is_p2p())
            .filter_map(|iface| match &iface.addr {
                if_addrs::IfAddr::V4(v4) => Some((v4.ip, v4.prefixlen)),
                if_addrs::IfAddr::V6(_) => None,
            })
            .collect(),
        Err(e) => {
            tracing::debug!(error = %e, "wake: could not read network interfaces; limited broadcast only");
            Vec::new()
        }
    }
}

fn open_socket(bind: Ipv4Addr) -> io::Result<UdpSocket> {
    let socket = UdpSocket::bind((bind, 0))?;
    socket.set_broadcast(true)?;
    socket.set_write_timeout(Some(SEND_TIMEOUT))?;
    Ok(socket)
}

/// Sends `packet` to every target, carrying on past failures. Returns how
/// many went out.
fn send_all(
    packet: &[u8],
    targets: &[SocketAddrV4],
    mut send: impl FnMut(&[u8], SocketAddrV4) -> io::Result<usize>,
) -> u32 {
    let mut sent = 0;
    for &to in targets {
        match send(packet, to) {
            Ok(n) if n == packet.len() => sent += 1,
            Ok(n) => tracing::debug!(%to, n, "wake: short send"),
            Err(e) => tracing::debug!(%to, error = %e, "wake: send failed"),
        }
    }
    sent
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAC: &str = "00:04:4b:8c:21:5e";

    #[test]
    fn send_all_tries_every_target_past_failures() {
        let targets = wol::wake_targets(
            Some(Ipv4Addr::new(192, 168, 1, 64)),
            &[(Ipv4Addr::new(192, 168, 1, 10), 24)],
        );
        let packet = wol::magic_packet(wol::parse_mac(MAC).unwrap());
        let mut tried = Vec::new();
        let sent = send_all(&packet, &targets, |buf, to| {
            assert_eq!(buf.len(), 102);
            tried.push(to);
            if to.ip() == &Ipv4Addr::BROADCAST {
                Err(io::Error::new(io::ErrorKind::PermissionDenied, "blocked"))
            } else {
                Ok(buf.len())
            }
        });
        assert_eq!(tried, targets, "every target, in order");
        assert_eq!(sent, 2, "the directed broadcast on ports 9 and 7");
        assert_eq!(
            send_all(&packet, &targets, |_, _| Ok(10)),
            0,
            "a short send isn't counted"
        );
    }

    /// The real socket path, over loopback only: no broadcast leaves the
    /// machine (and nothing raises a Local Network prompt).
    #[test]
    fn sends_the_magic_packet_over_a_real_socket() {
        let receiver = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        receiver
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let to = match receiver.local_addr().unwrap() {
            std::net::SocketAddr::V4(a) => a,
            other => panic!("unexpected {other}"),
        };
        let socket = open_socket(Ipv4Addr::LOCALHOST).unwrap();
        assert!(socket.broadcast().unwrap(), "broadcast is enabled");
        let packet = wol::magic_packet(wol::parse_mac(MAC).unwrap());
        assert_eq!(
            send_all(&packet, &[to], |buf, to| socket.send_to(buf, to)),
            1
        );
        let mut buf = [0u8; 256];
        let (n, _) = receiver.recv_from(&mut buf).unwrap();
        assert_eq!(&buf[..n], &packet[..]);
    }

    #[tokio::test]
    async fn an_unusable_mac_sends_nothing_and_is_not_an_error() {
        for mac in [
            "",
            "nope",
            "00:00:00:00:00:00",
            "ff:ff:ff:ff:ff:ff",
            "01:00:5e:00:00:fb",
        ] {
            let r = wake_device(mac.into(), Some("192.168.1.64".into())).await;
            assert_eq!(r, Ok(WakeResult { sent: 0 }), "{mac:?}");
        }
    }
}

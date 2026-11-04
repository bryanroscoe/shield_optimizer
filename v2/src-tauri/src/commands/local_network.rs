//! macOS Local Network privacy (macOS 15 and later).
//!
//! An app without the Local Network privilege has its traffic to the LAN
//! blocked, and the adb server this app starts is held to the app's decision:
//! macOS attributes a helper's local network operations to the app that
//! spawned it (Apple TN3179, "macOS considerations"). adb run from Terminal is
//! exempt, which is why it can work there while the app's own adb fails.
//!
//! `local_network_access` does one local network operation from the app's own
//! process, so the system alert appears at launch with our usage string
//! instead of from adb in the middle of a scan, and reports what it saw. There
//! is no API that reads the privilege (TN3179, "Check for local network
//! access"), so the answer is evidence, not a reading: `Blocked` is claimed
//! only when every attempt failed with "No route to host" on a network that is
//! actually up. Anything less certain is `Unknown`, which claims nothing.

use std::net::Ipv4Addr;

use serde::Serialize;

#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LocalNetworkStatus {
    /// A multicast datagram left on a LAN interface.
    Allowed,
    /// Every attempt failed with EHOSTUNREACH while a LAN interface was up.
    Blocked,
    /// No evidence either way.
    Unknown,
    /// No broadcast-capable IPv4 interface is up, so there is nothing to test.
    NoNetwork,
    /// Not macOS; there is no such permission to ask about.
    NotApplicable,
}

#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct LocalNetworkAccess {
    pub status: LocalNetworkStatus,
}

/// What one attempt on one interface came back with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeOutcome {
    Sent,
    HostUnreachable,
    OtherError,
}

/// `EHOSTUNREACH` in Darwin's `<sys/errno.h>`. The probe only runs on macOS,
/// so this is the only value it can mean; written out so the classification
/// is testable on every platform CI runs on.
const DARWIN_EHOSTUNREACH: i32 = 65;

pub fn outcome_from_os_error(code: Option<i32>) -> ProbeOutcome {
    match code {
        Some(DARWIN_EHOSTUNREACH) => ProbeOutcome::HostUnreachable,
        _ => ProbeOutcome::OtherError,
    }
}

/// Whether an interface address is on a network the privilege governs: an
/// up, broadcast-capable IPv4 interface (TN3179's definition of a local
/// network, which excludes VPN tunnels), not loopback, not 169.254/16.
pub fn is_lan_ipv4(ip: Ipv4Addr, broadcast_capable: bool, running: bool, point_to_point: bool) -> bool {
    broadcast_capable
        && running
        && !point_to_point
        && !ip.is_loopback()
        && !ip.is_link_local()
        && !ip.is_unspecified()
}

pub fn classify(lan_interfaces: usize, outcomes: &[ProbeOutcome]) -> LocalNetworkStatus {
    if lan_interfaces == 0 {
        return LocalNetworkStatus::NoNetwork;
    }
    if outcomes.contains(&ProbeOutcome::Sent) {
        return LocalNetworkStatus::Allowed;
    }
    if !outcomes.is_empty() && outcomes.iter().all(|o| *o == ProbeOutcome::HostUnreachable) {
        return LocalNetworkStatus::Blocked;
    }
    LocalNetworkStatus::Unknown
}

/// A one-question mDNS query (RFC 6762) for `name`'s PTR records: the same
/// kind of packet macOS sends on its own all day, and one adb itself sends
/// when it browses for TVs.
pub fn mdns_ptr_query(name: &str) -> Vec<u8> {
    // id 0, flags 0 (standard query), 1 question, no answer/authority/extra.
    let mut packet = vec![0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0];
    for label in name.trim_end_matches('.').split('.') {
        packet.push(label.len() as u8);
        packet.extend_from_slice(label.as_bytes());
    }
    packet.push(0);
    packet.extend_from_slice(&[0, 12, 0, 1]); // PTR, IN
    packet
}

pub const MDNS_PROBE_NAME: &str = "_adb-tls-connect._tcp.local";

/// The Privacy & Security → Local Network pane. Fixed: nothing from the
/// frontend reaches this command line.
pub const LOCAL_NETWORK_SETTINGS_URL: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_LocalNetwork";

pub fn settings_command() -> (&'static str, [&'static str; 1]) {
    ("/usr/bin/open", [LOCAL_NETWORK_SETTINGS_URL])
}

#[cfg(target_os = "macos")]
fn probe() -> LocalNetworkStatus {
    use std::net::{IpAddr, SocketAddr, UdpSocket};

    let interfaces = match if_addrs::get_if_addrs() {
        Ok(list) => list,
        Err(e) => {
            tracing::debug!(error = %e, "local network probe: could not list interfaces");
            return LocalNetworkStatus::Unknown;
        }
    };
    let mut lan: Vec<Ipv4Addr> = Vec::new();
    for iface in &interfaces {
        if let if_addrs::IfAddr::V4(v4) = &iface.addr {
            if is_lan_ipv4(v4.ip, v4.broadcast.is_some(), iface.is_oper_up(), iface.is_p2p())
                && !lan.contains(&v4.ip)
            {
                lan.push(v4.ip);
            }
        }
    }
    lan.truncate(4);

    let query = mdns_ptr_query(MDNS_PROBE_NAME);
    let group = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(224, 0, 0, 251)), 5353);
    let attempt = |source: Ipv4Addr| -> ProbeOutcome {
        let fail = |e: std::io::Error| outcome_from_os_error(e.raw_os_error());
        let socket = match UdpSocket::bind((source, 0)) {
            Ok(s) => s,
            Err(e) => return fail(e),
        };
        // Out of this interface, so a Mac with no default route still has a
        // route for the group and the only reason left to refuse is policy.
        if let Err(e) = socket2::SockRef::from(&socket).set_multicast_if_v4(&source) {
            return fail(e);
        }
        let _ = socket.set_multicast_ttl_v4(255);
        // Connecting is the operation TN3179 recommends for raising the alert;
        // the send is what shows whether traffic is let through.
        if let Err(e) = socket.connect(group) {
            return fail(e);
        }
        match socket.send(&query) {
            Ok(_) => ProbeOutcome::Sent,
            Err(e) => fail(e),
        }
    };
    let outcomes: Vec<ProbeOutcome> = lan.iter().map(|ip| attempt(*ip)).collect();
    let status = classify(lan.len(), &outcomes);
    tracing::info!(?status, interfaces = lan.len(), ?outcomes, "local network probe");
    status
}

/// Trigger the macOS Local Network alert from this process and report what
/// the attempt saw. Off macOS it does nothing.
#[tauri::command]
pub async fn local_network_access() -> Result<LocalNetworkAccess, String> {
    #[cfg(target_os = "macos")]
    {
        let status = tokio::task::spawn_blocking(probe)
            .await
            .map_err(|e| e.to_string())?;
        Ok(LocalNetworkAccess { status })
    }
    #[cfg(not(target_os = "macos"))]
    {
        Ok(LocalNetworkAccess {
            status: LocalNetworkStatus::NotApplicable,
        })
    }
}

/// Open System Settings at Privacy & Security → Local Network.
#[tauri::command]
pub async fn open_local_network_settings() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let (program, args) = settings_command();
        let status = tokio::process::Command::new(program)
            .args(args)
            .status()
            .await
            .map_err(|e| e.to_string())?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("Could not open System Settings ({status})."))
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err("Local Network settings exist only on macOS.".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_ehostunreach_counts_as_unreachable() {
        assert_eq!(outcome_from_os_error(Some(65)), ProbeOutcome::HostUnreachable);
        // ENETUNREACH, EPERM, EADDRNOTAVAIL, and no errno at all.
        for code in [Some(51), Some(1), Some(49), None] {
            assert_eq!(outcome_from_os_error(code), ProbeOutcome::OtherError, "{code:?}");
        }
    }

    #[test]
    fn blocked_needs_a_network_and_every_attempt_unreachable() {
        use ProbeOutcome::*;
        assert_eq!(classify(1, &[HostUnreachable]), LocalNetworkStatus::Blocked);
        assert_eq!(classify(2, &[HostUnreachable, HostUnreachable]), LocalNetworkStatus::Blocked);
        // No LAN up: "No route to host" is just the truth, not a permission.
        assert_eq!(classify(0, &[]), LocalNetworkStatus::NoNetwork);
        assert_eq!(classify(0, &[HostUnreachable]), LocalNetworkStatus::NoNetwork);
        // Mixed or other failures are not evidence of a block.
        assert_eq!(classify(2, &[HostUnreachable, OtherError]), LocalNetworkStatus::Unknown);
        assert_eq!(classify(1, &[OtherError]), LocalNetworkStatus::Unknown);
        assert_eq!(classify(1, &[]), LocalNetworkStatus::Unknown);
    }

    #[test]
    fn any_datagram_sent_means_allowed() {
        use ProbeOutcome::*;
        assert_eq!(classify(1, &[Sent]), LocalNetworkStatus::Allowed);
        assert_eq!(classify(2, &[HostUnreachable, Sent]), LocalNetworkStatus::Allowed);
    }

    #[test]
    fn lan_interfaces_exclude_loopback_link_local_and_tunnels() {
        let home = Ipv4Addr::new(192, 168, 1, 20);
        assert!(is_lan_ipv4(home, true, true, false));
        assert!(!is_lan_ipv4(Ipv4Addr::LOCALHOST, true, true, false));
        assert!(!is_lan_ipv4(Ipv4Addr::new(169, 254, 3, 4), true, true, false));
        assert!(!is_lan_ipv4(Ipv4Addr::UNSPECIFIED, true, true, false));
        assert!(!is_lan_ipv4(home, false, true, false), "not broadcast-capable");
        assert!(!is_lan_ipv4(home, true, false, false), "not running");
        assert!(!is_lan_ipv4(Ipv4Addr::new(10, 8, 0, 2), true, true, true), "VPN tunnel");
    }

    #[test]
    fn mdns_query_is_one_well_formed_ptr_question() {
        let q = mdns_ptr_query(MDNS_PROBE_NAME);
        assert_eq!(&q[..12], &[0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0]);
        let mut expected = Vec::new();
        for label in ["_adb-tls-connect", "_tcp", "local"] {
            expected.push(label.len() as u8);
            expected.extend_from_slice(label.as_bytes());
        }
        expected.extend_from_slice(&[0, 0, 12, 0, 1]);
        assert_eq!(&q[12..], expected.as_slice());
        assert_eq!(mdns_ptr_query("_adb-tls-connect._tcp.local."), q);
    }

    #[test]
    fn settings_deep_link_is_fixed() {
        assert_eq!(
            settings_command(),
            (
                "/usr/bin/open",
                ["x-apple.systempreferences:com.apple.preference.security?Privacy_LocalNetwork"]
            )
        );
    }

    #[test]
    fn status_serializes_snake_case() {
        let json = serde_json::to_string(&LocalNetworkAccess {
            status: LocalNetworkStatus::NoNetwork,
        })
        .unwrap();
        assert_eq!(json, r#"{"status":"no_network"}"#);
        assert_eq!(
            serde_json::to_string(&LocalNetworkStatus::NotApplicable).unwrap(),
            r#""not_applicable""#
        );
    }

    #[tokio::test]
    #[cfg(not(target_os = "macos"))]
    async fn off_macos_nothing_is_probed() {
        assert_eq!(
            local_network_access().await.unwrap().status,
            LocalNetworkStatus::NotApplicable
        );
        assert!(open_local_network_settings().await.is_err());
    }
}

//! macOS Local Network privacy (macOS 15 and later).
//!
//! An app without the Local Network privilege has its traffic to the LAN
//! blocked, and the adb server this app starts is held to the app's decision:
//! macOS attributes a helper's local network operations to the app that
//! spawned it (Apple TN3179, "macOS considerations"). adb run from Terminal is
//! exempt, which is why it can work there while the app's own adb fails.
//!
//! What it is not: after an update, an adb server the earlier version left
//! running can be cut off by macOS for good (EHOSTUNREACH on every TV) while
//! the new app, and any server it starts, reach the LAN fine. And a new
//! binary's very first LAN connect can fail with EHOSTUNREACH and work about
//! a second later even when access is allowed. Both were reproduced on macOS
//! 26.6.2 (2026-11-04). So "No route to host" from adb alone proves nothing,
//! and one failed attempt here proves nothing either.
//!
//! `local_network_access` does local network operations from the app's own
//! process, so the system alert appears at launch with our usage string
//! instead of from adb in the middle of a scan, and reports what it saw. There
//! is no API that reads the privilege (TN3179, "Check for local network
//! access"), so the answer is evidence, not a reading: `Blocked` is claimed
//! only when every attempt, [`ATTEMPTS`] of them about a second apart, failed
//! with "No route to host" on a network that is actually up. Anything less
//! certain is `Unknown`, which claims nothing.

use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;

use serde::Serialize;

use crate::adb::server_repair::{self, Reach};

#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LocalNetworkStatus {
    /// Traffic from this process reached the LAN.
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

/// How many times a check tries before believing "No route to host", and how
/// long it waits between tries: the same rule as the adb repair's
/// [`server_repair::app_reaches`].
pub const ATTEMPTS: u32 = server_repair::PROBE_ATTEMPTS;
pub const RETRY_DELAY: Duration = server_repair::PROBE_RETRY_DELAY;

pub fn outcome_from_os_error(code: Option<i32>) -> ProbeOutcome {
    if code.is_some() && code == server_repair::HOST_UNREACHABLE {
        ProbeOutcome::HostUnreachable
    } else {
        ProbeOutcome::OtherError
    }
}

/// One round of the multicast probe (one datagram per LAN interface).
pub fn reach_from_round(outcomes: &[ProbeOutcome]) -> Reach {
    if outcomes.contains(&ProbeOutcome::Sent) {
        Reach::Reachable
    } else if !outcomes.is_empty() && outcomes.iter().all(|o| *o == ProbeOutcome::HostUnreachable) {
        Reach::Unreachable
    } else {
        Reach::Unknown
    }
}

/// The multicast check's retry rule, the same one
/// [`server_repair::app_reaches`] applies to a TCP connect: an attempt that got
/// through or failed some other way decides at once; "No route to host" is
/// believed only when all `attempts` say it, `pause` apart. macOS can refuse a
/// new process's first LAN connect and allow the next.
pub fn retry_until_decided(
    attempts: u32,
    mut attempt: impl FnMut() -> Reach,
    mut pause: impl FnMut(),
) -> Reach {
    for i in 0..attempts.max(1) {
        if i > 0 {
            pause();
        }
        let reach = attempt();
        if reach != Reach::Unreachable {
            return reach;
        }
    }
    Reach::Unreachable
}

/// Whether an interface address is on a network the privilege governs: an
/// up, broadcast-capable IPv4 interface (TN3179's definition of a local
/// network, which excludes VPN tunnels), not loopback, not 169.254/16.
pub fn is_lan_ipv4(
    ip: Ipv4Addr,
    broadcast_capable: bool,
    running: bool,
    point_to_point: bool,
) -> bool {
    broadcast_capable
        && running
        && !point_to_point
        && !ip.is_loopback()
        && !ip.is_link_local()
        && !ip.is_unspecified()
}

/// The status from the multicast check and, when the caller named one, a
/// TCP check of a TV ([`server_repair::app_reaches`], from this process). The
/// TV can only confirm access: a TV that can't be
/// reached proves nothing about permission on its own, since it may be off.
pub fn classify(
    lan_interfaces: usize,
    multicast: Reach,
    target: Option<Reach>,
) -> LocalNetworkStatus {
    if multicast == Reach::Reachable || target == Some(Reach::Reachable) {
        return LocalNetworkStatus::Allowed;
    }
    if lan_interfaces == 0 {
        return LocalNetworkStatus::NoNetwork;
    }
    if multicast == Reach::Unreachable && target != Some(Reach::Unknown) {
        return LocalNetworkStatus::Blocked;
    }
    LocalNetworkStatus::Unknown
}

/// `host` or `host:port` as a socket address, port 5555 when absent. IP
/// literals only: nothing is resolved.
pub fn parse_target(target: &str) -> Option<SocketAddr> {
    let t = target.trim();
    if let Ok(addr) = t.parse::<SocketAddr>() {
        return Some(addr);
    }
    t.trim_start_matches('[')
        .trim_end_matches(']')
        .parse::<std::net::IpAddr>()
        .ok()
        .map(|ip| SocketAddr::new(ip, 5555))
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

/// System Settings → Privacy & Security, at its Local Network anchor where
/// macOS defines one. macOS 26.6 has no anchor for Local Network (its
/// PrivacySecurity.searchTerms lists none) and opens Privacy & Security itself,
/// one click away, which is why the button is labelled for that. Fixed:
/// nothing from the frontend reaches this command line.
pub const LOCAL_NETWORK_SETTINGS_URL: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_LocalNetwork";

pub fn settings_command() -> (&'static str, [&'static str; 1]) {
    ("/usr/bin/open", [LOCAL_NETWORK_SETTINGS_URL])
}

/// The LAN interfaces and the multicast check across them.
#[cfg(target_os = "macos")]
fn multicast_probe() -> (usize, Reach) {
    use std::net::{IpAddr, UdpSocket};

    let interfaces = match if_addrs::get_if_addrs() {
        Ok(list) => list,
        Err(e) => {
            tracing::debug!(error = %e, "local network probe: could not list interfaces");
            return (0, Reach::Unknown);
        }
    };
    let mut lan: Vec<Ipv4Addr> = Vec::new();
    for iface in &interfaces {
        if let if_addrs::IfAddr::V4(v4) = &iface.addr {
            if is_lan_ipv4(
                v4.ip,
                v4.broadcast.is_some(),
                iface.is_oper_up(),
                iface.is_p2p(),
            ) && !lan.contains(&v4.ip)
            {
                lan.push(v4.ip);
            }
        }
    }
    lan.truncate(4);
    if lan.is_empty() {
        return (0, Reach::Unknown);
    }

    let query = mdns_ptr_query(MDNS_PROBE_NAME);
    let group = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(224, 0, 0, 251)), 5353);
    let send_from = |source: Ipv4Addr| -> ProbeOutcome {
        // The errno is what tells a block from anything else; keep it in the log.
        let fail = |stage: &str, e: std::io::Error| {
            tracing::debug!(stage, errno = ?e.raw_os_error(), error = %e, "local network probe attempt failed");
            outcome_from_os_error(e.raw_os_error())
        };
        let socket = match UdpSocket::bind((source, 0)) {
            Ok(s) => s,
            Err(e) => return fail("bind", e),
        };
        // Out of this interface, so a Mac with no default route still has a
        // route for the group and the only reason left to refuse is policy.
        if let Err(e) = socket2::SockRef::from(&socket).set_multicast_if_v4(&source) {
            return fail("multicast_if", e);
        }
        let _ = socket.set_multicast_ttl_v4(255);
        // Connecting is the operation TN3179 recommends for raising the alert;
        // the send is what shows whether traffic is let through.
        if let Err(e) = socket.connect(group) {
            return fail("connect", e);
        }
        match socket.send(&query) {
            Ok(_) => ProbeOutcome::Sent,
            Err(e) => fail("send", e),
        }
    };
    let reach = retry_until_decided(
        ATTEMPTS,
        || {
            let outcomes: Vec<ProbeOutcome> = lan.iter().map(|ip| send_from(*ip)).collect();
            tracing::debug!(?outcomes, "local network probe round");
            reach_from_round(&outcomes)
        },
        || std::thread::sleep(RETRY_DELAY),
    );
    (lan.len(), reach)
}

/// Trigger the macOS Local Network alert from this process and report what
/// the attempts saw. `target` (an IP literal, optionally with a port) adds a
/// TCP check of that TV from the app itself. Off macOS it does nothing.
#[tauri::command]
pub async fn local_network_access(target: Option<String>) -> Result<LocalNetworkAccess, String> {
    #[cfg(target_os = "macos")]
    {
        let (lan, multicast) = tokio::task::spawn_blocking(multicast_probe)
            .await
            .map_err(|e| e.to_string())?;
        let target_reach = match target.as_deref().and_then(parse_target) {
            Some(addr) => Some(server_repair::app_reaches(&addr.to_string()).await),
            None => None,
        };
        let status = classify(lan, multicast, target_reach);
        tracing::info!(
            ?status,
            interfaces = lan,
            ?multicast,
            ?target_reach,
            "local network probe"
        );
        Ok(LocalNetworkAccess { status })
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = target;
        Ok(LocalNetworkAccess {
            status: LocalNetworkStatus::NotApplicable,
        })
    }
}

/// Open System Settings at Privacy & Security (→ Local Network where macOS
/// has a link for it).
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
    use std::cell::Cell;

    #[test]
    fn only_ehostunreach_counts_as_unreachable() {
        if server_repair::HOST_UNREACHABLE.is_some() {
            assert_eq!(
                outcome_from_os_error(server_repair::HOST_UNREACHABLE),
                ProbeOutcome::HostUnreachable
            );
        }
        #[cfg(target_os = "macos")]
        assert_eq!(
            outcome_from_os_error(Some(65)),
            ProbeOutcome::HostUnreachable
        );
        // Darwin ENETUNREACH, EPERM, EADDRNOTAVAIL, EHOSTDOWN, and no errno.
        for code in [Some(51), Some(1), Some(49), Some(64), None] {
            assert_eq!(
                outcome_from_os_error(code),
                ProbeOutcome::OtherError,
                "{code:?}"
            );
        }
    }

    #[test]
    fn rounds_reach_on_any_send_and_block_only_when_all_unreachable() {
        use ProbeOutcome::*;
        assert_eq!(reach_from_round(&[HostUnreachable, Sent]), Reach::Reachable);
        assert_eq!(
            reach_from_round(&[HostUnreachable, HostUnreachable]),
            Reach::Unreachable
        );
        assert_eq!(
            reach_from_round(&[HostUnreachable, OtherError]),
            Reach::Unknown
        );
        assert_eq!(reach_from_round(&[]), Reach::Unknown);
    }

    /// Runs `retry_until_decided` over a script of attempt results.
    fn run(script: &[Reach]) -> (Reach, usize, usize) {
        let tried = Cell::new(0usize);
        let paused = Cell::new(0usize);
        let reach = retry_until_decided(
            ATTEMPTS,
            || {
                let r = script[tried.get()];
                tried.set(tried.get() + 1);
                r
            },
            || paused.set(paused.get() + 1),
        );
        (reach, tried.get(), paused.get())
    }

    #[test]
    fn a_first_failure_then_success_is_reached() {
        use Reach::*;
        assert_eq!(run(&[Unreachable, Reachable]), (Reachable, 2, 1));
        assert_eq!(
            run(&[Unreachable, Unreachable, Reachable]),
            (Reachable, 3, 2)
        );
    }

    #[test]
    fn unreachable_is_believed_only_after_every_attempt() {
        use Reach::*;
        assert_eq!(
            run(&[Unreachable, Unreachable, Unreachable]),
            (Unreachable, 3, 2)
        );
        // Another answer decides at once, without waiting.
        assert_eq!(run(&[Reachable]), (Reachable, 1, 0));
        assert_eq!(run(&[Unreachable, Unknown]), (Unknown, 2, 1));
    }

    #[test]
    fn status_claims_a_block_only_on_evidence() {
        use Reach::*;
        assert_eq!(classify(1, Unreachable, None), LocalNetworkStatus::Blocked);
        assert_eq!(
            classify(1, Unreachable, Some(Unreachable)),
            LocalNetworkStatus::Blocked
        );
        // The app itself reaching the TV settles it, whatever multicast said.
        assert_eq!(
            classify(1, Unreachable, Some(Reachable)),
            LocalNetworkStatus::Allowed
        );
        assert_eq!(classify(1, Reachable, None), LocalNetworkStatus::Allowed);
        // A TV timing out (maybe off) leaves it unknown.
        assert_eq!(
            classify(1, Unreachable, Some(Unknown)),
            LocalNetworkStatus::Unknown
        );
        assert_eq!(classify(1, Unknown, None), LocalNetworkStatus::Unknown);
        assert_eq!(
            classify(1, Unknown, Some(Unreachable)),
            LocalNetworkStatus::Unknown,
            "an unreachable TV alone is not a permission"
        );
        // No LAN up: "No route to host" is just the truth, not a permission.
        assert_eq!(classify(0, Unknown, None), LocalNetworkStatus::NoNetwork);
        assert_eq!(
            classify(0, Unreachable, Some(Unreachable)),
            LocalNetworkStatus::NoNetwork
        );
    }

    #[test]
    fn targets_are_ip_literals_with_adb_default_port() {
        assert_eq!(
            parse_target("192.168.1.9"),
            Some("192.168.1.9:5555".parse().unwrap())
        );
        assert_eq!(
            parse_target(" 192.168.1.9:40000 "),
            Some("192.168.1.9:40000".parse().unwrap())
        );
        assert_eq!(
            parse_target("[fe80::1]:5555"),
            Some("[fe80::1]:5555".parse().unwrap())
        );
        assert_eq!(parse_target("living-room.local"), None, "never resolved");
        assert_eq!(parse_target("adb-X._adb-tls-connect._tcp"), None);
        assert_eq!(parse_target(""), None);
    }

    #[test]
    fn lan_interfaces_exclude_loopback_link_local_and_tunnels() {
        let home = Ipv4Addr::new(192, 168, 1, 20);
        assert!(is_lan_ipv4(home, true, true, false));
        assert!(!is_lan_ipv4(Ipv4Addr::LOCALHOST, true, true, false));
        assert!(!is_lan_ipv4(
            Ipv4Addr::new(169, 254, 3, 4),
            true,
            true,
            false
        ));
        assert!(!is_lan_ipv4(Ipv4Addr::UNSPECIFIED, true, true, false));
        assert!(
            !is_lan_ipv4(home, false, true, false),
            "not broadcast-capable"
        );
        assert!(!is_lan_ipv4(home, true, false, false), "not running");
        assert!(
            !is_lan_ipv4(Ipv4Addr::new(10, 8, 0, 2), true, true, true),
            "VPN tunnel"
        );
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
            local_network_access(Some("192.168.1.9".into()))
                .await
                .unwrap()
                .status,
            LocalNetworkStatus::NotApplicable
        );
        assert!(open_local_network_settings().await.is_err());
    }
}

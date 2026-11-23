//! The MAC address of the network interface a TV is reached on, read while it
//! is connected, so a saved device can later be woken with Wake-on-LAN.
//!
//! Best effort by design: any failure, an unreadable interface or an ambiguous
//! answer is "no MAC", never an error. The hardware id is read in the same
//! shell call, so the caller files the MAC only under the device that
//! actually answered.

use std::collections::BTreeMap;

use serde::Serialize;
use tauri::State;

use crate::adb::{batch_command, split_batch};

use super::devices::verified_hardware_id;
use super::AppState;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeviceMacAddress {
    /// `ro.serialno` as read in the same call; `None` when unverifiable.
    pub hardware_id: Option<String>,
    /// Lowercase `aa:bb:cc:dd:ee:ff`, or `None` when it could not be told.
    pub mac: Option<String>,
}

const MAC_READS: [&str; 4] = [
    "getprop ro.serialno",
    "ip -o -4 addr show",
    "ip -o link show",
    // `path:value` per interface; read when `ip` is denied or missing.
    "grep -H . /sys/class/net/*/address",
];

/// `device_mac_address` — `host` is the address the app reaches the device
/// on, which picks the interface carrying it.
#[tauri::command]
pub async fn device_mac_address(
    state: State<'_, AppState>,
    serial: String,
    host: Option<String>,
) -> Result<DeviceMacAddress, String> {
    Ok(device_mac_address_impl(state.inner(), &serial, host.as_deref()).await)
}

pub async fn device_mac_address_impl(
    state: &AppState,
    serial: &str,
    host: Option<&str>,
) -> DeviceMacAddress {
    let adb = state.adb_snapshot().await;
    let command = format!("{}; true", batch_command(&MAC_READS));
    match adb.shell(serial, &command).await {
        Ok(out) => mac_from_sections(&split_batch(&out.stdout, MAC_READS.len()), host),
        Err(_) => DeviceMacAddress {
            hardware_id: None,
            mac: None,
        },
    }
}

fn mac_from_sections(sections: &[String], host: Option<&str>) -> DeviceMacAddress {
    let get = |i: usize| sections.get(i).map(String::as_str).unwrap_or("");
    DeviceMacAddress {
        hardware_id: verified_hardware_id(get(0)).map(str::to_string),
        mac: pick_mac(get(1), get(2), get(3), host),
    }
}

/// Lowercase a MAC and accept it only when it can name one real interface:
/// six octets, not all zeros, not broadcast or multicast, and not Android's
/// `02:00:00:00:00:00` placeholder for a hidden address.
fn normalize_mac(raw: &str) -> Option<String> {
    let mac = raw.trim().to_ascii_lowercase();
    let octets: Vec<&str> = mac.split(':').collect();
    if octets.len() != 6
        || octets
            .iter()
            .any(|o| o.len() != 2 || !o.chars().all(|c| c.is_ascii_hexdigit()))
    {
        return None;
    }
    let first = u8::from_str_radix(octets[0], 16).ok()?;
    if first & 1 == 1 || mac == "00:00:00:00:00:00" || mac == "02:00:00:00:00:00" {
        return None;
    }
    Some(mac)
}

/// `wlan0@if3` → `wlan0`.
fn interface_name(raw: &str) -> &str {
    let name = raw.trim_end_matches(':');
    name.split('@').next().unwrap_or(name)
}

/// The interface whose IPv4 address is `host`; without one, the only
/// interface with a non-loopback IPv4 address and a MAC. Anything else is too
/// uncertain to wake the right device with, so it is no MAC.
fn pick_mac(addrs: &str, links: &str, sysfs: &str, host: Option<&str>) -> Option<String> {
    let mut macs: BTreeMap<String, String> = BTreeMap::new();
    // `3: wlan0: <...> mtu 1500 ... link/ether aa:bb:cc:dd:ee:ff brd ...`
    for line in links.lines() {
        let mut words = line.split_whitespace();
        let (Some(_), Some(name)) = (words.next(), words.next()) else {
            continue;
        };
        let rest: Vec<&str> = words.collect();
        if let Some(i) = rest.iter().position(|w| *w == "link/ether") {
            if let Some(mac) = rest.get(i + 1).and_then(|m| normalize_mac(m)) {
                macs.insert(interface_name(name).to_string(), mac);
            }
        }
    }
    // `/sys/class/net/wlan0/address:aa:bb:cc:dd:ee:ff` — what the kernel
    // reports directly, so it wins over `ip link`.
    for line in sysfs.lines() {
        let Some((name, mac)) = line
            .trim()
            .strip_prefix("/sys/class/net/")
            .and_then(|p| p.split_once("/address:"))
        else {
            continue;
        };
        if let Some(mac) = normalize_mac(mac) {
            macs.insert(name.to_string(), mac);
        }
    }
    // `3: wlan0    inet 192.168.1.5/24 brd ... scope global wlan0`
    let mut ipv4: Vec<(String, String)> = Vec::new();
    for line in addrs.lines() {
        let words: Vec<&str> = line.split_whitespace().collect();
        let Some(i) = words.iter().position(|w| *w == "inet") else {
            continue;
        };
        let (Some(name), Some(addr)) = (words.get(1), words.get(i + 1)) else {
            continue;
        };
        let ip = addr.split('/').next().unwrap_or(addr);
        ipv4.push((interface_name(name).to_string(), ip.to_string()));
    }

    let host = host.map(|h| h.trim().trim_start_matches('[').trim_end_matches(']'));
    if let Some(host) = host.filter(|h| !h.is_empty()) {
        let carrying: Vec<&String> = ipv4
            .iter()
            .filter(|(_, ip)| ip == host)
            .map(|(name, _)| name)
            .collect();
        if let [name] = carrying.as_slice() {
            return macs.get(*name).cloned();
        }
        if !carrying.is_empty() {
            return None;
        }
    }
    let mut candidates: Vec<&String> = ipv4
        .iter()
        .filter(|(name, ip)| !ip.starts_with("127.") && macs.contains_key(name))
        .map(|(name, _)| name)
        .collect();
    candidates.sort();
    candidates.dedup();
    match candidates.as_slice() {
        [name] => macs.get(*name).cloned(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adb::BATCH_SEPARATOR;
    use crate::commands::test_support::{state_with, MockAdb};

    const ADDRS: &str =
        "1: lo    inet 127.0.0.1/8 scope host lo\\       valid_lft forever preferred_lft forever\n\
3: eth0    inet 192.168.1.40/24 brd 192.168.1.255 scope global eth0\\       valid_lft forever\n\
4: wlan0    inet 192.168.1.42/24 brd 192.168.1.255 scope global wlan0\\       valid_lft forever";
    const LINKS: &str = "1: lo: <LOOPBACK,UP,LOWER_UP> mtu 65536 qdisc noqueue state UNKNOWN mode DEFAULT group default qlen 1000\\    link/loopback 00:00:00:00:00:00 brd 00:00:00:00:00:00\n\
3: eth0: <BROADCAST,MULTICAST,UP,LOWER_UP> mtu 1500 qdisc mq state UP mode DEFAULT group default qlen 1000\\    link/ether 00:04:4B:AA:BB:01 brd ff:ff:ff:ff:ff:ff\n\
4: wlan0: <BROADCAST,MULTICAST,UP,LOWER_UP> mtu 1500 qdisc mq state UP mode DORMANT group default qlen 3000\\    link/ether 00:04:4b:aa:bb:02 brd ff:ff:ff:ff:ff:ff";

    #[test]
    fn picks_the_interface_carrying_the_connected_address() {
        assert_eq!(
            pick_mac(ADDRS, LINKS, "", Some("192.168.1.42")).as_deref(),
            Some("00:04:4b:aa:bb:02")
        );
        assert_eq!(
            pick_mac(ADDRS, LINKS, "", Some("192.168.1.40")).as_deref(),
            Some("00:04:4b:aa:bb:01"),
            "uppercase input is normalized"
        );
    }

    #[test]
    fn two_live_interfaces_and_no_matching_host_is_no_mac() {
        assert_eq!(pick_mac(ADDRS, LINKS, "", None), None);
        assert_eq!(pick_mac(ADDRS, LINKS, "", Some("10.0.0.9")), None);
    }

    #[test]
    fn a_single_live_interface_is_used_when_the_host_is_unknown() {
        let addrs = "1: lo    inet 127.0.0.1/8 scope host lo\n4: wlan0    inet 10.0.0.5/24 scope global wlan0";
        assert_eq!(
            pick_mac(addrs, LINKS, "", None).as_deref(),
            Some("00:04:4b:aa:bb:02")
        );
    }

    #[test]
    fn sysfs_wins_and_fills_in_when_ip_link_is_denied() {
        let sysfs = "/sys/class/net/eth0/address:00:04:4b:aa:bb:01\n/sys/class/net/wlan0/address:0e:11:22:33:44:55\n/sys/class/net/lo/address:00:00:00:00:00:00";
        assert_eq!(
            pick_mac(ADDRS, "", sysfs, Some("192.168.1.42")).as_deref(),
            Some("0e:11:22:33:44:55")
        );
    }

    #[test]
    fn placeholder_and_malformed_macs_are_rejected() {
        for bad in [
            "02:00:00:00:00:00",
            "00:00:00:00:00:00",
            "ff:ff:ff:ff:ff:ff",
            "01:00:5e:00:00:01",
            "00:04:4b:aa:bb",
            "zz:04:4b:aa:bb:02",
            "",
        ] {
            assert_eq!(normalize_mac(bad), None, "{bad}");
        }
        let sysfs = "/sys/class/net/wlan0/address:02:00:00:00:00:00";
        assert_eq!(pick_mac(ADDRS, "", sysfs, Some("192.168.1.42")), None);
    }

    #[test]
    fn virtual_interface_suffixes_are_stripped() {
        let addrs = "5: eth0@if6    inet 192.168.1.50/24 scope global eth0";
        let links = "5: eth0@if6: <BROADCAST,UP> mtu 1500\\    link/ether 00:11:22:33:44:55 brd ff:ff:ff:ff:ff:ff";
        assert_eq!(
            pick_mac(addrs, links, "", Some("192.168.1.50")).as_deref(),
            Some("00:11:22:33:44:55")
        );
    }

    #[tokio::test]
    async fn reads_id_and_mac_in_one_call_and_never_errors() {
        let out = ["0323220012345", ADDRS, LINKS, ""].join(&format!("\n{BATCH_SEPARATOR}\n"));
        let state = state_with(MockAdb::default().on_shell("ro.serialno", &out));
        let read = device_mac_address_impl(&state, "192.168.1.42:5555", Some("192.168.1.42")).await;
        assert_eq!(
            read,
            DeviceMacAddress {
                hardware_id: Some("0323220012345".into()),
                mac: Some("00:04:4b:aa:bb:02".into()),
            }
        );

        let state = state_with(MockAdb::default().on_shell_err("ro.serialno", "device offline"));
        let read = device_mac_address_impl(&state, "192.168.1.42:5555", Some("192.168.1.42")).await;
        assert_eq!(
            read,
            DeviceMacAddress {
                hardware_id: None,
                mac: None
            }
        );

        let unknown = format!("unknown\n{BATCH_SEPARATOR}\n");
        let state = state_with(MockAdb::default().on_shell("ro.serialno", &unknown));
        let read = device_mac_address_impl(&state, "192.168.1.42:5555", None).await;
        assert_eq!(read.hardware_id, None, "a placeholder serial is no id");
        assert_eq!(read.mac, None);
    }
}

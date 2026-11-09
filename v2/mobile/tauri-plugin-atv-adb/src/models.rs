use serde::{Deserialize, Serialize};

/// A wireless-debugging / network-debugging service found over mDNS on the LAN.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredAdbDevice {
    pub name: String,
    /// Optional Cast TXT display metadata for this endpoint, not verified identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub friendly_name: Option<String>,
    /// IPv4 when the TV has one; otherwise bracketed IPv6 (`[2001:db8::5]`, or
    /// `[fe80::1%3]` with a numeric scope), so `"{host}:{port}"` is a socket address.
    pub host: String,
    pub port: u16,
    /// mDNS service type, e.g. `_adb-tls-pairing._tcp.`, `_adb-tls-connect._tcp.`
    /// or the legacy `_adb._tcp.`.
    pub service: String,
}

/// Payload of [`NETWORK_CHANGED_EVENT`](crate::NETWORK_CHANGED_EVENT): the phone's
/// default network changed in a way that can strand an open TV connection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkChange {
    /// `lost` (no default network), `changed` (a different one) or `addresses`
    /// (an address this phone had on it is gone).
    pub reason: String,
}

impl NetworkChange {
    /// One message from the Kotlin network watcher; anything malformed is dropped.
    #[cfg_attr(not(mobile), allow(dead_code))]
    pub(crate) fn from_channel_json(json: &str) -> Option<Self> {
        serde_json::from_str::<Self>(json)
            .ok()
            .filter(|change| !change.reason.is_empty())
    }
}

/// What the Kotlin create-document picker reported: `saved` only after the whole file was
/// written to the document the user chose; `false` when they backed out of the picker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaveCopyResponse {
    pub saved: bool,
    /// The provider's display name for the new document, when it reports one.
    #[serde(default)]
    pub name: Option<String>,
}

// Payloads sent across the JNI bridge to the Kotlin AdbPlugin's @Commands.
// Bridge-only, so gated off host builds where the desktop stub never sends them.

#[cfg(mobile)]
#[derive(Debug, Serialize)]
pub(crate) struct DiscoverPayload {
    #[serde(rename = "timeoutMs")]
    pub timeout_ms: u64,
}

#[cfg(mobile)]
#[derive(Debug, Deserialize)]
pub(crate) struct DiscoverResponse {
    pub devices: Vec<DiscoveredAdbDevice>,
}

#[cfg(mobile)]
#[derive(Serialize)]
pub(crate) struct WatchNetworkPayload {
    pub channel: tauri::ipc::Channel,
}

#[cfg(mobile)]
#[derive(Serialize)]
pub(crate) struct SaveCopyPayload<'a> {
    pub path: &'a str,
    pub name: &'a str,
    pub mime: &'a str,
}

#[cfg(mobile)]
#[derive(Serialize)]
pub(crate) struct ShareFilePayload<'a> {
    pub path: &'a str,
    pub mime: &'a str,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;

    #[test]
    fn friendly_name_is_optional_display_metadata_and_keeps_the_raw_adb_instance() {
        let mut device: DiscoveredAdbDevice = serde_json::from_str(
            r#"{"name":"adb-tv-a","host":"192.0.2.10","port":5555,"service":"_adb._tcp."}"#,
        )
        .unwrap();
        assert!(device.friendly_name.is_none());
        device.friendly_name = Some("Studio TV".to_string());
        let value = serde_json::to_value(&device).unwrap();
        assert_eq!(value["friendly_name"], "Studio TV");
        assert_eq!(value["name"], "adb-tv-a");
        assert_eq!(value["port"], 5555);
    }

    #[test]
    fn network_change_parses_the_kotlin_watcher_message() {
        for reason in ["lost", "changed", "addresses"] {
            let json = format!(r#"{{"reason":"{reason}"}}"#);
            assert_eq!(
                NetworkChange::from_channel_json(&json),
                Some(NetworkChange {
                    reason: reason.to_string()
                })
            );
        }
    }

    #[test]
    fn save_copy_response_reads_the_kotlin_result() {
        let saved: SaveCopyResponse =
            serde_json::from_str(r#"{"saved":true,"name":"tv-screenshot.png"}"#).unwrap();
        assert_eq!(saved.name.as_deref(), Some("tv-screenshot.png"));
        let canceled: SaveCopyResponse = serde_json::from_str(r#"{"saved":false}"#).unwrap();
        assert!(!canceled.saved);
        assert_eq!(canceled.name, None);
    }

    #[test]
    fn malformed_network_change_is_dropped() {
        for json in ["", "null", "{}", r#"{"reason":""}"#, r#"{"reason":3}"#] {
            assert_eq!(NetworkChange::from_channel_json(json), None, "{json}");
        }
    }

    // The app connects with `format!("{host}:{port}").parse::<SocketAddr>()`, so the
    // host forms the Kotlin discovery emits (Java's uncompressed IPv6 text, numeric scope)
    // must parse, and a bare IPv6 host must not be what it emits.
    #[test]
    fn discovered_hosts_form_socket_addresses() {
        for host in [
            "192.168.1.20",
            "[2001:db8:0:0:0:0:0:5]",
            "[fe80:0:0:0:1c2d:3e4f:5a6b:7c8d%3]",
        ] {
            let address = format!("{host}:5555");
            assert!(address.parse::<SocketAddr>().is_ok(), "{address}");
        }
        assert!("fe80::1:5555".parse::<SocketAddr>().is_err());
        assert!("[fe80::1%wlan0]:5555".parse::<SocketAddr>().is_err());
    }
}

use serde::{Deserialize, Serialize};

/// A wireless-debugging / network-debugging service found over mDNS on the LAN.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredAdbDevice {
    pub name: String,
    #[serde(default)]
    pub friendly_name: Option<String>,
    #[serde(default)]
    pub network: Option<String>,
    pub host: String,
    pub port: u16,
    /// mDNS service type, e.g. `_adb-tls-pairing._tcp.`, `_adb-tls-connect._tcp.`
    /// or the legacy `_adb._tcp.`.
    pub service: String,
}

// Payloads sent across the JNI bridge to the Kotlin AdbPlugin's `discover`
// @Command. Bridge-only, so gated off host builds where the desktop stub never
// sends them.

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

#[cfg(test)]
mod tests {
    use super::DiscoveredAdbDevice;

    #[test]
    fn old_discovery_payload_does_not_need_metadata_fields() {
        let device: DiscoveredAdbDevice = serde_json::from_str(
            r#"{"name":"adb-fixture","host":"192.0.2.42","port":5555,"service":"_adb._tcp."}"#,
        )
        .unwrap();
        assert!(device.friendly_name.is_none());
        assert!(device.network.is_none());
    }

    #[test]
    fn unlocated_name_keeps_its_metadata_source_and_has_no_endpoint() {
        let device: DiscoveredAdbDevice = serde_json::from_str(
            r#"{"name":"Study TV","friendly_name":"Study TV","host":"","port":0,"service":"_androidtvremote2._tcp."}"#,
        )
        .unwrap();
        assert_eq!(device.friendly_name.as_deref(), Some("Study TV"));
        assert!(device.host.is_empty());
        assert_eq!(device.port, 0);
        assert_eq!(device.service, "_androidtvremote2._tcp.");
    }
}

use serde::{Deserialize, Serialize};

/// A wireless-debugging / network-debugging service found over mDNS on the LAN.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredAdbDevice {
    pub name: String,
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

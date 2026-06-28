use serde::{Deserialize, Serialize};

/// A wireless-debugging service found over mDNS on the local network.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredAdbDevice {
    pub name: String,
    pub host: String,
    pub port: u16,
    /// mDNS service type, e.g. `_adb-tls-pairing._tcp.` or `_adb-tls-connect._tcp.`.
    pub service: String,
}

/// Result of a pair/connect against a device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectResponse {
    pub serial: String,
    pub host: String,
    pub port: u16,
    pub message: String,
}

/// Output of a `shell:` stream, shaped like the core `AdbOutput`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdbCommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

// Payloads sent across the JNI bridge to the Kotlin AdbPlugin @Command methods.
// Bridge-only, so gated off host builds where the desktop stub never sends them.

#[cfg(mobile)]
#[derive(Debug, Serialize)]
pub(crate) struct DiscoverPayload {
    #[serde(rename = "timeoutMs")]
    pub timeout_ms: u64,
}

#[cfg(mobile)]
#[derive(Debug, Serialize)]
pub(crate) struct PairPayload<'a> {
    pub host: &'a str,
    pub port: u16,
    pub code: &'a str,
}

#[cfg(mobile)]
#[derive(Debug, Serialize)]
pub(crate) struct ConnectPayload<'a> {
    pub host: &'a str,
    pub port: u16,
}

#[cfg(mobile)]
#[derive(Debug, Serialize)]
pub(crate) struct SerialPayload<'a> {
    pub serial: &'a str,
}

#[cfg(mobile)]
#[derive(Debug, Serialize)]
pub(crate) struct ShellPayload<'a> {
    pub serial: &'a str,
    pub command: &'a str,
}

#[cfg(mobile)]
#[derive(Debug, Deserialize)]
pub(crate) struct DiscoverResponse {
    pub devices: Vec<DiscoveredAdbDevice>,
}

#[cfg(mobile)]
#[derive(Debug, Deserialize)]
pub(crate) struct ScreencapResponse {
    /// Base64-encoded PNG bytes (the bridge never carries raw bytes through JSON).
    pub png_base64: String,
}

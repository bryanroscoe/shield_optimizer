pub use atv_direct_adb::{SharedWirelessAdb, WirelessAdb, WirelessStatus};
pub use tauri_plugin_atv_adb::DiscoveredAdbDevice;

/// How long Android NSD browses for wireless-debugging services.
pub const DISCOVER_TIMEOUT_MS: u64 = 3_000;

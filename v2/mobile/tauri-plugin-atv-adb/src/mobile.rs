use tauri::{
    plugin::{PluginApi, PluginHandle},
    AppHandle, Runtime,
};

use crate::error::Result;
use crate::models::*;

/// Must match the Kotlin plugin's package (the Android library namespace).
const PLUGIN_IDENTIFIER: &str = "app.tauri.atvadb";

pub fn init<R: Runtime, C: serde::de::DeserializeOwned>(
    _app: &AppHandle<R>,
    api: PluginApi<R, C>,
) -> Result<Adb<R>> {
    let handle = api.register_android_plugin(PLUGIN_IDENTIFIER, "AdbPlugin")?;
    Ok(Adb(handle))
}

/// Rust-side handle to the Kotlin plugin. Its only job now is **mDNS
/// discovery** (Android's `NsdManager`); connect/shell/screencap/reboot are
/// driven directly from Rust by the app's `WirelessAdb` via the pure-Rust
/// `adb_client` crate. Discovery blocks on the JNI hop, so callers on an async
/// runtime should wrap it in `spawn_blocking`.
pub struct Adb<R: Runtime>(PluginHandle<R>);

impl<R: Runtime> Adb<R> {
    pub fn discover(&self, timeout_ms: u64) -> Result<Vec<DiscoveredAdbDevice>> {
        let res: DiscoverResponse = self
            .0
            .run_mobile_plugin("discover", DiscoverPayload { timeout_ms })?;
        Ok(res.devices)
    }
}

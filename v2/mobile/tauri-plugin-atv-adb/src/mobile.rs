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

/// Rust-side handle to the libadb-android-backed Kotlin transport.
///
/// Every call blocks on the JNI hop, so callers on an async runtime should wrap
/// these in `spawn_blocking` (latency is dominated by the on-device command,
/// not the bridge).
pub struct Adb<R: Runtime>(PluginHandle<R>);

impl<R: Runtime> Adb<R> {
    pub fn discover(&self, timeout_ms: u64) -> Result<Vec<DiscoveredAdbDevice>> {
        let res: DiscoverResponse = self
            .0
            .run_mobile_plugin("discover", DiscoverPayload { timeout_ms })?;
        Ok(res.devices)
    }

    pub fn pair(&self, host: &str, port: u16, code: &str) -> Result<ConnectResponse> {
        Ok(self
            .0
            .run_mobile_plugin("pair", PairPayload { host, port, code })?)
    }

    pub fn connect(&self, host: &str, port: u16) -> Result<ConnectResponse> {
        Ok(self
            .0
            .run_mobile_plugin("connect", ConnectPayload { host, port })?)
    }

    pub fn disconnect(&self, serial: &str) -> Result<()> {
        let _: serde_json::Value = self
            .0
            .run_mobile_plugin("disconnect", SerialPayload { serial })?;
        Ok(())
    }

    pub fn shell(&self, serial: &str, command: &str) -> Result<AdbCommandOutput> {
        Ok(self
            .0
            .run_mobile_plugin("shell", ShellPayload { serial, command })?)
    }

    /// Returns base64-encoded PNG bytes from `exec-out screencap -p`.
    pub fn screencap(&self, serial: &str) -> Result<String> {
        let res: ScreencapResponse = self
            .0
            .run_mobile_plugin("screencap", SerialPayload { serial })?;
        Ok(res.png_base64)
    }
}

use tauri::{
    ipc::{Channel, InvokeResponseBody},
    plugin::{PluginApi, PluginHandle},
    AppHandle, Emitter, Runtime,
};

use crate::error::Result;
use crate::models::*;
use crate::NETWORK_CHANGED_EVENT;

/// Must match the Kotlin plugin's package (the Android library namespace).
const PLUGIN_IDENTIFIER: &str = "app.tauri.atvadb";

pub fn init<R: Runtime, C: serde::de::DeserializeOwned>(
    app: &AppHandle<R>,
    api: PluginApi<R, C>,
) -> Result<Adb<R>> {
    let handle = api.register_android_plugin(PLUGIN_IDENTIFIER, "AdbPlugin")?;
    // Without the watch the app still works; it notices a dropped TV on its heartbeat.
    if let Err(error) = watch_network(app, &handle) {
        eprintln!("atv-adb: network watch unavailable: {error}");
    }
    Ok(Adb(handle))
}

/// Starts the Kotlin default-network watcher and re-emits each change it reports to
/// the frontend as [`NETWORK_CHANGED_EVENT`].
fn watch_network<R: Runtime>(app: &AppHandle<R>, handle: &PluginHandle<R>) -> Result<()> {
    let app = app.clone();
    let channel: Channel = Channel::new(move |body| {
        if let InvokeResponseBody::Json(json) = body {
            if let Some(change) = NetworkChange::from_channel_json(&json) {
                let _ = app.emit(NETWORK_CHANGED_EVENT, change);
            }
        }
        Ok(())
    });
    handle.run_mobile_plugin::<()>("watchNetwork", WatchNetworkPayload { channel })?;
    Ok(())
}

/// Rust-side handle to the Kotlin plugin. Its job is **mDNS discovery**
/// (Android's `NsdManager`) plus the network watch started in [`init`];
/// connect/shell/screencap/reboot are driven directly from Rust by the app's
/// `WirelessAdb` via the pure-Rust `adb_client` crate. Discovery blocks on the
/// JNI hop, so callers on an async runtime should wrap it in `spawn_blocking`.
pub struct Adb<R: Runtime>(PluginHandle<R>);

impl<R: Runtime> Adb<R> {
    pub fn discover(&self, timeout_ms: u64) -> Result<Vec<DiscoveredAdbDevice>> {
        let res: DiscoverResponse = self
            .0
            .run_mobile_plugin("discover", DiscoverPayload { timeout_ms })?;
        Ok(res.devices)
    }
}

use tauri::{plugin::PluginApi, AppHandle, Runtime};

use crate::error::Result;
use crate::models::*;

pub fn init<R: Runtime, C: serde::de::DeserializeOwned>(
    _app: &AppHandle<R>,
    _api: PluginApi<R, C>,
) -> Result<Adb<R>> {
    Ok(Adb(std::marker::PhantomData))
}

/// Host-build stub. mDNS discovery has no transport off Android, so it returns
/// an empty list — the app's WirelessAdb driver never reaches this on desktop,
/// but it keeps the crate compiling for `cargo test` / `cargo build` on CI hosts.
pub struct Adb<R: Runtime>(std::marker::PhantomData<fn() -> R>);

impl<R: Runtime> Adb<R> {
    pub fn discover(&self, _timeout_ms: u64) -> Result<Vec<DiscoveredAdbDevice>> {
        Ok(Vec::new())
    }
}

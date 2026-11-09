use tauri::{plugin::PluginApi, AppHandle, Runtime};

use crate::error::{Error, Result};
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

    /// Saving to a document needs Android's picker; host builds have none.
    pub fn save_copy(&self, _path: &str, _name: &str, _mime: &str) -> Result<SaveCopyResponse> {
        Err(Error::ExportUnsupported)
    }

    pub fn share_file(&self, _path: &str, _mime: &str) -> Result<()> {
        Err(Error::ExportUnsupported)
    }
}

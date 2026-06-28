use tauri::{plugin::PluginApi, AppHandle, Runtime};

use crate::error::{Error, Result};
use crate::models::*;

pub fn init<R: Runtime, C: serde::de::DeserializeOwned>(
    _app: &AppHandle<R>,
    _api: PluginApi<R, C>,
) -> Result<Adb<R>> {
    Ok(Adb(std::marker::PhantomData))
}

/// Host-build stub. Wireless ADB has no transport off Android, so every call
/// reports `Unsupported` — the app's WirelessAdb driver never reaches these on
/// desktop, but they keep the crate compiling for `cargo test` on CI hosts.
pub struct Adb<R: Runtime>(std::marker::PhantomData<fn() -> R>);

impl<R: Runtime> Adb<R> {
    pub fn discover(&self, _timeout_ms: u64) -> Result<Vec<DiscoveredAdbDevice>> {
        Ok(Vec::new())
    }

    pub fn pair(&self, _host: &str, _port: u16, _code: &str) -> Result<ConnectResponse> {
        Err(Error::Unsupported)
    }

    pub fn connect(&self, _host: &str, _port: u16) -> Result<ConnectResponse> {
        Err(Error::Unsupported)
    }

    pub fn disconnect(&self, _serial: &str) -> Result<()> {
        Ok(())
    }

    pub fn shell(&self, _serial: &str, _command: &str) -> Result<AdbCommandOutput> {
        Err(Error::Unsupported)
    }

    pub fn screencap(&self, _serial: &str) -> Result<String> {
        Err(Error::Unsupported)
    }
}

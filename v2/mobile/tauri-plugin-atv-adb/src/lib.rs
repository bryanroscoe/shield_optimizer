//! Wireless-ADB transport plugin for ATV Optimizer.
//!
//! On Android this ships a libadb-android-backed Kotlin plugin (see `android/`)
//! and exposes it to Rust as [`Adb`]. The app's `WirelessAdb` (`impl AdbDriver`)
//! drives a TV over Wi-Fi through it — no PC, no adb server. The plugin has no
//! JS-invokable commands; the frontend talks to the app's `wireless_*`
//! commands, which call [`AdbExt::adb`].

use tauri::{
    plugin::{Builder, TauriPlugin},
    Manager, Runtime,
};

mod error;
mod models;

pub use error::{Error, Result};
pub use models::{AdbCommandOutput, ConnectResponse, DiscoveredAdbDevice};

#[cfg(not(mobile))]
mod desktop;
#[cfg(mobile)]
mod mobile;

#[cfg(not(mobile))]
pub use desktop::Adb;
#[cfg(mobile)]
pub use mobile::Adb;

/// Access the wireless-ADB transport from any [`Manager`] (`app.adb()`).
pub trait AdbExt<R: Runtime> {
    fn adb(&self) -> &Adb<R>;
}

impl<R: Runtime, T: Manager<R>> AdbExt<R> for T {
    fn adb(&self) -> &Adb<R> {
        self.state::<Adb<R>>().inner()
    }
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("atv-adb")
        .setup(|app, api| {
            #[cfg(mobile)]
            let adb = mobile::init(app, api)?;
            #[cfg(not(mobile))]
            let adb = desktop::init(app, api)?;
            app.manage(adb);
            Ok(())
        })
        .build()
}

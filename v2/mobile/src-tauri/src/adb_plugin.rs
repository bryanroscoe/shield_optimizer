use tauri::{plugin::TauriPlugin, Manager, Runtime};

#[cfg(target_os = "android")]
const PLUGIN_IDENTIFIER: &str = "com.atvoptimizer.adb";

pub struct AdbPlugin<R: Runtime> {
    #[cfg(target_os = "android")]
    handle: tauri::plugin::PluginHandle<R>,
    #[cfg(not(target_os = "android"))]
    _marker: std::marker::PhantomData<fn() -> R>,
}

impl<R: Runtime> AdbPlugin<R> {
    #[cfg(target_os = "android")]
    pub async fn run<T: serde::de::DeserializeOwned>(
        &self,
        command: impl AsRef<str>,
        payload: impl serde::Serialize,
    ) -> Result<T, tauri::plugin::mobile::PluginInvokeError> {
        self.handle.run_mobile_plugin_async(command, payload).await
    }
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    tauri::plugin::Builder::<R>::new("atv_adb")
        .setup(|app, _api| {
            #[cfg(target_os = "android")]
            let bridge = AdbPlugin {
                handle: _api.register_android_plugin(PLUGIN_IDENTIFIER, "AdbPlugin")?,
            };
            #[cfg(not(target_os = "android"))]
            let bridge = AdbPlugin::<R> {
                _marker: std::marker::PhantomData,
            };
            app.manage(bridge);
            Ok(())
        })
        .build()
}

use serde::{Serialize, Serializer};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Transport(String),
    #[error("wireless ADB is only available on Android builds")]
    Unsupported,
    #[error("Save a copy and Share are only available on Android.")]
    ExportUnsupported,
    #[error("Picking files from this phone is only available on Android.")]
    ImportUnsupported,
    #[cfg(mobile)]
    #[error(transparent)]
    PluginInvoke(#[from] tauri::plugin::mobile::PluginInvokeError),
}

impl Serialize for Error {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;

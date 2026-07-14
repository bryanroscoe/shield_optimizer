use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};

use crate::{Result, RustADBError};

pub fn check_extension_is_apk<P: AsRef<Path>>(path: P) -> Result<()> {
    if let Some(extension) = path.as_ref().extension()
        && ![OsStr::new("apk")].contains(&extension)
    {
        return Err(RustADBError::WrongFileExtension(format!(
            "{} is not an APK file",
            extension.to_string_lossy()
        )));
    }

    log::debug!("Given file is an APK");

    Ok(())
}

/// Get the default path to the ADB key file.
/// First checks for the presence of the environment variable `ANDROID_USER_HOME`, defaulting to the user's home directory.
pub fn get_default_adb_key_path() -> Result<PathBuf> {
    let android_user_home = std::env::var("ANDROID_USER_HOME")
        .ok()
        .map(|android_user_home| PathBuf::from(android_user_home).join("android"));
    let default_dot_android = std::env::home_dir().map(|home| home.join(".android"));

    Ok(android_user_home
        .or(default_dot_android)
        .ok_or(RustADBError::NoHomeDirectory)?
        .join("adbkey"))
}

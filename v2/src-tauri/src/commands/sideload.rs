//! APK sideload — `adb install` against a user-picked file.

use serde::Serialize;
use std::path::PathBuf;
use tauri::State;

use super::AppState;

#[derive(Serialize)]
pub struct InstallApkResult {
    pub ok: bool,
    /// Path that was installed (or attempted).
    pub path: String,
    /// adb's verbatim output — surfaces helpful errors like
    /// `INSTALL_FAILED_VERSION_DOWNGRADE` to the user.
    pub message: String,
    /// Optional decoded hint for common failure codes.
    pub hint: Option<String>,
}

/// `install_apk` — `adb -s <serial> install [-r] <path>`. The frontend uses
/// the dialog plugin to obtain a file path before calling this.
#[tauri::command]
pub async fn install_apk(
    state: State<'_, AppState>,
    serial: String,
    apk_path: String,
    reinstall: Option<bool>,
) -> Result<InstallApkResult, String> {
    let path_buf = PathBuf::from(&apk_path);
    if !path_buf.is_file() {
        return Ok(InstallApkResult {
            ok: false,
            path: apk_path,
            message: "APK file does not exist".to_string(),
            hint: None,
        });
    }

    let adb = state.adb_snapshot().await;
    let mut args: Vec<String> = vec!["-s".into(), serial.clone(), "install".into()];
    if reinstall.unwrap_or(true) {
        args.push("-r".into());
    }
    args.push(apk_path.clone());
    let args_ref: Vec<&str> = args.iter().map(String::as_str).collect();

    let out = adb
        .raw(&args_ref)
        .await
        .map_err(|e| format!("adb install: {e}"))?;
    let combined = if out.stdout.trim().is_empty() {
        out.stderr.clone()
    } else {
        out.stdout.clone()
    };
    let ok = combined.contains("Success");
    let hint = decode_install_error(&combined);

    Ok(InstallApkResult {
        ok,
        path: apk_path,
        message: combined,
        hint,
    })
}

/// Decode the common `INSTALL_FAILED_*` / `DELETE_FAILED_*` codes into a one-line
/// hint. Mirrors v1's `Get-UninstallErrorReason` + the inline decoder in
/// `Install-ApkFile`.
fn decode_install_error(text: &str) -> Option<String> {
    for (needle, hint) in [
        (
            "INSTALL_FAILED_INSUFFICIENT_STORAGE",
            "Not enough free storage on the device — free up space and retry.",
        ),
        (
            "INSTALL_FAILED_VERSION_DOWNGRADE",
            "Installed version is newer than this APK. Uninstall the device's copy first, or use a newer APK.",
        ),
        (
            "INSTALL_FAILED_ALREADY_EXISTS",
            "Same version already installed. Pass `reinstall=true` to force.",
        ),
        (
            "INSTALL_FAILED_OLDER_SDK",
            "APK requires a newer Android version than this device runs.",
        ),
        (
            "INSTALL_FAILED_NO_MATCHING_ABIS",
            "APK doesn't include a native library for this device's CPU architecture.",
        ),
        (
            "INSTALL_FAILED_INVALID_APK",
            "APK file is corrupt or malformed.",
        ),
        (
            "INSTALL_PARSE_FAILED",
            "APK couldn't be parsed (may be corrupt or not actually an APK).",
        ),
    ] {
        if text.contains(needle) {
            return Some(hint.to_string());
        }
    }
    None
}

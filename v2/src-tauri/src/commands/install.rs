//! ADB install / status commands — let the UI auto-download Google's
//! platform-tools when no adb is present, matching v1's Check-Adb behavior.

use std::sync::Arc;

use serde::Serialize;
use tauri::State;

use crate::adb::driver::discover_adb_binary;
use crate::adb::{install_platform_tools, AdbDriver, SubprocessAdb};

use super::AppState;

#[derive(Serialize)]
pub struct AdbStatus {
    /// Is an adb binary currently configured?
    pub available: bool,
    /// Path to the binary if discovery resolves one — useful in the UI for
    /// "Using adb at /opt/homebrew/bin/adb" diagnostics.
    pub path: Option<String>,
    /// What the last `adb devices` call returned, as a quick connectivity probe.
    /// `None` when adb isn't available.
    pub last_probe: Option<String>,
}

/// `adb_status` — fast diagnostic the UI shows when devices fail to list.
#[tauri::command]
pub async fn adb_status(state: State<'_, AppState>) -> Result<AdbStatus, String> {
    let adb = state.adb_snapshot().await;
    let path = discover_adb_binary().map(|p| p.display().to_string());
    let probe = adb.raw(&["devices"]).await;
    match probe {
        Ok(out) => Ok(AdbStatus {
            available: true,
            path,
            last_probe: Some(out.stdout),
        }),
        Err(e) => Ok(AdbStatus {
            available: false,
            path,
            last_probe: Some(e.to_string()),
        }),
    }
}

#[derive(Serialize)]
pub struct InstallResult {
    pub ok: bool,
    pub path: Option<String>,
    pub message: String,
}

/// `install_adb` — download Google's platform-tools archive, extract into the
/// OS app-data dir, and swap the live driver so the next `list_devices` call
/// uses the freshly-installed binary. Matches v1's auto-download flow.
#[tauri::command]
pub async fn install_adb(state: State<'_, AppState>) -> Result<InstallResult, String> {
    match install_platform_tools().await {
        Ok(path) => {
            let new_driver: Arc<dyn AdbDriver> = Arc::new(SubprocessAdb::new(path.clone()));
            state.replace_adb(new_driver).await;
            Ok(InstallResult {
                ok: true,
                path: Some(path.display().to_string()),
                message: format!("Installed platform-tools to {}", path.display()),
            })
        }
        Err(e) => Ok(InstallResult {
            ok: false,
            path: None,
            message: e.to_string(),
        }),
    }
}

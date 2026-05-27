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

#[derive(Serialize)]
pub struct RestartResult {
    pub ok: bool,
    pub message: String,
}

/// `restart_adb` — `adb kill-server` then `adb start-server`. Matches v1's
/// Restart-AdbServer (main-menu shortcut A). Useful when the daemon wedges
/// after the device sleeps, when multiple adb versions collided, or after a
/// USB-cable swap. Does not redownload — for that use `install_adb`.
#[tauri::command]
pub async fn restart_adb(state: State<'_, AppState>) -> Result<RestartResult, String> {
    let adb = state.adb_snapshot().await;
    // kill-server can fail with no daemon running — that's fine, we still
    // care about the start-server result.
    let _ = adb.raw(&["kill-server"]).await;
    match adb.raw(&["start-server"]).await {
        Ok(out) => {
            // start-server prints "* daemon not running…" on first start;
            // that's success. Real failures contain "error" / "cannot".
            let s = format!("{}{}", out.stdout, out.stderr);
            let ok = !s.to_lowercase().contains("error") && !s.to_lowercase().contains("cannot");
            Ok(RestartResult {
                ok,
                message: if s.trim().is_empty() {
                    "ADB server restarted.".to_string()
                } else {
                    s
                },
            })
        }
        Err(e) => Ok(RestartResult {
            ok: false,
            message: e.to_string(),
        }),
    }
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

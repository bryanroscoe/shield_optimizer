use serde::Serialize;
use shield_optimizer_core::commands::{devices::ConnectResult, AppState};
use tauri::State;

use crate::wireless_adb::{DiscoveredAdbDevice, SharedWirelessAdb, WirelessStatus};

#[derive(Clone)]
pub struct MobileState {
    pub wireless: SharedWirelessAdb,
}

#[derive(Serialize)]
pub struct DiscoveryResult {
    pub devices: Vec<DiscoveredAdbDevice>,
    pub message: String,
}

#[tauri::command]
pub async fn wireless_discover(state: State<'_, MobileState>) -> Result<DiscoveryResult, String> {
    let devices = state.wireless.discover().await?;
    let message = if devices.is_empty() {
        "No wireless-debugging pairing services found. Enter the TV's IP and pairing port manually."
            .to_string()
    } else {
        format!("Found {} wireless-debugging service(s).", devices.len())
    };
    Ok(DiscoveryResult { devices, message })
}

#[tauri::command]
pub async fn wireless_pair(
    state: State<'_, MobileState>,
    host: String,
    port: u16,
    code: String,
) -> Result<ConnectResult, String> {
    match state.wireless.pair(host.trim(), port, code.trim()).await {
        Ok(message) => Ok(ConnectResult { ok: true, message }),
        Err(message) => Ok(ConnectResult { ok: false, message }),
    }
}

#[tauri::command]
pub async fn wireless_connect(
    state: State<'_, MobileState>,
    app_state: State<'_, AppState>,
    host: String,
    port: u16,
) -> Result<ConnectResult, String> {
    app_state.drop_all_remote_sessions().await;
    match state.wireless.connect(host.trim(), port).await {
        Ok(message) => Ok(ConnectResult { ok: true, message }),
        Err(message) => Ok(ConnectResult { ok: false, message }),
    }
}

#[tauri::command]
pub async fn wireless_disconnect(
    state: State<'_, MobileState>,
    app_state: State<'_, AppState>,
) -> Result<ConnectResult, String> {
    app_state.drop_all_remote_sessions().await;
    match state.wireless.disconnect().await {
        Ok(message) => Ok(ConnectResult { ok: true, message }),
        Err(message) => Ok(ConnectResult { ok: false, message }),
    }
}

/// `wireless_status` — cheap liveness probe the frontend polls to detect a
/// broken connection. Runs a fast `echo` over the cached connection; if it
/// fails the cached connection is cleared and `connected: false` is returned.
#[tauri::command]
pub async fn wireless_status(
    state: State<'_, MobileState>,
    app_state: State<'_, AppState>,
) -> Result<WirelessStatus, String> {
    let status = state.wireless.status().await;
    if !status.connected {
        app_state.drop_all_remote_sessions().await;
    }
    Ok(status)
}

#[tauri::command]
pub async fn find_remote(
    state: State<'_, shield_optimizer_core::commands::AppState>,
    serial: String,
) -> Result<ConnectResult, String> {
    let adb = state.adb_snapshot().await;
    let cmd = "am start -n com.nvidia.remotelocator/.ShieldRemoteLocatorActivity";
    match adb.shell(&serial, cmd).await {
        Ok(out) => Ok(ConnectResult {
            ok: true,
            message: out.stdout,
        }),
        Err(e) => {
            tracing::warn!(serial = %serial, error = %e, "find_remote shell failed");
            Ok(ConnectResult {
                ok: false,
                message: e.to_string(),
            })
        }
    }
}

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

use atv_optimizer_core::adb::AdbOutput;
use atv_optimizer_core::commands::{devices::ConnectResult, AppState};
use atv_optimizer_core::engine::cast_name::lookup_target;
use atv_optimizer_core::net::cast_display_name;
use serde::Serialize;
use tauri::State;
use tauri_plugin_atv_adb::AdbExt;

use atv_optimizer_core::engine::telemetry_diag::{discovery_diag, error_codes, Diag, DiagReason};

use crate::wireless_adb::{
    DiscoveredAdbDevice, SharedWirelessAdb, WirelessStatus, DISCOVER_TIMEOUT_MS,
};

#[derive(Clone)]
pub struct MobileState {
    pub wireless: SharedWirelessAdb,
}

#[derive(Serialize)]
pub struct DiscoveryResult {
    pub devices: Vec<DiscoveredAdbDevice>,
    pub message: String,
}

/// Discovery that found nothing is reported (no log excerpt on the phone):
/// how long it browsed and the error code if it failed outright.
#[tauri::command]
pub async fn wireless_discover(
    app: tauri::AppHandle,
    app_state: State<'_, AppState>,
) -> Result<DiscoveryResult, String> {
    let started = std::time::Instant::now();
    let result = tokio::task::spawn_blocking(move || app.adb().discover(DISCOVER_TIMEOUT_MS))
        .await
        .map_err(|e| format!("wireless discovery task failed: {e}"))
        .and_then(|result| result.map_err(|e| e.to_string()));
    let found = result.as_ref().map_or(0, Vec::len);
    if found == 0 && app_state.telemetry.is_active() {
        app_state.telemetry.record_diagnostic(
            DiagReason::DiscoverEmpty,
            None,
            None,
            Diag {
                discovery: Some(discovery_diag(DISCOVER_TIMEOUT_MS, 0)),
                errors: result
                    .as_ref()
                    .err()
                    .map(|e| error_codes([e.as_str()]))
                    .unwrap_or_default(),
                ..Default::default()
            }
            .with_ms(started.elapsed()),
        );
    }
    let mut devices = result?;
    add_cast_names(&mut devices).await;
    let message = if devices.is_empty() {
        "No wireless-debugging pairing services found. Enter the TV's IP and pairing port manually."
            .to_string()
    } else {
        format!("Found {} wireless-debugging service(s).", devices.len())
    };
    Ok(DiscoveryResult { devices, message })
}

const CAST_NAME_TIMEOUT: Duration = Duration::from_millis(1_200);
const CAST_NAMES_BUDGET: Duration = Duration::from_millis(1_500);

/// Below Android 14 discovery skips Cast adverts (see `discoveryServiceTypes` in
/// the plugin), so a TV advertising only `adb-<serial>` has no name to show. Its
/// Cast endpoint reports the name its owner gave it. Display only: a name never
/// identifies a TV, and one already advertised is never replaced.
async fn add_cast_names(devices: &mut [DiscoveredAdbDevice]) {
    let mut hosts: Vec<Ipv4Addr> = devices
        .iter()
        .filter(|device| device.friendly_name.is_none())
        .filter_map(|device| lookup_target(&device.host))
        .collect();
    hosts.sort_unstable();
    hosts.dedup();
    if hosts.is_empty() {
        return;
    }
    let mut lookups = tokio::task::JoinSet::new();
    for ip in hosts {
        lookups.spawn(async move {
            (
                ip,
                cast_display_name(IpAddr::V4(ip), CAST_NAME_TIMEOUT).await,
            )
        });
    }
    let mut names: HashMap<Ipv4Addr, String> = HashMap::new();
    let collect = async {
        while let Some(joined) = lookups.join_next().await {
            if let Ok((ip, Some(name))) = joined {
                names.insert(ip, name);
            }
        }
    };
    // Dropping the set at the end aborts any lookup still running.
    let _ = tokio::time::timeout(CAST_NAMES_BUDGET, collect).await;
    for device in devices
        .iter_mut()
        .filter(|device| device.friendly_name.is_none())
    {
        if let Some(name) = lookup_target(&device.host).and_then(|ip| names.get(&ip)) {
            device.friendly_name = Some(name.clone());
        }
    }
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
    request_id: u64,
) -> Result<ConnectResult, String> {
    state.wireless.begin_request(request_id)?;
    app_state.drop_all_remote_sessions().await;
    match state.wireless.connect(host.trim(), port, request_id).await {
        Ok(message) => Ok(ConnectResult { ok: true, message }),
        Err(message) => Ok(ConnectResult { ok: false, message }),
    }
}

#[tauri::command]
pub async fn wireless_disconnect(
    state: State<'_, MobileState>,
    app_state: State<'_, AppState>,
    request_id: u64,
) -> Result<ConnectResult, String> {
    state.wireless.begin_request(request_id)?;
    app_state.drop_all_remote_sessions().await;
    match state.wireless.disconnect(request_id).await {
        Ok(message) => Ok(ConnectResult { ok: true, message }),
        Err(message) => Ok(ConnectResult { ok: false, message }),
    }
}

#[tauri::command]
pub async fn wireless_cancel_connect(
    state: State<'_, MobileState>,
    request_id: u64,
    canceled_request_id: u64,
) -> Result<(), String> {
    state
        .wireless
        .cancel_connect(request_id, canceled_request_id)
        .await
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
    state: State<'_, atv_optimizer_core::commands::AppState>,
    serial: String,
) -> Result<ConnectResult, String> {
    let adb = state.adb_snapshot().await;
    let cmd = "am start -n com.nvidia.remotelocator/.ShieldRemoteLocatorActivity";
    match adb.shell(&serial, cmd).await {
        Ok(out) => Ok(classify_find_remote(out)),
        Err(e) => {
            tracing::warn!(serial = %serial, error = %e, "find_remote shell failed");
            Ok(ConnectResult {
                ok: false,
                message: e.to_string(),
            })
        }
    }
}

fn classify_find_remote(out: AdbOutput) -> ConnectResult {
    let message = out.combined().trim().to_string();
    let ok = out.success() && !out.shell_reported_failure();
    ConnectResult {
        ok,
        message: if ok && message.is_empty() {
            "Remote locator opened.".to_string()
        } else if !ok && message.is_empty() {
            format!("Remote locator failed with exit code {:?}.", out.exit_code)
        } else {
            message
        },
    }
}

#[cfg(test)]
mod tests {
    use super::classify_find_remote;
    use atv_optimizer_core::adb::AdbOutput;

    #[test]
    fn find_remote_requires_a_successful_shell_result() {
        let failed = classify_find_remote(AdbOutput {
            stdout: String::new(),
            stderr: "Error: Activity class does not exist".to_string(),
            exit_code: Some(1),
        });
        assert!(!failed.ok);
        assert!(failed.message.contains("does not exist"));

        let succeeded = classify_find_remote(AdbOutput {
            stdout: "Starting: Intent".to_string(),
            stderr: String::new(),
            exit_code: Some(0),
        });
        assert!(succeeded.ok);
    }
}

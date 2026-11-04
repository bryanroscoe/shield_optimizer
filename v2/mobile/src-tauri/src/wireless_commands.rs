use atv_optimizer_core::adb::AdbOutput;
use atv_optimizer_core::commands::{devices::ConnectResult, AppState};
use serde::Serialize;
use tauri::State;

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

fn discovered_adb_count(devices: &[DiscoveredAdbDevice]) -> usize {
    devices
        .iter()
        .filter(|device| {
            let service = device
                .service
                .trim()
                .trim_end_matches('.')
                .to_ascii_lowercase();
            let service = service.strip_suffix(".local").unwrap_or(&service);
            !device.host.trim().is_empty()
                && device.port != 0
                && matches!(
                    service,
                    "_adb._tcp" | "_adb-tls-connect._tcp" | "_adb-tls-pairing._tcp"
                )
        })
        .count()
}

/// Discovery without an ADB service is reported (no log excerpt on the phone):
/// how long it browsed and the error code if it failed outright.
#[tauri::command]
pub async fn wireless_discover(
    state: State<'_, MobileState>,
    app_state: State<'_, AppState>,
) -> Result<DiscoveryResult, String> {
    let started = std::time::Instant::now();
    let result = state.wireless.discover().await;
    let found = result
        .as_ref()
        .map_or(0, |devices| discovered_adb_count(devices));
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
    let devices = result?;
    let message = if devices.is_empty() {
        "No TV services found. Enter the TV's ADB details manually.".to_string()
    } else {
        format!("Received {} discovery service record(s).", devices.len())
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
    use super::{classify_find_remote, discovered_adb_count, DiscoveredAdbDevice};
    use atv_optimizer_core::adb::AdbOutput;

    #[test]
    fn advertised_names_do_not_hide_empty_adb_discovery() {
        let record = |service: &str, host: &str, port| DiscoveredAdbDevice {
            name: "Synthetic service".into(),
            friendly_name: Some("Synthetic TV".into()),
            network: Some("fixture".into()),
            host: host.into(),
            port,
            service: service.into(),
        };
        let mut devices = vec![
            record("_googlecast._tcp.", "192.0.2.42", 8009),
            record("_androidtvremote2._tcp.", "", 0),
            record("_adb-pretend._tcp.", "192.0.2.42", 5555),
        ];
        assert_eq!(discovered_adb_count(&devices), 0);
        for service in [
            "_adb._tcp.",
            "_adb-tls-connect._tcp.",
            "_ADB-TLS-PAIRING._TCP.local.",
        ] {
            devices.push(record(service, "192.0.2.42", 37123));
        }
        assert_eq!(discovered_adb_count(&devices), 3);
        devices.push(record("_adb._tcp.", "", 5555));
        devices.push(record("_adb._tcp.", "192.0.2.42", 0));
        assert_eq!(discovered_adb_count(&devices), 3);
    }

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

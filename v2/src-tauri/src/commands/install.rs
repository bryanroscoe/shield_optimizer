//! ADB install / status commands — let the UI auto-download Google's
//! platform-tools when no adb is present, matching v1's Check-Adb behavior.

use std::sync::Arc;

use serde::Serialize;
use tauri::State;

use crate::adb::driver::{cached_adb_binary, forget_cached_adb_binary};
use crate::adb::{install_platform_tools, parse_device_list, AdbDriver, SubprocessAdb};
use crate::engine::types::ConnectionType;

use super::AppState;
use shield_optimizer_core::commands::devices::{verify_connected_transport, TransportReadiness};

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
    let path = cached_adb_binary().map(|p| p.display().to_string());
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
    pub server_ready: bool,
    pub message: String,
}

/// How many times to try reconnecting one network device after a restart, and
/// how long to wait between tries (the wait grows with each attempt). Four
/// attempts spans about 1.8s, which covers the adbd socket-teardown window
/// without making a genuinely-absent device feel like a hang.
const RECONNECT_ATTEMPTS: u32 = 4;
const RECONNECT_BACKOFF: std::time::Duration = std::time::Duration::from_millis(300);

/// `restart_adb` — `adb kill-server` then `adb start-server`. Matches v1's
/// Restart-AdbServer (main-menu shortcut A). Useful when the daemon wedges
/// after the device sleeps, when multiple adb versions collided, or after a
/// USB-cable swap. Does not redownload — for that use `install_adb`.
#[tauri::command]
pub async fn restart_adb(state: State<'_, AppState>) -> Result<RestartResult, String> {
    restart_adb_impl(state.inner()).await
}

/// Reusable implementation — callable from tests against an `AppState` without
/// the `State<'_, T>` lifetime constraint.
async fn restart_adb_impl(state: &AppState) -> Result<RestartResult, String> {
    let adb = state.adb_snapshot().await;

    // kill-server drops every TCP connection. USB devices re-enumerate on
    // their own after the daemon restarts; network devices (ip:port) do not —
    // so capture them first and reconnect afterward. Without this, "Restart
    // ADB" silently disconnects the user's network device and the list comes
    // back empty even though the daemon restarted fine.
    let network_serials: Vec<String> = match adb.raw(&["devices"]).await {
        Ok(out) => parse_device_list(&out.stdout)
            .into_iter()
            .filter(|e| e.connection == ConnectionType::Network)
            .map(|e| e.serial)
            .collect(),
        Err(_) => Vec::new(),
    };

    // kill-server can fail with no daemon running — that's fine, we still
    // care about the start-server result.
    let _ = adb.raw(&["kill-server"]).await;
    let start_output = match adb.raw(&["start-server"]).await {
        Ok(out) if out.success() => out.combined(),
        Ok(out) => {
            return Ok(RestartResult {
                ok: false,
                server_ready: false,
                message: format!("ADB server failed to start: {}", out.combined().trim()),
            });
        }
        Err(error) => {
            return Ok(RestartResult {
                ok: false,
                server_ready: false,
                message: error.to_string(),
            });
        }
    };
    let lower = start_output.to_lowercase();
    if lower.contains("error") || lower.contains("cannot") || lower.contains("failed") {
        return Ok(RestartResult {
            ok: false,
            server_ready: false,
            message: start_output.trim().to_string(),
        });
    }

    // Reconnect the network devices we saw before the restart.
    //
    // Not on the first try. `start-server` returns as soon as the daemon has
    // forked, and the TV's adbd needs a moment to free the socket the
    // kill-server just tore down — connecting into that window fails with
    // "No route to host" even though the device is up and port 5555 is open.
    // Restarting by hand a second later then works, which is what makes this
    // read as "ADB is broken" rather than "too early". So give each device a
    // few attempts before calling it unreachable.
    let mut reconnected = Vec::new();
    let mut failed = Vec::new();
    let mut unauthorized = Vec::new();
    for serial in &network_serials {
        let mut connected = false;
        let mut last_failure = String::new();
        for attempt in 0..RECONNECT_ATTEMPTS {
            if attempt > 0 {
                tokio::time::sleep(RECONNECT_BACKOFF * attempt).await;
            }
            // ADB auto-attaches paired mDNS services; their transport keys are
            // not socket addresses and must never be passed to `adb connect`.
            if shield_optimizer_core::adb::is_network_endpoint(serial) {
                match adb.raw(&["connect", serial]).await {
                    Ok(out) => {
                        let response = out.combined();
                        let lower = response.to_lowercase();
                        if lower.contains("failed to authenticate") {
                            unauthorized.push(serial.clone());
                            break;
                        }
                        if !lower.contains("connected to")
                            || lower.contains("failed")
                            || lower.contains("cannot")
                        {
                            last_failure = response.trim().to_string();
                            continue;
                        }
                    }
                    Err(error) => {
                        last_failure = error.to_string();
                        let lower = last_failure.to_lowercase();
                        if lower.contains("failed to authenticate")
                            || lower.contains("device unauthorized")
                            || lower.contains("device is unauthorized")
                        {
                            unauthorized.push(serial.clone());
                            break;
                        }
                        continue;
                    }
                }
            }
            match verify_connected_transport(&*adb, serial).await {
                Ok(TransportReadiness::Ready) => connected = true,
                Ok(TransportReadiness::Unauthorized) => {
                    unauthorized.push(serial.clone());
                    break;
                }
                Err(reason) => last_failure = reason,
            }
            if connected {
                break;
            }
        }
        if connected {
            reconnected.push(serial.clone());
        } else if !unauthorized.contains(serial) {
            failed.push(format!("{serial}: {last_failure}"));
        }
    }
    let ok = failed.is_empty() && unauthorized.is_empty();

    let mut message = if start_output.trim().is_empty() {
        "ADB server restarted.".to_string()
    } else {
        start_output.trim().to_string()
    };
    if !reconnected.is_empty() {
        message.push_str(&format!("\nReconnected: {}", reconnected.join(", ")));
    }
    if !failed.is_empty() {
        message.push_str(&format!(
            "\nCould not reconnect after {} tries: {} — check the TV is awake and Network Debugging is still on, then try Scan LAN or Add by IP.",
            RECONNECT_ATTEMPTS,
            failed.join(", ")
        ));
    }

    if !unauthorized.is_empty() {
        message.push_str(&format!(
            "\nAwaiting authorization: {} — approve this computer on the TV, then Refresh or Ask again.",
            unauthorized.join(", ")
        ));
    }
    Ok(RestartResult {
        ok,
        server_ready: true,
        message,
    })
}

/// `install_adb` — download Google's platform-tools archive, extract into the
/// OS app-data dir, and swap the live driver so the next `list_devices` call
/// uses the freshly-installed binary. Matches v1's auto-download flow.
#[tauri::command]
pub async fn install_adb(state: State<'_, AppState>) -> Result<InstallResult, String> {
    install_with(state.inner(), install_platform_tools).await
}

pub(crate) async fn install_with<F, Fut>(
    state: &AppState,
    installer: F,
) -> Result<InstallResult, String>
where
    F: FnOnce(Arc<shield_optimizer_core::adb::Lease>) -> Fut,
    Fut: std::future::Future<Output = Result<std::path::PathBuf, crate::adb::InstallError>>,
{
    let operation = Arc::new(state.try_adb_operation().map_err(|e| e.to_string())?);
    match installer(Arc::clone(&operation)).await {
        Ok(path) => {
            // A managed install outranks everything discovery would otherwise
            // fall back to, so the memoized answer is now stale.
            forget_cached_adb_binary();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::test_support::{state_with, MockAdb};

    #[tokio::test(start_paused = true)]
    async fn restart_preserves_authorization_wait_without_repeated_connects() {
        for response in ["connect-output", "connect-error", "readiness"] {
            let serial = "192.0.2.1:5555";
            let mock = MockAdb::default()
                .on_raw("devices", &format!("{serial}\tunauthorized\n"))
                .on_raw("start-server", "daemon started");
            let mock = if response == "connect-error" {
                mock.on_raw_err("connect", "failed to authenticate to 192.0.2.1:5555")
            } else {
                mock.on_raw(
                    "connect",
                    if response == "readiness" {
                        "already connected to 192.0.2.1:5555"
                    } else {
                        "failed to authenticate to 192.0.2.1:5555"
                    },
                )
            }
            .on_raw_err("get-state", "device unauthorized");
            let calls = mock.raw_log();
            let result = restart_adb_impl(&state_with(mock)).await.unwrap();
            assert!(!result.ok);
            assert!(result.server_ready);
            assert!(result.message.contains("approve this computer on the TV"));
            assert!(!result.message.contains("Could not reconnect"));
            assert_eq!(
                calls
                    .lock()
                    .unwrap()
                    .iter()
                    .filter(|call| call.starts_with("connect "))
                    .count(),
                1
            );
        }
    }

    #[tokio::test(start_paused = true)]
    async fn restart_waits_for_mdns_reattachment_without_dialling_a_service_key() {
        let serial = "adb-fixture._adb-tls-connect._tcp";
        let mock = MockAdb::default()
            .on_raw("devices", &format!("{serial}\tdevice\n"))
            .on_raw("start-server", "daemon started")
            .on_raw_seq("get-state", &[Err("device not found"), Ok("device")]);
        let calls = mock.raw_log();
        let result = restart_adb_impl(&state_with(mock)).await.unwrap();
        assert!(result.ok, "{}", result.message);
        assert!(result.server_ready);
        let calls = calls.lock().unwrap();
        assert!(!calls.iter().any(|call| call.starts_with("connect ")));
        assert_eq!(
            calls
                .iter()
                .filter(|call| call.ends_with(" get-state"))
                .count(),
            2
        );
    }

    #[tokio::test]
    async fn failed_server_start_never_attempts_reconnection() {
        for nonzero in [false, true] {
            let mock = MockAdb::default().on_raw("devices", "192.0.2.1:5555\tdevice\n");
            let mock = if nonzero {
                mock.on_raw_exit("start-server", "daemon unavailable", 1)
            } else {
                mock.on_raw("start-server", "failed to start daemon")
            };
            let calls = mock.raw_log();
            let result = restart_adb_impl(&state_with(mock)).await.unwrap();
            assert!(!result.ok);
            assert!(!result.server_ready);
            assert!(!calls
                .lock()
                .unwrap()
                .iter()
                .any(|call| call.starts_with("connect ")));
        }
    }

    #[tokio::test(start_paused = true)]
    async fn restart_does_not_claim_reconnected_when_target_is_not_ready() {
        for status in ["offline", ""] {
            let serial = "192.168.1.50:5555";
            let mock = MockAdb::default()
                .on_raw("devices", &format!("{serial}\tdevice\n"))
                .on_raw("start-server", "daemon started")
                .on_raw("connect", &format!("already connected to {serial}"))
                .on_raw("get-state", status);
            let calls = mock.raw_log();
            let result = restart_adb_impl(&state_with(mock)).await.unwrap();
            assert!(!result.ok, "{status:?} is not a ready transport");
            assert!(!result.message.contains("Reconnected:"));
            assert_eq!(
                calls
                    .lock()
                    .unwrap()
                    .iter()
                    .filter(|c| c.starts_with("connect "))
                    .count(),
                4
            );
        }
    }

    #[tokio::test(start_paused = true)]
    async fn restart_waits_for_ready_state_instead_of_connect_text() {
        let serial = "192.168.1.50:5555";
        let mock = MockAdb::default()
            .on_raw("devices", &format!("{serial}\tdevice\n"))
            .on_raw("start-server", "daemon started")
            .on_raw("connect", &format!("already connected to {serial}"))
            .on_raw_seq("get-state", &[Ok("offline"), Ok("device")]);
        let calls = mock.raw_log();
        let result = restart_adb_impl(&state_with(mock)).await.unwrap();
        assert!(result.ok, "{}", result.message);
        assert!(result.message.contains("Reconnected:"));
        assert_eq!(
            calls
                .lock()
                .unwrap()
                .iter()
                .filter(|c| c.ends_with(" get-state"))
                .count(),
            2
        );
    }

    #[tokio::test(start_paused = true)]
    async fn restart_adb_reports_failure_when_network_reconnect_fails() {
        // Daemon restarts fine, but the previously-connected network device
        // can't be reconnected — that's a user-visible failure, and the message
        // must name the device and point at the recovery action.
        let serial = "192.168.1.50:5555";
        let mock = MockAdb::default()
            .on_raw(
                "devices",
                &format!("List of devices attached\n{serial}\tdevice\n"),
            )
            .on_raw("start-server", "* daemon started successfully")
            .on_raw_err("connect", "connection refused");
        let state = state_with(mock);

        let res = restart_adb_impl(&state).await.unwrap();

        assert!(!res.ok);
        assert!(
            res.server_ready,
            "a stale TV does not mean the daemon failed"
        );
        assert!(
            res.message.contains("Could not reconnect") && res.message.contains(serial),
            "message should name the unreconnected device: {}",
            res.message
        );
    }

    #[tokio::test(start_paused = true)]
    async fn restart_adb_retries_a_reconnect_that_fails_immediately() {
        // The daemon has only just forked and the TV's adbd is still tearing
        // down the socket kill-server closed, so the first connect comes back
        // "No route to host" for a device that is plainly up. Reporting that as
        // unreachable is what makes a working setup look broken.
        let serial = "192.168.42.196:5555";
        let mock = MockAdb::default()
            .on_raw(
                "devices",
                &format!("List of devices attached\n{serial}\tdevice\n"),
            )
            .on_raw("start-server", "* daemon started successfully")
            .on_raw_seq(
                "connect",
                &[
                    Err("failed to connect to '192.168.42.196:5555': No route to host"),
                    Ok("connected to 192.168.42.196:5555"),
                ],
            )
            .on_raw("get-state", "device");
        let state = state_with(mock);

        let res = restart_adb_impl(&state).await.unwrap();

        assert!(res.ok, "a retried reconnect is a success: {}", res.message);
        assert!(
            res.message.contains("Reconnected") && res.message.contains(serial),
            "message should name the device it got back: {}",
            res.message
        );
        assert!(
            !res.message.contains("Could not reconnect"),
            "a device that came back must not also be listed as lost: {}",
            res.message
        );
    }

    #[tokio::test]
    async fn restart_adb_reports_failure_when_start_server_errors() {
        // No network devices to reconnect, but start-server itself fails — the
        // result must be a failure carrying the daemon's error text.
        let mock = MockAdb::default().on_raw_err("start-server", "cannot bind to port 5037");
        let state = state_with(mock);

        let res = restart_adb_impl(&state).await.unwrap();

        assert!(!res.ok);
        assert!(!res.server_ready);
        assert!(
            res.message.contains("cannot bind to port 5037"),
            "message should surface the daemon error: {}",
            res.message
        );
    }
}

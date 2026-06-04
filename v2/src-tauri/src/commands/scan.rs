//! Network-scan command — matches v1's `Scan-Network` UX.

use std::time::Duration;

use serde::Serialize;
use tauri::State;

use crate::adb::{local_subnet_prefix, scan_subnet, AdbDriver};

use super::AppState;

#[derive(Serialize)]
pub struct ScanResult {
    /// First three octets of the scanned /24 (e.g. "192.168.42"), or `null`
    /// if the gateway couldn't be detected.
    pub subnet: Option<String>,
    /// IPs that answered on the ADB port.
    pub found: Vec<String>,
    /// IPs that `adb connect` succeeded against.
    pub connected: Vec<String>,
    /// IPs the daemon reached but that haven't authorized this computer's ADB
    /// key — the device shows an "Allow USB debugging?" prompt and registers
    /// as `unauthorized` in the device list.
    pub unauthorized: Vec<String>,
    /// IPs that responded to the port probe but `adb connect` failed.
    pub failed: Vec<String>,
    /// Human-readable summary line — useful diagnostic for the UI.
    pub message: String,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum ConnectOutcome {
    Connected,
    /// Device reachable but this host's ADB key isn't approved yet. The
    /// device is still added to `adb devices` (as `unauthorized`), so this
    /// is "waiting on the user", not a failure — and retrying won't help.
    Unauthorized,
    Failed,
}

/// Classify `adb connect <target>` output. Detection is text-based on the
/// combined streams rather than the exit code, since `adb connect` exits 0
/// even on "failed to authenticate" / "failed to connect" with current
/// platform-tools, and exit-code conventions vary across versions.
fn classify_connect_output(combined: &str) -> ConnectOutcome {
    let s = combined.to_lowercase();
    if s.contains("failed to authenticate") {
        ConnectOutcome::Unauthorized
    } else if s.contains("connected to") && !s.contains("failed") && !s.contains("cannot") {
        ConnectOutcome::Connected
    } else {
        ConnectOutcome::Failed
    }
}

/// A nonzero exit surfaces as `Err` from the driver and counts as failed.
async fn adb_connect(adb: &dyn AdbDriver, target: &str) -> ConnectOutcome {
    match adb.raw(&["connect", target]).await {
        Ok(out) => classify_connect_output(&format!("{}\n{}", out.stdout, out.stderr)),
        Err(_) => ConnectOutcome::Failed,
    }
}

/// `scan_network` — sweep the local /24 for ADB-listening devices and try
/// `adb connect` against each responder. Returns a structured summary so the
/// UI can render counts and any per-IP failures.
#[tauri::command]
pub async fn scan_network(state: State<'_, AppState>) -> Result<ScanResult, String> {
    let Some(prefix) = local_subnet_prefix().await else {
        return Ok(ScanResult {
            subnet: None,
            found: vec![],
            connected: vec![],
            unauthorized: vec![],
            failed: vec![],
            message: "Could not detect default gateway. Set SHIELD_OPTIMIZER_SUBNET=\"a.b.c\" \
                      to override, or use Connect IP."
                .to_string(),
        });
    };
    let subnet_label = format!("{}.{}.{}", prefix[0], prefix[1], prefix[2]);

    let hits = scan_subnet(prefix).await;
    let found: Vec<String> = hits.iter().map(|h| h.ip.clone()).collect();

    let adb = state.adb_snapshot().await;

    // Warm the adb daemon before connecting. The port sweep just opened and
    // dropped raw TCP sockets against each device's adbd; firing `adb connect`
    // immediately afterward — especially against a cold daemon — tends to get
    // a transient refusal, which is why a manual "Restart ADB" (which starts
    // the daemon) made the same devices connect. Starting the server here, plus
    // a single retry below, makes the scan connect on its own.
    let _ = adb.raw(&["start-server"]).await;

    let mut connected = Vec::new();
    let mut unauthorized = Vec::new();
    let mut failed = Vec::new();
    for hit in &hits {
        let target = format!("{}:5555", hit.ip);
        let mut outcome = adb_connect(adb.as_ref(), &target).await;
        // Only a hard failure is worth retrying — "unauthorized" means the
        // device is waiting for the user to approve the prompt on-screen.
        if outcome == ConnectOutcome::Failed {
            tokio::time::sleep(Duration::from_millis(400)).await;
            outcome = adb_connect(adb.as_ref(), &target).await;
        }
        match outcome {
            ConnectOutcome::Connected => connected.push(hit.ip.clone()),
            ConnectOutcome::Unauthorized => unauthorized.push(hit.ip.clone()),
            ConnectOutcome::Failed => failed.push(hit.ip.clone()),
        }
    }

    let message = summary_message(&subnet_label, hits.len(), &connected, &unauthorized);

    Ok(ScanResult {
        subnet: Some(subnet_label),
        found,
        connected,
        unauthorized,
        failed,
        message,
    })
}

fn summary_message(
    subnet_label: &str,
    found: usize,
    connected: &[String],
    unauthorized: &[String],
) -> String {
    if found == 0 {
        return format!(
            "No devices on {subnet_label}.x answered on the ADB port. Make sure Network \
             Debugging is enabled on your TV, or use Connect IP for newer Google TVs that \
             need PIN pairing first."
        );
    }
    let mut message = format!(
        "Scanned {subnet_label}.x — found {found} device{}, connected {}.",
        if found == 1 { "" } else { "s" },
        connected.len()
    );
    if !unauthorized.is_empty() {
        message.push_str(&format!(
            " {} need{} authorization — accept the \"Allow USB debugging?\" prompt on the \
             TV, then Refresh.",
            unauthorized.len(),
            if unauthorized.len() == 1 { "s" } else { "" }
        ));
    }
    message
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn classifies_fresh_and_already_connected() {
        assert_eq!(
            classify_connect_output("connected to 192.168.42.71:5555"),
            ConnectOutcome::Connected
        );
        assert_eq!(
            classify_connect_output("already connected to 192.168.42.71:5555"),
            ConnectOutcome::Connected
        );
    }

    #[test]
    fn classifies_connected_despite_daemon_startup_noise() {
        let out = "* daemon not running; starting now at tcp:5037\n\
                   * daemon started successfully\nconnected to 192.168.42.71:5555";
        assert_eq!(classify_connect_output(out), ConnectOutcome::Connected);
    }

    #[test]
    fn classifies_unauthorized() {
        // Real output from platform-tools 37.0.0 against a TV that hasn't
        // approved this host's key — exits 0, device lands in `adb devices`
        // as `unauthorized`.
        assert_eq!(
            classify_connect_output("failed to authenticate to 192.168.42.143:5555"),
            ConnectOutcome::Unauthorized
        );
    }

    #[test]
    fn classifies_failures() {
        assert_eq!(
            classify_connect_output("failed to connect to '192.168.42.9:5555': Connection refused"),
            ConnectOutcome::Failed
        );
        assert_eq!(
            classify_connect_output("cannot connect to 192.168.42.9:5555: timeout"),
            ConnectOutcome::Failed
        );
        assert_eq!(classify_connect_output(""), ConnectOutcome::Failed);
    }

    #[test]
    fn summary_mentions_unauthorized_devices() {
        let msg = summary_message(
            "192.168.42",
            4,
            &[],
            &["192.168.42.143".into(), "192.168.42.25".into()],
        );
        assert_eq!(
            msg,
            "Scanned 192.168.42.x — found 4 devices, connected 0. 2 need authorization — \
             accept the \"Allow USB debugging?\" prompt on the TV, then Refresh."
        );
    }

    #[test]
    fn summary_plain_when_all_connected() {
        let msg = summary_message("10.0.0", 1, &["10.0.0.5".into()], &[]);
        assert_eq!(msg, "Scanned 10.0.0.x — found 1 device, connected 1.");
    }
}

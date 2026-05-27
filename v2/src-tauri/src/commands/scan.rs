//! Network-scan command — matches v1's `Scan-Network` UX.

use serde::Serialize;
use tauri::State;

use crate::adb::{local_subnet_prefix, scan_subnet};

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
    /// IPs that responded to the port probe but `adb connect` failed.
    pub failed: Vec<String>,
    /// Human-readable summary line — useful diagnostic for the UI.
    pub message: String,
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
    let mut connected = Vec::new();
    let mut failed = Vec::new();
    for hit in &hits {
        let target = format!("{}:5555", hit.ip);
        match adb.raw(&["connect", &target]).await {
            Ok(out) if out.success() && !out.stdout.contains("failed") => {
                connected.push(hit.ip.clone());
            }
            _ => failed.push(hit.ip.clone()),
        }
    }

    let message = if hits.is_empty() {
        format!(
            "No devices on {subnet_label}.x answered on the ADB port. Make sure Network \
             Debugging is enabled on your TV, or use Connect IP for newer Google TVs that \
             need PIN pairing first."
        )
    } else {
        format!(
            "Scanned {subnet_label}.x — found {} device{}, connected {}.",
            hits.len(),
            if hits.len() == 1 { "" } else { "s" },
            connected.len()
        )
    };

    Ok(ScanResult {
        subnet: Some(subnet_label),
        found,
        connected,
        failed,
        message,
    })
}

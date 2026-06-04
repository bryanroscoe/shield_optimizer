//! Recovery — re-enable every disabled package in one shot. v1's
//! Run-PanicRecovery: the safety net users hit when they've broken something.

use serde::Serialize;
use tauri::State;

use crate::adb::parse_disabled_packages_output;

use super::AppState;

#[derive(Serialize)]
pub struct RecoveryResult {
    pub restored: Vec<String>,
    pub failed: Vec<RecoveryFailure>,
    pub message: String,
}

#[derive(Serialize)]
pub struct RecoveryFailure {
    pub package: String,
    pub error: String,
}

/// `panic_recovery` — `pm enable` every package in `pm list packages -d`.
/// Returns counts of restored vs. failed. Matches v1's Run-PanicRecovery
/// (§12 of FEATURES.md).
#[tauri::command]
pub async fn panic_recovery(
    state: State<'_, AppState>,
    serial: String,
) -> Result<RecoveryResult, String> {
    let adb = state.adb_snapshot().await;
    let disabled_out = adb
        .shell(&serial, "pm list packages -d")
        .await
        .map_err(|e| format!("pm list packages -d: {e}"))?;
    let disabled = parse_disabled_packages_output(&disabled_out.stdout);

    if disabled.is_empty() {
        return Ok(RecoveryResult {
            restored: vec![],
            failed: vec![],
            message: "No disabled packages — nothing to restore.".to_string(),
        });
    }

    let mut restored = Vec::new();
    let mut failed = Vec::new();

    for pkg in &disabled {
        match adb.shell(&serial, &format!("pm enable {pkg}")).await {
            Ok(out) if !out.shell_reported_failure() => {
                restored.push(pkg.clone());
            }
            Ok(out) => failed.push(RecoveryFailure {
                package: pkg.clone(),
                error: out.combined().trim().to_string(),
            }),
            Err(e) => failed.push(RecoveryFailure {
                package: pkg.clone(),
                error: e.to_string(),
            }),
        }
    }

    let message = format!(
        "Restored {}/{} packages.{}",
        restored.len(),
        disabled.len(),
        if !failed.is_empty() {
            format!(" {} failed.", failed.len())
        } else {
            String::new()
        }
    );

    Ok(RecoveryResult {
        restored,
        failed,
        message,
    })
}

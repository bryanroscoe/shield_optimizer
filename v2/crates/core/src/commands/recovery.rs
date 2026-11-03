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
    panic_recovery_impl(state.inner(), &serial).await
}

/// Public so the mobile billing tests can prove recovery stays free after a refund.
pub async fn panic_recovery_impl(state: &AppState, serial: &str) -> Result<RecoveryResult, String> {
    let adb = state.adb_snapshot().await;
    let disabled_out = adb
        .shell(serial, "pm list packages -d --user 0")
        .await
        .map_err(|e| format!("pm list packages -d: {e}"))?;
    if !super::verification::succeeded(&disabled_out) {
        return Err(format!(
            "Read disabled packages failed: {}",
            disabled_out.combined().trim()
        ));
    }
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
        if !crate::engine::is_valid_package_name(pkg) {
            failed.push(RecoveryFailure {
                package: pkg.clone(),
                error: "invalid package name".to_string(),
            });
            continue;
        }
        match adb
            .shell(serial, &format!("pm enable --user 0 {pkg}"))
            .await
        {
            Ok(out) if super::verification::succeeded(&out) => {
                match super::verification::verify_package(adb.as_ref(), serial, pkg, true).await {
                    Ok(()) => restored.push(pkg.clone()),
                    Err(error) => failed.push(RecoveryFailure {
                        package: pkg.clone(),
                        error,
                    }),
                }
            }
            Ok(out) => failed.push(RecoveryFailure {
                package: pkg.clone(),
                error: format!("exit {:?}: {}", out.exit_code, out.combined().trim()),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::test_support::{state_with, MockAdb};
    use crate::license::Entitlement;

    #[tokio::test]
    async fn free_user_can_recover_disabled_packages_after_downgrade() {
        let mock = MockAdb::default()
            .on_shell(
                "pm list packages -d --user 0",
                "package:com.example.one\npackage:com.example.two\n",
            )
            .on_shell(
                "pm list packages -e --user 0 com.example.one",
                &format!(
                    "package:com.example.one\n{}0\n",
                    crate::adb::batch::BATCH_STATUS
                ),
            )
            .on_shell(
                "pm list packages -e --user 0 com.example.two",
                &format!(
                    "package:com.example.two\n{}0\n",
                    crate::adb::batch::BATCH_STATUS
                ),
            );
        let log = mock.shell_log();
        let state = state_with(mock).with_entitlement(Entitlement::Pro);
        state.set_entitlement(Entitlement::Free);
        let result = panic_recovery_impl(&state, "TV").await.unwrap();
        assert_eq!(result.restored, vec!["com.example.one", "com.example.two"]);
        assert!(result.failed.is_empty());
        let calls = log.lock().unwrap();
        assert!(calls
            .iter()
            .any(|x| x == "pm enable --user 0 com.example.one"));
        assert!(calls
            .iter()
            .any(|x| x == "pm enable --user 0 com.example.two"));
        assert!(!calls
            .iter()
            .any(|x| x.contains("disable-user") || x.contains("uninstall")));
    }
    #[tokio::test]
    async fn recovery_nonzero_and_unconfirmed_enable_are_not_restored() {
        for mock in [
            MockAdb::default()
                .on_shell("pm list packages -d --user 0", "package:com.example.app")
                .on_shell_exit("pm enable --user 0", "", 1),
            MockAdb::default()
                .on_shell("pm list packages -d --user 0", "package:com.example.app")
                .on_shell(
                    "pm list packages -e --user 0",
                    &format!("\n{}0\n", crate::adb::batch::BATCH_STATUS),
                ),
        ] {
            let result = panic_recovery_impl(&state_with(mock), "TV").await.unwrap();
            assert!(result.restored.is_empty());
            assert_eq!(result.failed.len(), 1);
        }
    }
}

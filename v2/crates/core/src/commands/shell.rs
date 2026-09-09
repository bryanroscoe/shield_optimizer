//! Free-form ADB shell runner.
//!
//! The catch-all escape hatch: anything the curated UI doesn't cover, the user
//! can type here.
//!
//! Every other disable path in the app goes through a typed command that
//! consults `engine::safety`. This one cannot, so it calls
//! `engine::safety::shell_command_blocked` before anything reaches the device
//! — but see that function's docs before trusting it: the check is an
//! anti-footgun that catches accidental destructive commands, **not** a
//! boundary that holds against someone trying to get around it. It is not one,
//! and does not need to be: the user running this app already has `adb shell`
//! against the same device.

use serde::Serialize;
use tauri::State;

use crate::engine::shell_command_blocked;
use crate::license::Feature;

use super::AppState;

/// Result of one shell invocation. Both streams are surfaced because on-device
/// tools (`pm`, `settings`, `cmd`) split their output across them
/// inconsistently — showing only stdout hides half the failures.
#[derive(Debug, Serialize)]
pub struct ShellRunResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    /// The safety gate refused to run this. Nothing was sent to the device.
    pub blocked: bool,
    /// Why it was refused, when `blocked`.
    pub blocked_reason: Option<String>,
}

/// Cap on returned output. A stray `logcat` or `dumpsys` with no filter can
/// emit tens of megabytes; the frontend has to render whatever comes back, so
/// the truncation happens here rather than after it has crossed the IPC
/// boundary and blown up the webview.
const MAX_OUTPUT_BYTES: usize = 256 * 1024;

/// `run_shell` — run an arbitrary command on the device.
#[tauri::command]
pub async fn run_shell(
    state: State<'_, AppState>,
    serial: String,
    command: String,
) -> Result<ShellRunResult, String> {
    state.require_pro(Feature::ShellRunner)?;
    run_shell_impl(state.inner(), &serial, &command).await
}

async fn run_shell_impl(
    state: &AppState,
    serial: &str,
    command: &str,
) -> Result<ShellRunResult, String> {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return Err("Enter a command to run.".into());
    }

    if let Some((package, reason)) = shell_command_blocked(trimmed) {
        return Ok(ShellRunResult {
            stdout: String::new(),
            stderr: String::new(),
            exit_code: None,
            blocked: true,
            blocked_reason: Some(format!(
                "Refused: this command would disable or remove {package}, which is on the \
                 do-not-disable list. {reason}"
            )),
        });
    }

    let adb = state.adb_snapshot().await;
    let out = adb
        .shell(serial, trimmed)
        .await
        .map_err(|e| format!("{trimmed}: {e}"))?;

    Ok(ShellRunResult {
        stdout: truncate(out.stdout),
        stderr: truncate(out.stderr),
        exit_code: out.exit_code,
        blocked: false,
        blocked_reason: None,
    })
}

/// Trim to [`MAX_OUTPUT_BYTES`] on a char boundary, with a visible marker so a
/// truncated dump is never mistaken for the whole thing.
fn truncate(mut s: String) -> String {
    if s.len() <= MAX_OUTPUT_BYTES {
        return s;
    }
    let mut cut = MAX_OUTPUT_BYTES;
    while cut > 0 && !s.is_char_boundary(cut) {
        cut -= 1;
    }
    s.truncate(cut);
    s.push_str("\n… output truncated at 256 KB …");
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::test_support::{state_with, MockAdb};

    #[tokio::test]
    async fn runs_a_plain_command_and_returns_both_streams() {
        let mock = MockAdb::default().on_shell("getprop ro.product.model", "SHIELD Android TV\n");
        let log = mock.shell_log();
        let state = state_with(mock);

        let r = run_shell_impl(&state, "serial", "getprop ro.product.model")
            .await
            .unwrap();
        assert!(!r.blocked);
        assert_eq!(r.stdout.trim(), "SHIELD Android TV");
        assert_eq!(log.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_blocked_command_never_reaches_the_device() {
        // The invariant this module exists to hold: refusal happens before the
        // driver is touched, so an empty shell log is the assertion that
        // matters, not just the flag on the result.
        let mock = MockAdb::default();
        let log = mock.shell_log();
        let state = state_with(mock);

        let r = run_shell_impl(
            &state,
            "serial",
            "pm disable-user --user 0 com.android.systemui",
        )
        .await
        .unwrap();
        assert!(r.blocked);
        assert!(r
            .blocked_reason
            .as_ref()
            .unwrap()
            .contains("com.android.systemui"));
        assert!(r
            .blocked_reason
            .as_ref()
            .unwrap()
            .contains("do-not-disable"));
        assert!(
            log.lock().unwrap().is_empty(),
            "a refused command must not be sent to the device"
        );
    }

    #[tokio::test]
    async fn a_chained_command_hiding_a_disable_is_still_refused_whole() {
        // The harmless first statement must not buy the destructive one a ride.
        let mock = MockAdb::default();
        let log = mock.shell_log();
        let state = state_with(mock);

        let r = run_shell_impl(&state, "serial", "echo hi; pm uninstall com.android.shell")
            .await
            .unwrap();
        assert!(r.blocked);
        assert!(log.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn disabling_an_unprotected_package_is_allowed_through() {
        let mock = MockAdb::default()
            .on_shell("pm disable-user", "Package ... new state: disabled-user\n");
        let state = state_with(mock);

        let r = run_shell_impl(
            &state,
            "serial",
            "pm disable-user --user 0 com.facebook.katana",
        )
        .await
        .unwrap();
        assert!(!r.blocked);
        assert!(r.stdout.contains("disabled-user"));
    }

    #[tokio::test]
    async fn an_empty_command_is_rejected_before_the_driver() {
        let mock = MockAdb::default();
        let log = mock.shell_log();
        let state = state_with(mock);

        assert!(run_shell_impl(&state, "serial", "   ").await.is_err());
        assert!(log.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_transport_failure_surfaces_as_an_error_not_an_empty_result() {
        let state = state_with(MockAdb::default().on_shell_err("id", "device offline"));
        let err = run_shell_impl(&state, "serial", "id").await.unwrap_err();
        assert!(err.contains("device offline"));
    }

    #[test]
    fn oversized_output_is_truncated_on_a_char_boundary_and_marked() {
        // Multi-byte chars straddling the cut would panic a naive truncate.
        let huge = "é".repeat(MAX_OUTPUT_BYTES);
        let out = truncate(huge);
        assert!(out.len() < MAX_OUTPUT_BYTES + 64);
        assert!(out.ends_with("… output truncated at 256 KB …"));
    }

    #[test]
    fn output_under_the_cap_is_returned_untouched() {
        assert_eq!(truncate("short".into()), "short");
    }
}

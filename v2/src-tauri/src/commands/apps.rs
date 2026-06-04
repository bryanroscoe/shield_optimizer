//! Per-package action commands — disable / enable / uninstall.
//!
//! Honors the reversibility model (architectural commitment #7): disable is
//! a `pm disable-user --user 0` (reversible via enable), uninstall is a
//! `pm uninstall --user 0` (semi-reversible via `cmd package install-existing`
//! or Play Store).

use std::collections::{HashMap, HashSet};

use serde::Serialize;
use tauri::State;

use crate::adb::{parse_disabled_packages_output, parse_installed_packages_output};
use crate::engine::{classify_safety, is_valid_package_name, Safety};

use super::AppState;

/// Reject malformed package names before they're interpolated into a shell
/// command. Packages from `pm list` are well-formed, but the custom-launcher
/// and any manual-entry path are user-controlled — this keeps a stray value
/// from injecting shell syntax into `adb shell`. Returns an error result to
/// surface verbatim if the name is invalid.
fn reject_invalid_package(package: &str) -> Option<ActionResult> {
    if is_valid_package_name(package) {
        return None;
    }
    Some(ActionResult {
        ok: false,
        message: format!("Refusing to act on invalid package name: {package:?}"),
    })
}

/// `safety_info` — pure lookup the frontend uses to decide between "show a
/// loud confirm", "show a hard block badge", and "no extra ceremony".
/// Cheap; doesn't touch the device.
#[tauri::command]
pub fn safety_info(package: String) -> Safety {
    classify_safety(&package)
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PackageState {
    Enabled,
    Disabled,
    Missing,
}

/// `package_states` — query the device for the current state of each package
/// in `packages`. Two shell calls in parallel (`pm list packages` and
/// `pm list packages -d`), then categorize.
#[tauri::command]
pub async fn package_states(
    state: State<'_, AppState>,
    serial: String,
    packages: Vec<String>,
) -> Result<HashMap<String, PackageState>, String> {
    let adb = state.adb_snapshot().await;
    let (installed_res, disabled_res) = tokio::join!(
        adb.shell(&serial, "pm list packages"),
        adb.shell(&serial, "pm list packages -d"),
    );
    let installed = installed_res.map_err(|e| format!("pm list packages: {e}"))?;
    let disabled = disabled_res.map_err(|e| format!("pm list packages -d: {e}"))?;

    let installed_set: HashSet<String> = parse_installed_packages_output(&installed.stdout)
        .into_iter()
        .collect();
    let disabled_set: HashSet<String> = parse_disabled_packages_output(&disabled.stdout)
        .into_iter()
        .collect();

    let mut out = HashMap::with_capacity(packages.len());
    for pkg in packages {
        let s = if disabled_set.contains(&pkg) {
            PackageState::Disabled
        } else if installed_set.contains(&pkg) {
            PackageState::Enabled
        } else {
            PackageState::Missing
        };
        out.insert(pkg, s);
    }
    Ok(out)
}

#[derive(Serialize)]
pub struct ActionResult {
    pub ok: bool,
    /// `pm` stdout/stderr — surfaced to the UI so the user can see the actual
    /// error message when something fails (e.g. "Failure [DELETE_FAILED_…]").
    pub message: String,
}

/// `disable_package` — `pm disable-user --user 0 <pkg>`. Reversible.
///
/// Refuses outright if `package` is on the engine's NEVER_DISABLE list —
/// these would brick the device or break ADB. The user can't override this
/// from the UI; they'd have to use `adb shell` directly.
#[tauri::command]
pub async fn disable_package(
    state: State<'_, AppState>,
    serial: String,
    package: String,
) -> Result<ActionResult, String> {
    if let Some(rejection) = reject_invalid_package(&package) {
        return Ok(rejection);
    }
    if let Safety::NeverDisable { reason } = classify_safety(&package) {
        return Ok(ActionResult {
            ok: false,
            message: format!("Refusing to disable {package}: {reason}"),
        });
    }
    run(
        &state,
        &serial,
        &format!("pm disable-user --user 0 {package}"),
    )
    .await
}

/// `enable_package` — `pm enable <pkg>`. Reverses a previous disable.
#[tauri::command]
pub async fn enable_package(
    state: State<'_, AppState>,
    serial: String,
    package: String,
) -> Result<ActionResult, String> {
    if let Some(rejection) = reject_invalid_package(&package) {
        return Ok(rejection);
    }
    run(&state, &serial, &format!("pm enable {package}")).await
}

/// Decode a `pm uninstall` failure into a user-readable hint. Mirrors v1's
/// `Get-UninstallErrorReason` (§16.6). Returns `None` when nothing matches
/// so the caller can fall back to the raw output.
pub fn decode_uninstall_error(stdout: &str) -> Option<&'static str> {
    if stdout.contains("Broken pipe") {
        return Some("Protected system app — cannot be removed. Try Disable instead.");
    }
    if stdout.contains("not installed for") {
        return Some("App not installed for this user.");
    }
    if stdout.contains("DELETE_FAILED_INTERNAL_ERROR") {
        return Some("Internal error — the app may be running. Reboot the device and retry.");
    }
    if stdout.contains("DELETE_FAILED_DEVICE_POLICY_MANAGER") {
        return Some("Blocked by device policy manager (work profile / admin).");
    }
    if stdout.contains("DELETE_FAILED_OWNER_BLOCKED") {
        return Some("Blocked — package is owned by another user or profile.");
    }
    None
}

/// `uninstall_package` — `pm uninstall --user 0 <pkg>`. Semi-reversible via
/// `cmd package install-existing` (if the APK is still on /system) or the
/// Play Store.
///
/// Same NEVER_DISABLE refusal as `disable_package` — uninstalling these has
/// the same brick risk as disabling them, and is less reversible.
#[tauri::command]
pub async fn uninstall_package(
    state: State<'_, AppState>,
    serial: String,
    package: String,
) -> Result<ActionResult, String> {
    if let Some(rejection) = reject_invalid_package(&package) {
        return Ok(rejection);
    }
    if let Safety::NeverDisable { reason } = classify_safety(&package) {
        return Ok(ActionResult {
            ok: false,
            message: format!("Refusing to uninstall {package}: {reason}"),
        });
    }
    let mut result = run(&state, &serial, &format!("pm uninstall --user 0 {package}")).await?;
    if !result.ok {
        if let Some(hint) = decode_uninstall_error(&result.message) {
            result.message = format!("{}\n→ {hint}", result.message.trim());
        }
    }
    Ok(result)
}

/// `reinstall_existing` — `cmd package install-existing <pkg>`. Brings back a
/// previously-uninstalled app from /system without a Play Store fetch.
#[tauri::command]
pub async fn reinstall_existing(
    state: State<'_, AppState>,
    serial: String,
    package: String,
) -> Result<ActionResult, String> {
    if let Some(rejection) = reject_invalid_package(&package) {
        return Ok(rejection);
    }
    run(
        &state,
        &serial,
        &format!("cmd package install-existing {package}"),
    )
    .await
}

/// `open_play_store` — launch the Play Store detail page for `package` on the
/// device. Use when an app was fully uninstalled and isn't available via
/// `install-existing` (third-party apps, or system apps wiped from /data).
///
/// Reject package strings containing shell metacharacters since the value is
/// interpolated into a URL passed to `am start`. Real package names are
/// `[a-zA-Z0-9_.]` only, so this is more than permissive enough.
#[tauri::command]
pub async fn open_play_store(
    state: State<'_, AppState>,
    serial: String,
    package: String,
) -> Result<ActionResult, String> {
    if package.is_empty()
        || package
            .chars()
            .any(|c| !(c.is_ascii_alphanumeric() || c == '.' || c == '_'))
    {
        return Ok(ActionResult {
            ok: false,
            message: format!("invalid package name: {package}"),
        });
    }
    run(
        &state,
        &serial,
        &format!("am start -a android.intent.action.VIEW -d market://details?id={package}"),
    )
    .await
}

async fn run(state: &AppState, serial: &str, cmd: &str) -> Result<ActionResult, String> {
    let adb = state.adb_snapshot().await;
    let out = adb
        .shell(serial, cmd)
        .await
        .map_err(|e| format!("{cmd}: {e}"))?;
    let message = if !out.stdout.trim().is_empty() {
        out.stdout
    } else if !out.stderr.trim().is_empty() {
        out.stderr
    } else {
        "(no output)".to_string()
    };
    // pm's exit codes are unreliable across Android versions — inspect the
    // output for the known failure markers instead.
    let ok = !message.contains("Failure")
        && !message.contains("Error")
        && !message.contains("Exception")
        && !message.contains("not installed for");
    Ok(ActionResult { ok, message })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_protected_system_app() {
        let out = "Failure [DELETE_FAILED_INTERNAL_ERROR] Broken pipe";
        let hint = decode_uninstall_error(out).expect("should decode");
        assert!(hint.contains("Protected system app"));
    }

    #[test]
    fn decodes_not_installed_for_user() {
        let out = "Failure [not installed for 0]";
        assert!(decode_uninstall_error(out).is_some());
    }

    #[test]
    fn unrecognized_returns_none() {
        assert!(decode_uninstall_error("Something totally weird").is_none());
    }
}

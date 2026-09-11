//! Per-package action commands — disable / enable / uninstall.
//!
//! Honors the reversibility model (architectural commitment #7): disable is
//! a `pm disable-user --user 0` (reversible via enable), uninstall is a
//! `pm uninstall --user 0` (semi-reversible via `cmd package install-existing`
//! or Play Store).

use std::collections::{HashMap, HashSet};

use serde::Serialize;
use tauri::State;

use crate::adb::{
    checked_batch_command, parse_checked_batch, parse_disabled_packages_output,
    parse_installed_packages_output, parse_permission_granted, parse_total_pss_by_process,
    parse_usage_stats, AppUsage,
};
use crate::engine::{classify_safety, is_valid_package_name, Safety};
use crate::license::Feature;

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
/// in `packages`. One batched shell call (`pm list packages` +
/// `pm list packages -d`), then categorize.
#[tauri::command]
pub async fn package_states(
    state: State<'_, AppState>,
    serial: String,
    packages: Vec<String>,
) -> Result<HashMap<String, PackageState>, String> {
    package_states_impl(state.inner(), &serial, packages).await
}

/// Device-free core of `package_states` so it can run against a mock driver.
pub async fn package_states_impl(
    state: &AppState,
    serial: &str,
    packages: Vec<String>,
) -> Result<HashMap<String, PackageState>, String> {
    let adb = state.adb_snapshot().await;
    let cmd = checked_batch_command(&["pm list packages", "pm list packages -d"]);
    let out = adb
        .shell(serial, &cmd)
        .await
        .map_err(|e| format!("pm list packages: {e}"))?;
    let sections = parse_checked_batch(&out.stdout, 2, &[0, 1])?;

    let installed_set: HashSet<String> = parse_installed_packages_output(&sections[0])
        .into_iter()
        .collect();
    // An Android device always has installed packages. An empty result must
    // not make every requested package appear missing, even with exit zero.
    if installed_set.is_empty() {
        return Err("pm list packages: no packages reported".to_string());
    }
    let disabled_set: HashSet<String> = parse_disabled_packages_output(&sections[1])
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
pub struct OtherPackage {
    pub package: String,
    /// Preinstalled (not in `pm list packages -3`).
    pub system: bool,
    pub enabled: bool,
    /// Friendly name from the known-names map or, for an all-installed read,
    /// the app catalog. Lets the UI search by a real name instead of only the
    /// package id. `None` for unrecognized packages.
    pub name: Option<String>,
}

/// Package-name prefixes that belong to the device vendor / OS, not the user.
/// Android flags some of these as third-party (a preinstalled Google language
/// IME updated into `/data` shows up in `pm list packages -3`), which would
/// otherwise mislabel them and bury genuinely-sideloaded apps. Treating these
/// as system keeps the default view focused on what the user actually added.
fn is_first_party_package(pkg: &str) -> bool {
    const PREFIXES: &[&str] = &[
        "com.google.",
        "com.android.",
        "com.nvidia.",
        "com.amazon.",
        "org.chromium.",
    ];
    pkg == "android" || PREFIXES.iter().any(|p| pkg.starts_with(p))
}

/// `list_other_packages` — every installed package that is NOT in the curated
/// catalog, so the App List can act on the long tail (sideloaded apps like
/// SmartTube most of all — they get the same Backup / Copy / Disable tools).
/// Third-party first, then system, names ascending.
#[tauri::command]
pub async fn list_other_packages(
    state: State<'_, AppState>,
    serial: String,
) -> Result<Vec<OtherPackage>, String> {
    list_other_packages_impl(state.inner(), &serial).await
}

pub async fn list_other_packages_impl(
    state: &AppState,
    serial: &str,
) -> Result<Vec<OtherPackage>, String> {
    list_packages_impl(state, serial, false).await
}

/// `list_installed_packages` — every installed package, including entries in
/// the curated catalog. This read-only view is used by workflows such as APK
/// backup that need a complete picker rather than the App List's "other only"
/// view.
#[tauri::command]
pub async fn list_installed_packages(
    state: State<'_, AppState>,
    serial: String,
) -> Result<Vec<OtherPackage>, String> {
    list_installed_packages_impl(state.inner(), &serial).await
}

pub async fn list_installed_packages_impl(
    state: &AppState,
    serial: &str,
) -> Result<Vec<OtherPackage>, String> {
    list_packages_impl(state, serial, true).await
}

async fn list_packages_impl(
    state: &AppState,
    serial: &str,
    include_curated: bool,
) -> Result<Vec<OtherPackage>, String> {
    use std::time::Duration;
    use tokio::time::timeout;

    let adb = state.adb_snapshot().await;

    // Third-party classification is optional; installed and disabled state
    // must both be known before exposing app actions.
    let cmd = checked_batch_command(&[
        "pm list packages",
        "pm list packages -3",
        "pm list packages -d",
    ]);
    let batched = timeout(Duration::from_secs(15), adb.shell(serial, &cmd))
        .await
        .map_err(|_| "pm list packages timed out".to_string())?
        .map_err(|e| format!("pm list packages: {e}"))?;
    let sections = parse_checked_batch(&batched.stdout, 3, &[0, 2])?;

    let all_pkgs = parse_installed_packages_output(&sections[0]);
    // Exit zero with no installed packages is still an unusable device read.
    if all_pkgs.is_empty() {
        return Err("pm list packages: no packages reported".to_string());
    }
    if sections[1].is_empty() {
        tracing::debug!("pm list packages -3 returned nothing; treating all as system");
    }

    let third: HashSet<String> = parse_installed_packages_output(&sections[1])
        .into_iter()
        .collect();
    let disabled: HashSet<String> = parse_disabled_packages_output(&sections[2])
        .into_iter()
        .collect();
    let catalog_names: HashMap<&str, &str> = state
        .app_lists
        .common
        .iter()
        .chain(state.app_lists.shield.iter())
        .chain(state.app_lists.googletv.iter())
        .map(|e| (e.package.as_str(), e.name.as_str()))
        .collect();

    let mut out: Vec<OtherPackage> = all_pkgs
        .into_iter()
        .filter(|p| include_curated || !catalog_names.contains_key(p.as_str()))
        .map(|package| OtherPackage {
            // System if Android says so OR it's a vendor/OS package Android
            // happens to flag third-party (updated Google IMEs, etc.).
            system: !third.contains(&package) || is_first_party_package(&package),
            enabled: !disabled.contains(&package),
            name: state.known_names.get(&package).cloned().or_else(|| {
                catalog_names
                    .get(package.as_str())
                    .map(|name| (*name).to_string())
            }),
            package,
        })
        .collect();
    out.sort_by(|a, b| {
        a.system
            .cmp(&b.system)
            .then_with(|| a.package.cmp(&b.package))
    });
    Ok(out)
}

/// `app_memory_map` — package → resident RAM (MB), from a single `dumpsys
/// meminfo`. Lazy companion to the App List: the UI loads the list first, then
/// fetches this and flags which apps are actually using RAM right now. Most
/// apps return nothing (not running) — the ones that do are the real signal,
/// e.g. an unused video app quietly holding background RAM.
#[tauri::command]
pub async fn app_memory_map(
    state: State<'_, AppState>,
    serial: String,
) -> Result<HashMap<String, f64>, String> {
    app_memory_map_impl(state.inner(), &serial).await
}

pub async fn app_memory_map_impl(
    state: &AppState,
    serial: &str,
) -> Result<HashMap<String, f64>, String> {
    let adb = state.adb_snapshot().await;
    let out = adb
        .shell(serial, "dumpsys meminfo")
        .await
        .map_err(|e| format!("dumpsys meminfo: {e}"))?;
    Ok(parse_total_pss_by_process(&out.stdout))
}

/// `app_usage_map` — package → last-used + launch count, from a single
/// `dumpsys usagestats`. Powers the "Review / remove if unused" signal: an app
/// never opened (or not in months) is a strong candidate to disable/uninstall.
#[tauri::command]
pub async fn app_usage_map(
    state: State<'_, AppState>,
    serial: String,
) -> Result<HashMap<String, AppUsage>, String> {
    app_usage_map_impl(state.inner(), &serial).await
}

pub async fn app_usage_map_impl(
    state: &AppState,
    serial: &str,
) -> Result<HashMap<String, AppUsage>, String> {
    let adb = state.adb_snapshot().await;
    let out = adb
        .shell(serial, "dumpsys usagestats")
        .await
        .map_err(|e| format!("dumpsys usagestats: {e}"))?;
    Ok(parse_usage_stats(&out.stdout))
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
    state.require_pro(Feature::CuratedDebloat)?;
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

/// `trim_caches` — ask the package manager to clear app caches device-wide.
/// The huge byte count means "free everything trimmable"; caches rebuild on
/// next app launch, so no confirmation ceremony is needed.
#[tauri::command]
pub async fn trim_caches(
    state: State<'_, AppState>,
    serial: String,
) -> Result<ActionResult, String> {
    run(&state, &serial, "pm trim-caches 999999999999").await
}

/// `force_stop` — `am force-stop <pkg>`. Kills the app's processes; it
/// restarts on next launch, so unlike disable nothing persists and no safety
/// gate beyond name validation is needed.
#[tauri::command]
pub async fn force_stop(
    state: State<'_, AppState>,
    serial: String,
    package: String,
) -> Result<ActionResult, String> {
    if let Some(rejection) = reject_invalid_package(&package) {
        return Ok(rejection);
    }
    run(&state, &serial, &format!("am force-stop {package}")).await
}

/// Permission names share the package-name character set; reuse the setting-key
/// allowlist so neither value can carry shell metacharacters into the command.
fn is_valid_permission(permission: &str) -> bool {
    crate::commands::is_valid_setting_key(permission)
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PermissionState {
    Granted,
    Revoked,
    /// Package not installed, or it doesn't declare the permission.
    Missing,
}

/// `app_permission_state` — is `permission` granted to `package` right now?
/// Reads `dumpsys package <pkg>`. Used by the Tweaks "disable the Assistant
/// button" toggle (revoking RECORD_AUDIO from Google's search app).
#[tauri::command]
pub async fn app_permission_state(
    state: State<'_, AppState>,
    serial: String,
    package: String,
    permission: String,
) -> Result<PermissionState, String> {
    app_permission_state_impl(state.inner(), &serial, &package, &permission).await
}

pub async fn app_permission_state_impl(
    state: &AppState,
    serial: &str,
    package: &str,
    permission: &str,
) -> Result<PermissionState, String> {
    if !is_valid_package_name(package) || !is_valid_permission(permission) {
        return Ok(PermissionState::Missing);
    }
    let adb = state.adb_snapshot().await;
    let out = adb
        .shell(serial, &format!("dumpsys package {package}"))
        .await
        .map_err(|e| format!("dumpsys package {package}: {e}"))?;
    Ok(match parse_permission_granted(&out.stdout, permission) {
        Some(true) => PermissionState::Granted,
        Some(false) => PermissionState::Revoked,
        None => PermissionState::Missing,
    })
}

/// `set_app_permission` — `pm grant`/`pm revoke <pkg> <permission>`. Reversible.
/// Powers the Assistant-button toggle; the safety gate that matters for disable
/// doesn't apply (revoking one runtime permission can't brick the device), but
/// inputs are still validated so nothing reaches the shell unchecked.
#[tauri::command]
pub async fn set_app_permission(
    state: State<'_, AppState>,
    serial: String,
    package: String,
    permission: String,
    grant: bool,
) -> Result<ActionResult, String> {
    set_app_permission_impl(state.inner(), &serial, &package, &permission, grant).await
}

pub async fn set_app_permission_impl(
    state: &AppState,
    serial: &str,
    package: &str,
    permission: &str,
    grant: bool,
) -> Result<ActionResult, String> {
    if !is_valid_package_name(package) || !is_valid_permission(permission) {
        return Ok(ActionResult {
            ok: false,
            message: format!("Refusing: invalid package/permission ({package:?}, {permission:?})"),
        });
    }
    state.require_pro(Feature::AppPermissionWrite)?;
    let verb = if grant { "grant" } else { "revoke" };
    run(state, serial, &format!("pm {verb} {package} {permission}")).await
}

/// `set_app_op` — `appops set <pkg> <op> allow|deny`. Unlike `pm revoke`,
/// `appops deny` blocks the operation silently without triggering Android's
/// re-grant dialog. Powers the Assistant-button toggle.
#[tauri::command]
pub async fn set_app_op(
    state: State<'_, AppState>,
    serial: String,
    package: String,
    op: String,
    allow: bool,
) -> Result<ActionResult, String> {
    set_app_op_impl(state.inner(), &serial, &package, &op, allow).await
}

pub async fn set_app_op_impl(
    state: &AppState,
    serial: &str,
    package: &str,
    op: &str,
    allow: bool,
) -> Result<ActionResult, String> {
    if !is_valid_package_name(package) {
        return Ok(ActionResult {
            ok: false,
            message: format!("Refusing: invalid package ({package:?})"),
        });
    }
    if !op.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') || op.is_empty() {
        return Ok(ActionResult {
            ok: false,
            message: format!("Refusing: invalid op ({op:?})"),
        });
    }
    state.require_pro(Feature::AppPermissionWrite)?;
    let mode = if allow { "allow" } else { "deny" };
    run(
        state,
        serial,
        &format!("cmd appops set {package} {op} {mode}"),
    )
    .await
}

/// `get_app_op` — reads the current appops mode for `<pkg> <op>`.
/// Returns `"allow"`, `"deny"`, `"ignore"`, `"default"`, or `"missing"`.
#[tauri::command]
pub async fn get_app_op(
    state: State<'_, AppState>,
    serial: String,
    package: String,
    op: String,
) -> Result<String, String> {
    get_app_op_impl(state.inner(), &serial, &package, &op).await
}

pub async fn get_app_op_impl(
    state: &AppState,
    serial: &str,
    package: &str,
    op: &str,
) -> Result<String, String> {
    if !is_valid_package_name(package)
        || !op.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        || op.is_empty()
    {
        return Ok("missing".to_string());
    }
    let adb = state.adb_snapshot().await;
    let out = adb
        .shell(serial, &format!("cmd appops get {package} {op}"))
        .await
        .map_err(|e| format!("cmd appops get: {e}"))?;
    let stdout = out.stdout.trim().to_lowercase();
    if stdout.contains("allow") {
        Ok("allow".to_string())
    } else if stdout.contains("deny") {
        Ok("deny".to_string())
    } else if stdout.contains("ignore") {
        Ok("ignore".to_string())
    } else if stdout.contains("default") {
        Ok("default".to_string())
    } else {
        Ok("missing".to_string())
    }
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
    state.require_pro(Feature::CuratedDebloat)?;
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

    #[tokio::test]
    async fn disabled_read_failure_never_becomes_enabled_state() {
        use crate::commands::test_support::{state_with, MockAdb};
        let status = crate::adb::batch::BATCH_STATUS;
        let two = format!("package:com.example\n{status}0\n{BATCH_SEPARATOR}\n{status}1\n");
        let state = state_with(MockAdb::default().on_shell(BATCH_SEPARATOR, &two));
        assert!(
            package_states_impl(&state, "serial", vec!["com.example".into()])
                .await
                .is_err()
        );
        let three = format!("package:com.example\n{status}0\n{BATCH_SEPARATOR}\n{status}0\n{BATCH_SEPARATOR}\n{status}1\n");
        let state = state_with(MockAdb::default().on_shell(BATCH_SEPARATOR, &three));
        assert!(list_other_packages_impl(&state, "serial").await.is_err());
        assert!(list_installed_packages_impl(&state, "serial")
            .await
            .is_err());
    }
    use crate::adb::BATCH_SEPARATOR;

    /// Device output for a batched shell: sections joined by the sentinel the
    /// device would echo between sub-commands.
    fn batched(sections: &[&str]) -> String {
        sections
            .iter()
            .map(|s| format!("{s}\n{}0\n", crate::adb::batch::BATCH_STATUS))
            .collect::<Vec<_>>()
            .join(&format!("\n{BATCH_SEPARATOR}\n"))
    }

    #[test]
    fn first_party_packages_classified_as_system() {
        // Vendor/OS packages — system even when Android flags them third-party.
        assert!(is_first_party_package(
            "com.google.android.apps.inputmethod.hindi"
        ));
        assert!(is_first_party_package("com.android.vending"));
        assert!(is_first_party_package("com.nvidia.ota"));
        assert!(is_first_party_package("android"));
        // Genuinely-sideloaded apps stay third-party.
        assert!(!is_first_party_package("com.teamsmart.videomanager.tv"));
        assert!(!is_first_party_package("ca.devmesh.overseerrtv"));
        assert!(!is_first_party_package("air.com.shirogames.evoland12"));
        // Not fooled by a prefix appearing mid-string.
        assert!(!is_first_party_package("org.evil.com.google.fake"));
    }

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

    #[tokio::test]
    async fn list_other_packages_attaches_known_friendly_names() {
        use crate::commands::test_support::{state_with, MockAdb};
        use std::collections::HashMap;

        // Two sideloads not in the (empty) catalog: one known, one not.
        // All three `pm list` variants ride one batched shell, so the mock
        // answers the sentinel with the concatenated sections (all / -3 / -d);
        // a per-command needle would match the whole compound command.
        let listing = "package:com.limelight.noir\npackage:com.unknown.app";
        let mock = MockAdb::default().on_shell(BATCH_SEPARATOR, &batched(&[listing, listing, ""]));
        let log = mock.shell_log();
        let mut names = HashMap::new();
        names.insert(
            "com.limelight.noir".to_string(),
            "Artemis (Moonlight)".to_string(),
        );
        let state = state_with(mock).with_known_names(names);

        let others = list_other_packages_impl(&state, "serial").await.unwrap();
        let artemis = others
            .iter()
            .find(|o| o.package == "com.limelight.noir")
            .expect("artemis listed");
        assert_eq!(artemis.name.as_deref(), Some("Artemis (Moonlight)"));
        let unknown = others
            .iter()
            .find(|o| o.package == "com.unknown.app")
            .expect("unknown listed");
        assert_eq!(
            unknown.name, None,
            "unrecognized package has no friendly name"
        );
        assert!(!unknown.system, "a sideload in `-3` stays third-party");
        assert!(
            unknown.enabled,
            "an empty `-d` section means nothing disabled"
        );

        let calls = log.lock().unwrap();
        assert_eq!(
            calls.len(),
            1,
            "all / -3 / -d must cost one round-trip: {calls:?}"
        );
    }

    #[tokio::test]
    async fn installed_view_includes_catalog_and_other_packages_without_changing_other_view() {
        use crate::commands::test_support::MockAdb;
        use crate::engine::{ActionMethod, AppEntry, AppListBundle, RiskTier};
        use std::sync::Arc;

        fn entry(package: &str, name: &str) -> AppEntry {
            AppEntry {
                package: package.to_string(),
                name: name.to_string(),
                method: ActionMethod::Disable,
                risk: RiskTier::Safe,
                optimize_description: String::new(),
                restore_description: String::new(),
                default_optimize: false,
                default_restore: false,
                play_store: false,
                defunct: false,
                review: false,
            }
        }

        let listing =
            "package:com.catalog.video\npackage:com.example.sideload\npackage:com.android.settings";
        let third_party = "package:com.catalog.video\npackage:com.example.sideload";
        let disabled = "package:com.android.settings";
        let output = batched(&[listing, third_party, disabled]);
        let bundle = AppListBundle {
            common: vec![entry("com.catalog.video", "Catalog Video")],
            shield: vec![],
            googletv: vec![],
        };
        let make_state = || {
            AppState::new(
                Arc::new(MockAdb::default().on_shell(BATCH_SEPARATOR, &output)),
                bundle.clone(),
                std::env::temp_dir(),
            )
            .with_known_names(HashMap::from([(
                "com.example.sideload".to_string(),
                "Sideload Player".to_string(),
            )]))
        };

        let installed = list_installed_packages_impl(&make_state(), "serial")
            .await
            .expect("all installed packages");
        assert_eq!(installed.len(), 3);
        let catalog = installed
            .iter()
            .find(|row| row.package == "com.catalog.video")
            .expect("curated package included");
        assert_eq!(catalog.name.as_deref(), Some("Catalog Video"));
        assert!(!catalog.system);
        assert!(catalog.enabled);
        let sideload = installed
            .iter()
            .find(|row| row.package == "com.example.sideload")
            .expect("noncatalog package included");
        assert_eq!(sideload.name.as_deref(), Some("Sideload Player"));
        assert!(!sideload.system);
        assert!(sideload.enabled);
        let protected = installed
            .iter()
            .find(|row| row.package == "com.android.settings")
            .expect("protected system package remains visible to read-only backup picker");
        assert!(matches!(
            classify_safety(&protected.package),
            Safety::NeverDisable { .. }
        ));
        assert!(protected.system);
        assert!(!protected.enabled);

        let others = list_other_packages_impl(&make_state(), "serial")
            .await
            .expect("other-only packages");
        assert_eq!(others.len(), 2);
        assert!(
            others.iter().all(|row| row.package != "com.catalog.video"),
            "the existing command must continue excluding curated packages"
        );
    }

    #[tokio::test]
    async fn list_other_packages_errors_when_the_primary_listing_is_empty() {
        use crate::commands::test_support::{state_with, MockAdb};

        // The batch exits with the last sub-command's status, so a failed
        // `pm list packages` reads as success with an empty section — that must
        // surface, not render an empty "Everything else" list.
        let state =
            state_with(MockAdb::default().on_shell(BATCH_SEPARATOR, &batched(&["", "", ""])));
        let err = match list_other_packages_impl(&state, "serial").await {
            Ok(rows) => panic!(
                "empty package listing must be an error, got {} rows",
                rows.len()
            ),
            Err(e) => e,
        };
        assert!(err.contains("pm list packages"), "unhelpful error: {err}");
    }

    #[tokio::test]
    async fn list_installed_packages_errors_when_the_primary_listing_is_empty() {
        use crate::commands::test_support::{state_with, MockAdb};

        let state =
            state_with(MockAdb::default().on_shell(BATCH_SEPARATOR, &batched(&["", "", ""])));
        let err = match list_installed_packages_impl(&state, "serial").await {
            Ok(rows) => panic!(
                "empty package listing must be an error, got {} rows",
                rows.len()
            ),
            Err(err) => err,
        };
        assert!(err.contains("pm list packages"), "unhelpful error: {err}");
    }

    #[tokio::test]
    async fn package_states_categorizes_from_one_batched_call() {
        use crate::commands::test_support::{state_with, MockAdb};

        let mock = MockAdb::default().on_shell(
            BATCH_SEPARATOR,
            &batched(&[
                "package:com.example.on\npackage:com.example.off",
                "package:com.example.off",
            ]),
        );
        let log = mock.shell_log();
        let state = state_with(mock);

        let states = package_states_impl(
            &state,
            "serial",
            vec![
                "com.example.on".to_string(),
                "com.example.off".to_string(),
                "com.example.absent".to_string(),
            ],
        )
        .await
        .unwrap();

        assert_eq!(states["com.example.on"], PackageState::Enabled);
        assert_eq!(states["com.example.off"], PackageState::Disabled);
        assert_eq!(states["com.example.absent"], PackageState::Missing);
        assert_eq!(
            log.lock().unwrap().len(),
            1,
            "installed + disabled must cost one round-trip"
        );
    }

    #[tokio::test]
    async fn package_states_errors_when_the_installed_section_is_empty() {
        use crate::commands::test_support::{state_with, MockAdb};

        // Otherwise every package would silently read as Missing.
        let state = state_with(MockAdb::default().on_shell(BATCH_SEPARATOR, &batched(&["", ""])));
        let err = package_states_impl(&state, "serial", vec!["com.example.on".to_string()])
            .await
            .expect_err("empty package listing must be an error");
        assert!(err.contains("pm list packages"), "unhelpful error: {err}");
    }

    #[tokio::test]
    async fn app_memory_map_parses_running_processes() {
        use crate::commands::test_support::{state_with, MockAdb};

        let meminfo = "Total PSS by process:\n\
             243,712K: com.netflix.ninja (pid 2201)\n\
             184,200K: com.teamsmart.videomanager.tv (pid 1899)\n";
        let state = state_with(MockAdb::default().on_shell("dumpsys meminfo", meminfo));
        let map = app_memory_map_impl(&state, "serial").await.unwrap();
        assert!(
            (map.get("com.netflix.ninja").copied().unwrap_or(0.0) - 238.0).abs() < 1.0,
            "netflix ~238 MB, got {:?}",
            map.get("com.netflix.ninja")
        );
        assert!(map.contains_key("com.teamsmart.videomanager.tv"));
        assert!(!map.contains_key("com.not.running"));
    }

    #[tokio::test]
    async fn app_usage_map_reports_last_used() {
        use crate::commands::test_support::{state_with, MockAdb};

        let usage = "package=com.netflix.ninja lastTimeUsed=\"2026-06-01 09:00:00\" appLaunchCount=5\n\
                     package=com.unused.app lastTimeUsed=\"1969-12-31 18:00:00\" appLaunchCount=0\n";
        let state = state_with(MockAdb::default().on_shell("dumpsys usagestats", usage));
        let map = app_usage_map_impl(&state, "serial").await.unwrap();
        assert_eq!(
            map.get("com.netflix.ninja")
                .and_then(|u| u.last_used.as_deref()),
            Some("2026-06-01 09:00:00")
        );
        assert_eq!(
            map.get("com.unused.app").and_then(|u| u.last_used.clone()),
            None
        );
    }

    #[tokio::test]
    async fn app_permission_state_reads_grant() {
        use crate::commands::test_support::{state_with, MockAdb};

        let dump = "android.permission.RECORD_AUDIO: granted=true, flags=[ GRANTED_BY_DEFAULT ]";
        let state = state_with(MockAdb::default().on_shell("dumpsys package", dump));
        let got = app_permission_state_impl(
            &state,
            "serial",
            "com.google.android.katniss",
            "android.permission.RECORD_AUDIO",
        )
        .await
        .unwrap();
        assert_eq!(got, PermissionState::Granted);
    }

    #[tokio::test]
    async fn app_permission_state_missing_when_not_listed() {
        use crate::commands::test_support::{state_with, MockAdb};

        // Empty dumpsys (package absent / no such permission) → Missing.
        let state = state_with(MockAdb::default());
        let got = app_permission_state_impl(
            &state,
            "serial",
            "com.google.android.katniss",
            "android.permission.RECORD_AUDIO",
        )
        .await
        .unwrap();
        assert_eq!(got, PermissionState::Missing);
    }

    #[tokio::test]
    async fn set_app_permission_revoke_succeeds_silently() {
        use crate::commands::test_support::{state_with, MockAdb};

        // pm revoke is silent on success; run() reports ok with "(no output)".
        let state = state_with(MockAdb::default());
        let r = set_app_permission_impl(
            &state,
            "serial",
            "com.google.android.katniss",
            "android.permission.RECORD_AUDIO",
            false,
        )
        .await
        .unwrap();
        assert!(
            r.ok,
            "silent pm revoke should read as success: {}",
            r.message
        );
    }

    #[tokio::test]
    async fn set_app_permission_rejects_injection() {
        use crate::commands::test_support::{state_with, MockAdb};

        let state = state_with(MockAdb::default());
        let r = set_app_permission_impl(
            &state,
            "serial",
            "com.google.android.katniss",
            "RECORD_AUDIO; reboot",
            false,
        )
        .await
        .unwrap();
        assert!(
            !r.ok,
            "a permission with shell metacharacters must be refused"
        );
    }

    #[tokio::test]
    async fn free_entitlement_blocks_permission_writes_before_adb() {
        use crate::commands::test_support::{state_with, MockAdb};
        use crate::license::Entitlement;

        let mock = MockAdb::default();
        let log = mock.shell_log();
        let state = state_with(mock).with_entitlement(Entitlement::Free);
        let err = match set_app_permission_impl(
            &state,
            "SERIAL",
            "com.example.app",
            "android.permission.RECORD_AUDIO",
            false,
        )
        .await
        {
            Ok(result) => panic!(
                "free entitlement unexpectedly allowed write: {}",
                result.message
            ),
            Err(err) => err,
        };
        assert_eq!(err, "LOCKED:app_permission_write");
        assert!(log.lock().unwrap().is_empty(), "ADB must not be called");
    }

    #[tokio::test]
    async fn free_entitlement_blocks_appops_before_adb() {
        use crate::commands::test_support::{state_with, MockAdb};
        use crate::license::Entitlement;

        let mock = MockAdb::default();
        let log = mock.shell_log();
        let state = state_with(mock).with_entitlement(Entitlement::Free);
        let err = match set_app_op_impl(&state, "SERIAL", "com.example.app", "RECORD_AUDIO", false)
            .await
        {
            Ok(result) => panic!(
                "free entitlement unexpectedly allowed appops: {}",
                result.message
            ),
            Err(err) => err,
        };
        assert_eq!(err, "LOCKED:app_permission_write");
        assert!(log.lock().unwrap().is_empty(), "ADB must not be called");
    }
}

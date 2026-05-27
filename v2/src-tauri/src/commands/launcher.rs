//! Launcher catalog + set-default commands.

use serde::Serialize;
use tauri::State;

use crate::engine::{launcher_catalog, LauncherEntry};

use super::AppState;

#[derive(Serialize)]
pub struct LauncherStatus {
    pub entry: LauncherEntry,
    pub installed: bool,
    pub enabled: bool,
}

#[tauri::command]
pub async fn list_launchers(
    state: State<'_, AppState>,
    serial: String,
) -> Result<Vec<LauncherStatus>, String> {
    // Pull installed + disabled package lists concurrently.
    let adb = state.adb_snapshot().await;
    let (installed_res, disabled_res) = tokio::join!(
        adb.shell(&serial, "pm list packages"),
        adb.shell(&serial, "pm list packages -d"),
    );
    let installed = installed_res.map_err(|e| format!("pm list packages: {e}"))?;
    let disabled = disabled_res.map_err(|e| format!("pm list packages -d: {e}"))?;

    let installed_pkgs = crate::adb::parse_installed_packages_output(&installed.stdout);
    let disabled_pkgs = crate::adb::parse_disabled_packages_output(&disabled.stdout);

    let result = launcher_catalog()
        .into_iter()
        .map(|entry| {
            let installed = installed_pkgs.iter().any(|p| p == &entry.package);
            let enabled = installed && !disabled_pkgs.iter().any(|p| p == &entry.package);
            LauncherStatus {
                entry,
                installed,
                enabled,
            }
        })
        .collect();
    Ok(result)
}

#[derive(Serialize)]
pub struct CurrentLauncher {
    pub package: Option<String>,
    pub activity: Option<String>,
}

#[tauri::command]
pub async fn current_launcher(
    state: State<'_, AppState>,
    serial: String,
) -> Result<CurrentLauncher, String> {
    let adb = state.adb_snapshot().await;
    let out = adb
        .shell(
            &serial,
            "cmd package resolve-activity --brief -a android.intent.action.MAIN -c android.intent.category.HOME",
        )
        .await
        .map_err(|e| format!("resolve-activity: {e}"))?;

    // Output is two lines: a priority/info line then `pkg/activity`.
    let component = out.stdout.lines().map(str::trim).find(|l| l.contains('/'));

    let (package, activity) = match component {
        Some(c) => {
            let (p, a) = c.split_once('/').unwrap();
            (Some(p.to_string()), Some(a.to_string()))
        }
        None => (None, None),
    };

    Ok(CurrentLauncher { package, activity })
}

#[derive(Serialize)]
pub struct SetLauncherResult {
    pub ok: bool,
    /// Identifier of the strategy that worked (or the last one tried on failure).
    /// One of: "role_api", "set_home_activity", "home_intent_kick".
    pub strategy: Option<String>,
    /// Active launcher after the attempt — useful for the UI to render
    /// the post-action state.
    pub current_launcher: Option<String>,
    /// Verbatim ADB error from the last failed attempt, if relevant.
    pub last_error: Option<String>,
}

/// `set_default_launcher` — port of v1's multi-strategy promotion (PR #17/#18).
/// Strategy:
///   1. `pm enable <pkg>` — unblock a previously-disabled launcher.
///   2. Modern role API: `cmd role add-role-holder android.app.role.HOME <pkg>`.
///      Skipped immediately when the build returns "Unknown command".
///   3. For each discovered HOME activity (via `cmd package query-activities
///      --components`, falling back to common-name guesses), try
///      `cmd package set-home-activity --user 0 <comp>` then `pm
///      set-home-activity --user 0 <comp>`.
///   4. Last resort: send a HOME intent — when other launchers are disabled,
///      the system resolves to the only remaining one.
/// Every attempt is verified by re-resolving the active launcher.
#[tauri::command]
pub async fn set_default_launcher(
    state: State<'_, AppState>,
    serial: String,
    package: String,
) -> Result<SetLauncherResult, String> {
    let adb = state.adb_snapshot().await;

    // 1. Enable the package — no-op for already-enabled.
    let _ = adb.shell(&serial, &format!("pm enable {package}")).await;

    let mut last_error: Option<String> = None;

    // 2. Role API.
    let role_out = adb
        .shell(
            &serial,
            &format!("cmd role add-role-holder android.app.role.HOME {package}"),
        )
        .await;
    match role_out {
        Ok(out) if !out.stdout.contains("Unknown command") => {
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            if active_launcher(&*adb, &serial).await.as_deref() == Some(package.as_str()) {
                return Ok(SetLauncherResult {
                    ok: true,
                    strategy: Some("role_api".into()),
                    current_launcher: Some(package),
                    last_error: None,
                });
            }
            if !out.stdout.is_empty() {
                last_error = Some(out.stdout);
            } else if !out.stderr.is_empty() {
                last_error = Some(out.stderr);
            }
        }
        Ok(_) => { /* Unknown command — fall through. */ }
        Err(e) => last_error = Some(e.to_string()),
    }

    // 3. Discover activity candidates.
    let mut candidates: Vec<String> = Vec::new();
    if let Some(activity) = discover_home_activity(&*adb, &serial, &package).await {
        candidates.push(activity);
    }
    for guess in [
        ".MainActivity",
        ".Main",
        ".LauncherActivity",
        ".HomeActivity",
    ] {
        candidates.push(format!("{package}/{guess}"));
    }

    for comp in &candidates {
        for cmd in [
            format!("cmd package set-home-activity --user 0 {comp}"),
            format!("pm set-home-activity --user 0 {comp}"),
        ] {
            if let Ok(out) = adb.shell(&serial, &cmd).await {
                last_error = Some(if out.stderr.is_empty() {
                    out.stdout.clone()
                } else {
                    out.stderr.clone()
                });
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                if active_launcher(&*adb, &serial).await.as_deref() == Some(package.as_str()) {
                    return Ok(SetLauncherResult {
                        ok: true,
                        strategy: Some("set_home_activity".into()),
                        current_launcher: Some(package),
                        last_error: None,
                    });
                }
            }
        }
    }

    // 4. HOME-intent kick — system will resolve to the only remaining HOME app
    // if everything else got disabled.
    let _ = adb
        .shell(
            &serial,
            "am start -W -a android.intent.action.MAIN -c android.intent.category.HOME",
        )
        .await;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    let now_active = active_launcher(&*adb, &serial).await;
    if now_active.as_deref() == Some(package.as_str()) {
        return Ok(SetLauncherResult {
            ok: true,
            strategy: Some("home_intent_kick".into()),
            current_launcher: now_active,
            last_error: None,
        });
    }

    Ok(SetLauncherResult {
        ok: false,
        strategy: None,
        current_launcher: now_active,
        last_error,
    })
}

async fn active_launcher(adb: &dyn crate::adb::AdbDriver, serial: &str) -> Option<String> {
    let out = adb
        .shell(
            serial,
            "cmd package resolve-activity --brief -a android.intent.action.MAIN -c android.intent.category.HOME",
        )
        .await
        .ok()?;
    out.stdout
        .lines()
        .map(str::trim)
        .find(|l| l.contains('/'))
        .and_then(|c| c.split_once('/'))
        .map(|(p, _)| p.to_string())
}

/// Find a HOME activity for `package` via `cmd package query-activities`.
/// Returns a `pkg/activity` component string ready for set-home-activity.
async fn discover_home_activity(
    adb: &dyn crate::adb::AdbDriver,
    serial: &str,
    package: &str,
) -> Option<String> {
    let out = adb
        .shell(
            serial,
            "cmd package query-activities --components -a android.intent.action.MAIN -c android.intent.category.HOME",
        )
        .await
        .ok()?;
    let needle = format!("{package}/");
    out.stdout
        .lines()
        .map(str::trim)
        .find(|l| l.starts_with(&needle))
        .map(str::to_string)
}

/// `channel_provider_disabled` — fast check used by the launcher wizard to warn
/// users that disabling `com.android.providers.tv` will break Watch Next rows.
#[tauri::command]
pub async fn channel_provider_disabled(
    state: State<'_, AppState>,
    serial: String,
) -> Result<bool, String> {
    let adb = state.adb_snapshot().await;
    let out = adb
        .shell(&serial, "pm list packages -d com.android.providers.tv")
        .await
        .map_err(|e| format!("pm list packages -d: {e}"))?;
    Ok(out
        .stdout
        .lines()
        .any(|l| l.trim() == "package:com.android.providers.tv"))
}

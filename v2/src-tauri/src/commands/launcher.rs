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
    // Pull installed + disabled package lists in two ADB calls.
    let installed = state
        .adb
        .shell(&serial, "pm list packages")
        .await
        .map_err(|e| format!("pm list packages: {e}"))?;
    let disabled = state
        .adb
        .shell(&serial, "pm list packages -d")
        .await
        .map_err(|e| format!("pm list packages -d: {e}"))?;

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
    let out = state
        .adb
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

/// `channel_provider_disabled` — fast check used by the launcher wizard to warn
/// users that disabling `com.android.providers.tv` will break Watch Next rows.
#[tauri::command]
pub async fn channel_provider_disabled(
    state: State<'_, AppState>,
    serial: String,
) -> Result<bool, String> {
    let out = state
        .adb
        .shell(&serial, "pm list packages -d com.android.providers.tv")
        .await
        .map_err(|e| format!("pm list packages -d: {e}"))?;
    Ok(out
        .stdout
        .lines()
        .any(|l| l.trim() == "package:com.android.providers.tv"))
}

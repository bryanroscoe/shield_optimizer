//! Launcher catalog + set-default commands.

use serde::Serialize;
use tauri::State;

use crate::engine::{is_valid_package_name, launcher_catalog, LauncherEntry};

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
    set_default_launcher_impl(state.inner(), &serial, &package).await
}

/// Reusable implementation — callable from inside other commands without
/// the `State<'_, T>` lifetime constraint getting in the way.
pub async fn set_default_launcher_impl(
    state: &AppState,
    serial: &str,
    package: &str,
) -> Result<SetLauncherResult, String> {
    // `package` can come from a custom-launcher entry the user typed, so it's
    // interpolated into shell commands below — validate it first.
    if !is_valid_package_name(package) {
        return Ok(SetLauncherResult {
            ok: false,
            strategy: None,
            current_launcher: None,
            last_error: Some(format!("Invalid package name: {package:?}")),
        });
    }

    let adb = state.adb_snapshot().await;

    // 1. Enable the package — no-op for already-enabled.
    let _ = adb.shell(serial, &format!("pm enable {package}")).await;

    let mut last_error: Option<String> = None;

    // 2. Role API.
    let role_out = adb
        .shell(
            serial,
            &format!("cmd role add-role-holder android.app.role.HOME {package}"),
        )
        .await;
    match role_out {
        Ok(out) if !out.stdout.contains("Unknown command") => {
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            if active_launcher(&*adb, serial).await.as_deref() == Some(package) {
                return Ok(SetLauncherResult {
                    ok: true,
                    strategy: Some("role_api".into()),
                    current_launcher: Some(package.to_string()),
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
    if let Some(activity) = discover_home_activity(&*adb, serial, package).await {
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
            if let Ok(out) = adb.shell(serial, &cmd).await {
                // Only record a diagnostic if the attempt actually printed
                // something — set-home-activity is silent on success, so
                // capturing its empty output would mask the real last error.
                let msg = if out.stderr.trim().is_empty() {
                    out.stdout.trim()
                } else {
                    out.stderr.trim()
                };
                if !msg.is_empty() {
                    last_error = Some(msg.to_string());
                }
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                if active_launcher(&*adb, serial).await.as_deref() == Some(package) {
                    return Ok(SetLauncherResult {
                        ok: true,
                        strategy: Some("set_home_activity".into()),
                        current_launcher: Some(package.to_string()),
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
            serial,
            "am start -W -a android.intent.action.MAIN -c android.intent.category.HOME",
        )
        .await;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    let now_active = active_launcher(&*adb, serial).await;
    if now_active.as_deref() == Some(package) {
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

/// One HOME-capable package on the device, with a friendly name resolved via
/// the launcher catalog (where possible) and its current enable state.
#[derive(Serialize)]
pub struct HomeHandler {
    pub package: String,
    pub name: String,
    pub enabled: bool,
    /// True when the v1 safe-fallback list says we must never disable this
    /// package (e.g. `com.android.tv.settings` — emergency HOME fallback).
    pub safe_fallback: bool,
}

/// Packages we never disable as part of the stock-launcher wizard, even if
/// the user picked Yes. Matches v1's `Disable-AllStockLaunchers` safety list.
const SAFE_FALLBACKS: &[&str] = &["com.android.tv.settings", "com.android.settings"];

/// Known stock launcher packages — friendly names for handlers we recognize
/// but that aren't in our custom-launcher catalog.
const STOCK_LAUNCHER_NAMES: &[(&str, &str)] = &[
    (
        "com.google.android.tvlauncher",
        "Android TV Launcher (Stock)",
    ),
    (
        "com.google.android.apps.tv.launcherx",
        "Google TV Home (Stock)",
    ),
    (
        "com.google.android.leanbacklauncher",
        "Leanback Launcher (Stock)",
    ),
    ("com.amazon.tv.launcher", "Amazon TV Launcher"),
    (
        "com.google.android.tungsten.setupwraith",
        "Setup Wraith (HOME)",
    ),
    (
        "com.droidlogic.launcher.provider",
        "Droidlogic Launcher Provider",
    ),
];

/// `list_home_handlers` — every HOME-capable package the device reports,
/// excluding `target_package` (the chosen custom launcher) and any custom
/// launcher already in our catalog. Mirrors v1's `Get-HomeHandlers` (§6.4):
/// invokes `cmd package query-activities` (without `--components`, which gives
/// the richer ResolveInfo dump) and parses `packageName=<pkg>` rows.
#[tauri::command]
pub async fn list_home_handlers(
    state: State<'_, AppState>,
    serial: String,
    target_package: String,
) -> Result<Vec<HomeHandler>, String> {
    let adb = state.adb_snapshot().await;
    let out = adb
        .shell(
            &serial,
            "cmd package query-activities -a android.intent.action.MAIN -c android.intent.category.HOME",
        )
        .await
        .map_err(|e| format!("query-activities: {e}"))?;

    // Disabled package set so we can tag each handler.
    let disabled_out = adb
        .shell(&serial, "pm list packages -d")
        .await
        .map_err(|e| format!("pm list packages -d: {e}"))?;
    let disabled = crate::adb::parse_disabled_packages_output(&disabled_out.stdout);

    let mut seen = std::collections::HashSet::new();
    let mut handlers = Vec::new();
    for pkg in parse_home_handler_packages(&out.stdout) {
        if pkg == target_package || !seen.insert(pkg.clone()) {
            continue;
        }
        let name = STOCK_LAUNCHER_NAMES
            .iter()
            .find(|(p, _)| *p == pkg.as_str())
            .map(|(_, n)| (*n).to_string())
            .unwrap_or_else(|| pkg.clone());
        let enabled = !disabled.iter().any(|d| d == &pkg);
        // A handler is "off-limits" if it's a HOME-emergency fallback OR if
        // the broader safety list says don't touch it. The UI uses this to
        // disable the checkbox in the stock-launcher wizard.
        let safe_fallback =
            SAFE_FALLBACKS.contains(&pkg.as_str()) || crate::engine::is_never_disable(&pkg);
        handlers.push(HomeHandler {
            package: pkg,
            name,
            enabled,
            safe_fallback,
        });
    }
    Ok(handlers)
}

/// Parse `cmd package query-activities` output for `packageName=<pkg>` rows.
/// Each Activity block exposes one packageName line. Strict regex (real
/// package names start with a letter and only contain `[a-zA-Z0-9_.]`)
/// avoids matching anything that happens to contain the string.
fn parse_home_handler_packages(stdout: &str) -> Vec<String> {
    static RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"^\s*packageName=([a-zA-Z][a-zA-Z0-9_.]+)\s*$").unwrap()
    });
    stdout
        .lines()
        .filter_map(|line| RE.captures(line).map(|c| c[1].to_string()))
        .collect()
}

#[derive(Serialize)]
pub struct StockLauncherResult {
    pub processed: Vec<String>,
    pub failed: Vec<String>,
    pub skipped_safe: Vec<String>,
    pub summary: String,
}

/// `disable_stock_launchers` — `pm disable-user --user 0` for each package
/// passed in `packages`, refusing to touch safe-fallback packages. Designed
/// to be called after `set_default_launcher` has promoted the chosen custom
/// launcher. Mirrors v1's Disable-AllStockLaunchers (§6.4).
#[tauri::command]
pub async fn disable_stock_launchers(
    state: State<'_, AppState>,
    serial: String,
    packages: Vec<String>,
) -> Result<StockLauncherResult, String> {
    let adb = state.adb_snapshot().await;
    let mut processed = Vec::new();
    let mut failed = Vec::new();
    let mut skipped_safe = Vec::new();
    for pkg in packages {
        // Two-tier guard: the original SAFE_FALLBACKS list (HOME-handler
        // emergency fallbacks like com.android.tv.settings), AND the broader
        // engine::safety::NEVER_DISABLE list (anything that would brick).
        if SAFE_FALLBACKS.contains(&pkg.as_str()) || crate::engine::is_never_disable(&pkg) {
            skipped_safe.push(pkg);
            continue;
        }
        let cmd = format!("pm disable-user --user 0 {pkg}");
        match adb.shell(&serial, &cmd).await {
            Ok(out) if !out.shell_reported_failure() => {
                processed.push(pkg);
            }
            _ => failed.push(pkg),
        }
    }
    let summary = format!(
        "Disabled {} HOME handler(s). {} failed. {} safe fallback(s) skipped.",
        processed.len(),
        failed.len(),
        skipped_safe.len(),
    );
    Ok(StockLauncherResult {
        processed,
        failed,
        skipped_safe,
        summary,
    })
}

/// `restore_stock_launchers` — `pm enable` for each package. Mirrors v1's
/// Restore-AllStockLaunchers (§6.5). Safe to call with arbitrary packages —
/// `pm enable` on something that's already enabled is a no-op.
#[tauri::command]
pub async fn restore_stock_launchers(
    state: State<'_, AppState>,
    serial: String,
    packages: Vec<String>,
) -> Result<StockLauncherResult, String> {
    let adb = state.adb_snapshot().await;
    let mut processed = Vec::new();
    let mut failed = Vec::new();
    for pkg in packages {
        match adb.shell(&serial, &format!("pm enable {pkg}")).await {
            Ok(out) if !out.shell_reported_failure() => {
                processed.push(pkg);
            }
            _ => failed.push(pkg),
        }
    }
    let summary = format!(
        "Re-enabled {} HOME handler(s). {} failed.",
        processed.len(),
        failed.len(),
    );
    Ok(StockLauncherResult {
        processed,
        failed,
        skipped_safe: Vec::new(),
        summary,
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_home_handler_packages_from_resolveinfo() {
        let input = "Activity #0:\n  \
                     Priority=0 PreferredOrder=0 Match=0x108000 Specific=null\n  \
                     ActivityInfo:\n    \
                     name=com.spocky.projengmenu.ui.home.MainActivity\n    \
                     packageName=com.spocky.projengmenu\n    \
                     labelRes=0x7f0e0000\n\n\
                     Activity #1:\n  \
                     ActivityInfo:\n    \
                     name=com.google.android.tvlauncher.MainActivity\n    \
                     packageName=com.google.android.tvlauncher\n";
        let pkgs = parse_home_handler_packages(input);
        assert_eq!(
            pkgs,
            vec![
                "com.spocky.projengmenu".to_string(),
                "com.google.android.tvlauncher".to_string(),
            ]
        );
    }

    #[test]
    fn ignores_unrelated_lines_with_slashes_or_paths() {
        // /data/... should never be matched as a package. Only well-formed
        // `packageName=<pkg>` lines qualify.
        let input = "Path: /data/local/tmp\nResult: foo/bar\npackageName=com.example.foo\n";
        assert_eq!(
            parse_home_handler_packages(input),
            vec!["com.example.foo".to_string()]
        );
    }
}

//! Launcher catalog + set-default commands.

use serde::Serialize;
use tauri::State;

use crate::engine::{
    is_last_enabled_home_handler, is_valid_package_name, launcher_catalog, launcher_rows,
    stock_launcher_catalog, LauncherStatus,
};

use super::{home_tracking, AppState};

const HOME_HANDLER_QUERY: &str =
    "cmd package query-activities -a android.intent.action.MAIN -c android.intent.category.HOME";

#[tauri::command]
pub async fn list_launchers(
    state: State<'_, AppState>,
    serial: String,
) -> Result<Vec<LauncherStatus>, String> {
    // Pull installed + disabled package lists and HOME handlers concurrently.
    let adb = state.adb_snapshot().await;
    let (installed_res, disabled_res, handlers_res) = tokio::join!(
        adb.shell(&serial, "pm list packages"),
        adb.shell(&serial, "pm list packages -d"),
        adb.shell(&serial, HOME_HANDLER_QUERY),
    );
    let installed = installed_res.map_err(|e| format!("pm list packages: {e}"))?;
    let disabled = disabled_res.map_err(|e| format!("pm list packages -d: {e}"))?;
    // The HOME query only adds "other handler" rows — degrade to none rather
    // than blanking the whole list on builds where `cmd package` is limited.
    let handler_pkgs = match handlers_res {
        Ok(out) => parse_home_handler_packages(&out.stdout),
        Err(e) => {
            tracing::warn!(error = %e, "query-activities failed; listing catalog launchers only");
            Vec::new()
        }
    };

    let installed_pkgs = crate::adb::parse_installed_packages_output(&installed.stdout);
    let disabled_pkgs = crate::adb::parse_disabled_packages_output(&disabled.stdout);

    // Disabled handlers don't answer the HOME query — the tracker is what
    // keeps their rows (and the Enable path back) alive. Prune entries that
    // were re-enabled or uninstalled out-of-band.
    let tracked = home_tracking::prune(&state.data_dir, &serial, &disabled_pkgs).await;

    Ok(launcher_rows(
        &installed_pkgs,
        &disabled_pkgs,
        &handler_pkgs,
        &tracked,
    ))
}

/// `disable_launcher` — `disable_package` plus the launcher-specific guard:
/// refuses to disable the last enabled HOME handler, which would leave the
/// device with nowhere to land on Home. Non-catalog handlers are recorded in
/// the tracker so their row survives being disabled.
#[tauri::command]
pub async fn disable_launcher(
    state: State<'_, AppState>,
    serial: String,
    package: String,
) -> Result<crate::commands::apps::ActionResult, String> {
    let adb = state.adb_snapshot().await;
    let enabled_handlers = adb
        .shell(&serial, HOME_HANDLER_QUERY)
        .await
        .map(|out| parse_home_handler_packages(&out.stdout))
        .map_err(|e| format!("query-activities: {e}"))?;
    if is_last_enabled_home_handler(&package, &enabled_handlers) {
        return Ok(crate::commands::apps::ActionResult {
            ok: false,
            message: format!(
                "Refusing to disable {package}: it's the only enabled launcher left on this \
                 device. Enable another launcher first."
            ),
        });
    }

    let data_dir = state.data_dir.clone();
    let result =
        crate::commands::apps::disable_package(state, serial.clone(), package.clone()).await?;

    let in_catalogs = stock_launcher_catalog()
        .iter()
        .chain(launcher_catalog().iter())
        .any(|e| e.package == package);
    if result.ok && !in_catalogs {
        home_tracking::record(&data_dir, &serial, &package).await;
    }
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
    /// One of: "role_api", "set_home_activity", "home_intent_kick",
    /// "disable_stock_takeover" (stock launcher disabled to hand HOME over —
    /// the only method that works on builds whose role/set-home-activity
    /// commands accept-but-ignore).
    pub strategy: Option<String>,
    /// Active launcher after the attempt — useful for the UI to render
    /// the post-action state.
    pub current_launcher: Option<String>,
    /// Verbatim ADB error from the last failed attempt, if relevant.
    pub last_error: Option<String>,
    /// True when the polite strategies failed but disabling the active
    /// *stock* launcher would hand HOME to the target (the only working
    /// method on accept-but-ignore builds). The UI asks the user and retries
    /// with `allow_stock_disable` — it is never done silently.
    pub stock_takeover_available: bool,
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
    allow_stock_disable: Option<bool>,
) -> Result<SetLauncherResult, String> {
    set_default_launcher_impl(
        state.inner(),
        &serial,
        &package,
        allow_stock_disable.unwrap_or(false),
    )
    .await
}

/// Reusable implementation — callable from inside other commands without
/// the `State<'_, T>` lifetime constraint getting in the way.
/// `allow_stock_disable` opts into the disable-stock-takeover last resort;
/// without it the caller gets `stock_takeover_available` back and decides.
pub async fn set_default_launcher_impl(
    state: &AppState,
    serial: &str,
    package: &str,
    allow_stock_disable: bool,
) -> Result<SetLauncherResult, String> {
    // `package` can come from a custom-launcher entry the user typed, so it's
    // interpolated into shell commands below — validate it first.
    if !is_valid_package_name(package) {
        return Ok(SetLauncherResult {
            ok: false,
            strategy: None,
            current_launcher: None,
            last_error: Some(format!("Invalid package name: {package:?}")),
            stock_takeover_available: false,
        });
    }

    let adb = state.adb_snapshot().await;

    // 1. Enable the package — no-op for already-enabled.
    let _ = adb.shell(serial, &format!("pm enable {package}")).await;

    let mut last_error: Option<String> = None;
    // Set when a strategy was acknowledged by the device ("Success" or a clean
    // silent exit) even if the active-HOME resolver never confirmed the switch
    // — that combination means "accepted, press Home" rather than "failed".
    let mut device_accepted = false;

    // 2. Role API.
    let role_out = adb
        .shell(
            serial,
            &format!("cmd role add-role-holder android.app.role.HOME {package}"),
        )
        .await;
    match role_out {
        Ok(out) if !out.stdout.contains("Unknown command") => {
            if verify_active(&*adb, serial, package).await {
                return Ok(SetLauncherResult {
                    ok: true,
                    strategy: Some("role_api".into()),
                    current_launcher: Some(package.to_string()),
                    last_error: None,
                    stock_takeover_available: false,
                });
            }
            let msg = if out.stdout.trim().is_empty() {
                out.stderr.trim().to_string()
            } else {
                out.stdout.trim().to_string()
            };
            if is_success_ack(&msg) || msg.is_empty() {
                device_accepted = true;
            } else {
                last_error = Some(msg);
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

    'attempts: for comp in &candidates {
        for cmd in [
            format!("cmd package set-home-activity --user 0 {comp}"),
            format!("pm set-home-activity --user 0 {comp}"),
        ] {
            if let Ok(out) = adb.shell(serial, &cmd).await {
                // set-home-activity prints a bare "Success" on many builds and
                // nothing on others — both are acceptance, NOT diagnostics.
                // Only real error text (e.g. "Error: …", usage spew) goes into
                // last_error; recording the ack produced "Failed: Success".
                let msg = if out.stderr.trim().is_empty() {
                    out.stdout.trim()
                } else {
                    out.stderr.trim()
                };
                if is_success_ack(msg) || msg.is_empty() {
                    device_accepted = true;
                    if verify_active(&*adb, serial, package).await {
                        return Ok(SetLauncherResult {
                            ok: true,
                            strategy: Some("set_home_activity".into()),
                            current_launcher: Some(package.to_string()),
                            last_error: None,
                            stock_takeover_available: false,
                        });
                    }
                    // Accepted but the resolver didn't confirm: the preference
                    // is now set to a real component of `package`. Re-running
                    // the remaining guesses can only overwrite it with a worse
                    // one — stop here and let the HOME-intent kick finish it.
                    break 'attempts;
                }
                // Real error — nothing changed on the device, no point polling
                // the resolver. Try the next variant/candidate.
                last_error = Some(msg.to_string());
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
            stock_takeover_available: false,
        });
    }

    // 5. The strategy that actually works on Android 11 TV builds where the
    // role API silently no-ops and set-home-activity answers "Success"
    // without effect (verified live on a Shield 2019 / Android 11): when the
    // app holding HOME is a *stock* launcher, disable it — Android then
    // resolves HOME to the remaining launcher. This is v1's proven
    // Setup-Launcher mechanism — but it is NEVER applied without the
    // caller's explicit opt-in (`allow_stock_disable`); otherwise we report
    // that the option exists and let the user decide. Never touches a custom
    // launcher; the NEVER_DISABLE gate still applies; and if the takeover
    // doesn't verify, the stock launcher is re-enabled.
    if let Some(active) = now_active.clone() {
        let active_is_stock = stock_launcher_catalog().iter().any(|e| e.package == active);
        let blocked = matches!(
            crate::engine::classify_safety(&active),
            crate::engine::Safety::NeverDisable { .. }
        );
        if active != package && active_is_stock && !blocked {
            if !allow_stock_disable {
                return Ok(SetLauncherResult {
                    ok: false,
                    strategy: None,
                    current_launcher: Some(active.clone()),
                    last_error: Some(format!(
                        "This device ignores the standard launcher-switch commands. The only \
                         way to hand HOME to {package} is to disable the stock launcher \
                         ({active}) — it can be re-enabled from the list at any time."
                    )),
                    stock_takeover_available: true,
                });
            }
            let disabled_ok = matches!(
                adb.shell(serial, &format!("pm disable-user --user 0 {active}")).await,
                Ok(ref o) if !o.shell_reported_failure()
            );
            if disabled_ok {
                let _ = adb
                    .shell(
                        serial,
                        "am start -W -a android.intent.action.MAIN -c android.intent.category.HOME",
                    )
                    .await;
                if verify_active(&*adb, serial, package).await {
                    return Ok(SetLauncherResult {
                        ok: true,
                        strategy: Some("disable_stock_takeover".into()),
                        current_launcher: Some(package.to_string()),
                        last_error: None,
                        stock_takeover_available: false,
                    });
                }
                // Takeover didn't verify — put the stock launcher back the
                // way we found it rather than leaving a half-applied state.
                if is_valid_package_name(&active) {
                    let _ = adb.shell(serial, &format!("pm enable {active}")).await;
                }
            }
        }
    }

    // The device acknowledged the change but the resolver never confirmed it.
    // That's "accepted, not yet visible" — common on builds that only apply
    // the preference on the next physical Home press. Say so instead of
    // surfacing the raw "Success" ack as a failure reason.
    if device_accepted {
        last_error = Some(format!(
            "The device accepted the launcher change but still reports {} as the active HOME app. \
             Press Home on the TV, then hit Refresh — some devices only switch on the next Home press.",
            now_active.as_deref().unwrap_or("the previous launcher")
        ));
    }

    Ok(SetLauncherResult {
        ok: false,
        strategy: None,
        current_launcher: now_active,
        last_error,
        stock_takeover_available: false,
    })
}

/// `cmd package set-home-activity` / `pm set-home-activity` acknowledge with a
/// bare "Success" line on many builds (and stay silent on others). That's an
/// acceptance, not a diagnostic — surfacing it as an error produced the
/// infamous "Failed: Success" message.
fn is_success_ack(s: &str) -> bool {
    s.trim().eq_ignore_ascii_case("success")
}

/// Poll the active-HOME resolver until it reports `package`, with backoff.
/// Propagation after set-home-activity / role changes isn't instant on every
/// build — a single immediate check produced false "failed" results even when
/// the device had accepted the change.
async fn verify_active(adb: &dyn crate::adb::AdbDriver, serial: &str, package: &str) -> bool {
    for delay_ms in [200u64, 500, 900] {
        tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
        if active_launcher(adb, serial).await.as_deref() == Some(package) {
            return true;
        }
    }
    false
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

/// `channel_provider_disabled` — fast check used by the Launcher tab to warn
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

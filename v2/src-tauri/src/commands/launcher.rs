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

/// Per-step progress sink for `set_default_launcher`. The multi-strategy
/// switch can take a few seconds (enable → role → set-home-activity → verify,
/// with backoff polls in between), so the frontend passes a Channel to narrate
/// each step. Internal callers (snapshot apply, tests) use `Progress::Silent`.
pub enum Progress {
    Channel(tauri::ipc::Channel<String>),
    Silent,
}

impl Progress {
    fn step(&self, msg: &str) {
        if let Progress::Channel(ch) = self {
            let _ = ch.send(msg.to_string());
        }
    }
}

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
///   2. Stock fast path: if a *stock* launcher currently holds HOME, try the
///      polite setters (role + set-home-activity) once and, if HOME still
///      resolves to stock, go straight to the opt-in disable-stock takeover.
///      An enabled stock launcher overrides set-home-activity / the role API
///      (they answer "Success" but HOME stays on stock; verified live on
///      Shield / Android 11), so the only reliable switch is to disable stock —
///      v1's Launcher-Wizard move. Other launchers are never touched.
///   3. Otherwise (switching between non-stock launchers) run the full ladder:
///      role API → set-home-activity over discovered/guessed HOME activities →
///      HOME-intent kick → disable-stock takeover as a last resort.
/// Every attempt is verified by re-resolving the active launcher, and the
/// takeover is gated on the caller's explicit `allow_stock_disable` opt-in.
#[tauri::command]
pub async fn set_default_launcher(
    state: State<'_, AppState>,
    serial: String,
    package: String,
    allow_stock_disable: Option<bool>,
    on_progress: tauri::ipc::Channel<String>,
) -> Result<SetLauncherResult, String> {
    set_default_launcher_impl(
        state.inner(),
        &serial,
        &package,
        allow_stock_disable.unwrap_or(false),
        &Progress::Channel(on_progress),
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
    progress: &Progress,
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
    progress.step("Enabling this launcher");
    let _ = adb.shell(serial, &format!("pm enable {package}")).await;

    // Stock fast path: when the launcher currently holding HOME is *stock*, the
    // polite setters can't win — an enabled stock launcher overrides
    // set-home-activity and the role API (they return "Success" but HOME keeps
    // resolving to stock; verified live on Shield / Android 11). So try the
    // cheap setters exactly once, and if HOME still resolves to stock go
    // straight to the opt-in disable-stock takeover rather than grinding the
    // full strategy ladder with its multi-second verify back-offs. Switches
    // between non-stock launchers fall through to the normal ladder below,
    // which works for them.
    if let Some(active) = active_launcher(&*adb, serial).await {
        let active_is_stock = stock_launcher_catalog().iter().any(|e| e.package == active);
        if active_is_stock && active != package {
            progress.step("Assigning the Home role to it");
            let _ = adb
                .shell(
                    serial,
                    &format!("cmd role add-role-holder android.app.role.HOME {package}"),
                )
                .await;
            if let Some(comp) = discover_home_activity(&*adb, serial, package).await {
                progress.step("Registering it as the Home app");
                let _ = adb
                    .shell(
                        serial,
                        &format!("cmd package set-home-activity --user 0 {comp}"),
                    )
                    .await;
            }
            // When the setters work they take effect immediately; when stock
            // overrides them they never will — one quick check is enough.
            progress.step("Checking whether Home switched over");
            tokio::time::sleep(std::time::Duration::from_millis(400)).await;
            if active_launcher(&*adb, serial).await.as_deref() == Some(package) {
                return Ok(SetLauncherResult {
                    ok: true,
                    strategy: Some("set_home_activity".into()),
                    current_launcher: Some(package.to_string()),
                    last_error: None,
                    stock_takeover_available: false,
                });
            }
            if let Some(result) = stock_takeover(
                &*adb,
                serial,
                package,
                &active,
                allow_stock_disable,
                progress,
            )
            .await
            {
                return Ok(result);
            }
            // Stock is on the NEVER_DISABLE list — fall through to the ladder,
            // which will at least try the polite strategies and report cleanly.
        }
    }

    let mut last_error: Option<String> = None;
    // Set when a strategy was acknowledged by the device ("Success" or a clean
    // silent exit) even if the active-HOME resolver never confirmed the switch
    // — that combination means "accepted, press Home" rather than "failed".
    let mut device_accepted = false;

    // 2. Role API.
    progress.step("Assigning the Home role to it");
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
    progress.step("Registering it as the Home app");
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
    progress.step("Switching Home over to it");
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

    // 5. Last resort: if the launcher still holding HOME is *stock*, disable it
    // (the same takeover the stock fast path uses). This catches the case where
    // HOME wasn't stock at the start but resolved back to it after the polite
    // strategies. Gated, never touches other launchers, re-enables on failure.
    if let Some(active) = now_active.clone() {
        if let Some(result) = stock_takeover(
            &*adb,
            serial,
            package,
            &active,
            allow_stock_disable,
            progress,
        )
        .await
        {
            return Ok(result);
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

/// Disable the active *stock* launcher so HOME resolves to `package`. On builds
/// where an enabled stock launcher overrides set-home-activity / the role API
/// (they answer "Success" but HOME keeps resolving to stock — verified live on
/// Shield / Android 11), this is the only switch that sticks. It's v1's proven
/// Launcher-Wizard move: leave the *other* launchers alone, just take stock out
/// of the way. Gated on `allow_stock_disable`; without it, returns
/// `stock_takeover_available` so the UI can confirm. Never disables a
/// NEVER_DISABLE package, and re-enables stock if the switch doesn't verify.
/// Returns `None` when `active` isn't a disable-able stock launcher — the
/// caller then falls through to its normal failure path.
async fn stock_takeover(
    adb: &dyn crate::adb::AdbDriver,
    serial: &str,
    package: &str,
    active: &str,
    allow_stock_disable: bool,
    progress: &Progress,
) -> Option<SetLauncherResult> {
    let active_is_stock = stock_launcher_catalog().iter().any(|e| e.package == active);
    let blocked = matches!(
        crate::engine::classify_safety(active),
        crate::engine::Safety::NeverDisable { .. }
    );
    if active == package || !active_is_stock || blocked {
        return None;
    }
    if !allow_stock_disable {
        return Some(SetLauncherResult {
            ok: false,
            strategy: None,
            current_launcher: Some(active.to_string()),
            last_error: Some(format!(
                "Switching to {package} means disabling the stock launcher ({active}) — on this \
                 device that's the only thing that hands HOME over. Your other launchers are left \
                 alone, and stock can be re-enabled from this list at any time."
            )),
            stock_takeover_available: true,
        });
    }
    progress.step(&format!(
        "Disabling the stock launcher ({active}) to hand Home over"
    ));
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
        progress.step("Checking whether Home switched over");
        if verify_active(adb, serial, package).await {
            return Some(SetLauncherResult {
                ok: true,
                strategy: Some("disable_stock_takeover".into()),
                current_launcher: Some(package.to_string()),
                last_error: None,
                stock_takeover_available: false,
            });
        }
        // Didn't verify — put stock back rather than leave a half-applied state.
        if is_valid_package_name(active) {
            let _ = adb.shell(serial, &format!("pm enable {active}")).await;
        }
    }
    Some(SetLauncherResult {
        ok: false,
        strategy: None,
        current_launcher: Some(active.to_string()),
        last_error: Some(format!(
            "Disabled {active} but HOME still didn't switch to {package}, so it was re-enabled to \
             avoid leaving the device without a launcher. Try again, or set it from the TV's \
             Settings."
        )),
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
    use crate::commands::test_support::{state_with, MockAdb};

    #[tokio::test]
    async fn set_launcher_first_strategy_wins_and_skips_the_rest() {
        // Role API succeeds and the resolver confirms it — the fallback ladder
        // (set-home-activity, query-activities, HOME-intent kick) must not run.
        let mock = MockAdb::default()
            .on_shell("add-role-holder", "Success")
            .on_shell("resolve-activity", "com.example.launcher/.MainActivity");
        let log = mock.shell_log();
        let state = state_with(mock);

        let res = set_default_launcher_impl(
            &state,
            "serial",
            "com.example.launcher",
            false,
            &Progress::Silent,
        )
        .await
        .unwrap();

        assert!(res.ok);
        assert_eq!(res.strategy.as_deref(), Some("role_api"));
        assert_eq!(
            res.current_launcher.as_deref(),
            Some("com.example.launcher")
        );

        let calls = log.lock().unwrap();
        assert!(!calls.iter().any(|c| c.contains("set-home-activity")));
        assert!(!calls.iter().any(|c| c.contains("query-activities")));
        assert!(!calls.iter().any(|c| c.contains("am start")));
    }

    #[tokio::test]
    async fn set_launcher_falls_back_to_set_home_activity_when_role_unsupported() {
        // Build doesn't know the role command; the set-home-activity fallback
        // takes over and verifies.
        let mock = MockAdb::default()
            .on_shell("add-role-holder", "Unknown command")
            .on_shell("set-home-activity", "Success")
            .on_shell("resolve-activity", "com.example.launcher/.MainActivity");
        let log = mock.shell_log();
        let state = state_with(mock);

        let res = set_default_launcher_impl(
            &state,
            "serial",
            "com.example.launcher",
            false,
            &Progress::Silent,
        )
        .await
        .unwrap();

        assert!(res.ok);
        assert_eq!(res.strategy.as_deref(), Some("set_home_activity"));

        let calls = log.lock().unwrap();
        assert!(calls.iter().any(|c| c.contains("add-role-holder")));
        assert!(calls.iter().any(|c| c.contains("set-home-activity")));
    }

    #[tokio::test]
    async fn set_launcher_reports_failure_with_reason_when_all_strategies_fail() {
        // Role errors out, set-home-activity returns a real error, the resolver
        // never names the target — the result should fail and surface why.
        let mock = MockAdb::default()
            .on_shell_err("add-role-holder", "secure connection refused")
            .on_shell_failure("set-home-activity", "Error: Activity class does not exist");
        let state = state_with(mock);

        let res = set_default_launcher_impl(
            &state,
            "serial",
            "com.example.launcher",
            false,
            &Progress::Silent,
        )
        .await
        .unwrap();

        assert!(!res.ok);
        assert_eq!(res.strategy, None);
        let err = res.last_error.expect("a failure reason");
        assert!(
            err.contains("Activity class does not exist"),
            "unhelpful failure message: {err}"
        );
    }

    #[tokio::test]
    async fn set_launcher_stock_takeover_reverts_when_it_does_not_verify() {
        // Resolver always names the stock launcher, so the takeover never
        // verifies — the stock launcher must be re-enabled (revert).
        let mock = MockAdb::default()
            .on_shell("add-role-holder", "Unknown command")
            .on_shell_failure("set-home-activity", "Error: no such activity")
            .on_shell("resolve-activity", "com.google.android.tvlauncher/.Home");
        let log = mock.shell_log();
        let state = state_with(mock);

        let res = set_default_launcher_impl(
            &state,
            "serial",
            "com.example.launcher",
            true,
            &Progress::Silent,
        )
        .await
        .unwrap();

        assert!(!res.ok);
        let calls = log.lock().unwrap();
        assert!(calls
            .iter()
            .any(|c| c == "pm disable-user --user 0 com.google.android.tvlauncher"));
        assert!(
            calls
                .iter()
                .any(|c| c == "pm enable com.google.android.tvlauncher"),
            "stock launcher should be re-enabled after a failed takeover"
        );
    }

    #[tokio::test]
    async fn set_launcher_from_stock_offers_takeover_without_grinding_the_ladder() {
        // Stock holds HOME and overrides the polite setters (the resolver keeps
        // naming stock even after set-home-activity "Success"). Without the
        // opt-in, the fast path should try the setter once and then surface the
        // takeover immediately — no HOME-intent kick, no stock disabled.
        let mock = MockAdb::default()
            .on_shell("add-role-holder", "Success")
            .on_shell("query-activities", "com.example.launcher/.MainActivity")
            .on_shell("set-home-activity", "Success")
            .on_shell("resolve-activity", "com.google.android.tvlauncher/.Home");
        let log = mock.shell_log();
        let state = state_with(mock);

        let res = set_default_launcher_impl(
            &state,
            "serial",
            "com.example.launcher",
            false,
            &Progress::Silent,
        )
        .await
        .unwrap();

        assert!(!res.ok);
        assert!(
            res.stock_takeover_available,
            "should offer the disable-stock takeover"
        );
        let calls = log.lock().unwrap();
        assert!(
            calls.iter().any(|c| c.contains("set-home-activity")),
            "fast path should try the polite setter once"
        );
        assert!(
            !calls.iter().any(|c| c.contains("am start")),
            "fast path should skip the HOME-intent kick"
        );
        assert!(
            !calls.iter().any(|c| c.contains("disable-user")),
            "must not disable stock without the opt-in"
        );
    }

    #[tokio::test]
    async fn set_launcher_stock_takeover_does_not_revert_when_it_verifies() {
        // Resolver reports the stock launcher until it is disabled, then the
        // target — the takeover verifies, so no revert should be issued.
        let mock = MockAdb::default()
            .on_shell("add-role-holder", "Unknown command")
            .on_shell_failure("set-home-activity", "Error: no such activity")
            .on_shell_seq(
                "resolve-activity",
                &[
                    // stock holds HOME on the fast-path read and the quick check,
                    // then the target after the takeover disables stock.
                    "com.google.android.tvlauncher/.Home",
                    "com.google.android.tvlauncher/.Home",
                    "com.example.launcher/.MainActivity",
                ],
            );
        let log = mock.shell_log();
        let state = state_with(mock);

        let res = set_default_launcher_impl(
            &state,
            "serial",
            "com.example.launcher",
            true,
            &Progress::Silent,
        )
        .await
        .unwrap();

        assert!(res.ok);
        assert_eq!(res.strategy.as_deref(), Some("disable_stock_takeover"));
        let calls = log.lock().unwrap();
        assert!(calls
            .iter()
            .any(|c| c == "pm disable-user --user 0 com.google.android.tvlauncher"));
        assert!(
            !calls
                .iter()
                .any(|c| c == "pm enable com.google.android.tvlauncher"),
            "no revert expected after a verified takeover"
        );
    }

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

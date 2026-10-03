//! Launcher catalog + set-default commands.

use serde::Serialize;
use tauri::State;

use crate::engine::{
    is_last_enabled_home_handler, is_valid_package_name, launcher_rows, paired_transient_holders,
    pick_current_home, HomeReading, LauncherStatus,
};
use crate::license::Feature;

use super::loader::launchers;
use super::{home_tracking, AppState};

/// Public so the desktop diagnostics bundle asks the device the *same*
/// question the launcher tab does — a bug report that used a different query
/// would describe a state the app never saw.
pub const HOME_HANDLER_QUERY: &str =
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
    list_launchers_impl(state.inner(), &serial).await
}

/// Device-free core of `list_launchers` so it can run against a mock driver.
pub async fn list_launchers_impl(
    state: &AppState,
    serial: &str,
) -> Result<Vec<LauncherStatus>, String> {
    // Installed + disabled package lists and HOME handlers in one round-trip.
    // The old `tokio::join!` bought no concurrency on the mobile transport,
    // which serializes every shell behind one connection.
    let adb = state.adb_snapshot().await;
    let cmd = crate::adb::checked_batch_command(&[
        "pm list packages",
        "pm list packages -d",
        HOME_HANDLER_QUERY,
    ]);
    let out = adb
        .shell(serial, &cmd)
        .await
        .map_err(|e| format!("pm list packages: {e}"))?;
    let sections = crate::adb::parse_checked_batch(&out.stdout, 3, &[0, 1])?;

    let installed_pkgs = crate::adb::parse_installed_packages_output(&sections[0]);
    // An empty installed list must not hide every launcher's Enable path.
    if installed_pkgs.is_empty() {
        return Err("pm list packages: no packages reported".to_string());
    }
    let disabled_pkgs = crate::adb::parse_disabled_packages_output(&sections[1]);
    // The HOME query only adds "other handler" rows — an empty section (builds
    // where `cmd package` is limited) degrades to none rather than blanking
    // the whole list.
    if sections[2].is_empty() {
        tracing::warn!("query-activities returned nothing; listing catalog launchers only");
    }
    // HOME handlers only. Every TV app's launch activity declares
    // LEANBACK_LAUNCHER — that category is how an app gets a tile on the home
    // screen, not a claim to *be* the home screen — so listing it turned
    // YouTube, Plex and the Play Store into "Home apps". An app that doesn't
    // declare HOME can still be tried from the Advanced picker, which says
    // honestly when Android won't accept it.
    let handler_pkgs = parse_home_handler_packages(&sections[2]);

    // Disabled handlers don't answer the HOME query — the tracker is what
    // keeps their rows (and the Enable path back) alive. Prune entries that
    // were re-enabled or uninstalled out-of-band.
    let tracked = home_tracking::prune(&state.data_dir, serial, &disabled_pkgs).await;

    Ok(launcher_rows(
        launchers(),
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
    let outcome = async {
        let adb = state.adb_snapshot().await;
        let enabled_handlers = adb
            .shell(&serial, HOME_HANDLER_QUERY)
            .await
            .map(|out| parse_home_handler_packages(&out.stdout))
            .map_err(|e| format!("query-activities: {e}"))?;
        if is_last_enabled_home_handler(&package, &enabled_handlers, launchers()) {
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

        if result.ok && !launchers().contains(&package) {
            home_tracking::record(&data_dir, &serial, &package).await;
        }
        Ok(result)
    }
    .await;
    match &outcome {
        Ok(r) => tracing::info!(%serial, %package, ok = r.ok, "disable launcher finished"),
        Err(e) => {
            tracing::info!(%serial, %package, ok = false, error = %e, "disable launcher finished")
        }
    }
    outcome
}

/// What `set_home_any` observed. It never disables anything, so a refusal
/// here always leaves the TV exactly as it was, plus the target enabled.
#[derive(Serialize)]
pub struct SetHomeAnyResult {
    pub ok: bool,
    /// Active launcher after the attempt; `None` when the device couldn't say.
    pub current_launcher: Option<String>,
    /// Whether the package declares a Home screen. `None` when the device
    /// can't answer the question (`query-activities` is Android 9+) — unknown
    /// claims nothing either way.
    pub declares_home: Option<bool>,
    /// The stock launcher still holds Home after the polite setters. On these
    /// builds only disabling stock hands Home over, which is the separate,
    /// confirmed "Disable stock launcher" step — never done here.
    pub stock_holds_home: bool,
    pub message: String,
    #[serde(default)]
    pub diagnostics: Vec<String>,
}

/// `set_home_any` — the Advanced picker's "set this app as Home". Enables the
/// package and tries the role API and set-home-activity, then verifies. Unlike
/// `set_default_launcher` it has no stock-takeover path at all: picking an app
/// must never imply disabling the stock launcher.
#[tauri::command]
pub async fn set_home_any(
    state: State<'_, AppState>,
    serial: String,
    package: String,
    activity: Option<String>,
) -> Result<SetHomeAnyResult, String> {
    state.require_pro(Feature::LauncherTakeover)?;
    let result = set_home_any_impl(state.inner(), &serial, &package, activity.as_deref()).await;
    match &result {
        Ok(r) => tracing::info!(
            %serial,
            %package,
            ok = r.ok,
            current_launcher = r.current_launcher.as_deref().unwrap_or("unknown"),
            declares_home = ?r.declares_home,
            stock_holds_home = r.stock_holds_home,
            "set home (any app) finished"
        ),
        Err(e) => tracing::info!(
            %serial,
            %package,
            ok = false,
            error = %e,
            "set home (any app) finished"
        ),
    }
    result
}

/// An activity name as the picker may pass it: `.Main`, `Main` or a fully
/// qualified class. Interpolated into a shell command, so strict.
fn is_valid_activity_name(activity: &str) -> bool {
    let body = activity.strip_prefix('.').unwrap_or(activity);
    !body.is_empty()
        && body.split('.').all(|seg| {
            seg.chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
                && seg
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
        })
}

pub async fn set_home_any_impl(
    state: &AppState,
    serial: &str,
    package: &str,
    activity: Option<&str>,
) -> Result<SetHomeAnyResult, String> {
    let mut diagnostics = Vec::new();
    let refuse = |message: String, diagnostics: Vec<String>| SetHomeAnyResult {
        ok: false,
        current_launcher: None,
        declares_home: None,
        stock_holds_home: false,
        message,
        diagnostics,
    };
    if !is_valid_package_name(package) {
        return Ok(refuse(
            format!("Invalid package name: {package:?}"),
            diagnostics,
        ));
    }
    let activity = activity.map(str::trim).filter(|a| !a.is_empty());
    if let Some(a) = activity {
        if !is_valid_activity_name(a) {
            return Ok(refuse(format!("Invalid activity name: {a:?}"), diagnostics));
        }
    }

    let adb = state.adb_snapshot().await;
    let enable_result = adb.shell(serial, &format!("pm enable {package}")).await;
    if let Some(failure) = command_failure(&enable_result) {
        diagnostics.push(format!("pm enable {package} -> {failure}"));
        return Ok(refuse(
            format!("Couldn't enable {package}: {failure}"),
            diagnostics,
        ));
    }
    diagnostics.push(format!("pm enable {package} -> ok"));

    // Does it declare a Home screen? Only a query that listed at least one
    // component answers that; an empty or refused query leaves it unknown.
    let declared = adb
        .shell(
            serial,
            "cmd package query-activities --components -a android.intent.action.MAIN -c android.intent.category.HOME",
        )
        .await
        .ok()
        .filter(|out| out.success() && !out.shell_reported_failure())
        .map(|out| {
            out.stdout
                .lines()
                .map(str::trim)
                .filter(|l| l.contains('/'))
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .filter(|components| !components.is_empty());
    let needle = format!("{package}/");
    let declared_component = declared
        .as_ref()
        .and_then(|c| c.iter().find(|l| l.starts_with(&needle)).cloned());
    let declares_home = declared.as_ref().map(|_| declared_component.is_some());
    diagnostics.push(format!(
        "query-activities HOME -> {}",
        match declares_home {
            Some(true) => "declares Home",
            Some(false) => "does not declare Home",
            None => "unavailable",
        }
    ));

    let mut candidates = Vec::new();
    if let Some(a) = activity {
        let class = if a.starts_with('.') || !a.contains('.') {
            format!(".{}", a.trim_start_matches('.'))
        } else {
            a.to_string()
        };
        candidates.push(format!("{package}/{class}"));
    }
    if let Some(c) = declared_component {
        if !candidates.contains(&c) {
            candidates.push(c);
        }
    }
    if activity.is_none() && declares_home != Some(true) {
        for guess in [
            ".MainActivity",
            ".Main",
            ".LauncherActivity",
            ".HomeActivity",
        ] {
            candidates.push(format!("{package}/{guess}"));
        }
    }

    let role = adb
        .shell(
            serial,
            &format!("cmd role add-role-holder android.app.role.HOME {package}"),
        )
        .await;
    diagnostics.push(match command_failure(&role) {
        Some(failure) => format!("cmd role add-role-holder HOME {package} -> {failure}"),
        None => format!("cmd role add-role-holder HOME {package} -> ok"),
    });
    let setters = try_home_setters(&*adb, serial, &candidates).await;
    diagnostics.extend(setters.attempts.iter().cloned());

    if verify_active(&*adb, serial, package, &mut diagnostics)
        .await
        .confirmed
    {
        focus_home(&*adb, serial, &Progress::Silent).await;
        return Ok(SetHomeAnyResult {
            ok: true,
            current_launcher: Some(package.to_string()),
            declares_home,
            stock_holds_home: false,
            message: format!("{package} is now the Home app."),
            diagnostics,
        });
    }

    let current = read_current_home(&*adb, serial)
        .await
        .ok()
        .and_then(|r| r.package);
    let now = current
        .as_deref()
        .map(|c| format!("Home is still {c}"))
        .unwrap_or_else(|| "Android didn't report which app holds Home".to_string());
    let stock_holds_home =
        declares_home != Some(false) && current.as_deref().is_some_and(|c| launchers().is_stock(c));
    let message = if declares_home == Some(false) {
        format!(
            "Android didn't accept {package} as Home; it doesn't declare a Home screen. \
             Nothing was disabled. {now}."
        )
    } else if stock_holds_home {
        format!(
            "Android took the request, but the stock launcher still holds Home. On this TV \
             only disabling the stock launcher hands Home over; that is the separate \
             \"Disable stock launcher\" step. Nothing was disabled. {now}."
        )
    } else if setters.accepted {
        format!(
            "Android accepted {package} as Home but hasn't switched yet. Press Home on the \
             TV, then Refresh. {now}."
        )
    } else {
        format!("Android didn't accept {package} as Home. Nothing was disabled. {now}.")
    };
    Ok(SetHomeAnyResult {
        ok: false,
        current_launcher: current,
        declares_home,
        stock_holds_home,
        message,
        diagnostics,
    })
}

/// `disable_stock_launcher` — the explicit, confirmed step that hands Home
/// from the stock launcher to `target`. Separate from every "set as Home"
/// action so it is never implied by picking an app.
///
/// Refuses unless `target` is an enabled Home handler, so disabling stock
/// always leaves somewhere for the Home key to land. When stock holds Home it
/// runs the same verify-or-restore takeover `set_default_launcher` uses; when
/// `target` already holds Home it disables the enabled stock launchers behind
/// it and restores them if Home moves.
#[tauri::command]
pub async fn disable_stock_launcher(
    state: State<'_, AppState>,
    serial: String,
    target: String,
) -> Result<SetLauncherResult, String> {
    state.require_pro(Feature::LauncherTakeover)?;
    let result =
        disable_stock_launcher_impl(state.inner(), &serial, &target, &Progress::Silent).await;
    log_launcher_result("disable stock launcher finished", &serial, &target, &result);
    result
}

/// One info line per launcher switch, where the outcome is final.
fn log_launcher_result(
    what: &'static str,
    serial: &str,
    package: &str,
    result: &Result<SetLauncherResult, String>,
) {
    match result {
        Ok(r) => tracing::info!(
            serial,
            package,
            ok = r.ok,
            strategy = r.strategy.as_deref().unwrap_or("none"),
            current_launcher = r.current_launcher.as_deref().unwrap_or("unknown"),
            stock_takeover_available = r.stock_takeover_available,
            "{what}"
        ),
        Err(e) => tracing::info!(serial, package, ok = false, error = %e, "{what}"),
    }
}

pub async fn disable_stock_launcher_impl(
    state: &AppState,
    serial: &str,
    target: &str,
    progress: &Progress,
) -> Result<SetLauncherResult, String> {
    let mut diagnostics = Vec::new();
    let refuse =
        |current: Option<String>, error: String, diagnostics: Vec<String>| SetLauncherResult {
            ok: false,
            strategy: None,
            current_launcher: current,
            last_error: Some(error),
            stock_takeover_available: false,
            diagnostics,
        };
    if !is_valid_package_name(target) {
        return Ok(refuse(
            None,
            format!("Invalid package name: {target:?}"),
            diagnostics,
        ));
    }
    if launchers().is_stock(target) {
        return Ok(refuse(
            None,
            format!("{target} is a stock launcher; pick the app that should take over Home."),
            diagnostics,
        ));
    }
    // Settings declares HOME only as a recovery hatch. Handing Home to it and
    // disabling stock would "verify" and leave the TV with no home screen.
    if crate::engine::launcher::safe_home_handlers().contains(&target) {
        return Ok(refuse(
            None,
            format!(
                "{target} is the Settings recovery fallback, not a launcher; pick a real launcher to take over Home."
            ),
            diagnostics,
        ));
    }

    if launchers().is_transient_home_holder(target) {
        return Ok(refuse(
            None,
            format!(
                "{target} is Google TV's setup helper, not a launcher; pick a real launcher to take over Home."
            ),
            diagnostics,
        ));
    }

    let adb = state.adb_snapshot().await;
    let handlers = match adb.shell(serial, HOME_HANDLER_QUERY).await {
        Ok(out) if out.success() && !out.shell_reported_failure() => {
            parse_home_handler_packages(&out.stdout)
        }
        _ => {
            return Ok(refuse(
                None,
                "Couldn't read this TV's Home apps, so disabling the stock launcher can't be \
                 proven safe. Nothing was disabled."
                    .to_string(),
                diagnostics,
            ))
        }
    };
    diagnostics.push(format!("query-activities HOME -> {}", handlers.join(", ")));

    if !handlers.iter().any(|h| h == target) {
        return Ok(refuse(
            None,
            format!(
                "{target} doesn't declare a Home screen, so disabling the stock launcher would \
                 leave the TV without one. Nothing was disabled."
            ),
            diagnostics,
        ));
    }
    let stocks: Vec<String> = handlers
        .iter()
        .filter(|h| launchers().is_stock(h))
        .filter(|h| {
            !matches!(
                crate::engine::classify_safety(h),
                crate::engine::Safety::NeverDisable { .. }
            )
        })
        .cloned()
        .collect();
    if stocks.is_empty() {
        return Ok(refuse(
            None,
            "No enabled stock launcher to disable.".to_string(),
            diagnostics,
        ));
    }
    // Disabling every stock launcher in turn must never take the last Home.
    let mut remaining = handlers.clone();
    for stock in &stocks {
        if is_last_enabled_home_handler(stock, &remaining, launchers()) {
            return Ok(refuse(
                None,
                format!(
                    "Refusing to disable {stock}: it's the only enabled launcher left on this \
                     device. Nothing was disabled."
                ),
                diagnostics,
            ));
        }
        remaining.retain(|h| h != stock);
    }

    let reading = read_current_home(&*adb, serial).await.unwrap_or_default();
    diagnostics.push(match reading.package.as_deref() {
        Some(a) => format!("current Home -> {a}"),
        None => "current Home -> unavailable".to_string(),
    });
    diagnostics.extend(reading.note.clone());
    let active = reading.package;
    match active.as_deref() {
        Some(a) if stocks.iter().any(|s| s == a) => {
            if let Some(result) =
                stock_takeover(&*adb, serial, target, a, true, progress, &mut diagnostics).await
            {
                return Ok(result);
            }
            Ok(refuse(
                active.clone(),
                format!("{a} can't be disabled safely. Nothing was disabled."),
                diagnostics,
            ))
        }
        Some(a) if a == target => {
            let mut disabled = Vec::new();
            let mut failure = None;
            for stock in &stocks {
                progress.step(&format!("Disabling the stock launcher ({stock})"));
                let result = adb
                    .shell(serial, &format!("pm disable-user --user 0 {stock}"))
                    .await;
                match command_failure(&result) {
                    Some(f) => {
                        diagnostics.push(format!("pm disable-user {stock} -> {f}"));
                        // An errored disable may still have landed; restore it too.
                        disabled.push(stock.clone());
                        failure = Some(format!("Stock-disable command failed for {stock}: {f}"));
                        break;
                    }
                    None => {
                        diagnostics.push(format!("pm disable-user {stock} -> ok"));
                        disabled.push(stock.clone());
                    }
                }
            }
            if failure.is_none()
                && verify_active(&*adb, serial, target, &mut diagnostics)
                    .await
                    .confirmed
            {
                match disable_paired_holders(
                    &*adb,
                    serial,
                    target,
                    &disabled,
                    Some(&handlers),
                    progress,
                    &mut diagnostics,
                )
                .await
                {
                    Ok(()) => {
                        return Ok(SetLauncherResult {
                            ok: true,
                            strategy: Some("disable_stock_takeover".into()),
                            current_launcher: Some(target.to_string()),
                            last_error: None,
                            stock_takeover_available: false,
                            diagnostics,
                        });
                    }
                    Err(reason) => failure = Some(reason),
                }
            }
            for stock in &disabled {
                let restore = adb.shell(serial, &format!("pm enable {stock}")).await;
                diagnostics.push(match command_failure(&restore) {
                    Some(f) => format!("pm enable {stock} (restore) -> {f}"),
                    None => format!("pm enable {stock} (restore) -> ok"),
                });
            }
            let current = read_current_home(&*adb, serial)
                .await
                .ok()
                .and_then(|r| r.package);
            Ok(refuse(
                current,
                format!(
                    "{}. The stock launcher was re-enabled.",
                    failure.unwrap_or_else(|| format!(
                        "Home moved away from {target} after the stock launcher was disabled"
                    ))
                ),
                diagnostics,
            ))
        }
        other => Ok(refuse(
            active.clone(),
            format!(
                "Set {target} as Home first; Home is currently {}. Nothing was disabled.",
                other.unwrap_or("unknown")
            ),
            diagnostics,
        )),
    }
}

#[derive(Serialize)]
pub struct CurrentLauncher {
    pub package: Option<String>,
    pub activity: Option<String>,
    /// Why the answer differs from what `resolve-activity` said, when it does.
    /// For the diagnostics transcript only.
    pub note: Option<String>,
}

#[tauri::command]
pub async fn current_launcher(
    state: State<'_, AppState>,
    serial: String,
) -> Result<CurrentLauncher, String> {
    let adb = state.adb_snapshot().await;
    let reading = read_current_home(&*adb, &serial).await?;
    Ok(CurrentLauncher {
        package: reading.package,
        activity: reading.activity,
        note: reading.note,
    })
}

const RESOLVE_HOME: &str =
    "cmd package resolve-activity --brief -a android.intent.action.MAIN -c android.intent.category.HOME";
const HOME_ROLE_HOLDERS: &str = "cmd role get-role-holders android.app.role.HOME";

/// The current Home app, read the one way every caller uses: the launcher
/// rows' ACTIVE tag, snapshot capture and the restore plan, the disable-stock
/// preconditions and the diagnostics bundle. The HOME role holder wins,
/// unless the resolver names a stock launcher that overrides it (Android 10+);
/// `resolve-activity` is the fallback for builds without the
/// role command. See `engine::pick_current_home` for why.
///
/// `Err` only when neither source could be read at all.
pub async fn read_current_home(
    adb: &dyn crate::adb::AdbDriver,
    serial: &str,
) -> Result<HomeReading, String> {
    let role = home_role_holders(adb, serial).await;
    let component = match adb.shell(serial, RESOLVE_HOME).await {
        Ok(out) if out.success() && !out.shell_reported_failure() => out
            .stdout
            .lines()
            .map(str::trim)
            .find(|l| l.contains('/'))
            .map(str::to_string),
        Ok(_) => None,
        Err(e) if role.is_none() => return Err(format!("resolve-activity: {e}")),
        Err(_) => None,
    };
    let reading = pick_current_home(role.as_deref(), component.as_deref(), launchers());
    if let Some(note) = &reading.note {
        tracing::info!(serial, note = note.as_str(), "current Home read");
    }
    Ok(reading)
}

/// `cmd role get-role-holders android.app.role.HOME`, or `None` when the
/// build has no such command, it failed, or it named no one.
async fn home_role_holders(adb: &dyn crate::adb::AdbDriver, serial: &str) -> Option<Vec<String>> {
    let out = adb.shell(serial, HOME_ROLE_HOLDERS).await.ok()?;
    if !out.success() || out.shell_reported_failure() {
        return None;
    }
    let holders: Vec<String> = out
        .stdout
        .lines()
        .flat_map(|l| l.split(','))
        .map(str::trim)
        .filter(|h| is_valid_package_name(h))
        .map(str::to_string)
        .collect();
    (!holders.is_empty()).then_some(holders)
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
    /// Every command this attempt issued and what the device said back, in
    /// order. Launcher behavior varies enough between builds that a failure
    /// report is only actionable with the per-stage record; the UI offers it
    /// as copyable detail rather than showing it inline.
    #[serde(default)]
    pub diagnostics: Vec<String>,
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
    state.require_pro(Feature::LauncherTakeover)?;
    let result = set_default_launcher_impl(
        state.inner(),
        &serial,
        &package,
        allow_stock_disable.unwrap_or(false),
        &Progress::Channel(on_progress),
    )
    .await;
    log_launcher_result("set default launcher finished", &serial, &package, &result);
    result
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
    // Per-stage record of what was issued and what came back. A launcher
    // failure is only diagnosable with this: the same sequence succeeds on one
    // build and is silently ignored on another, and the difference is only
    // visible in the individual command results (GitHub #87).
    let mut diagnostics: Vec<String> = Vec::new();

    // `package` can come from a custom-launcher entry the user typed, so it's
    // interpolated into shell commands below — validate it first.
    if !is_valid_package_name(package) {
        return Ok(SetLauncherResult {
            ok: false,
            strategy: None,
            current_launcher: None,
            last_error: Some(format!("Invalid package name: {package:?}")),
            stock_takeover_available: false,
            diagnostics,
        });
    }

    // Setup Wraith declares HOME, but it is the setup wizard. Handing Home to
    // it, possibly with the stock launcher disabled, leaves no home screen.
    if launchers().is_transient_home_holder(package) {
        return Ok(SetLauncherResult {
            ok: false,
            strategy: None,
            current_launcher: None,
            last_error: Some(format!(
                "{package} is Google TV's setup helper, not a launcher; it can't be the default."
            )),
            stock_takeover_available: false,
            diagnostics,
        });
    }

    let adb = state.adb_snapshot().await;

    // 1. Enable the package — no-op for already-enabled.
    progress.step("Enabling this launcher");
    let enable_result = adb.shell(serial, &format!("pm enable {package}")).await;
    diagnostics.push(match command_failure(&enable_result) {
        Some(ref failure) => format!("pm enable {package} -> {failure}"),
        None => format!("pm enable {package} -> ok"),
    });
    if let Some(failure) = command_failure(&enable_result) {
        return Ok(SetLauncherResult {
            ok: false,
            strategy: None,
            current_launcher: None,
            last_error: Some(format!("Target enable failed for {package}: {failure}")),
            stock_takeover_available: false,
            diagnostics,
        });
    }

    // Stock fast path: when the launcher currently holding HOME is *stock*, the
    // polite setters can't win — an enabled stock launcher overrides
    // set-home-activity and the role API (they return "Success" but HOME keeps
    // resolving to stock; verified live on Shield / Android 11). So try the
    // cheap setters exactly once, and if HOME still resolves to stock go
    // straight to the opt-in disable-stock takeover rather than grinding the
    // full strategy ladder with its multi-second verify back-offs. Switches
    // between non-stock launchers fall through to the normal ladder below,
    // which works for them.
    let active_before = active_launcher(&*adb, serial).await;
    diagnostics.push(match active_before.as_deref() {
        Some(active) => format!("resolve-activity HOME -> {active}"),
        None => "resolve-activity HOME -> unavailable".to_string(),
    });
    if let Some(active) = active_before.clone() {
        let active_is_stock = launchers().is_stock(&active);
        if active_is_stock && active != package {
            progress.step("Assigning the Home role to it");
            let role_result = adb
                .shell(
                    serial,
                    &format!("cmd role add-role-holder android.app.role.HOME {package}"),
                )
                .await;
            diagnostics.push(match command_failure(&role_result) {
                Some(ref failure) => {
                    format!("cmd role add-role-holder HOME {package} -> {failure}")
                }
                None => format!("cmd role add-role-holder HOME {package} -> ok"),
            });
            progress.step("Registering it as the Home app");
            let candidates = home_activity_candidates(&*adb, serial, package).await;
            let setters = try_home_setters(&*adb, serial, &candidates).await;
            diagnostics.extend(setters.attempts.iter().cloned());
            // When the setters work they take effect immediately; when stock
            // overrides them they never will — one quick check is enough.
            progress.step("Checking whether Home switched over");
            tokio::time::sleep(std::time::Duration::from_millis(400)).await;
            if active_launcher(&*adb, serial).await.as_deref() == Some(package) {
                focus_home(&*adb, serial, progress).await;
                return Ok(SetLauncherResult {
                    ok: true,
                    strategy: Some("set_home_activity".into()),
                    current_launcher: Some(package.to_string()),
                    last_error: None,
                    stock_takeover_available: false,
                    diagnostics,
                });
            }
            if let Some(result) = stock_takeover(
                &*adb,
                serial,
                package,
                &active,
                allow_stock_disable,
                progress,
                &mut diagnostics,
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
            if verify_active(&*adb, serial, package, &mut diagnostics)
                .await
                .confirmed
            {
                focus_home(&*adb, serial, progress).await;
                return Ok(SetLauncherResult {
                    ok: true,
                    strategy: Some("role_api".into()),
                    current_launcher: Some(package.to_string()),
                    last_error: None,
                    stock_takeover_available: false,
                    diagnostics,
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

    // 3. Register the target as the HOME activity. Same candidate list and
    // both command spellings as the stock fast path — one implementation.
    progress.step("Registering it as the Home app");
    let candidates = home_activity_candidates(&*adb, serial, package).await;
    let setters = try_home_setters(&*adb, serial, &candidates).await;
    diagnostics.extend(setters.attempts.iter().cloned());
    if let Some(error) = setters.last_error {
        last_error = Some(error);
    }
    if setters.accepted {
        device_accepted = true;
        if verify_active(&*adb, serial, package, &mut diagnostics)
            .await
            .confirmed
        {
            focus_home(&*adb, serial, progress).await;
            return Ok(SetLauncherResult {
                ok: true,
                strategy: Some("set_home_activity".into()),
                current_launcher: Some(package.to_string()),
                last_error: None,
                stock_takeover_available: false,
                diagnostics,
            });
        }
        // Accepted but the resolver didn't confirm: the preference is now set
        // to a real component of `package`, so the HOME-intent kick below is
        // what finishes it.
    }

    // 4. HOME-intent kick — system will resolve to the only remaining HOME app
    // if everything else got disabled. Verified the same way as the earlier
    // steps (poll + role check) rather than a single quick read, so a device
    // that's still settling through a transient holder (Setup Wraith, #122)
    // gets the same chance to land before this is called a failure.
    progress.step("Switching Home over to it");
    let _ = adb
        .shell(
            serial,
            "am start -W -a android.intent.action.MAIN -c android.intent.category.HOME",
        )
        .await;
    let kick_verification = verify_active(&*adb, serial, package, &mut diagnostics).await;
    if kick_verification.confirmed {
        return Ok(SetLauncherResult {
            ok: true,
            strategy: Some("home_intent_kick".into()),
            current_launcher: Some(package.to_string()),
            last_error: None,
            stock_takeover_available: false,
            diagnostics,
        });
    }
    let now_active = kick_verification.last_active;

    // 5. Last resort: if the launcher still holding HOME is *stock*, disable it
    // (the same takeover the stock fast path uses). This catches the case where
    // HOME wasn't stock at the start but resolved back to it after the polite
    // strategies. Gated, never touches other launchers, attempts to restore on failure.
    if let Some(active) = now_active.clone() {
        if let Some(result) = stock_takeover(
            &*adb,
            serial,
            package,
            &active,
            allow_stock_disable,
            progress,
            &mut diagnostics,
        )
        .await
        {
            return Ok(result);
        }
    }

    // The device acknowledged the change but the resolver never confirmed it.
    // That's "accepted, not yet visible" — common on builds that only apply
    // the preference on the next physical Home press. Say so instead of
    // surfacing the raw "Success" ack as a failure reason. When the resolver
    // is stuck on a transient holder (Setup Wraith, #122) that's worth
    // calling out explicitly — it isn't a stuck state, just Android mid
    // hand-off.
    if device_accepted {
        let holder = now_active.as_deref();
        let settling_note = if holder.is_some_and(|h| launchers().is_transient_home_holder(h)) {
            " That's a transient hand-off screen, not a stuck state — it's normal to see it \
             briefly while Android settles on the new default."
        } else {
            ""
        };
        last_error = Some(format!(
            "The device accepted the launcher change but still reports {} as the active HOME app.{settling_note} \
             Press Home on the TV, then hit Refresh — some devices only switch on the next Home press.",
            holder.unwrap_or("the previous launcher")
        ));
    }

    Ok(SetLauncherResult {
        ok: false,
        strategy: None,
        current_launcher: now_active,
        last_error,
        stock_takeover_available: false,
        diagnostics,
    })
}

/// After `target` is confirmed as Home with `stocks` disabled, also disable
/// the transient HOME holders the catalog pairs with those stock launchers
/// (Setup Wraith on Google TV, #122): with stock gone they take the Home button
/// back. A device with no such holder enabled is untouched.
///
/// Every guard of the stock takeover applies: the do-not-disable gate, the
/// last-Home-handler check, and a second verification that `target` still holds
/// Home. On any failure the holders disabled here are re-enabled before this
/// returns `Err`, and the caller re-enables stock, so the TV is never left
/// switched halfway.
async fn disable_paired_holders(
    adb: &dyn crate::adb::AdbDriver,
    serial: &str,
    target: &str,
    stocks: &[String],
    inventory: Option<&[String]>,
    progress: &Progress,
    diagnostics: &mut Vec<String>,
) -> Result<(), String> {
    if stocks
        .iter()
        .all(|s| launchers().disable_with_for(s).is_empty())
    {
        return Ok(());
    }
    // The Home-handler list read before stock was disabled. Without it the
    // helper can't be checked, and leaving it on with stock off is the #122
    // state, so that is a failure the caller rolls back.
    let Some(enabled) = inventory else {
        diagnostics.push("query-activities HOME (setup helper) -> unavailable".to_string());
        return Err(
            "Couldn't read this TV's Home apps before disabling stock, so the setup helper \
             couldn't be checked"
                .to_string(),
        );
    };
    let mut disabled: Vec<String> = Vec::new();
    let mut failure = None;
    for holder in paired_transient_holders(launchers(), stocks, enabled) {
        if matches!(
            crate::engine::classify_safety(&holder),
            crate::engine::Safety::NeverDisable { .. }
        ) {
            diagnostics.push(format!("{holder} skipped: on the do-not-disable list"));
            continue;
        }
        let remaining: Vec<String> = enabled
            .iter()
            .filter(|h| !stocks.contains(h))
            .cloned()
            .collect();
        if is_last_enabled_home_handler(&holder, &remaining, launchers())
            || !remaining.iter().any(|h| h == target)
        {
            diagnostics.push(format!("{holder} skipped: it would leave no Home app"));
            continue;
        }
        progress.step("Turning off Google TV's setup helper");
        let result = adb
            .shell(serial, &format!("pm disable-user --user 0 {holder}"))
            .await;
        // An errored disable may still have landed; restore it too.
        disabled.push(holder.clone());
        match command_failure(&result) {
            Some(f) => {
                diagnostics.push(format!("pm disable-user {holder} -> {f}"));
                failure = Some(format!(
                    "Setup-helper disable command failed for {holder}: {f}"
                ));
                break;
            }
            None => diagnostics.push(format!("pm disable-user {holder} -> ok")),
        }
    }
    if disabled.is_empty() {
        return Ok(());
    }
    if failure.is_none() {
        let _ = adb
            .shell(
                serial,
                "am start -W -a android.intent.action.MAIN -c android.intent.category.HOME",
            )
            .await;
        if verify_active(adb, serial, target, diagnostics)
            .await
            .confirmed
        {
            return Ok(());
        }
        failure = Some(format!(
            "Home moved away from {target} after the setup helper was disabled"
        ));
    }
    let mut unrestored: Vec<String> = Vec::new();
    for holder in &disabled {
        let restore = adb.shell(serial, &format!("pm enable {holder}")).await;
        diagnostics.push(match command_failure(&restore) {
            Some(f) => {
                unrestored.push(holder.clone());
                format!("pm enable {holder} (restore) -> {f}")
            }
            None => format!("pm enable {holder} (restore) -> ok"),
        });
    }
    let mut reason = failure.unwrap_or_default();
    if !unrestored.is_empty() {
        reason.push_str(&format!(
            ". Setup Wraith may still be off ({}). Use Re-enable Setup Wraith or Emergency Recovery",
            unrestored.join(", ")
        ));
    }
    Err(reason)
}

/// `disable_setup_helper` — the Launcher tab's one-click fix for an enabled
/// transient HOME holder (Setup Wraith) that is live while stock is already
/// disabled. Same shape as the takeover's own step: the do-not-disable gate,
/// the last-Home-handler check, a verification that Home still resolves, and a
/// re-enable if it doesn't.
#[tauri::command]
pub async fn disable_setup_helper(
    state: State<'_, AppState>,
    serial: String,
    package: String,
) -> Result<crate::commands::apps::ActionResult, String> {
    state.require_pro(Feature::LauncherTakeover)?;
    let result = disable_setup_helper_impl(state.inner(), &serial, &package).await;
    match &result {
        Ok(r) => tracing::info!(%serial, %package, ok = r.ok, "disable setup helper finished"),
        Err(e) => {
            tracing::info!(%serial, %package, ok = false, error = %e, "disable setup helper finished")
        }
    }
    result
}

pub async fn disable_setup_helper_impl(
    state: &AppState,
    serial: &str,
    package: &str,
) -> Result<crate::commands::apps::ActionResult, String> {
    let refuse = |message: String| Ok(crate::commands::apps::ActionResult { ok: false, message });
    if !is_valid_package_name(package) || !launchers().is_transient_home_holder(package) {
        return refuse(format!("{package} is not a setup helper this app knows."));
    }
    if matches!(
        crate::engine::classify_safety(package),
        crate::engine::Safety::NeverDisable { .. }
    ) {
        return refuse(format!("{package} is on the do-not-disable list."));
    }
    let adb = state.adb_snapshot().await;
    let enabled = match adb.shell(serial, HOME_HANDLER_QUERY).await {
        Ok(out) if out.success() && !out.shell_reported_failure() => {
            parse_home_handler_packages(&out.stdout)
        }
        _ => {
            return refuse(
                "Couldn't read this TV's Home apps, so turning the setup helper off can't be \
                 proven safe. Nothing was changed."
                    .to_string(),
            )
        }
    };
    if is_last_enabled_home_handler(package, &enabled, launchers()) {
        return refuse(format!(
            "Refusing to disable {package}: no real launcher is enabled, so it would leave the \
             TV without a Home app. Re-enable a launcher first."
        ));
    }
    let result = adb
        .shell(serial, &format!("pm disable-user --user 0 {package}"))
        .await;
    let failed = command_failure(&result);
    let _ = adb
        .shell(
            serial,
            "am start -W -a android.intent.action.MAIN -c android.intent.category.HOME",
        )
        .await;
    let home_ok = failed.is_none()
        && read_current_home(&*adb, serial)
            .await
            .ok()
            .and_then(|r| r.package)
            .is_some_and(|p| p != package);
    if home_ok {
        return Ok(crate::commands::apps::ActionResult {
            ok: true,
            message: "Setup Wraith is off.".to_string(),
        });
    }
    let restore = adb.shell(serial, &format!("pm enable {package}")).await;
    if let Some(f) = command_failure(&restore) {
        return refuse(format!(
            "Couldn't confirm {package} is back on ({f}). Setup Wraith may still be off. Use \
             Re-enable Setup Wraith or Emergency Recovery."
        ));
    }
    refuse(match failed {
        Some(f) => format!("Couldn't disable {package}: {f}. It was left enabled."),
        None => format!(
            "Home didn't stay on a launcher after {package} was disabled, so it was re-enabled."
        ),
    })
}

/// Disable the active *stock* launcher so HOME resolves to `package`. On builds
/// where an enabled stock launcher overrides set-home-activity / the role API
/// (they answer "Success" but HOME keeps resolving to stock — verified live on
/// Shield / Android 11), this is the only switch that sticks. It's v1's proven
/// Launcher-Wizard move: leave the *other* launchers alone, just take stock out
/// of the way. Gated on `allow_stock_disable`; without it, returns
/// `stock_takeover_available` so the UI can confirm. Never disables a
/// NEVER_DISABLE package, and attempts to restore stock if the switch doesn't verify.
/// Returns `None` when `active` isn't a disable-able stock launcher — the
/// caller then falls through to its normal failure path.
async fn stock_takeover(
    adb: &dyn crate::adb::AdbDriver,
    serial: &str,
    package: &str,
    active: &str,
    allow_stock_disable: bool,
    progress: &Progress,
    diagnostics: &mut Vec<String>,
) -> Option<SetLauncherResult> {
    let active_is_stock = launchers().is_stock(active);
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
            diagnostics: std::mem::take(diagnostics),
        });
    }
    let inventory = if launchers().disable_with_for(active).is_empty() {
        Some(Vec::new())
    } else {
        match adb.shell(serial, HOME_HANDLER_QUERY).await {
            Ok(out) if out.success() && !out.shell_reported_failure() => {
                Some(parse_home_handler_packages(&out.stdout))
            }
            _ => None,
        }
    };
    progress.step(&format!(
        "Disabling the stock launcher ({active}) to hand Home over"
    ));
    let disable_result = adb
        .shell(serial, &format!("pm disable-user --user 0 {active}"))
        .await;
    diagnostics.push(match command_failure(&disable_result) {
        Some(ref failure) => format!("pm disable-user {active} -> {failure}"),
        None => format!("pm disable-user {active} -> ok"),
    });
    let primary_failure = if let Some(failure) = command_failure(&disable_result) {
        format!("Stock-disable command failed for {active}: {failure}")
    } else {
        let _ = adb
            .shell(
                serial,
                "am start -W -a android.intent.action.MAIN -c android.intent.category.HOME",
            )
            .await;
        progress.step("Checking whether Home switched over");
        let verification = verify_active(adb, serial, package, diagnostics).await;
        let mut paired_failure = None;
        if verification.confirmed {
            match disable_paired_holders(
                adb,
                serial,
                package,
                &[active.to_string()],
                inventory.as_deref(),
                progress,
                diagnostics,
            )
            .await
            {
                Ok(()) => {
                    return Some(SetLauncherResult {
                        ok: true,
                        strategy: Some("disable_stock_takeover".into()),
                        current_launcher: Some(package.to_string()),
                        last_error: None,
                        stock_takeover_available: false,
                        diagnostics: std::mem::take(diagnostics),
                    });
                }
                Err(reason) => paired_failure = Some(reason),
            }
        }
        // Stock only gets re-enabled below when it *positively* still holds
        // HOME at the end of the window, or when the resolver never answered
        // at all — an unavailable resolver proves nothing, so it is treated
        // like "still stock", not like a real replacement. Only a holder we
        // actually observed and that isn't stock itself — the target (a race
        // with the last poll), a transient holder still settling, or some
        // third app — means disabling stock is not what's keeping the switch
        // from landing, so restoring it would only undo real progress
        // (GitHub #122: the onn 4K Pro reported this as a failure and rolled
        // stock back even though the switch had actually worked).
        if let Some(reason) = paired_failure {
            reason
        } else if let Some(holder) = verification
            .last_active
            .as_deref()
            .filter(|holder| *holder != active)
        {
            let settling = launchers().is_transient_home_holder(holder);
            let holder_note = if settling {
                format!(" (it currently shows {holder}, a transient hand-off screen)")
            } else {
                format!(" (it currently shows {holder})")
            };
            return Some(SetLauncherResult {
                ok: false,
                strategy: None,
                current_launcher: verification.last_active.clone(),
                last_error: Some(format!(
                    "{active} was disabled but Android hasn't confirmed {package} as the Home \
                     app yet{holder_note}. Stock was left disabled rather than restored, since it \
                     no longer holds Home. Press Home on the TV, then hit Refresh."
                )),
                stock_takeover_available: false,
                diagnostics: std::mem::take(diagnostics),
            });
        } else {
            format!(
                "Takeover verification failed: Android did not report {package} as the Home app after the stock-disable command completed for {active}"
            )
        }
    };

    // A transport or shell error cannot prove that the disable had no effect.
    // Once issued, every non-verified takeover attempts to restore this same stock package.
    progress.step("Restoring the stock launcher");
    let restore_result = adb.shell(serial, &format!("pm enable {active}")).await;
    diagnostics.push(match command_failure(&restore_result) {
        Some(ref failure) => format!("pm enable {active} (restore) -> {failure}"),
        None => format!("pm enable {active} (restore) -> ok"),
    });
    let restoration = match command_failure(&restore_result) {
        Some(failure) => format!("Stock-restore command failed for {active}: {failure}"),
        None => format!(
            "Restore command completed for {active}; enabled state was not independently verified"
        ),
    };
    let current = active_launcher(adb, serial).await;
    let observation = match current.as_deref() {
        Some(observed) => format!("Android reports {observed} as the Home app"),
        None => "Android's Home resolver was unavailable after the restore attempt".to_string(),
    };

    Some(SetLauncherResult {
        ok: false,
        strategy: None,
        current_launcher: current,
        last_error: Some(format!("{primary_failure}. {restoration}. {observation}.")),
        stock_takeover_available: false,
        diagnostics: std::mem::take(diagnostics),
    })
}

fn command_failure(result: &crate::adb::AdbResult<crate::adb::AdbOutput>) -> Option<String> {
    match result {
        Err(error) => Some(format!("ADB call failed: {error}")),
        Ok(output) if !output.success() => {
            let detail = command_output_detail(output);
            // `{:?}` on an Option renders as `Some(13)` / `None`, and this
            // string goes straight into the launcher error banner and a
            // confirm() body. Users were reading "exit code 13".
            let code = match output.exit_code {
                Some(code) => format!("exit code {code}"),
                None => "no exit code".to_string(),
            };
            Some(format!("the command failed ({code}){detail}"))
        }
        Ok(output) if output.shell_reported_failure() => Some(format!(
            "device reported failure: {}",
            command_output_detail(output).trim_start_matches(": ")
        )),
        Ok(_) => None,
    }
}

fn command_output_detail(output: &crate::adb::AdbOutput) -> String {
    let detail = output.combined().trim().to_string();
    if detail.is_empty() {
        String::new()
    } else {
        format!(": {detail}")
    }
}

/// `cmd package set-home-activity` / `pm set-home-activity` acknowledge with a
/// bare "Success" line on many builds (and stay silent on others). That's an
/// acceptance, not a diagnostic — surfacing it as an error produced the
/// infamous "Failed: Success" message.
fn is_success_ack(s: &str) -> bool {
    s.trim().eq_ignore_ascii_case("success")
}

/// Foreground the now-default launcher so the TV actually shows it the moment
/// we report success — otherwise the screen sits on whatever was up (or the
/// screensaver) until the user presses Home. HOME resolves to the default we
/// just set, so this brings the new launcher forward. Fire-and-forget; the
/// takeover and HOME-intent-kick paths already do this inline.
async fn focus_home(adb: &dyn crate::adb::AdbDriver, serial: &str, progress: &Progress) {
    progress.step("Opening the new launcher");
    let _ = adb
        .shell(
            serial,
            "am start -a android.intent.action.MAIN -c android.intent.category.HOME",
        )
        .await;
}

/// Backoff schedule for [`verify_active`], summing to ~5s — long enough for
/// Android to settle after a role/set-home-activity change on real hardware
/// (GitHub #122: the onn 4K Pro hadn't finished switching within the old
/// ~1.6s window). Tiny under `cfg(test)` so the polling loop doesn't make the
/// test suite slow.
#[cfg(not(test))]
fn verify_poll_delays() -> &'static [u64] {
    &[200, 300, 500, 800, 1200, 2000]
}
#[cfg(test)]
fn verify_poll_delays() -> &'static [u64] {
    &[1, 1, 1, 1, 1, 1]
}

/// What a `verify_active` poll established.
struct VerifyOutcome {
    /// `true` once the resolver or the HOME role named `package`.
    confirmed: bool,
    /// Whatever the last poll observed HOME resolving to, when not
    /// confirmed — so a caller deciding whether to roll back a takeover
    /// doesn't have to re-query (and risk a different, racier answer than
    /// what was actually logged).
    last_active: Option<String>,
}

/// Poll the active-HOME resolver — and the HOME role's holder list — until
/// one names `package`, with backoff. Propagation after set-home-activity /
/// role changes isn't instant on every build — a single immediate check
/// produced false "failed" results even when the device had accepted the
/// change.
///
/// A device may hand HOME through a transient holder while it settles on the
/// real default (Google TV's Setup Wraith, GitHub #122) — seeing one here is
/// "not decided yet", so the loop just keeps polling through it rather than
/// treating it as a mismatch. Every check made is appended to `diagnostics`.
async fn verify_active(
    adb: &dyn crate::adb::AdbDriver,
    serial: &str,
    package: &str,
    diagnostics: &mut Vec<String>,
) -> VerifyOutcome {
    let mut last_active = None;
    for delay_ms in verify_poll_delays() {
        tokio::time::sleep(std::time::Duration::from_millis(*delay_ms)).await;
        if role_names_target(adb, serial, package, diagnostics).await {
            return VerifyOutcome {
                confirmed: true,
                last_active: Some(package.to_string()),
            };
        }
        let active = active_launcher(adb, serial).await;
        diagnostics.push(match active.as_deref() {
            Some(a) if a == package => format!("resolve-activity HOME -> {a}"),
            Some(a) if launchers().is_transient_home_holder(a) => {
                format!("resolve-activity HOME -> {a} (transient, still settling)")
            }
            Some(a) => format!("resolve-activity HOME -> {a}"),
            None => "resolve-activity HOME -> unavailable".to_string(),
        });
        if active.as_deref() == Some(package) {
            return VerifyOutcome {
                confirmed: true,
                last_active: active,
            };
        }
        last_active = active;
    }
    VerifyOutcome {
        confirmed: false,
        last_active,
    }
}

/// True when `cmd role get-role-holders android.app.role.HOME` names
/// `package` as a current holder. Some builds update the role's holder list
/// before the HOME resolver catches up, so this is an earlier, independent
/// success signal alongside `resolve-activity` — never a replacement for it,
/// since older builds don't support the role command at all.
async fn role_names_target(
    adb: &dyn crate::adb::AdbDriver,
    serial: &str,
    package: &str,
    diagnostics: &mut Vec<String>,
) -> bool {
    let out = adb.shell(serial, HOME_ROLE_HOLDERS).await;
    match out {
        Ok(o) if o.success() && !o.shell_reported_failure() => {
            let holders = o.stdout.trim();
            let matched = holders
                .lines()
                .flat_map(|l| l.split(','))
                .map(str::trim)
                .any(|h| h == package);
            diagnostics.push(format!(
                "cmd role get-role-holders HOME -> {}",
                if holders.is_empty() {
                    "(none)"
                } else {
                    holders
                }
            ));
            matched
        }
        Ok(o) => {
            diagnostics.push(format!(
                "cmd role get-role-holders HOME -> {}",
                command_output_detail(&o).trim_start_matches(": ")
            ));
            false
        }
        Err(_) => false,
    }
}

/// The package `resolve-activity` names for HOME now, or `None` when the
/// device couldn't say. Only the switch and verify logic reads the resolver
/// alone, because an enabled stock launcher that overrides the role shows up
/// there; anything reporting the current launcher uses `read_current_home`.
async fn active_launcher(adb: &dyn crate::adb::AdbDriver, serial: &str) -> Option<String> {
    let out = adb.shell(serial, RESOLVE_HOME).await.ok()?;
    if !out.success() || out.shell_reported_failure() {
        return None;
    }
    out.stdout
        .lines()
        .map(str::trim)
        .find(|l| l.contains('/'))
        .and_then(|c| c.split_once('/'))
        .map(|(p, _)| p.to_string())
}

/// Find a HOME activity for `package` via `cmd package query-activities`.
/// Returns a `pkg/activity` component string ready for set-home-activity.
/// Components to try registering as the HOME activity, best first.
///
/// The discovered component is authoritative when we can get one, but
/// `discover_home_activity` depends on `cmd package query-activities`, which
/// does not exist before Android 9. On older builds it returns nothing, so the
/// conventional entry-point names are the only thing left to try — without
/// them, such a device never gets a set-home-activity call at all.
async fn home_activity_candidates(
    adb: &dyn crate::adb::AdbDriver,
    serial: &str,
    package: &str,
) -> Vec<String> {
    let mut candidates: Vec<String> = Vec::new();
    if let Some(activity) = discover_home_activity(adb, serial, package).await {
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
    candidates
}

/// What a run of the set-home-activity ladder observed.
#[derive(Default)]
struct HomeSetterOutcome {
    /// The device acknowledged one of the commands ("Success" or a clean
    /// silent exit). That is acceptance, not proof that HOME moved.
    accepted: bool,
    /// Real error text from the last command that reported one.
    last_error: Option<String>,
    /// Each command issued and what it reported, for the diagnostic report.
    attempts: Vec<String>,
}

/// Try `set-home-activity` over `candidates`, both the `cmd package` and the
/// older `pm` spelling, stopping at the first acknowledgement.
///
/// Both spellings matter: `cmd package` is the modern one, `pm` is what older
/// builds answer to, and which of the two works is exactly the kind of thing
/// that differs between the devices in bug reports.
async fn try_home_setters(
    adb: &dyn crate::adb::AdbDriver,
    serial: &str,
    candidates: &[String],
) -> HomeSetterOutcome {
    let mut outcome = HomeSetterOutcome::default();
    for comp in candidates {
        for cmd in [
            format!("cmd package set-home-activity --user 0 {comp}"),
            format!("pm set-home-activity --user 0 {comp}"),
        ] {
            let Ok(out) = adb.shell(serial, &cmd).await else {
                outcome.attempts.push(format!("{cmd} -> transport error"));
                continue;
            };
            // set-home-activity prints a bare "Success" on many builds and
            // nothing on others — both are acceptance, NOT diagnostics. Only
            // real error text goes into last_error; recording the ack produced
            // "Failed: Success".
            let msg = if out.stderr.trim().is_empty() {
                out.stdout.trim()
            } else {
                out.stderr.trim()
            };
            if is_success_ack(msg) || msg.is_empty() {
                outcome.accepted = true;
                outcome.attempts.push(format!("{cmd} -> accepted"));
                // Accepted: the preference now points at a real component of
                // this package. Running the remaining guesses could only
                // overwrite it with a worse one.
                return outcome;
            }
            outcome.attempts.push(format!("{cmd} -> {msg}"));
            outcome.last_error = Some(msg.to_string());
        }
    }
    outcome
}

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
///
/// Public so desktop's diagnostics bundle (`commands::diagnostics` in the
/// `shield-optimizer-v2` crate) reuses this parser instead of assuming
/// `HOME_HANDLER_QUERY`'s `name=` field is a flattened `pkg/activity`
/// component — it isn't; `name=` is the bare class and `packageName=` is the
/// separate field this parses.
pub fn parse_home_handler_packages(stdout: &str) -> Vec<String> {
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
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;

    use crate::adb::{AdbDriver, AdbError, AdbOutput, AdbResult, BATCH_SEPARATOR};
    use crate::commands::test_support::{state_with, MockAdb};
    use crate::engine::AppListBundle;

    struct ShellStep {
        command: &'static str,
        result: AdbResult<AdbOutput>,
    }

    struct ScriptedAdb {
        steps: Mutex<VecDeque<ShellStep>>,
        log: Arc<Mutex<Vec<String>>>,
    }

    impl ScriptedAdb {
        fn new(steps: Vec<ShellStep>) -> (Self, Arc<Mutex<Vec<String>>>) {
            let log = Arc::new(Mutex::new(Vec::new()));
            (
                Self {
                    steps: Mutex::new(steps.into()),
                    log: Arc::clone(&log),
                },
                log,
            )
        }
    }

    #[async_trait]
    impl AdbDriver for ScriptedAdb {
        async fn raw(&self, _args: &[&str]) -> AdbResult<AdbOutput> {
            Err(AdbError::Unsupported {
                operation: "launcher test raw",
            })
        }

        async fn shell(&self, _serial: &str, command: &str) -> AdbResult<AdbOutput> {
            self.log.lock().unwrap().push(command.to_string());
            let step = self
                .steps
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| panic!("unexpected shell command: {command}"));
            assert_eq!(command, step.command);
            step.result
        }
    }

    fn adb_output(stdout: &str, stderr: &str, exit_code: Option<i32>) -> AdbOutput {
        AdbOutput {
            stdout: stdout.to_string(),
            stderr: stderr.to_string(),
            exit_code,
        }
    }

    fn state_with_script(script: ScriptedAdb) -> AppState {
        AppState::new(
            Arc::new(script),
            AppListBundle::default(),
            std::env::temp_dir(),
        )
        .with_entitlement(crate::license::Entitlement::Pro)
    }

    /// Device output for a batched shell: sections joined by the sentinel the
    /// device would echo between sub-commands.
    fn batched(sections: &[&str]) -> String {
        sections
            .iter()
            .map(|s| format!("{s}\n{}0\n", crate::adb::batch::BATCH_STATUS))
            .collect::<Vec<_>>()
            .join(&format!("\n{BATCH_SEPARATOR}\n"))
    }

    #[tokio::test]
    async fn invalid_target_package_runs_no_commands() {
        let mock = MockAdb::default();
        let log = mock.shell_log();
        let state = state_with(mock);

        let result = set_default_launcher_impl(
            &state,
            "serial",
            "bad package; reboot",
            true,
            &Progress::Silent,
        )
        .await
        .expect("invalid package is a result, not a transport error");

        assert!(!result.ok);
        assert_eq!(result.strategy, None);
        assert_eq!(result.current_launcher, None);
        assert!(!result.stock_takeover_available);
        assert!(result
            .last_error
            .as_deref()
            .is_some_and(|error| error.contains("Invalid package name")));
        assert!(log.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn target_enable_failures_stop_before_any_launcher_mutation() {
        let cases = [
            (
                "transport",
                Err(AdbError::Transport("USB link dropped".into())),
                "ADB call failed",
            ),
            (
                "stdout",
                Ok(adb_output(
                    "Failure [not installed for user 0]",
                    "",
                    Some(0),
                )),
                "device reported failure",
            ),
            (
                "stderr",
                Ok(adb_output("", "Error: package unavailable", Some(0))),
                "device reported failure",
            ),
            (
                "nonzero",
                Ok(adb_output("", "permission denied", Some(13))),
                "exit code 13",
            ),
            (
                "unknown-status",
                Ok(adb_output("", "shell status unavailable", None)),
                "no exit code",
            ),
        ];

        for (name, enable_result, expected_error) in cases {
            let command = "pm enable com.example.launcher";
            let (script, log) = ScriptedAdb::new(vec![ShellStep {
                command,
                result: enable_result,
            }]);
            let state = state_with_script(script);

            let result = set_default_launcher_impl(
                &state,
                "serial",
                "com.example.launcher",
                true,
                &Progress::Silent,
            )
            .await
            .unwrap_or_else(|error| panic!("{name}: unexpected outer error: {error}"));

            assert!(!result.ok, "{name}");
            assert_eq!(result.strategy, None, "{name}");
            assert_eq!(result.current_launcher, None, "{name}");
            assert!(!result.stock_takeover_available, "{name}");
            let error = result.last_error.expect("stage-specific error");
            assert!(error.contains("Target enable failed"), "{name}: {error}");
            assert!(error.contains(expected_error), "{name}: {error}");
            assert_eq!(&*log.lock().unwrap(), &[command.to_string()], "{name}");
        }
    }

    #[tokio::test]
    async fn stock_disable_uncertainty_restores_once_and_reports_unavailable_home() {
        let stock = "com.google.android.tvlauncher";
        let target = "com.example.launcher";
        let disable = "pm disable-user --user 0 com.google.android.tvlauncher";
        let restore = "pm enable com.google.android.tvlauncher";
        let resolve = "cmd package resolve-activity --brief -a android.intent.action.MAIN -c android.intent.category.HOME";
        let (script, log) = ScriptedAdb::new(vec![
            ShellStep {
                command: disable,
                result: Err(AdbError::Transport("reply lost".into())),
            },
            ShellStep {
                command: restore,
                result: Ok(adb_output("Package enabled", "", Some(0))),
            },
            ShellStep {
                command: resolve,
                result: Ok(adb_output(
                    "com.stale.cached/.Home",
                    "resolver command failed",
                    Some(1),
                )),
            },
        ]);

        let result = stock_takeover(
            &script,
            "serial",
            target,
            stock,
            true,
            &Progress::Silent,
            &mut Vec::new(),
        )
        .await
        .expect("stock takeover result");

        assert!(!result.ok);
        assert_eq!(result.current_launcher, None);
        let error = result.last_error.expect("failure details");
        assert!(error.contains("Stock-disable command failed"), "{error}");
        assert!(error.contains("Restore command completed"), "{error}");
        assert!(error.contains("resolver was unavailable"), "{error}");
        assert_eq!(
            &*log.lock().unwrap(),
            &[
                disable.to_string(),
                restore.to_string(),
                resolve.to_string()
            ]
        );
    }

    #[tokio::test]
    async fn restore_failure_variants_keep_primary_failure_and_fresh_home_observation() {
        let stock = "com.google.android.tvlauncher";
        let target = "com.example.launcher";
        let other = "com.vendor.recoveryhome";
        let disable = "pm disable-user --user 0 com.google.android.tvlauncher";
        let restore = "pm enable com.google.android.tvlauncher";
        let resolve = "cmd package resolve-activity --brief -a android.intent.action.MAIN -c android.intent.category.HOME";
        let cases = [
            (
                "transport",
                Err(AdbError::Transport("restore reply lost".into())),
                "ADB call failed",
                "restore reply lost",
            ),
            (
                "nonzero",
                Ok(adb_output("", "restore permission denied", Some(13))),
                "exit code 13",
                "restore permission denied",
            ),
            (
                "unknown-status",
                Ok(adb_output("", "restore status unavailable", None)),
                "no exit code",
                "restore status unavailable",
            ),
            (
                "stdout",
                Ok(adb_output("Failure [restore rejected]", "", Some(0))),
                "device reported failure",
                "Failure [restore rejected]",
            ),
            (
                "stderr",
                Ok(adb_output("", "Error: restore rejected", Some(0))),
                "device reported failure",
                "Error: restore rejected",
            ),
        ];

        for (name, restore_result, expected_restore_error, expected_restore_detail) in cases {
            let (script, log) = ScriptedAdb::new(vec![
                ShellStep {
                    command: disable,
                    result: Ok(adb_output("Failure [transport reset]", "", Some(0))),
                },
                ShellStep {
                    command: restore,
                    result: restore_result,
                },
                ShellStep {
                    command: resolve,
                    result: Ok(adb_output(
                        "com.vendor.recoveryhome/.HomeActivity",
                        "",
                        Some(0),
                    )),
                },
            ]);

            let result = stock_takeover(
                &script,
                "serial",
                target,
                stock,
                true,
                &Progress::Silent,
                &mut Vec::new(),
            )
            .await
            .unwrap_or_else(|| panic!("{name}: stock takeover result"));

            assert!(!result.ok, "{name}");
            assert_eq!(result.current_launcher.as_deref(), Some(other), "{name}");
            let error = result.last_error.expect("failure details");
            assert!(
                error.contains("Stock-disable command failed")
                    && error.contains("Failure [transport reset]"),
                "{name}: {error}"
            );
            assert!(
                error.contains("Stock-restore command failed"),
                "{name}: {error}"
            );
            assert!(
                error.contains(expected_restore_error) && error.contains(expected_restore_detail),
                "{name}: {error}"
            );
            assert!(
                !error.contains("Restore command completed")
                    && !error.contains("was restored")
                    && !error.contains("was re-enabled"),
                "{name}: {error}"
            );
            assert!(
                error.contains("Android reports com.vendor.recoveryhome"),
                "{name}: {error}"
            );
            assert_eq!(
                &*log.lock().unwrap(),
                &[
                    disable.to_string(),
                    restore.to_string(),
                    resolve.to_string()
                ],
                "{name}"
            );
        }
    }

    #[tokio::test]
    async fn list_launchers_reads_every_section_from_one_batched_call() {
        // installed / disabled / HOME handlers in one round-trip. A rule per sub-command would match the whole compound
        // command, so the mock answers the sentinel with the concatenated
        // sections.
        let mock = MockAdb::default().on_shell(
            BATCH_SEPARATOR,
            &batched(&[
                "package:com.google.android.tvlauncher\n\
                 package:com.spocky.projengmenu\n\
                 package:com.example.otherhome\n\
                 package:com.example.leanbackonly",
                "package:com.spocky.projengmenu",
                "    packageName=com.example.otherhome",
            ]),
        );
        let log = mock.shell_log();
        let state = state_with(mock);

        let rows = list_launchers_impl(&state, "list-launchers-serial")
            .await
            .unwrap_or_else(|e| panic!("launcher rows: {e}"));

        let stock = rows
            .iter()
            .find(|r| r.entry.package == "com.google.android.tvlauncher")
            .expect("stock launcher row");
        assert!(stock.installed && stock.enabled && stock.stock);

        let projectivy = rows
            .iter()
            .find(|r| r.entry.package == "com.spocky.projengmenu")
            .expect("catalog launcher row");
        assert!(
            projectivy.installed && !projectivy.enabled,
            "the disabled section must reach the rows"
        );

        let other = rows
            .iter()
            .find(|r| r.entry.package == "com.example.otherhome")
            .expect("non-catalog HOME handler row");
        assert!(other.other && other.enabled);

        let calls = log.lock().unwrap();
        assert_eq!(
            calls.len(),
            1,
            "installed + disabled + the HOME query must cost one round-trip: {calls:?}"
        );
    }

    #[tokio::test]
    async fn a_leanback_only_app_is_not_listed_as_a_home_app() {
        // Every TV app (YouTube, Plex, the Play Store) declares
        // LEANBACK_LAUNCHER so it gets a tile on the home screen. Treating that
        // as a Home handler flooded the Launcher tab with "HOME APP" rows for
        // ordinary apps. It is installed and enabled, but it must not get a row,
        // and the leanback category must not be queried at all.
        let mock = MockAdb::default().on_shell(
            BATCH_SEPARATOR,
            &batched(&[
                "package:com.google.android.tvlauncher\n\
                 package:com.plexapp.android",
                "",
                "    packageName=com.google.android.tvlauncher",
            ]),
        );
        let log = mock.shell_log();
        let state = state_with(mock);

        let rows = list_launchers_impl(&state, "leanback-only-serial")
            .await
            .unwrap_or_else(|e| panic!("launcher rows: {e}"));
        assert!(
            !rows
                .iter()
                .any(|r| r.entry.package == "com.plexapp.android"),
            "a leanback-only app must not be listed as a Home app: {:?}",
            rows.iter().map(|r| &r.entry.package).collect::<Vec<_>>()
        );
        assert!(!rows.iter().any(|r| r.other));
        let calls = log.lock().unwrap();
        assert!(
            !calls.iter().any(|c| c.contains("LEANBACK_LAUNCHER")),
            "the leanback entry point is not a Home screen and must not be queried as one: {calls:?}"
        );
    }

    #[tokio::test]
    async fn list_launchers_degrades_when_the_home_query_is_empty() {
        let mock = MockAdb::default().on_shell(
            BATCH_SEPARATOR,
            &batched(&["package:com.google.android.tvlauncher", "", ""]),
        );
        let state = state_with(mock);

        let rows = list_launchers_impl(&state, "list-launchers-degraded")
            .await
            .unwrap_or_else(|e| panic!("rows despite an empty HOME query: {e}"));
        assert!(rows
            .iter()
            .any(|r| r.entry.package == "com.google.android.tvlauncher"));
        assert!(!rows.iter().any(|r| r.other));
    }

    #[tokio::test]
    async fn list_launchers_rejects_failed_disabled_reads_before_pruning_tracking() {
        let status = crate::adb::batch::BATCH_STATUS;
        let output = format!(
            "package:com.example\n{status}0\n{BATCH_SEPARATOR}\n{status}1\n{BATCH_SEPARATOR}\n{status}0\n"
        );
        let state = state_with(MockAdb::default().on_shell(BATCH_SEPARATOR, &output));
        assert!(list_launchers_impl(&state, "serial").await.is_err());
    }

    #[tokio::test]
    async fn list_launchers_errors_when_the_installed_section_is_empty() {
        // Otherwise every launcher renders as not-installed and the Enable
        // path back disappears.
        let state =
            state_with(MockAdb::default().on_shell(BATCH_SEPARATOR, &batched(&["", "", ""])));
        let err = match list_launchers_impl(&state, "list-launchers-empty").await {
            Ok(rows) => panic!(
                "empty package listing must be an error, got {} rows",
                rows.len()
            ),
            Err(e) => e,
        };
        assert!(err.contains("pm list packages"), "unhelpful error: {err}");
    }

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
        // The HOME-intent *kick* (am start -W) must not run; the plain `am start`
        // that foregrounds the launcher on success is expected.
        assert!(!calls.iter().any(|c| c.contains("am start -W")));
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
        // verifies — the stock restore command must be attempted.
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
        assert_eq!(
            res.current_launcher.as_deref(),
            Some("com.google.android.tvlauncher")
        );
        let error = res.last_error.as_deref().expect("takeover error");
        assert!(error.contains("Takeover verification failed"), "{error}");
        assert!(error.contains("Restore command completed"), "{error}");
        assert!(
            !error.contains("was re-enabled"),
            "must not claim an unverified enabled state: {error}"
        );
        let calls = log.lock().unwrap();
        assert!(calls
            .iter()
            .any(|c| c == "pm disable-user --user 0 com.google.android.tvlauncher"));
        assert!(
            calls
                .iter()
                .any(|c| c == "pm enable com.google.android.tvlauncher"),
            "stock restore command should be attempted after a failed takeover"
        );
        assert_eq!(
            calls
                .iter()
                .filter(|command| *command == "pm enable com.google.android.tvlauncher")
                .count(),
            1,
            "restore exactly the stock package once"
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

    /// GitHub #87 — Sony XBR-55X850D, Android 8.0, stock holding HOME.
    ///
    /// `cmd package query-activities` is Android 9+ and `cmd role` is Android
    /// 10+, so on this device both come back "Unknown command". The stock fast
    /// path used to call set-home-activity only for a component
    /// `query-activities` had found, which meant no setter ran at all and the
    /// first thing that actually happened was disabling the stock launcher —
    /// handing HOME to a package nothing had registered. It has to try the
    /// conventional component names before it touches stock.
    #[tokio::test]
    async fn android_8_registers_home_before_disabling_stock() {
        let mock = MockAdb::default()
            .on_shell("add-role-holder", "Unknown command: role")
            .on_shell("query-activities", "Unknown command: package")
            .on_shell("set-home-activity", "Success")
            .on_shell("resolve-activity", "com.google.android.tvlauncher/.Home");
        let log = mock.shell_log();
        let state = state_with(mock);

        let res = set_default_launcher_impl(
            &state,
            "serial",
            "com.spocky.projengmenu",
            false,
            &Progress::Silent,
        )
        .await
        .unwrap();

        let calls = log.lock().unwrap().clone();
        let setter_index = calls.iter().position(|c| c.contains("set-home-activity"));
        assert!(
            setter_index.is_some(),
            "no set-home-activity ran at all — this is the #87 failure: {calls:?}"
        );
        // A guessed component is the only thing available when discovery is
        // unsupported, and it must name the target package.
        assert!(
            calls
                .iter()
                .any(|c| c.contains("set-home-activity") && c.contains("com.spocky.projengmenu")),
            "the setter must target the requested launcher: {calls:?}"
        );
        // Ordering is the point: never disable stock before trying to register.
        if let Some(disable_index) = calls.iter().position(|c| c.contains("disable-user")) {
            assert!(
                setter_index.unwrap() < disable_index,
                "stock was disabled before anything registered HOME: {calls:?}"
            );
        }
        assert!(!res.ok);
        assert!(res.stock_takeover_available);
    }

    /// Both spellings matter: `cmd package` is modern, `pm` is what older
    /// builds answer to. Trying only one is how a device ends up with no
    /// registration at all.
    #[tokio::test]
    async fn a_rejected_cmd_package_setter_still_tries_the_pm_spelling() {
        // Non-stock holds HOME, so this takes the general ladder. The modern
        // `cmd package` spelling is refused; the older `pm` one works. Trying
        // only the first is how a device ends up with nothing registered.
        let mock = MockAdb::default()
            .on_shell("add-role-holder", "Unknown command")
            .on_shell("query-activities", "Unknown command")
            .on_shell(
                "cmd package set-home-activity",
                "Error: Unknown command: package",
            )
            .on_shell("pm set-home-activity", "Success")
            .on_shell_seq(
                "resolve-activity",
                &[
                    "com.example.other/.Home",
                    "com.example.launcher/.MainActivity",
                ],
            );
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

        assert!(res.ok, "the pm spelling succeeded: {:?}", res.last_error);
        let calls = log.lock().unwrap();
        assert!(
            calls
                .iter()
                .any(|c| c.starts_with("cmd package set-home-activity")),
            "{calls:?}"
        );
        assert!(
            calls.iter().any(|c| c.starts_with("pm set-home-activity")),
            "{calls:?}"
        );
    }

    /// The report a user can paste back. Without the per-stage record, the
    /// error says a switch failed but not which command the device refused.
    #[tokio::test]
    async fn a_failed_switch_records_every_stage_it_went_through() {
        let mock = MockAdb::default()
            .on_shell("add-role-holder", "Unknown command: role")
            .on_shell("query-activities", "Unknown command: package")
            .on_shell("set-home-activity", "Success")
            .on_shell("resolve-activity", "com.google.android.tvlauncher/.Home");
        let state = state_with(mock);

        let res = set_default_launcher_impl(
            &state,
            "serial",
            "com.spocky.projengmenu",
            false,
            &Progress::Silent,
        )
        .await
        .unwrap();

        let report = res.diagnostics.join("\n");
        assert!(
            report.contains("pm enable com.spocky.projengmenu"),
            "{report}"
        );
        assert!(
            report.contains("resolve-activity HOME -> com.google.android.tvlauncher"),
            "{report}"
        );
        assert!(report.contains("add-role-holder"), "{report}");
        assert!(report.contains("set-home-activity"), "{report}");
        // Every line says what came back, not just what was sent.
        assert!(
            res.diagnostics.iter().all(|line| line.contains("->")),
            "{report}"
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

    /// GitHub #122 — onn 4K Pro: setting Projectivy as default reported a
    /// failure and rolled stock back even though the switch had actually
    /// worked. Three cases from the issue's polling/rollback fix.
    mod launcher_verification_122 {
        use super::*;

        const GTV_STOCK: &str = "com.google.android.apps.tv.launcherx";
        const WRAITH: &str = "com.google.android.tungsten.setupwraith";

        #[tokio::test]
        async fn unreadable_home_apps_never_leave_stock_off_with_the_helper_on() {
            let mock = MockAdb::default()
                .on_shell("add-role-holder", "Unknown command")
                .on_shell_failure("set-home-activity", "Error: no such activity")
                .on_shell_err("query-activities", "device offline")
                .on_shell_seq(
                    "resolve-activity",
                    &[
                        &format!("{GTV_STOCK}/.Home"),
                        &format!("{GTV_STOCK}/.Home"),
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

            let calls = log.lock().unwrap();
            let stock_off = calls
                .iter()
                .any(|c| c == &format!("pm disable-user --user 0 {GTV_STOCK}"));
            let stock_back = calls.iter().any(|c| c == &format!("pm enable {GTV_STOCK}"));
            assert!(
                !res.ok && (!stock_off || stock_back),
                "stock off with the helper unchecked: {calls:?} {:?}",
                res.last_error
            );
        }

        #[tokio::test]
        async fn a_failed_helper_rollback_is_reported_not_called_re_enabled() {
            let query = format!(
                "  priority=0\n    packageName={WRAITH}\n  priority=0\n    packageName=com.example.launcher\n"
            );
            let mock = MockAdb::default()
                .on_shell("query-activities", &query)
                .on_shell("get-role-holders", "")
                .on_shell("resolve-activity", &format!("{WRAITH}/.Wraith"))
                .on_shell_failure("pm enable", "Failure: not allowed");
            let state = state_with(mock);

            let res = disable_setup_helper_impl(&state, "serial", WRAITH)
                .await
                .unwrap();

            assert!(!res.ok);
            assert!(res.message.contains("may still be off"), "{}", res.message);
            assert!(!res.message.contains("was re-enabled"), "{}", res.message);
        }

        #[tokio::test]
        async fn resolver_lag_then_target_confirms_without_rollback() {
            // The resolver can take a few polls to catch up after a
            // stock-disable takeover, showing a transient holder (Setup
            // Wraith) along the way — that's "still settling", not a
            // mismatch, and the loop must keep polling through it rather
            // than giving up.
            let mock = MockAdb::default()
                .on_shell("add-role-holder", "Unknown command")
                .on_shell_failure("set-home-activity", "Error: no such activity")
                .on_shell_seq(
                    "resolve-activity",
                    &[
                        "com.google.android.tvlauncher/.Home", // active_before
                        "com.google.android.tvlauncher/.Home", // fast-path quick check
                        "com.google.android.tvlauncher/.Home", // verify poll 1: still stock
                        "com.google.android.tungsten.setupwraith/.Wraith", // verify poll 2: transient
                        "com.example.launcher/.MainActivity",              // verify poll 3: landed
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

            assert!(res.ok, "{:?}", res.last_error);
            assert_eq!(res.strategy.as_deref(), Some("disable_stock_takeover"));
            let calls = log.lock().unwrap();
            assert!(
                !calls
                    .iter()
                    .any(|c| c == "pm enable com.google.android.tvlauncher"),
                "must not roll back a switch that lands within the poll window: {calls:?}"
            );
        }

        #[tokio::test]
        async fn role_holder_confirms_through_a_stuck_resolver_value() {
            // Some builds update the HOME role's holder list before the
            // resolver catches up. The role naming the target is success on
            // its own, even while the resolver is still stuck on Setup
            // Wraith.
            let mock = MockAdb::default()
                .on_shell("add-role-holder", "Unknown command")
                .on_shell_failure("set-home-activity", "Error: no such activity")
                .on_shell_seq(
                    "resolve-activity",
                    &[
                        "com.google.android.tvlauncher/.Home", // active_before
                        "com.google.android.tvlauncher/.Home", // fast-path quick check
                        "com.google.android.tungsten.setupwraith/.Wraith", // stuck from here on
                    ],
                )
                .on_shell("get-role-holders", "com.example.launcher");
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

            assert!(res.ok, "{:?}", res.last_error);
            assert_eq!(res.strategy.as_deref(), Some("disable_stock_takeover"));
            let calls = log.lock().unwrap();
            assert!(calls.iter().any(|c| c.contains("get-role-holders")));
            assert!(
                !calls
                    .iter()
                    .any(|c| c == "pm enable com.google.android.tvlauncher"),
                "role confirmation must not be second-guessed by a stale resolver read: {calls:?}"
            );
        }

        #[tokio::test]
        async fn rolls_back_only_when_stock_positively_still_holds_home() {
            // The negative case this whole fix has to preserve: if stock
            // genuinely never let go of HOME, that's a real failure and
            // stock must be restored.
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
            assert!(
                calls
                    .iter()
                    .any(|c| c == "pm enable com.google.android.tvlauncher"),
                "stock genuinely still holding Home must be restored: {calls:?}"
            );
        }

        #[tokio::test]
        async fn restores_stock_when_the_resolver_never_answers_during_verification() {
            // An unavailable resolver is not evidence that a replacement took
            // over Home — it is no evidence at all. Treating it like "some
            // other app landed" would leave a TV with stock disabled and
            // nothing confirmed to hand it Home back.
            let mock = MockAdb::default()
                .on_shell("add-role-holder", "Unknown command")
                .on_shell_failure("set-home-activity", "Error: no such activity")
                .on_shell_seq(
                    "resolve-activity",
                    &[
                        "com.google.android.tvlauncher/.Home", // active_before
                        "com.google.android.tvlauncher/.Home", // quick check after setters
                        "",                                    // every verify poll: unavailable
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

            assert!(!res.ok);
            let calls = log.lock().unwrap();
            assert!(
                calls
                    .iter()
                    .any(|c| c == "pm enable com.google.android.tvlauncher"),
                "an unreadable resolver must restore stock, not leave it disabled: {calls:?}"
            );
        }
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

    const COMPONENTS_QUERY: &str = "query-activities --components";
    const HANDLERS_QUERY: &str = "query-activities -a";

    #[tokio::test]
    async fn set_home_any_refuses_honestly_for_an_app_without_a_home_screen() {
        // YouTube declares only LEANBACK_LAUNCHER. The device lists the stock
        // launcher as its only HOME component, so the answer is "doesn't
        // declare a Home screen" — and nothing may be disabled on the way.
        let mock = MockAdb::default()
            .on_shell(
                COMPONENTS_QUERY,
                "com.google.android.tvlauncher/.MainActivity",
            )
            .on_shell("add-role-holder", "Error: not a home app")
            .on_shell("set-home-activity", "Error: component not found")
            .on_shell("resolve-activity", "com.google.android.tvlauncher/.Home");
        let log = mock.shell_log();
        let state = state_with(mock);

        let res = set_home_any_impl(&state, "serial", "com.google.android.youtube.tv", None)
            .await
            .unwrap();
        assert!(!res.ok);
        assert_eq!(res.declares_home, Some(false));
        assert!(!res.stock_holds_home);
        assert!(
            res.message.contains("doesn't declare a Home screen"),
            "{}",
            res.message
        );
        let calls = log.lock().unwrap();
        assert!(
            !calls.iter().any(|c| c.contains("disable")),
            "picking an app must never disable anything: {calls:?}"
        );
    }

    #[tokio::test]
    async fn set_home_any_reports_stock_holding_home_without_taking_it_over() {
        let mock = MockAdb::default()
            .on_shell(
                COMPONENTS_QUERY,
                "com.google.android.tvlauncher/.MainActivity\ncom.spocky.projengmenu/.ui.home.HomeActivity",
            )
            .on_shell("add-role-holder", "Success")
            .on_shell("set-home-activity", "Success")
            .on_shell("resolve-activity", "com.google.android.tvlauncher/.Home");
        let log = mock.shell_log();
        let state = state_with(mock);

        let res = set_home_any_impl(&state, "serial", "com.spocky.projengmenu", None)
            .await
            .unwrap();
        assert!(!res.ok);
        assert_eq!(res.declares_home, Some(true));
        assert!(res.stock_holds_home);
        assert!(
            res.message.contains("Disable stock launcher"),
            "{}",
            res.message
        );
        let calls = log.lock().unwrap();
        assert!(calls
            .iter()
            .any(|c| c.ends_with("com.spocky.projengmenu/.ui.home.HomeActivity")));
        assert!(!calls.iter().any(|c| c.contains("disable")), "{calls:?}");
    }

    #[tokio::test]
    async fn set_home_any_succeeds_when_home_verifies_and_tries_the_given_activity_first() {
        let mock = MockAdb::default()
            .on_shell(COMPONENTS_QUERY, "")
            .on_shell("add-role-holder", "Unknown command: role")
            .on_shell("set-home-activity", "Success")
            .on_shell("resolve-activity", "com.example.home/.Custom");
        let log = mock.shell_log();
        let state = state_with(mock);

        let res = set_home_any_impl(&state, "serial", "com.example.home", Some(".Custom"))
            .await
            .unwrap();
        assert!(res.ok, "{}", res.message);
        // An empty query answers nothing: unknown, not "doesn't declare".
        assert_eq!(res.declares_home, None);
        let calls = log.lock().unwrap();
        let first_setter = calls
            .iter()
            .find(|c| c.contains("set-home-activity"))
            .expect("a setter ran");
        assert!(
            first_setter.ends_with("com.example.home/.Custom"),
            "{first_setter}"
        );
    }

    #[tokio::test]
    async fn set_home_any_rejects_injected_activity_names() {
        let mock = MockAdb::default();
        let log = mock.shell_log();
        let state = state_with(mock);
        for bad in [".Main; reboot", "a b", ".", "com..x", ".1Main"] {
            let res = set_home_any_impl(&state, "serial", "com.example.home", Some(bad))
                .await
                .unwrap();
            assert!(!res.ok, "{bad:?} accepted");
        }
        assert!(log.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn disable_stock_refuses_when_the_target_has_no_home_screen() {
        // The TV must never be left with no Home screen: a target that isn't
        // an enabled HOME handler would leave stock as the only one.
        let mock = MockAdb::default()
            .on_shell(
                HANDLERS_QUERY,
                "    packageName=com.google.android.tvlauncher",
            )
            .on_shell("resolve-activity", "com.google.android.tvlauncher/.Home");
        let log = mock.shell_log();
        let state = state_with(mock);

        let res = disable_stock_launcher_impl(
            &state,
            "serial",
            "com.google.android.youtube.tv",
            &Progress::Silent,
        )
        .await
        .unwrap();
        assert!(!res.ok);
        assert!(
            res.last_error
                .as_deref()
                .is_some_and(|e| e.contains("doesn't declare a Home screen")),
            "{:?}",
            res.last_error
        );
        let calls = log.lock().unwrap();
        assert!(
            !calls.iter().any(|c| c.contains("disable-user")),
            "{calls:?}"
        );
    }

    #[tokio::test]
    async fn disable_stock_refuses_a_settings_recovery_handler() {
        for target in crate::engine::launcher::safe_home_handlers() {
            let state = state_with(MockAdb::default());
            let res = disable_stock_launcher_impl(&state, "serial", target, &Progress::Silent)
                .await
                .unwrap();
            assert!(!res.ok, "{target} must never take over Home");
        }
    }

    #[tokio::test]
    async fn disable_stock_refuses_a_stock_target() {
        let state = state_with(MockAdb::default());
        let res = disable_stock_launcher_impl(
            &state,
            "serial",
            "com.google.android.tvlauncher",
            &Progress::Silent,
        )
        .await
        .unwrap();
        assert!(!res.ok);
    }

    #[tokio::test]
    async fn disable_stock_hands_home_over_when_stock_holds_it() {
        let mock = MockAdb::default()
            .on_shell(
                HANDLERS_QUERY,
                "    packageName=com.google.android.tvlauncher\n    packageName=com.spocky.projengmenu",
            )
            .on_shell_seq(
                "resolve-activity",
                &[
                    "com.google.android.tvlauncher/.Home",
                    "com.spocky.projengmenu/.Home",
                ],
            );
        let log = mock.shell_log();
        let state = state_with(mock);

        let res = disable_stock_launcher_impl(
            &state,
            "serial",
            "com.spocky.projengmenu",
            &Progress::Silent,
        )
        .await
        .unwrap();
        assert!(res.ok, "{:?}", res.last_error);
        let calls = log.lock().unwrap();
        assert!(calls
            .iter()
            .any(|c| c == "pm disable-user --user 0 com.google.android.tvlauncher"));
        assert!(
            !calls.iter().any(|c| c.starts_with("pm enable")),
            "{calls:?}"
        );
    }

    #[tokio::test]
    async fn disable_stock_behind_the_active_target_restores_if_home_moves() {
        // Target already holds Home; stock is disabled behind it. If Home then
        // stops resolving to the target, stock comes back.
        let mock = MockAdb::default()
            .on_shell(
                HANDLERS_QUERY,
                "    packageName=com.google.android.tvlauncher\n    packageName=com.spocky.projengmenu",
            )
            .on_shell_seq(
                "resolve-activity",
                &[
                    "com.spocky.projengmenu/.Home",
                    "com.android.tv.settings/.FallbackHome",
                ],
            );
        let log = mock.shell_log();
        let state = state_with(mock);

        let res = disable_stock_launcher_impl(
            &state,
            "serial",
            "com.spocky.projengmenu",
            &Progress::Silent,
        )
        .await
        .unwrap();
        assert!(!res.ok);
        let calls = log.lock().unwrap();
        assert!(calls
            .iter()
            .any(|c| c == "pm disable-user --user 0 com.google.android.tvlauncher"));
        assert!(calls
            .iter()
            .any(|c| c == "pm enable com.google.android.tvlauncher"));
    }
}

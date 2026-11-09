//! Snapshot save / load / apply-plan commands.

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::State;

use crate::adb::{
    checked_batch_command, parse_checked_batch, parse_disabled_packages_output,
    parse_installed_packages_output,
};
use crate::engine::snapshot::{
    compute_apply_plan, tracked_setting_keys, ApplyPlanInputs, Snapshot, SnapshotApplyPlan,
    SCHEMA_VERSION,
};
use crate::engine::{
    home_keep_reason, is_never_disable, is_valid_package_name, may_be_home_handler,
};
use crate::license::Feature;

use super::launcher::{
    parse_home_handler_packages, read_current_home, read_enabled_home_handlers,
    set_default_launcher_core, Progress, HOME_HANDLER_QUERY,
};
use super::loader::launchers;
use super::verification::Unconfirmed;
use super::{
    is_valid_setting_key, quote_shell_arg, AppState, LIST_DISABLED_PACKAGES, LIST_PACKAGES,
};

/// Read the device's current values for the tracked setting keys, keyed the
/// same way snapshots store them (`"<ns>.<key>"`). Used so apply-plans can
/// skip settings already at the target value. Every read must complete before
/// a missing value can be treated as absent.
async fn current_settings_map(
    adb: &dyn crate::adb::AdbDriver,
    serial: &str,
) -> Result<std::collections::BTreeMap<String, String>, String> {
    let keys = tracked_setting_keys();
    let commands = keys
        .iter()
        .map(|(ns, key)| format!("settings get {ns} {key}"))
        .collect::<Vec<_>>();
    let cmd = checked_batch_command(&commands.iter().map(String::as_str).collect::<Vec<_>>());
    let mut map = std::collections::BTreeMap::new();
    let out = adb
        .shell(serial, &cmd)
        .await
        .map_err(|e| format!("read snapshot settings: {e}"))?;
    let required = (0..keys.len()).collect::<Vec<_>>();
    let sections = parse_checked_batch(&out.stdout, keys.len(), &required)?;
    for ((ns, key), value) in keys.iter().zip(sections) {
        if value != "null" {
            map.insert(format!("{ns}.{key}"), value);
        }
    }
    Ok(map)
}

/// What a restore plan reads from the device: the installed and disabled
/// package lists and the HOME handlers. Preview and apply need the same
/// reads, so they share one batched shell call (each separate call costs a
/// full RTT on mobile, where every call is serialized behind a single TCP
/// connection).
struct RestoreReads {
    installed: Vec<String>,
    disabled: Vec<String>,
    /// `None` when the HOME query failed or listed nothing (Android 8 has no
    /// `query-activities`); the plan then keeps every possible Home app on.
    home_handlers: Option<Vec<String>>,
}

async fn restore_reads(
    adb: &dyn crate::adb::AdbDriver,
    serial: &str,
) -> Result<RestoreReads, String> {
    let cmd = checked_batch_command(&[LIST_PACKAGES, LIST_DISABLED_PACKAGES, HOME_HANDLER_QUERY]);
    let out = adb
        .shell(serial, &cmd)
        .await
        .map_err(|e| format!("pm list packages: {e}"))?;
    let sections = parse_checked_batch(&out.stdout, 3, &[0, 1])?;
    let installed = parse_installed_packages_output(&sections[0]);
    // An empty installed list is not a usable input for an apply plan.
    if installed.is_empty() {
        return Err("pm list packages: no packages reported".to_string());
    }
    let disabled = parse_disabled_packages_output(&sections[1]);
    let home_handlers = Some(parse_home_handler_packages(&sections[2])).filter(|h| !h.is_empty());
    Ok(RestoreReads {
        installed,
        disabled,
        home_handlers,
    })
}

/// The disabled packages a snapshot records. A failed read must fail the
/// save: a "nothing disabled" snapshot is one the Launcher's "Save snapshot
/// first" would trust as a way back.
async fn read_disabled_packages(
    adb: &dyn crate::adb::AdbDriver,
    serial: &str,
) -> Result<Vec<String>, String> {
    let out = adb
        .shell(serial, &checked_batch_command(&[LIST_DISABLED_PACKAGES]))
        .await
        .map_err(|e| format!("pm list packages -d: {e}"))?;
    let sections = parse_checked_batch(&out.stdout, 1, &[0])
        .map_err(|e| format!("pm list packages -d: {e}"))?;
    Ok(parse_disabled_packages_output(&sections[0]))
}

/// Read and parse a snapshot, confined to the snapshot directory: the
/// frontend hands us paths and we should not blindly read arbitrary
/// locations.
async fn read_confined_snapshot(
    snapshot_dir: &Path,
    snapshot_path: &str,
) -> Result<Snapshot, String> {
    let canonical_path = tokio::fs::canonicalize(PathBuf::from(snapshot_path))
        .await
        .map_err(|e| format!("snapshot path: {e}"))?;
    let canonical_dir = tokio::fs::canonicalize(snapshot_dir)
        .await
        .map_err(|e| format!("snapshot dir: {e}"))?;
    if !canonical_path.starts_with(&canonical_dir) {
        return Err(format!(
            "snapshot path is outside the configured snapshot directory ({})",
            canonical_dir.display()
        ));
    }
    let contents = tokio::fs::read_to_string(&canonical_path)
        .await
        .map_err(|e| format!("read snapshot: {e}"))?;
    Snapshot::from_json(&contents).map_err(|e| format!("parse snapshot: {e}"))
}

/// Read the device and plan `snap` against it. Preview and apply share it,
/// so the preview shows what apply runs. Also returns the HOME handlers it
/// read, which tell apply which packages are Home apps.
async fn plan_for(
    state: &AppState,
    serial: &str,
    snap: &Snapshot,
) -> Result<(SnapshotApplyPlan, Option<Vec<String>>), String> {
    let adb = state.adb_snapshot().await;
    let reads = restore_reads(adb.as_ref(), serial).await?;
    let device = crate::commands::devices::device_profile_impl(state, serial).await?;
    let current_settings = current_settings_map(adb.as_ref(), serial).await?;
    let current_launcher = read_current_home(adb.as_ref(), serial)
        .await
        .ok()
        .and_then(|r| r.package);
    let plan = compute_apply_plan(
        snap,
        &ApplyPlanInputs {
            target_device_type: device.device_type,
            currently_installed: &reads.installed,
            currently_disabled: &reads.disabled,
            current_settings: &current_settings,
            current_launcher: current_launcher.as_deref(),
            home_handlers: reads.home_handlers.as_deref(),
            launchers: launchers(),
        },
    );
    Ok((plan, reads.home_handlers))
}

#[derive(Serialize)]
pub struct SnapshotFile {
    pub path: String,
    pub filename: String,
    pub saved_at: String,
    /// User-given snapshot name (schema v2+); None for older snapshots.
    pub label: Option<String>,
    pub device_name: String,
    pub device_serial: String,
    pub device_type: crate::engine::DeviceType,
    pub disabled_count: usize,
    pub settings_count: usize,
    pub launcher: Option<String>,
}

/// `delete_snapshot` — remove a snapshot file from disk. Path is confined to
/// the configured snapshot directory; anything outside is rejected.
#[tauri::command]
pub async fn delete_snapshot(
    state: State<'_, AppState>,
    snapshot_path: String,
) -> Result<(), String> {
    state.require_pro(Feature::Snapshot)?;
    delete_snapshot_file(&state.snapshot_dir, Path::new(&snapshot_path)).await
}

async fn delete_snapshot_file(snapshot_dir: &Path, snapshot_path: &Path) -> Result<(), String> {
    let canonical_path = tokio::fs::canonicalize(snapshot_path)
        .await
        .map_err(|e| format!("snapshot path: {e}"))?;
    let canonical_dir = tokio::fs::canonicalize(snapshot_dir)
        .await
        .map_err(|e| format!("snapshot dir: {e}"))?;
    if !canonical_path.starts_with(&canonical_dir) {
        return Err("snapshot path is outside the snapshot directory".into());
    }
    let metadata = tokio::fs::symlink_metadata(snapshot_path)
        .await
        .map_err(|e| format!("snapshot file: {e}"))?;
    // Deleting a link's resolved target would remove a different file than the
    // entry the user picked. Snapshots are regular JSON files, never links.
    if !metadata.is_file() || canonical_path.extension().and_then(|e| e.to_str()) != Some("json") {
        return Err("snapshot must be a regular JSON file".into());
    }
    tokio::fs::remove_file(&canonical_path)
        .await
        .map_err(|e| format!("delete: {e}"))
}

/// `snapshot_dir_path` — where snapshots are saved on this machine. The UI
/// uses this to surface the path and to feed it into the OS file picker.
///
/// Creates the directory if it doesn't exist yet so the "Open folder" / reveal
/// action works even before the first snapshot is saved. Best-effort: a failure
/// to create still returns the path (the reveal will simply error in the UI).
#[tauri::command]
pub async fn snapshot_dir_path(state: State<'_, AppState>) -> Result<String, String> {
    state.require_pro(Feature::Snapshot)?;
    let _ = tokio::fs::create_dir_all(&state.snapshot_dir).await;
    Ok(state.snapshot_dir.display().to_string())
}

/// `list_snapshots` — return saved snapshots in `snapshot_dir`, newest first.
#[tauri::command]
pub async fn list_snapshots(state: State<'_, AppState>) -> Result<Vec<SnapshotFile>, String> {
    state.require_pro(Feature::Snapshot)?;
    let dir = state.snapshot_dir.clone();
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut out = Vec::new();
    let mut entries = tokio::fs::read_dir(&dir)
        .await
        .map_err(|e| format!("read_dir: {e}"))?;
    while let Some(entry) = entries.next_entry().await.transpose() {
        let entry = entry.map_err(|e| format!("read_dir entry: {e}"))?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Ok(contents) = tokio::fs::read_to_string(&path).await else {
            continue;
        };
        let Ok(snap) = Snapshot::from_json(&contents) else {
            continue;
        };
        let filename = path
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or("")
            .to_string();
        out.push(SnapshotFile {
            path: path.display().to_string(),
            filename,
            saved_at: snap.saved_at,
            label: snap.label,
            device_name: snap.device_name,
            device_serial: snap.device_serial,
            device_type: snap.device_type,
            disabled_count: snap.disabled_packages.len(),
            settings_count: snap.settings.len() + snap.absent_settings.len(),
            launcher: snap.current_launcher,
        });
    }
    // Sort newest first by saved_at (ISO-8601 sorts lexicographically).
    out.sort_by(|a, b| b.saved_at.cmp(&a.saved_at));
    Ok(out)
}

/// `save_snapshot` — capture current device state to a JSON file.
#[tauri::command]
pub async fn save_snapshot(
    state: State<'_, AppState>,
    serial: String,
    device_name: String,
    label: Option<String>,
) -> Result<SnapshotFile, String> {
    state.require_pro(Feature::Snapshot)?;
    // Empty/whitespace label is treated as "no label".
    let label = label
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let adb = state.adb_snapshot().await;

    // Pull all the inputs the engine needs.
    let disabled_packages = read_disabled_packages(adb.as_ref(), &serial).await?;

    // Current launcher: the HOME role holder, not a setup helper the
    // resolver ranks higher (#122).
    let current_launcher = crate::commands::launcher::read_current_home(adb.as_ref(), &serial)
        .await?
        .package;

    let settings = current_settings_map(adb.as_ref(), &serial).await?;
    let absent_settings = tracked_setting_keys()
        .iter()
        .map(|(ns, key)| format!("{ns}.{key}"))
        .filter(|key| !settings.contains_key(key))
        .collect();

    // Detect device type the same way list_devices does — we'll just refetch
    // here for snapshot purposes since it's cheap.
    let device_type =
        match crate::commands::devices::device_profile_impl(state.inner(), &serial).await {
            Ok(d) => d.device_type,
            Err(_) => crate::engine::DeviceType::Unknown,
        };

    // Android version.
    let ver_out = adb
        .shell(&serial, "getprop ro.build.version.release")
        .await
        .map_err(|e| format!("getprop: {e}"))?;
    let android_version = ver_out.stdout.trim().to_string();

    let saved_at = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let snap = Snapshot {
        schema_version: SCHEMA_VERSION,
        saved_at: saved_at.clone(),
        label: label.clone(),
        device_name: device_name.clone(),
        device_serial: serial.clone(),
        device_type,
        android_version,
        disabled_packages,
        current_launcher,
        settings,
        absent_settings,
    };

    // Write to disk.
    let dir = state.snapshot_dir.clone();
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| format!("create snapshot dir: {e}"))?;

    // Filename stem prefers the user's label, falling back to the device name.
    let stem_source = label.as_deref().unwrap_or(&device_name);
    let safe_name: String = stem_source
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    let filename = format!("{safe_name}_{stamp}.json");
    let path: PathBuf = dir.join(&filename);

    let json = snap
        .to_json()
        .map_err(|e| format!("serialize snapshot: {e}"))?;
    tokio::fs::write(&path, &json)
        .await
        .map_err(|e| format!("write snapshot: {e}"))?;

    Ok(SnapshotFile {
        path: path.display().to_string(),
        filename,
        saved_at: snap.saved_at,
        label: snap.label,
        device_name: snap.device_name,
        device_serial: snap.device_serial,
        device_type: snap.device_type,
        disabled_count: snap.disabled_packages.len(),
        settings_count: snap.settings.len() + snap.absent_settings.len(),
        launcher: snap.current_launcher,
    })
}

/// `preview_apply` — compute the plan for applying `snapshot_path` to `serial`
/// without executing it. Lets the UI show the user exactly what would happen
/// before they confirm.
#[tauri::command]
pub async fn preview_apply(
    state: State<'_, AppState>,
    serial: String,
    snapshot_path: String,
) -> Result<SnapshotApplyPlan, String> {
    state.require_pro(Feature::Snapshot)?;
    let snap = read_confined_snapshot(&state.snapshot_dir, &snapshot_path).await?;
    Ok(plan_for(state.inner(), &serial, &snap).await?.0)
}

#[derive(Serialize)]
pub struct ApplyResult {
    pub packages_disabled: Vec<String>,
    pub packages_failed: Vec<String>,
    pub launcher_set: bool,
    pub launcher_message: Option<String>,
    pub settings_written: Vec<String>,
    pub settings_deleted: Vec<String>,
    pub settings_failed: Vec<String>,
    pub summary: String,
}

/// What `disable_from_plan` needs to keep the TV's Home screen.
struct HomeGuard<'a> {
    /// The HOME handlers the plan read, which mark the packages that need a
    /// fresh check before they go.
    handlers: Option<&'a [String]>,
    current_home: Option<&'a str>,
    /// Packages the plan already kept on.
    kept: Vec<String>,
}

#[derive(Default)]
struct DisableOutcome {
    disabled: Vec<String>,
    /// Display entries: `pkg` or `pkg (reason)`.
    failed: Vec<String>,
    /// Home apps left on at run time.
    kept: Vec<String>,
    /// For each disabled stock launcher with a paired setup helper, the Home
    /// apps read just before it went off.
    inventories: Vec<(String, Vec<String>)>,
    /// Disables Android accepted whose result could not be read back. Listed
    /// as failed, but they may have landed.
    unconfirmed: Vec<String>,
}

/// Execute the disable portion of an apply-plan, in the plan's order.
///
/// Every package that may be a Home app is checked against a fresh read of
/// the Home apps right before it goes: the plan's read is moments old, and a
/// restore must never leave the TV without a Home screen. An unreadable list
/// keeps it on, like the only Home app.
async fn disable_from_plan(
    adb: &dyn crate::adb::AdbDriver,
    serial: &str,
    packages_to_disable: &[String],
    home: HomeGuard<'_>,
) -> DisableOutcome {
    let catalog = launchers();
    let mut kept_names = home.kept;
    let mut out = DisableOutcome::default();
    for pkg in packages_to_disable {
        if !is_valid_package_name(pkg) {
            out.failed
                .push(format!("{pkg:?} (invalid package name in snapshot)"));
            continue;
        }
        if is_never_disable(pkg) {
            out.failed.push(format!("{pkg} (refused: brick risk)"));
            continue;
        }
        let mut inventory = None;
        if may_be_home_handler(pkg, home.handlers, home.current_home, catalog) {
            let handlers = read_enabled_home_handlers(adb, serial).await;
            if let Some(reason) = home_keep_reason(
                pkg,
                handlers.as_deref(),
                home.current_home,
                &kept_names,
                catalog,
            ) {
                out.failed.push(format!("{pkg} (kept: {})", reason.label()));
                kept_names.push(pkg.clone());
                out.kept.push(pkg.clone());
                continue;
            }
            inventory = handlers;
        }
        let cmd = format!("pm disable-user --user 0 {pkg}");
        match adb.shell(serial, &cmd).await {
            Ok(result) if super::verification::succeeded(&result) => {
                // A clean write is not proof: read the state back for user 0.
                match super::verification::verify_package(adb, serial, pkg, false).await {
                    Ok(()) => out.disabled.push(pkg.clone()),
                    Err(Unconfirmed::Differs) => {
                        out.failed
                            .push(format!("{pkg} ({})", Unconfirmed::Differs.message()));
                        continue;
                    }
                    // It may have gone off. Not claimed, but the Home
                    // follow-ups below still run as if it had.
                    Err(Unconfirmed::Unreadable) => {
                        out.failed
                            .push(format!("{pkg} ({})", Unconfirmed::Unreadable.message()));
                        out.unconfirmed.push(pkg.clone());
                    }
                }
                if let Some(inventory) = inventory {
                    if !catalog.disable_with_for(pkg).is_empty() {
                        out.inventories.push((pkg.clone(), inventory));
                    }
                }
            }
            _ => out.failed.push(pkg.clone()),
        }
    }
    out
}

/// A later, more specific result for an unconfirmed disable replaces its
/// "could not be read back" entry rather than listing the package twice.
fn drop_unconfirmed_entry(failed: &mut Vec<String>, unconfirmed: &[String], pkg: &str) {
    if unconfirmed.iter().any(|p| p == pkg) {
        let entry = format!("{pkg} ({})", Unconfirmed::Unreadable.message());
        failed.retain(|f| f != &entry);
    }
}

/// Set Home for a restore. No stock takeover here: a snapshot that had stock
/// disabled carries that in its disabled-packages list. The plan only names
/// an installed, enabled launcher, so the setter's own setup-helper restore
/// is skipped: a restore never re-enables anything.
async fn switch_launcher(state: &AppState, serial: &str, launcher: &str) -> (bool, Option<String>) {
    match set_default_launcher_core(state, serial, launcher, false, &Progress::Silent).await {
        Ok(r) if r.ok => (
            true,
            Some(format!("{launcher} via {}", r.strategy.unwrap_or_default())),
        ),
        Ok(r) => (false, r.last_error),
        Err(e) => (false, Some(e)),
    }
}

/// `apply_snapshot` — actually run the previewed plan against the device.
/// Honors commitment #7 (reversibility): only `pm disable-user` writes,
/// never re-enables a currently-enabled package the snapshot didn't list.
#[tauri::command]
pub async fn apply_snapshot(
    state: State<'_, AppState>,
    serial: String,
    snapshot_path: String,
) -> Result<ApplyResult, String> {
    state.require_pro(Feature::Snapshot)?;
    let snap = read_confined_snapshot(&state.snapshot_dir, &snapshot_path).await?;
    apply_snapshot_impl(state.inner(), &serial, &snap).await
}

async fn apply_snapshot_impl(
    state: &AppState,
    serial: &str,
    snap: &Snapshot,
) -> Result<ApplyResult, String> {
    let (plan, home_handlers) = plan_for(state, serial, snap).await?;
    let adb = state.adb_snapshot().await;
    let catalog = launchers();

    // 1. Home first, while nothing is disabled yet. `None` skips the whole
    // switch ladder.
    let mut launcher_set = false;
    let mut launcher_message = None;
    if let Some(launcher) = &plan.launcher_to_set {
        (launcher_set, launcher_message) = switch_launcher(state, serial, launcher).await;
    } else if let Some(launcher) = &plan.launcher_disabled {
        launcher_message = Some(format!(
            "{launcher} is disabled; enable it first, then restore"
        ));
    } else if let Some(launcher) = &plan.launcher_not_installed {
        launcher_message = Some(format!("{launcher} isn't installed; skipped"));
    }

    // 2. Disable packages from the plan (additive only — never re-enable).
    let mut packages_failed: Vec<String> = plan
        .packages_kept
        .iter()
        .map(|k| format!("{} (kept: {})", k.package, k.reason))
        .collect();
    let mut kept: Vec<String> = plan
        .packages_kept
        .iter()
        .map(|k| k.package.clone())
        .collect();
    let outcome = disable_from_plan(
        adb.as_ref(),
        serial,
        &plan.packages_to_disable,
        HomeGuard {
            handlers: home_handlers.as_deref(),
            current_home: plan.current_launcher.as_deref(),
            kept: kept.clone(),
        },
    )
    .await;
    let mut packages_disabled = outcome.disabled;
    packages_failed.extend(outcome.failed);
    kept.extend(outcome.kept);

    // An enabled stock launcher overrides the polite setters (Shield, Google
    // TV), so with it now off they can finish the switch.
    if let Some(launcher) = &plan.launcher_to_set {
        let stock_may_be_off = packages_disabled
            .iter()
            .chain(&outcome.unconfirmed)
            .any(|p| catalog.is_stock(p));
        if !launcher_set && stock_may_be_off {
            (launcher_set, launcher_message) = switch_launcher(state, serial, launcher).await;
        }
    }

    // 3. Settle each disabled stock launcher's setup helper (Google TV's
    // Setup Wraith) now that Home is where the snapshot wants it (#159).
    // Snapshots from older app versions list stock without its helper.
    let mut helper_notes: Vec<String> = Vec::new();
    for (stock, inventory) in &outcome.inventories {
        let mut diagnostics = Vec::new();
        use crate::commands::launcher::PairedHelperOutcome as Outcome;
        match crate::commands::launcher::settle_paired_helpers_after_stock_disable(
            adb.as_ref(),
            serial,
            stock,
            Some(inventory),
            &mut diagnostics,
        )
        .await
        {
            Outcome::Untouched | Outcome::Disabled { .. } => {}
            Outcome::RolledBack { reason } => {
                packages_disabled.retain(|p| p != stock);
                drop_unconfirmed_entry(&mut packages_failed, &outcome.unconfirmed, stock);
                packages_failed.push(format!("{stock} (setup helper: {reason})"));
            }
            Outcome::LeftOn => {
                let helper_also_off = catalog
                    .disable_with_for(stock)
                    .iter()
                    .any(|h| packages_disabled.contains(h));
                if helper_also_off {
                    // The snapshot turned the helper off too, and no other
                    // launcher took Home: the TV may have no Home screen.
                    // Bring stock back rather than report the opposite state.
                    let restore = adb.shell(serial, &format!("pm enable {stock}")).await;
                    let restored = matches!(&restore, Ok(out) if out.success() && !out.shell_reported_failure());
                    packages_disabled.retain(|p| p != stock);
                    drop_unconfirmed_entry(&mut packages_failed, &outcome.unconfirmed, stock);
                    packages_failed.push(if restored {
                        format!("{stock} (turned back on: no other launcher took Home)")
                    } else {
                        format!("{stock} (no other launcher took Home and turning it back on failed; use Emergency Recovery)")
                    });
                } else {
                    helper_notes.push(format!(
                        "{stock} is off but Google TV's setup helper (Setup Wraith) is still on and \
                         can take the Home button. Set another launcher as Home, then use Turn it \
                         off on the Launcher tab."
                    ));
                }
            }
        }
        tracing::info!(serial = %serial, stock = %stock, diagnostics = ?diagnostics, "snapshot: paired helper");
    }

    let (settings_written, settings_deleted, settings_failed) =
        apply_settings_from_plan(adb.as_ref(), serial, &plan).await;

    let launcher_summary = if launcher_set {
        "set".to_string()
    } else if let Some(launcher) = &plan.launcher_disabled {
        format!("skipped ({launcher} is disabled; enable it first, then restore)")
    } else {
        "unchanged".to_string()
    };
    let mut summary = format!(
        "Disabled {} packages ({} failed). Launcher: {launcher_summary}. {} settings written, {} reset ({} failed).",
        packages_disabled.len(),
        packages_failed.len(),
        settings_written.len(),
        settings_deleted.len(),
        settings_failed.len()
    );
    if !kept.is_empty() {
        summary.push_str(&format!(
            " Kept on so the TV keeps a Home screen: {}.",
            kept.join(", ")
        ));
    }
    for note in &helper_notes {
        summary.push(' ');
        summary.push_str(note);
    }

    Ok(ApplyResult {
        packages_disabled,
        packages_failed,
        launcher_set,
        launcher_message,
        settings_written,
        settings_deleted,
        settings_failed,
        summary,
    })
}

async fn apply_settings_from_plan(
    adb: &dyn crate::adb::AdbDriver,
    serial: &str,
    plan: &SnapshotApplyPlan,
) -> (Vec<String>, Vec<String>, Vec<String>) {
    let mut settings_written = Vec::new();
    let mut settings_deleted = Vec::new();
    let mut settings_failed = Vec::new();
    // Apply one setting per shell call. In a `;` batch a single failing
    // `settings put` (e.g. a SecurityException on a protected key) is invisible
    // — `adb shell` exits 0 even when the write fails, so success is read back
    // per key rather than assumed for the batch.
    let writes = plan
        .settings_to_write
        .iter()
        .map(|(key, value)| (key, Some(value)));
    let deletes = plan.settings_to_delete.iter().map(|key| (key, None));
    for (k, value) in writes.chain(deletes) {
        // Key shape is `"ns.subkey"` per the snapshot schema.
        let Some((ns, key)) = k.split_once('.') else {
            settings_failed.push(format!("{k}: malformed key (expected ns.subkey)"));
            continue;
        };
        if !matches!(ns, "global" | "secure" | "system") {
            settings_failed.push(format!("{k}: unrecognized namespace"));
            continue;
        }
        if !is_valid_setting_key(key) {
            settings_failed.push(format!("{k}: key contains invalid characters"));
            continue;
        }
        // Single-quote the value so shell metacharacters inside it are inert;
        // legitimate content (numbers, spaces, device names) passes through.
        let command = match value {
            Some(value) => format!("settings put {ns} {key} {}", quote_shell_arg(value)),
            None => format!("settings delete {ns} {key}"),
        };
        match adb.shell(serial, &command).await {
            Ok(out) if super::verification::succeeded(&out) => {
                if let Err(e) = super::verification::verify_setting(
                    adb,
                    serial,
                    ns,
                    key,
                    value.map(String::as_str),
                )
                .await
                {
                    settings_failed.push(format!("{k}: {}", e.message()));
                    continue;
                }
                if value.is_some() {
                    settings_written.push(k.clone());
                } else {
                    settings_deleted.push(k.clone());
                }
            }
            Ok(out) => {
                settings_failed.push(format!("{k}: {}", super::verification::failure_text(&out)))
            }
            Err(e) => settings_failed.push(format!("{k}: {e}")),
        }
    }

    (settings_written, settings_deleted, settings_failed)
}

#[cfg(test)]
mod tests {
    use super::{
        apply_settings_from_plan, apply_snapshot_impl, current_settings_map, delete_snapshot_file,
        disable_from_plan, read_disabled_packages, restore_reads, HomeGuard,
    };
    use crate::adb::BATCH_SEPARATOR;
    use crate::commands::test_support::{state_with, MockAdb};
    use crate::engine::snapshot::{Snapshot, SCHEMA_VERSION};
    use crate::engine::DeviceType;

    fn no_home() -> HomeGuard<'static> {
        HomeGuard {
            handlers: None,
            current_home: None,
            kept: Vec::new(),
        }
    }

    #[tokio::test]
    async fn delete_snapshot_removes_only_the_selected_local_file() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("snapshots");
        tokio::fs::create_dir(&dir).await.unwrap();
        let selected = dir.join("selected.json");
        let other = dir.join("other.json");
        tokio::fs::write(&selected, b"selected snapshot")
            .await
            .unwrap();
        tokio::fs::write(&other, b"keep this snapshot")
            .await
            .unwrap();

        delete_snapshot_file(&dir, &selected).await.unwrap();
        assert!(!selected.exists());
        assert_eq!(
            tokio::fs::read(&other).await.unwrap(),
            b"keep this snapshot"
        );
        assert!(delete_snapshot_file(&dir, &selected).await.is_err());
    }

    #[tokio::test]
    async fn delete_snapshot_rejects_outside_paths_directories_and_other_files() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("snapshots");
        tokio::fs::create_dir(&dir).await.unwrap();
        let outside = root.path().join("outside.json");
        let text = dir.join("notes.txt");
        let directory = dir.join("directory.json");
        tokio::fs::write(&outside, b"outside").await.unwrap();
        tokio::fs::write(&text, b"notes").await.unwrap();
        tokio::fs::create_dir(&directory).await.unwrap();

        for path in [
            &outside,
            &text,
            &directory,
            &dir.join("../outside.json"),
            &dir,
        ] {
            assert!(delete_snapshot_file(&dir, path).await.is_err(), "{path:?}");
            assert!(path.exists(), "rejected path was removed: {path:?}");
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn delete_snapshot_rejects_symlinks_without_removing_their_targets() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("snapshots");
        tokio::fs::create_dir(&dir).await.unwrap();
        let inside = dir.join("inside.json");
        let outside = root.path().join("outside.json");
        tokio::fs::write(&inside, b"inside").await.unwrap();
        tokio::fs::write(&outside, b"outside").await.unwrap();
        for (name, target) in [
            ("inside-link.json", &inside),
            ("outside-link.json", &outside),
        ] {
            let link = dir.join(name);
            symlink(target, &link).unwrap();
            assert!(delete_snapshot_file(&dir, &link).await.is_err());
            assert!(link.exists());
            assert!(target.exists());
        }
    }

    #[tokio::test]
    async fn snapshot_plan_requires_a_successful_disabled_read() {
        let status = crate::adb::batch::BATCH_STATUS;
        let output = format!("package:com.example\n{status}0\n{BATCH_SEPARATOR}\n{status}1\n");
        let driver = MockAdb::default().on_shell(BATCH_SEPARATOR, &output);
        assert!(restore_reads(&driver, "serial").await.is_err());
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
    async fn restore_reads_take_every_list_in_one_round_trip_for_user_zero() {
        let mock = MockAdb::default().on_shell(
            BATCH_SEPARATOR,
            &batched(&[
                "package:com.example.on\npackage:com.example.off",
                "package:com.example.off",
                "  packageName=com.example.on",
            ]),
        );
        let log = mock.shell_log();

        let reads = restore_reads(&mock, "serial").await.unwrap();
        assert_eq!(reads.installed, ["com.example.on", "com.example.off"]);
        assert_eq!(reads.disabled, ["com.example.off"]);
        assert_eq!(reads.home_handlers.unwrap(), ["com.example.on"]);

        let calls = log.lock().unwrap();
        assert_eq!(
            calls.len(),
            1,
            "installed + disabled + Home apps must cost one round-trip: {calls:?}"
        );
        assert!(calls[0].contains("(pm list packages --user 0)"));
        assert!(calls[0].contains("(pm list packages -d --user 0)"));
    }

    #[tokio::test]
    async fn an_unanswered_home_query_reads_as_unknown_not_as_no_home_apps() {
        // Android 8 has no query-activities.
        let mock = MockAdb::default().on_shell(
            BATCH_SEPARATOR,
            &batched(&["package:com.example.on", "", "Unknown command: package"]),
        );
        assert!(restore_reads(&mock, "serial")
            .await
            .unwrap()
            .home_handlers
            .is_none());
    }

    #[tokio::test]
    async fn restore_reads_error_when_the_installed_section_is_empty() {
        // The batch exits with the last sub-command's status, so a failed
        // `pm list packages` reads as success with an empty section — that
        // must surface instead of reducing the apply plan to a no-op.
        let state = MockAdb::default().on_shell(BATCH_SEPARATOR, &batched(&["", "", ""]));
        let Err(err) = restore_reads(&state, "serial").await else {
            panic!("empty package listing must be an error");
        };
        assert!(err.contains("pm list packages"), "unhelpful error: {err}");
    }

    #[tokio::test]
    async fn restore_reads_surface_a_transport_failure() {
        let mock = MockAdb::default().on_shell_err(BATCH_SEPARATOR, "device offline");
        let Err(err) = restore_reads(&mock, "serial").await else {
            panic!("a transport error must not be swallowed");
        };
        assert!(err.contains("pm list packages"), "unhelpful error: {err}");
    }

    #[tokio::test]
    async fn a_failed_disabled_read_fails_the_save_instead_of_recording_none() {
        let status = crate::adb::batch::BATCH_STATUS;
        // Desktop: nonzero exit is a transport error. Mobile: Ok with the
        // exit status in the batch.
        let failed = MockAdb::default().on_shell_err("pm list packages -d", "exit 255");
        assert!(read_disabled_packages(&failed, "serial").await.is_err());
        let failed = MockAdb::default().on_shell(
            "pm list packages -d",
            &format!("Error: java.lang.SecurityException\n{status}255\n"),
        );
        assert!(read_disabled_packages(&failed, "serial").await.is_err());
        let truncated = MockAdb::default().on_shell("pm list packages -d", "");
        assert!(read_disabled_packages(&truncated, "serial").await.is_err());

        let mock = MockAdb::default().on_shell(
            "pm list packages -d",
            &batched(&["package:com.example.off"]),
        );
        let log = mock.shell_log();
        assert_eq!(
            read_disabled_packages(&mock, "serial").await.unwrap(),
            ["com.example.off"]
        );
        assert!(log.lock().unwrap()[0].contains("(pm list packages -d --user 0)"));
        // An empty list read successfully is a real answer.
        let none = MockAdb::default().on_shell("pm list packages -d", &batched(&[""]));
        assert!(read_disabled_packages(&none, "serial")
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn current_settings_map_pairs_keys_to_values_in_order() {
        // tracked_setting_keys order: window/transition/animator scale,
        // 4x hdmi, match_content_frame_rate, long_press_timeout, encoded
        // surround (2), screensaver_components, screensaver_enabled.
        let mock = MockAdb::default().on_shell(
            "settings get",
            &batched(&[
                "0.5", "0.5", "0.5", "1", "null", "0", "1", "2", "400", "null", "", "null", "null",
            ]),
        );
        let map = current_settings_map(&mock, "serial").await.unwrap();
        assert_eq!(
            map.get("global.window_animation_scale").map(String::as_str),
            Some("0.5")
        );
        assert_eq!(
            map.get("global.hdmi_system_audio_control_enabled")
                .map(String::as_str),
            Some("1")
        );
        assert_eq!(
            map.get("secure.match_content_frame_rate")
                .map(String::as_str),
            Some("2")
        );
        assert_eq!(
            map.get("secure.long_press_timeout").map(String::as_str),
            Some("400")
        );
        // "null" line is dropped, not stored.
        assert!(!map.contains_key("global.hdmi_control_auto_wakeup_enabled"));
        assert_eq!(
            map.get("global.encoded_surround_output_enabled_formats")
                .map(String::as_str),
            Some("")
        );
    }

    #[tokio::test]
    async fn settings_capture_rejects_failed_and_truncated_reads() {
        let complete = batched(&vec!["null"; super::tracked_setting_keys().len()]);
        let status = crate::adb::batch::BATCH_STATUS;
        for output in [
            complete.replacen(&format!("{status}0"), &format!("{status}1"), 1),
            "null\n".into(),
        ] {
            let mock = MockAdb::default().on_shell("settings get", &output);
            assert!(current_settings_map(&mock, "serial").await.is_err());
        }
        let mock = MockAdb::default().on_shell_err("settings get", "offline");
        assert!(current_settings_map(&mock, "serial").await.is_err());
    }

    #[tokio::test]
    async fn setting_apply_distinguishes_empty_put_delete_and_failure() {
        let plan = super::SnapshotApplyPlan {
            packages_to_disable: vec![],
            packages_already_disabled: vec![],
            packages_not_installed: vec![],
            launcher_to_set: None,
            current_launcher: None,
            launcher_not_installed: None,
            launcher_disabled: None,
            packages_kept: vec![],
            cross_device_warning: None,
            settings_already_set: vec![],
            current_values: Default::default(),
            settings_to_write: [("global.empty".into(), String::new())].into(),
            settings_to_delete: vec![
                "global.unset".into(),
                "global.denied".into(),
                "global.bad;key".into(),
            ],
        };
        let mock = MockAdb::default()
            .on_shell("settings list global", &batched(&["empty="]))
            .on_shell_failure("settings delete global denied", "Error: permission denied");
        let log = mock.shell_log();
        let (written, deleted, failed) = apply_settings_from_plan(&mock, "serial", &plan).await;
        assert_eq!(written, ["global.empty"]);
        assert_eq!(deleted, ["global.unset"]);
        assert_eq!(failed.len(), 2);
        let calls = log.lock().unwrap();
        let readback = crate::adb::checked_batch_command(&["settings list global"]);
        assert_eq!(
            *calls,
            [
                "settings put global empty ''",
                readback.as_str(),
                "settings delete global unset",
                readback.as_str(),
                "settings delete global denied"
            ]
        );
    }

    fn settings_plan(write: (&str, &str)) -> super::SnapshotApplyPlan {
        super::SnapshotApplyPlan {
            packages_to_disable: vec![],
            packages_already_disabled: vec![],
            packages_not_installed: vec![],
            launcher_to_set: None,
            current_launcher: None,
            launcher_not_installed: None,
            launcher_disabled: None,
            packages_kept: vec![],
            cross_device_warning: None,
            settings_already_set: vec![],
            current_values: Default::default(),
            settings_to_write: [(write.0.to_string(), write.1.to_string())].into(),
            settings_to_delete: vec![],
        }
    }

    #[tokio::test]
    async fn a_setting_write_that_fails_or_reads_back_different_is_not_restored() {
        let plan = settings_plan(("secure.screensaver_enabled", "1"));
        for mock in [
            MockAdb::default().on_shell_exit("settings put", "", 1),
            MockAdb::default()
                .on_shell("settings list secure", &batched(&["screensaver_enabled=0"])),
            MockAdb::default().on_shell("settings list secure", "screensaver_enabled=1"),
        ] {
            let (written, deleted, failed) = apply_settings_from_plan(&mock, "TV", &plan).await;
            assert!(written.is_empty() && deleted.is_empty());
            assert_eq!(failed.len(), 1, "{failed:?}");
        }
        let confirmed = MockAdb::default()
            .on_shell("settings list secure", &batched(&["screensaver_enabled=1"]));
        let (written, _, failed) = apply_settings_from_plan(&confirmed, "TV", &plan).await;
        assert_eq!(written, ["secure.screensaver_enabled"]);
        assert!(failed.is_empty());
    }

    const STOCK: &str = "com.google.android.tvlauncher";
    const PROJECTIVY: &str = "com.spocky.projengmenu";
    const SETTINGS: &str = "com.android.tv.settings";
    const BLOAT: &str = "com.example.bloat";

    fn home_list(pkgs: &[&str]) -> String {
        pkgs.iter()
            .map(|p| format!("  packageName={p}\n"))
            .collect()
    }

    fn package_list(pkgs: &[&str]) -> String {
        pkgs.iter().map(|p| format!("package:{p}\n")).collect()
    }

    /// A TV `apply_snapshot_impl` can plan against: one profiled device, every
    /// tracked setting unset, these package lists and HOME handlers.
    /// `fresh_home` answers the read taken right before each Home app goes.
    fn restore_tv(
        installed: &[&str],
        disabled: &[&str],
        home: &[&str],
        fresh_home: &str,
    ) -> MockAdb {
        // Readback after each disable: confirm whatever was asked.
        let readback = batched(&[&package_list(installed)]);
        restore_tv_with_readback(installed, disabled, home, fresh_home, &readback)
    }

    fn restore_tv_with_readback(
        installed: &[&str],
        disabled: &[&str],
        home: &[&str],
        fresh_home: &str,
        readback: &str,
    ) -> MockAdb {
        MockAdb::default()
            .on_raw("devices", "serial\tdevice\n")
            // Before "settings get": the profile batch also reads a setting.
            .on_shell("getprop ro.product.brand", "profile")
            .on_shell(
                "(pm list packages --user 0)",
                &batched(&[
                    &package_list(installed),
                    &package_list(disabled),
                    &home_list(home),
                ]),
            )
            .on_shell(
                "settings get",
                &batched(&vec!["null"; super::tracked_setting_keys().len()]),
            )
            .on_shell("query-activities", fresh_home)
            .on_shell("(pm list packages -d --user 0 ", readback)
    }

    fn snapshot(launcher: Option<&str>, disabled: &[&str]) -> Snapshot {
        Snapshot {
            schema_version: SCHEMA_VERSION,
            saved_at: "2026-11-05T00:00:00Z".into(),
            label: None,
            device_name: "TV".into(),
            device_serial: "serial".into(),
            device_type: DeviceType::Unknown,
            android_version: "11".into(),
            disabled_packages: disabled.iter().map(|p| p.to_string()).collect(),
            current_launcher: launcher.map(str::to_string),
            settings: Default::default(),
            absent_settings: Vec::new(),
        }
    }

    fn disabled_any(log: &std::sync::Arc<std::sync::Mutex<Vec<String>>>, pkg: &str) -> bool {
        log.lock()
            .unwrap()
            .iter()
            .any(|c| c == &format!("pm disable-user --user 0 {pkg}"))
    }

    #[tokio::test]
    async fn a_restore_never_disables_the_only_home_app() {
        // Recorded with Projectivy as Home and stock disabled; Projectivy
        // isn't on this TV, so stock is its only launcher.
        let mock = restore_tv(
            &[STOCK, SETTINGS, BLOAT],
            &[],
            &[STOCK, SETTINGS],
            &home_list(&[STOCK, SETTINGS]),
        )
        .on_shell("resolve-activity", &format!("{STOCK}/.MainActivity"));
        let log = mock.shell_log();
        let res = apply_snapshot_impl(
            &state_with(mock),
            "serial",
            &snapshot(Some(PROJECTIVY), &[STOCK, BLOAT]),
        )
        .await
        .unwrap();
        assert_eq!(res.packages_disabled, [BLOAT]);
        assert_eq!(
            res.packages_failed,
            [format!("{STOCK} (kept: only Home app)")]
        );
        assert!(!res.launcher_set);
        assert_eq!(
            res.launcher_message.as_deref(),
            Some("com.spocky.projengmenu isn't installed; skipped")
        );
        assert!(
            res.summary
                .contains("Kept on so the TV keeps a Home screen: com.google.android.tvlauncher."),
            "{}",
            res.summary
        );
        assert!(!disabled_any(&log, STOCK));
    }

    #[tokio::test]
    async fn each_home_app_is_rechecked_right_before_it_goes() {
        // The plan saw Projectivy enabled. When stock's turn comes the Home
        // list says otherwise, or can't be read (Android 8 has no
        // query-activities).
        for (fresh, reason) in [
            (home_list(&[STOCK, SETTINGS]), "only Home app"),
            (
                "Unknown command: package".to_string(),
                "Home apps unreadable",
            ),
        ] {
            let mock = restore_tv(
                &[STOCK, PROJECTIVY, SETTINGS],
                &[],
                &[STOCK, PROJECTIVY, SETTINGS],
                &fresh,
            )
            .on_shell("resolve-activity", &format!("{STOCK}/.MainActivity"));
            let log = mock.shell_log();
            let res = apply_snapshot_impl(&state_with(mock), "serial", &snapshot(None, &[STOCK]))
                .await
                .unwrap();
            assert!(res.packages_disabled.is_empty());
            assert_eq!(res.packages_failed, [format!("{STOCK} (kept: {reason})")]);
            assert!(!disabled_any(&log, STOCK));
        }
    }

    #[tokio::test]
    async fn a_disabled_snapshot_launcher_is_skipped_and_nothing_is_re_enabled() {
        const GTV: &str = "com.google.android.apps.tv.launcherx";
        const WRAITH: &str = "com.google.android.tungsten.setupwraith";
        let mock = restore_tv(
            &[GTV, WRAITH, PROJECTIVY, SETTINGS, BLOAT],
            &[GTV, WRAITH],
            &[PROJECTIVY, SETTINGS],
            &home_list(&[PROJECTIVY, SETTINGS]),
        )
        .on_shell("resolve-activity", &format!("{PROJECTIVY}/.Main"));
        let log = mock.shell_log();
        let res = apply_snapshot_impl(&state_with(mock), "serial", &snapshot(Some(GTV), &[BLOAT]))
            .await
            .unwrap();
        assert!(!res.launcher_set);
        assert_eq!(
            res.launcher_message,
            Some(format!("{GTV} is disabled; enable it first, then restore"))
        );
        assert!(
            res.summary.contains(&format!(
                "Launcher: skipped ({GTV} is disabled; enable it first, then restore)"
            )),
            "{}",
            res.summary
        );
        assert_eq!(res.packages_disabled, [BLOAT]);
        let calls = log.lock().unwrap();
        assert!(
            !calls.iter().any(|c| c.starts_with("pm enable")
                || c.contains("add-role-holder")
                || c.contains("set-home-activity")),
            "a restore must never re-enable a launcher or its setup helper: {calls:?}"
        );
    }

    #[tokio::test]
    async fn a_restore_sets_home_before_it_disables_anything() {
        const MONET: &str = "com.klevico.monet";
        let mock = restore_tv(
            &[PROJECTIVY, MONET, SETTINGS, BLOAT],
            &[],
            &[PROJECTIVY, MONET, SETTINGS],
            &home_list(&[PROJECTIVY, MONET, SETTINGS]),
        )
        .on_shell_seq(
            "resolve-activity",
            &[&format!("{MONET}/.Main"), &format!("{PROJECTIVY}/.Main")],
        );
        let log = mock.shell_log();
        let res = apply_snapshot_impl(
            &state_with(mock),
            "serial",
            &snapshot(Some(PROJECTIVY), &[MONET, BLOAT]),
        )
        .await
        .unwrap();
        assert!(res.launcher_set, "{:?}", res.launcher_message);
        assert_eq!(res.packages_disabled, [BLOAT, MONET]);
        let calls = log.lock().unwrap();
        let switch = calls.iter().position(|c| c.contains("add-role-holder"));
        let first_disable = calls.iter().position(|c| c.starts_with("pm disable-user"));
        assert!(switch.unwrap() < first_disable.unwrap(), "{calls:?}");
    }

    #[tokio::test]
    async fn stock_that_overrides_the_setters_goes_off_and_then_the_switch_finishes() {
        // Shield: an enabled stock launcher wins over the polite setters, so
        // the switch only lands once the snapshot's stock disable has run.
        let stock_home = format!("{STOCK}/.MainActivity");
        let mock = restore_tv(
            &[STOCK, PROJECTIVY, SETTINGS],
            &[],
            &[STOCK, PROJECTIVY, SETTINGS],
            &home_list(&[STOCK, PROJECTIVY, SETTINGS]),
        )
        .on_shell_seq(
            "resolve-activity",
            &[
                &stock_home,
                &stock_home,
                &stock_home,
                &format!("{PROJECTIVY}/.Main"),
            ],
        );
        let res = apply_snapshot_impl(
            &state_with(mock),
            "serial",
            &snapshot(Some(PROJECTIVY), &[STOCK]),
        )
        .await
        .unwrap();
        assert_eq!(res.packages_disabled, [STOCK]);
        assert!(res.launcher_set, "{:?}", res.launcher_message);
    }

    #[tokio::test]
    async fn disable_from_plan_records_invalid_name_without_aborting() {
        // A snapshot file might contain a malformed or injected package name.
        // The invalid entry must go to packages_failed; valid ones still run.
        // Empty stdout = pm success; the readback confirms both valid ones.
        let mock = MockAdb::default().on_shell(
            "(pm list packages -d --user 0 ",
            &batched(&[&package_list(&["com.valid.package", "com.another.valid"])]),
        );
        let pkgs = vec![
            "com.valid.package".to_string(),
            "evil; reboot".to_string(),
            "com.another.valid".to_string(),
        ];
        let out = disable_from_plan(&mock, "serial", &pkgs, no_home()).await;
        assert_eq!(out.disabled, ["com.valid.package", "com.another.valid"]);
        assert_eq!(out.failed.len(), 1, "exactly the invalid entry should fail");
        assert!(
            out.failed[0].contains("invalid package name"),
            "failure reason missing from message: {}",
            out.failed[0]
        );
    }

    #[tokio::test]
    async fn disable_from_plan_records_pm_failure_without_aborting() {
        // `pm disable-user` reports failure via stdout while `adb shell` still
        // exits 0 — that package must land in packages_failed, the others must
        // still succeed.
        let mock = MockAdb::default()
            .on_shell_failure(
                "pm disable-user --user 0 com.fails.pkg",
                "Failure [not installed for user 0]",
            )
            .on_shell(
                "(pm list packages -d --user 0 ",
                &batched(&[&package_list(&["com.ok.one", "com.ok.two"])]),
            );
        let pkgs = vec![
            "com.ok.one".to_string(),
            "com.fails.pkg".to_string(),
            "com.ok.two".to_string(),
        ];
        let out = disable_from_plan(&mock, "serial", &pkgs, no_home()).await;
        assert_eq!(out.disabled, ["com.ok.one", "com.ok.two"]);
        assert_eq!(out.failed, ["com.fails.pkg"]);
    }

    #[tokio::test]
    async fn a_disable_that_exits_nonzero_or_is_not_confirmed_is_a_failure() {
        for mock in [
            MockAdb::default().on_shell_exit("pm disable-user", "", 1),
            // The write looked clean, but user 0 still has it enabled.
            MockAdb::default().on_shell("(pm list packages -d --user 0 ", &batched(&[""])),
            // A readback that never completed proves nothing.
            MockAdb::default().on_shell(
                "(pm list packages -d --user 0 ",
                "package:com.example.app\n",
            ),
        ] {
            let out = disable_from_plan(&mock, "TV", &["com.example.app".into()], no_home()).await;
            assert!(out.disabled.is_empty());
            assert_eq!(out.failed.len(), 1, "{:?}", out.failed);
        }
    }

    #[tokio::test]
    async fn an_unreadable_stock_disable_is_not_claimed_but_the_switch_still_finishes() {
        // The stock disable may have landed even though its readback was cut
        // off, so the launcher switch that needs stock off still runs.
        let stock_home = format!("{STOCK}/.MainActivity");
        let mock = restore_tv_with_readback(
            &[STOCK, PROJECTIVY, SETTINGS],
            &[],
            &[STOCK, PROJECTIVY, SETTINGS],
            &home_list(&[STOCK, PROJECTIVY, SETTINGS]),
            &format!("package:{STOCK}\n"),
        )
        .on_shell_seq(
            "resolve-activity",
            &[
                &stock_home,
                &stock_home,
                &stock_home,
                &format!("{PROJECTIVY}/.Main"),
            ],
        );
        let res = apply_snapshot_impl(
            &state_with(mock),
            "serial",
            &snapshot(Some(PROJECTIVY), &[STOCK]),
        )
        .await
        .unwrap();
        assert!(res.packages_disabled.is_empty());
        assert_eq!(res.packages_failed.len(), 1, "{:?}", res.packages_failed);
        assert!(
            res.packages_failed[0].contains("could not be read back"),
            "{:?}",
            res.packages_failed
        );
        assert!(res.launcher_set, "{:?}", res.launcher_message);
    }
}

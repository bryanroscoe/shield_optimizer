//! Snapshot save / load / apply-plan commands.

use std::path::PathBuf;

use serde::Serialize;
use tauri::State;

use crate::adb::{parse_disabled_packages_output, parse_installed_packages_output};
use crate::engine::snapshot::{
    compute_apply_plan, tracked_setting_keys, ApplyPlanInputs, Snapshot, SnapshotApplyPlan,
    SCHEMA_VERSION,
};

use super::AppState;

#[derive(Serialize)]
pub struct SnapshotFile {
    pub path: String,
    pub filename: String,
    pub saved_at: String,
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
    let path = PathBuf::from(&snapshot_path);
    let canonical_path = tokio::fs::canonicalize(&path)
        .await
        .map_err(|e| format!("snapshot path: {e}"))?;
    let canonical_dir = tokio::fs::canonicalize(&state.snapshot_dir)
        .await
        .map_err(|e| format!("snapshot dir: {e}"))?;
    if !canonical_path.starts_with(&canonical_dir) {
        return Err("snapshot path is outside the snapshot directory".into());
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
    let _ = tokio::fs::create_dir_all(&state.snapshot_dir).await;
    Ok(state.snapshot_dir.display().to_string())
}

/// `list_snapshots` — return saved snapshots in `snapshot_dir`, newest first.
#[tauri::command]
pub async fn list_snapshots(state: State<'_, AppState>) -> Result<Vec<SnapshotFile>, String> {
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
            device_name: snap.device_name,
            device_serial: snap.device_serial,
            device_type: snap.device_type,
            disabled_count: snap.disabled_packages.len(),
            settings_count: snap.settings.len(),
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
) -> Result<SnapshotFile, String> {
    let adb = state.adb_snapshot().await;

    // Pull all the inputs the engine needs.
    let disabled_out = adb
        .shell(&serial, "pm list packages -d")
        .await
        .map_err(|e| format!("pm list packages -d: {e}"))?;
    let disabled_packages = parse_disabled_packages_output(&disabled_out.stdout);

    // Current launcher.
    let launcher_out = adb
        .shell(
            &serial,
            "cmd package resolve-activity --brief -a android.intent.action.MAIN -c android.intent.category.HOME",
        )
        .await
        .map_err(|e| format!("resolve-activity: {e}"))?;
    let current_launcher = launcher_out
        .stdout
        .lines()
        .map(str::trim)
        .find(|l| l.contains('/'))
        .and_then(|c| c.split_once('/'))
        .map(|(p, _)| p.to_string());

    // Batch the `settings get` queries into one shell call. Output each
    // value on its own line in declared order so we can match them up
    // positionally. ~200ms total instead of ~200ms × 9.
    let keys = tracked_setting_keys();
    let cmd = keys
        .iter()
        .map(|(ns, key)| format!("settings get {ns} {key}"))
        .collect::<Vec<_>>()
        .join("; ");
    let mut settings = std::collections::BTreeMap::new();
    if let Ok(out) = adb.shell(&serial, &cmd).await {
        for ((ns, key), raw) in keys.iter().zip(out.stdout.lines()) {
            let v = raw.trim();
            if !v.is_empty() && v != "null" {
                settings.insert(format!("{ns}.{key}"), v.to_string());
            }
        }
    }

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
        device_name: device_name.clone(),
        device_serial: serial.clone(),
        device_type,
        android_version,
        disabled_packages,
        current_launcher,
        settings,
    };

    // Write to disk.
    let dir = state.snapshot_dir.clone();
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| format!("create snapshot dir: {e}"))?;

    let safe_name: String = device_name
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
        device_name: snap.device_name,
        device_serial: snap.device_serial,
        device_type: snap.device_type,
        disabled_count: snap.disabled_packages.len(),
        settings_count: snap.settings.len(),
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
    // Confine reads to the configured snapshot directory — the frontend
    // hands us paths and we should not blindly read arbitrary locations.
    let path = PathBuf::from(&snapshot_path);
    let canonical_path = tokio::fs::canonicalize(&path)
        .await
        .map_err(|e| format!("snapshot path: {e}"))?;
    let canonical_dir = tokio::fs::canonicalize(&state.snapshot_dir)
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
    let snap = Snapshot::from_json(&contents).map_err(|e| format!("parse snapshot: {e}"))?;

    let adb = state.adb_snapshot().await;
    let (installed_res, disabled_res) = tokio::join!(
        adb.shell(&serial, "pm list packages"),
        adb.shell(&serial, "pm list packages -d"),
    );
    let installed = installed_res.map_err(|e| format!("pm list packages: {e}"))?;
    let disabled = disabled_res.map_err(|e| format!("pm list packages -d: {e}"))?;

    let installed_pkgs = parse_installed_packages_output(&installed.stdout);
    let disabled_pkgs = parse_disabled_packages_output(&disabled.stdout);

    let device = crate::commands::devices::device_profile_impl(state.inner(), &serial).await?;

    let plan = compute_apply_plan(
        &snap,
        &ApplyPlanInputs {
            target_device_type: device.device_type,
            currently_installed: &installed_pkgs,
            currently_disabled: &disabled_pkgs,
        },
    );
    Ok(plan)
}

#[derive(Serialize)]
pub struct ApplyResult {
    pub packages_disabled: Vec<String>,
    pub packages_failed: Vec<String>,
    pub launcher_set: bool,
    pub launcher_message: Option<String>,
    pub settings_written: Vec<String>,
    pub settings_failed: Vec<String>,
    pub summary: String,
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
    // Same containment check as preview_apply.
    let path = PathBuf::from(&snapshot_path);
    let canonical_path = tokio::fs::canonicalize(&path)
        .await
        .map_err(|e| format!("snapshot path: {e}"))?;
    let canonical_dir = tokio::fs::canonicalize(&state.snapshot_dir)
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
    let snap = Snapshot::from_json(&contents).map_err(|e| format!("parse snapshot: {e}"))?;

    let adb = state.adb_snapshot().await;
    let (installed_res, disabled_res) = tokio::join!(
        adb.shell(&serial, "pm list packages"),
        adb.shell(&serial, "pm list packages -d"),
    );
    let installed = installed_res.map_err(|e| format!("pm list packages: {e}"))?;
    let disabled = disabled_res.map_err(|e| format!("pm list packages -d: {e}"))?;
    let installed_pkgs = parse_installed_packages_output(&installed.stdout);
    let disabled_pkgs = parse_disabled_packages_output(&disabled.stdout);

    let device = crate::commands::devices::device_profile_impl(state.inner(), &serial).await?;
    let plan = compute_apply_plan(
        &snap,
        &ApplyPlanInputs {
            target_device_type: device.device_type,
            currently_installed: &installed_pkgs,
            currently_disabled: &disabled_pkgs,
        },
    );

    // 1. Disable packages from the plan (additive only — never re-enable).
    // Defense-in-depth: refuse to disable anything on the NEVER_DISABLE list
    // even if a malformed snapshot lists it. Categorized as "failed" so the
    // UI surfaces it instead of silently ignoring.
    let mut packages_disabled = Vec::new();
    let mut packages_failed = Vec::new();
    for pkg in &plan.packages_to_disable {
        if crate::engine::is_never_disable(pkg) {
            packages_failed.push(format!("{pkg} (refused: brick risk)"));
            continue;
        }
        let cmd = format!("pm disable-user --user 0 {pkg}");
        match adb.shell(&serial, &cmd).await {
            Ok(out) if !out.stdout.contains("Failure") && !out.stdout.contains("Error") => {
                packages_disabled.push(pkg.clone());
            }
            _ => packages_failed.push(pkg.clone()),
        }
    }

    // 2. Set launcher if requested.
    let mut launcher_set = false;
    let mut launcher_message = None;
    if let Some(launcher_pkg) = &plan.launcher_to_set {
        // Reuse the multi-strategy set-default helper from the launcher module.
        let result = crate::commands::launcher::set_default_launcher_impl(
            state.inner(),
            &serial,
            launcher_pkg,
        )
        .await;
        if let Ok(r) = result {
            launcher_set = r.ok;
            launcher_message = if r.ok {
                Some(format!(
                    "{launcher_pkg} via {}",
                    r.strategy.unwrap_or_default()
                ))
            } else {
                r.last_error
            };
        }
    }

    // 3. Write tracked settings — batch into a single shell call.
    let mut settings_written = Vec::new();
    let mut settings_failed = Vec::new();
    if !plan.settings_to_write.is_empty() {
        let mut parts = Vec::new();
        let mut keys: Vec<&String> = Vec::new();
        for (k, v) in &plan.settings_to_write {
            // Key shape is `"ns.subkey"` per the snapshot schema.
            if let Some((ns, key)) = k.split_once('.') {
                parts.push(format!("settings put {ns} {key} {v}"));
                keys.push(k);
            }
        }
        let cmd = parts.join("; ");
        match adb.shell(&serial, &cmd).await {
            Ok(_) => {
                for k in keys {
                    settings_written.push(k.clone());
                }
            }
            Err(e) => {
                for k in keys {
                    settings_failed.push(format!("{k}: {e}"));
                }
            }
        }
    }

    let summary = format!(
        "Disabled {} packages ({} failed). Launcher: {}. {} settings written.",
        packages_disabled.len(),
        packages_failed.len(),
        if launcher_set { "set" } else { "unchanged" },
        settings_written.len()
    );

    Ok(ApplyResult {
        packages_disabled,
        packages_failed,
        launcher_set,
        launcher_message,
        settings_written,
        settings_failed,
        summary,
    })
}

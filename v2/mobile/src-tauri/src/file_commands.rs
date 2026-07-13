//! Mobile file-transfer & backups commands (design §7.2 / §7.3).
//!
//! These are the mobile-appropriate counterparts to the desktop file manager
//! (`v2/src-tauri/src/commands/files.rs`), which is desktop-only (local FS on
//! the same machine as `adb`). On a phone there is no shared filesystem, so:
//!
//! - **pull** streams a device file into the app's own scoped storage
//!   (`data_dir/downloads` or `data_dir/backups`) — no Android SAF needed for
//!   app-private writes.
//! - **push** would need a SAF document picker to reach a user-chosen source
//!   file; that is a documented follow-up and not implemented here.
//!
//! All device I/O rides the shared `AdbDriver` seam (shell + `raw_transfer`),
//! keeping `crates/core/engine` pure. The transfer itself is implemented by
//! `WirelessAdb::raw_transfer` via `adb_client`'s sync service.

use chrono::{DateTime, Utc};
use serde::Serialize;
use shield_optimizer_core::adb::{parse_ls_output, FileEntry};
use shield_optimizer_core::commands::AppState;
use tauri::State;

/// A file pulled off the device into the app's scoped `downloads/` dir.
#[derive(Serialize)]
pub struct PulledFile {
    pub name: String,
    pub path: String,
    pub size_bytes: u64,
}

/// One APK backup living in the app's scoped `backups/` dir.
#[derive(Serialize)]
pub struct BackupEntry {
    pub package: String,
    pub path: String,
    pub size_bytes: u64,
    /// ISO-8601 (UTC) of the backup file's last-modified time.
    pub saved_at: String,
}

/// Validate a device path for browsing/pull: absolute, no `..` traversal, no
/// control characters (they'd corrupt the shell line even single-quoted). The
/// same injection guards the desktop file manager uses, minus the `/sdcard`
/// confinement so the UI can browse `/storage/emulated/0` too.
fn validate_device_path(path: &str) -> Result<String, String> {
    let p = path.trim();
    if !p.starts_with('/') {
        return Err(format!("Path must be absolute: {p:?}"));
    }
    if p.split('/').any(|seg| seg == "..") {
        return Err("Path traversal (`..`) is not allowed.".to_string());
    }
    if p.chars().any(|c| c.is_control()) {
        return Err("Path contains control characters.".to_string());
    }
    Ok(p.to_string())
}

/// Single-quote a validated device path for the device-side shell.
fn quote_path(p: &str) -> String {
    format!("'{}'", p.replace('\'', r"'\''"))
}

/// A package name is `[A-Za-z0-9._]` — reject anything else so it can never
/// break out of the `pm path` shell word.
fn validate_package(pkg: &str) -> Result<String, String> {
    let p = pkg.trim();
    if p.is_empty()
        || !p
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_')
    {
        return Err(format!("Invalid package name: {pkg:?}"));
    }
    Ok(p.to_string())
}

fn basename(path: &str) -> Option<&str> {
    path.rsplit('/').next().filter(|s| !s.is_empty())
}

/// `list_remote_dir` — browse a directory on the connected TV. Returns the
/// parsed `ls -lA` listing, directories first then files (case-insensitive).
#[tauri::command]
pub async fn list_remote_dir(
    state: State<'_, AppState>,
    serial: String,
    path: String,
) -> Result<Vec<FileEntry>, String> {
    let dir = validate_device_path(&path)?;
    let adb = state.adb_snapshot().await;
    // Trailing slash forces `ls` to dereference the directory rather than
    // stat the entry itself.
    let slashed = format!("{}/", dir.trim_end_matches('/'));
    let out = adb
        .shell(&serial, &format!("ls -lA {}", quote_path(&slashed)))
        .await
        .map_err(|e| format!("ls: {e}"))?;
    let combined = format!("{}{}", out.stdout, out.stderr);
    if combined.contains("No such file or directory") {
        return Err(format!("No such directory: {dir}"));
    }
    if combined.contains("Permission denied") && out.stdout.trim().is_empty() {
        return Err(format!("Permission denied: {dir}"));
    }
    let mut entries = parse_ls_output(&out.stdout);
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(entries)
}

/// `pull_file` — download one device file into the app's scoped `downloads/`
/// dir and return where it landed. App-private write, so no SAF is required.
#[tauri::command]
pub async fn pull_file(
    state: State<'_, AppState>,
    serial: String,
    remote_path: String,
) -> Result<PulledFile, String> {
    let remote = validate_device_path(&remote_path)?;
    let name = basename(&remote)
        .ok_or_else(|| format!("Not a file path: {remote}"))?
        .to_string();
    let dir = state.data_dir.join("downloads");
    std::fs::create_dir_all(&dir).map_err(|e| format!("create downloads dir: {e}"))?;
    let local = dir.join(&name);
    let local_str = local.to_string_lossy().into_owned();

    let adb = state.adb_snapshot().await;
    adb.raw_transfer(&["-s", &serial, "pull", &remote, &local_str])
        .await
        .map_err(|e| format!("pull: {e}"))?;

    let size_bytes = std::fs::metadata(&local).map(|m| m.len()).unwrap_or(0);
    Ok(PulledFile {
        name,
        path: local_str,
        size_bytes,
    })
}

/// `backup_apk` — resolve a package's base APK with `pm path`, then pull it into
/// the app's scoped `backups/` dir as `<package>.apk`.
#[tauri::command]
pub async fn backup_apk(
    state: State<'_, AppState>,
    serial: String,
    package: String,
) -> Result<BackupEntry, String> {
    let pkg = validate_package(&package)?;
    let adb = state.adb_snapshot().await;
    let out = adb
        .shell(&serial, &format!("pm path {pkg}"))
        .await
        .map_err(|e| format!("pm path: {e}"))?;

    // `pm path` emits `package:/path/to/base.apk` (one line per split APK).
    // Prefer the base split; fall back to the first path.
    let paths: Vec<&str> = out
        .stdout
        .lines()
        .filter_map(|l| l.trim().strip_prefix("package:"))
        .filter(|p| !p.is_empty())
        .collect();
    let apk_path = paths
        .iter()
        .find(|p| p.ends_with("base.apk"))
        .or_else(|| paths.first())
        .ok_or_else(|| format!("No APK found for {pkg} — is it installed?"))?
        .to_string();

    let dir = state.data_dir.join("backups");
    std::fs::create_dir_all(&dir).map_err(|e| format!("create backups dir: {e}"))?;
    let local = dir.join(format!("{pkg}.apk"));
    let local_str = local.to_string_lossy().into_owned();

    adb.raw_transfer(&["-s", &serial, "pull", &apk_path, &local_str])
        .await
        .map_err(|e| format!("pull apk: {e}"))?;

    let meta = std::fs::metadata(&local).map_err(|e| format!("stat backup: {e}"))?;
    Ok(BackupEntry {
        package: pkg,
        path: local_str,
        size_bytes: meta.len(),
        saved_at: file_modified_iso(&meta),
    })
}

/// `list_backups` — every `<package>.apk` in the app's scoped `backups/` dir,
/// newest first. An absent dir is not an error — returns an empty list.
#[tauri::command]
pub async fn list_backups(state: State<'_, AppState>) -> Result<Vec<BackupEntry>, String> {
    let dir = state.data_dir.join("backups");
    let read = match std::fs::read_dir(&dir) {
        Ok(r) => r,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("read backups dir: {e}")),
    };
    let mut out = Vec::new();
    for entry in read.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("apk") {
            continue;
        }
        let Some(package) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let Ok(meta) = entry.metadata() else { continue };
        out.push(BackupEntry {
            package: package.to_string(),
            path: path.to_string_lossy().into_owned(),
            size_bytes: meta.len(),
            saved_at: file_modified_iso(&meta),
        });
    }
    // Newest first (ISO-8601 sorts lexicographically).
    out.sort_by(|a, b| b.saved_at.cmp(&a.saved_at));
    Ok(out)
}

/// Format a file's last-modified time as ISO-8601 UTC, or empty on error.
fn file_modified_iso(meta: &std::fs::Metadata) -> String {
    meta.modified()
        .ok()
        .map(|t| DateTime::<Utc>::from(t).to_rfc3339())
        .unwrap_or_default()
}

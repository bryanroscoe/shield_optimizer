//! File manager — browse / download / upload / delete on the device's user
//! storage. Confined to `/sdcard`: system paths stay out of reach, which is
//! the same foot-gun avoidance v1 practiced (and aTV Tools doesn't).

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::State;

use shield_optimizer_core::commands::files::{
    delete_path_with, find_files_with, validate_device_path, validate_sdcard_path, FindResult,
};

use crate::adb::{parse_ls_output, FileEntry};

use super::AppState;

/// Single-quote a validated device path for the device-side shell.
fn quote_path(p: &str) -> String {
    format!("'{}'", p.replace('\'', r"'\''"))
}

/// `list_dir` — entries of a directory under `/sdcard`, folders first.
#[tauri::command]
pub async fn list_dir(
    state: State<'_, AppState>,
    serial: String,
    path: String,
    allow_system: bool,
) -> Result<Vec<FileEntry>, String> {
    let path = validate_device_path(&path, allow_system)?;
    let adb = state.adb_snapshot().await;
    // Trailing slash matters: `/sdcard` is itself a symlink (to
    // /storage/self/primary), and `ls -lA` on a bare symlink lists the link
    // line instead of the directory contents. The slash forces dereference.
    let slashed = format!("{}/", path.trim_end_matches('/'));
    let out = adb
        .shell(&serial, &format!("ls -lA {}", quote_path(&slashed)))
        .await
        .map_err(|e| format!("ls: {e}"))?;
    let combined = out.combined();
    if combined.contains("No such file or directory") {
        return Err(format!("No such directory: {path}"));
    }
    if combined.contains("Permission denied") && out.stdout.trim().is_empty() {
        return Err(format!("Permission denied: {path}"));
    }
    let mut entries = parse_ls_output(&out.stdout);
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(entries)
}

#[derive(Serialize)]
pub struct FileTransferResult {
    pub ok: bool,
    pub message: String,
    /// Local path of the downloaded file (pull only).
    pub local_path: Option<String>,
}

/// `pull_file` — download one file from the device into `local_dir`.
#[tauri::command]
pub async fn pull_file(
    state: State<'_, AppState>,
    serial: String,
    remote_path: String,
    local_dir: String,
    allow_system: bool,
) -> Result<FileTransferResult, String> {
    let remote = validate_device_path(&remote_path, allow_system)?;
    let dir = PathBuf::from(&local_dir);
    if !dir.is_dir() {
        return Err(format!("Not a folder: {local_dir}"));
    }
    let file_name = Path::new(&remote)
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| format!("Not a file path: {remote}"))?;
    let local = dir.join(file_name);
    let local_str = local.display().to_string();

    let adb = state.adb_snapshot().await;
    // `adb pull` takes the remote path as a plain argument — no device-side
    // shell involved, so no quoting needed (spaces and specials are fine).
    adb.raw_transfer(&["-s", &serial, "pull", &remote, &local_str])
        .await
        .map_err(|e| format!("pull {remote}: {e}"))?;
    Ok(FileTransferResult {
        ok: true,
        message: format!("Downloaded {file_name} to {local_dir}."),
        local_path: Some(local_str),
    })
}

/// `push_file` — upload one local file into a device directory.
#[tauri::command]
pub async fn push_file(
    state: State<'_, AppState>,
    serial: String,
    local_path: String,
    remote_dir: String,
    allow_system: bool,
) -> Result<FileTransferResult, String> {
    let remote_dir = validate_device_path(&remote_dir, allow_system)?;
    let local = PathBuf::from(&local_path);
    if !local.is_file() {
        return Err(format!("Not a file: {local_path}"));
    }
    let file_name = local
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| format!("Unreadable file name: {local_path}"))?;
    let remote = format!("{}/{}", remote_dir.trim_end_matches('/'), file_name);

    let adb = state.adb_snapshot().await;
    adb.raw_transfer(&["-s", &serial, "push", &local_path, &remote])
        .await
        .map_err(|e| format!("push {file_name}: {e}"))?;
    Ok(FileTransferResult {
        ok: true,
        message: format!("Uploaded {file_name} to {remote_dir}."),
        local_path: None,
    })
}

/// `copy_file_to_device` — pull a file from one connected device and push it
/// to another's `/sdcard`. Both paths are `/sdcard`-confined; the file lands
/// in `target_dir` under its original name. A temp file on this computer
/// bridges the two transfers and is cleaned up either way.
#[tauri::command]
pub async fn copy_file_to_device(
    state: State<'_, AppState>,
    source_serial: String,
    remote_path: String,
    target_serial: String,
    target_dir: String,
) -> Result<FileTransferResult, String> {
    let remote = validate_sdcard_path(&remote_path)?;
    let target_dir = validate_sdcard_path(&target_dir)?;
    if source_serial == target_serial {
        return Err("Source and target device are the same.".to_string());
    }
    let file_name = Path::new(&remote)
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| format!("Not a file path: {remote}"))?
        .to_string();

    let temp = std::env::temp_dir().join(format!("shield-filecopy-{}", sanitize_temp(&file_name)));
    let temp_str = temp.display().to_string();
    let adb = state.adb_snapshot().await;

    let result = async {
        adb.raw_transfer(&["-s", &source_serial, "pull", &remote, &temp_str])
            .await
            .map_err(|e| format!("pull {remote}: {e}"))?;
        let dest = format!("{}/{file_name}", target_dir.trim_end_matches('/'));
        adb.raw_transfer(&["-s", &target_serial, "push", &temp_str, &dest])
            .await
            .map_err(|e| format!("push to {target_serial}: {e}"))?;
        Ok::<FileTransferResult, String>(FileTransferResult {
            ok: true,
            message: format!("Copied {file_name} to {target_serial}:{target_dir}."),
            local_path: None,
        })
    }
    .await;

    let _ = tokio::fs::remove_file(&temp).await;
    result
}

/// Flatten a file name for use as a temp-file stem (avoids odd characters in
/// the host temp path; the real name is reapplied on push).
fn sanitize_temp(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

/// `delete_path` — remove a file or directory (recursively). Confined to
/// `/sdcard` unless `allow_system` (power-user) is set; the UI confirms before
/// calling, and core's `protected_delete_reason` still blocks catastrophic
/// targets.
#[tauri::command]
pub async fn delete_path(
    state: State<'_, AppState>,
    serial: String,
    path: String,
    allow_system: bool,
) -> Result<FileTransferResult, String> {
    let adb = state.adb_snapshot().await;
    let r = delete_path_with(adb.as_ref(), &serial, &path, allow_system).await?;
    Ok(FileTransferResult {
        ok: r.ok,
        message: r.message,
        local_path: None,
    })
}

/// `find_files` — locate files matching a name pattern under one or more
/// `/sdcard` directories. Powers the app-backup finder.
#[tauri::command]
pub async fn find_files(
    state: State<'_, AppState>,
    serial: String,
    dirs: Vec<String>,
    pattern: String,
) -> Result<FindResult, String> {
    let adb = state.adb_snapshot().await;
    find_files_with(adb.as_ref(), &serial, &dirs, &pattern).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn quotes_single_quotes_in_paths() {
        assert_eq!(quote_path("/sdcard/it's here"), r"'/sdcard/it'\''s here'");
    }
}

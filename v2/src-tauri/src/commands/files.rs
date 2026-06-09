//! File manager — browse / download / upload / delete on the device's user
//! storage. Confined to `/sdcard`: system paths stay out of reach, which is
//! the same foot-gun avoidance v1 practiced (and aTV Tools doesn't).

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::State;

use crate::adb::{parse_ls_output, FileEntry};

use super::AppState;

/// Validate a device path for file-manager use. Always absolute, no `..`
/// traversal, no control characters (they'd corrupt the shell line even
/// quoted). When `allow_system` is false the path must also live under
/// `/sdcard`; power-user mode lifts only that boundary — the injection guards
/// stay. Returns the trimmed path.
fn validate_device_path(path: &str, allow_system: bool) -> Result<String, String> {
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
    if !allow_system && p != "/sdcard" && !p.starts_with("/sdcard/") {
        return Err(format!("Path must be under /sdcard: {p:?}"));
    }
    Ok(p.to_string())
}

/// `/sdcard`-confined validation — for the paths that are user-storage only by
/// design (the device-to-device copy and the backup finder).
fn validate_sdcard_path(path: &str) -> Result<String, String> {
    validate_device_path(path, false)
}

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

/// Refuse deleting the filesystem root, `/sdcard` itself, or a critical system
/// mount even in power-user mode — a recursive delete there could brick the
/// device. Subpaths are the user's call (and mostly permission-denied without
/// root). Returns the refusal message, or `None` if the path is deletable.
fn protected_delete_reason(path: &str) -> Option<String> {
    let trimmed = path.trim_end_matches('/');
    if trimmed == "/sdcard" {
        return Some("Refusing to delete /sdcard itself.".to_string());
    }
    const PROTECTED: &[&str] = &[
        "", "/system", "/data", "/vendor", "/proc", "/sys", "/dev", "/boot", "/init", "/sbin",
        "/bin", "/etc",
    ];
    if PROTECTED.contains(&trimmed) {
        return Some(format!(
            "Refusing to delete a protected system path: {path}"
        ));
    }
    None
}

/// `delete_path` — remove a file or directory (recursively). Confined to
/// `/sdcard` unless `allow_system` (power-user) is set; the UI confirms before
/// calling, and `protected_delete_reason` still blocks catastrophic targets.
#[tauri::command]
pub async fn delete_path(
    state: State<'_, AppState>,
    serial: String,
    path: String,
    allow_system: bool,
) -> Result<FileTransferResult, String> {
    let path = validate_device_path(&path, allow_system)?;
    if let Some(reason) = protected_delete_reason(&path) {
        return Err(reason);
    }
    let adb = state.adb_snapshot().await;
    let out = adb
        .shell(&serial, &format!("rm -rf {}", quote_path(&path)))
        .await
        .map_err(|e| format!("rm: {e}"))?;
    let noise = out.combined().trim().to_string();
    if noise.is_empty() {
        Ok(FileTransferResult {
            ok: true,
            message: format!("Deleted {path}."),
            local_path: None,
        })
    } else {
        Ok(FileTransferResult {
            ok: false,
            message: noise,
            local_path: None,
        })
    }
}

/// Filename patterns for `find -name`: glob stars and dots only — no slashes,
/// quotes, or anything the shell could reinterpret.
fn validate_find_pattern(pattern: &str) -> Result<(), String> {
    let ok = !pattern.is_empty()
        && pattern.len() <= 64
        && pattern
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '*' | '.' | '_' | '-'));
    if ok {
        Ok(())
    } else {
        Err(format!("Invalid search pattern: {pattern:?}"))
    }
}

/// `find_files` — locate files matching a name pattern under one or more
/// `/sdcard` directories. Powers the app-backup finder (e.g. Projectivy's
/// `*.plbackup` exports land wherever the user's file picker put them).
#[tauri::command]
pub async fn find_files(
    state: State<'_, AppState>,
    serial: String,
    dirs: Vec<String>,
    pattern: String,
) -> Result<Vec<String>, String> {
    validate_find_pattern(&pattern)?;
    let adb = state.adb_snapshot().await;
    let mut hits = Vec::new();
    for dir in dirs {
        let dir = validate_sdcard_path(&dir)?;
        // Errors (missing dir, permission) are expected for some candidates —
        // suppress them; an empty result is the honest answer.
        let cmd = format!(
            "find {} -maxdepth 4 -type f -name '{pattern}' 2>/dev/null",
            quote_path(&dir)
        );
        let Ok(out) = adb.shell(&serial, &cmd).await else {
            continue;
        };
        for line in out.stdout.lines() {
            let line = line.trim();
            if line.starts_with("/sdcard") && !hits.iter().any(|h| h == line) {
                hits.push(line.to_string());
            }
            if hits.len() >= 100 {
                break;
            }
        }
    }
    Ok(hits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn find_patterns_validated() {
        assert!(validate_find_pattern("*.plbackup").is_ok());
        assert!(validate_find_pattern("backup_*.zip").is_ok());
        assert!(validate_find_pattern("a/b").is_err());
        assert!(validate_find_pattern("x'y").is_err());
        assert!(validate_find_pattern("").is_err());
        assert!(validate_find_pattern(&"x".repeat(65)).is_err());
    }

    #[test]
    fn accepts_sdcard_paths() {
        assert_eq!(validate_sdcard_path("/sdcard").unwrap(), "/sdcard");
        assert_eq!(
            validate_sdcard_path("/sdcard/Download/file 1.mp4").unwrap(),
            "/sdcard/Download/file 1.mp4"
        );
    }

    #[test]
    fn rejects_escapes_and_system_paths() {
        assert!(validate_sdcard_path("/data/data/com.x").is_err());
        assert!(validate_sdcard_path("/sdcard/../data").is_err());
        assert!(validate_sdcard_path("/sdcardX/evil").is_err());
        assert!(validate_sdcard_path("/sdcard/a\nb").is_err());
        assert!(validate_sdcard_path("").is_err());
    }

    #[test]
    fn power_user_mode_allows_system_paths_but_keeps_injection_guards() {
        // System paths are reachable only with allow_system.
        assert!(validate_device_path("/system/app", false).is_err());
        assert_eq!(
            validate_device_path("/system/app", true).unwrap(),
            "/system/app"
        );
        assert_eq!(validate_device_path("/", true).unwrap(), "/");
        // The shell-safety guards never relax.
        assert!(validate_device_path("/system/../x", true).is_err());
        assert!(validate_device_path("/system/a\nb", true).is_err());
        assert!(validate_device_path("relative/path", true).is_err());
    }

    #[test]
    fn protected_paths_are_never_deletable() {
        for p in ["/", "/system", "/data", "/vendor", "/sdcard", "/system/"] {
            assert!(protected_delete_reason(p).is_some(), "{p} must be refused");
        }
        // Real targets are allowed through the guard.
        assert!(protected_delete_reason("/sdcard/Download/old.zip").is_none());
        assert!(protected_delete_reason("/system/app/Bloat/Bloat.apk").is_none());
    }

    #[test]
    fn quotes_single_quotes_in_paths() {
        assert_eq!(quote_path("/sdcard/it's here"), r"'/sdcard/it'\''s here'");
    }
}

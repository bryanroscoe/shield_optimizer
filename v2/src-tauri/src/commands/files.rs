//! File manager — browse / download / upload / delete on the device's user
//! storage. Confined to `/sdcard`: system paths stay out of reach, which is
//! the same foot-gun avoidance v1 practiced (and aTV Tools doesn't).

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::State;

use crate::adb::{parse_ls_output, FileEntry};

use super::AppState;

/// Validate a device path for file-manager use: must live under `/sdcard`,
/// no `..` traversal, no control characters (they'd corrupt the shell line
/// even quoted). Returns the trimmed path.
fn validate_sdcard_path(path: &str) -> Result<String, String> {
    let p = path.trim();
    if p != "/sdcard" && !p.starts_with("/sdcard/") {
        return Err(format!("Path must be under /sdcard: {p:?}"));
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

/// `list_dir` — entries of a directory under `/sdcard`, folders first.
#[tauri::command]
pub async fn list_dir(
    state: State<'_, AppState>,
    serial: String,
    path: String,
) -> Result<Vec<FileEntry>, String> {
    let path = validate_sdcard_path(&path)?;
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
) -> Result<FileTransferResult, String> {
    let remote = validate_sdcard_path(&remote_path)?;
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
    adb.raw(&["-s", &serial, "pull", &remote, &local_str])
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
) -> Result<FileTransferResult, String> {
    let remote_dir = validate_sdcard_path(&remote_dir)?;
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
    adb.raw(&["-s", &serial, "push", &local_path, &remote])
        .await
        .map_err(|e| format!("push {file_name}: {e}"))?;
    Ok(FileTransferResult {
        ok: true,
        message: format!("Uploaded {file_name} to {remote_dir}."),
        local_path: None,
    })
}

/// `delete_path` — remove a file or directory (recursively) under `/sdcard`.
/// The UI confirms before calling; `/sdcard` itself is refused.
#[tauri::command]
pub async fn delete_path(
    state: State<'_, AppState>,
    serial: String,
    path: String,
) -> Result<FileTransferResult, String> {
    let path = validate_sdcard_path(&path)?;
    if path.trim_end_matches('/') == "/sdcard" {
        return Err("Refusing to delete /sdcard itself.".to_string());
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
    fn quotes_single_quotes_in_paths() {
        assert_eq!(quote_path("/sdcard/it's here"), r"'/sdcard/it'\''s here'");
    }
}

//! File-manager guards and device-side file operations shared by desktop and
//! mobile. Both apps wrap these in their own Tauri commands because the local
//! side differs (a host folder on desktop, app-private storage on a phone),
//! but the device-side rules — what may be touched and how a search failure is
//! reported — must not drift between them.

use serde::Serialize;

use crate::adb::AdbDriver;
use crate::commands::apps::ActionResult;
use crate::commands::quote_shell_arg;

/// Validate a device path for file-manager use. Always absolute, no `..`
/// traversal, no control characters (they'd corrupt the shell line even
/// quoted). When `allow_system` is false the path must also live under
/// `/sdcard`; power-user mode lifts only that boundary — the injection guards
/// stay. Returns the trimmed path.
pub fn validate_device_path(path: &str, allow_system: bool) -> Result<String, String> {
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
pub fn validate_sdcard_path(path: &str) -> Result<String, String> {
    validate_device_path(path, false)
}

/// Refuse deleting the filesystem root, `/sdcard` itself, or a critical system
/// mount even in power-user mode — a recursive delete there could brick the
/// device. Subpaths are the user's call (and mostly permission-denied without
/// root). Returns the refusal message, or `None` if the path is deletable.
pub fn protected_delete_reason(path: &str) -> Option<String> {
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

/// Remove a file or directory (recursively). Confined to `/sdcard` unless
/// `allow_system` (desktop power-user mode) is set; callers confirm with the
/// user first, and `protected_delete_reason` still blocks catastrophic
/// targets.
pub async fn delete_path_with(
    adb: &dyn AdbDriver,
    serial: &str,
    path: &str,
    allow_system: bool,
) -> Result<ActionResult, String> {
    let path = validate_device_path(path, allow_system)?;
    if let Some(reason) = protected_delete_reason(&path) {
        return Err(reason);
    }
    let out = adb
        .shell(serial, &format!("rm -rf {}", quote_shell_arg(&path)))
        .await
        .map_err(|e| format!("rm: {e}"))?;
    let noise = out.combined().trim().to_string();
    // The desktop driver turns a nonzero exit into an error above; the mobile
    // transport hands it back as data, so it is checked here too.
    let exited_cleanly = matches!(out.exit_code, None | Some(0));
    if noise.is_empty() && exited_cleanly {
        Ok(ActionResult {
            ok: true,
            message: format!("Deleted {path}."),
        })
    } else if noise.is_empty() {
        Ok(ActionResult {
            ok: false,
            message: format!("The TV did not confirm that {path} was deleted."),
        })
    } else {
        Ok(ActionResult {
            ok: false,
            message: noise,
        })
    }
}

/// Filename patterns for `find -name`: glob stars and dots only — no slashes,
/// quotes, or anything the shell could reinterpret.
pub fn validate_find_pattern(pattern: &str) -> Result<(), String> {
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

/// Most hits one search returns.
pub const MAX_FIND_HITS: usize = 100;

/// Printed after `find` finishes. Its presence is the only proof the search
/// ran to the end: a missing directory makes `find` exit nonzero (which the
/// desktop driver reports as an error, indistinguishable from a dropped
/// connection), while a cut-off stream produces partial output with no error
/// at all.
const FIND_DONE: &str = "__ATVOPT_FIND_DONE__";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FindResult {
    /// Matching file paths, capped at [`MAX_FIND_HITS`].
    pub hits: Vec<String>,
    /// Directories the search could not be run against at all, because the ADB
    /// call itself failed. A directory that simply does not exist is *not*
    /// listed here — that is a real "no matches". Without this split, a
    /// dropped connection rendered as "export from the app first", telling the
    /// user to redo something that had worked (GitHub #86).
    pub unsearched: Vec<String>,
}

/// Locate files matching a name pattern under one or more `/sdcard`
/// directories. Powers the app-backup finder (e.g. Projectivy's `*.plbackup`
/// exports land wherever the user's file picker put them).
pub async fn find_files_with(
    adb: &dyn AdbDriver,
    serial: &str,
    dirs: &[String],
    pattern: &str,
) -> Result<FindResult, String> {
    validate_find_pattern(pattern)?;
    let dirs = dirs
        .iter()
        .map(|dir| validate_sdcard_path(dir))
        .collect::<Result<Vec<_>, _>>()?;
    let mut hits: Vec<String> = Vec::new();
    let mut unsearched = Vec::new();
    for dir in dirs {
        // A missing directory or a permission denial is expected for some
        // candidates, and `find` reports those on stderr — suppress them, since
        // an empty result is the honest answer. The trailing marker keeps the
        // exit status 0 and proves the search completed.
        let cmd = format!(
            "find {} -maxdepth 4 -type f -name '{pattern}' 2>/dev/null; echo {FIND_DONE}",
            quote_shell_arg(&dir)
        );
        let out = match adb.shell(serial, &cmd).await {
            Ok(out) if out.stdout.lines().any(|line| line.trim() == FIND_DONE) => out,
            _ => {
                unsearched.push(dir);
                continue;
            }
        };
        for line in out.stdout.lines() {
            if hits.len() >= MAX_FIND_HITS {
                break;
            }
            let line = line.trim();
            if line.starts_with("/sdcard/") && !hits.iter().any(|h| h == line) {
                hits.push(line.to_string());
            }
        }
    }
    Ok(FindResult { hits, unsearched })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::test_support::MockAdb;
    use pretty_assertions::assert_eq;

    fn dirs(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

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
        assert!(validate_sdcard_path("/storage/emulated/0/x").is_err());
        assert!(validate_sdcard_path("").is_err());
    }

    #[test]
    fn power_user_mode_allows_system_paths_but_keeps_injection_guards() {
        assert!(validate_device_path("/system/app", false).is_err());
        assert_eq!(
            validate_device_path("/system/app", true).unwrap(),
            "/system/app"
        );
        assert_eq!(validate_device_path("/", true).unwrap(), "/");
        assert!(validate_device_path("/system/../x", true).is_err());
        assert!(validate_device_path("/system/a\nb", true).is_err());
        assert!(validate_device_path("relative/path", true).is_err());
    }

    #[test]
    fn protected_paths_are_never_deletable() {
        for p in [
            "/", "/system", "/data", "/vendor", "/sdcard", "/system/", "/sdcard/",
        ] {
            assert!(protected_delete_reason(p).is_some(), "{p} must be refused");
        }
        assert!(protected_delete_reason("/sdcard/Download/old.zip").is_none());
        assert!(protected_delete_reason("/system/app/Bloat/Bloat.apk").is_none());
    }

    const SMARTTUBE_DIRS: &[&str] = &[
        "/sdcard/Documents/SmartTubeBackup",
        "/sdcard/SmartTubeBackup",
        "/sdcard/Android/data/com.teamsmart.videomanager.tv",
    ];

    #[tokio::test]
    async fn missing_directories_are_no_matches_not_unsearched() {
        let hit = "/sdcard/Documents/SmartTubeBackup/org.smarttube.stable_1.zip";
        let mock = MockAdb::default()
            .on_shell(
                "'/sdcard/Documents/SmartTubeBackup'",
                &format!("{hit}\n{FIND_DONE}\n"),
            )
            // `find` printed nothing for a directory that isn't there, but the
            // marker still ran — that is an honest empty search.
            .on_shell("'/sdcard/SmartTubeBackup'", &format!("{FIND_DONE}\n"))
            .on_shell("'/sdcard/Android/data/", &format!("{FIND_DONE}\n"));
        let log = mock.shell_log();
        let result = find_files_with(&mock, "tv", &dirs(SMARTTUBE_DIRS), "*.zip")
            .await
            .unwrap();
        assert_eq!(
            result,
            FindResult {
                hits: vec![hit.to_string()],
                unsearched: vec![],
            }
        );
        let log = log.lock().unwrap();
        assert_eq!(log.len(), 3);
        assert!(log[0].contains("-name '*.zip'"));
    }

    #[tokio::test]
    async fn a_failed_call_or_a_cut_off_stream_is_unsearched() {
        let mock = MockAdb::default()
            .on_shell_err("'/sdcard/Documents/SmartTubeBackup'", "device offline")
            // Output without the completion marker: the stream ended early.
            .on_shell(
                "'/sdcard/SmartTubeBackup'",
                "/sdcard/SmartTubeBackup/partial.zip\n",
            )
            .on_shell("'/sdcard/Android/data/", &format!("{FIND_DONE}\n"));
        let result = find_files_with(&mock, "tv", &dirs(SMARTTUBE_DIRS), "*.zip")
            .await
            .unwrap();
        assert!(result.hits.is_empty());
        assert_eq!(
            result.unsearched,
            dirs(&SMARTTUBE_DIRS[..2]),
            "a search that never finished must not read as no matches"
        );
    }

    #[tokio::test]
    async fn find_rejects_bad_dirs_before_touching_the_device() {
        let mock = MockAdb::default();
        let log = mock.shell_log();
        assert!(
            find_files_with(&mock, "tv", &dirs(&["/sdcard/ok", "/data"]), "*.zip")
                .await
                .is_err()
        );
        assert!(
            find_files_with(&mock, "tv", &dirs(&["/sdcard"]), "*.zip'; rm")
                .await
                .is_err()
        );
        assert!(log.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn hits_are_capped_and_deduplicated() {
        let mut many: String = (0..150)
            .map(|i| format!("/sdcard/Download/f{i}.zip\n"))
            .collect();
        many.push_str("/sdcard/Download/f0.zip\n");
        many.push_str(FIND_DONE);
        let mock = MockAdb::default().on_shell("find", &many);
        let result = find_files_with(
            &mock,
            "tv",
            &dirs(&["/sdcard/Download", "/sdcard"]),
            "*.zip",
        )
        .await
        .unwrap();
        assert_eq!(result.hits.len(), MAX_FIND_HITS);
        assert!(result.unsearched.is_empty());
    }

    #[tokio::test]
    async fn delete_is_confined_and_quoted() {
        let mock = MockAdb::default();
        let log = mock.shell_log();
        for bad in [
            "/sdcard",
            "/sdcard/",
            "/sdcard/../data",
            "/data/local/tmp/x",
            "/storage/emulated/0/x",
            "relative",
            "/sdcard/a\nb",
        ] {
            assert!(
                delete_path_with(&mock, "tv", bad, false).await.is_err(),
                "{bad:?} must be refused"
            );
        }
        assert!(log.lock().unwrap().is_empty());

        let r = delete_path_with(&mock, "tv", "/sdcard/Download/it's.zip", false)
            .await
            .unwrap();
        assert!(r.ok, "{}", r.message);
        assert_eq!(
            log.lock().unwrap().as_slice(),
            [r"rm -rf '/sdcard/Download/it'\''s.zip'".to_string()]
        );
    }

    #[tokio::test]
    async fn delete_reports_device_side_failures() {
        let mock = MockAdb::default()
            .on_shell("noisy", "rm: /sdcard/noisy: Permission denied")
            .on_shell_exit("silent", "", 1)
            .on_shell_err("gone", "device offline");
        let noisy = delete_path_with(&mock, "tv", "/sdcard/noisy", false)
            .await
            .unwrap();
        assert!(!noisy.ok);
        assert!(noisy.message.contains("Permission denied"));
        let silent = delete_path_with(&mock, "tv", "/sdcard/silent", false)
            .await
            .unwrap();
        assert!(!silent.ok);
        assert!(delete_path_with(&mock, "tv", "/sdcard/gone", false)
            .await
            .is_err());
    }
}

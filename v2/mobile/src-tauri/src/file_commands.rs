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

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use shield_optimizer_core::adb::{parse_ls_output, FileEntry};
use shield_optimizer_core::commands::{apps::ActionResult, AppState};
use tauri::State;

const BACKUP_SCHEMA_VERSION: u32 = 1;
const BACKUP_MANIFEST: &str = "backup.json";

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
    pub apk_count: usize,
    /// False for base-only backups created by older app versions. Those files
    /// may be missing required split APKs, so the UI must not promise restore.
    pub complete: bool,
    /// ISO-8601 (UTC) of the backup file's last-modified time.
    pub saved_at: String,
}

#[derive(Deserialize, Serialize)]
struct BackupManifest {
    schema_version: u32,
    package: String,
    saved_at: String,
    apk_files: Vec<String>,
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

fn safe_apk_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && name.ends_with(".apk")
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

fn parse_pm_paths(stdout: &str) -> Result<Vec<String>, String> {
    let mut paths = Vec::new();
    for raw in stdout
        .lines()
        .filter_map(|line| line.trim().strip_prefix("package:"))
        .filter(|path| !path.is_empty())
    {
        let path = validate_device_path(raw)?;
        let name = basename(&path).ok_or_else(|| format!("Invalid APK path: {path}"))?;
        if !safe_apk_name(name) {
            return Err(format!("Unsafe APK filename reported by the TV: {name:?}"));
        }
        paths.push(path);
    }
    Ok(paths)
}

fn read_manifest(dir: &Path) -> Result<BackupManifest, String> {
    let manifest_path = dir.join(BACKUP_MANIFEST);
    let bytes = std::fs::read(&manifest_path)
        .map_err(|e| format!("read {}: {e}", manifest_path.display()))?;
    let manifest: BackupManifest = serde_json::from_slice(&bytes)
        .map_err(|e| format!("parse {}: {e}", manifest_path.display()))?;
    if manifest.schema_version != BACKUP_SCHEMA_VERSION {
        return Err(format!(
            "Unsupported APK backup schema {}.",
            manifest.schema_version
        ));
    }
    validate_package(&manifest.package)?;
    if manifest.apk_files.is_empty() || manifest.apk_files.iter().any(|f| !safe_apk_name(f)) {
        return Err("APK backup manifest contains invalid files.".to_string());
    }
    Ok(manifest)
}

fn bundle_files(dir: &Path, manifest: &BackupManifest) -> Result<Vec<PathBuf>, String> {
    let root = dir
        .canonicalize()
        .map_err(|e| format!("open backup bundle: {e}"))?;
    manifest
        .apk_files
        .iter()
        .map(|name| {
            let path = dir.join(name);
            let link_meta = std::fs::symlink_metadata(&path)
                .map_err(|e| format!("open backup APK {name}: {e}"))?;
            if link_meta.file_type().is_symlink() || !link_meta.is_file() {
                return Err(format!("APK backup is missing {name}."));
            }
            let canonical = path
                .canonicalize()
                .map_err(|e| format!("open backup APK {name}: {e}"))?;
            if !canonical.starts_with(&root) {
                return Err(format!("APK backup path escapes its bundle: {name}."));
            }
            Ok(canonical)
        })
        .collect()
}

fn shell_failed(out: &shield_optimizer_core::adb::AdbOutput) -> bool {
    out.exit_code.is_some_and(|code| code != 0) || out.shell_reported_failure()
}

fn bundle_entry(dir: &Path) -> Result<BackupEntry, String> {
    let manifest = read_manifest(dir)?;
    let files = bundle_files(dir, &manifest)?;
    let size_bytes = files.iter().try_fold(0_u64, |total, path| {
        std::fs::metadata(path)
            .map(|meta| total.saturating_add(meta.len()))
            .map_err(|e| format!("stat {}: {e}", path.display()))
    })?;
    Ok(BackupEntry {
        package: manifest.package,
        path: dir.to_string_lossy().into_owned(),
        size_bytes,
        apk_count: files.len(),
        complete: true,
        saved_at: manifest.saved_at,
    })
}

fn confined_backup_path(root: &Path, requested: &str) -> Result<PathBuf, String> {
    let root = root
        .canonicalize()
        .map_err(|e| format!("open backups directory: {e}"))?;
    let requested = PathBuf::from(requested)
        .canonicalize()
        .map_err(|e| format!("open backup: {e}"))?;
    if requested == root || !requested.starts_with(&root) {
        return Err("Backup path is outside this app's backup directory.".to_string());
    }
    Ok(requested)
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

/// Hard cap on one pulled file — 2 GiB. `AdbDriver` exposes no stat, so the
/// remote size isn't knowable up front; the limit is enforced after the
/// transfer by deleting the oversized file rather than leaving it to fill the
/// phone's app storage.
const MAX_PULL_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const BYTES_PER_GIB: f64 = (1024 * 1024 * 1024) as f64;

/// Highest ` (n)` suffix tried before giving up — far past any real use, and
/// it keeps the probe loop bounded.
const MAX_DUPLICATE_SUFFIX: u32 = 1000;

/// Pick a destination in `dir` that doesn't clobber an existing download:
/// `report.txt`, then `report (2).txt`, `report (3).txt`, … with the counter
/// inserted before the extension the way a browser's download dir does it.
fn unique_download_path(dir: &Path, name: &str) -> Result<PathBuf, String> {
    let first = dir.join(name);
    if !first.exists() {
        return Ok(first);
    }
    // A leading dot belongs to the name (`.bashrc`), not to an extension.
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    };
    for n in 2..=MAX_DUPLICATE_SUFFIX {
        let candidate = dir.join(format!("{stem} ({n}){ext}"));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(format!(
        "There are already {MAX_DUPLICATE_SUFFIX} copies of {name} in this app's downloads. \
         Delete some before pulling another."
    ))
}

/// `pull_file` — download one device file into the app's scoped `downloads/`
/// dir and return where it landed. App-private write, so no SAF is required.
/// Existing downloads are kept (` (2)`, ` (3)`, … suffix) and anything over
/// [`MAX_PULL_BYTES`] is discarded instead of stored.
#[tauri::command]
pub async fn pull_file(
    state: State<'_, AppState>,
    serial: String,
    remote_path: String,
) -> Result<PulledFile, String> {
    let remote = validate_device_path(&remote_path)?;
    let remote_name = basename(&remote)
        .ok_or_else(|| format!("Not a file path: {remote}"))?
        .to_string();
    let dir = state.data_dir.join("downloads");
    std::fs::create_dir_all(&dir).map_err(|e| format!("create downloads dir: {e}"))?;
    let local = unique_download_path(&dir, &remote_name)?;
    let local_str = local.to_string_lossy().into_owned();

    let adb = state.adb_snapshot().await;
    adb.raw_transfer(&["-s", &serial, "pull", &remote, &local_str])
        .await
        .map_err(|e| format!("pull: {e}"))?;

    let size_bytes = std::fs::metadata(&local).map(|m| m.len()).unwrap_or(0);
    if size_bytes > MAX_PULL_BYTES {
        let _ = std::fs::remove_file(&local);
        return Err(format!(
            "{remote_name} is {:.1} GiB — over the {:.0} GiB limit for a single download. \
             Nothing was kept on this phone.",
            size_bytes as f64 / BYTES_PER_GIB,
            MAX_PULL_BYTES as f64 / BYTES_PER_GIB,
        ));
    }
    Ok(PulledFile {
        // The saved name, which may carry a ` (2)` suffix — the UI points at
        // the file that actually exists.
        name: local
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&remote_name)
            .to_string(),
        path: local_str,
        size_bytes,
    })
}

/// `backup_apk` — resolve every APK reported by `pm path` and pull the complete
/// set into one versioned bundle. Split APKs are only restorable as a set.
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

    if shell_failed(&out) {
        return Err(format!(
            "Could not resolve {pkg}: {}",
            out.combined().trim()
        ));
    }
    let paths = parse_pm_paths(&out.stdout)?;
    if paths.is_empty() {
        return Err(format!("No APK found for {pkg} — is it installed?"));
    }

    let root = state.data_dir.join("backups");
    std::fs::create_dir_all(&root).map_err(|e| format!("create backups dir: {e}"))?;
    let saved_at = Utc::now();
    let dir = root.join(format!("{pkg}-{}", saved_at.format("%Y%m%dT%H%M%S%9fZ")));
    std::fs::create_dir(&dir).map_err(|e| format!("create backup bundle: {e}"))?;

    let result = async {
        let mut apk_files = Vec::with_capacity(paths.len());
        for (index, remote) in paths.iter().enumerate() {
            let remote_name = basename(remote).expect("validated APK path has a basename");
            let local_name = format!("{index:03}-{remote_name}");
            let local = dir.join(&local_name);
            let local_str = local.to_string_lossy().into_owned();
            adb.raw_transfer(&["-s", &serial, "pull", remote, &local_str])
                .await
                .map_err(|e| format!("pull {remote_name}: {e}"))?;
            let size = std::fs::metadata(&local)
                .map_err(|e| format!("stat {local_name}: {e}"))?
                .len();
            if size == 0 {
                return Err(format!("The TV returned an empty APK: {remote_name}"));
            }
            apk_files.push(local_name);
        }
        let manifest = BackupManifest {
            schema_version: BACKUP_SCHEMA_VERSION,
            package: pkg.clone(),
            saved_at: saved_at.to_rfc3339(),
            apk_files,
        };
        let json = serde_json::to_vec_pretty(&manifest)
            .map_err(|e| format!("encode backup manifest: {e}"))?;
        std::fs::write(dir.join(BACKUP_MANIFEST), json)
            .map_err(|e| format!("write backup manifest: {e}"))?;
        bundle_entry(&dir)
    }
    .await;

    if result.is_err() {
        let _ = std::fs::remove_dir_all(&dir);
    }
    result
}

/// `restore_apk_backup` — push a path-confined complete bundle to a temporary
/// TV directory and install every APK together. Cleanup runs on every result.
#[tauri::command]
pub async fn restore_apk_backup(
    state: State<'_, AppState>,
    serial: String,
    backup_path: String,
) -> Result<ActionResult, String> {
    let root = state.data_dir.join("backups");
    let dir = confined_backup_path(&root, &backup_path)?;
    if !dir.is_dir() {
        return Err(
            "This legacy backup may contain only base.apk and cannot be restored safely. Create a new complete backup first."
                .to_string(),
        );
    }
    let manifest = read_manifest(&dir)?;
    let files = bundle_files(&dir, &manifest)?;
    let stamp = Utc::now().format("%Y%m%d%H%M%S%9f");
    let remote_dir = format!("/data/local/tmp/atv-optimizer-restore-{stamp}");
    let adb = state.adb_snapshot().await;
    let mkdir = adb
        .shell(&serial, &format!("mkdir -p {}", quote_path(&remote_dir)))
        .await
        .map_err(|e| format!("prepare restore: {e}"))?;
    if shell_failed(&mkdir) {
        return Err(format!(
            "Could not prepare restore: {}",
            mkdir.combined().trim()
        ));
    }

    let result = async {
        let mut remote_files = Vec::with_capacity(files.len());
        for file in &files {
            let name = file
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| "Backup contains an invalid APK filename.".to_string())?;
            let local = file.to_string_lossy().into_owned();
            let remote = format!("{remote_dir}/{name}");
            adb.raw_transfer(&["-s", &serial, "push", &local, &remote])
                .await
                .map_err(|e| format!("push {name}: {e}"))?;
            remote_files.push(remote);
        }

        let verb = if remote_files.len() > 1 {
            "install-multiple"
        } else {
            "install"
        };
        let paths = remote_files
            .iter()
            .map(|path| quote_path(path))
            .collect::<Vec<_>>()
            .join(" ");
        let out = adb
            .shell(&serial, &format!("pm {verb} -r {paths}"))
            .await
            .map_err(|e| format!("install backup: {e}"))?;
        let message = out.combined().trim().to_string();
        let ok = !shell_failed(&out) && message.to_ascii_lowercase().contains("success");
        Ok(ActionResult {
            ok,
            message: if ok {
                format!(
                    "Restored {} from {} APK file(s).",
                    manifest.package,
                    files.len()
                )
            } else if message.is_empty() {
                "The TV did not confirm that the backup was installed.".to_string()
            } else {
                message
            },
        })
    }
    .await;

    let _ = adb
        .shell(&serial, &format!("rm -rf {}", quote_path(&remote_dir)))
        .await;
    result
}

/// Resolve a delete target: path-confined to the backups root *and* a
/// top-level entry inside it, so a crafted path can never remove a nested
/// directory or a file the backups list never showed.
fn confined_delete_target(root: &Path, requested: &str) -> Result<PathBuf, String> {
    let canonical_root = root
        .canonicalize()
        .map_err(|e| format!("open backups directory: {e}"))?;
    let path = confined_backup_path(root, requested)?;
    if path.parent() != Some(canonical_root.as_path()) {
        return Err(
            "Backup path is not a top-level entry in this app's backup directory.".to_string(),
        );
    }
    if !path.is_dir() && path.extension().and_then(|e| e.to_str()) != Some("apk") {
        return Err("Not an APK backup.".to_string());
    }
    Ok(path)
}

/// `delete_backup` — remove one backup from the app's scoped storage: a
/// complete bundle directory (its APKs plus the manifest) or a legacy
/// base-only `.apk` file. The TV is not touched.
#[tauri::command]
pub async fn delete_backup(
    state: State<'_, AppState>,
    backup_path: String,
) -> Result<ActionResult, String> {
    let root = state.data_dir.join("backups");
    let path = confined_delete_target(&root, &backup_path)?;
    if path.is_dir() {
        std::fs::remove_dir_all(&path).map_err(|e| format!("delete backup bundle: {e}"))?;
    } else {
        std::fs::remove_file(&path).map_err(|e| format!("delete backup: {e}"))?;
    }
    Ok(ActionResult {
        ok: true,
        message: "Backup deleted from this phone.".to_string(),
    })
}

/// `list_backups` — complete bundle directories plus base-only legacy APKs,
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
        if path.is_dir() {
            if let Ok(bundle) = bundle_entry(&path) {
                out.push(bundle);
            }
            continue;
        }
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
            apk_count: 1,
            complete: false,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_split_apk_path() {
        let paths = parse_pm_paths(
            "package:/data/app/x/base.apk\npackage:/data/app/x/split_config.en.apk\nnoise\n",
        )
        .expect("valid paths");
        assert_eq!(paths.len(), 2);
        assert!(paths[1].ends_with("split_config.en.apk"));
    }

    #[test]
    fn rejects_unsafe_reported_apk_path() {
        assert!(parse_pm_paths("package:/data/app/x/../../escape.apk\n").is_err());
        assert!(parse_pm_paths("package:/data/app/x/not-an-apk\n").is_err());
    }

    #[test]
    fn restore_path_must_stay_inside_backup_root() {
        let root = tempfile::tempdir().expect("root");
        let inside = root.path().join("com.example-1");
        std::fs::create_dir(&inside).expect("inside");
        let outside = tempfile::tempdir().expect("outside");
        assert_eq!(
            confined_backup_path(root.path(), inside.to_str().expect("utf8")).expect("confined"),
            inside.canonicalize().expect("canonical")
        );
        assert!(
            confined_backup_path(root.path(), outside.path().to_str().expect("outside utf8"))
                .is_err()
        );
    }

    #[test]
    fn delete_target_must_be_a_top_level_backup() {
        let root = tempfile::tempdir().expect("root");
        let bundle = root.path().join("com.example-1");
        std::fs::create_dir(&bundle).expect("bundle");
        let nested = bundle.join("nested");
        std::fs::create_dir(&nested).expect("nested");
        let legacy = root.path().join("com.legacy.apk");
        std::fs::write(&legacy, b"apk").expect("legacy");
        let stray = root.path().join("notes.txt");
        std::fs::write(&stray, b"nope").expect("stray");
        let outside = tempfile::tempdir().expect("outside");

        assert_eq!(
            confined_delete_target(root.path(), bundle.to_str().expect("utf8")).expect("bundle"),
            bundle.canonicalize().expect("canonical")
        );
        assert_eq!(
            confined_delete_target(root.path(), legacy.to_str().expect("utf8")).expect("legacy"),
            legacy.canonicalize().expect("canonical")
        );
        // Nested dir, non-APK file, the root itself and anything outside it.
        assert!(confined_delete_target(root.path(), nested.to_str().expect("utf8")).is_err());
        assert!(confined_delete_target(root.path(), stray.to_str().expect("utf8")).is_err());
        assert!(confined_delete_target(root.path(), root.path().to_str().expect("utf8")).is_err());
        assert!(
            confined_delete_target(root.path(), outside.path().to_str().expect("utf8")).is_err()
        );
    }

    #[test]
    fn download_names_dedupe_instead_of_clobbering() {
        let dir = tempfile::tempdir().expect("dir");
        let root = dir.path();

        // Nothing there yet: the plain name.
        assert_eq!(
            unique_download_path(root, "report.txt").expect("first"),
            root.join("report.txt")
        );

        // Each existing copy pushes the counter, inserted before the extension.
        std::fs::write(root.join("report.txt"), b"one").expect("first file");
        assert_eq!(
            unique_download_path(root, "report.txt").expect("second"),
            root.join("report (2).txt")
        );
        std::fs::write(root.join("report (2).txt"), b"two").expect("second file");
        assert_eq!(
            unique_download_path(root, "report.txt").expect("third"),
            root.join("report (3).txt")
        );

        // A multi-dot name keeps every dot but the last in the stem.
        std::fs::write(root.join("clip.tar.gz"), b"gz").expect("gz");
        assert_eq!(
            unique_download_path(root, "clip.tar.gz").expect("gz dedupe"),
            root.join("clip.tar (2).gz")
        );

        // No extension, and a dotfile whose leading dot is part of the name.
        std::fs::write(root.join("logcat"), b"log").expect("logcat");
        assert_eq!(
            unique_download_path(root, "logcat").expect("no ext"),
            root.join("logcat (2)")
        );
        std::fs::write(root.join(".bashrc"), b"rc").expect("dotfile");
        assert_eq!(
            unique_download_path(root, ".bashrc").expect("dotfile dedupe"),
            root.join(".bashrc (2)")
        );

        // A directory in the way counts as taken, too.
        std::fs::create_dir(root.join("shots")).expect("dir in the way");
        assert_eq!(
            unique_download_path(root, "shots").expect("dir dedupe"),
            root.join("shots (2)")
        );
    }

    #[test]
    fn complete_bundle_round_trips() {
        let root = tempfile::tempdir().expect("root");
        let dir = root.path().join("com.example-1");
        std::fs::create_dir(&dir).expect("bundle dir");
        std::fs::write(dir.join("000-base.apk"), b"base").expect("base");
        std::fs::write(dir.join("001-split_config.en.apk"), b"split").expect("split");
        let manifest = BackupManifest {
            schema_version: BACKUP_SCHEMA_VERSION,
            package: "com.example".to_string(),
            saved_at: "2026-07-13T23:30:00Z".to_string(),
            apk_files: vec![
                "000-base.apk".to_string(),
                "001-split_config.en.apk".to_string(),
            ],
        };
        std::fs::write(
            dir.join(BACKUP_MANIFEST),
            serde_json::to_vec(&manifest).expect("json"),
        )
        .expect("manifest");

        let entry = bundle_entry(&dir).expect("entry");
        assert!(entry.complete);
        assert_eq!(entry.apk_count, 2);
        assert_eq!(entry.size_bytes, 9);
    }
}

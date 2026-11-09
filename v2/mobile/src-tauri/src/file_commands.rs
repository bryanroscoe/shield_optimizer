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
//! - **find** and **delete** are the shared core implementations
//!   (`commands::files`), `/sdcard`-confined exactly as on desktop. Mobile has
//!   no power-user mode, so the system-path override is never offered.
//!
//! All device I/O rides the shared `AdbDriver` seam (shell + `raw_transfer`),
//! keeping `crates/core/engine` pure. Transfers are implemented by
//! `WirelessAdb::pull_limited` and `WirelessAdb::raw_transfer` via
//! `adb_client`'s sync service; restores use `raw_transfer`'s
//! `install`/`install-multiple`, which stream to the package manager.

use std::path::{Path, PathBuf};

use atv_optimizer_core::adb::{parse_ls_output, AdbError, FileEntry};
use atv_optimizer_core::commands::files::{self, delete_path_with, find_files_with, FindResult};
use atv_optimizer_core::commands::{apps::ActionResult, AppState};
use atv_optimizer_core::engine::install_outcome::{
    apk_basename, decode_install_error, install_succeeded, is_safe_apk_name, parse_pm_paths,
};
use atv_optimizer_core::engine::is_valid_package_name;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
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

/// Browsing and pull accept any absolute device path: the same injection
/// guards as the desktop file manager (absolute, no `..`, no control
/// characters) with power-user scope, so the UI can browse
/// `/storage/emulated/0` as well as `/sdcard`. Delete stays `/sdcard`-confined
/// through the shared `delete_path_with`.
fn validate_device_path(path: &str) -> Result<String, String> {
    files::validate_device_path(path, true)
}

/// Single-quote a validated device path for the device-side shell.
fn quote_path(p: &str) -> String {
    format!("'{}'", p.replace('\'', r"'\''"))
}

/// The shared package-name rule (`name(.name)+`, each segment starting with
/// a letter) — it can never break out of the `pm path` shell word.
fn validate_package(pkg: &str) -> Result<String, String> {
    let p = pkg.trim();
    if !is_valid_package_name(p) {
        return Err(format!("Invalid package name: {pkg:?}"));
    }
    Ok(p.to_string())
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
    if manifest.apk_files.is_empty() || manifest.apk_files.iter().any(|f| !is_safe_apk_name(f)) {
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

fn shell_failed(out: &atv_optimizer_core::adb::AdbOutput) -> bool {
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

/// Hard cap on one pulled file — 2 GiB. `pull_limited` refuses a larger file
/// before streaming and stops a growing one at the cap, and the size is checked
/// again before the download is kept, so it never fills the phone's app
/// storage.
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
    pull_file_impl(state.inner(), &serial, &remote_path).await
}

async fn pull_file_impl(
    state: &AppState,
    serial: &str,
    remote_path: &str,
) -> Result<PulledFile, String> {
    let remote = validate_device_path(remote_path)?;
    let remote_name = apk_basename(&remote)
        .ok_or_else(|| format!("Not a file path: {remote}"))?
        .to_string();
    let dir = state.data_dir.join("downloads");
    std::fs::create_dir_all(&dir).map_err(|e| format!("create downloads dir: {e}"))?;
    let temporary = tempfile::NamedTempFile::new_in(&dir)
        .map_err(|e| format!("stage download: {e}"))?
        .into_temp_path();

    let adb = state.adb_snapshot().await;
    adb.pull_limited(serial, &remote, &temporary, MAX_PULL_BYTES)
        .await
        .map_err(|e| format!("pull: {e}"))?;

    let size_bytes = std::fs::metadata(&temporary)
        .map_err(|e| format!("download metadata: {e}"))?
        .len();
    if size_bytes > MAX_PULL_BYTES {
        return Err(format!(
            "{remote_name} is {:.1} GiB — over the {:.0} GiB limit for a single download. \
             Nothing was kept on this phone.",
            size_bytes as f64 / BYTES_PER_GIB,
            MAX_PULL_BYTES as f64 / BYTES_PER_GIB,
        ));
    }
    let local = publish_download(temporary, &dir, &remote_name)?;
    let local_str = local.to_string_lossy().into_owned();
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

fn publish_download(
    mut temporary: tempfile::TempPath,
    dir: &Path,
    name: &str,
) -> Result<PathBuf, String> {
    for _ in 0..MAX_DUPLICATE_SUFFIX {
        let local = unique_download_path(dir, name)?;
        match temporary.persist_noclobber(&local) {
            Ok(()) => return Ok(local),
            Err(e) if e.error.kind() == std::io::ErrorKind::AlreadyExists => temporary = e.path,
            Err(e) => return Err(format!("publish download: {e}")),
        }
    }
    Err("Could not reserve a download name. Retry the download.".into())
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
            let remote_name = apk_basename(remote).expect("validated APK path has a basename");
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

/// `restore_apk_backup` — install every APK of a path-confined complete bundle
/// together, the way `adb install-multiple -r` does. The driver streams them
/// to the package manager (or, on TVs without `cmd`, stages them in
/// `/data/local/tmp` and removes them again), and judges success by its
/// `Success` reply.
#[tauri::command]
pub async fn restore_apk_backup(
    state: State<'_, AppState>,
    serial: String,
    backup_path: String,
) -> Result<ActionResult, String> {
    restore_apk_backup_impl(state.inner(), &serial, &backup_path).await
}

async fn restore_apk_backup_impl(
    state: &AppState,
    serial: &str,
    backup_path: &str,
) -> Result<ActionResult, String> {
    let root = state.data_dir.join("backups");
    let dir = confined_backup_path(&root, backup_path)?;
    if !dir.is_dir() {
        return Err(
            "This legacy backup may contain only base.apk and cannot be restored safely. Create a new complete backup first."
                .to_string(),
        );
    }
    let manifest = read_manifest(&dir)?;
    let files = bundle_files(&dir, &manifest)?;
    let locals: Vec<String> = files
        .iter()
        .map(|file| file.to_string_lossy().into_owned())
        .collect();
    let verb = if locals.len() > 1 {
        "install-multiple"
    } else {
        "install"
    };
    let mut args = vec!["-s", serial, verb, "-r"];
    args.extend(locals.iter().map(String::as_str));

    let adb = state.adb_snapshot().await;
    let (ok, message) = match adb.raw_transfer(&args).await {
        Ok(out) => {
            let combined = out.combined();
            (
                install_succeeded(out.success(), &combined),
                combined.trim().to_string(),
            )
        }
        // `adb install` exits nonzero with the package manager's reason.
        Err(AdbError::NonZeroExit { stderr, .. }) => (false, stderr.trim().to_string()),
        Err(e) => return Err(format!("install backup: {e}")),
    };
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
        } else if let Some(hint) = decode_install_error(&message) {
            format!("{message}\n{hint}")
        } else {
            message
        },
    })
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

/// `find_files` — the app-files catalog search (SmartTube backups, Projectivy
/// exports, …). Directories whose search could not run come back in
/// `unsearched` so the UI never reports them as "no matches" (GitHub #86).
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

/// `delete_path` — delete a file or folder on the TV. Always `/sdcard`-confined
/// with `/sdcard` itself refused; the screen confirms before calling.
#[tauri::command]
pub async fn delete_path(
    state: State<'_, AppState>,
    serial: String,
    path: String,
) -> Result<ActionResult, String> {
    let adb = state.adb_snapshot().await;
    delete_path_with(adb.as_ref(), &serial, &path, false).await
}

/// What [`reclaim_partial_storage`] removed.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Reclaimed {
    pub partial_downloads: usize,
    pub incomplete_bundles: usize,
}

/// `tempfile`'s staging name for a pull that has not been published yet.
fn is_staged_download(name: &str) -> bool {
    name.starts_with(".tmp")
}

/// A bundle directory name as `backup_apk` writes it:
/// `<package>-<YYYYMMDD>T<HHMMSS><nanos>Z`.
fn is_bundle_dir_name(name: &str) -> bool {
    let Some((package, stamp)) = name.rsplit_once('-') else {
        return false;
    };
    let Some(digits) = stamp.strip_suffix('Z') else {
        return false;
    };
    let Some((date, time)) = digits.split_once('T') else {
        return false;
    };
    is_valid_package_name(package)
        && date.len() == 8
        && time.len() >= 6
        && date.chars().chain(time.chars()).all(|c| c.is_ascii_digit())
}

/// Free app-private space an interrupted pull or APK backup left behind:
/// staged `.tmp*` downloads, and bundle directories that never got their
/// manifest. Run once at startup, before any transfer can be in flight.
///
/// A directory that has a manifest file is kept whatever its state — a valid
/// bundle is the user's backup, and an unreadable one is theirs to delete.
/// Symlinks, other files and directories that don't carry the bundle naming
/// scheme are never touched.
pub fn reclaim_partial_storage(data_dir: &Path) -> Reclaimed {
    let mut reclaimed = Reclaimed::default();
    if let Ok(entries) = std::fs::read_dir(data_dir.join("downloads")) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            let Ok(meta) = std::fs::symlink_metadata(entry.path()) else {
                continue;
            };
            if meta.is_file() && is_staged_download(name) {
                match std::fs::remove_file(entry.path()) {
                    Ok(()) => reclaimed.partial_downloads += 1,
                    Err(e) => tracing::warn!(error = %e, "could not remove a partial download"),
                }
            }
        }
    }
    if let Ok(entries) = std::fs::read_dir(data_dir.join("backups")) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            let Ok(meta) = std::fs::symlink_metadata(&path) else {
                continue;
            };
            if !meta.is_dir() || !is_bundle_dir_name(name) {
                continue;
            }
            // `symlink_metadata` so a dangling or linked manifest still
            // counts as "a manifest is there" and the bundle is kept.
            if std::fs::symlink_metadata(path.join(BACKUP_MANIFEST)).is_ok() {
                continue;
            }
            match std::fs::remove_dir_all(&path) {
                Ok(()) => reclaimed.incomplete_bundles += 1,
                Err(e) => tracing::warn!(error = %e, "could not remove an incomplete APK backup"),
            }
        }
    }
    reclaimed
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

    struct DownloadDriver {
        fail: bool,
    }

    #[async_trait::async_trait]
    impl atv_optimizer_core::adb::AdbDriver for DownloadDriver {
        async fn raw(
            &self,
            _: &[&str],
        ) -> atv_optimizer_core::adb::AdbResult<atv_optimizer_core::adb::AdbOutput> {
            unreachable!()
        }
        async fn shell(
            &self,
            _: &str,
            _: &str,
        ) -> atv_optimizer_core::adb::AdbResult<atv_optimizer_core::adb::AdbOutput> {
            unreachable!()
        }
        async fn pull_limited(
            &self,
            serial: &str,
            remote: &str,
            local: &Path,
            limit: u64,
        ) -> atv_optimizer_core::adb::AdbResult<()> {
            assert_eq!(serial, "TV");
            assert_eq!(remote, "/sdcard/report.txt");
            assert_eq!(limit, MAX_PULL_BYTES);
            std::fs::write(local, b"data")?;
            if self.fail {
                return Err(std::io::Error::other("disk full").into());
            }
            Ok(())
        }
    }

    #[tokio::test]
    async fn failed_download_removes_partial_output_and_preserves_existing_files() {
        let dir = tempfile::tempdir().unwrap();
        let downloads = dir.path().join("downloads");
        std::fs::create_dir(&downloads).unwrap();
        std::fs::write(downloads.join("report.txt"), b"original").unwrap();
        let state = AppState::new(
            std::sync::Arc::new(DownloadDriver { fail: true }),
            Default::default(),
            dir.path().to_path_buf(),
        );
        assert!(pull_file_impl(&state, "TV", "/sdcard/report.txt")
            .await
            .is_err());
        assert_eq!(std::fs::read_dir(&downloads).unwrap().count(), 1);
        assert_eq!(
            std::fs::read(downloads.join("report.txt")).unwrap(),
            b"original"
        );
    }

    #[tokio::test]
    async fn concurrent_downloads_publish_distinct_complete_files() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::new(
            std::sync::Arc::new(DownloadDriver { fail: false }),
            Default::default(),
            dir.path().to_path_buf(),
        );
        let (a, b) = tokio::join!(
            pull_file_impl(&state, "TV", "/sdcard/report.txt"),
            pull_file_impl(&state, "TV", "/sdcard/report.txt")
        );
        let (a, b) = (a.unwrap(), b.unwrap());
        assert_ne!(a.path, b.path);
        assert_eq!(std::fs::read(a.path).unwrap(), b"data");
        assert_eq!(std::fs::read(b.path).unwrap(), b"data");
        assert_eq!(
            std::fs::read_dir(dir.path().join("downloads"))
                .unwrap()
                .count(),
            2
        );
    }

    /// Records `raw_transfer` calls and answers with `reply`. Any `shell`
    /// call fails the test: a restore no longer stages files or runs `pm`.
    struct InstallDriver {
        reply: fn() -> atv_optimizer_core::adb::AdbResult<atv_optimizer_core::adb::AdbOutput>,
        calls: std::sync::Mutex<Vec<Vec<String>>>,
    }

    #[async_trait::async_trait]
    impl atv_optimizer_core::adb::AdbDriver for InstallDriver {
        async fn raw(
            &self,
            _: &[&str],
        ) -> atv_optimizer_core::adb::AdbResult<atv_optimizer_core::adb::AdbOutput> {
            unreachable!()
        }
        async fn raw_transfer(
            &self,
            args: &[&str],
        ) -> atv_optimizer_core::adb::AdbResult<atv_optimizer_core::adb::AdbOutput> {
            self.calls
                .lock()
                .unwrap()
                .push(args.iter().map(|arg| arg.to_string()).collect());
            (self.reply)()
        }
        async fn shell(
            &self,
            _: &str,
            command: &str,
        ) -> atv_optimizer_core::adb::AdbResult<atv_optimizer_core::adb::AdbOutput> {
            panic!("restore ran a shell command: {command}")
        }
    }

    fn write_bundle(data_dir: &Path, files: &[&str]) -> PathBuf {
        let dir = data_dir.join("backups").join("com.example-1");
        std::fs::create_dir_all(&dir).expect("bundle dir");
        for file in files {
            std::fs::write(dir.join(file), b"apk").expect("apk");
        }
        let manifest = BackupManifest {
            schema_version: BACKUP_SCHEMA_VERSION,
            package: "com.example".to_string(),
            saved_at: "2026-11-04T00:00:00Z".to_string(),
            apk_files: files.iter().map(|file| file.to_string()).collect(),
        };
        std::fs::write(
            dir.join(BACKUP_MANIFEST),
            serde_json::to_vec(&manifest).expect("json"),
        )
        .expect("manifest");
        dir
    }

    async fn restore_with(
        files: &[&str],
        reply: fn() -> atv_optimizer_core::adb::AdbResult<atv_optimizer_core::adb::AdbOutput>,
    ) -> (Result<ActionResult, String>, Vec<Vec<String>>, PathBuf) {
        let data = tempfile::tempdir().expect("data dir");
        let bundle = write_bundle(data.path(), files);
        let driver = std::sync::Arc::new(InstallDriver {
            reply,
            calls: Default::default(),
        });
        let state = AppState::new(
            driver.clone(),
            Default::default(),
            data.path().to_path_buf(),
        );
        let result = restore_apk_backup_impl(&state, "TV", bundle.to_str().expect("utf8")).await;
        let calls = driver.calls.lock().unwrap().clone();
        let canonical = bundle.canonicalize().expect("canonical");
        (result, calls, canonical)
    }

    fn success() -> atv_optimizer_core::adb::AdbResult<atv_optimizer_core::adb::AdbOutput> {
        Ok(atv_optimizer_core::adb::AdbOutput {
            stdout: "Success\n".to_string(),
            stderr: String::new(),
            exit_code: Some(0),
        })
    }

    #[tokio::test]
    async fn restore_installs_every_split_together_without_staging_on_the_tv() {
        let (result, calls, bundle) =
            restore_with(&["000-base.apk", "001-split_config.en.apk"], success).await;
        let result = result.expect("restore");
        assert!(result.ok, "{}", result.message);
        assert_eq!(result.message, "Restored com.example from 2 APK file(s).");
        let expected: Vec<String> = ["-s", "TV", "install-multiple", "-r"]
            .iter()
            .map(|arg| arg.to_string())
            .chain(
                ["000-base.apk", "001-split_config.en.apk"]
                    .iter()
                    .map(|file| bundle.join(file).to_string_lossy().into_owned()),
            )
            .collect();
        assert_eq!(calls, [expected]);
    }

    #[tokio::test]
    async fn a_single_apk_restore_uses_install() {
        let (result, calls, _) = restore_with(&["000-base.apk"], success).await;
        assert!(result.expect("restore").ok);
        assert_eq!(calls[0][2], "install");
    }

    #[tokio::test]
    async fn a_package_manager_refusal_is_reported_not_raised() {
        let (result, _, _) = restore_with(&["000-base.apk"], || {
            Err(AdbError::NonZeroExit {
                code: Some(1),
                stderr: "Failure [INSTALL_FAILED_VERSION_DOWNGRADE]".to_string(),
            })
        })
        .await;
        let result = result.expect("an answer, not an error");
        assert!(!result.ok);
        assert!(result
            .message
            .starts_with("Failure [INSTALL_FAILED_VERSION_DOWNGRADE]"));
        assert!(result.message.contains("newer"), "{}", result.message);
    }

    #[tokio::test]
    async fn a_transport_failure_is_an_error_and_exit_zero_without_success_is_not_ok() {
        let (result, _, _) = restore_with(&["000-base.apk"], || {
            Err(AdbError::Transport(
                "Connection to the TV was lost: timed out".to_string(),
            ))
        })
        .await;
        match result {
            Err(message) => assert!(message.contains("Connection to the TV was lost")),
            Ok(_) => panic!("a lost connection must be an error"),
        }

        let (result, _, _) = restore_with(&["000-base.apk"], || {
            Ok(atv_optimizer_core::adb::AdbOutput {
                stdout: String::new(),
                stderr: String::new(),
                exit_code: Some(0),
            })
        })
        .await;
        let result = result.expect("answer");
        assert!(!result.ok);
        assert_eq!(
            result.message,
            "The TV did not confirm that the backup was installed."
        );
    }

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

    #[test]
    fn bundle_names_match_what_backup_apk_writes() {
        let stamp = Utc::now().format("%Y%m%dT%H%M%S%9fZ").to_string();
        assert!(is_bundle_dir_name(&format!("com.example.app-{stamp}")));
        assert!(is_bundle_dir_name("com.example-20261108T120000123456789Z"));
        assert!(!is_bundle_dir_name("com.example-1"));
        assert!(!is_bundle_dir_name("notes"));
        assert!(!is_bundle_dir_name("bad;pkg-20261108T120000Z"));
    }

    #[test]
    fn reclaim_removes_partials_and_manifestless_bundles_only() {
        let data = tempfile::tempdir().expect("data");
        let downloads = data.path().join("downloads");
        let backups = data.path().join("backups");
        std::fs::create_dir_all(&downloads).unwrap();
        std::fs::create_dir_all(&backups).unwrap();

        std::fs::write(downloads.join(".tmpAbC123"), b"partial").unwrap();
        std::fs::write(downloads.join("report.txt"), b"kept").unwrap();
        std::fs::write(downloads.join(".bashrc"), b"kept").unwrap();

        // Interrupted: APKs pulled, manifest never written.
        let partial = backups.join("com.example-20261108T120000000000001Z");
        std::fs::create_dir(&partial).unwrap();
        std::fs::write(partial.join("000-base.apk"), b"apk").unwrap();
        // Complete bundle with a valid manifest.
        let complete = backups.join("com.example-20261108T120000000000002Z");
        std::fs::create_dir(&complete).unwrap();
        std::fs::write(complete.join("000-base.apk"), b"apk").unwrap();
        let manifest = BackupManifest {
            schema_version: BACKUP_SCHEMA_VERSION,
            package: "com.example".to_string(),
            saved_at: "2026-11-08T12:00:00Z".to_string(),
            apk_files: vec!["000-base.apk".to_string()],
        };
        std::fs::write(
            complete.join(BACKUP_MANIFEST),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        // An unreadable manifest is still the user's to delete.
        let corrupt = backups.join("com.example-20261108T120000000000003Z");
        std::fs::create_dir(&corrupt).unwrap();
        std::fs::write(corrupt.join(BACKUP_MANIFEST), b"{not json").unwrap();
        // Not our naming scheme, and a legacy base-only APK file.
        let foreign = backups.join("keep-me");
        std::fs::create_dir(&foreign).unwrap();
        std::fs::write(backups.join("com.legacy.apk"), b"apk").unwrap();

        let reclaimed = reclaim_partial_storage(data.path());
        assert_eq!(
            reclaimed,
            Reclaimed {
                partial_downloads: 1,
                incomplete_bundles: 1
            }
        );
        assert!(!downloads.join(".tmpAbC123").exists());
        assert!(downloads.join("report.txt").exists());
        assert!(downloads.join(".bashrc").exists());
        assert!(!partial.exists());
        assert!(complete.join("000-base.apk").exists());
        assert!(bundle_entry(&complete).is_ok());
        assert!(corrupt.join(BACKUP_MANIFEST).exists());
        assert!(foreign.exists());
        assert!(backups.join("com.legacy.apk").exists());

        // Idempotent, and absent folders are not an error.
        assert_eq!(reclaim_partial_storage(data.path()), Reclaimed::default());
        let empty = tempfile::tempdir().unwrap();
        assert_eq!(reclaim_partial_storage(empty.path()), Reclaimed::default());
    }

    #[test]
    fn package_names_use_the_shared_rule() {
        assert!(validate_package("com.example.app").is_ok());
        assert!(validate_package("com.example;id").is_err());
        assert!(validate_package("nodots").is_err());
        assert!(validate_package("").is_err());
    }

    #[test]
    fn browse_paths_keep_power_user_scope_and_the_injection_guards() {
        assert!(validate_device_path("/storage/emulated/0/Download").is_ok());
        assert!(validate_device_path("/sdcard").is_ok());
        assert!(validate_device_path("relative").is_err());
        assert!(validate_device_path("/sdcard/../data").is_err());
        assert!(validate_device_path("/sdcard/a\nb").is_err());
    }
}

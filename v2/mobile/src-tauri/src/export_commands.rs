//! Save a copy / Share for one file the user picked (`EXPORT-DISCOVERABILITY.md`, phase 1).
//!
//! Only two kinds of file can leave the app: a TV screenshot (`<data>/screenshots/*.png`) and a
//! file copied off the TV (`<data>/downloads/*`). Every request names one file, which is
//! re-validated here against those two folders, so the frontend can't point the bridge at the
//! ADB key, the license, snapshots or anything else in the private data root.
//!
//! - **Save a copy** opens Android's create-document picker (Storage Access Framework) through the
//!   atv-adb Kotlin plugin, which streams the file to the chosen document. No storage permission.
//! - **Share** copies the file into `<cache>/exports/<unique>/`, the only folder the app's
//!   FileProvider maps, and opens the share sheet with a temporary read grant. Staged copies are
//!   removed after [`STAGE_TTL`].
//!
//! Screenshots leave under a name without the TV's address (`tv-screenshot-<time>.png`).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

use atv_optimizer_core::commands::AppState;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_atv_adb::AdbExt;

/// The folder under the app cache dir that `res/xml/file_paths.xml` exposes. Keep in step.
pub const EXPORTS_DIR: &str = "exports";
/// How long a staged share copy is kept for the receiving app to read it.
pub const STAGE_TTL: Duration = Duration::from_secs(60 * 60);
/// Share stages a copy; anything larger goes through Save a copy, which streams.
pub const MAX_SHARE_BYTES: u64 = 512 * 1024 * 1024;
/// The picker can stay open while the user browses; past this the request is given up.
const SAVE_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const SHARE_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExportMode {
    Save,
    Share,
}

/// What the frontend reports. `ShareOpened` means only that the share sheet opened: Android
/// doesn't say whether the receiving app finished.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ExportOutcome {
    Saved { name: Option<String> },
    Canceled,
    ShareOpened,
}

/// One validated source file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exportable {
    pub source: PathBuf,
    pub suggested_name: String,
    pub mime: &'static str,
    pub size_bytes: u64,
}

/// One save or share at a time; the system UI covers the app anyway.
#[derive(Default)]
pub struct ExportGate(AtomicBool);

pub struct ExportBusy<'a>(&'a AtomicBool);

impl ExportGate {
    pub fn try_begin(&self) -> Option<ExportBusy<'_>> {
        self.0
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()
            .map(|_| ExportBusy(&self.0))
    }
}

impl Drop for ExportBusy<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

const NOT_EXPORTABLE: &str =
    "Only TV screenshots and files copied from the TV can be saved or shared.";

/// Confine `requested` to a regular file directly inside `<data>/screenshots` (PNG only) or
/// `<data>/downloads`. Symlinks are resolved first, so a link can't point the export elsewhere.
pub fn resolve_exportable(data_dir: &Path, requested: &Path) -> Result<Exportable, String> {
    let source = std::fs::canonicalize(requested)
        .map_err(|_| "That file is no longer in the app.".to_string())?;
    let meta = std::fs::metadata(&source).map_err(|e| format!("read file: {e}"))?;
    if !meta.is_file() {
        return Err(NOT_EXPORTABLE.into());
    }
    let parent = source.parent().ok_or(NOT_EXPORTABLE)?;
    let name = source
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or(NOT_EXPORTABLE)?
        .to_string();
    let root = |dir: &str| std::fs::canonicalize(data_dir.join(dir)).ok();
    let (suggested_name, mime) = if root("screenshots").as_deref() == Some(parent) {
        if !name.to_ascii_lowercase().ends_with(".png") {
            return Err(NOT_EXPORTABLE.into());
        }
        (screenshot_export_name(&name), "image/png")
    } else if root("downloads").as_deref() == Some(parent) {
        let mime = mime_for(&name);
        (name, mime)
    } else {
        return Err(NOT_EXPORTABLE.into());
    };
    Ok(Exportable {
        source,
        suggested_name,
        mime,
        size_bytes: meta.len(),
    })
}

/// `<serial>_<YYYY-MM-DD_HH-MM-SS>[ (n)].png` → `tv-screenshot-<YYYY-MM-DD_HH-MM-SS>.png`, so the
/// TV's address doesn't travel with the image. A name that doesn't carry the stamp gets none.
pub fn screenshot_export_name(file_name: &str) -> String {
    let stem = file_name
        .get(..file_name.len().saturating_sub(4))
        .unwrap_or(file_name);
    let stem = match stem.rfind(" (") {
        Some(i) if stem.ends_with(')') => &stem[..i],
        _ => stem,
    };
    let stamp = stem
        .rsplit_once('_')
        .and_then(|(rest, time)| rest.rsplit_once('_').map(|(_, date)| (date, time)));
    match stamp {
        Some((date, time)) if is_stamp(date, "dddd-dd-dd") && is_stamp(time, "dd-dd-dd") => {
            format!("tv-screenshot-{date}_{time}.png")
        }
        _ => "tv-screenshot.png".to_string(),
    }
}

fn is_stamp(value: &str, pattern: &str) -> bool {
    value.len() == pattern.len()
        && value.bytes().zip(pattern.bytes()).all(|(v, p)| match p {
            b'd' => v.is_ascii_digit(),
            other => v == other,
        })
}

/// A MIME type the receiving app or picker can act on; unknown extensions are generic bytes.
pub fn mime_for(name: &str) -> &'static str {
    let ext = name
        .rsplit_once('.')
        .filter(|(stem, _)| !stem.is_empty())
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "mp4" => "video/mp4",
        "mkv" => "video/x-matroska",
        "webm" => "video/webm",
        "mp3" => "audio/mpeg",
        "txt" | "log" => "text/plain",
        "json" => "application/json",
        "xml" => "text/xml",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        "apk" => "application/vnd.android.package-archive",
        _ => "application/octet-stream",
    }
}

/// Remove staged share copies older than `ttl`. Only directories this module created
/// (`share-*`) are touched, and symlinks are never followed.
pub fn prune_staged(exports: &Path, ttl: Duration, now: SystemTime) -> usize {
    let Ok(entries) = std::fs::read_dir(exports) else {
        return 0;
    };
    let mut removed = 0;
    for entry in entries.flatten() {
        let is_ours = entry
            .file_name()
            .to_str()
            .is_some_and(|n| n.starts_with("share-"));
        let Ok(meta) = std::fs::symlink_metadata(entry.path()) else {
            continue;
        };
        if !is_ours || !meta.is_dir() {
            continue;
        }
        let age = meta
            .modified()
            .ok()
            .and_then(|m| now.duration_since(m).ok())
            .unwrap_or_default();
        if age >= ttl && std::fs::remove_dir_all(entry.path()).is_ok() {
            removed += 1;
        }
    }
    removed
}

/// Copy `item` into a fresh `<exports>/share-<n>/<suggested name>` for the share sheet.
pub fn stage_for_share(
    exports: &Path,
    item: &Exportable,
    now: SystemTime,
) -> Result<PathBuf, String> {
    if item.size_bytes > MAX_SHARE_BYTES {
        return Err("This file is too large to share from here. Use Save a copy instead.".into());
    }
    std::fs::create_dir_all(exports).map_err(|e| format!("prepare share: {e}"))?;
    prune_staged(exports, STAGE_TTL, now);
    let stamp = now
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let dir = (0..100u32)
        .map(|n| exports.join(format!("share-{stamp}-{n}")))
        .find(|dir| std::fs::create_dir(dir).is_ok())
        .ok_or("Couldn't prepare the file for sharing. Try again.")?;
    let staged = dir.join(&item.suggested_name);
    if let Err(e) = std::fs::copy(&item.source, &staged) {
        let _ = std::fs::remove_dir_all(&dir);
        return Err(format!("prepare share: {e}"));
    }
    Ok(staged)
}

/// Startup cleanup of share copies left from an earlier run.
pub fn prune_at_startup(cache_dir: &Path) {
    let removed = prune_staged(&cache_dir.join(EXPORTS_DIR), STAGE_TTL, SystemTime::now());
    if removed > 0 {
        tracing::info!(removed, "removed expired share copies");
    }
}

async fn blocking<T: Send + 'static>(
    timeout: Duration,
    late: &str,
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    match tokio::time::timeout(timeout, tokio::task::spawn_blocking(f)).await {
        Ok(Ok(result)) => result,
        Ok(Err(join)) => Err(join.to_string()),
        Err(_) => Err(late.to_string()),
    }
}

/// `export_file` — Save a copy or Share one screenshot or copied file. Nothing leaves the
/// phone unless the user completes Android's picker or share sheet.
#[tauri::command]
pub async fn export_file(
    app: AppHandle,
    state: State<'_, AppState>,
    gate: State<'_, ExportGate>,
    path: String,
    mode: ExportMode,
) -> Result<ExportOutcome, String> {
    let item = resolve_exportable(&state.data_dir, Path::new(&path))?;
    let _busy = gate
        .try_begin()
        .ok_or("Another save or share is still open.")?;
    match mode {
        ExportMode::Save => {
            let source = item.source.to_string_lossy().into_owned();
            let (name, mime) = (item.suggested_name.clone(), item.mime);
            let handle = app.clone();
            let saved = blocking(
                SAVE_TIMEOUT,
                "The save didn't finish in time. Check the folder you picked before trying again.",
                move || {
                    handle
                        .adb()
                        .save_copy(&source, &name, mime)
                        .map_err(|e| e.to_string())
                },
            )
            .await?;
            Ok(if saved.saved {
                ExportOutcome::Saved { name: saved.name }
            } else {
                ExportOutcome::Canceled
            })
        }
        ExportMode::Share => {
            let exports = app
                .path()
                .app_cache_dir()
                .map_err(|e| format!("prepare share: {e}"))?
                .join(EXPORTS_DIR);
            let mime = item.mime;
            // No timeout on the local copy (at most MAX_SHARE_BYTES), so the gate stays held
            // until it ends and never overlaps another export.
            let staged = tokio::task::spawn_blocking(move || {
                stage_for_share(&exports, &item, SystemTime::now())
            })
            .await
            .map_err(|e| e.to_string())??;
            let handle = app.clone();
            blocking(SHARE_TIMEOUT, "The share sheet didn't open.", move || {
                handle
                    .adb()
                    .share_file(&staged.to_string_lossy(), mime)
                    .map_err(|e| e.to_string())
            })
            .await?;
            Ok(ExportOutcome::ShareOpened)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data_dir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for sub in ["screenshots", "downloads", "snapshots", "backups/pkg-1"] {
            std::fs::create_dir_all(dir.path().join(sub)).unwrap();
        }
        dir
    }

    #[test]
    fn screenshots_and_downloads_resolve_with_a_type_and_a_safe_name() {
        let dir = data_dir();
        let shot = dir
            .path()
            .join("screenshots/192-168-1-20-5555_2026-11-08_12-30-05.png");
        std::fs::write(&shot, b"\x89PNG").unwrap();
        let item = resolve_exportable(dir.path(), &shot).unwrap();
        assert_eq!(item.mime, "image/png");
        assert_eq!(item.suggested_name, "tv-screenshot-2026-11-08_12-30-05.png");
        assert_eq!(item.size_bytes, 4);

        let file = dir.path().join("downloads/report (2).txt");
        std::fs::write(&file, b"hello").unwrap();
        let item = resolve_exportable(dir.path(), &file).unwrap();
        assert_eq!(item.mime, "text/plain");
        assert_eq!(item.suggested_name, "report (2).txt");
    }

    #[test]
    fn everything_else_in_the_data_root_is_refused() {
        let dir = data_dir();
        for (path, body) in [
            ("adb_key", "secret"),
            ("license.json", "{}"),
            ("debug.log", "log"),
            ("snapshots/before.json", "{}"),
            ("backups/pkg-1/base.apk", "apk"),
            ("screenshots/not-an-image.txt", "x"),
        ] {
            let full = dir.path().join(path);
            std::fs::write(&full, body).unwrap();
            assert_eq!(
                resolve_exportable(dir.path(), &full),
                Err(NOT_EXPORTABLE.to_string()),
                "{path}"
            );
        }
        // A folder, a traversal out of downloads and a missing file.
        std::fs::create_dir(dir.path().join("downloads/folder")).unwrap();
        assert!(resolve_exportable(dir.path(), &dir.path().join("downloads/folder")).is_err());
        assert!(resolve_exportable(dir.path(), &dir.path().join("downloads/../adb_key")).is_err());
        assert!(resolve_exportable(dir.path(), &dir.path().join("downloads/gone.txt")).is_err());
        // Nested below downloads is not a copied file either.
        std::fs::write(dir.path().join("downloads/folder/inner.txt"), "x").unwrap();
        assert!(
            resolve_exportable(dir.path(), &dir.path().join("downloads/folder/inner.txt")).is_err()
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_in_downloads_cannot_reach_the_adb_key() {
        let dir = data_dir();
        std::fs::write(dir.path().join("adb_key"), "secret").unwrap();
        let link = dir.path().join("downloads/key.txt");
        std::os::unix::fs::symlink(dir.path().join("adb_key"), &link).unwrap();
        assert_eq!(
            resolve_exportable(dir.path(), &link),
            Err(NOT_EXPORTABLE.to_string())
        );
    }

    #[test]
    fn screenshot_names_drop_the_tv_address() {
        for (name, expected) in [
            (
                "192-168-1-20-5555_2026-11-08_12-30-05.png",
                "tv-screenshot-2026-11-08_12-30-05.png",
            ),
            (
                "adb-SERIAL-x_2026-01-02_03-04-05 (3).png",
                "tv-screenshot-2026-01-02_03-04-05.png",
            ),
            ("odd.png", "tv-screenshot.png"),
            ("serial_2026-1-2_3-4-5.png", "tv-screenshot.png"),
            (".png", "tv-screenshot.png"),
        ] {
            assert_eq!(screenshot_export_name(name), expected, "{name}");
        }
    }

    #[test]
    fn mime_types_follow_the_extension() {
        assert_eq!(mime_for("clip.MP4"), "video/mp4");
        assert_eq!(
            mime_for("app.apk"),
            "application/vnd.android.package-archive"
        );
        assert_eq!(mime_for(".bashrc"), "application/octet-stream");
        assert_eq!(mime_for("noext"), "application/octet-stream");
        assert_eq!(mime_for("data.bin"), "application/octet-stream");
    }

    #[test]
    fn share_stages_a_copy_and_keeps_the_original() {
        let dir = data_dir();
        let shot = dir.path().join("screenshots/tv_2026-11-08_12-30-05.png");
        std::fs::write(&shot, b"\x89PNG-bytes").unwrap();
        let item = resolve_exportable(dir.path(), &shot).unwrap();
        let exports = dir.path().join("cache").join(EXPORTS_DIR);
        let now = SystemTime::now();
        let first = stage_for_share(&exports, &item, now).unwrap();
        let second = stage_for_share(&exports, &item, now).unwrap();
        assert_ne!(first, second);
        for staged in [&first, &second] {
            assert!(staged.starts_with(&exports));
            assert_eq!(
                staged.file_name().unwrap(),
                "tv-screenshot-2026-11-08_12-30-05.png"
            );
            assert_eq!(std::fs::read(staged).unwrap(), b"\x89PNG-bytes");
        }
        assert_eq!(std::fs::read(&shot).unwrap(), b"\x89PNG-bytes");
    }

    #[test]
    fn oversized_shares_are_refused_before_copying() {
        let dir = data_dir();
        let item = Exportable {
            source: dir.path().join("downloads/huge.mkv"),
            suggested_name: "huge.mkv".into(),
            mime: "video/x-matroska",
            size_bytes: MAX_SHARE_BYTES + 1,
        };
        let exports = dir.path().join("cache/exports");
        assert!(stage_for_share(&exports, &item, SystemTime::now())
            .unwrap_err()
            .contains("Save a copy"));
        assert!(!exports.exists());
    }

    #[test]
    fn pruning_removes_only_expired_share_folders() {
        let dir = tempfile::tempdir().unwrap();
        let exports = dir.path().join(EXPORTS_DIR);
        std::fs::create_dir_all(exports.join("share-1-0")).unwrap();
        std::fs::write(exports.join("share-1-0/a.png"), "x").unwrap();
        std::fs::create_dir_all(exports.join("other")).unwrap();
        std::fs::write(exports.join("share-file"), "x").unwrap();
        let now = SystemTime::now();
        assert_eq!(prune_staged(&exports, STAGE_TTL, now), 0);
        assert!(exports.join("share-1-0").exists());
        assert_eq!(prune_staged(&exports, STAGE_TTL, now + STAGE_TTL), 1);
        assert!(!exports.join("share-1-0").exists());
        assert!(exports.join("other").exists());
        assert!(exports.join("share-file").exists());
        assert_eq!(prune_staged(&dir.path().join("missing"), STAGE_TTL, now), 0);
    }

    #[test]
    fn one_export_at_a_time() {
        let gate = ExportGate::default();
        let busy = gate.try_begin().unwrap();
        assert!(gate.try_begin().is_none());
        drop(busy);
        assert!(gate.try_begin().is_some());
    }

    #[test]
    fn outcomes_serialize_for_the_frontend() {
        assert_eq!(
            serde_json::to_value(ExportOutcome::Saved {
                name: Some("a.png".into())
            })
            .unwrap(),
            serde_json::json!({ "status": "saved", "name": "a.png" })
        );
        assert_eq!(
            serde_json::to_value(ExportOutcome::Canceled).unwrap(),
            serde_json::json!({ "status": "canceled" })
        );
        assert_eq!(
            serde_json::to_value(ExportOutcome::ShareOpened).unwrap(),
            serde_json::json!({ "status": "share_opened" })
        );
        assert_eq!(
            serde_json::from_str::<ExportMode>(r#""share""#).unwrap(),
            ExportMode::Share
        );
    }
}

//! Screenshot capture — `adb exec-out screencap -p`, saved under the app
//! data dir with a base64 copy for an instant in-app preview.

use std::{
    io::Write,
    path::{Path, PathBuf},
};

use serde::Serialize;
use tauri::State;

use base64::Engine as _;

use super::AppState;

#[derive(Serialize)]
pub struct ScreenshotResult {
    /// Absolute path of the saved PNG.
    pub path: String,
    /// The same PNG, base64-encoded, for a `data:` URL preview in the UI.
    pub base64: String,
}

const PNG_MAGIC: &[u8] = &[0x89, b'P', b'N', b'G'];

/// `screencap -p` writes the PNG to stdout; on failure (secure surface, DRM,
/// permission) it writes an error message instead. The magic bytes are the
/// reliable success signal.
fn validate_png(bytes: &[u8]) -> Result<(), String> {
    if bytes.starts_with(PNG_MAGIC) {
        return Ok(());
    }
    let text = String::from_utf8_lossy(&bytes[..bytes.len().min(200)]);
    let text = text.trim();
    if text.is_empty() {
        Err(
            "screencap returned no data — the screen may be off or showing protected content."
                .into(),
        )
    } else {
        Err(format!("screencap failed: {text}"))
    }
}

/// Serials like `192.168.42.71:5555` contain `:`, which is invalid in
/// Windows filenames — flatten everything non-alphanumeric to `-`.
fn filename_safe(serial: &str) -> String {
    serial
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

const MAX_CAPTURE_NAMES: u32 = 1000;

fn publish_screenshot(
    dir: &Path,
    stem: &str,
    write: impl FnOnce(&mut std::fs::File) -> std::io::Result<()>,
) -> Result<PathBuf, String> {
    let mut temporary = tempfile::NamedTempFile::new_in(dir)
        .map_err(|e| format!("stage screenshot in {}: {e}", dir.display()))?;
    write(temporary.as_file_mut()).map_err(|e| format!("write screenshot: {e}"))?;
    temporary
        .as_file_mut()
        .flush()
        .map_err(|e| format!("flush screenshot: {e}"))?;
    // Publish complete bytes atomically; existence checks alone race other captures.
    for n in 1..=MAX_CAPTURE_NAMES {
        let name = if n == 1 {
            format!("{stem}.png")
        } else {
            format!("{stem} ({n}).png")
        };
        let path = dir.join(name);
        match temporary.persist_noclobber(&path) {
            Ok(_) => return Ok(path),
            Err(e) if e.error.kind() == std::io::ErrorKind::AlreadyExists => temporary = e.file,
            Err(e) => return Err(format!("save screenshot {}: {}", path.display(), e.error)),
        }
    }
    Err("Could not reserve a screenshot filename. Earlier captures were preserved; retry the capture.".into())
}

#[tauri::command]
pub async fn take_screenshot(
    state: State<'_, AppState>,
    serial: String,
) -> Result<ScreenshotResult, String> {
    let adb = state.adb_snapshot().await;
    let bytes = adb
        .raw_bytes(&["-s", &serial, "exec-out", "screencap", "-p"])
        .await
        .map_err(|e| e.to_string())?;
    validate_png(&bytes)?;

    // Sibling of the snapshots dir in the app data root (e.g.
    // `…/ShieldOptimizer/screenshots`).
    let dir = state
        .snapshot_dir
        .parent()
        .map(|p| p.join("screenshots"))
        .unwrap_or_else(|| state.snapshot_dir.join("screenshots"));
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| format!("create {}: {e}", dir.display()))?;
    let stamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S");
    let stem = format!("{}_{stamp}", filename_safe(&serial));
    let saved_bytes = bytes.clone();
    let path = tokio::task::spawn_blocking(move || {
        publish_screenshot(&dir, &stem, |file| file.write_all(&saved_bytes))
    })
    .await
    .map_err(|e| format!("save screenshot task: {e}"))??;

    Ok(ScreenshotResult {
        path: path.display().to_string(),
        base64: base64::engine::general_purpose::STANDARD.encode(&bytes),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn accepts_png_bytes() {
        assert!(validate_png(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A]).is_ok());
    }

    #[test]
    fn rejects_error_text_with_message() {
        let err = validate_png(b"Unable to take screenshot: secure surface").unwrap_err();
        assert!(err.contains("secure surface"));
    }

    #[test]
    fn rejects_empty_output() {
        let err = validate_png(b"").unwrap_err();
        assert!(err.contains("no data"));
    }

    #[test]
    fn flattens_serial_for_filenames() {
        assert_eq!(filename_safe("192.168.42.71:5555"), "192-168-42-71-5555");
        assert_eq!(filename_safe("0123456789ABCDEF"), "0123456789ABCDEF");
    }
    fn save(dir: &Path, stem: &str, bytes: &[u8]) -> Result<PathBuf, String> {
        publish_screenshot(dir, stem, |file| file.write_all(bytes))
    }

    #[test]
    fn same_timestamp_retains_each_capture() {
        let dir = tempfile::tempdir().unwrap();
        let stem = "TV_2026-10-17_12-00-00";
        let first = save(dir.path(), stem, b"first PNG").unwrap();
        let second = save(dir.path(), stem, b"second PNG").unwrap();
        assert_eq!(first, dir.path().join(format!("{stem}.png")));
        assert_eq!(second, dir.path().join(format!("{stem} (2).png")));
        assert_eq!(std::fs::read(first).unwrap(), b"first PNG");
        assert_eq!(std::fs::read(second).unwrap(), b"second PNG");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
    }

    #[test]
    fn concurrent_same_timestamp_captures_never_replace_each_other() {
        let dir = tempfile::tempdir().unwrap();
        let existing = dir.path().join("TV_fixed-time.png");
        std::fs::write(&existing, b"old capture").unwrap();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(12));
        let handles: Vec<_> = (0..12)
            .map(|n| {
                let dir = dir.path().to_owned();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    let bytes = format!("PNG capture {n}");
                    barrier.wait();
                    let path = save(&dir, "TV_fixed-time", bytes.as_bytes()).unwrap();
                    (path, bytes)
                })
            })
            .collect();
        let mut paths = std::collections::HashSet::new();
        for handle in handles {
            let (path, bytes) = handle.join().unwrap();
            assert_eq!(std::fs::read(&path).unwrap(), bytes.as_bytes());
            assert!(paths.insert(path));
        }
        assert_eq!(std::fs::read(existing).unwrap(), b"old capture");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 13);
    }

    #[test]
    fn failed_partial_write_publishes_nothing_and_preserves_prior_capture() {
        let dir = tempfile::tempdir().unwrap();
        let existing = save(dir.path(), "TV", b"old capture").unwrap();
        let error = publish_screenshot(dir.path(), "TV", |file| {
            file.write_all(b"partial")?;
            Err(std::io::Error::other("injected write failure"))
        })
        .unwrap_err();
        assert!(error.contains("injected write failure"));
        assert_eq!(std::fs::read(existing).unwrap(), b"old capture");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn exhausted_names_fail_without_overwriting_or_leaving_staging_files() {
        let dir = tempfile::tempdir().unwrap();
        for n in 1..=MAX_CAPTURE_NAMES {
            let name = if n == 1 {
                "TV.png".into()
            } else {
                format!("TV ({n}).png")
            };
            std::fs::write(dir.path().join(name), b"retained").unwrap();
        }
        let error = save(dir.path(), "TV", b"new").unwrap_err();
        assert!(error.contains("Earlier captures were preserved"));
        let entries: Vec<_> = std::fs::read_dir(dir.path()).unwrap().collect();
        assert_eq!(entries.len(), MAX_CAPTURE_NAMES as usize);
        for entry in entries {
            assert_eq!(std::fs::read(entry.unwrap().path()).unwrap(), b"retained");
        }
    }

    #[test]
    fn unavailable_directory_returns_an_error_without_a_saved_path() {
        let dir = tempfile::tempdir().unwrap();
        assert!(save(&dir.path().join("missing"), "TV", b"new")
            .unwrap_err()
            .contains("stage screenshot"));
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[cfg(unix)]
    #[test]
    fn existing_symlink_target_is_not_followed_or_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("retained.png");
        std::fs::write(&target, b"retained").unwrap();
        let link = dir.path().join("TV.png");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let saved = save(dir.path(), "TV", b"new").unwrap();
        assert_eq!(saved, dir.path().join("TV (2).png"));
        assert!(link.is_symlink());
        assert_eq!(std::fs::read(target).unwrap(), b"retained");
        assert_eq!(std::fs::read(saved).unwrap(), b"new");
    }
}

//! Auto-install of platform-tools — matches v1's `Check-Adb` behavior:
//! downloads `platform-tools-latest-<os>.zip` from Google when no adb is
//! found locally, extracts into the OS app-data directory, and returns the
//! path to the unpacked `adb` binary.

use std::io::{Read, Seek};
use std::path::{Path, PathBuf};

use thiserror::Error;

/// Google's official platform-tools download endpoints.
fn platform_tools_url() -> &'static str {
    // Picked by target OS at compile time. The latest- aliases are stable
    // URLs Google maintains: https://developer.android.com/tools/releases/platform-tools
    if cfg!(target_os = "macos") {
        "https://dl.google.com/android/repository/platform-tools-latest-darwin.zip"
    } else if cfg!(target_os = "linux") {
        "https://dl.google.com/android/repository/platform-tools-latest-linux.zip"
    } else if cfg!(target_os = "windows") {
        "https://dl.google.com/android/repository/platform-tools-latest-windows.zip"
    } else {
        // Best-effort fallback — won't actually work on unsupported OSes
        // but at least we hand a real URL to the error.
        "https://dl.google.com/android/repository/platform-tools-latest-linux.zip"
    }
}

#[derive(Debug, Error)]
pub enum InstallError {
    #[error("HTTP error downloading platform-tools: {0}")]
    Download(#[from] reqwest::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("ZIP error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("platform-tools archive did not contain the expected adb binary")]
    MissingAdb,
    #[error("could not resolve the install directory")]
    NoInstallDir,
}

/// Where we extract platform-tools to — matches the `dirs::data_local_dir`
/// convention used elsewhere for snapshots.
fn install_root() -> Option<PathBuf> {
    Some(
        dirs::data_local_dir()?
            .join("ShieldOptimizer")
            .join("platform-tools"),
    )
}

/// Path to the adb binary inside `install_root`, regardless of whether it
/// currently exists.
pub fn adb_path_in_install_root() -> Option<PathBuf> {
    let exe = if cfg!(windows) { "adb.exe" } else { "adb" };
    Some(install_root()?.join(exe))
}

/// Download and unpack platform-tools. Returns the path to the extracted
/// adb binary on success.
///
/// Network call is async, ZIP extraction is sync (the `zip` crate is sync;
/// archives are ~12MB so this is fine on a single blocking-thread spawn).
pub async fn install_platform_tools() -> Result<PathBuf, InstallError> {
    let install_to = install_root().ok_or(InstallError::NoInstallDir)?;
    tokio::fs::create_dir_all(&install_to).await?;

    let url = platform_tools_url();
    tracing::info!(%url, dest = %install_to.display(), "downloading platform-tools");

    let bytes = reqwest::get(url).await?.error_for_status()?.bytes().await?;
    tracing::info!(size = bytes.len(), "platform-tools archive downloaded");

    // Extract on a blocking thread — zip crate is synchronous.
    let install_clone = install_to.clone();
    let adb_path = tokio::task::spawn_blocking(move || -> Result<PathBuf, InstallError> {
        let cursor = std::io::Cursor::new(bytes.as_ref());
        extract_zip_to(cursor, &install_clone)
    })
    .await
    .map_err(|e| InstallError::Io(std::io::Error::other(e.to_string())))??;

    Ok(adb_path)
}

/// Extract the platform-tools ZIP, flattening the top-level `platform-tools/`
/// directory the archive ships with so the binary lands at `<root>/adb`.
/// Returns the final adb path.
fn extract_zip_to<R: Read + Seek>(reader: R, root: &Path) -> Result<PathBuf, InstallError> {
    let mut archive = zip::ZipArchive::new(reader)?;
    let exe_name = if cfg!(windows) { "adb.exe" } else { "adb" };
    let mut adb_path: Option<PathBuf> = None;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let raw_name = entry.name().to_string();

        // Strip the leading `platform-tools/` prefix that Google's archive
        // ships with. If the entry isn't under that prefix, skip it — we
        // don't want to extract anything outside the install root.
        let rel = match raw_name.strip_prefix("platform-tools/") {
            Some(r) if !r.is_empty() => r,
            _ => continue,
        };

        // Zip-slip protection: refuse any entry whose path tries to escape.
        // Reject both `..` traversal and absolute paths (a zip can legally
        // contain an entry like `/etc/passwd` — `Path::join` would discard
        // `root` and write to the absolute target without this guard).
        let rel_path = Path::new(rel);
        if rel_path.is_absolute()
            || rel_path
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            continue;
        }

        let out_path = root.join(rel_path);
        // Belt and suspenders: verify the resolved path is under `root`
        // even after `join` does its substitution-on-absolute thing.
        if !out_path.starts_with(root) {
            continue;
        }

        if entry.is_dir() {
            std::fs::create_dir_all(&out_path)?;
            continue;
        }

        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut out = std::fs::File::create(&out_path)?;
        std::io::copy(&mut entry, &mut out)?;

        // Preserve the executable bit on Unix so `adb` is runnable.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Some(mode) = entry.unix_mode() {
                let _ = std::fs::set_permissions(&out_path, std::fs::Permissions::from_mode(mode));
            }
        }

        if out_path.file_name().and_then(|n| n.to_str()) == Some(exe_name) {
            adb_path = Some(out_path);
        }
    }

    adb_path.ok_or(InstallError::MissingAdb)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_root_is_under_data_local_dir() {
        let root = install_root().expect("data_local_dir resolves on test platform");
        assert!(root.ends_with("ShieldOptimizer/platform-tools"));
    }

    #[test]
    fn platform_tools_url_matches_current_os() {
        let url = platform_tools_url();
        assert!(url.starts_with("https://dl.google.com/android/repository/platform-tools-latest-"));
        if cfg!(target_os = "macos") {
            assert!(url.ends_with("darwin.zip"));
        } else if cfg!(target_os = "linux") {
            assert!(url.ends_with("linux.zip"));
        } else if cfg!(windows) {
            assert!(url.ends_with("windows.zip"));
        }
    }

    #[test]
    fn extract_zip_rejects_zip_slip() {
        // Build a tiny zip in memory that tries to write outside the target dir.
        let buf = std::io::Cursor::new(Vec::<u8>::new());
        let mut writer = zip::ZipWriter::new(buf);
        writer
            .start_file::<_, ()>(
                "platform-tools/../escape.txt",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
        std::io::Write::write_all(&mut writer, b"should not be extracted").unwrap();
        let buf = writer.finish().unwrap();

        let temp = tempfile::tempdir().unwrap_or_else(|_| {
            // Fall back to a known-good temp path if tempfile isn't available.
            panic!("could not get tempdir");
        });
        let cursor = std::io::Cursor::new(buf.into_inner());
        // Should fail with MissingAdb (since we didn't write an adb entry),
        // and not have written the escape file.
        let result = extract_zip_to(cursor, temp.path());
        assert!(matches!(result, Err(InstallError::MissingAdb)));

        let escape = temp.path().parent().unwrap().join("escape.txt");
        assert!(!escape.exists(), "zip-slip protection failed");
    }
}

//! Auto-install of platform-tools — matches v1's `Check-Adb` behavior:
//! downloads `platform-tools-latest-<os>.zip` from Google when no adb is
//! found locally, extracts into the OS app-data directory, and returns the
//! path to the unpacked `adb` binary.

use std::io::{Read, Seek};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use atv_optimizer_core::adb::Lease;

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
    #[error("platform-tools download is larger than {limit} bytes; refusing it")]
    TooLarge { limit: u64 },
    #[error(
        "could not replace the existing platform-tools at {path} ({source}); the existing \
         install was left as it was. Quit anything using adb from that folder and try again."
    )]
    Replace {
        path: String,
        source: std::io::Error,
    },
}

/// Google's archive is about 12 MB; anything far larger is not it.
const MAX_ARCHIVE_BYTES: u64 = 100 * 1024 * 1024;

/// Where we extract platform-tools to: inside the session's data folder,
/// beside the snapshots.
fn install_root() -> Option<PathBuf> {
    Some(crate::rename::session_data_dir()?.join("platform-tools"))
}

fn adb_exe() -> &'static str {
    if cfg!(windows) {
        "adb.exe"
    } else {
        "adb"
    }
}

/// Path to the adb binary inside `install_root`, regardless of whether it
/// currently exists.
pub fn adb_path_in_install_root() -> Option<PathBuf> {
    Some(install_root()?.join(adb_exe()))
}

/// The platform-tools a release before the rename downloaded. The data-folder
/// migration leaves it where it is, so it keeps working until a new install.
pub fn adb_path_in_legacy_install_root() -> Option<PathBuf> {
    Some(
        dirs::data_local_dir()?
            .join(crate::rename::LEGACY_DATA_DIR_NAME)
            .join("platform-tools")
            .join(adb_exe()),
    )
}

/// Download and unpack platform-tools. Returns the path to the extracted
/// adb binary on success.
///
/// Network call is async, ZIP extraction is sync (the `zip` crate is sync;
/// archives are ~12MB so this is fine on a single blocking-thread spawn).
pub async fn install_platform_tools(operation: Arc<Lease>) -> Result<PathBuf, InstallError> {
    let install_to = install_root().ok_or(InstallError::NoInstallDir)?;

    let url = platform_tools_url();
    tracing::info!(%url, dest = %install_to.display(), "downloading platform-tools");

    let bytes = download_capped(url, MAX_ARCHIVE_BYTES).await?;
    tracing::info!(size = bytes.len(), "platform-tools archive downloaded");

    finish_extraction(operation, move || -> Result<PathBuf, InstallError> {
        install_archive(std::io::Cursor::new(bytes), &install_to)
    })
    .await
}

async fn download_capped(url: &str, limit: u64) -> Result<Vec<u8>, InstallError> {
    let client = reqwest::Client::builder()
        .user_agent("atv-optimizer-platform-tools")
        .connect_timeout(std::time::Duration::from_secs(15))
        .timeout(std::time::Duration::from_secs(600))
        .build()?;
    let mut response = client.get(url).send().await?.error_for_status()?;
    if response.content_length().is_some_and(|len| len > limit) {
        return Err(InstallError::TooLarge { limit });
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if body.len() as u64 + chunk.len() as u64 > limit {
            return Err(InstallError::TooLarge { limit });
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

/// Unpack into a sibling folder, check the result is a usable adb, then swap
/// it in. A partial extract (disk full, a locked file) never replaces a
/// working install, and never leaves an `adb` without its libraries where
/// discovery would pick it up.
fn install_archive<R: Read + Seek>(reader: R, root: &Path) -> Result<PathBuf, InstallError> {
    let parent = root.parent().ok_or(InstallError::NoInstallDir)?;
    std::fs::create_dir_all(parent)?;
    remove_leftovers(parent);
    let stamp = format!(
        "{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    );
    let staging = parent.join(format!("{STAGING_PREFIX}{stamp}"));
    std::fs::create_dir(&staging)?;
    let staged = extract_zip_to(reader, &staging).and_then(|adb| {
        if usable_adb(&adb) {
            Ok(adb)
        } else {
            Err(InstallError::MissingAdb)
        }
    });
    let staged = match staged {
        Ok(adb) => adb,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(e);
        }
    };
    let relative = staged
        .strip_prefix(&staging)
        .map(Path::to_path_buf)
        .map_err(|_| InstallError::MissingAdb)?;

    let previous = parent.join(format!("{RETIRED_PREFIX}{stamp}"));
    let had_previous = root.exists();
    if had_previous {
        // Windows refuses to rename a folder whose adb.exe is running.
        if let Err(source) = rename_retrying(root, &previous) {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(InstallError::Replace {
                path: root.display().to_string(),
                source,
            });
        }
    }
    if let Err(source) = rename_retrying(&staging, root) {
        if had_previous {
            let _ = std::fs::rename(&previous, root);
        }
        let _ = std::fs::remove_dir_all(&staging);
        return Err(InstallError::Replace {
            path: root.display().to_string(),
            source,
        });
    }
    if had_previous {
        let _ = std::fs::remove_dir_all(&previous);
    }
    Ok(root.join(relative))
}

/// Antivirus scanners (Windows Defender above all) hold a just-written
/// `adb.exe` open for a moment, and the folder rename fails with access
/// denied until they let go.
const RENAME_BACKOFF_MS: &[u64] = &[100, 250, 500, 1000];

fn rename_retrying(from: &Path, to: &Path) -> std::io::Result<()> {
    retry_rename(
        || std::fs::rename(from, to),
        |ms| std::thread::sleep(std::time::Duration::from_millis(ms)),
    )
}

fn retry_rename(
    mut rename: impl FnMut() -> std::io::Result<()>,
    mut sleep: impl FnMut(u64),
) -> std::io::Result<()> {
    let mut backoff = RENAME_BACKOFF_MS.iter();
    loop {
        match rename() {
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => match backoff.next() {
                Some(&ms) => sleep(ms),
                None => return Err(e),
            },
            other => return other,
        }
    }
}

const STAGING_PREFIX: &str = "platform-tools.installing-";
const RETIRED_PREFIX: &str = "platform-tools.replaced-";

/// Folders an interrupted earlier install left beside the real one. Only
/// names this module creates are touched.
fn remove_leftovers(parent: &Path) {
    let Ok(entries) = std::fs::read_dir(parent) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if name.starts_with(STAGING_PREFIX) || name.starts_with(RETIRED_PREFIX) {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

fn usable_adb(path: &Path) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() || metadata.len() == 0 {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            return false;
        }
    }
    true
}

pub(crate) async fn finish_extraction<F>(
    operation: Arc<Lease>,
    extract: F,
) -> Result<PathBuf, InstallError>
where
    F: FnOnce() -> Result<PathBuf, InstallError> + Send + 'static,
{
    tokio::task::spawn_blocking(move || {
        // The worker outlives cancellation of its async waiter.
        let _operation = operation;
        extract()
    })
    .await
    .map_err(|e| InstallError::Io(std::io::Error::other(e.to_string())))?
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

        if out_path.file_name().and_then(|n| n.to_str()) == Some(exe_name)
            && out_path.parent() == Some(root)
        {
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
        assert!(root.ends_with("ATVOptimizer/platform-tools"));
        let legacy = adb_path_in_legacy_install_root().unwrap();
        assert!(legacy
            .parent()
            .unwrap()
            .ends_with("ShieldOptimizer/platform-tools"));
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

    fn archive(entries: &[(&str, &[u8], u32)]) -> std::io::Cursor<Vec<u8>> {
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::<u8>::new()));
        for (name, body, mode) in entries {
            writer
                .start_file::<_, ()>(
                    *name,
                    zip::write::SimpleFileOptions::default().unix_permissions(*mode),
                )
                .unwrap();
            std::io::Write::write_all(&mut writer, body).unwrap();
        }
        std::io::Cursor::new(writer.finish().unwrap().into_inner())
    }

    #[test]
    fn a_locked_rename_is_retried_then_given_up() {
        use std::io::{Error, ErrorKind};
        let mut calls = 0;
        let mut slept = Vec::new();
        let r = retry_rename(
            || {
                calls += 1;
                if calls < 3 {
                    Err(Error::from(ErrorKind::PermissionDenied))
                } else {
                    Ok(())
                }
            },
            |ms| slept.push(ms),
        );
        assert!(r.is_ok());
        assert_eq!((calls, slept), (3, vec![100, 250]));

        let mut calls = 0;
        let r = retry_rename(
            || {
                calls += 1;
                Err(Error::from(ErrorKind::PermissionDenied))
            },
            |_| {},
        );
        assert_eq!(r.unwrap_err().kind(), ErrorKind::PermissionDenied);
        assert_eq!(calls, RENAME_BACKOFF_MS.len() + 1);

        // Anything else fails at once.
        let mut calls = 0;
        let r = retry_rename(
            || {
                calls += 1;
                Err(Error::from(ErrorKind::NotFound))
            },
            |_| panic!("must not wait"),
        );
        assert_eq!((r.unwrap_err().kind(), calls), (ErrorKind::NotFound, 1));
    }

    #[test]
    fn a_fresh_install_lands_in_place_with_its_libraries() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("platform-tools");
        let adb = install_archive(
            archive(&[
                (&format!("platform-tools/{}", adb_exe()), b"adb", 0o755),
                ("platform-tools/AdbWinApi.dll", b"dll", 0o644),
            ]),
            &root,
        )
        .unwrap();
        assert_eq!(adb, root.join(adb_exe()));
        assert!(root.join("AdbWinApi.dll").is_file());
        let leftovers: Vec<_> = std::fs::read_dir(temp.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(leftovers, vec![std::ffi::OsString::from("platform-tools")]);
    }

    #[test]
    fn a_broken_archive_leaves_the_working_install_alone() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("platform-tools");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join(adb_exe()), b"old adb").unwrap();

        let no_adb = install_archive(archive(&[("platform-tools/README", b"x", 0o644)]), &root);
        assert!(matches!(no_adb, Err(InstallError::MissingAdb)));
        let truncated = install_archive(std::io::Cursor::new(b"PK\x03\x04 short".to_vec()), &root);
        assert!(truncated.is_err());
        #[cfg(unix)]
        {
            let not_executable = install_archive(
                archive(&[(&format!("platform-tools/{}", adb_exe()), b"adb", 0o644)]),
                &root,
            );
            assert!(matches!(not_executable, Err(InstallError::MissingAdb)));
        }

        assert_eq!(std::fs::read(root.join(adb_exe())).unwrap(), b"old adb");
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 1);
    }

    #[test]
    fn a_new_archive_replaces_the_old_install_and_sweeps_leftovers() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("platform-tools");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("stale.dll"), b"old").unwrap();
        std::fs::create_dir(temp.path().join(format!("{STAGING_PREFIX}1-2"))).unwrap();
        std::fs::create_dir(temp.path().join("unrelated")).unwrap();

        install_archive(
            archive(&[(&format!("platform-tools/{}", adb_exe()), b"new adb", 0o755)]),
            &root,
        )
        .unwrap();
        assert_eq!(std::fs::read(root.join(adb_exe())).unwrap(), b"new adb");
        assert!(!root.join("stale.dll").exists());
        assert!(temp.path().join("unrelated").is_dir());
        assert!(!temp.path().join(format!("{STAGING_PREFIX}1-2")).exists());
    }
}

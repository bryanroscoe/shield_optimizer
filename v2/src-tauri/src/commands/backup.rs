//! APK backup (device → local folder) and app cloning (device → device).
//!
//! Backup resolves the package's APK(s) with `pm path` — split APKs return
//! multiple lines that must later be installed together — and pulls each to
//! a user-chosen folder. Cloning chains the same pull into the existing
//! install path against a second device (`install` / `install-multiple`).

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::State;

use crate::engine::install_outcome::{
    decode_install_error, install_succeeded, parse_pm_paths, pm_path_failed,
};
use crate::engine::is_valid_package_name;

use super::AppState;

#[derive(Serialize)]
pub struct BackupApkResult {
    pub ok: bool,
    /// Local paths of the pulled APK file(s).
    pub files: Vec<String>,
    /// More than one APK — a split APK that must be installed together
    /// (`adb install-multiple`).
    pub split: bool,
    pub message: String,
}

/// Pull `versionName=` out of `dumpsys package <pkg>` output.
fn parse_version_name(dumpsys: &str) -> Option<String> {
    dumpsys
        .lines()
        .find_map(|l| l.trim().strip_prefix("versionName="))
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// Local filename for one pulled APK: `<pkg>-<version>.apk` for a single
/// APK, `<pkg>-<version>-<remote stem>.apk` for split parts (the stem keeps
/// `base` / `split_config.en` distinguishable).
fn backup_file_name(package: &str, version: Option<&str>, remote: &str, split: bool) -> String {
    let version = version
        .and_then(safe_name_component)
        .unwrap_or_else(|| "unknown".to_string());
    if !split {
        return format!("{package}-{version}.apk");
    }
    let stem = Path::new(remote)
        .file_stem()
        .and_then(|s| s.to_str())
        .and_then(safe_name_component)
        .unwrap_or_else(|| "part".to_string());
    format!("{package}-{version}-{stem}.apk")
}

/// Local filenames for every pulled APK, unique within one backup.
/// Sanitising can map two split names to one file (`a:b` and `a_b`, a 64-char
/// cap, `..` → `part`); a repeat gets `-2`, `-3`… before `.apk`. Names that
/// do not collide stay exactly as earlier backups spelled them, and restore
/// (`clone_app`) uses the returned paths rather than reading names back.
fn backup_file_names(package: &str, version: Option<&str>, remotes: &[String]) -> Vec<String> {
    let split = remotes.len() > 1;
    let mut used = std::collections::HashSet::new();
    remotes
        .iter()
        .map(|remote| {
            let name = backup_file_name(package, version, remote, split);
            let stem = name.strip_suffix(".apk").unwrap_or(&name).to_string();
            let mut candidate = name;
            let mut n = 2;
            while !used.insert(candidate.to_ascii_lowercase()) {
                candidate = format!("{stem}-{n}.apk");
                n += 1;
            }
            candidate
        })
        .collect()
}

/// The device reports `versionName` and the APK paths, so either can carry
/// separators or `..`. Only `[A-Za-z0-9._-]` reaches a local file name;
/// anything that still spells a parent directory is dropped.
fn safe_name_component(raw: &str) -> Option<String> {
    let cleaned: String = raw
        .chars()
        .take(64)
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() || cleaned.contains("..") {
        None
    } else {
        Some(cleaned)
    }
}

/// Resolve a package's APK paths and version on `serial`.
async fn apk_paths_and_version(
    adb: &dyn crate::adb::AdbDriver,
    serial: &str,
    package: &str,
) -> Result<(Vec<String>, Option<String>), String> {
    let path_out = adb
        .shell(serial, &format!("pm path {package}"))
        .await
        .map_err(|e| format!("pm path: {e}"))?;
    if pm_path_failed(path_out.exit_code, &path_out.stdout, &path_out.stderr) {
        return Err(format!(
            "Could not resolve {package}: {}",
            path_out.combined().trim()
        ));
    }
    let remotes = parse_pm_paths(&path_out.stdout)?;
    if remotes.is_empty() {
        return Err(format!(
            "{package} has no APK path on the device — is it installed? ({})",
            path_out.combined().trim()
        ));
    }
    let version = adb
        .shell(serial, &format!("dumpsys package {package}"))
        .await
        .ok()
        .and_then(|out| parse_version_name(&out.stdout));
    Ok((remotes, version))
}

async fn pull_apks(
    adb: &dyn crate::adb::AdbDriver,
    serial: &str,
    package: &str,
    remotes: &[String],
    version: Option<&str>,
    dest: &Path,
) -> Result<Vec<PathBuf>, String> {
    let names = backup_file_names(package, version, remotes);
    let mut locals = Vec::new();
    for (remote, name) in remotes.iter().zip(names) {
        let local = dest.join(name);
        if local.parent() != Some(dest) {
            return Err(format!(
                "Refusing to save {remote} outside {}",
                dest.display()
            ));
        }
        let local_str = local.display().to_string();
        adb.raw_transfer(&["-s", serial, "pull", remote, &local_str])
            .await
            .map_err(|e| format!("pull {remote}: {e}"))?;
        locals.push(local);
    }
    Ok(locals)
}

/// `backup_apk` — save an installed app's APK(s) to `dest_dir` on this
/// computer. The frontend obtains `dest_dir` from the folder picker.
#[tauri::command]
pub async fn backup_apk(
    state: State<'_, AppState>,
    serial: String,
    package: String,
    dest_dir: String,
) -> Result<BackupApkResult, String> {
    if !is_valid_package_name(&package) {
        return Err(format!("Invalid package name: {package:?}"));
    }
    let dest = PathBuf::from(&dest_dir);
    if !dest.is_dir() {
        return Err(format!("Not a folder: {dest_dir}"));
    }

    let adb = state.adb_snapshot().await;
    let (remotes, version) = apk_paths_and_version(adb.as_ref(), &serial, &package).await?;
    let split = remotes.len() > 1;
    let locals = pull_apks(
        adb.as_ref(),
        &serial,
        &package,
        &remotes,
        version.as_deref(),
        &dest,
    )
    .await?;

    let message = if split {
        format!(
            "Saved {} split-APK parts to {dest_dir}. Install them together (Copy to another \
             device does this automatically; manual installs need `adb install-multiple`).",
            locals.len()
        )
    } else {
        format!("Saved {} to {dest_dir}.", locals[0].display())
    };
    Ok(BackupApkResult {
        ok: true,
        files: locals.iter().map(|p| p.display().to_string()).collect(),
        split,
        message,
    })
}

#[derive(Serialize)]
pub struct CloneAppResult {
    pub ok: bool,
    pub message: String,
    /// Decoded hint for common install failures, when available.
    pub hint: Option<String>,
}

/// `clone_app` — copy an installed app from one connected device to another:
/// pull its APK(s) to a temp folder, then `install` / `install-multiple` on
/// the target. App *data* does not transfer (no root), and DRM/licensed apps
/// may refuse to run — the UI surfaces that caveat up front.
#[tauri::command]
pub async fn clone_app(
    state: State<'_, AppState>,
    source_serial: String,
    target_serial: String,
    package: String,
) -> Result<CloneAppResult, String> {
    if !is_valid_package_name(&package) {
        return Err(format!("Invalid package name: {package:?}"));
    }
    if source_serial == target_serial {
        return Err("Source and target device are the same.".to_string());
    }

    let adb = state.adb_snapshot().await;
    let (remotes, version) = apk_paths_and_version(adb.as_ref(), &source_serial, &package).await?;

    let temp = super::files::private_temp_dir("atv-clone")
        .await
        .map_err(|e| format!("create temp dir: {e}"))?;

    let result = async {
        let locals = pull_apks(
            adb.as_ref(),
            &source_serial,
            &package,
            &remotes,
            version.as_deref(),
            &temp,
        )
        .await?;

        let mut args: Vec<String> = vec!["-s".into(), target_serial.clone()];
        args.push(if locals.len() > 1 {
            "install-multiple".into()
        } else {
            "install".into()
        });
        args.push("-r".into());
        args.extend(locals.iter().map(|p| p.display().to_string()));
        let args_ref: Vec<&str> = args.iter().map(String::as_str).collect();

        let out = super::sideload::install_answer(adb.raw_transfer(&args_ref).await)?;
        let combined = out.combined().trim().to_string();
        let ok = install_succeeded(out.success(), &combined);
        Ok::<CloneAppResult, String>(CloneAppResult {
            hint: if ok {
                None
            } else {
                decode_install_error(&combined)
            },
            message: if ok {
                format!(
                    "Installed {package} on {target_serial}. App data does not transfer — sign \
                     in again on the target device."
                )
            } else {
                combined
            },
            ok,
        })
    }
    .await;

    // Best-effort temp cleanup either way.
    let _ = tokio::fs::remove_dir_all(&temp).await;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[tokio::test]
    async fn a_package_named_like_an_error_still_resolves() {
        use crate::commands::test_support::{state_with, MockAdb};
        let state = state_with(
            MockAdb::default()
                .on_shell(
                    "pm path",
                    "package:/data/app/~~x==/com.example.ErrorReporter-1==/base.apk\n",
                )
                .on_shell("dumpsys package", "    versionName=1.0\n"),
        );
        let adb = state.adb_snapshot().await;
        let (paths, version) =
            apk_paths_and_version(adb.as_ref(), "tv", "com.example.ErrorReporter")
                .await
                .expect("a clean answer is not a failure");
        assert_eq!(paths.len(), 1);
        assert_eq!(version.as_deref(), Some("1.0"));
    }

    #[tokio::test]
    async fn an_unsafe_or_failed_pm_path_is_refused() {
        use crate::commands::test_support::{state_with, MockAdb};
        for (stdout, exit) in [
            ("package:/data/app/x/../../escape.apk\n", 0),
            ("package:/data/app/x/base.apk\n", 1),
            ("Error: unknown package\n", 0),
        ] {
            let state = state_with(MockAdb::default().on_shell_exit("pm path", stdout, exit));
            let adb = state.adb_snapshot().await;
            assert!(
                apk_paths_and_version(adb.as_ref(), "tv", "com.example.app")
                    .await
                    .is_err(),
                "{stdout:?} exit {exit}"
            );
        }
    }

    #[test]
    fn parses_version_name_from_dumpsys() {
        let out = "  Package [com.plexapp.android] (abc):\n    versionCode=12345\n    versionName=10.2.0.5678\n";
        assert_eq!(parse_version_name(out).as_deref(), Some("10.2.0.5678"));
        assert_eq!(parse_version_name("no version here"), None);
    }

    #[test]
    fn names_single_and_split_backups() {
        assert_eq!(
            backup_file_name("com.x", Some("1.2"), "/data/app/y/base.apk", false),
            "com.x-1.2.apk"
        );
        assert_eq!(
            backup_file_name(
                "com.x",
                Some("1.2"),
                "/data/app/y/split_config.en.apk",
                true
            ),
            "com.x-1.2-split_config.en.apk"
        );
        assert_eq!(
            backup_file_name("com.x", None, "/data/app/y/base.apk", false),
            "com.x-unknown.apk"
        );
    }

    #[test]
    fn a_device_reported_version_or_path_never_leaves_the_chosen_folder() {
        for hostile in ["1/../../../x", "..", "../evil", "1\\..\\..\\x", "a..b", ""] {
            let name = backup_file_name("com.x", Some(hostile), "/data/app/y/base.apk", false);
            assert_eq!(name, "com.x-unknown.apk", "{hostile:?}");
        }
        assert_eq!(
            backup_file_name(
                "com.x",
                Some("2.0 beta/rc:1"),
                "/data/app/y/base.apk",
                false
            ),
            "com.x-2.0_beta_rc_1.apk"
        );
        assert_eq!(
            backup_file_name("com.x", Some("1.0"), "/data/app/y/sp lit:..x.apk", true),
            "com.x-1.0-part.apk"
        );
        let dest = Path::new("chosen");
        for version in ["1/../../../x", "C:\\evil", "/abs/path"] {
            let local = dest.join(backup_file_name(
                "com.x",
                Some(version),
                "/data/app/y/base.apk",
                false,
            ));
            assert_eq!(local.parent(), Some(dest), "{version:?}");
        }
    }

    #[tokio::test]
    async fn colliding_split_names_never_overwrite_each_other() {
        use crate::commands::test_support::{state_with, MockAdb};
        let state = state_with(MockAdb::default());
        let adb = state.adb_snapshot().await;
        let dest = tempfile::tempdir().unwrap();
        let remotes: Vec<String> = [
            "/data/app/y/base.apk",
            "/data/app/y/split:a.apk",
            "/data/app/y/split_a.apk",
            "/data/app/y/x..y.apk",
            "/data/app/y/..z.apk",
        ]
        .map(String::from)
        .to_vec();
        let locals = pull_apks(
            adb.as_ref(),
            "tv",
            "com.x",
            &remotes,
            Some("1.0"),
            dest.path(),
        )
        .await
        .unwrap();
        let names: Vec<String> = locals
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            names,
            [
                "com.x-1.0-base.apk",
                "com.x-1.0-split_a.apk",
                "com.x-1.0-split_a-2.apk",
                "com.x-1.0-part.apk",
                "com.x-1.0-part-2.apk",
            ]
        );
        assert!(locals.iter().all(|p| p.parent() == Some(dest.path())));
    }

    #[tokio::test]
    async fn pulls_land_inside_the_destination_whatever_the_version_says() {
        use crate::commands::test_support::{state_with, MockAdb};
        let state = state_with(MockAdb::default());
        let adb = state.adb_snapshot().await;
        let dest = tempfile::tempdir().unwrap();
        let locals = pull_apks(
            adb.as_ref(),
            "tv",
            "com.x",
            &["/data/app/y/base.apk".to_string()],
            Some("1/../../../x"),
            dest.path(),
        )
        .await
        .unwrap();
        assert_eq!(locals, vec![dest.path().join("com.x-unknown.apk")]);
    }
}

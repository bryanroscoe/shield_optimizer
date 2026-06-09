//! APK backup (device → local folder) and app cloning (device → device).
//!
//! Backup resolves the package's APK(s) with `pm path` — split APKs return
//! multiple lines that must later be installed together — and pulls each to
//! a user-chosen folder. Cloning chains the same pull into the existing
//! install path against a second device (`install` / `install-multiple`).

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::State;

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

/// Parse `pm path <pkg>` output: one `package:<path>` line per APK.
fn parse_pm_path_output(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .filter_map(|l| l.trim().strip_prefix("package:"))
        .filter(|p| !p.is_empty())
        .map(str::to_string)
        .collect()
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
    let version = version.unwrap_or("unknown");
    if !split {
        return format!("{package}-{version}.apk");
    }
    let stem = Path::new(remote)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("part");
    format!("{package}-{version}-{stem}.apk")
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
    let remotes = parse_pm_path_output(&path_out.stdout);
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
    let split = remotes.len() > 1;
    let mut locals = Vec::new();
    for remote in remotes {
        let local = dest.join(backup_file_name(package, version, remote, split));
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

    let stamp = chrono::Local::now().format("%Y%m%d%H%M%S%3f");
    let temp = std::env::temp_dir().join(format!("shield-clone-{stamp}"));
    tokio::fs::create_dir_all(&temp)
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

        let out = adb
            .raw_transfer(&args_ref)
            .await
            .map_err(|e| format!("adb install: {e}"))?;
        let combined = out.combined().trim().to_string();
        let ok = combined.contains("Success");
        Ok::<CloneAppResult, String>(CloneAppResult {
            hint: if ok {
                None
            } else {
                super::sideload::decode_install_error(&combined)
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

    #[test]
    fn parses_single_apk_path() {
        let out = "package:/data/app/~~abc==/com.plexapp.android-xyz==/base.apk\n";
        assert_eq!(
            parse_pm_path_output(out),
            vec!["/data/app/~~abc==/com.plexapp.android-xyz==/base.apk".to_string()]
        );
    }

    #[test]
    fn parses_split_apk_paths() {
        let out = "package:/data/app/x/base.apk\npackage:/data/app/x/split_config.en.apk\n";
        assert_eq!(parse_pm_path_output(out).len(), 2);
    }

    #[test]
    fn ignores_noise_lines() {
        assert!(parse_pm_path_output("error: no devices\n").is_empty());
        assert!(parse_pm_path_output("").is_empty());
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
}

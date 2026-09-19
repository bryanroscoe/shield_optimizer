//! APK sideload — `adb install` against a user-picked file.

use serde::Serialize;
use std::path::PathBuf;
use tauri::State;

use super::AppState;

#[derive(Serialize)]
pub struct InstallApkResult {
    pub ok: bool,
    /// Path that was installed (or attempted).
    pub path: String,
    /// adb's verbatim output — surfaces helpful errors like
    /// `INSTALL_FAILED_VERSION_DOWNGRADE` to the user.
    pub message: String,
    /// Optional decoded hint for common failure codes.
    pub hint: Option<String>,
}

/// `install_apk` — `adb -s <serial> install [-r] <path>`. The frontend uses
/// the dialog plugin to obtain a file path before calling this.
#[tauri::command]
pub async fn install_apk(
    state: State<'_, AppState>,
    serial: String,
    apk_path: String,
    reinstall: Option<bool>,
) -> Result<InstallApkResult, String> {
    let path_buf = PathBuf::from(&apk_path);
    if !path_buf.is_file() {
        return Ok(InstallApkResult {
            ok: false,
            path: apk_path,
            message: "APK file does not exist".to_string(),
            hint: None,
        });
    }

    let adb = state.adb_snapshot().await;
    let mut args: Vec<String> = vec!["-s".into(), serial.clone(), "install".into()];
    if reinstall.unwrap_or(true) {
        args.push("-r".into());
    }
    args.push(apk_path.clone());
    let args_ref: Vec<&str> = args.iter().map(String::as_str).collect();

    let out = adb
        .raw_transfer(&args_ref)
        .await
        .map_err(|e| format!("adb install: {e}"))?;
    let combined = if out.stdout.trim().is_empty() {
        out.stderr.clone()
    } else {
        out.stdout.clone()
    };
    let ok = combined.contains("Success");
    let hint = decode_install_error(&combined);

    Ok(InstallApkResult {
        ok,
        path: apk_path,
        message: combined,
        hint,
    })
}

#[derive(Serialize)]
pub struct DiscoveredApk {
    pub path: String,
    pub name: String,
    pub size_bytes: u64,
    /// Package id read from the APK's AndroidManifest.xml, when decodable.
    /// Lets the UI flag APKs that are already installed on the device.
    pub package: Option<String>,
}

/// Read the `package` attribute from an APK's binary `AndroidManifest.xml`.
/// Returns `None` if the file isn't a readable APK or the manifest can't be
/// decoded — best-effort; the install flow doesn't depend on it.
fn read_apk_package_id(apk_path: &std::path::Path) -> Option<String> {
    let file = std::fs::File::open(apk_path).ok()?;
    let mut zip = zip::ZipArchive::new(file).ok()?;
    let mut manifest = zip.by_name("AndroidManifest.xml").ok()?;
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(&mut manifest, &mut bytes).ok()?;
    let doc = axmldecoder::parse(&bytes).ok()?;
    if let Some(axmldecoder::Node::Element(root)) = doc.get_root() {
        if root.get_tag() == "manifest" {
            return root.get_attributes().get("package").cloned();
        }
    }
    None
}

/// What an APK claims about itself, read locally before anything is installed.
///
/// Every field is best-effort: an APK whose manifest will not decode still
/// reports its name and size, and the UI says the rest is unknown rather than
/// guessing. Nothing here is a signature check — it answers "what is this and
/// will it run here", not "is it trustworthy".
#[derive(Serialize)]
pub struct ApkInspection {
    pub path: String,
    pub name: String,
    pub size_bytes: u64,
    pub package: Option<String>,
    /// ABIs the APK ships native code for, from its `lib/<abi>/` entries.
    /// Empty means no native libraries at all, which runs anywhere.
    pub abis: Vec<String>,
    /// ABIs the device reports (`ro.product.cpu.abilist`), when we could ask.
    pub device_abis: Vec<String>,
    /// `Some(false)` only when the APK ships native code and none of it matches
    /// the device. `None` means we could not read one side or the other, and
    /// the UI must not claim a mismatch it did not establish.
    pub abi_compatible: Option<bool>,
    /// True when the device already reports this package installed.
    pub already_installed: bool,
}

/// Read the `lib/<abi>/` prefixes an APK ships native code for.
fn read_apk_abis(apk_path: &std::path::Path) -> Vec<String> {
    let Ok(file) = std::fs::File::open(apk_path) else {
        return Vec::new();
    };
    let Ok(mut zip) = zip::ZipArchive::new(file) else {
        return Vec::new();
    };
    let mut abis: Vec<String> = Vec::new();
    for i in 0..zip.len() {
        let Ok(entry) = zip.by_index(i) else { continue };
        let name = entry.name();
        let Some(rest) = name.strip_prefix("lib/") else {
            continue;
        };
        let Some((abi, _)) = rest.split_once('/') else {
            continue;
        };
        if !abi.is_empty() && !abis.iter().any(|a| a == abi) {
            abis.push(abi.to_string());
        }
    }
    abis.sort();
    abis
}

/// `inspect_apk` — read what an APK is before installing it.
///
/// Exists because a drag-and-drop install has no file picker to read the name
/// out of: without this the user learns what they installed from the result
/// message, which is far too late. Pure reads — this never touches the device
/// beyond two `getprop`/`pm` queries.
#[tauri::command]
pub async fn inspect_apk(
    state: State<'_, AppState>,
    serial: String,
    path: String,
) -> Result<ApkInspection, String> {
    let apk = PathBuf::from(&path);
    let metadata = tokio::fs::metadata(&apk)
        .await
        .map_err(|e| format!("{path}: {e}"))?;
    if !metadata.is_file() {
        return Err(format!("{path} is not a file"));
    }
    let name = apk
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();
    let package = read_apk_package_id(&apk);
    let abis = read_apk_abis(&apk);

    let adb = state.adb_snapshot().await;
    let device_abis: Vec<String> = match adb.shell(&serial, "getprop ro.product.cpu.abilist").await
    {
        Ok(out) => out
            .stdout
            .trim()
            .split(',')
            .map(|a| a.trim().to_string())
            .filter(|a| !a.is_empty())
            .collect(),
        Err(_) => Vec::new(),
    };

    // Only claim a mismatch when both sides are known AND the APK actually
    // ships native code. An APK with no `lib/` runs on any ABI.
    let abi_compatible = if abis.is_empty() {
        Some(true)
    } else if device_abis.is_empty() {
        None
    } else {
        Some(abis.iter().any(|a| device_abis.iter().any(|d| d == a)))
    };

    let already_installed = match &package {
        Some(pkg) => match adb.shell(&serial, &format!("pm list packages {pkg}")).await {
            Ok(out) => out
                .stdout
                .lines()
                .any(|line| line.trim() == format!("package:{pkg}")),
            Err(_) => false,
        },
        None => false,
    };

    Ok(ApkInspection {
        path,
        name,
        size_bytes: metadata.len(),
        package,
        abis,
        device_abis,
        abi_compatible,
        already_installed,
    })
}

/// `list_apks_in_folder` — scan `folder` for `.apk` files. Used by the
/// Install APK UI to surface a "pick from these" list without the user
/// re-navigating the file picker. Mirrors v1's auto-discovery of `./apks/`.
///
/// Returns up to 50 entries; deeper recursion intentionally avoided.
#[tauri::command]
pub async fn list_apks_in_folder(folder: String) -> Result<Vec<DiscoveredApk>, String> {
    let dir = PathBuf::from(&folder);
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    let mut read = tokio::fs::read_dir(&dir)
        .await
        .map_err(|e| format!("read_dir {folder}: {e}"))?;
    while let Some(entry) = read.next_entry().await.transpose() {
        let entry = entry.map_err(|e| format!("read_dir entry: {e}"))?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("apk") {
            continue;
        }
        let metadata = match entry.metadata().await {
            Ok(m) => m,
            Err(_) => continue,
        };
        if !metadata.is_file() {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        let package = read_apk_package_id(&path);
        out.push(DiscoveredApk {
            path: path.display().to_string(),
            name,
            size_bytes: metadata.len(),
            package,
        });
        if out.len() >= 50 {
            break;
        }
    }
    out.sort_by_key(|a| a.name.to_lowercase());
    Ok(out)
}

/// Decode the common `INSTALL_FAILED_*` / `DELETE_FAILED_*` codes into a one-line
/// hint. Mirrors v1's `Get-UninstallErrorReason` + the inline decoder in
/// `Install-ApkFile`.
pub(crate) fn decode_install_error(text: &str) -> Option<String> {
    for (needle, hint) in [
        (
            "INSTALL_FAILED_INSUFFICIENT_STORAGE",
            "Not enough free storage on the device — free up space and retry.",
        ),
        (
            "INSTALL_FAILED_VERSION_DOWNGRADE",
            "Installed version is newer than this APK. Uninstall the device's copy first, or use a newer APK.",
        ),
        (
            "INSTALL_FAILED_ALREADY_EXISTS",
            "Same version already installed. Pass `reinstall=true` to force.",
        ),
        (
            "INSTALL_FAILED_OLDER_SDK",
            "APK requires a newer Android version than this device runs.",
        ),
        (
            "INSTALL_FAILED_NO_MATCHING_ABIS",
            "APK doesn't include a native library for this device's CPU architecture.",
        ),
        (
            "INSTALL_FAILED_INVALID_APK",
            "APK file is corrupt or malformed.",
        ),
        (
            "INSTALL_PARSE_FAILED",
            "APK couldn't be parsed (may be corrupt or not actually an APK).",
        ),
    ] {
        if text.contains(needle) {
            return Some(hint.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::decode_install_error;

    #[test]
    fn decodes_common_install_failures() {
        assert!(decode_install_error("INSTALL_FAILED_INSUFFICIENT_STORAGE")
            .unwrap()
            .contains("storage"));
        assert!(decode_install_error("INSTALL_FAILED_VERSION_DOWNGRADE")
            .unwrap()
            .contains("newer"));
        assert!(decode_install_error("INSTALL_FAILED_NO_MATCHING_ABIS")
            .unwrap()
            .contains("architecture"));
        assert!(decode_install_error("Success").is_none());
    }
}

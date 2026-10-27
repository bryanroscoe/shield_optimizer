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
    /// adb output from both streams — surfaces helpful errors like
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
    install_apk_impl(state.inner(), &serial, &apk_path, reinstall).await
}

async fn install_apk_impl(
    state: &AppState,
    serial: &str,
    apk_path: &str,
    reinstall: Option<bool>,
) -> Result<InstallApkResult, String> {
    let path_buf = PathBuf::from(apk_path);
    if !path_buf.is_file() {
        return Ok(InstallApkResult {
            ok: false,
            path: apk_path.to_string(),
            message: "APK file does not exist".to_string(),
            hint: None,
        });
    }

    let adb = state.adb_snapshot().await;
    let mut args: Vec<String> = vec!["-s".into(), serial.to_string(), "install".into()];
    if reinstall.unwrap_or(true) {
        args.push("-r".into());
    }
    args.push(apk_path.to_string());
    let args_ref: Vec<&str> = args.iter().map(String::as_str).collect();

    let out = adb
        .raw_transfer(&args_ref)
        .await
        .map_err(|e| format!("adb install: {e}"))?;
    let combined = match (out.stdout.trim().is_empty(), out.stderr.trim().is_empty()) {
        (true, _) => out.stderr.clone(),
        (_, true) => out.stdout.clone(),
        _ => format!(
            "{}{}{}",
            out.stdout,
            if out.stdout.ends_with('\n') { "" } else { "\n" },
            out.stderr
        ),
    };
    let failed = combined.lines().any(|line| {
        let line = line.trim_start().to_ascii_lowercase();
        let status = line.split([' ', '\t', ':', '[']).next().unwrap_or_default();
        matches!(status, "failure" | "error" | "failed" | "exception")
            || [
                "install_failed",
                "install_parse_failed",
                "delete_failed",
                ": failed ",
                ": error:",
                "exception:",
            ]
            .iter()
            .any(|marker| line.contains(marker))
    });
    let ok = out.success() && combined.lines().any(|line| line.trim() == "Success") && !failed;
    let hint = decode_install_error(&combined);

    Ok(InstallApkResult {
        ok,
        path: apk_path.to_string(),
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
            // The manifest is attacker-controlled, and this id is later
            // interpolated into a device-side shell command (`pm list
            // packages <id>`) before the user has confirmed anything. An id
            // that is not a well-formed package name is treated as unreadable.
            return trusted_package_id(root.get_attributes().get("package").cloned());
        }
    }
    None
}

fn trusted_package_id(raw: Option<String>) -> Option<String> {
    raw.filter(|pkg| shield_optimizer_core::engine::is_valid_package_name(pkg))
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
    /// `None` when the TV could not answer; unreadable is not "absent".
    pub already_installed: Option<bool>,
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
            // A pm "Failure"/"Exception" can still exit zero; that is an
            // unread answer, not proof the package is absent.
            Ok(out) if out.shell_reported_failure() => None,
            Ok(out) => Some(
                out.stdout
                    .lines()
                    .any(|line| line.trim() == format!("package:{pkg}")),
            ),
            Err(_) => None,
        },
        None => None,
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
    use super::{decode_install_error, trusted_package_id};

    #[test]
    fn a_manifest_package_id_with_shell_syntax_is_treated_as_unreadable() {
        let hostile = "com.example.app; pm disable-user --user 0 com.android.systemui";
        assert_eq!(trusted_package_id(Some(hostile.to_string())), None);
        assert_eq!(
            trusted_package_id(Some("com.example.app$(reboot)".into())),
            None
        );
        assert_eq!(
            trusted_package_id(Some("org.smarttube.stable".into())),
            Some("org.smarttube.stable".to_string())
        );
        assert_eq!(trusted_package_id(None), None);
    }

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

    enum Reply {
        Output(crate::adb::AdbOutput),
        Error(String),
    }

    type InstallCalls = std::sync::Arc<std::sync::Mutex<Vec<Vec<String>>>>;

    struct InstallMock {
        reply: Reply,
        calls: InstallCalls,
    }

    #[async_trait::async_trait]
    impl crate::adb::AdbDriver for InstallMock {
        async fn raw(&self, _: &[&str]) -> crate::adb::AdbResult<crate::adb::AdbOutput> {
            panic!("install must use the existing transfer wrapper, not raw commands")
        }
        async fn shell(&self, _: &str, _: &str) -> crate::adb::AdbResult<crate::adb::AdbOutput> {
            panic!("install must not replay through shell")
        }
        async fn raw_transfer(
            &self,
            args: &[&str],
        ) -> crate::adb::AdbResult<crate::adb::AdbOutput> {
            self.calls
                .lock()
                .unwrap()
                .push(args.iter().map(|arg| arg.to_string()).collect());
            match &self.reply {
                Reply::Output(output) => Ok(output.clone()),
                Reply::Error(message) => Err(crate::adb::AdbError::Transport(message.clone())),
            }
        }
    }

    fn mock_state(reply: Reply) -> (super::AppState, InstallCalls) {
        let calls = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let driver = InstallMock {
            reply,
            calls: calls.clone(),
        };
        let state = super::AppState::new(
            std::sync::Arc::new(driver),
            crate::engine::AppListBundle::default(),
            std::path::PathBuf::from("unused-mock-snapshots"),
        );
        (state, calls)
    }

    fn result(stdout: &str, stderr: &str, exit_code: Option<i32>) -> Reply {
        Reply::Output(crate::adb::AdbOutput {
            stdout: stdout.into(),
            stderr: stderr.into(),
            exit_code,
        })
    }

    fn save_install_fixture(filename: &str, value: &impl serde::Serialize) {
        if let Ok(directory) = std::env::var("NATIVE_INSTALL_RESULT_EVIDENCE") {
            let directory = std::path::PathBuf::from(directory);
            std::fs::create_dir_all(&directory).unwrap();
            std::fs::write(
                directory.join(filename),
                serde_json::to_vec_pretty(value).unwrap(),
            )
            .unwrap();
        }
    }

    #[tokio::test]
    async fn install_apk_preserves_replace_flag_and_single_transfer_attempt() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("simulated input.apk");
        std::fs::write(&file, b"simulation marker, not an executable APK").unwrap();
        let path = file.to_str().unwrap();
        for (replace, flag) in [(None, true), (Some(true), true), (Some(false), false)] {
            let (state, calls) = mock_state(result("Success\n", "", Some(0)));
            let installed = super::install_apk_impl(&state, "mock-tv", path, replace)
                .await
                .unwrap();
            assert!(installed.ok);
            assert_eq!(installed.path, path);
            let mut expected = vec!["-s", "mock-tv", "install"];
            if flag {
                expected.push("-r");
            }
            expected.push(path);
            assert_eq!(*calls.lock().unwrap(), vec![expected]);
            if replace.is_none() {
                save_install_fixture("success-result.json", &installed);
            }
        }
    }

    #[tokio::test]
    async fn install_apk_requires_authoritative_success_without_contradictory_failure() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("simulated.apk");
        std::fs::write(&file, b"simulation only, not an executable APK").unwrap();
        let cases = [
            ("Performing Streamed Install\nSuccess\n", "", Some(0), true),
            ("", "Success\n", Some(0), true),
            (
                "failure-demo.apk: 1 file pushed\nSuccess",
                "",
                Some(0),
                true,
            ),
            ("Successfully streamed bytes", "", Some(0), false),
            ("Not Success", "", Some(0), false),
            (
                "Success",
                "Failure [INSTALL_FAILED_VERSION_DOWNGRADE]",
                Some(0),
                false,
            ),
            (
                "Success\nFailure [INSTALL_FAILED_INVALID_APK]",
                "",
                Some(0),
                false,
            ),
            ("Success", "Error: install rejected", Some(0), false),
            (
                "Success",
                "adb: error: installation refused",
                Some(0),
                false,
            ),
            ("Success", "", Some(1), false),
            ("Success", "", None, false),
            ("", "", Some(0), false),
        ];
        let mut fixtures = Vec::new();
        for (index, (stdout, stderr, exit, expected)) in cases.into_iter().enumerate() {
            let (state, calls) = mock_state(result(stdout, stderr, exit));
            let installed =
                super::install_apk_impl(&state, "mock-tv", file.to_str().unwrap(), None)
                    .await
                    .unwrap();
            assert_eq!(
                installed.ok, expected,
                "stdout={stdout:?}, stderr={stderr:?}, exit={exit:?}"
            );
            assert_eq!(calls.lock().unwrap().len(), 1);
            if !stdout.is_empty() {
                assert!(installed.message.contains(stdout));
            }
            if !stderr.is_empty() {
                assert!(installed.message.contains(stderr));
            }
            if stderr.contains("INSTALL_FAILED_VERSION_DOWNGRADE") {
                assert!(installed.hint.as_deref().unwrap().contains("newer"));
            } else if stdout.contains("INSTALL_FAILED_INVALID_APK") {
                assert!(installed.hint.as_deref().unwrap().contains("corrupt"));
            } else {
                assert!(
                    installed.hint.is_none(),
                    "only known failure codes provide a backend hint"
                );
            }
            fixtures.push(serde_json::json!({ "case_index": index, "stdout": stdout, "stderr": stderr, "exit_code": exit, "result": &installed }));
        }
        save_install_fixture("command-results.json", &fixtures);
    }

    #[tokio::test]
    async fn native_indeterminate_install_error_is_preserved_without_replay() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("simulated.apk");
        std::fs::write(&file, b"simulation only").unwrap();
        let unknown = "Installation outcome unknown; verify installed apps before retrying. Commit response was lost. Success";
        let (state, calls) = mock_state(Reply::Error(unknown.into()));
        let error = super::install_apk_impl(&state, "mock-tv", file.to_str().unwrap(), Some(false))
            .await
            .err()
            .unwrap();
        assert!(error.contains(unknown));
        assert_eq!(calls.lock().unwrap().len(), 1);
        save_install_fixture(
            "unknown-result.json",
            &serde_json::json!({ "error": error }),
        );
    }

    #[tokio::test]
    async fn native_install_rejection_preserves_actionable_code_without_replay() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("simulated.apk");
        std::fs::write(&file, b"simulation only").unwrap();
        let rejected = "APK installation rejected: Failure [INSTALL_FAILED_VERSION_DOWNGRADE]";
        let (state, calls) = mock_state(Reply::Error(rejected.into()));
        let error = super::install_apk_impl(&state, "mock-tv", file.to_str().unwrap(), Some(false))
            .await
            .err()
            .unwrap();
        assert!(error.contains(rejected));
        assert_eq!(calls.lock().unwrap().len(), 1);
        save_install_fixture(
            "rejected-error.json",
            &serde_json::json!({ "error": error }),
        );
    }

    #[tokio::test]
    async fn missing_install_file_never_calls_the_driver() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("absent.apk");
        let (state, calls) = mock_state(result("Success", "", Some(0)));
        let installed = super::install_apk_impl(&state, "mock-tv", file.to_str().unwrap(), None)
            .await
            .unwrap();
        assert!(!installed.ok);
        assert!(calls.lock().unwrap().is_empty());
    }
}

//! Reading what `adb install` / `pm path` said, shared by the desktop
//! sideload, backup and clone commands and the phone's APK backups.
//!
//! Pure text in, decisions out: the commands do the transfer and hand the
//! output here.

/// Join adb's two output streams for display, skipping an empty one and
/// keeping them on separate lines.
pub fn combine_streams(stdout: &str, stderr: &str) -> String {
    match (stdout.trim().is_empty(), stderr.trim().is_empty()) {
        (true, _) => stderr.to_string(),
        (_, true) => stdout.to_string(),
        _ => format!(
            "{stdout}{}{stderr}",
            if stdout.ends_with('\n') { "" } else { "\n" }
        ),
    }
}

/// True when any line of the install output reports a failure. A transfer
/// line that merely mentions a file name (`failure-demo.apk: 1 file pushed`)
/// is not one.
fn reports_failure(output: &str) -> bool {
    output.lines().any(|line| {
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
    })
}

/// An install succeeded only when the process exited cleanly, a line reads
/// exactly `Success`, and no line reports a failure. `Successfully streamed`
/// or a `Success` followed by `Failure [...]` is not success.
pub fn install_succeeded(exited_ok: bool, output: &str) -> bool {
    exited_ok && output.lines().any(|line| line.trim() == "Success") && !reports_failure(output)
}

/// Decode the common `INSTALL_FAILED_*` / `DELETE_FAILED_*` codes into a
/// one-line hint. Mirrors v1's `Get-UninstallErrorReason` + the inline
/// decoder in `Install-ApkFile`.
pub fn decode_install_error(text: &str) -> Option<String> {
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

/// The last path segment, when there is one.
pub fn apk_basename(path: &str) -> Option<&str> {
    path.rsplit('/').next().filter(|s| !s.is_empty())
}

/// A file name that is safe to write locally and to hand back to `adb
/// install`: `[A-Za-z0-9._-]`, ending `.apk`, never `.` or `..`.
pub fn is_safe_apk_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && name.ends_with(".apk")
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// Parse `pm path <pkg>` output: one `package:<path>` line per APK (split
/// APKs report several). Lines without the prefix are noise and skipped.
///
/// The TV's answer is untrusted: every reported path must be absolute, free
/// of `..` and control characters, and end in a safe APK file name, or the
/// whole answer is refused rather than half used.
pub fn parse_pm_paths(stdout: &str) -> Result<Vec<String>, String> {
    let mut paths = Vec::new();
    for raw in stdout
        .lines()
        .filter_map(|line| line.trim().strip_prefix("package:"))
        .map(str::trim)
        .filter(|path| !path.is_empty())
    {
        if !raw.starts_with('/')
            || raw.split('/').any(|seg| seg == "..")
            || raw.chars().any(|c| c.is_control())
        {
            return Err(format!("Unsafe APK path reported by the TV: {raw:?}"));
        }
        let name = apk_basename(raw).ok_or_else(|| format!("Invalid APK path: {raw}"))?;
        if !is_safe_apk_name(name) {
            return Err(format!("Unsafe APK filename reported by the TV: {name:?}"));
        }
        paths.push(raw.to_string());
    }
    Ok(paths)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn success_needs_a_clean_exit_an_exact_line_and_no_failure() {
        let cases = [
            ("Performing Streamed Install\nSuccess\n", true, true),
            ("failure-demo.apk: 1 file pushed\nSuccess", true, true),
            ("Successfully streamed bytes", true, false),
            ("Not Success", true, false),
            (
                "Success\nFailure [INSTALL_FAILED_VERSION_DOWNGRADE]",
                true,
                false,
            ),
            ("Success\nError: install rejected", true, false),
            ("Success\nadb: error: installation refused", true, false),
            ("Success", false, false),
            ("", true, false),
        ];
        for (output, exited_ok, expected) in cases {
            assert_eq!(
                install_succeeded(exited_ok, output),
                expected,
                "{output:?} exit_ok={exited_ok}"
            );
        }
    }

    #[test]
    fn streams_are_joined_on_separate_lines() {
        assert_eq!(combine_streams("", "err\n"), "err\n");
        assert_eq!(combine_streams("out\n", ""), "out\n");
        assert_eq!(combine_streams("out", "err"), "out\nerr");
        assert_eq!(combine_streams("out\n", "err"), "out\nerr");
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

    #[test]
    fn parses_single_and_split_apk_paths() {
        assert_eq!(
            parse_pm_paths("package:/data/app/~~abc==/com.plexapp.android-xyz==/base.apk\n")
                .unwrap(),
            vec!["/data/app/~~abc==/com.plexapp.android-xyz==/base.apk".to_string()]
        );
        let paths = parse_pm_paths(
            "package:/data/app/x/base.apk\npackage:/data/app/x/split_config.en.apk\nnoise\n",
        )
        .unwrap();
        assert_eq!(paths.len(), 2);
        assert!(paths[1].ends_with("split_config.en.apk"));
    }

    #[test]
    fn noise_alone_is_no_paths() {
        assert!(parse_pm_paths("error: no devices\n").unwrap().is_empty());
        assert!(parse_pm_paths("").unwrap().is_empty());
    }

    #[test]
    fn rejects_unsafe_reported_apk_paths() {
        assert!(parse_pm_paths("package:/data/app/x/../../escape.apk\n").is_err());
        assert!(parse_pm_paths("package:/data/app/x/not-an-apk\n").is_err());
        assert!(parse_pm_paths("package:relative/base.apk\n").is_err());
        assert!(parse_pm_paths("package:/data/app/x/ba\u{7}se.apk\n").is_err());
        assert!(parse_pm_paths("package:/data/app/x/b$(id).apk\n").is_err());
        // One bad line refuses the whole answer.
        assert!(
            parse_pm_paths("package:/data/app/x/base.apk\npackage:/data/app/x/../e.apk\n").is_err()
        );
    }

    #[test]
    fn safe_apk_names() {
        assert!(is_safe_apk_name("split_config.en.apk"));
        assert!(!is_safe_apk_name(".."));
        assert!(!is_safe_apk_name("base.zip"));
        assert!(!is_safe_apk_name("a b.apk"));
    }
}

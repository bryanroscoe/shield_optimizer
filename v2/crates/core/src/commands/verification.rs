//! Checked readback for restore results. A successful shell write alone does
//! not prove that Android reached the requested state.
use crate::adb::{checked_batch_command, AdbDriver, AdbOutput, BATCH_SEPARATOR, BATCH_STATUS};

pub(super) fn succeeded(out: &AdbOutput) -> bool {
    out.success() && !out.shell_reported_failure() && !out.combined().contains("not installed for")
}

/// A failed write, for people: its output, and its exit status when that is
/// not 0 (`pm` can fail while exiting 0).
pub(super) fn failure_text(out: &AdbOutput) -> String {
    let output = out.combined();
    let output = output.trim();
    match out.exit_code {
        Some(0) => output.to_string(),
        Some(code) if output.is_empty() => format!("exit status {code}"),
        Some(code) => format!("exit status {code}: {output}"),
        None if output.is_empty() => "no exit status".to_string(),
        None => format!("no exit status: {output}"),
    }
}

/// Why a write was not confirmed.
#[derive(Debug, PartialEq)]
pub(super) enum Unconfirmed {
    /// The TV answered, and not with the requested state.
    Differs,
    /// The TV's state could not be read; the write may still have landed.
    Unreadable,
}

impl Unconfirmed {
    pub(super) fn message(&self) -> &'static str {
        match self {
            Unconfirmed::Differs => "Android did not report the requested state; recheck the TV.",
            Unconfirmed::Unreadable => {
                "Sent, but the TV's state could not be read back; recheck the TV."
            }
        }
    }
}

/// One read in a checked batch. Trusts only the batch's own exit status: the
/// body is device data (a TV named "Error Den" is not a failed read), so it
/// is never scanned for failure words here, and never echoed in an error.
pub(super) async fn checked_read(
    adb: &dyn AdbDriver,
    serial: &str,
    command: &str,
) -> Result<String, Unconfirmed> {
    let out = adb
        .shell(serial, &checked_batch_command(&[command]))
        .await
        .map_err(|_| Unconfirmed::Unreadable)?;
    if !out.success() || out.stdout.contains(BATCH_SEPARATOR) {
        return Err(Unconfirmed::Unreadable);
    }
    let (body, status) = out
        .stdout
        .rsplit_once(BATCH_STATUS)
        .ok_or(Unconfirmed::Unreadable)?;
    match status.trim().parse::<i32>() {
        Ok(0) => Ok(body.trim_matches(['\n', '\r']).to_string()),
        _ => Err(Unconfirmed::Unreadable),
    }
}

pub(super) async fn verify_package(
    adb: &dyn AdbDriver,
    serial: &str,
    package: &str,
    enabled: bool,
) -> Result<(), Unconfirmed> {
    let command = if enabled {
        format!("pm list packages -e --user 0 {package}")
    } else {
        super::list_disabled_package(package)
    };
    let text = checked_read(adb, serial, &command).await?;
    let wanted = format!("package:{package}");
    if text.lines().any(|line| line.trim() == wanted) {
        Ok(())
    } else {
        Err(Unconfirmed::Differs)
    }
}

/// A line no `settings list` prints: not `key=value`, and shaped like an
/// error. Values sit after the `=`, so they never match.
fn error_line(line: &str) -> bool {
    let line = line.trim();
    !line.contains('=')
        && (line.starts_with("Error")
            || line.starts_with("Failure")
            || line.starts_with("java.")
            || line.contains("Exception"))
}

pub(super) async fn verify_setting(
    adb: &dyn AdbDriver,
    serial: &str,
    namespace: &str,
    key: &str,
    expected: Option<&str>,
) -> Result<(), Unconfirmed> {
    // Presence-aware readback: get alone conflates absence and literal null.
    // Only the requested key is kept; other values never reach an error.
    let text = checked_read(adb, serial, &format!("settings list {namespace}")).await?;
    if text.lines().any(error_line) {
        return Err(Unconfirmed::Unreadable);
    }
    let mut found = None;
    for line in text.lines() {
        if let Some((name, value)) = line.split_once('=') {
            if name == key {
                if found.is_some() {
                    return Err(Unconfirmed::Unreadable);
                }
                found = Some(value);
            }
        }
    }
    if found == expected {
        Ok(())
    } else {
        Err(Unconfirmed::Differs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::test_support::MockAdb;

    #[test]
    fn success_requires_exit_zero_and_both_clean_streams() {
        for out in [
            AdbOutput {
                stdout: "".into(),
                stderr: "".into(),
                exit_code: Some(1),
            },
            AdbOutput {
                stdout: "ok".into(),
                stderr: "SecurityException".into(),
                exit_code: Some(0),
            },
            AdbOutput {
                stdout: "Failure".into(),
                stderr: "".into(),
                exit_code: Some(0),
            },
            AdbOutput {
                stdout: "ok".into(),
                stderr: "".into(),
                exit_code: None,
            },
        ] {
            assert!(!succeeded(&out));
        }
    }

    #[tokio::test]
    async fn setting_readback_distinguishes_absent_empty_and_literal_null() {
        for (body, expected, matches) in [
            ("other=1", None, true),
            ("key=", Some(""), true),
            ("key=null", Some("null"), true),
            ("key=", None, false),
            ("other=1", Some(""), false),
            ("key=1\nkey=1", Some("1"), false),
            ("key=2", Some("1"), false),
        ] {
            let mock =
                MockAdb::default().on_shell("settings list", &format!("{body}\n{BATCH_STATUS}0\n"));
            assert_eq!(
                verify_setting(&mock, "TV", "secure", "key", expected)
                    .await
                    .is_ok(),
                matches
            );
        }
        let truncated = MockAdb::default().on_shell("settings list", "key=1");
        assert!(verify_setting(&truncated, "TV", "secure", "key", Some("1"))
            .await
            .is_err());
    }
    #[tokio::test]
    async fn failed_partial_setting_list_never_exposes_values() {
        let mock = MockAdb::default().on_shell_output(
            "settings list",
            AdbOutput {
                stdout: "unrelated=SYNTHETIC_PRIVATE_TOKEN\n".into(),
                stderr: "Error: read interrupted".into(),
                exit_code: Some(1),
            },
        );
        let error = verify_setting(&mock, "TV", "secure", "key", Some("1"))
            .await
            .unwrap_err();
        assert_eq!(error, Unconfirmed::Unreadable);
        assert!(!error.message().contains("SYNTHETIC_PRIVATE_TOKEN"));
        let mock = MockAdb::default().on_shell_err("settings list", "SYNTHETIC_PRIVATE_TOKEN");
        let error = verify_setting(&mock, "TV", "secure", "key", Some("1"))
            .await
            .unwrap_err();
        assert_eq!(error, Unconfirmed::Unreadable);
    }

    #[tokio::test]
    async fn failure_words_in_other_settings_values_do_not_fail_the_readback() {
        // A TV named "Error Prone Den", or a component called
        // ExceptionHandlerService, is data, not a failed read.
        let body = "device_name=Error Prone Den\nhandler=com.x/.ExceptionHandlerService\nkey=1";
        let mock =
            MockAdb::default().on_shell("settings list", &format!("{body}\n{BATCH_STATUS}0\n"));
        assert_eq!(
            verify_setting(&mock, "TV", "global", "key", Some("1")).await,
            Ok(())
        );
        // An error printed in place of the list is not "key absent".
        for body in [
            "Error: no such namespace",
            "java.lang.SecurityException: denied",
        ] {
            let mock =
                MockAdb::default().on_shell("settings list", &format!("{body}\n{BATCH_STATUS}0\n"));
            assert_eq!(
                verify_setting(&mock, "TV", "global", "key", None).await,
                Err(Unconfirmed::Unreadable)
            );
        }
        let failed = MockAdb::default().on_shell("settings list", &format!("{BATCH_STATUS}255\n"));
        assert_eq!(
            verify_setting(&failed, "TV", "global", "key", None).await,
            Err(Unconfirmed::Unreadable)
        );
    }

    #[tokio::test]
    async fn package_readback_separates_a_different_state_from_an_unreadable_one() {
        let still_on =
            MockAdb::default().on_shell("pm list packages -d", &format!("\n{BATCH_STATUS}0\n"));
        assert_eq!(
            verify_package(&still_on, "TV", "com.example.app", false).await,
            Err(Unconfirmed::Differs)
        );
        let prefix_only = MockAdb::default().on_shell(
            "pm list packages -d",
            &format!("package:com.example.app.extra\n{BATCH_STATUS}0\n"),
        );
        assert_eq!(
            verify_package(&prefix_only, "TV", "com.example.app", false).await,
            Err(Unconfirmed::Differs)
        );
        let offline = MockAdb::default().on_shell_err("pm list packages -d", "device offline");
        assert_eq!(
            verify_package(&offline, "TV", "com.example.app", false).await,
            Err(Unconfirmed::Unreadable)
        );
        let truncated =
            MockAdb::default().on_shell("pm list packages -d", "package:com.example.app\n");
        assert_eq!(
            verify_package(&truncated, "TV", "com.example.app", false).await,
            Err(Unconfirmed::Unreadable)
        );
    }

    #[test]
    fn failure_text_names_the_exit_status_only_when_it_is_not_zero() {
        let out = |stdout: &str, exit_code| AdbOutput {
            stdout: stdout.into(),
            stderr: String::new(),
            exit_code,
        };
        assert_eq!(failure_text(&out("Failure [x]", Some(0))), "Failure [x]");
        assert_eq!(failure_text(&out("", Some(1))), "exit status 1");
        assert_eq!(
            failure_text(&out("denied\n", Some(255))),
            "exit status 255: denied"
        );
        assert_eq!(failure_text(&out("", None)), "no exit status");
    }
}

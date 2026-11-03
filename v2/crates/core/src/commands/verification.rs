//! Checked readback for restore results. A successful shell write alone does
//! not prove that Android reached the requested state.
use crate::adb::{checked_batch_command, parse_checked_batch, AdbDriver, AdbOutput};

pub(super) fn succeeded(out: &AdbOutput) -> bool {
    out.success() && !out.shell_reported_failure() && !out.combined().contains("not installed for")
}

pub(super) async fn checked_read(
    adb: &dyn AdbDriver,
    serial: &str,
    command: &str,
) -> Result<String, String> {
    let out = adb
        .shell(serial, &checked_batch_command(&[command]))
        .await
        .map_err(|_| "Readback failed; recheck the TV.".to_string())?;
    if !succeeded(&out) {
        // Do not return settings dumps (or their values) in error messages.
        return Err(format!(
            "Read failed (exit {:?}); recheck the TV.",
            out.exit_code
        ));
    }
    Ok(parse_checked_batch(&out.stdout, 1, &[0])?.remove(0))
}

pub(super) async fn verify_package(
    adb: &dyn AdbDriver,
    serial: &str,
    package: &str,
    enabled: bool,
) -> Result<(), String> {
    let flag = if enabled { "-e" } else { "-d" };
    let text = checked_read(
        adb,
        serial,
        &format!("pm list packages {flag} --user 0 {package}"),
    )
    .await?;
    if text
        .lines()
        .any(|line| line.trim() == format!("package:{package}"))
    {
        Ok(())
    } else {
        Err("Package state was not confirmed for user 0; recheck the TV.".into())
    }
}

pub(super) async fn verify_setting(
    adb: &dyn AdbDriver,
    serial: &str,
    namespace: &str,
    key: &str,
    expected: Option<&str>,
) -> Result<(), String> {
    // Presence-aware readback: get alone conflates absence and literal null.
    // Only retain the requested key; the settings list is never logged or saved.
    let text = checked_read(adb, serial, &format!("settings list {namespace}")).await?;
    let mut found = None;
    for line in text.lines() {
        if let Some((name, value)) = line.split_once('=') {
            if name == key {
                if found.is_some() {
                    return Err("Duplicate setting in readback.".into());
                }
                found = Some(value);
            }
        }
    }
    if found == expected {
        Ok(())
    } else {
        Err("Setting readback did not match the requested state; recheck the TV.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adb::batch::BATCH_STATUS;
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
        assert!(!error.contains("SYNTHETIC_PRIVATE_TOKEN"));
        assert!(!error.contains("unrelated"));
        let mock = MockAdb::default().on_shell_err("settings list", "SYNTHETIC_PRIVATE_TOKEN");
        let error = verify_setting(&mock, "TV", "secure", "key", Some("1"))
            .await
            .unwrap_err();
        assert!(!error.contains("SYNTHETIC_PRIVATE_TOKEN"));
    }
}

//! Batched device-side shell invocation.
//!
//! Several read paths used to fan out N separate `adb shell` calls where one
//! compound shell would do. On the desktop driver each call is a process
//! spawn; on the mobile driver every call is serialized behind a single TCP
//! connection, so the fan-out costs N × Wi-Fi RTT for zero concurrency.
//! `batch_command` folds them into one invocation whose stdout `split_batch`
//! cuts back apart on a sentinel.

/// Marker echoed between sub-commands. Deliberately long and prefixed so it
/// cannot collide with real `dumpsys` / `pm` / `df` output.
pub const BATCH_SEPARATOR: &str = "__SHIELDOPT_SEP__";
pub const BATCH_STATUS: &str = "__SHIELDOPT_STATUS__";

pub fn checked_batch_command(cmds: &[&str]) -> String {
    cmds.iter()
        .map(|cmd| format!("({cmd}) 2>/dev/null; printf '\\n{BATCH_STATUS}%s\\n' $?"))
        .collect::<Vec<_>>()
        .join(&format!("; echo {BATCH_SEPARATOR}; "))
}

/// Required reads must distinguish an empty successful list from a failed
/// command. Optional reads still need a completion marker to detect truncation.
pub fn parse_checked_batch(
    output: &str,
    n: usize,
    required: &[usize],
) -> Result<Vec<String>, String> {
    let raw: Vec<_> = output.split(BATCH_SEPARATOR).collect();
    if raw.len() != n {
        return Err("Incomplete device response. Retry the read.".into());
    }
    raw.iter()
        .enumerate()
        .map(|(i, section)| {
            let (body, status) = section.rsplit_once(BATCH_STATUS).ok_or_else(|| {
                format!("Device read {} did not complete. Retry the read.", i + 1)
            })?;
            let status: i32 = status
                .trim()
                .parse()
                .map_err(|_| format!("Invalid status for device read {}.", i + 1))?;
            if status != 0 && required.contains(&i) {
                return Err(format!(
                    "Device read {} failed (exit {status}). Retry the read.",
                    i + 1
                ));
            }
            if required.contains(&i)
                && body.lines().any(|line| {
                    let line = line.trim();
                    line.starts_with("Error")
                        || line.starts_with("Failure")
                        || line.contains("Exception")
                })
            {
                return Err(format!(
                    "Device read {} reported an error. Retry the read.",
                    i + 1
                ));
            }
            if status != 0 {
                return Ok(String::new());
            }
            Ok(body.trim_matches(['\n', '\r']).to_string())
        })
        .collect()
}

/// Join `cmds` into one command string for `AdbDriver::shell`.
///
/// Sub-commands are chained with `;` (never `&&`) so every one runs even when
/// an earlier one fails, and each has its stderr discarded. Every parser in
/// the tree reads only `AdbOutput::stdout`, so dropping stderr leaves desktop
/// behavior byte-identical while keeping the sections parseable on the mobile
/// transport, which merges stdout and stderr into one stream.
///
/// No double quotes are emitted, so the result survives being passed as a
/// single `adb -s <serial> shell <cmd>` argument the same way the compound
/// commands already in the tree (`devices.rs::harvest_properties`) do.
pub fn batch_command(cmds: &[&str]) -> String {
    let mut out = String::new();
    for (i, cmd) in cmds.iter().enumerate() {
        if i > 0 {
            out.push_str("; echo ");
            out.push_str(BATCH_SEPARATOR);
            out.push_str("; ");
        }
        out.push_str(cmd);
        out.push_str(" 2>/dev/null");
    }
    out
}

/// Cut batched `output` back into exactly `n` sections, in command order.
///
/// Always returns `n` entries: output truncated mid-batch (a killed shell, a
/// transport read cap) pads the tail with empty strings, so a missing section
/// parses to the same default it would have from a failed individual call.
/// A trailing sentinel and the newlines around each sentinel are absorbed;
/// interior indentation is left untouched because the `dumpsys` parsers rely
/// on it.
pub fn split_batch(output: &str, n: usize) -> Vec<String> {
    let mut sections: Vec<String> = output
        .split(BATCH_SEPARATOR)
        .map(|s| s.trim_matches(|c| c == '\n' || c == '\r').to_string())
        .take(n)
        .collect();
    sections.resize(n, String::new());
    sections
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_reads_reject_failed_or_missing_required_sections() {
        let output =
            format!("package:com.example\n{BATCH_STATUS}0\n{BATCH_SEPARATOR}\n{BATCH_STATUS}1\n");
        assert!(parse_checked_batch(&output, 2, &[0, 1]).is_err());
        let empty_success =
            output.replace(&format!("{BATCH_STATUS}1"), &format!("{BATCH_STATUS}0"));
        assert_eq!(
            parse_checked_batch(&empty_success, 2, &[0, 1]).unwrap()[1],
            ""
        );
        assert!(parse_checked_batch("package:com.example", 2, &[0, 1]).is_err());
        assert!(
            parse_checked_batch(&format!("{BATCH_STATUS}0\n{BATCH_SEPARATOR}"), 2, &[0, 1])
                .is_err()
        );
        assert_eq!(parse_checked_batch(&output, 2, &[0]).unwrap()[1], "");
    }

    #[cfg(unix)]
    #[test]
    fn checked_batch_records_each_real_shell_exit_even_if_the_last_succeeds() {
        let cmd = checked_batch_command(&["printf packages", "exit 7", "printf memory"]);
        let output = std::process::Command::new("sh")
            .args(["-c", &cmd])
            .output()
            .unwrap();
        assert!(output.status.success());
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(parse_checked_batch(&stdout, 3, &[0, 1])
            .unwrap_err()
            .contains("exit 7"));
        assert_eq!(
            parse_checked_batch(&stdout, 3, &[0, 2]).unwrap(),
            ["packages", "", "memory"]
        );
    }

    #[test]
    fn joins_with_semicolons_and_discards_stderr() {
        let cmd = batch_command(&["pm list packages", "dumpsys meminfo"]);
        assert_eq!(
            cmd,
            "pm list packages 2>/dev/null; echo __SHIELDOPT_SEP__; dumpsys meminfo 2>/dev/null"
        );
        // `&&` would skip the rest of the batch after one failure.
        assert!(!cmd.contains("&&"));
        // Embedded double quotes would need re-quoting through `adb shell`.
        assert!(!cmd.contains('"'));
    }

    #[test]
    fn single_command_gets_no_separator() {
        assert_eq!(batch_command(&["df -h /data"]), "df -h /data 2>/dev/null");
    }

    #[test]
    fn empty_batch_is_empty() {
        assert_eq!(batch_command(&[]), "");
        assert!(split_batch("", 0).is_empty());
    }

    #[test]
    fn splits_into_exactly_n_sections() {
        let out = "one\n__SHIELDOPT_SEP__\ntwo\n__SHIELDOPT_SEP__\nthree\n";
        assert_eq!(split_batch(out, 3), vec!["one", "two", "three"]);
    }

    #[test]
    fn truncated_output_pads_with_empty_sections() {
        // Shell died after the second section.
        let out = "one\n__SHIELDOPT_SEP__\ntwo";
        assert_eq!(split_batch(out, 4), vec!["one", "two", "", ""]);
        // Nothing came back at all.
        assert_eq!(split_batch("", 3), vec!["", "", ""]);
    }

    #[test]
    fn empty_sections_stay_empty_and_keep_position() {
        let out = "\n__SHIELDOPT_SEP__\nmiddle\n__SHIELDOPT_SEP__\n";
        assert_eq!(split_batch(out, 3), vec!["", "middle", ""]);
    }

    #[test]
    fn tolerates_sentinel_at_the_very_end() {
        // A trailing `echo` sentinel (extra separator) must not add a section
        // or displace the real ones.
        let out = "one\n__SHIELDOPT_SEP__\ntwo\n__SHIELDOPT_SEP__\n";
        assert_eq!(split_batch(out, 2), vec!["one", "two"]);
        assert_eq!(split_batch("only\n__SHIELDOPT_SEP__", 1), vec!["only"]);
    }

    #[test]
    fn preserves_interior_indentation_and_blank_lines() {
        let out = "  mSupportedHdrTypes=[1]\n\n  modeId 2\n__SHIELDOPT_SEP__\nb";
        let sections = split_batch(out, 2);
        assert_eq!(sections[0], "  mSupportedHdrTypes=[1]\n\n  modeId 2");
        assert_eq!(sections[1], "b");
    }

    #[test]
    fn extra_sections_beyond_n_are_dropped() {
        let out = "a\n__SHIELDOPT_SEP__\nb\n__SHIELDOPT_SEP__\nc";
        assert_eq!(split_batch(out, 2), vec!["a", "b"]);
    }

    #[test]
    fn round_trips_a_realistic_batch() {
        let cmd = batch_command(&["df -h /data", "cat /proc/meminfo"]);
        assert_eq!(cmd.matches(BATCH_SEPARATOR).count(), 1);
        let device_output = format!(
            "Filesystem Size Used Avail Use% Mounted on\n\
             /dev/block/dm-5 11G 8.4G 2.4G 78% /data\n\
             {BATCH_SEPARATOR}\n\
             MemTotal:        3000000 kB\n"
        );
        let sections = split_batch(&device_output, 2);
        assert!(sections[0].contains("78%"));
        assert!(sections[1].starts_with("MemTotal:"));
    }
}

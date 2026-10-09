use serde::Serialize;
use tauri::State;

use crate::adb::driver::{BoundedShellOutput, ShellTermination};
use crate::engine::logcat::{parse_main_pid, LogcatOptions};

use super::{quote_shell_arg, AppState};

#[derive(Debug, Serialize)]
pub struct DeviceLogs {
    pub output: BoundedShellOutput,
    pub package: Option<String>,
    pub pid: Option<u32>,
}

#[tauri::command]
pub async fn read_device_logs(
    state: State<'_, AppState>,
    serial: String,
    options: LogcatOptions,
) -> Result<DeviceLogs, String> {
    read_device_logs_impl(state.inner(), &serial, options).await
}

async fn read_device_logs_impl(
    state: &AppState,
    serial: &str,
    options: LogcatOptions,
) -> Result<DeviceLogs, String> {
    options.validate()?;
    let adb = state.adb_snapshot().await;
    let pid = if let Some(package) = &options.package {
        let output = adb
            .shell_bounded(serial, &format!("pidof {}", quote_shell_arg(package)))
            .await
            .map_err(|e| format!("Could not resolve the app's main process: {e}"))?;
        if output.termination != ShellTermination::Completed || output.exit_code != Some(0) {
            return Err("Could not resolve the app's main process. It may be stopped, or this TV may not support pidof. No unfiltered logs were requested.".into());
        }
        Some(parse_main_pid(&output.stdout)?)
    } else {
        None
    };
    let command = options.command(pid)?;
    // Bounded execution redacts command output from the app's debug/report logs.
    let output = adb
        .shell_bounded(serial, &command)
        .await
        .map_err(|e| format!("Could not read device logs: {e}"))?;
    Ok(DeviceLogs {
        output,
        package: options.package,
        pid,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::test_support::{state_with, MockAdb};
    use crate::engine::logcat::LogPriority;

    fn options(package: Option<&str>) -> LogcatOptions {
        LogcatOptions {
            priority: LogPriority::Warning,
            lines: 100,
            tag: None,
            package: package.map(str::to_string),
        }
    }

    #[tokio::test]
    async fn app_capture_resolves_the_main_pid_each_time() {
        let mock = MockAdb::default()
            .on_shell("pidof", "123\n")
            .on_shell("logcat", "one log line\n");
        let calls = mock.shell_log();
        let result =
            read_device_logs_impl(&state_with(mock), "tv", options(Some("com.example.app")))
                .await
                .unwrap();
        assert_eq!(result.pid, Some(123));
        assert_eq!(result.output.stdout, "one log line\n");
        assert_eq!(
            calls.lock().unwrap().as_slice(),
            [
                "pidof 'com.example.app'",
                "logcat -d -v threadtime -t 100 --pid=123 '*:W'"
            ]
        );
    }

    #[tokio::test]
    async fn failed_or_ambiguous_pid_never_reads_all_logs() {
        for reply in ["", "1 2", "pidof: not found"] {
            let mock = MockAdb::default().on_shell("pidof", reply);
            let calls = mock.shell_log();
            assert!(read_device_logs_impl(
                &state_with(mock),
                "tv",
                options(Some("com.example.app"))
            )
            .await
            .is_err());
            assert_eq!(calls.lock().unwrap().len(), 1);
        }
    }

    #[tokio::test]
    async fn unsuccessful_pid_command_never_uses_its_stdout() {
        let mock = MockAdb::default().on_shell_exit("pidof", "123", 1);
        let calls = mock.shell_log();
        assert!(
            read_device_logs_impl(&state_with(mock), "tv", options(Some("com.example.app")))
                .await
                .is_err()
        );
        assert_eq!(calls.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn invalid_filter_never_touches_the_driver() {
        let mock = MockAdb::default();
        let calls = mock.shell_log();
        assert!(
            read_device_logs_impl(&state_with(mock), "tv", options(Some("com.example;id")))
                .await
                .is_err()
        );
        assert!(calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn device_failure_is_not_an_empty_success() {
        let mock = MockAdb::default().on_shell_err("logcat", "device offline");
        assert!(
            read_device_logs_impl(&state_with(mock), "tv", options(None))
                .await
                .unwrap_err()
                .contains("device offline")
        );
    }
}

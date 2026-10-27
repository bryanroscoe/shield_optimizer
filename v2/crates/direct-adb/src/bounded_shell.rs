use std::time::Duration;

use adb_client::tcp::{
    run_identity_bound_shell, BoundedShellControl, BoundedShellTermination, SHELL_OUTPUT_LIMIT,
};
use shield_optimizer_core::adb::driver::{BoundedShellOutput, ShellTermination};
use shield_optimizer_core::adb::{AdbError, AdbResult, Lease};
use std::sync::Arc;

use super::{validate_requested_serial, WirelessAdb};

const SHELL_DEADLINE: Duration = Duration::from_secs(30);

pub(crate) struct CancelOnDrop(pub(crate) BoundedShellControl);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

impl WirelessAdb {
    pub(crate) async fn bounded_shell_impl(
        &self,
        serial: &str,
        command: &str,
    ) -> AdbResult<BoundedShellOutput> {
        let Some((active, host, port)) = self.info() else {
            return Err(AdbError::Transport("Not connected to a device.".into()));
        };
        validate_requested_serial(&active, serial).map_err(AdbError::Transport)?;
        let address = format!("{host}:{port}")
            .parse()
            .map_err(|e| AdbError::Transport(format!("invalid device address: {e}")))?;
        let operation = self.work().map_err(AdbError::Transport)?;
        let deadline = tokio::time::Instant::now() + SHELL_DEADLINE;
        let expected = self
            .verified_owned_hardware_id(serial, Duration::from_secs(5))
            .await?;
        let control = BoundedShellControl::new(
            deadline.saturating_duration_since(tokio::time::Instant::now()),
        )?;
        let command = command.to_owned();
        let key = self.key_path.clone();
        run_worker(operation, control, move |control| {
            let output = run_identity_bound_shell(address, &key, &command, &expected, control)
                .map_err(|e| AdbError::Transport(e.to_string()))?;
            Ok(BoundedShellOutput {
                stdout: bounded_utf8(&output.stdout),
                stderr: bounded_utf8(&output.stderr),
                exit_code: output.exit_code.map(i32::from),
                termination: match output.termination {
                    BoundedShellTermination::Completed => ShellTermination::Completed,
                    BoundedShellTermination::OutputLimit => ShellTermination::OutputLimit,
                    BoundedShellTermination::Timeout => ShellTermination::Timeout,
                },
            })
        })
        .await
    }
}

pub(crate) async fn run_worker<T, F>(
    operation: Arc<Lease>,
    control: BoundedShellControl,
    run: F,
) -> AdbResult<T>
where
    T: Send + 'static,
    F: FnOnce(BoundedShellControl) -> AdbResult<T> + Send + 'static,
{
    let _cancel = CancelOnDrop(control.clone());
    tokio::task::spawn_blocking(move || {
        let _operation = operation;
        run(control)
    })
    .await
    .map_err(|e| AdbError::Transport(format!("bounded shell worker failed: {e}")))?
}

fn bounded_utf8(bytes: &[u8]) -> String {
    let mut text = String::from_utf8_lossy(bytes).into_owned();
    let mut end = text.len().min(SHELL_OUTPUT_LIMIT);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replacement_characters_cannot_expand_output_past_the_limit() {
        let text = bounded_utf8(&vec![0xff; SHELL_OUTPUT_LIMIT]);
        assert!(text.len() <= SHELL_OUTPUT_LIMIT);
        assert!(text.chars().all(|c| c == '\u{fffd}'));
    }

    #[tokio::test]
    async fn canceled_worker_keeps_lease_until_cleanup_finishes() {
        let operations = shield_optimizer_core::adb::Leases::new();
        let operation = Arc::new(operations.try_acquire().unwrap());
        let control = BoundedShellControl::new(Duration::from_secs(2)).unwrap();
        let (started, started_rx) = tokio::sync::oneshot::channel();
        let (release, release_rx) = std::sync::mpsc::channel();
        let task = tokio::spawn(run_worker(operation, control, move |control| {
            // Required-key failure finishes the watchdog without any network I/O.
            let key = tempfile::tempdir().unwrap();
            assert!(adb_client::tcp::run_bounded_shell(
                "127.0.0.1:0".parse().unwrap(),
                &key.path().join("missing"),
                "fixture",
                control
            )
            .is_err());
            started.send(()).unwrap();
            release_rx.recv().unwrap();
            Err::<BoundedShellOutput, _>(AdbError::Transport("fixture cleanup complete".into()))
        }));
        started_rx.await.unwrap();
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        assert_eq!(operations.active(), 1);
        operations.close();
        assert!(operations.try_acquire().is_none());
        release.send(()).unwrap();
        assert!(
            operations
                .wait_idle(tokio::time::Instant::now() + Duration::from_secs(2))
                .await
        );
    }

    #[tokio::test]
    async fn bounded_shell_rejects_wrong_target_and_closed_admission_before_work() {
        let operations = shield_optimizer_core::adb::Leases::new();
        let temp = tempfile::tempdir().unwrap();
        let missing = temp.path().join("never-created");
        let driver = WirelessAdb::with_existing_key(missing.clone(), operations.clone());
        *super::super::lock_recovering(&driver.identity) = Some(super::super::Identity {
            serial: "127.0.0.1:1".into(),
            host: "127.0.0.1".into(),
            port: 1,
        });
        assert!(driver
            .bounded_shell_impl("127.0.0.1:2", "fixture")
            .await
            .unwrap_err()
            .to_string()
            .contains("not 127.0.0.1:2"));
        operations.close();
        assert!(driver
            .bounded_shell_impl("127.0.0.1:1", "fixture")
            .await
            .unwrap_err()
            .to_string()
            .contains("quitting"));
        assert!(!missing.exists());
        assert_eq!(operations.active(), 0);
    }
}

use super::{bounded_shell::run_worker, validate_requested_serial, WirelessAdb};
use adb_client::tcp::{run_streamed_install, BoundedShellControl, InstallStatus};
use shield_optimizer_core::adb::{AdbError, AdbOutput, AdbResult};
use std::{fs::File, path::Path, time::Duration};

const INSTALL_DEADLINE: Duration = Duration::from_secs(15 * 60);

impl WirelessAdb {
    pub(crate) async fn verified_owned_hardware_id(
        &self,
        serial: &str,
        budget: Duration,
    ) -> AdbResult<String> {
        let probe_control = BoundedShellControl::new(budget)?;
        let _cancel_probe = super::bounded_shell::CancelOnDrop(probe_control.clone());
        let raw_id = tokio::time::timeout(
            budget,
            self.on_device(serial, move |device| {
                // Identity refusal must not trigger on_device's post-error probe.
                Ok(device.hardware_serial_bounded(probe_control))
            }),
        )
        .await
        .map_err(|_| {
            AdbError::Transport(
                "Selected TV hardware identity query timed out; no new operation was started."
                    .into(),
            )
        })?
        .map_err(AdbError::Transport)?
        .map_err(|e| AdbError::Transport(e.to_string()))?;
        let expected_hardware_id =
            shield_optimizer_core::commands::devices::verified_hardware_id(&raw_id)
                .ok_or_else(|| {
                    AdbError::Transport(
                "Selected TV has no verified hardware identity; no new operation was started."
                    .into(),
            )
                })?
                .to_owned();
        Ok(expected_hardware_id)
    }

    pub(crate) async fn streamed_install_impl(
        &self,
        serial: &str,
        local: &str,
        replace: bool,
    ) -> AdbResult<AdbOutput> {
        if local.is_empty() || local.starts_with('-') {
            return Err(AdbError::Unsupported {
                operation: "install_flags",
            });
        }
        let Some((active, host, port)) = self.info() else {
            return Err(AdbError::Transport("Not connected to a device.".into()));
        };
        validate_requested_serial(&active, serial).map_err(AdbError::Transport)?;
        let address = format!("{host}:{port}")
            .parse()
            .map_err(|e| AdbError::Transport(format!("invalid device address: {e}")))?;
        let deadline = tokio::time::Instant::now() + INSTALL_DEADLINE;
        let path = Path::new(local);
        if path
            .extension()
            .is_none_or(|extension| !extension.eq_ignore_ascii_case("apk"))
            || !std::fs::metadata(path)?.is_file()
        {
            return Err(AdbError::Transport("Select a regular .apk file.".into()));
        }
        let mut file = File::open(path)?;
        let metadata = file.metadata()?;
        if !metadata.is_file() || metadata.len() == 0 {
            return Err(AdbError::Transport(
                "Select a non-empty regular .apk file.".into(),
            ));
        }
        let size = metadata.len();
        let expected_hardware_id = self
            .verified_owned_hardware_id(
                serial,
                deadline
                    .saturating_duration_since(tokio::time::Instant::now())
                    .min(Duration::from_secs(5)),
            )
            .await?;
        let operation = self.work().map_err(AdbError::Transport)?;
        let control = BoundedShellControl::new(
            deadline.saturating_duration_since(tokio::time::Instant::now()),
        )?;
        let key = self.key_path.clone();
        run_worker(operation, control, move |control| {
            let outcome = run_streamed_install(
                address,
                &key,
                &mut file,
                size,
                replace,
                &expected_hardware_id,
                control,
            )
            .map_err(|e| AdbError::Transport(format!("Could not start APK installation: {e}")))?;
            match outcome.status {
                InstallStatus::Success => Ok(AdbOutput {
                    stdout: String::from_utf8_lossy(&outcome.response).into_owned(),
                    stderr: String::new(),
                    exit_code: Some(0),
                }),
                InstallStatus::Rejected => Err(AdbError::Transport(format!(
                    "APK installation rejected: {}",
                    String::from_utf8_lossy(&outcome.response).trim_end()
                ))),
                InstallStatus::Indeterminate => Err(AdbError::Transport(format!(
                    "Installation outcome unknown; verify installed apps before retrying. {} {}",
                    outcome.detail,
                    String::from_utf8_lossy(&outcome.response).trim_end()
                ))),
            }
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shield_optimizer_core::adb::{AdbDriver, Leases};
    use std::sync::Arc;

    #[tokio::test]
    async fn native_install_rejects_flags_and_multiple_apks_before_connection_work() {
        let operations = Leases::new();
        let driver = WirelessAdb::with_existing_key("never-created-key".into(), operations.clone());
        for args in [
            vec!["-s", "tv", "install", "-g"],
            vec!["-s", "tv", "install", ""],
            vec!["-s", "tv", "install-multiple", "a.apk", "b.apk"],
            vec!["-s", "tv", "install", "-t", "a.apk"],
        ] {
            assert!(matches!(
                driver.raw_transfer(&args).await,
                Err(AdbError::Unsupported { .. })
            ));
        }
        assert_eq!(operations.active(), 0);
    }

    #[tokio::test]
    async fn canceled_install_worker_retains_lease_until_cleanup_finishes() {
        let operations = Leases::new();
        let operation = Arc::new(operations.try_acquire().unwrap());
        let control = BoundedShellControl::new(Duration::from_secs(2)).unwrap();
        let (started, started_rx) = tokio::sync::oneshot::channel();
        let (release, release_rx) = std::sync::mpsc::channel();
        let task = tokio::spawn(run_worker(operation, control, move |control| {
            let mut input = std::io::Cursor::new(b"abc");
            assert!(run_streamed_install(
                "127.0.0.1:0".parse().unwrap(),
                Path::new("missing-install-fixture-key-never-created"),
                &mut input,
                3,
                false,
                "fixture-hardware",
                control
            )
            .is_err());
            started.send(()).unwrap();
            release_rx.recv().unwrap();
            Err::<AdbOutput, _>(AdbError::Transport("fixture cleanup complete".into()))
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
    async fn queued_identity_probe_times_out_without_detaching_its_worker_lease() {
        let operations = Leases::new();
        let driver = Arc::new(WirelessAdb::with_existing_key(
            "never-created-key".into(),
            operations.clone(),
        ));
        let conn = driver.conn.clone();
        let (held, held_rx) = tokio::sync::oneshot::channel();
        let (release, release_rx) = std::sync::mpsc::channel();
        let holder = std::thread::spawn(move || {
            let _guard = super::super::lock_recovering(&conn);
            held.send(()).unwrap();
            release_rx.recv().unwrap();
        });
        held_rx.await.unwrap();
        assert!(driver
            .verified_owned_hardware_id("fixture", Duration::from_millis(20))
            .await
            .is_err());
        assert_eq!(operations.active(), 1);
        operations.close();
        release.send(()).unwrap();
        assert!(
            operations
                .wait_idle(tokio::time::Instant::now() + Duration::from_secs(2))
                .await
        );
        holder.join().unwrap();
    }
}

use super::{bounded_shell::run_worker, validate_requested_serial, WirelessAdb};
use adb_client::tcp::{ADBTcpDevice, BoundedShellControl};
use adb_client::RustADBError;
use atv_optimizer_core::adb::{AdbError, AdbOutput, AdbResult};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

const INSTALL_DEADLINE: Duration = Duration::from_secs(15 * 60);
/// Bound on each wait for the package manager, including verification after
/// the last byte; the total deadline still caps the whole install.
const INSTALL_WAIT_TIMEOUT: Duration = Duration::from_secs(180);

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
                // Identity refusal must not trigger on_device's post-error
                // probe, but a dead or desynchronized socket is still evicted.
                match device.hardware_serial_bounded(probe_control) {
                    Err(error) if super::is_fatal_transport_error(&error) => Err(error),
                    other => Ok(other),
                }
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
            atv_optimizer_core::commands::devices::verified_hardware_id(&raw_id)
                .ok_or_else(|| {
                    AdbError::Transport(
                "Selected TV has no verified hardware identity; no new operation was started."
                    .into(),
            )
                })?
                .to_owned();
        Ok(expected_hardware_id)
    }

    /// `install [-r] <apk>` and `install-multiple [-r] <apk>…`, the forms the
    /// apps emit. The APKs stream to the package manager over a dedicated
    /// connection that must report the selected TV's verified hardware id
    /// before any byte is sent, under one cancellable total deadline, so a
    /// failed install never evicts the selected TV's session. The vendored
    /// `install_apks` picks AOSP's channel (`exec:cmd`, an `abb_exec`/`cmd`
    /// install session for splits, or a `pm` fallback without `cmd`).
    pub(crate) async fn install_impl(
        &self,
        serial: &str,
        verb: &str,
        rest: &[&str],
    ) -> AdbResult<AdbOutput> {
        let (replace, apks) = split_install_args(verb, rest)?;
        // A local storage error must not reach (or evict) a healthy TV
        // connection, so every file is checked before any TV work.
        for apk in &apks {
            check_local_apk(apk)?;
        }
        let Some((active, host, port)) = self.info() else {
            return Err(AdbError::Transport("Not connected to a device.".into()));
        };
        validate_requested_serial(&active, serial).map_err(AdbError::Transport)?;
        let address = format!("{host}:{port}")
            .parse()
            .map_err(|e| AdbError::Transport(format!("invalid device address: {e}")))?;
        let deadline = tokio::time::Instant::now() + INSTALL_DEADLINE;
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
        let preferred = self.preferred_keys.clone();
        run_worker(operation, control, move |control| {
            let mut device = ADBTcpDevice::new_with_required_keys_and_control(
                address, &key, &preferred, &control,
            )
            .map_err(|e| AdbError::Transport(format!("Could not start APK installation: {e}")))?;
            let probe = BoundedShellControl::new(control.remaining()?.min(Duration::from_secs(5)))?;
            device
                .verify_hardware_binding_bounded(&expected_hardware_id, probe)
                .map_err(|e| {
                    AdbError::Transport(format!(
                        "Install identity check failed before any APK was sent: {e}"
                    ))
                })?;
            let options: &[&str] = if replace { &["-r"] } else { &[] };
            let timeout = control.remaining()?.min(INSTALL_WAIT_TIMEOUT);
            install_result(device.install_apks(&apks, options, timeout))
        })
        .await
    }
}

/// Only `-r` is accepted: other package-manager flags (grant permissions,
/// downgrade, test-only) change what an install may do, and neither app
/// sends them. `install` takes exactly one APK.
fn split_install_args(verb: &str, rest: &[&str]) -> AdbResult<(bool, Vec<PathBuf>)> {
    let (replace, paths) = match rest {
        ["-r", paths @ ..] => (true, paths),
        paths => (false, paths),
    };
    if paths.is_empty()
        || paths
            .iter()
            .any(|path| path.is_empty() || path.starts_with('-'))
    {
        return Err(AdbError::Unsupported {
            operation: "install_flags",
        });
    }
    if verb == "install" && paths.len() != 1 {
        return Err(AdbError::Unsupported {
            operation: "install_multiple",
        });
    }
    Ok((replace, paths.iter().map(PathBuf::from).collect()))
}

fn check_local_apk(path: &Path) -> AdbResult<()> {
    if path
        .extension()
        .is_none_or(|extension| !extension.eq_ignore_ascii_case("apk"))
    {
        return Err(AdbError::Transport("Select a regular .apk file.".into()));
    }
    let metadata = std::fs::metadata(path)
        .map_err(|e| AdbError::Transport(format!("open {}: {e}", path.display())))?;
    if !metadata.is_file() || metadata.len() == 0 {
        return Err(AdbError::Transport(
            "Select a non-empty regular .apk file.".into(),
        ));
    }
    Ok(())
}

/// The same contract as `adb install`: exit 0 with the package manager's
/// `Success`, or a nonzero exit carrying its reason. A refusal (or the TV
/// refusing a staged copy on devices without `cmd`) is the install's answer.
/// Any other error came after the install request may have reached the TV,
/// so its outcome is unknown rather than a failure.
fn install_result(result: Result<String, RustADBError>) -> AdbResult<AdbOutput> {
    match result {
        Ok(stdout) => Ok(AdbOutput {
            stdout,
            stderr: String::new(),
            exit_code: Some(0),
        }),
        Err(RustADBError::InstallFailed(reason)) => Err(AdbError::NonZeroExit {
            code: Some(1),
            stderr: reason,
        }),
        Err(error @ RustADBError::SyncFailed(_)) => Err(AdbError::NonZeroExit {
            code: Some(1),
            stderr: error.to_string(),
        }),
        Err(error) => Err(AdbError::Uncertain {
            operation: "APK installation",
            detail: format!("{error}; verify installed apps before retrying"),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atv_optimizer_core::adb::{AdbDriver, Leases};
    use std::sync::Arc;

    #[tokio::test]
    async fn native_install_rejects_flags_and_extra_apks_before_connection_work() {
        let operations = Leases::new();
        let driver = WirelessAdb::with_existing_key("never-created-key".into(), operations.clone());
        for args in [
            vec!["-s", "tv", "install", "-g"],
            vec!["-s", "tv", "install", ""],
            vec!["-s", "tv", "install", "a.apk", "b.apk"],
            vec!["-s", "tv", "install", "-r", "a.apk", "b.apk"],
            vec!["-s", "tv", "install", "-t", "a.apk"],
            vec!["-s", "tv", "install", "-r", "-d", "a.apk"],
            vec!["-s", "tv", "install-multiple"],
            vec!["-s", "tv", "install-multiple", "-r"],
            vec!["-s", "tv", "install-multiple", "-g", "a.apk", "b.apk"],
            vec!["-s", "tv", "install-multiple", "-r", "a.apk", "-r;reboot"],
        ] {
            assert!(
                matches!(
                    driver.raw_transfer(&args).await,
                    Err(AdbError::Unsupported { .. })
                ),
                "{args:?}"
            );
        }
        assert_eq!(operations.active(), 0);
    }

    #[test]
    fn install_arguments_split_into_replace_and_apks() {
        let (replace, apks) =
            split_install_args("install-multiple", &["-r", "/a.apk", "/b.apk"]).unwrap();
        assert!(replace);
        assert_eq!(apks, [PathBuf::from("/a.apk"), PathBuf::from("/b.apk")]);
        let (replace, apks) = split_install_args("install", &["/a.apk"]).unwrap();
        assert!(!replace);
        assert_eq!(apks, [PathBuf::from("/a.apk")]);
        // adb accepts install-multiple with a single APK.
        assert!(split_install_args("install-multiple", &["/a.apk"]).is_ok());
    }

    #[tokio::test]
    async fn local_apk_problems_fail_before_any_tv_work() {
        let operations = Leases::new();
        let driver = WirelessAdb::with_existing_key("never-created-key".into(), operations.clone());
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("base.apk");
        std::fs::write(&base, b"apk").unwrap();
        let empty = dir.path().join("split.apk");
        std::fs::write(&empty, b"").unwrap();
        let text = dir.path().join("notes.txt");
        std::fs::write(&text, b"x").unwrap();
        let missing = dir.path().join("missing.apk");
        for (apks, expected) in [
            (vec![&base, &empty], "non-empty"),
            (vec![&base, &text], ".apk"),
            (vec![&base, &missing], "missing.apk"),
        ] {
            let mut args = vec!["-s", "tv", "install-multiple", "-r"];
            args.extend(apks.iter().map(|path| path.to_str().unwrap()));
            let error = driver.raw_transfer(&args).await.unwrap_err().to_string();
            assert!(error.contains(expected), "{error}");
        }
        // Valid files reach the connection check, which still costs no lease.
        let error = driver
            .raw_transfer(&["-s", "tv", "install", "-r", base.to_str().unwrap()])
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("Not connected"), "{error}");
        assert_eq!(operations.active(), 0);
    }

    #[test]
    fn a_package_manager_refusal_is_an_answer_and_anything_else_is_unknown() {
        let success = install_result(Ok("Success\n".to_string())).unwrap();
        assert_eq!(
            (success.stdout.as_str(), success.exit_code),
            ("Success\n", Some(0))
        );
        match install_result(Err(RustADBError::InstallFailed(
            "Failure [INSTALL_FAILED_VERSION_DOWNGRADE]".to_string(),
        ))) {
            Err(AdbError::NonZeroExit { code, stderr }) => {
                assert_eq!(code, Some(1));
                assert_eq!(stderr, "Failure [INSTALL_FAILED_VERSION_DOWNGRADE]");
            }
            other => panic!("{other:?}"),
        }
        match install_result(Err(RustADBError::SyncFailed(
            "push: No space left on device".to_string(),
        ))) {
            Err(AdbError::NonZeroExit { stderr, .. }) => {
                assert!(stderr.contains("No space left on device"), "{stderr}");
            }
            other => panic!("{other:?}"),
        }
        for error in [
            RustADBError::ProtocolError("lost ADB message framing".to_string()),
            RustADBError::IOError(std::io::Error::from(std::io::ErrorKind::TimedOut)),
        ] {
            let message = install_result(Err(error)).unwrap_err().to_string();
            assert!(message.contains("outcome is unknown"), "{message}");
            assert!(message.contains("before retrying"), "{message}");
        }
    }

    #[tokio::test]
    async fn canceled_install_worker_retains_lease_until_cleanup_finishes() {
        let operations = Leases::new();
        let operation = Arc::new(operations.try_acquire().unwrap());
        let control = BoundedShellControl::new(Duration::from_secs(2)).unwrap();
        let (started, started_rx) = tokio::sync::oneshot::channel();
        let (release, release_rx) = std::sync::mpsc::channel();
        let task = tokio::spawn(run_worker(operation, control, move |control| {
            // Required-key failure finishes the attempt without any network I/O.
            assert!(ADBTcpDevice::new_with_required_key_and_control(
                "127.0.0.1:0".parse().unwrap(),
                Path::new("missing-install-fixture-key-never-created"),
                &control
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

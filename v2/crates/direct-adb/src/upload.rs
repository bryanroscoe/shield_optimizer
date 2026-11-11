//! Every upload phase shares the same socket and hardware verification. In particular,
//! cleanup never resolves the currently selected endpoint after a transfer failure.
use super::*;
use adb_client::tcp::BoundedShellControl;
use atv_optimizer_core::adb::upload::{UploadIo, UploadResult};

struct UploadConnection {
    device: ADBTcpDevice,
    // Parent owns the socket for cancellation throughout the transaction. Each phase
    // borrows it under a fresh control: with_control finishes its argument on return.
    control: BoundedShellControl,
}
impl UploadIo for UploadConnection {
    fn shell(&mut self, command: &str) -> AdbResult<AdbOutput> {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let mut out = crate::limited_writer::LimitedWriter::new(&mut stdout, 8192);
        let mut err = crate::limited_writer::LimitedWriter::new(&mut stderr, 8192);
        let exit = self
            .device
            .shell_command_with_control(
                &command,
                Some(&mut out),
                Some(&mut err),
                BoundedShellControl::new(self.control.remaining()?)?,
            )
            .map_err(|error| AdbError::Transport(error.to_string()))?;
        Ok(AdbOutput {
            stdout: String::from_utf8_lossy(&stdout).into_owned(),
            stderr: String::from_utf8_lossy(&stderr).into_owned(),
            exit_code: exit.map(i32::from),
        })
    }
    fn push(&mut self, local: &Path, remote: &str) -> AdbResult<()> {
        let mut file = std::fs::File::open(local)
            .map_err(|error| AdbError::Transport(format!("open staged upload: {error}")))?;
        self.device
            .push_with_control(
                &mut file,
                &remote,
                BoundedShellControl::new(self.control.remaining()?)?,
            )
            .map_err(upload_error)
    }
}

impl WirelessAdb {
    /// Query the admitted connection, not whichever TV later occupies its address.
    /// Generation comparison and hardware query share the connection lock.
    async fn upload_hardware_id(
        &self,
        serial: &str,
        generation: u64,
        budget: Duration,
    ) -> AdbResult<String> {
        let control = BoundedShellControl::new(budget)?;
        let operation = self.work().map_err(AdbError::Transport)?;
        let conn = self.conn.clone();
        let identity = self.identity.clone();
        let serial = serial.to_owned();
        crate::bounded_shell::run_worker(operation, control, move |control| {
            let mut guard = lock_within(&conn, control.remaining()?).ok_or_else(|| {
                AdbError::Transport("Selected TV hardware identity query timed out; no upload was started.".into())
            })?;
            control.remaining()?;
            let active = guard.as_mut().ok_or_else(|| AdbError::Transport("Not connected to a device.".into()))?;
            validate_requested_serial(&active.serial, &serial).map_err(AdbError::Transport)?;
            if active.request_id != generation {
                return Err(AdbError::Transport("Selected TV connection changed while the upload was waiting. Nothing was uploaded.".into()));
            }
            let raw_id = match active.device.hardware_serial_bounded(control) {
                Ok(id) => id,
                Err(error) => {
                    // Never probe a replacement connection. Clear only the exact locked
                    // generation if this identity query lost its transport.
                    if is_fatal_transport_error(&error) {
                        *guard = None;
                        *lock_recovering(&identity) = None;
                    }
                    return Err(AdbError::Transport(format!("Upload hardware identity query failed: {error}")));
                }
            };
            atv_optimizer_core::commands::devices::verified_hardware_id(&raw_id)
                .map(str::to_owned)
                .ok_or_else(|| AdbError::Transport("Selected TV has no verified hardware identity; no upload was started.".into()))
        }).await
    }

    pub(crate) async fn upload_file_impl(
        &self,
        serial: &str,
        expected_hardware_id: &str,
        local: &Path,
        target: &str,
        expected_size: u64,
        overwrite: bool,
    ) -> AdbResult<UploadResult> {
        let requested_hardware =
            atv_optimizer_core::commands::devices::verified_hardware_id(expected_hardware_id)
                .ok_or_else(|| {
                    AdbError::Transport(
                        "The upload needs a verified TV hardware identity. Nothing was uploaded."
                            .into(),
                    )
                })?
                .to_owned();
        let deadline = tokio::time::Instant::now() + PUSH_DEADLINE;
        // Capture the generation at admission, before waiting for the selected socket.
        // The same address can now refer to a different connection (and hardware).
        let (active, host, port, generation) = lock_recovering(&self.identity)
            .as_ref()
            .map(|identity| {
                (
                    identity.serial.clone(),
                    identity.host.clone(),
                    identity.port,
                    identity.request_id,
                )
            })
            .ok_or_else(|| AdbError::Transport("Not connected to a device.".into()))?;
        validate_requested_serial(&active, serial).map_err(AdbError::Transport)?;
        let address = format!("{host}:{port}")
            .parse()
            .map_err(|error| AdbError::Transport(format!("invalid upload address: {error}")))?;
        let expected = self
            .upload_hardware_id(serial, generation, Duration::from_secs(5))
            .await?;
        if expected != requested_hardware {
            return Err(AdbError::Transport("The TV hardware identity changed since the upload was selected. Nothing was uploaded.".into()));
        }
        let operation = self.work().map_err(AdbError::Transport)?;
        let control = BoundedShellControl::new(
            deadline.saturating_duration_since(tokio::time::Instant::now()),
        )?;
        let key = self.key_path.clone();
        let preferred = self.dedicated_keys();
        let local = local.to_owned();
        let target = target.to_owned();
        crate::bounded_shell::run_worker(operation, control, move |control| {
            let mut device = ADBTcpDevice::new_with_required_keys_and_control(
                address, &key, &preferred, &control,
            )
            .map_err(|error| AdbError::Transport(dedicated_error(&error)))?;
            let probe = BoundedShellControl::new(control.remaining()?.min(Duration::from_secs(5)))?;
            device
                .verify_hardware_binding_bounded(&expected, probe)
                .map_err(|error| {
                    AdbError::Transport(format!(
                        "Upload identity check failed before any byte was sent: {error}"
                    ))
                })?;
            let mut io = UploadConnection { device, control };
            atv_optimizer_core::adb::upload::upload(
                &mut io,
                &local,
                &target,
                expected_size,
                overwrite,
            )
        })
        .await
    }
}

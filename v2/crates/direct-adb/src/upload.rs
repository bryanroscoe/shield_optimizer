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
                remote,
                BoundedShellControl::new(self.control.remaining()?)?,
            )
            .map_err(upload_error)
    }
}

impl WirelessAdb {
    pub(crate) async fn upload_file_impl(
        &self,
        serial: &str,
        local: &Path,
        target: &str,
        expected_size: u64,
        overwrite: bool,
    ) -> AdbResult<UploadResult> {
        let deadline = tokio::time::Instant::now() + PUSH_DEADLINE;
        let Some((active, host, port)) = self.info() else {
            return Err(AdbError::Transport("Not connected to a device.".into()));
        };
        validate_requested_serial(&active, serial).map_err(AdbError::Transport)?;
        let address = format!("{host}:{port}")
            .parse()
            .map_err(|error| AdbError::Transport(format!("invalid upload address: {error}")))?;
        let expected = self
            .verified_owned_hardware_id(serial, Duration::from_secs(5))
            .await?;
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

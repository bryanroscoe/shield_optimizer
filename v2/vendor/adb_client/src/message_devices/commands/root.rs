use crate::{
    Result, RustADBError,
    message_devices::{
        adb_message_device::ADBMessageDevice,
        adb_message_transport::{ADBMessageTransport, DEFAULT_READ_TIMEOUT, is_hang_up},
    },
    models::ADBLocalCommand,
};

/// Bound on the status text a one-shot adbd service (root, remount, verity)
/// prints before closing.
const SERVICE_MESSAGE_LIMIT: usize = 64 * 1024;

impl<T: ADBMessageTransport> ADBMessageDevice<T> {
    /// Ask adbd to restart as root. The OKAY to the OPEN is the acceptance;
    /// adbd then says what it did and restarts, dropping the connection. A
    /// production build answers that it cannot instead.
    pub(crate) fn root(&mut self) -> Result<()> {
        let output = self.service_message(&ADBLocalCommand::Root)?;
        if output.contains("cannot run as root") {
            return Err(RustADBError::ADBRequestFailed(output.trim().to_string()));
        }
        log::info!("{}", output.trim());
        Ok(())
    }

    /// Open a one-shot service and read what it prints until it closes. The
    /// request is accepted once its OPEN is acknowledged. adbd may restart or
    /// the device reboot right after answering, so the connection dropping
    /// then ends the message instead of failing the request.
    pub(crate) fn service_message(&mut self, command: &ADBLocalCommand) -> Result<String> {
        let mut stream = self.open_service(command, DEFAULT_READ_TIMEOUT)?;
        let mut output = Vec::new();
        match stream.read_to_close(Some(&mut output), SERVICE_MESSAGE_LIMIT) {
            Ok(_) => {}
            Err(RustADBError::IOError(error)) if is_hang_up(error.kind()) => {
                log::debug!("{command} dropped the connection after it was accepted: {error}");
            }
            Err(error) => return Err(error),
        }
        Ok(String::from_utf8_lossy(&output).into_owned())
    }
}

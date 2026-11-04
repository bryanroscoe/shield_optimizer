use std::io::Read;

use crate::{
    Result, RustADBError,
    message_devices::{
        adb_message_device::ADBMessageDevice,
        adb_message_transport::{ADBMessageTransport, DEFAULT_READ_TIMEOUT},
        adb_service::adb_error,
    },
    models::ADBLocalCommand,
};

impl<T: ADBMessageTransport> ADBMessageDevice<T> {
    /// Ask adbd to restart as root. adbd says what it did and closes the
    /// stream; a production build answers that it cannot.
    pub(crate) fn root(&mut self) -> Result<()> {
        let output = self.read_service_output(&ADBLocalCommand::Root)?;
        if output.contains("cannot run as root") {
            return Err(RustADBError::ADBRequestFailed(output.trim().to_string()));
        }
        log::info!("{}", output.trim());
        Ok(())
    }

    /// Everything a one-shot service prints before closing.
    pub(crate) fn read_service_output(&mut self, command: &ADBLocalCommand) -> Result<String> {
        let mut stream = self.open_service(command, DEFAULT_READ_TIMEOUT)?;
        let mut output = Vec::new();
        stream.read_to_end(&mut output).map_err(adb_error)?;
        Ok(String::from_utf8_lossy(&output).into_owned())
    }
}

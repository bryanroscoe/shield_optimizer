use crate::{
    Result,
    message_devices::{
        adb_message_device::ADBMessageDevice, adb_message_transport::ADBMessageTransport,
    },
    models::ADBLocalCommand,
};

impl<T: ADBMessageTransport> ADBMessageDevice<T> {
    pub(crate) fn enable_verity(&mut self) -> Result<()> {
        let output = self.service_message(&ADBLocalCommand::EnableVerity)?;
        log::info!("{}", output.trim());
        Ok(())
    }

    pub(crate) fn disable_verity(&mut self) -> Result<()> {
        let output = self.service_message(&ADBLocalCommand::DisableVerity)?;
        log::info!("{}", output.trim());
        Ok(())
    }
}

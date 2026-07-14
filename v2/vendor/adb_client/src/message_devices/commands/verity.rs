use crate::{
    Result,
    message_devices::{
        adb_message_device::ADBMessageDevice, adb_message_transport::ADBMessageTransport,
        message_commands::MessageCommand,
    },
    models::ADBLocalCommand,
};

impl<T: ADBMessageTransport> ADBMessageDevice<T> {
    pub(crate) fn enable_verity(&mut self) -> Result<()> {
        self.open_session(&ADBLocalCommand::EnableVerity)?;

        self.get_transport_mut()
            .read_message()
            .and_then(|message| message.assert_command(MessageCommand::Okay))
    }

    pub(crate) fn disable_verity(&mut self) -> Result<()> {
        self.open_session(&ADBLocalCommand::DisableVerity)?;

        self.get_transport_mut()
            .read_message()
            .and_then(|message| message.assert_command(MessageCommand::Okay))
    }
}

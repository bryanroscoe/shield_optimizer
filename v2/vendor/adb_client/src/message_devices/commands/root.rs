use crate::{
    Result,
    message_devices::{
        adb_message_device::ADBMessageDevice, adb_message_transport::ADBMessageTransport,
        message_commands::MessageCommand,
    },
    models::ADBLocalCommand,
};

impl<T: ADBMessageTransport> ADBMessageDevice<T> {
    pub(crate) fn root(&mut self) -> Result<()> {
        self.open_session(&ADBLocalCommand::Root)?;

        self.get_transport_mut()
            .read_message()
            .and_then(|message| message.assert_command(MessageCommand::Okay))
    }
}

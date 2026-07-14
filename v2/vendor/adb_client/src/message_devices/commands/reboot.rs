use crate::{
    RebootType, Result,
    message_devices::{
        adb_message_device::ADBMessageDevice, adb_message_transport::ADBMessageTransport,
    },
    models::ADBLocalCommand,
};

impl<T: ADBMessageTransport> ADBMessageDevice<T> {
    pub(crate) fn reboot(&mut self, reboot_type: RebootType) -> Result<()> {
        // Receiving the OPEN acknowledgement proves adbd accepted the reboot
        // service. Waiting for another message is incorrect: the device may
        // tear down the transport immediately as it begins rebooting.
        self.open_session(&ADBLocalCommand::Reboot(reboot_type))?;
        Ok(())
    }
}

use crate::{
    Result,
    models::{ADBCommand, ADBLocalCommand, RebootType},
    server_device::ADBServerDevice,
};

impl ADBServerDevice {
    /// Reboots the device
    pub fn reboot(&mut self, reboot_type: RebootType) -> Result<()> {
        self.set_serial_transport()?;

        self.transport
            .proxy_connection(
                &ADBCommand::Local(ADBLocalCommand::Reboot(reboot_type)),
                false,
            )
            .map(|_| ())
    }
}

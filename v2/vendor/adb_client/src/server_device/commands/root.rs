use crate::{
    Result,
    models::{ADBCommand, ADBLocalCommand},
    server_device::ADBServerDevice,
};

impl ADBServerDevice {
    /// Restart adb daemon with root permissions
    pub fn root(&mut self) -> Result<()> {
        self.set_serial_transport()?;

        self.transport
            .proxy_connection(&ADBCommand::Local(ADBLocalCommand::Root), false)
            .map(|_| ())
    }
}

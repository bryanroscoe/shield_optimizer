use crate::{
    Result,
    models::{ADBCommand, ADBLocalCommand},
    server_device::ADBServerDevice,
};

impl ADBServerDevice {
    /// Reconnect device
    pub fn reconnect(&mut self) -> Result<()> {
        self.set_serial_transport()?;

        self.transport
            .proxy_connection(&ADBCommand::Local(ADBLocalCommand::Reconnect), false)
            .map(|_| ())
    }
}

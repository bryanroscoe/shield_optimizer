use crate::{
    Result,
    models::{ADBCommand, ADBLocalCommand},
    server_device::ADBServerDevice,
};

impl ADBServerDevice {
    /// Disable verity on the device
    pub fn disable_verity(&mut self) -> Result<()> {
        self.set_serial_transport()?;

        self.transport
            .send_adb_request(&ADBCommand::Local(ADBLocalCommand::DisableVerity))
    }

    /// Enable verity on the device
    pub fn enable_verity(&mut self) -> Result<()> {
        self.set_serial_transport()?;

        self.transport
            .send_adb_request(&ADBCommand::Local(ADBLocalCommand::EnableVerity))
    }
}

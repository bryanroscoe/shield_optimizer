use crate::{
    Result,
    emulator::{ADBEmulatorCommand, ADBEmulatorDevice},
};

impl ADBEmulatorDevice {
    /// Send a SMS to this emulator with given content with given phone number
    pub fn rotate(&mut self) -> Result<()> {
        let _ = self.connect()?.send_command(&ADBEmulatorCommand::Rotate)?;
        Ok(())
    }
}

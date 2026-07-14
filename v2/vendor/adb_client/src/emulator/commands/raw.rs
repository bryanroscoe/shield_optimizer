use crate::{
    Result,
    emulator::{ADBEmulatorCommand, ADBEmulatorDevice},
};

impl ADBEmulatorDevice {
    /// Send a raw console command to this emulator and return its response
    pub fn send_raw_command(&mut self, command: &str) -> Result<String> {
        self.connect()?
            .send_command(&ADBEmulatorCommand::Raw(command.to_string()))
    }
}

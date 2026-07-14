use std::path::PathBuf;

use crate::{
    Result,
    emulator::{ADBEmulatorCommand, ADBEmulatorDevice},
};

impl ADBEmulatorDevice {
    /// Get the AVD discovery path of this emulator
    pub fn avd_discovery_path(&mut self) -> Result<PathBuf> {
        let path = self
            .connect()?
            .send_command(&ADBEmulatorCommand::AvdDiscoveryPath)?;
        Ok(PathBuf::from(path.trim()))
    }
    /// Get the gRPC port of this emulator
    pub fn avd_grpc_port(&mut self) -> Result<u16> {
        let port = self
            .connect()?
            .send_command(&ADBEmulatorCommand::AvdGrpcPort)?;
        Ok(port.trim().parse()?)
    }
}

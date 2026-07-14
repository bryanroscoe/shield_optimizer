use crate::{
    Result,
    models::{ADBCommand, ADBHostCommand, HostFeatures},
    server_device::ADBServerDevice,
};

impl ADBServerDevice {
    /// Lists available ADB server features.
    pub fn host_features(&mut self) -> Result<Vec<HostFeatures>> {
        self.set_serial_transport()?;

        let features = self
            .transport
            .proxy_connection(&ADBCommand::Host(ADBHostCommand::HostFeatures), true)?;

        Ok(features
            .split(|x| x.eq(&b','))
            .filter_map(|v| HostFeatures::try_from(v).ok())
            .collect())
    }
}

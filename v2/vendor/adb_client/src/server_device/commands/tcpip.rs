use crate::{
    Result,
    models::{ADBCommand, ADBLocalCommand},
    server_device::ADBServerDevice,
};

impl ADBServerDevice {
    /// Set adb daemon to tcp/ip mode
    pub fn tcpip(&mut self, port: u16) -> Result<()> {
        self.set_serial_transport()?;

        self.transport
            .proxy_connection(&ADBCommand::Local(ADBLocalCommand::TcpIp(port)), false)
            .map(|_| ())
    }
}

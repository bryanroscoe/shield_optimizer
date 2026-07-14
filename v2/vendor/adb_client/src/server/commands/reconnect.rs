use crate::{
    Result,
    models::{ADBCommand, ADBHostCommand},
    server::ADBServer,
};

impl ADBServer {
    /// Reconnect the device
    pub fn reconnect_offline(&mut self) -> Result<()> {
        self.connect()?
            .proxy_connection(&ADBCommand::Host(ADBHostCommand::ReconnectOffline), false)
            .map(|_| ())
    }
}

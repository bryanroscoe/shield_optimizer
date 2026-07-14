use crate::{
    Result,
    models::{ADBCommand, ADBHostCommand},
    server::ADBServer,
};

impl ADBServer {
    /// Asks the ADB server to quit immediately.
    pub fn kill(&mut self) -> Result<()> {
        self.connect()?
            .proxy_connection(&ADBCommand::Host(ADBHostCommand::Kill), false)
            .map(|_| ())
    }
}

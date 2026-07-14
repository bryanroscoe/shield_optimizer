use crate::{
    Result,
    models::{ADBCommand, ADBHostCommand},
    server::{ADBServer, models::ServerStatus},
};

impl ADBServer {
    /// Check ADB server status
    pub fn server_status(&mut self) -> Result<ServerStatus> {
        let status = self
            .connect()?
            .proxy_connection(&ADBCommand::Host(ADBHostCommand::ServerStatus), true)?;

        ServerStatus::try_from(status)
    }
}

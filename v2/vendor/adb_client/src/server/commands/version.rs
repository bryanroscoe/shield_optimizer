use crate::{
    Result,
    models::{ADBCommand, ADBHostCommand},
    server::{ADBServer, AdbVersion},
};

impl ADBServer {
    /// Gets server's internal version number.
    pub fn version(&mut self) -> Result<AdbVersion> {
        let version = self
            .connect()?
            .proxy_connection(&ADBCommand::Host(ADBHostCommand::Version), true)?;

        AdbVersion::try_from(version)
    }
}

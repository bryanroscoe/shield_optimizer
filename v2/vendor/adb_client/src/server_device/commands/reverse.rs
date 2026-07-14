use crate::{
    Result,
    models::{ADBCommand, ADBLocalCommand},
    server_device::ADBServerDevice,
};

impl ADBServerDevice {
    /// Reverse socket connection
    pub fn reverse(&mut self, remote: String, local: String) -> Result<()> {
        self.set_serial_transport()?;

        self.transport
            .proxy_connection(
                &ADBCommand::Local(ADBLocalCommand::Reverse(remote, local)),
                false,
            )
            .map(|_| ())
    }

    /// Remove a previously applied reverse rule by its remote endpoint.
    pub fn reverse_remove(&mut self, remote: String) -> Result<()> {
        self.set_serial_transport()?;

        self.transport
            .proxy_connection(
                &ADBCommand::Local(ADBLocalCommand::ReverseRemove(remote)),
                false,
            )
            .map(|_| ())
    }

    /// Remove all reverse rules
    pub fn reverse_remove_all(&mut self) -> Result<()> {
        self.set_serial_transport()?;

        self.transport
            .proxy_connection(&ADBCommand::Local(ADBLocalCommand::ReverseRemoveAll), false)
            .map(|_| ())
    }
}

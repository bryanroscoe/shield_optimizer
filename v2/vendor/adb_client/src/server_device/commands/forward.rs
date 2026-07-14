use crate::{
    Result,
    models::{ADBCommand, ADBLocalCommand},
    server_device::ADBServerDevice,
};

impl ADBServerDevice {
    /// Forward socket connection
    pub fn forward(&mut self, remote: String, local: String) -> Result<()> {
        self.set_serial_transport()?;

        self.transport
            .proxy_connection(
                &ADBCommand::Local(ADBLocalCommand::Forward(remote, local)),
                false,
            )
            .map(|_| ())
    }

    /// Remove a previously applied forward rule by its local endpoint.
    pub fn forward_remove(&mut self, local: String) -> Result<()> {
        self.set_serial_transport()?;

        self.transport
            .proxy_connection(
                &ADBCommand::Local(ADBLocalCommand::ForwardRemove(local)),
                false,
            )
            .map(|_| ())
    }

    /// Remove all previously applied forward rules
    pub fn forward_remove_all(&mut self) -> Result<()> {
        self.set_serial_transport()?;

        self.transport
            .proxy_connection(&ADBCommand::Local(ADBLocalCommand::ForwardRemoveAll), false)
            .map(|_| ())
    }
}

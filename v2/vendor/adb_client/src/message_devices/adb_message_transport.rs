use std::time::Duration;

use crate::{
    Result, adb_transport::ADBTransport,
    message_devices::adb_transport_message::ADBTransportMessage,
};

/// Safety net for every read that does not pass an explicit timeout. A silently
/// dropped peer (phone roams to another AP, TV loses Wi-Fi) never delivers a
/// TCP RST, so an unbounded read would block the owning connection forever.
/// Two minutes is long enough for a slow `pm install`, and every read in a
/// transfer resets it because it is an inactivity bound, not a total one.
pub const DEFAULT_READ_TIMEOUT: Duration = Duration::from_secs(120);
const DEFAULT_WRITE_TIMEOUT: Duration = Duration::from_secs(2);

/// Trait representing a transport able to read and write messages.
pub trait ADBMessageTransport: ADBTransport + Clone + Send + 'static {
    /// An upgrade of the connection has been asked by the device.
    /// Some transports may not need this feature, a blanket implementation is provided as default implementation.
    fn upgrade_connection(&mut self) -> Result<()> {
        log::trace!("not upgrade needed fot this transport");
        Ok(())
    }

    /// Read a message using given timeout on the underlying transport
    fn read_message_with_timeout(&mut self, read_timeout: Duration) -> Result<ADBTransportMessage>;

    /// Read data to underlying connection, using default timeout
    fn read_message(&mut self) -> Result<ADBTransportMessage> {
        self.read_message_with_timeout(DEFAULT_READ_TIMEOUT)
    }

    /// Write a message using given timeout on the underlying transport
    fn write_message_with_timeout(
        &mut self,
        message: ADBTransportMessage,
        write_timeout: Duration,
    ) -> Result<()>;

    /// Write data to underlying connection, using default timeout
    fn write_message(&mut self, message: ADBTransportMessage) -> Result<()> {
        self.write_message_with_timeout(message, DEFAULT_WRITE_TIMEOUT)
    }
}

use std::time::Duration;

use crate::{
    Result,
    adb_transport::ADBTransport,
    message_devices::{
        adb_connection_info::{ConnectionInfo, MAX_PAYLOAD_V1},
        adb_transport_message::ADBTransportMessage,
    },
};

/// A socket read or write ran out of time: `WouldBlock` on Unix, `TimedOut`
/// on Windows.
pub(crate) const fn is_timeout(kind: std::io::ErrorKind) -> bool {
    matches!(
        kind,
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
    )
}

/// The peer or the network ended the connection (rather than one stream).
pub(crate) const fn is_hang_up(kind: std::io::ErrorKind) -> bool {
    matches!(
        kind,
        std::io::ErrorKind::UnexpectedEof
            | std::io::ErrorKind::ConnectionReset
            | std::io::ErrorKind::ConnectionAborted
            | std::io::ErrorKind::BrokenPipe
    )
}

/// Safety net for every read that does not pass an explicit timeout. A silently
/// dropped peer (phone roams to another AP, TV loses Wi-Fi) never delivers a
/// TCP RST, so an unbounded read would block the owning connection forever.
/// Two minutes is long enough for a slow `pm install`, and every read in a
/// transfer resets it because it is an inactivity bound, not a total one.
pub const DEFAULT_READ_TIMEOUT: Duration = Duration::from_secs(120);
const DEFAULT_WRITE_TIMEOUT: Duration = Duration::from_secs(2);

/// Trait representing a transport able to read and write messages.
pub trait ADBMessageTransport: ADBTransport + Clone + Send + 'static {
    /// An upgrade of the connection has been asked by the device. The whole
    /// upgrade must finish within `timeout`.
    /// Some transports may not need this feature, a blanket implementation is provided as default implementation.
    fn upgrade_connection(&mut self, _timeout: Duration) -> Result<()> {
        log::trace!("not upgrade needed fot this transport");
        Ok(())
    }

    /// Record what the CNXN exchange negotiated. Transports that frame
    /// messages themselves enforce it on every later read and write.
    fn apply_connection_info(&mut self, _info: &ConnectionInfo) {}

    /// How many client certificates [`Self::upgrade_connection`] can choose
    /// from.
    fn tls_identity_count(&self) -> usize {
        1
    }

    /// Present certificate `index` on the next upgrade, or `None` to choose
    /// from the device's hints.
    fn present_tls_identity(&mut self, _index: Option<usize>) {}

    /// The certificate the last upgrade presented, and whether the device's
    /// hints named it. `None` until a device asked for one.
    fn presented_tls_identity(&self) -> Option<(usize, bool)> {
        None
    }

    /// Largest payload this side may send. A transport that does not record
    /// the negotiated limit stays within what every adbd accepts.
    fn max_payload(&self) -> usize {
        MAX_PAYLOAD_V1
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

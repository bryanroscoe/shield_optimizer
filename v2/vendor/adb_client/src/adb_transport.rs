use crate::Result;

/// Trait representing a transport usable by ADB protocol.
pub trait ADBTransport {
    /// Initializes the connection to this transport.
    fn connect(&mut self) -> Result<()>;

    /// Shuts down the connection to this transport.
    fn disconnect(&mut self) -> Result<()>;
}

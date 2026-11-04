use std::io::Write;

use crate::{
    Result,
    message_devices::{
        adb_message_device::ADBMessageDevice,
        adb_message_transport::{ADBMessageTransport, DEFAULT_READ_TIMEOUT},
    },
};

impl<T: ADBMessageTransport> ADBMessageDevice<T> {
    pub(crate) fn pull<A: AsRef<str>, W: Write>(&mut self, source: A, mut output: W) -> Result<()> {
        let source = source.as_ref();
        let mut sync = self.open_sync(DEFAULT_READ_TIMEOUT)?;
        // A missing or unreadable source fails here with the device's reason,
        // before any of `output` is written.
        sync.stat(source)?;
        sync.recv(source, &mut output)?;
        sync.finish();
        Ok(())
    }
}

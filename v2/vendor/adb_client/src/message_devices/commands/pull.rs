use std::io::Write;

use crate::{
    Result, RustADBError,
    message_devices::{
        adb_message_device::ADBMessageDevice,
        adb_message_transport::{ADBMessageTransport, DEFAULT_READ_TIMEOUT},
    },
};

impl<T: ADBMessageTransport> ADBMessageDevice<T> {
    pub(crate) fn pull<A: AsRef<str>, W: Write>(&mut self, source: A, output: W) -> Result<()> {
        self.pull_at_most(source.as_ref(), output, None)
    }

    /// Pull `source` unless the device reports it larger than `max_bytes`,
    /// in which case nothing is transferred ([`RustADBError::FileTooLarge`]).
    /// The size check and the transfer share one sync session. A file that
    /// grows past its reported size is the caller's `output` to bound.
    pub(crate) fn pull_at_most<W: Write>(
        &mut self,
        source: &str,
        mut output: W,
        max_bytes: Option<u64>,
    ) -> Result<()> {
        let mut sync = self.open_sync(DEFAULT_READ_TIMEOUT)?;
        // A missing or unreadable source fails here with the device's reason,
        // before any of `output` is written.
        let stat = sync.stat(source)?;
        if let Some(limit) = max_bytes
            && stat.file_size > limit
        {
            sync.finish();
            return Err(RustADBError::FileTooLarge(stat.file_size, limit));
        }
        sync.recv(source, &mut output)?;
        sync.finish();
        Ok(())
    }
}

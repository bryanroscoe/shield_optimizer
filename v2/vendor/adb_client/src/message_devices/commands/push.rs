use std::io::Read;

use crate::{
    Result,
    message_devices::{
        adb_message_device::ADBMessageDevice,
        adb_message_transport::{ADBMessageTransport, DEFAULT_READ_TIMEOUT},
        adb_sync::{PUSH_FILE_MODE, now_mtime},
    },
};

impl<T: ADBMessageTransport> ADBMessageDevice<T> {
    /// Push `stream` to `path` as a `rw-r--r--` file stamped with the current
    /// time. Fails with the device's reason if it answers `FAIL`.
    pub(crate) fn push<R: Read, A: AsRef<str>>(&mut self, mut stream: R, path: A) -> Result<()> {
        let mut sync = self.open_sync(DEFAULT_READ_TIMEOUT)?;
        sync.send(&mut stream, path.as_ref(), PUSH_FILE_MODE, now_mtime())?;
        sync.finish();
        Ok(())
    }
}

use crate::{
    ADBListItemType, Result,
    message_devices::{
        adb_message_device::ADBMessageDevice,
        adb_message_transport::{ADBMessageTransport, DEFAULT_READ_TIMEOUT},
    },
};

impl<T: ADBMessageTransport> ADBMessageDevice<T> {
    /// List the entries in the given directory on the device.
    /// note: path uses internal file paths, so Documents is at /storage/emulated/0/Documents
    pub(crate) fn list<A: AsRef<str>>(&mut self, path: A) -> Result<Vec<ADBListItemType>> {
        let mut sync = self.open_sync(DEFAULT_READ_TIMEOUT)?;
        let entries = sync.list(path.as_ref())?;
        sync.finish();
        Ok(entries)
    }
}

use crate::{
    AdbStatResponse, Result,
    message_devices::{
        adb_message_device::ADBMessageDevice,
        adb_message_transport::{ADBMessageTransport, DEFAULT_READ_TIMEOUT},
    },
};

impl<T: ADBMessageTransport> ADBMessageDevice<T> {
    pub(crate) fn stat(&mut self, remote_path: &dyn AsRef<str>) -> Result<AdbStatResponse> {
        let mut sync = self.open_sync(DEFAULT_READ_TIMEOUT)?;
        let response = sync.stat(remote_path.as_ref())?;
        sync.finish();
        Ok(response)
    }
}

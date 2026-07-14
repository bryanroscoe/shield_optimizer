use crate::{
    AdbStatResponse, Result,
    message_devices::{
        adb_message_device::ADBMessageDevice, adb_message_transport::ADBMessageTransport,
    },
};

impl<T: ADBMessageTransport> ADBMessageDevice<T> {
    pub(crate) fn stat(&mut self, remote_path: &dyn AsRef<str>) -> Result<AdbStatResponse> {
        let mut session = self.open_synchronization_session()?;
        let adb_stat_response = session.stat_with_explicit_ids(remote_path.as_ref())?;
        self.end_transaction(&mut session)?;
        Ok(adb_stat_response)
    }
}

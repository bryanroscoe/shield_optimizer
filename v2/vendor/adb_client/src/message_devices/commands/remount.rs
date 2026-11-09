use crate::{
    Result,
    message_devices::{
        adb_message_device::ADBMessageDevice, adb_message_transport::ADBMessageTransport,
    },
    models::{ADBLocalCommand, RemountInfo},
};

impl<T: ADBMessageTransport> ADBMessageDevice<T> {
    pub(crate) fn remount(&mut self) -> Result<Vec<RemountInfo>> {
        let output = self.service_message(&ADBLocalCommand::Remount)?;
        RemountInfo::from_str_response(output.trim())
    }
}

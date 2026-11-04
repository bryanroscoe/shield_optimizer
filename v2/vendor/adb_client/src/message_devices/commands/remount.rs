use std::io::Read;

use crate::{
    Result,
    message_devices::{
        adb_message_device::ADBMessageDevice,
        adb_message_transport::{ADBMessageTransport, DEFAULT_READ_TIMEOUT},
        adb_service::adb_error,
    },
    models::{ADBLocalCommand, RemountInfo},
};

impl<T: ADBMessageTransport> ADBMessageDevice<T> {
    pub(crate) fn remount(&mut self) -> Result<Vec<RemountInfo>> {
        let mut stream = self.open_service(&ADBLocalCommand::Remount, DEFAULT_READ_TIMEOUT)?;
        let mut output = Vec::new();
        stream.read_to_end(&mut output).map_err(adb_error)?;
        RemountInfo::from_str_response(String::from_utf8_lossy(&output).trim())
    }
}

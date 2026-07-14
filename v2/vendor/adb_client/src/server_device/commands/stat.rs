use std::io::{Read, Write};

use byteorder::{ByteOrder, LittleEndian};

use crate::{
    AdbStatResponse, Result, RustADBError,
    models::{ADBCommand, ADBLocalCommand, SyncCommand},
    server_device::ADBServerDevice,
};

impl ADBServerDevice {
    fn handle_stat_command<S: AsRef<str>>(&self, path: S) -> Result<AdbStatResponse> {
        let mut len_buf = [0_u8; 4];
        LittleEndian::write_u32(&mut len_buf, u32::try_from(path.as_ref().len())?);

        // 4 bytes of command name is already sent by send_sync_request
        self.transport.get_raw_connection()?.write_all(&len_buf)?;
        self.transport
            .get_raw_connection()?
            .write_all(path.as_ref().to_string().as_bytes())?;

        // Reads returned status code from ADB server
        let mut response = [0_u8; 4];
        self.transport
            .get_raw_connection()?
            .read_exact(&mut response)?;
        match std::str::from_utf8(response.as_ref())? {
            "STAT" => {
                let mut data = [0_u8; 12];
                self.transport.get_raw_connection()?.read_exact(&mut data)?;

                Ok(data.into())
            }
            x => Err(RustADBError::UnknownResponseType(format!(
                "Unknown response {x}"
            ))),
        }
    }

    /// Stat file given as path on the device.
    pub fn stat<A: AsRef<str>>(&mut self, path: A) -> Result<AdbStatResponse> {
        self.set_serial_transport()?;

        // Set device in SYNC mode
        self.transport
            .send_adb_request(&ADBCommand::Local(ADBLocalCommand::Sync))?;

        // Send a "Stat" command
        self.transport.send_sync_request(&SyncCommand::Stat)?;

        self.handle_stat_command(path)
    }
}

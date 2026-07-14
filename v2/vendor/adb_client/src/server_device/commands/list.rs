use crate::{
    Result,
    models::{ADBCommand, ADBListItem, ADBListItemType, ADBLocalCommand, SyncCommand},
    server_device::ADBServerDevice,
};
use byteorder::{ByteOrder, LittleEndian, ReadBytesExt};
use std::{
    io::{Read, Write},
    str,
};

impl ADBServerDevice {
    /// Lists files in path on the device.
    /// note: path uses internal file paths, so Documents is at /storage/emulated/0/Documents
    pub fn list<A: AsRef<str>>(&mut self, path: A) -> Result<Vec<ADBListItemType>> {
        self.set_serial_transport()?;

        // Set device in SYNC mode
        self.transport
            .send_adb_request(&ADBCommand::Local(ADBLocalCommand::Sync))?;

        // Send a list command
        self.transport.send_sync_request(&SyncCommand::List)?;

        self.handle_list_command(path)
    }

    fn handle_list_command<A: AsRef<str>>(&self, path: A) -> Result<Vec<ADBListItemType>> {
        // TODO: use LIS2 to support files over 2.14 GB in size.
        // SEE: https://github.com/cstyan/adbDocumentation?tab=readme-ov-file#adb-list
        let mut len_buf = [0_u8; 4];
        LittleEndian::write_u32(&mut len_buf, u32::try_from(path.as_ref().len())?);

        // 4 bytes of command name is already sent by send_sync_request
        self.transport.get_raw_connection()?.write_all(&len_buf)?;

        // List send the string of the directory to list, and then the server send a list of files
        self.transport
            .get_raw_connection()?
            .write_all(path.as_ref().to_string().as_bytes())?;

        let mut list_items = Vec::new();

        // Reads returned status code from ADB server
        let mut response = [0_u8; 4];
        loop {
            self.transport
                .get_raw_connection()?
                .read_exact(&mut response)?;
            match str::from_utf8(response.as_ref())? {
                "DENT" => {
                    let mut connection = self.transport.get_raw_connection()?;

                    let mode = connection.read_u32::<LittleEndian>()?;
                    let size = connection.read_u32::<LittleEndian>()?;
                    let time = connection.read_u32::<LittleEndian>()?;
                    let name_len = connection.read_u32::<LittleEndian>()?;
                    let mut name_buf = vec![0_u8; name_len as usize];
                    connection.read_exact(&mut name_buf)?;
                    let name = String::from_utf8(name_buf)?;

                    // First 9 bits are the file permissions
                    let permissions = mode & 0b1_1111_1111;

                    let entry = ADBListItem {
                        name,
                        time,
                        permissions,
                        size,
                    };

                    list_items.push(ADBListItemType::from_mode_and_entry(mode, entry));
                }
                "DONE" => {
                    return Ok(list_items);
                }
                x => log::error!("Got an unknown response {x}"),
            }
        }
    }
}

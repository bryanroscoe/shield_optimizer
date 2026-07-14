use std::io::Read;

use crate::{
    Result,
    models::{ADBCommand, ADBLocalCommand},
    server_device::ADBServerDevice,
};

impl ADBServerDevice {
    /// Uninstall a package from device
    pub fn uninstall(&mut self, package_name: &str, user: Option<&str>) -> Result<()> {
        self.set_serial_transport()?;

        self.transport
            .send_adb_request(&ADBCommand::Local(ADBLocalCommand::Uninstall(
                package_name.to_string(),
                user.map(ToString::to_string),
            )))?;

        let mut data = [0; 1024];
        let read_amount = self.transport.get_raw_connection()?.read(&mut data)?;

        match &data[0..read_amount] {
            b"Success\n" => {
                log::info!("Package {package_name} successfully uninstalled");
                Ok(())
            }
            d => Err(crate::RustADBError::ADBRequestFailed(String::from_utf8(
                d.to_vec(),
            )?)),
        }
    }
}

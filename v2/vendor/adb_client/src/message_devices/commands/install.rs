use std::{fs::File, path::Path};

use crate::{
    Result,
    message_devices::{
        adb_message_device::ADBMessageDevice, adb_message_transport::ADBMessageTransport,
        commands::utils::MessageWriter, message_commands::MessageCommand,
    },
    models::ADBLocalCommand,
    utils::check_extension_is_apk,
};

impl<T: ADBMessageTransport> ADBMessageDevice<T> {
    pub(crate) fn install(&mut self, apk_path: &dyn AsRef<Path>, user: Option<&str>) -> Result<()> {
        let mut apk_file = File::open(apk_path)?;

        check_extension_is_apk(apk_path)?;

        let file_size = apk_file.metadata()?.len();

        let mut session = self.open_session(&ADBLocalCommand::Install(
            file_size,
            user.map(ToString::to_string),
        ))?;

        {
            // Read data from apk_file and write it to the underlying session
            let mut writer = MessageWriter::new(&mut session);
            std::io::copy(&mut apk_file, &mut writer)?;
        }

        let final_status = session.get_transport_mut().read_message()?;

        match final_status.into_payload().as_slice() {
            b"Success\n" => {
                log::info!(
                    "APK file {} successfully installed",
                    apk_path.as_ref().display()
                );
                self.get_transport_mut()
                    .read_message()?
                    .assert_command(MessageCommand::Clse)?;
                Ok(())
            }
            d => Err(crate::RustADBError::ADBRequestFailed(String::from_utf8(
                d.to_vec(),
            )?)),
        }
    }
}

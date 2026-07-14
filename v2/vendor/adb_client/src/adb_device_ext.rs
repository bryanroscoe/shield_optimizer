use std::io::{Read, Write};
use std::path::Path;

#[cfg(feature = "framebuffer")]
use {
    image::{ImageBuffer, ImageFormat, Rgba},
    std::io::Cursor,
};

use crate::models::{ADBListItemType, AdbStatResponse, RemountInfo};
use crate::{ADBStatExtendedResponse, RebootType, Result};

/// Trait representing all features available on ADB devices.
pub trait ADBDeviceExt {
    /// Runs command in a shell on the device, and write its output and error streams into output.
    fn shell_command(
        &mut self,
        command: &dyn AsRef<str>,
        stdout: Option<&mut dyn Write>,
        stderr: Option<&mut dyn Write>,
    ) -> Result<Option<u8>>;

    /// Starts an interactive shell session on the device.
    /// Input data is read from reader and write to writer.
    fn shell(&mut self, reader: &mut dyn Read, writer: Box<dyn Write + Send>) -> Result<()>;

    /// Runs command on the device.
    /// Input data is read from reader and write to writer.
    fn exec(
        &mut self,
        command: &str,
        reader: &mut dyn Read,
        writer: Box<dyn Write + Send>,
    ) -> Result<()>;

    /// Display the stat information for a remote file using STAT protocol command.
    fn stat(&mut self, remote_path: &dyn AsRef<str>) -> Result<AdbStatResponse>;

    /// Display the stat information for a remote file using `stat` shell command.
    /// This is an extended version of `stat` that returns more detailed information.
    /// Returns `Ok(None)` if the file does not exist on the device.
    fn stat_extended(
        &mut self,
        remote_path: &dyn AsRef<str>,
    ) -> Result<Option<ADBStatExtendedResponse>> {
        let mut stdout = Vec::new();
        self.shell_command(
            &format!("stat {}", remote_path.as_ref()),
            Some(&mut stdout),
            None,
        )?;

        // all parsing magic happens here...
        ADBStatExtendedResponse::try_from(&stdout)
    }

    /// Pull the remote file pointed to by `source` and write its contents into `output`
    fn pull(&mut self, source: &dyn AsRef<str>, output: &mut dyn Write) -> Result<()>;

    /// Push `stream` to `path` on the device.
    fn push(&mut self, stream: &mut dyn Read, path: &dyn AsRef<str>) -> Result<()>;

    /// List the items in a directory on the device
    fn list(&mut self, path: &dyn AsRef<str>) -> Result<Vec<ADBListItemType>>;

    /// Reboot the device using given reboot type
    fn reboot(&mut self, reboot_type: RebootType) -> Result<()>;

    /// Remount the device partitions as read-write
    fn remount(&mut self) -> Result<Vec<RemountInfo>>;

    /// Restart adb daemon with root permissions
    fn root(&mut self) -> Result<()>;

    /// Run `activity` from `package` on device. Return the command output.
    fn run_activity(
        &mut self,
        package: &dyn AsRef<str>,
        activity: &dyn AsRef<str>,
    ) -> Result<Vec<u8>> {
        let mut output = Vec::new();
        let _status = self.shell_command(
            &format!(
                "am start {}/{}.{}",
                package.as_ref(),
                package.as_ref(),
                activity.as_ref()
            ),
            Some(&mut output),
            None,
        )?;

        Ok(output)
    }

    /// Install an APK pointed to by `apk_path` on device.
    fn install(&mut self, apk_path: &dyn AsRef<Path>, user: Option<&str>) -> Result<()>;

    /// Uninstall the package `package` from device.
    fn uninstall(&mut self, package: &dyn AsRef<str>, user: Option<&str>) -> Result<()>;

    /// Enable dm-verity on the device
    fn enable_verity(&mut self) -> Result<()>;

    /// Disable dm-verity on the device
    fn disable_verity(&mut self) -> Result<()>;

    #[cfg(feature = "framebuffer")]
    /// Inner method requesting framebuffer from an Android device
    fn framebuffer_inner(&mut self) -> Result<ImageBuffer<Rgba<u8>, Vec<u8>>>;

    /// Dump framebuffer of this device into given path.
    ///
    /// Output data format is currently only `PNG`.
    #[cfg(feature = "framebuffer")]
    fn framebuffer(&mut self, path: &dyn AsRef<Path>) -> Result<()> {
        // Big help from AOSP source code (<https://android.googlesource.com/platform/system/adb/+/refs/heads/main/framebuffer_service.cpp>)
        let img = self.framebuffer_inner()?;
        Ok(img.save(path.as_ref())?)
    }

    /// Dump framebuffer of this device and return corresponding bytes.
    ///
    /// Output data format is currently only `PNG`.
    #[cfg(feature = "framebuffer")]
    fn framebuffer_bytes(&mut self) -> Result<Vec<u8>> {
        let img = self.framebuffer_inner()?;
        let mut vec = Cursor::new(Vec::new());
        img.write_to(&mut vec, ImageFormat::Png)?;

        Ok(vec.into_inner())
    }

    /// Return a boxed instance representing this trait
    fn boxed(self) -> Box<dyn ADBDeviceExt>
    where
        Self: Sized + 'static,
    {
        Box::new(self)
    }
}

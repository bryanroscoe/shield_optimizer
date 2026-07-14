use crate::{
    ADBDeviceExt, ADBListItemType, RebootType, Result,
    message_devices::{
        adb_message_device::ADBMessageDevice, adb_message_transport::ADBMessageTransport,
    },
    models::{AdbStatResponse, RemountInfo},
};
use std::{
    io::{Read, Write},
    path::Path,
};

impl<T: ADBMessageTransport> ADBDeviceExt for ADBMessageDevice<T> {
    #[inline]
    fn shell_command(
        &mut self,
        command: &dyn AsRef<str>,
        stdout: Option<&mut dyn Write>,
        stderr: Option<&mut dyn Write>,
    ) -> Result<Option<u8>> {
        self.shell_command(command, stdout, stderr)
    }

    #[inline]
    fn shell(&mut self, reader: &mut dyn Read, writer: Box<dyn Write + Send>) -> Result<()> {
        self.shell(reader, writer)
    }

    #[inline]
    fn exec(
        &mut self,
        command: &str,
        reader: &mut dyn Read,
        writer: Box<dyn Write + Send>,
    ) -> Result<()> {
        self.exec(command, reader, writer)
    }

    #[inline]
    fn stat(&mut self, remote_path: &dyn AsRef<str>) -> Result<AdbStatResponse> {
        self.stat(remote_path)
    }

    #[inline]
    fn pull(&mut self, source: &dyn AsRef<str>, output: &mut dyn Write) -> Result<()> {
        self.pull(source, output)
    }

    #[inline]
    fn push(&mut self, stream: &mut dyn Read, path: &dyn AsRef<str>) -> Result<()> {
        self.push(stream, path)
    }

    #[inline]
    fn reboot(&mut self, reboot_type: RebootType) -> Result<()> {
        self.reboot(reboot_type)
    }

    #[inline]
    fn remount(&mut self) -> Result<Vec<RemountInfo>> {
        self.remount()
    }

    #[inline]
    fn root(&mut self) -> Result<()> {
        self.root()
    }

    #[inline]
    fn install(&mut self, apk_path: &dyn AsRef<Path>, user: Option<&str>) -> Result<()> {
        self.install(apk_path, user)
    }

    #[inline]
    fn uninstall(&mut self, package: &dyn AsRef<str>, user: Option<&str>) -> Result<()> {
        self.uninstall(package, user)
    }

    #[inline]
    fn enable_verity(&mut self) -> Result<()> {
        self.enable_verity()
    }

    #[inline]
    fn disable_verity(&mut self) -> Result<()> {
        self.disable_verity()
    }

    #[inline]
    #[cfg(feature = "framebuffer")]
    fn framebuffer_inner(&mut self) -> Result<image::ImageBuffer<image::Rgba<u8>, Vec<u8>>> {
        self.framebuffer_inner()
    }

    #[inline]
    fn list(&mut self, path: &dyn AsRef<str>) -> Result<Vec<ADBListItemType>> {
        self.list(path)
    }
}

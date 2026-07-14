use std::io::Write;
use std::path::Path;
use std::{io::Read, net::SocketAddr};

use crate::message_devices::adb_message_device::ADBMessageDevice;
use crate::models::RemountInfo;
use crate::tcp::tcp_transport::TcpTransport;
use crate::utils::get_default_adb_key_path;
use crate::{ADBDeviceExt, ADBListItemType, Result};

/// Represent a device reached and available over TCP.
#[derive(Debug)]
pub struct ADBTcpDevice {
    inner: ADBMessageDevice<TcpTransport>,
}

impl ADBTcpDevice {
    /// Instantiate a new [`ADBTcpDevice`]
    pub fn new<A: Into<SocketAddr>>(address: A) -> Result<Self> {
        Self::new_with_custom_private_key(address, get_default_adb_key_path()?)
    }

    /// Instantiate a new [`ADBTcpDevice`] using a custom private key path
    pub fn new_with_custom_private_key<P: AsRef<Path>, A: Into<SocketAddr>>(
        address: A,
        private_key_path: P,
    ) -> Result<Self> {
        Ok(Self {
            inner: ADBMessageDevice::new(
                TcpTransport::new(address, &private_key_path),
                private_key_path,
            )?,
        })
    }
}

impl ADBDeviceExt for ADBTcpDevice {
    #[inline]
    fn shell_command(
        &mut self,
        command: &dyn AsRef<str>,
        stdout: Option<&mut dyn Write>,
        stderr: Option<&mut dyn Write>,
    ) -> Result<Option<u8>> {
        self.inner.shell_command(command, stdout, stderr)
    }

    #[inline]
    fn shell(&mut self, reader: &mut dyn Read, writer: Box<dyn Write + Send>) -> Result<()> {
        self.inner.shell(reader, writer)
    }

    #[inline]
    fn stat(&mut self, remote_path: &dyn AsRef<str>) -> Result<crate::AdbStatResponse> {
        self.inner.stat(remote_path)
    }

    #[inline]
    fn pull(&mut self, source: &dyn AsRef<str>, output: &mut dyn Write) -> Result<()> {
        self.inner.pull(source, output)
    }

    #[inline]
    fn push(&mut self, stream: &mut dyn Read, path: &dyn AsRef<str>) -> Result<()> {
        self.inner.push(stream, path)
    }

    #[inline]
    fn reboot(&mut self, reboot_type: crate::RebootType) -> Result<()> {
        self.inner.reboot(reboot_type)
    }

    #[inline]
    fn remount(&mut self) -> Result<Vec<RemountInfo>> {
        self.inner.remount()
    }

    #[inline]
    fn root(&mut self) -> Result<()> {
        self.inner.root()
    }

    #[inline]
    fn install(&mut self, apk_path: &dyn AsRef<Path>, user: Option<&str>) -> Result<()> {
        self.inner.install(apk_path, user)
    }

    #[inline]
    fn uninstall(&mut self, package: &dyn AsRef<str>, user: Option<&str>) -> Result<()> {
        self.inner.uninstall(package, user)
    }

    #[inline]
    fn enable_verity(&mut self) -> Result<()> {
        self.inner.enable_verity()
    }

    #[inline]
    fn disable_verity(&mut self) -> Result<()> {
        self.inner.disable_verity()
    }

    #[inline]
    #[cfg(feature = "framebuffer")]
    fn framebuffer_inner(&mut self) -> Result<image::ImageBuffer<image::Rgba<u8>, Vec<u8>>> {
        self.inner.framebuffer_inner()
    }

    #[inline]
    fn list(&mut self, path: &dyn AsRef<str>) -> Result<Vec<ADBListItemType>> {
        self.inner.list(path)
    }

    #[inline]
    fn exec(
        &mut self,
        command: &str,
        reader: &mut dyn Read,
        writer: Box<dyn Write + Send>,
    ) -> Result<()> {
        self.inner.exec(command, reader, writer)
    }
}

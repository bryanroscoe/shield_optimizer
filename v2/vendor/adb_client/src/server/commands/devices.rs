use std::io::Read;

use crate::{
    Result, RustADBError,
    emulator::ADBEmulatorDevice,
    models::{ADBCommand, ADBHostCommand},
    server::{ADBServer, DeviceLong, DeviceShort},
    server_device::ADBServerDevice,
};

impl ADBServer {
    /// Gets a list of connected devices.
    pub fn devices(&mut self) -> Result<Vec<DeviceShort>> {
        let devices = self
            .connect()?
            .proxy_connection(&ADBCommand::Host(ADBHostCommand::Devices), true)?;

        let mut vec_devices: Vec<DeviceShort> = vec![];
        for device in devices.split(|x| x.eq(&b'\n')) {
            if device.is_empty() {
                break;
            }

            vec_devices.push(DeviceShort::try_from(device.to_vec())?);
        }

        Ok(vec_devices)
    }

    /// Gets an extended list of connected devices including the device paths in the state.
    pub fn devices_long(&mut self) -> Result<Vec<DeviceLong>> {
        let devices_long = self
            .connect()?
            .proxy_connection(&ADBCommand::Host(ADBHostCommand::DevicesLong), true)?;

        let mut vec_devices: Vec<DeviceLong> = vec![];
        for device in devices_long.split(|x| x.eq(&b'\n')) {
            if device.is_empty() {
                break;
            }

            vec_devices.push(DeviceLong::try_from(device)?);
        }

        Ok(vec_devices)
    }

    /// Get a device, assuming that only this device is connected.
    pub fn get_device(&mut self) -> Result<ADBServerDevice> {
        let mut devices = self.devices()?.into_iter();
        match devices.next() {
            Some(device) => match devices.next() {
                Some(_) => Err(RustADBError::DeviceNotFound(
                    "too many devices connected".to_string(),
                )),
                None => Ok(ADBServerDevice::new(device.identifier, self.socket_addr)),
            },
            None => Err(RustADBError::DeviceNotFound(
                "no device connected".to_string(),
            )),
        }
    }

    /// Get a device matching the given name, if existing.
    /// - There is no device connected => Error
    /// - There is a single device connected => Ok
    /// - There are more than 1 device connected => Error
    pub fn get_device_by_name(&mut self, name: &str) -> Result<ADBServerDevice> {
        let nb_devices = self
            .devices()?
            .into_iter()
            .filter(|d| d.identifier.as_str() == name)
            .count();
        if nb_devices == 1 {
            Ok(ADBServerDevice::new(name.to_string(), self.socket_addr))
        } else {
            Err(RustADBError::DeviceNotFound(format!(
                "could not find device {name}"
            )))
        }
    }

    /// Get a device matching the given transport id (as returned by `adb devices -l`).
    ///
    /// Transport ids are unique within a running ADB server and disambiguate devices that
    /// share the same serial number. They are reassigned on device reconnect or server
    /// restart, so callers should re-query rather than caching the id.
    pub fn get_device_by_transport_id(&mut self, transport_id: u32) -> Result<ADBServerDevice> {
        let nb_devices = self
            .devices_long()?
            .into_iter()
            .filter(|d| d.transport_id == transport_id)
            .count();
        if nb_devices == 1 {
            Ok(ADBServerDevice::new_with_transport_id(
                transport_id,
                self.socket_addr,
            ))
        } else {
            Err(RustADBError::DeviceNotFound(format!(
                "could not find device with transport id {transport_id}"
            )))
        }
    }

    /// Tracks new devices showing up.
    pub fn track_devices(&mut self, callback: impl Fn(DeviceShort) -> Result<()>) -> Result<()> {
        self.connect()?
            .send_adb_request(&ADBCommand::Host(ADBHostCommand::TrackDevices))?;

        loop {
            let length = self.get_transport()?.get_hex_body_length()?;

            if length > 0 {
                let mut body = vec![
                    0;
                    length
                        .try_into()
                        .map_err(|_| RustADBError::ConversionError)?
                ];
                self.get_transport()?
                    .get_raw_connection()?
                    .read_exact(&mut body)?;

                for device in body.split(|x| x.eq(&b'\n')) {
                    if device.is_empty() {
                        break;
                    }
                    callback(DeviceShort::try_from(device.to_vec())?)?;
                }
            }
        }
    }

    /// Get an emulator, assuming that only this device is connected.
    pub fn get_emulator_device(&mut self) -> Result<ADBEmulatorDevice> {
        let device = self.get_device()?;

        ADBEmulatorDevice::try_from(device)
    }

    /// Get an emulator by its name
    pub fn get_emulator_device_by_name(&mut self, name: &str) -> Result<ADBEmulatorDevice> {
        let device = self.get_device_by_name(name)?;

        ADBEmulatorDevice::try_from(device)
    }
}

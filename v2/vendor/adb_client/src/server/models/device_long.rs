use std::str::FromStr;
use std::sync::LazyLock;
use std::{fmt::Display, str};

use crate::RustADBError;
use crate::server::DeviceState;
use regex::bytes::Regex;

static DEVICES_LONG_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?P<identifier>\S+)\s+(?P<state>\w+)\s+(usb:(?P<usb1>\S+)|(?P<usb2>\S+))?\s*(product:(?P<product>.*)\s+model:(?P<model>.*)\s+device:(?P<device>\S+)\s+)?transport_id:(?P<transport_id>\d+)$").expect("cannot build devices long regex")
});

/// Represents a new device with more informations.
#[derive(Debug, PartialEq, Eq)]
pub struct DeviceLong {
    /// Unique device identifier.
    pub identifier: String,
    /// Connection state of the device.
    pub state: DeviceState,
    /// Usb port used by the device.
    pub usb: String,
    /// Product code.
    pub product: String,
    /// Device model.
    pub model: String,
    /// Device code.
    pub device: String,
    /// Transport identifier.
    pub transport_id: u32,
}

impl Display for DeviceLong {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}\t{} usb:{} product:{} model:{} device:{} transport_id:{}",
            self.identifier,
            self.state,
            self.usb,
            self.product,
            self.model,
            self.device,
            self.transport_id
        )
    }
}

impl TryFrom<&[u8]> for DeviceLong {
    type Error = RustADBError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        let groups = DEVICES_LONG_REGEX
            .captures(value)
            .ok_or(RustADBError::RegexParsingError)?;

        Ok(Self {
            identifier: String::from_utf8(
                groups
                    .name("identifier")
                    .ok_or(RustADBError::RegexParsingError)?
                    .as_bytes()
                    .to_vec(),
            )?,
            state: DeviceState::from_str(&String::from_utf8(
                groups
                    .name("state")
                    .ok_or(RustADBError::RegexParsingError)?
                    .as_bytes()
                    .to_vec(),
            )?)?,
            usb: match groups.name("usb1") {
                None => match groups.name("usb2") {
                    None => "Unk".to_string(),
                    Some(usb) => String::from_utf8(usb.as_bytes().to_vec())?,
                },
                Some(usb) => String::from_utf8(usb.as_bytes().to_vec())?,
            },
            product: match groups.name("product") {
                None => "Unk".to_string(),
                Some(product) => String::from_utf8(product.as_bytes().to_vec())?,
            },
            model: match groups.name("model") {
                None => "Unk".to_string(),
                Some(model) => String::from_utf8(model.as_bytes().to_vec())?,
            },
            device: match groups.name("device") {
                None => "Unk".to_string(),
                Some(device) => String::from_utf8(device.as_bytes().to_vec())?,
            },
            transport_id: (str::from_utf8(
                groups
                    .name("transport_id")
                    .ok_or(RustADBError::RegexParsingError)?
                    .as_bytes(),
            )?)
            .parse::<u32>()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::server::{DeviceLong, DeviceState};

    #[test]
    fn test_devices_long_parsing() {
        let inputs = [
            (
                "7a5158f05122195aa       device 1-5 product:gts210vewifixx model:SM_T813 device:gts210vewifi transport_id:4",
                DeviceLong {
                    identifier: "7a5158f05122195aa".to_string(),
                    state: DeviceState::Device,
                    usb: "1-5".to_string(),
                    product: "gts210vewifixx".to_string(),
                    model: "SM_T813".to_string(),
                    device: "gts210vewifi".to_string(),
                    transport_id: 4,
                },
            ),
            (
                "n311r05e               device usb:0-1.5 product:alioth model:M2012K11AC device:alioth transport_id:58",
                DeviceLong {
                    identifier: "n311r05e".to_string(),
                    state: DeviceState::Device,
                    usb: "0-1.5".to_string(),
                    product: "alioth".to_string(),
                    model: "M2012K11AC".to_string(),
                    device: "alioth".to_string(),
                    transport_id: 58,
                },
            ),
            (
                "192.168.100.192:5555   device product:alioth model:M2012K11AC device:alioth transport_id:97",
                DeviceLong {
                    identifier: "192.168.100.192:5555".to_string(),
                    state: DeviceState::Device,
                    usb: "Unk".to_string(),
                    product: "alioth".to_string(),
                    model: "M2012K11AC".to_string(),
                    device: "alioth".to_string(),
                    transport_id: 97,
                },
            ),
            (
                "emulator-5554          device product:sdk_gphone64_arm64 model:sdk_gphone64_arm64 device:emu64a transport_id:101",
                DeviceLong {
                    identifier: "emulator-5554".to_string(),
                    state: DeviceState::Device,
                    usb: "Unk".to_string(),
                    product: "sdk_gphone64_arm64".to_string(),
                    model: "sdk_gphone64_arm64".to_string(),
                    device: "emu64a".to_string(),
                    transport_id: 101,
                },
            ),
            (
                "QQ20131020250511       device 20-4 product:NOH-AN00 model:NOH_AN00 device:HWNOH transport_id:3",
                DeviceLong {
                    identifier: "QQ20131020250511".to_string(),
                    state: DeviceState::Device,
                    usb: "20-4".to_string(),
                    product: "NOH-AN00".to_string(),
                    model: "NOH_AN00".to_string(),
                    device: "HWNOH".to_string(),
                    transport_id: 3,
                },
            ),
            (
                "192.168.100.192:5555     device product:m425 plus-a model:M425_PLUS_A device:m450a transport_id:1",
                DeviceLong {
                    identifier: "192.168.100.192:5555".to_string(),
                    state: DeviceState::Device,
                    usb: "Unk".to_string(),
                    product: "m425 plus-a".to_string(),
                    model: "M425_PLUS_A".to_string(),
                    device: "m450a".to_string(),
                    transport_id: 1,
                },
            ),
            (
                "192.168.100.192:5555     device product:m425 plus-a model:M425 PLUS A device:m450a transport_id:1",
                DeviceLong {
                    identifier: "192.168.100.192:5555".to_string(),
                    state: DeviceState::Device,
                    usb: "Unk".to_string(),
                    product: "m425 plus-a".to_string(),
                    model: "M425 PLUS A".to_string(),
                    device: "m450a".to_string(),
                    transport_id: 1,
                },
            ),
        ];
        for (input, expected_device) in inputs {
            let device = DeviceLong::try_from(input.as_bytes())
                .unwrap_or_else(|_| panic!("cannot parse input: '{input}'"));
            assert_eq!(
                device, expected_device,
                "parsed device does not match expected"
            );
        }
    }
}

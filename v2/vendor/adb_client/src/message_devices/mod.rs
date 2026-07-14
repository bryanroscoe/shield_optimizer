/// USB-related definitions
#[cfg(feature = "usb")]
#[cfg_attr(docsrs, doc(cfg(feature = "usb")))]
pub mod usb;

/// Device reachable over TCP related definition
pub mod tcp;

mod adb_message_device;
mod adb_message_device_commands;
mod adb_message_transport;
mod adb_service;
mod adb_session;
mod adb_transport_message;
mod commands;
mod message_commands;
mod models;
mod utils;

pub use utils::BinaryDecodable;

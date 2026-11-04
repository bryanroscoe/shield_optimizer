use crate::message_devices::adb_message_transport::ADBMessageTransport;

/// The ids of one logical stream between this host and `adbd`, and the
/// transport it runs on. Reading and writing it goes through
/// [`super::adb_service::ADBService`], which checks the ids on every packet.
#[derive(Debug)]
pub struct ADBSession<T: ADBMessageTransport> {
    transport: T,
    local_id: u32,
    remote_id: u32,
}

impl<T: ADBMessageTransport> ADBSession<T> {
    pub const fn new(transport: T, local_id: u32, remote_id: u32) -> Self {
        Self {
            transport,
            local_id,
            remote_id,
        }
    }

    pub const fn get_transport_mut(&mut self) -> &mut T {
        &mut self.transport
    }

    pub const fn transport(&self) -> &T {
        &self.transport
    }

    pub const fn local_id(&self) -> u32 {
        self.local_id
    }

    pub const fn remote_id(&self) -> u32 {
        self.remote_id
    }
}

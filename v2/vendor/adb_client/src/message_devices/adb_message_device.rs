// Derived in part from Kadb (Apache-2.0): core/AdbConnection.kt.

use rand::RngExt;
use std::{
    path::Path,
    time::{Duration, Instant},
};

use crate::{
    Result, RustADBError,
    message_devices::adb_message_transport::DEFAULT_READ_TIMEOUT,
    message_devices::{
        adb_connection_info::{
            A_STLS_VERSION, A_VERSION, ConnectionInfo, MAX_PAYLOAD, host_banner,
        },
        adb_message_transport::ADBMessageTransport,
        adb_service::open_service_session,
        adb_session::ADBSession,
        adb_transport_message::{
            ADBTransportMessage, AUTH_RSAPUBLICKEY, AUTH_SIGNATURE, AUTH_TOKEN,
        },
        message_commands::{MessageCommand, MessageSubcommand},
        models::{ADBRsaKey, read_adb_private_key},
        utils::BinaryEncodable,
    },
    models::ADBLocalCommand,
};

/// Bound on the whole handshake when the caller does not choose one.
pub(crate) const DEFAULT_AUTH_TIMEOUT: Duration = Duration::from_secs(10);

/// One token is answered with our signature, every later one with our public
/// key, as AOSP's host does. A device that keeps asking past this is not
/// going to accept us.
const MAX_AUTH_TOKENS: usize = 4;

/// Generic structure representing an ADB device reachable over an [`ADBMessageTransport`].
/// Structure is totally agnostic over which transport is truly used.
#[derive(Debug)]
pub struct ADBMessageDevice<T: ADBMessageTransport> {
    transport: T,
    connection_info: ConnectionInfo,
}

impl<T: ADBMessageTransport> ADBMessageDevice<T> {
    /// Instantiate a new [`ADBMessageTransport`]. The connection handshake is
    /// bounded by a 10-second default.
    pub fn new<P: AsRef<Path>>(transport: T, adb_private_key_path: P) -> Result<Self> {
        Self::new_inner(transport, adb_private_key_path, DEFAULT_AUTH_TIMEOUT)
    }

    /// Instantiate a new [`ADBMessageTransport`] whose whole connection
    /// handshake, including the TLS upgrade and waiting for the user to
    /// approve the RSA key, must finish within `auth_timeout`.
    pub fn new_with_auth_timeout<P: AsRef<Path>>(
        transport: T,
        adb_private_key_path: P,
        auth_timeout: Duration,
    ) -> Result<Self> {
        Self::new_inner(transport, adb_private_key_path, auth_timeout)
    }

    fn new_inner<P: AsRef<Path>>(
        transport: T,
        adb_private_key_path: P,
        auth_timeout: Duration,
    ) -> Result<Self> {
        let private_key = if let Some(private_key) = read_adb_private_key(&adb_private_key_path)? {
            private_key
        } else {
            log::warn!(
                "No private key found at path {}. Generating a new random.",
                adb_private_key_path.as_ref().display()
            );
            ADBRsaKey::new_random()?
        };

        let mut message_device = Self {
            transport,
            connection_info: ConnectionInfo::unnegotiated(),
        };
        message_device.connect(&private_key, auth_timeout)?;

        Ok(message_device)
    }

    pub(crate) const fn get_transport_mut(&mut self) -> &mut T {
        &mut self.transport
    }

    /// What the CNXN exchange negotiated with this device.
    pub(crate) const fn connection_info(&self) -> &ConnectionInfo {
        &self.connection_info
    }

    fn read_before(&mut self, deadline: Instant) -> Result<ADBTransportMessage> {
        let timed_out = || {
            RustADBError::IOError(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "ADB connection handshake timed out",
            ))
        };
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(timed_out());
        }
        // A socket read timeout surfaces as WouldBlock on Unix and TimedOut on
        // Windows; report both the same way.
        match self.transport.read_message_with_timeout(remaining) {
            Err(RustADBError::IOError(error))
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                Err(timed_out())
            }
            other => other,
        }
    }

    /// Run the CNXN handshake as Kadb's `AdbConnection.connect` does: answer
    /// STLS and AUTH until the device sends CNXN, then negotiate from it.
    /// Everything shares one deadline, so a device that trickles replies
    /// cannot stretch the wait. The socket is fresh, so a stream packet
    /// (OKAY, WRTE, CLSE, OPEN) here can only be a broken peer and fails the
    /// handshake rather than being skipped.
    fn connect(&mut self, private_key: &ADBRsaKey, auth_timeout: Duration) -> Result<()> {
        self.transport.connect()?;
        let deadline = Instant::now() + auth_timeout;

        self.transport.write_message(ADBTransportMessage::try_new(
            MessageCommand::Cnxn,
            A_VERSION,
            MAX_PAYLOAD,
            &host_banner(),
        )?)?;

        let mut upgraded = false;
        let mut auth_tokens = 0_usize;
        let mut message = self.read_before(deadline)?;
        loop {
            match message.header().command() {
                MessageCommand::Cnxn => break,
                MessageCommand::Stls => {
                    if upgraded {
                        return Err(RustADBError::ProtocolError(
                            "device requested a second TLS upgrade".to_string(),
                        ));
                    }
                    self.transport.write_message(ADBTransportMessage::try_new(
                        MessageCommand::Stls,
                        A_STLS_VERSION,
                        0,
                        &[],
                    )?)?;
                    let remaining = deadline.saturating_duration_since(Instant::now());
                    self.transport.upgrade_connection(remaining)?;
                    upgraded = true;
                    log::debug!("Connection successfully upgraded from TCP to TLS");
                }
                MessageCommand::Auth => {
                    auth_tokens += 1;
                    self.answer_auth(message, private_key, auth_tokens)?;
                }
                other => {
                    return Err(RustADBError::WrongResponseReceived(
                        other.to_string(),
                        "CNXN, STLS or AUTH".to_string(),
                    ));
                }
            }
            message = self.read_before(deadline)?;
        }

        let header = message.header();
        let info = ConnectionInfo::negotiate(header.arg0(), header.arg1(), message.payload())?;
        self.transport.apply_connection_info(&info);
        log::info!(
            "Connected: protocol {:#010x}, max payload {}, features {:?}, device banner {}",
            info.protocol_version(),
            info.max_payload(),
            info.features(),
            String::from_utf8_lossy(message.payload())
        );
        self.connection_info = info;
        Ok(())
    }

    fn answer_auth(
        &mut self,
        message: ADBTransportMessage,
        private_key: &ADBRsaKey,
        auth_tokens: usize,
    ) -> Result<()> {
        if message.header().arg0() != AUTH_TOKEN {
            return Err(RustADBError::ADBRequestFailed(format!(
                "Received AUTH message with type != 1 ({})",
                message.header().arg0()
            )));
        }
        if auth_tokens > MAX_AUTH_TOKENS {
            return Err(RustADBError::ProtocolError(format!(
                "device sent {auth_tokens} AUTH tokens without accepting our key"
            )));
        }

        let reply = if auth_tokens == 1 {
            log::debug!("Authentication required");
            let sign = private_key.sign(message.into_payload())?;
            ADBTransportMessage::try_new(MessageCommand::Auth, AUTH_SIGNATURE, 0, &sign)?
        } else {
            // The device did not recognise our signature; offer the key so it
            // can show its "Allow USB debugging?" prompt.
            let mut pubkey = private_key.android_pubkey_encode()?.into_bytes();
            pubkey.push(b'\0');
            ADBTransportMessage::try_new(MessageCommand::Auth, AUTH_RSAPUBLICKEY, 0, &pubkey)?
        };
        self.transport.write_message(reply)
    }

    pub(crate) fn open_synchronization_session(&mut self) -> Result<ADBSession<T>> {
        self.open_session(&ADBLocalCommand::Sync)
    }

    pub(crate) fn open_raw_service(
        &mut self,
        service: &str,
        timeout: Duration,
    ) -> Result<ADBSession<T>> {
        let mut rng = rand::rng();
        let local_id = loop {
            let candidate = rng.random();
            if candidate != 0 {
                break candidate;
            }
        };
        open_service_session(self.get_transport_mut(), service, local_id, timeout)
    }

    pub(crate) fn open_session(&mut self, cmd: &ADBLocalCommand) -> Result<ADBSession<T>> {
        self.open_session_with_timeout(cmd, DEFAULT_READ_TIMEOUT)
    }

    pub(crate) fn open_session_with_timeout(
        &mut self,
        cmd: &ADBLocalCommand,
        timeout: Duration,
    ) -> Result<ADBSession<T>> {
        let mut rng = rand::rng();
        // Zero is reserved for a failed OPEN and must never be a real local id.
        let local_id: u32 = loop {
            let candidate = rng.random();
            if candidate != 0 {
                break candidate;
            }
        };

        let message = ADBTransportMessage::try_new(
            MessageCommand::Open,
            local_id, // Our 'local-id'
            0,
            cmd.to_string().as_bytes(),
        )?;
        self.transport.write_message(message)?;

        // A late CLSE/WRTE from the previous stream may still be in flight;
        // skip a bounded number of messages that are not addressed to us.
        let mut skipped = 0usize;
        let response = loop {
            let candidate = self.transport.read_message_with_timeout(timeout)?;
            if candidate.header().arg1() == local_id {
                break candidate;
            }
            skipped += 1;
            if skipped > 64 {
                return Err(RustADBError::ADBRequestFailed(
                    "Open session failed: too many messages for other streams".to_string(),
                ));
            }
        };

        if response.header().command() != MessageCommand::Okay {
            return Err(RustADBError::ADBRequestFailed(format!(
                "Open session failed: got {} in response instead of OKAY",
                response.header().command()
            )));
        }

        if response.header().arg1() != local_id {
            return Err(RustADBError::ADBRequestFailed(format!(
                "Open session failed: responses used {} for our local_id instead of {local_id}",
                response.header().arg1()
            )));
        }

        Ok(ADBSession::new(
            self.transport.clone(),
            local_id,
            response.header().arg0(),
        ))
    }

    pub(crate) fn end_transaction(&mut self, session: &mut ADBSession<T>) -> Result<()> {
        let quit_buffer = MessageSubcommand::Quit.with_arg(0u32);
        session.send_and_expect_okay(ADBTransportMessage::try_new(
            MessageCommand::Write,
            session.local_id(),
            session.remote_id(),
            &quit_buffer.encode(),
        )?)?;

        let _discard_close = self.transport.read_message()?;
        Ok(())
    }
}

impl<T: ADBMessageTransport> Drop for ADBMessageDevice<T> {
    fn drop(&mut self) {
        // Best effort here
        let _ = self.get_transport_mut().disconnect();
    }
}

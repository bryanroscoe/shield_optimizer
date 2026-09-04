use rand::RngExt;
use std::{path::Path, time::Duration};

use crate::{
    Result, RustADBError,
    message_devices::adb_message_transport::DEFAULT_READ_TIMEOUT,
    message_devices::{
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

const DEFAULT_AUTH_TIMEOUT: Duration = Duration::from_secs(10);

/// Generic structure representing an ADB device reachable over an [`ADBMessageTransport`].
/// Structure is totally agnostic over which transport is truly used.
#[derive(Debug)]
pub struct ADBMessageDevice<T: ADBMessageTransport> {
    transport: T,
}

impl<T: ADBMessageTransport> ADBMessageDevice<T> {
    /// Instantiate a new [`ADBMessageTransport`]
    pub fn new<P: AsRef<Path>>(transport: T, adb_private_key_path: P) -> Result<Self> {
        Self::new_inner(transport, adb_private_key_path, None)
    }

    /// Instantiate a new [`ADBMessageTransport`] with a finite timeout for each
    /// connection and RSA-authorization response.
    pub fn new_with_auth_timeout<P: AsRef<Path>>(
        transport: T,
        adb_private_key_path: P,
        auth_timeout: Duration,
    ) -> Result<Self> {
        Self::new_inner(transport, adb_private_key_path, Some(auth_timeout))
    }

    fn new_inner<P: AsRef<Path>>(
        transport: T,
        adb_private_key_path: P,
        auth_timeout: Option<Duration>,
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

        let mut message_device = Self { transport };
        message_device.connect(&private_key, auth_timeout)?;

        Ok(message_device)
    }

    pub(crate) const fn get_transport_mut(&mut self) -> &mut T {
        &mut self.transport
    }

    /// Send initial connect
    fn connect(&mut self, private_key: &ADBRsaKey, auth_timeout: Option<Duration>) -> Result<()> {
        self.get_transport_mut().connect()?;

        let message = ADBTransportMessage::try_new(
            MessageCommand::Cnxn,
            0x0100_0000,
            1_048_576,
            format!("host::{}\0", env!("CARGO_PKG_NAME")).as_bytes(),
        )?;

        self.get_transport_mut().write_message(message)?;

        let message = match auth_timeout {
            Some(timeout) => self
                .get_transport_mut()
                .read_message_with_timeout(timeout)?,
            None => self.get_transport_mut().read_message()?,
        };

        // Check if a client is requesting a secure connection and upgrade it if necessary
        match message.header().command() {
            MessageCommand::Stls => {
                self.get_transport_mut()
                    .write_message(ADBTransportMessage::try_new(
                        MessageCommand::Stls,
                        1,
                        0,
                        &[],
                    )?)?;
                self.get_transport_mut().upgrade_connection()?;
                log::debug!("Connection successfully upgraded from TCP to TLS");
                Ok(())
            }
            MessageCommand::Cnxn => {
                log::debug!("Unencrypted connection established");
                Ok(())
            }
            MessageCommand::Auth => {
                log::debug!("Authentication required");
                self.auth_handshake(message, private_key, auth_timeout)
            }
            _ => Err(crate::RustADBError::WrongResponseReceived(
                "Expected CNXN, STLS or AUTH command".to_string(),
                message.header().command().to_string(),
            )),
        }
    }

    fn auth_handshake(
        &mut self,
        message: ADBTransportMessage,
        private_key: &ADBRsaKey,
        auth_timeout: Option<Duration>,
    ) -> Result<()> {
        match message.header().command() {
            MessageCommand::Auth => {
                log::debug!("Authentication required");
            }
            _ => return Ok(()),
        }

        // At this point, we should have received an AUTH message with arg0 == 1
        let auth_message = match message.header().arg0() {
            AUTH_TOKEN => message,
            v => {
                return Err(RustADBError::ADBRequestFailed(format!(
                    "Received AUTH message with type != 1 ({v})"
                )));
            }
        };

        let sign = private_key.sign(auth_message.into_payload())?;

        let message = ADBTransportMessage::try_new(MessageCommand::Auth, AUTH_SIGNATURE, 0, &sign)?;

        self.transport.write_message(message)?;

        let received_response = match auth_timeout {
            Some(timeout) => self.transport.read_message_with_timeout(timeout)?,
            None => self.transport.read_message()?,
        };

        if received_response.header().command() == MessageCommand::Cnxn {
            log::info!(
                "Authentication OK, device info {}",
                String::from_utf8(received_response.into_payload())?
            );
            return Ok(());
        }

        let mut pubkey = private_key.android_pubkey_encode()?.into_bytes();
        pubkey.push(b'\0');

        let message =
            ADBTransportMessage::try_new(MessageCommand::Auth, AUTH_RSAPUBLICKEY, 0, &pubkey)?;

        self.transport.write_message(message)?;

        let response = self
            .transport
            .read_message_with_timeout(auth_timeout.unwrap_or(DEFAULT_AUTH_TIMEOUT))
            .and_then(|message| {
                message.assert_command(MessageCommand::Cnxn)?;
                Ok(message)
            })?;

        log::info!(
            "Authentication OK, device info {}",
            String::from_utf8(response.into_payload())?
        );
        Ok(())
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

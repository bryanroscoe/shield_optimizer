// Derived in part from Kadb (Apache-2.0): core/AdbConnection.kt.

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
        adb_message_transport::{ADBMessageTransport, is_timeout},
        adb_service::{
            ADBService, OpenFailure, STREAM_WRITE_TIMEOUT, open_service_session, open_stream,
            open_stream_classified,
        },
        adb_session::ADBSession,
        adb_transport_message::{
            ADBTransportMessage, AUTH_RSAPUBLICKEY, AUTH_SIGNATURE, AUTH_TOKEN,
        },
        message_commands::MessageCommand,
        models::{ADBRsaKey, read_adb_private_key},
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
    /// Next local stream id. Ids count up from 1 like adbd's and are never
    /// reused on a connection, so a late packet for a finished stream can
    /// never be taken for a newer one.
    next_local_id: u32,
}

impl<T: ADBMessageTransport> ADBMessageDevice<T> {
    /// Instantiate a new [`ADBMessageTransport`]. The connection handshake is
    /// bounded by a 10-second default.
    pub fn new<P: AsRef<Path>>(transport: T, adb_private_key_path: P) -> Result<Self> {
        Self::new_inner(
            transport,
            adb_private_key_path,
            DEFAULT_AUTH_TIMEOUT,
            false,
            None,
        )
    }

    /// Instantiate a new [`ADBMessageTransport`] whose whole connection
    /// handshake, including the TLS upgrade and waiting for the user to
    /// approve the RSA key, must finish within `auth_timeout`.
    pub fn new_with_auth_timeout<P: AsRef<Path>>(
        transport: T,
        adb_private_key_path: P,
        auth_timeout: Duration,
    ) -> Result<Self> {
        Self::new_inner(transport, adb_private_key_path, auth_timeout, false, None)
    }

    /// Like [`Self::new_with_auth_timeout`], but the key at
    /// `adb_private_key_path` must already exist: a missing key is
    /// `IOError(NotFound)` before anything is sent, never a new random key.
    pub(crate) fn new_with_required_key<P: AsRef<Path>>(
        transport: T,
        adb_private_key_path: P,
        auth_timeout: Duration,
    ) -> Result<Self> {
        Self::new_inner(transport, adb_private_key_path, auth_timeout, true, None)
    }

    /// Connect, calling `public_key_sent` once our RSA public key has been
    /// written in answer to an AUTH token: from then on the device may be
    /// showing its "Allow debugging?" prompt. It does not run for a key the
    /// device already trusts, over TLS, or when that write fails.
    pub(crate) fn new_with_auth_observer<P: AsRef<Path>>(
        transport: T,
        adb_private_key_path: P,
        auth_timeout: Duration,
        require_existing_key: bool,
        public_key_sent: &dyn Fn(),
    ) -> Result<Self> {
        Self::new_inner(
            transport,
            adb_private_key_path,
            auth_timeout,
            require_existing_key,
            Some(public_key_sent),
        )
    }

    /// Connect with an in-memory key, for tests that must not read or write a
    /// key file.
    #[cfg(test)]
    pub(crate) fn new_with_fixture_key(
        transport: T,
        private_key: &ADBRsaKey,
        auth_timeout: Duration,
    ) -> Result<Self> {
        let mut device = Self::unconnected(transport);
        device.connect(private_key, auth_timeout, None)?;
        Ok(device)
    }

    fn unconnected(transport: T) -> Self {
        Self {
            transport,
            connection_info: ConnectionInfo::unnegotiated(),
            next_local_id: 1,
        }
    }

    fn new_inner<P: AsRef<Path>>(
        transport: T,
        adb_private_key_path: P,
        auth_timeout: Duration,
        require_existing_key: bool,
        public_key_sent: Option<&dyn Fn()>,
    ) -> Result<Self> {
        let private_key = if let Some(private_key) = read_adb_private_key(&adb_private_key_path)? {
            private_key
        } else if require_existing_key {
            return Err(RustADBError::IOError(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "the required ADB key is missing; no replacement was generated",
            )));
        } else {
            log::warn!(
                "No private key found at path {}. Generating a new random.",
                adb_private_key_path.as_ref().display()
            );
            ADBRsaKey::new_random()?
        };

        let mut message_device = Self::unconnected(transport);
        message_device.connect(&private_key, auth_timeout, public_key_sent)?;

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
        match self.transport.read_message_with_timeout(remaining) {
            Err(RustADBError::IOError(error)) if is_timeout(error.kind()) => Err(timed_out()),
            other => other,
        }
    }

    /// Run the CNXN handshake as Kadb's `AdbConnection.connect` does: answer
    /// STLS and AUTH until the device sends CNXN, then negotiate from it.
    /// Everything shares one deadline, so a device that trickles replies
    /// cannot stretch the wait. The socket is fresh, so a stream packet
    /// (OKAY, WRTE, CLSE, OPEN) here can only be a broken peer and fails the
    /// handshake rather than being skipped.
    fn connect(
        &mut self,
        private_key: &ADBRsaKey,
        auth_timeout: Duration,
        public_key_sent: Option<&dyn Fn()>,
    ) -> Result<()> {
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
        let mut public_key_reported = false;
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
                    let offered_public_key = self.answer_auth(message, private_key, auth_tokens)?;
                    if offered_public_key
                        && !public_key_reported
                        && let Some(observe) = public_key_sent
                    {
                        public_key_reported = true;
                        observe();
                    }
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

    /// Answer one AUTH token. Returns whether the answer was our public key.
    fn answer_auth(
        &mut self,
        message: ADBTransportMessage,
        private_key: &ADBRsaKey,
        auth_tokens: usize,
    ) -> Result<bool> {
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
        self.transport.write_message(reply)?;
        Ok(auth_tokens > 1)
    }

    /// Zero is reserved for a failed OPEN and is skipped on wrap-around.
    const fn allocate_local_id(&mut self) -> u32 {
        let id = self.next_local_id;
        self.next_local_id = match self.next_local_id.checked_add(1) {
            Some(next) => next,
            None => 1,
        };
        id
    }

    pub(crate) fn open_raw_service(
        &mut self,
        service: &str,
        timeout: Duration,
    ) -> Result<ADBSession<T>> {
        let local_id = self.allocate_local_id();
        open_service_session(self.get_transport_mut(), service, local_id, timeout)
    }

    /// Open `destination` as a byte stream, telling an explicit refusal
    /// (CLSE) apart from an OPEN whose outcome is unknown. Diagnostics name
    /// only the service kind, never its arguments.
    pub(crate) fn open_stream_service_classified(
        &mut self,
        destination: &[u8],
        read_timeout: Duration,
        write_timeout: Duration,
    ) -> std::result::Result<ADBService<T>, OpenFailure> {
        let local_id = self.allocate_local_id();
        let session = open_stream_classified(
            self.get_transport_mut(),
            destination,
            local_id,
            read_timeout,
        )?;
        Ok(ADBService::new(session, read_timeout, write_timeout))
    }

    pub(crate) fn open_session(&mut self, cmd: &ADBLocalCommand) -> Result<ADBSession<T>> {
        self.open_session_with_timeout(cmd, DEFAULT_READ_TIMEOUT)
    }

    pub(crate) fn open_session_with_timeout(
        &mut self,
        cmd: &ADBLocalCommand,
        timeout: Duration,
    ) -> Result<ADBSession<T>> {
        let local_id = self.allocate_local_id();
        open_stream(
            self.get_transport_mut(),
            cmd.to_string().as_bytes(),
            local_id,
            timeout,
        )
    }

    /// Open `destination` as a byte stream whose reads wait at most
    /// `read_timeout` for each packet.
    pub(crate) fn open_stream_service(
        &mut self,
        destination: &[u8],
        read_timeout: Duration,
    ) -> Result<ADBService<T>> {
        let local_id = self.allocate_local_id();
        let session = open_stream(
            self.get_transport_mut(),
            destination,
            local_id,
            read_timeout,
        )?;
        Ok(ADBService::new(session, read_timeout, STREAM_WRITE_TIMEOUT))
    }

    pub(crate) fn open_service(
        &mut self,
        cmd: &ADBLocalCommand,
        read_timeout: Duration,
    ) -> Result<ADBService<T>> {
        self.open_stream_service(cmd.to_string().as_bytes(), read_timeout)
    }
}

impl<T: ADBMessageTransport> Drop for ADBMessageDevice<T> {
    fn drop(&mut self) {
        // Best effort here
        let _ = self.get_transport_mut().disconnect();
    }
}

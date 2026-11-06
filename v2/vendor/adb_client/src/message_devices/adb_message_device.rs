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
        tcp::PreferredKeys,
    },
    models::ADBLocalCommand,
};

/// Bound on the whole handshake when the caller does not choose one.
pub(crate) const DEFAULT_AUTH_TIMEOUT: Duration = Duration::from_secs(10);

/// Tokens are answered with a signature from each key in turn, then with the
/// first key's public key, as AOSP's host does (`send_auth_response` in
/// `client/auth.cpp`). A device that asks for this many more after the public
/// key is not going to accept us. With one key that is four tokens in all.
const PUBLIC_KEY_ROUNDS: usize = 3;

/// Which of a connection's keys the device accepted. Keys count from zero:
/// any preferred keys in their order, then the key file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AcceptedKey {
    /// It accepted the AUTH signature made with this key.
    Signature(usize),
    /// It connected after this key's public key was offered for approval.
    PublicKey(usize),
    /// Over TLS it trusted this key's certificate. `hinted` says whether its
    /// CA hints named the key; `refused` is how many other certificates it
    /// refused first, each on its own connection.
    Tls {
        /// The key whose certificate was trusted.
        index: usize,
        /// Whether the device's CA hints named this key.
        hinted: bool,
        /// Certificates refused before this one.
        refused: usize,
    },
}

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
    accepted_key: Option<AcceptedKey>,
}

impl<T: ADBMessageTransport> ADBMessageDevice<T> {
    /// Instantiate a new [`ADBMessageTransport`]. The connection handshake is
    /// bounded by a 10-second default.
    pub fn new<P: AsRef<Path>>(transport: T, adb_private_key_path: P) -> Result<Self> {
        Self::new_inner(
            transport,
            adb_private_key_path,
            &PreferredKeys::none(),
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
        Self::new_inner(
            transport,
            adb_private_key_path,
            &PreferredKeys::none(),
            auth_timeout,
            false,
            None,
        )
    }

    /// Like [`Self::new_with_auth_timeout`], but the key at
    /// `adb_private_key_path` must already exist: a missing key is
    /// `IOError(NotFound)` before anything is sent, never a new random key.
    pub(crate) fn new_with_required_key<P: AsRef<Path>>(
        transport: T,
        adb_private_key_path: P,
        auth_timeout: Duration,
    ) -> Result<Self> {
        Self::new_inner(
            transport,
            adb_private_key_path,
            &PreferredKeys::none(),
            auth_timeout,
            true,
            None,
        )
    }

    /// [`Self::new_with_required_key`], offering `preferred` before the key
    /// file. The transport must have been given the same `preferred`.
    pub(crate) fn new_with_required_keys<P: AsRef<Path>>(
        transport: T,
        adb_private_key_path: P,
        preferred: &PreferredKeys,
        auth_timeout: Duration,
    ) -> Result<Self> {
        Self::new_inner(
            transport,
            adb_private_key_path,
            preferred,
            auth_timeout,
            true,
            None,
        )
    }

    /// Connect, calling `public_key_sent` once our RSA public key has been
    /// written in answer to an AUTH token: from then on the device may be
    /// showing its "Allow debugging?" prompt. It does not run for a key the
    /// device already trusts, over TLS, or when that write fails.
    pub(crate) fn new_with_auth_observer<P: AsRef<Path>>(
        transport: T,
        adb_private_key_path: P,
        preferred: &PreferredKeys,
        auth_timeout: Duration,
        require_existing_key: bool,
        public_key_sent: &dyn Fn(),
    ) -> Result<Self> {
        Self::new_inner(
            transport,
            adb_private_key_path,
            preferred,
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
        Self::new_with_fixture_keys(transport, &[private_key], auth_timeout)
    }

    /// [`Self::new_with_fixture_key`] with several keys, in order.
    #[cfg(test)]
    pub(crate) fn new_with_fixture_keys(
        transport: T,
        keys: &[&ADBRsaKey],
        auth_timeout: Duration,
    ) -> Result<Self> {
        let mut device = Self::unconnected(transport);
        device.connect(keys, auth_timeout, None)?;
        Ok(device)
    }

    fn unconnected(transport: T) -> Self {
        Self {
            transport,
            connection_info: ConnectionInfo::unnegotiated(),
            next_local_id: 1,
            accepted_key: None,
        }
    }

    fn new_inner<P: AsRef<Path>>(
        transport: T,
        adb_private_key_path: P,
        preferred: &PreferredKeys,
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

        let mut keys: Vec<&ADBRsaKey> = preferred.keys().iter().map(|key| &key.auth).collect();
        keys.push(&private_key);
        let mut message_device = Self::unconnected(transport);
        message_device.connect(&keys, auth_timeout, public_key_sent)?;

        Ok(message_device)
    }

    pub(crate) const fn get_transport_mut(&mut self) -> &mut T {
        &mut self.transport
    }

    /// What the CNXN exchange negotiated with this device.
    pub(crate) const fn connection_info(&self) -> &ConnectionInfo {
        &self.connection_info
    }

    /// Which key the device accepted, when it asked for one.
    pub(crate) const fn accepted_key(&self) -> Option<AcceptedKey> {
        self.accepted_key
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

    /// Open the socket, send CNXN and return the device's first reply.
    ///
    /// Android 14's adbd sometimes closes a connection to its TLS port
    /// without answering when it arrives right after the previous one closed
    /// (about 1 in 10 back-to-back connects on the Android TV 14 emulator; none
    /// with 300 ms between them, and none on the plain port). Nothing has been
    /// asked of the device at that point, so one fresh connection is tried.
    /// A close after any reply, or a second silent close, is returned as is.
    fn greet(&mut self, deadline: Instant) -> Result<ADBTransportMessage> {
        const RETRY_PAUSE: Duration = Duration::from_millis(150);
        let mut retried = false;
        loop {
            self.transport.connect()?;
            self.transport.write_message(ADBTransportMessage::try_new(
                MessageCommand::Cnxn,
                A_VERSION,
                MAX_PAYLOAD,
                &host_banner(),
            )?)?;
            match self.read_before(deadline) {
                Err(RustADBError::IOError(error))
                    if !retried && error.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    retried = true;
                    let _ = self.transport.disconnect();
                    let pause = deadline
                        .saturating_duration_since(Instant::now())
                        .min(RETRY_PAUSE);
                    if pause < RETRY_PAUSE {
                        return Err(RustADBError::IOError(error));
                    }
                    std::thread::sleep(pause);
                }
                other => return other,
            }
        }
    }

    /// Run the CNXN handshake as Kadb's `AdbConnection.connect` does, with
    /// `keys` in order (see [`PreferredKeys`]). Everything shares one
    /// deadline, so a device that trickles replies cannot stretch the wait.
    ///
    /// When the transport can present more than one TLS certificate and the
    /// device refuses the one it presented, each certificate not yet tried
    /// gets one fresh connection, in order, still under the same deadline.
    fn connect(
        &mut self,
        keys: &[&ADBRsaKey],
        auth_timeout: Duration,
        public_key_sent: Option<&dyn Fn()>,
    ) -> Result<()> {
        let deadline = Instant::now() + auth_timeout;
        let identities = self.transport.tls_identity_count();
        let mut refused: Vec<usize> = Vec::new();
        let mut public_key_reported = false;
        self.transport.present_tls_identity(None);
        loop {
            let error = match self.handshake(
                keys,
                deadline,
                public_key_sent,
                &mut public_key_reported,
                refused.len(),
            ) {
                Err(error) if identities > 1 && error.is_client_identity_rejected() => error,
                other => return other,
            };
            let Some((presented, _)) = self.transport.presented_tls_identity() else {
                return Err(error);
            };
            if refused.contains(&presented) {
                return Err(error);
            }
            refused.push(presented);
            let Some(next) = (0..identities).find(|index| !refused.contains(index)) else {
                return Err(error);
            };
            log::debug!(
                "the device refused TLS client certificate {} of {identities}; trying {}",
                presented + 1,
                next + 1
            );
            let _ = self.transport.disconnect();
            self.transport.present_tls_identity(Some(next));
        }
    }

    /// One connection's handshake: answer STLS and AUTH until the device
    /// sends CNXN, then negotiate from it. The socket is fresh, so a stream
    /// packet (OKAY, WRTE, CLSE, OPEN) here can only be a broken peer and
    /// fails the handshake rather than being skipped.
    fn handshake(
        &mut self,
        keys: &[&ADBRsaKey],
        deadline: Instant,
        public_key_sent: Option<&dyn Fn()>,
        public_key_reported: &mut bool,
        refused_certificates: usize,
    ) -> Result<()> {
        let mut message = self.greet(deadline)?;

        let mut upgraded = false;
        let mut auth_tokens = 0_usize;
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
                    let offered_public_key = self.answer_auth(message, keys, auth_tokens)?;
                    if offered_public_key
                        && !*public_key_reported
                        && let Some(observe) = public_key_sent
                    {
                        *public_key_reported = true;
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
        self.accepted_key = if auth_tokens > keys.len() {
            Some(AcceptedKey::PublicKey(0))
        } else if auth_tokens > 0 {
            Some(AcceptedKey::Signature(auth_tokens - 1))
        } else if upgraded {
            self.transport
                .presented_tls_identity()
                .map(|(index, hinted)| AcceptedKey::Tls {
                    index,
                    hinted,
                    refused: refused_certificates,
                })
        } else {
            None
        };
        Ok(())
    }

    /// Answer AUTH token number `auth_tokens`: a signature with the next key
    /// while any remain, then the first key's public key. Returns whether the
    /// answer was a public key.
    fn answer_auth(
        &mut self,
        message: ADBTransportMessage,
        keys: &[&ADBRsaKey],
        auth_tokens: usize,
    ) -> Result<bool> {
        if message.header().arg0() != AUTH_TOKEN {
            return Err(RustADBError::ADBRequestFailed(format!(
                "Received AUTH message with type != 1 ({})",
                message.header().arg0()
            )));
        }
        if auth_tokens > keys.len() + PUBLIC_KEY_ROUNDS {
            return Err(RustADBError::ProtocolError(format!(
                "device sent {auth_tokens} AUTH tokens without accepting our key"
            )));
        }

        let reply = if let Some(key) = keys.get(auth_tokens - 1) {
            if auth_tokens == 1 {
                log::debug!("Authentication required");
            } else {
                log::debug!("Signing the next AUTH token with key {auth_tokens}");
            }
            let sign = key.sign(message.into_payload())?;
            ADBTransportMessage::try_new(MessageCommand::Auth, AUTH_SIGNATURE, 0, &sign)?
        } else {
            // The device recognised none of our signatures; offer the first
            // key so it can show its "Allow USB debugging?" prompt.
            let first = keys.first().ok_or_else(|| {
                RustADBError::ProtocolError("no ADB key to answer AUTH with".to_string())
            })?;
            let mut pubkey = first.android_pubkey_encode()?.into_bytes();
            pubkey.push(b'\0');
            ADBTransportMessage::try_new(MessageCommand::Auth, AUTH_RSAPUBLICKEY, 0, &pubkey)?
        };
        self.transport.write_message(reply)?;
        Ok(auth_tokens > keys.len())
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

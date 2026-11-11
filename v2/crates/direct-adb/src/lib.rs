/// Validates existing identity bytes without generating, replacing, or logging secret material.
pub fn validate_identity_pem(bytes: &[u8]) -> Result<(), String> {
    use rsa::{pkcs8::DecodePrivateKey, traits::PublicKeyParts};
    if bytes.len() > 32768 {
        return Err("Identity exceeds the PKCS#8 size limit.".into());
    }
    let text =
        std::str::from_utf8(bytes).map_err(|_| "Identity must be UTF-8 PKCS#8 PEM.".to_owned())?;
    let key = rsa::RsaPrivateKey::from_pkcs8_pem(text)
        .map_err(|_| "Identity must be RSA PKCS#8 PEM.".to_owned())?;
    if key.n().bits() != 2048 {
        return Err("ADB identity must be RSA 2048-bit.".into());
    }
    key.validate()
        .map_err(|_| "RSA identity validation failed.".to_owned())?;
    Ok(())
}
/// Loads an identity this app must only read, such as Google platform-tools'
/// `adbkey`, to offer before its own key ([`WirelessAdb::with_preferred_keys`]):
/// an unencrypted RSA 2048 key in PKCS#8 (`BEGIN PRIVATE KEY`, what adb
/// writes) or PKCS#1 (`BEGIN RSA PRIVATE KEY`) PEM, at most 32 KiB. Errors say
/// why without echoing any of the key.
pub fn load_external_identity_pem(bytes: &[u8]) -> Result<PreferredKeys, String> {
    if bytes.len() > 32768 {
        return Err("The key file exceeds the 32 KiB limit.".into());
    }
    let text =
        std::str::from_utf8(bytes).map_err(|_| "The key file is not PEM text.".to_owned())?;
    PreferredKeys::from_pems([text]).map_err(|error| error.to_string())
}
/// Generates bytes for a new app-owned identity; callers must publish them without replacing existing files.
pub fn generate_identity_pem() -> Result<Vec<u8>, String> {
    use rsa::pkcs8::{EncodePrivateKey, LineEnding};
    let key = rsa::RsaPrivateKey::new(&mut rsa::rand_core::OsRng, 2048)
        .map_err(|_| "RSA identity generation failed.".to_owned())?;
    key.to_pkcs8_pem(LineEnding::LF)
        .map(|pem| pem.as_bytes().to_vec())
        .map_err(|_| "PKCS#8 encoding failed.".to_owned())
}

mod bound_service;
mod bounded_shell;
mod limited_writer;
mod streamed_install;

#[cfg(test)]
mod connection_tests;

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub use adb_client::tcp::PreferredKeys;
use adb_client::tcp::{ADBTcpDevice, PairingError, PeerInfo};
use adb_client::{RebootType, RustADBError, ShellExit};
use async_trait::async_trait;
use atv_optimizer_core::adb::{
    is_reportable_instance_name, AdbByteStream, AdbDriver, AdbError, AdbOutput, AdbResult, Lease,
    Leases,
};
use atv_optimizer_core::commands::devices::normalize_connect_address;
use serde::Serialize;

const AUTHORIZATION_TIMEOUT: Duration = Duration::from_secs(30);

/// How a Network debugging (legacy AUTH) connection may authenticate. TLS
/// (Wireless debugging) presents its certificate the same way under both:
/// the TV never shows a prompt there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectAuth {
    /// Sign with the known keys, then offer the first one's public key when
    /// the TV refuses them all. That offer is what makes the TV show "Allow
    /// debugging?". The Android app always connects this way.
    OfferPublicKey,
    /// Sign with the known keys only. A TV that refuses them all is
    /// [`ConnectFailure::NotAllowed`] and shows nothing.
    SignaturesOnly,
}

/// Why [`WirelessAdb::connect_with`] did not connect.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConnectFailure {
    /// The TV answered and trusts none of the keys. Only
    /// [`ConnectAuth::SignaturesOnly`] ends this way: no public key was
    /// offered, so the TV showed no prompt.
    NotAllowed,
    /// The public key was offered and the attempt ended without approval:
    /// its deadline passed, it was canceled, or the TV closed it. The message
    /// is what [`WirelessAdb::connect`] reports for it.
    NotApproved(String),
    /// Anything else, as a message for the person.
    Failed(String),
}

impl ConnectFailure {
    /// The message [`WirelessAdb::connect`] returns for this failure.
    pub fn into_message(self) -> String {
        match self {
            Self::NotAllowed => "This TV has not allowed this device yet.".into(),
            Self::NotApproved(message) | Self::Failed(message) => message,
        }
    }
}
/// Inactivity bound for one shell response, matching the desktop driver's
/// 30-second command timeout. A silently dropped socket surfaces as an error
/// here instead of pinning the connection mutex forever.
const SHELL_INACTIVITY_TIMEOUT: Duration = Duration::from_secs(30);
/// Package installs and clears can legitimately produce nothing for a while.
const SLOW_SHELL_INACTIVITY_TIMEOUT: Duration = Duration::from_secs(180);
const LIVENESS_PROBE_TIMEOUT: Duration = Duration::from_secs(5);
/// How long [`WirelessAdb::status`] waits for a command that owns the
/// connection before reporting the TV by that command instead of its probe.
const STATUS_BUSY_WAIT: Duration = Duration::from_secs(1);
/// Total bound on an upload over its dedicated connection.
const PUSH_DEADLINE: Duration = Duration::from_secs(15 * 60);
const POST_ERROR_PROBE_TIMEOUT: Duration = Duration::from_secs(3);
/// Every error that evicts the live connection starts with this so the
/// frontend can flip its liveness state without parsing transport details.
pub const CONNECTION_LOST_PREFIX: &str = "Connection to the TV was lost";

/// A live wireless-ADB connection: the owned `adb_client` device plus the
/// identity used to synthesize `adb devices`. One connection at a time.
struct Connection {
    device: ADBTcpDevice,
    serial: String,
    request_id: u64,
}

#[derive(Default)]
struct ConnectionRequests {
    latest: u64,
    awaiting_authorization: bool,
    canceled: bool,
    control: Option<adb_client::tcp::BoundedShellControl>,
}

impl ConnectionRequests {
    fn begin(&mut self, request_id: u64) -> Result<(), String> {
        if request_id <= self.latest {
            return Err("Connection attempt superseded.".into());
        }
        if let Some(control) = self.control.take() {
            control.cancel();
        }
        self.latest = request_id;
        self.awaiting_authorization = false;
        self.canceled = false;
        Ok(())
    }

    fn check(&self, request_id: u64) -> Result<(), String> {
        if request_id != self.latest || self.canceled {
            return Err("Connection attempt canceled.".into());
        }
        Ok(())
    }
}

struct ConnectionAttempt {
    requests: Arc<Mutex<ConnectionRequests>>,
    request_id: u64,
    control: adb_client::tcp::BoundedShellControl,
    complete: bool,
}

impl Drop for ConnectionAttempt {
    fn drop(&mut self) {
        let mut requests = lock_recovering(&self.requests);
        if requests.latest == self.request_id {
            requests.control = None;
            requests.awaiting_authorization = false;
            requests.canceled = !self.complete;
        }
        self.control.cancel();
    }
}

struct AuthorizationObservation {
    requests: Arc<Mutex<ConnectionRequests>>,
    request_id: u64,
    /// The public key was written, whatever became of the request since.
    offered: std::sync::atomic::AtomicBool,
}

impl AuthorizationObservation {
    fn public_key_sent(&self) {
        self.offered
            .store(true, std::sync::atomic::Ordering::Release);
        let mut requests = lock_recovering(&self.requests);
        if requests.check(self.request_id).is_ok() {
            requests.awaiting_authorization = true;
        }
    }

    fn offered(&self) -> bool {
        self.offered.load(std::sync::atomic::Ordering::Acquire)
    }
}

impl Drop for AuthorizationObservation {
    fn drop(&mut self) {
        let mut requests = lock_recovering(&self.requests);
        if requests.check(self.request_id).is_ok() {
            requests.awaiting_authorization = false;
        }
    }
}

/// Who we are connected to. Mirrored outside the device mutex so async code
/// can read it without queueing behind a blocking network call.
#[derive(Clone)]
struct Identity {
    serial: String,
    host: String,
    port: u16,
}

/// Lock a std mutex even if a panic poisoned it. The guarded value is either
/// a plain identity or a device handle we are about to drop or replace, so
/// recovering is always safe, and it keeps one bad packet from making the
/// app unable to reconnect until it is force-killed.
fn lock_recovering<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// [`lock_recovering`], giving up after `wait`.
fn lock_within<T>(mutex: &Mutex<T>, wait: Duration) -> Option<std::sync::MutexGuard<'_, T>> {
    let deadline = std::time::Instant::now() + wait;
    loop {
        match mutex.try_lock() {
            Ok(guard) => return Some(guard),
            Err(std::sync::TryLockError::Poisoned(error)) => return Some(error.into_inner()),
            Err(std::sync::TryLockError::WouldBlock) => {
                if std::time::Instant::now() >= deadline {
                    return None;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }
}

fn validate_requested_serial(active: &str, requested: &str) -> Result<(), String> {
    if active == requested {
        Ok(())
    } else {
        Err(format!("Connected device is {active}, not {requested}."))
    }
}

/// `AdbDriver` backed by the pure-Rust, MIT-licensed `adb_client` crate.
///
/// The device is driven directly from Rust over TCP (TLS-1.3 + RSA AUTH,
/// stream multiplexing, `shell`/`exec` all in-crate) — no adb binary, no adb
/// server. Platform-specific discovery is owned by each app.
///
/// The connection lifecycle (`connect`/`disconnect`) has no adb server on a
/// phone, so those are explicit methods the mobile-only `wireless_*` commands
/// call; the trait surface synthesizes `raw(["devices"])`, routes
/// `shell`/screencap through the owned device, maps `reboot`, and streams
/// `push`/`pull` through the sync service via `raw_transfer` and
/// `pull_limited`, and streams `install`/`install-multiple` to the package
/// manager through `raw_transfer`. `pair` runs the
/// Android-11 SPAKE2 code-pairing exchange in-crate; `forward` is structurally
/// impossible on a phone and reports `Unsupported`.
pub struct WirelessAdb {
    /// PKCS#8 PEM ADB private key. Persisted so re-auth is silent across launches.
    key_path: PathBuf,
    /// Offered before `key_path`; none unless [`Self::with_preferred_keys`].
    preferred_keys: PreferredKeys,
    /// How the dedicated connections opened beside the main one (downloads,
    /// installs, bounded shells, bound services) authenticate. The Android
    /// app keeps [`ConnectAuth::OfferPublicKey`].
    dedicated_auth: ConnectAuth,
    operations: Arc<Leases>,
    allow_key_creation: bool,
    /// `adb_client` is blocking and `ADBTcpDevice` is not shareable across
    /// threads, so the owned device lives behind a std mutex and every call
    /// runs on `spawn_blocking`. `None` == disconnected.
    conn: Arc<Mutex<Option<Connection>>>,
    /// Identity of `conn`, readable without waiting on the device mutex.
    identity: Arc<Mutex<Option<Identity>>>,
    /// Serializes connect/disconnect transitions so a slow connect cannot
    /// resurrect a connection after the user has explicitly disconnected.
    lifecycle: tokio::sync::Mutex<()>,
    requests: Arc<Mutex<ConnectionRequests>>,
}

impl WirelessAdb {
    pub fn new(key_path: PathBuf) -> Self {
        Self::with_operations(key_path, Leases::new())
    }

    pub fn with_operations(key_path: PathBuf, operations: Arc<Leases>) -> Self {
        Self::with_key_policy(key_path, operations, true)
    }

    pub fn with_existing_key(key_path: PathBuf, operations: Arc<Leases>) -> Self {
        Self::with_key_policy(key_path, operations, false)
    }

    fn with_key_policy(
        key_path: PathBuf,
        operations: Arc<Leases>,
        allow_key_creation: bool,
    ) -> Self {
        Self {
            allow_key_creation,
            operations,
            key_path,
            preferred_keys: PreferredKeys::none(),
            dedicated_auth: ConnectAuth::OfferPublicKey,
            conn: Arc::new(Mutex::new(None)),
            identity: Arc::new(Mutex::new(None)),
            lifecycle: tokio::sync::Mutex::new(()),
            requests: Arc::new(Mutex::new(ConnectionRequests::default())),
        }
    }

    /// Offer `keys` before this driver's own key on every connection, as
    /// [`PreferredKeys`] describes: a TV that already trusts one of them
    /// connects without a new prompt, a TV that trusts none is asked to allow
    /// the first, and pairing registers the first. The own key is still
    /// required. Without this, connections use the own key alone.
    #[must_use]
    pub fn with_preferred_keys(mut self, keys: PreferredKeys) -> Self {
        self.preferred_keys = keys;
        self
    }

    /// How many keys are offered before this driver's own.
    pub fn preferred_key_count(&self) -> usize {
        self.preferred_keys.len()
    }

    /// Authenticate the dedicated connections opened beside the main one
    /// (downloads, installs, bounded shells, Remote and screen services)
    /// this way. With [`ConnectAuth::SignaturesOnly`] a TV that no longer
    /// allows the keys fails them with [`NO_LONGER_ALLOWED`] and shows
    /// nothing. The default offers the public key, as the main connect does.
    #[must_use]
    pub fn with_dedicated_connections(mut self, auth: ConnectAuth) -> Self {
        self.dedicated_auth = auth;
        self
    }

    /// The keys a dedicated connection offers, under the dedicated policy.
    pub(crate) fn dedicated_keys(&self) -> PreferredKeys {
        match self.dedicated_auth {
            ConnectAuth::OfferPublicKey => self.preferred_keys.clone(),
            ConnectAuth::SignaturesOnly => self.preferred_keys.clone().signing_only(),
        }
    }

    pub fn close_idle_session(&self) -> Result<(), String> {
        if self.operations.active() != 0 {
            return Err("Direct ADB work is still running.".into());
        }
        let mut conn = self
            .conn
            .try_lock()
            .map_err(|_| "Direct ADB socket is busy.".to_string())?;
        *conn = None;
        *lock_recovering(&self.identity) = None;
        Ok(())
    }

    fn work(&self) -> Result<Arc<Lease>, String> {
        self.operations
            .try_acquire()
            .map(Arc::new)
            .ok_or_else(|| "The app is quitting, so no new direct ADB work is started.".into())
    }

    /// Run `f` against the owned device on a blocking thread. Returns
    /// `Err("Not connected…")` when there is no connection. Any transport-level
    /// error from `adb_client` drops the connection (liveness) so a dead socket
    /// stops reporting as a live device.
    async fn on_device<T, F>(&self, serial: &str, f: F) -> Result<T, String>
    where
        T: Send + 'static,
        F: FnOnce(&mut ADBTcpDevice) -> Result<T, RustADBError> + Send + 'static,
    {
        // Without a wait the lock is always taken, so `None` never comes back.
        self.on_device_waiting(serial, None, f)
            .await?
            .ok_or_else(|| "Direct ADB socket is busy.".to_string())
    }

    /// [`Self::on_device`], except that `Ok(None)` means another command
    /// still held the connection after `wait` (when given), so `f` never ran.
    async fn on_device_waiting<T, F>(
        &self,
        serial: &str,
        wait: Option<Duration>,
        f: F,
    ) -> Result<Option<T>, String>
    where
        T: Send + 'static,
        F: FnOnce(&mut ADBTcpDevice) -> Result<T, RustADBError> + Send + 'static,
    {
        let conn = self.conn.clone();
        let identity = self.identity.clone();
        let requested_serial = serial.to_string();
        let operation = self.work()?;
        tokio::task::spawn_blocking(move || {
            let _operation = operation;
            let mut guard = match wait {
                None => lock_recovering(&conn),
                Some(wait) => match lock_within(&conn, wait) {
                    Some(guard) => guard,
                    None => return Ok(None),
                },
            };
            let Some(active) = guard.as_mut() else {
                return Err("Not connected to a device.".to_string());
            };
            validate_requested_serial(&active.serial, &requested_serial)?;
            let error = match f(&mut active.device) {
                Ok(value) => return Ok(Some(value)),
                Err(e) => e,
            };
            // Only a dead socket should evict the connection. A logical failure
            // for one request (missing remote file, rejected service) keeps the
            // TV connected, but is double-checked with a cheap probe because a
            // confused stream can also present as a protocol error.
            let fatal = is_fatal_transport_error(&error) || !probe_alive(&mut active.device);
            if fatal {
                *guard = None;
                *lock_recovering(&identity) = None;
                tracing::warn!(error = %error, "wireless transport error — cleared connection");
                Err(format!("{CONNECTION_LOST_PREFIX}: {error}"))
            } else {
                Err(request_error_message(&error))
            }
        })
        .await
        .map_err(|e| format!("wireless-adb task failed: {e}"))?
    }

    async fn on_device_controlled<T, F>(
        &self,
        serial: &str,
        budget: Duration,
        run: F,
    ) -> AdbResult<T>
    where
        T: Send + 'static,
        F: FnOnce(
                &mut ADBTcpDevice,
                adb_client::tcp::BoundedShellControl,
            ) -> Result<T, RustADBError>
            + Send
            + 'static,
    {
        let deadline = tokio::time::Instant::now() + budget;
        let control = adb_client::tcp::BoundedShellControl::new(budget)?;
        let _cancel = crate::bounded_shell::CancelOnDrop(control.clone());
        let operation = self.work().map_err(AdbError::Transport)?;
        let conn = self.conn.clone();
        let identity = self.identity.clone();
        let serial = serial.to_owned();
        let observed = control.clone();
        let worker = tokio::task::spawn_blocking(move || {
            let _operation = operation;
            let mut guard = lock_recovering(&conn);
            control.remaining()?;
            let active = guard
                .as_mut()
                .ok_or_else(|| AdbError::Transport("Not connected to a device.".into()))?;
            validate_requested_serial(&active.serial, &serial).map_err(AdbError::Transport)?;
            let error = match run(&mut active.device, control.clone()) {
                Ok(value) => return Ok(value),
                Err(error) => error,
            };
            // Nothing reached the socket under this deadline, so the
            // connection is as healthy as before the request.
            if !control.was_registered() {
                return Err(AdbError::Transport(request_error_message(&error)));
            }
            // As in `on_device`: an answer from the TV (a sync FAIL, a shell
            // stream without an exit status) keeps it connected. Lost framing,
            // a dead socket, or one the deadline or a cancel shut down (which
            // the probe sees) is evicted so the next request reconnects.
            if is_fatal_transport_error(&error) || !probe_alive(&mut active.device) {
                *guard = None;
                *lock_recovering(&identity) = None;
                tracing::warn!(error = %error, "wireless transport error — cleared connection");
                Err(AdbError::Transport(format!(
                    "{CONNECTION_LOST_PREFIX}: {error}. The command's outcome is unknown; check the TV before retrying."
                )))
            } else {
                Err(AdbError::Transport(request_error_message(&error)))
            }
        });
        match tokio::time::timeout_at(deadline, worker).await {
            Ok(result) => result.map_err(|error| AdbError::Uncertain {
                operation: "native command",
                detail: error.to_string(),
            })?,
            Err(_) => {
                // Cancellation and socket registration serialize before the certainty decision.
                observed.cancel();
                if observed.was_registered() {
                    // The cancel shuts the socket down; the worker evicts it.
                    Err(AdbError::Uncertain {
                        operation: "native command",
                        detail: format!("{CONNECTION_LOST_PREFIX} at the total deadline after the command started; verify the outcome before retrying"),
                    })
                } else {
                    Err(AdbError::Transport(
                        "Native command deadline reached while queued; no command was started."
                            .into(),
                    ))
                }
            }
        }
    }

    /// Snapshot of the connected device's identity. Reads the mirror, never
    /// the device mutex, so it returns immediately even while a command is
    /// mid-flight on the socket.
    fn info(&self) -> Option<(String, String, u16)> {
        lock_recovering(&self.identity)
            .as_ref()
            .map(|c| (c.serial.clone(), c.host.clone(), c.port))
    }

    /// Android-11 wireless-debugging pairing: the six-digit-code flow against
    /// the TV's `_adb-tls-pairing` port.
    ///
    /// Runs in the vendored `adb_client` crate (SPAKE2 over TLS 1.3, see
    /// `mobile/PAIRING-PLAN.md`) and registers the same persisted RSA identity
    /// `connect` later presents as its TLS client certificate, so a successful
    /// pairing is what lets the follow-up connect skip the *Allow debugging*
    /// prompt. This only trusts the key; the caller still calls `connect`.
    ///
    /// Legacy TVs (Nvidia Shield, older Google TV) use Network debugging on
    /// `:5555` and need no pairing — they connect directly.
    pub async fn pair(&self, host: &str, port: u16, code: &str) -> Result<String, String> {
        self.pair_reporting_instance(host, port, code)
            .await
            .map(|paired| paired.message)
    }

    /// [`pair`](Self::pair), also returning the name the device gave itself.
    pub async fn pair_reporting_instance(
        &self,
        host: &str,
        port: u16,
        code: &str,
    ) -> Result<Paired, String> {
        let addr: SocketAddr = format!("{host}:{port}")
            .parse()
            .map_err(|e| format!("invalid pairing address {host}:{port}: {e}"))?;
        let code = normalize_pairing_code(code)?;
        let key_path = self.key_path.clone();
        let preferred = self.preferred_keys.clone();
        let allow_key_creation = self.allow_key_creation;

        // `adb_client::tcp::pair` is blocking and applies its own 30-second
        // deadline through socket timeouts, so it cannot pin this thread.
        let operation = self.work()?;
        tokio::task::spawn_blocking(move || -> Result<Paired, String> {
            let _operation = operation;
            ensure_adb_key(&key_path, allow_key_creation)?;
            match adb_client::tcp::pair_with_keys(addr, &code, &key_path, &preferred) {
                Ok(outcome) => {
                    let instance = peer_instance_name(&outcome.peer_info);
                    tracing::info!(
                        peer_kind = outcome.peer_info.kind,
                        peer = %outcome.peer_info.data_lossy(),
                        instance_found = instance.is_some(),
                        "wireless pairing succeeded"
                    );
                    Ok(Paired {
                        message: "Paired with the TV.".to_string(),
                        instance,
                    })
                }
                Err(e) => {
                    tracing::warn!(error = %e, "wireless pairing failed");
                    Err(pair_error_message(&e))
                }
            }
        })
        .await
        .map_err(|e| format!("wireless-adb task failed: {e}"))?
    }

    pub fn begin_request(&self, request_id: u64) -> Result<(), String> {
        lock_recovering(&self.requests).begin(request_id)
    }

    /// True only while the current handshake has sent its key for approval.
    pub fn awaiting_authorization(&self) -> bool {
        lock_recovering(&self.requests).awaiting_authorization
    }

    pub async fn cancel_connect(
        &self,
        request_id: u64,
        canceled_request_id: u64,
    ) -> Result<(), String> {
        self.begin_request(request_id)?;
        let conn = self.conn.clone();
        let identity = self.identity.clone();
        let requests = self.requests.clone();
        let operation = self.work()?;
        tokio::task::spawn_blocking(move || {
            let _operation = operation;
            let mut guard = lock_recovering(&conn);
            let requests = lock_recovering(&requests);
            if requests.check(request_id).is_ok()
                && guard
                    .as_ref()
                    .is_some_and(|c| c.request_id == canceled_request_id)
            {
                *guard = None;
                *lock_recovering(&identity) = None;
            }
        })
        .await
        .map_err(|e| format!("cancel connection: {e}"))
    }

    /// Connect, offering the public key for approval when the TV trusts none
    /// of the keys, within the 30-second handshake budget.
    pub async fn connect(&self, host: &str, port: u16, request_id: u64) -> Result<String, String> {
        self.connect_within(host, port, request_id, AUTHORIZATION_TIMEOUT)
            .await
    }

    /// `connect` under an explicit handshake budget, so loopback tests can
    /// exercise the approval-timeout path without waiting 30 seconds.
    async fn connect_within(
        &self,
        host: &str,
        port: u16,
        request_id: u64,
        budget: Duration,
    ) -> Result<String, String> {
        self.connect_with(host, port, request_id, ConnectAuth::OfferPublicKey, budget)
            .await
            .map_err(ConnectFailure::into_message)
    }

    /// Connect under `auth` within `budget`, the bound on the whole handshake
    /// including any wait for the person to choose Allow on the TV.
    pub async fn connect_with(
        &self,
        host: &str,
        port: u16,
        request_id: u64,
        auth: ConnectAuth,
        budget: Duration,
    ) -> Result<String, ConnectFailure> {
        let _lifecycle = self.lifecycle.lock().await;
        lock_recovering(&self.requests)
            .check(request_id)
            .map_err(ConnectFailure::Failed)?;
        normalize_connect_address(&format!("{host}:{port}")).map_err(ConnectFailure::Failed)?;
        let addr: SocketAddr = format!("{host}:{port}").parse().map_err(|e| {
            ConnectFailure::Failed(format!("invalid device address {host}:{port}: {e}"))
        })?;
        let key_path = self.key_path.clone();
        let preferred = self.preferred_keys.clone();
        let allow_key_creation = self.allow_key_creation;
        let conn = self.conn.clone();
        let identity = self.identity.clone();
        let requests = self.requests.clone();
        let host_owned = host.to_string();
        let serial = format!("{host}:{port}");

        let operation = self.work().map_err(ConnectFailure::Failed)?;
        let control = adb_client::tcp::BoundedShellControl::new(budget)
            .map_err(|error| ConnectFailure::Failed(format!("Connection deadline: {error}")))?;
        let mut attempt = ConnectionAttempt {
            requests: requests.clone(),
            request_id,
            control: control.clone(),
            complete: false,
        };
        {
            let mut requests = lock_recovering(&requests);
            requests.check(request_id).map_err(ConnectFailure::Failed)?;
            requests.control = Some(control.clone());
        }
        let result = tokio::task::spawn_blocking(move || -> Result<String, ConnectFailure> {
            let _operation = operation;
            control
                .remaining()
                .map_err(|error| ConnectFailure::Failed(error.to_string()))?;
            ensure_adb_key(&key_path, allow_key_creation).map_err(ConnectFailure::Failed)?;
            let device = match auth {
                ConnectAuth::OfferPublicKey => {
                    // Offering the public key is what shows the TV's "Allow
                    // debugging?" prompt; the persisted key is remembered after.
                    let observation = AuthorizationObservation {
                        requests: requests.clone(),
                        request_id,
                        offered: std::sync::atomic::AtomicBool::new(false),
                    };
                    let device = ADBTcpDevice::new_with_keys_control_and_auth_observer(
                        addr,
                        &key_path,
                        &preferred,
                        !allow_key_creation,
                        control.clone(),
                        &|| observation.public_key_sent(),
                    );
                    let offered = observation.offered();
                    drop(observation);
                    device.map_err(|error| (error, offered))
                }
                ConnectAuth::SignaturesOnly => ADBTcpDevice::new_with_keys_and_control_signing_only(
                    addr,
                    &key_path,
                    &preferred,
                    !allow_key_creation,
                    control.clone(),
                )
                .map_err(|error| (error, false)),
            };
            let mut device = device.map_err(|(error, offered)| {
                if matches!(error, RustADBError::KeyNotAllowed) {
                    tracing::info!(serial = %serial, "wireless connect: the TV has not allowed these keys yet");
                    ConnectFailure::NotAllowed
                } else if offered {
                    tracing::info!(serial = %serial, error = %error, "wireless connect ended without approval");
                    ConnectFailure::NotApproved(connect_error_message(&error))
                } else {
                    tracing::warn!(serial = %serial, error = %error, "wireless connect failed");
                    ConnectFailure::Failed(match auth {
                        ConnectAuth::OfferPublicKey => connect_error_message(&error),
                        ConnectAuth::SignaturesOnly => quiet_connect_error_message(&error),
                    })
                }
            })?;
            if !preferred.is_empty() {
                tracing::debug!(
                    serial = %serial,
                    accepted_key = ?device.accepted_key(),
                    preferred_keys = preferred.len(),
                    "wireless connect authenticated"
                );
            }
            // A previous command may still own the connection mutex. Keep this
            // wait inside the handshake budget without interrupting that command.
            let mut guard = loop {
                control
                    .remaining()
                    .map_err(|error| ConnectFailure::Failed(error.to_string()))?;
                match conn.try_lock() {
                    Ok(guard) => break guard,
                    Err(std::sync::TryLockError::Poisoned(error)) => break error.into_inner(),
                    Err(std::sync::TryLockError::WouldBlock) => {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                }
            };
            // Cancellation and publication share this short lock. No socket
            // I/O holds it, so a pending handshake can be invalidated promptly.
            let requests = lock_recovering(&requests);
            requests.check(request_id).map_err(ConnectFailure::Failed)?;
            device
                .finish_connection()
                .map_err(|e| ConnectFailure::Failed(connect_error_message(&e)))?;
            // Replacing the handle drops the previous device, which closes
            // its socket. Publish the identity only once the swap is done.
            *guard = Some(Connection {
                device,
                serial: serial.clone(),
                request_id,
            });
            *lock_recovering(&identity) = Some(Identity {
                serial: serial.clone(),
                host: host_owned,
                port,
            });
            Ok(format!("Connected to {serial}."))
        })
        .await
        .map_err(|e| ConnectFailure::Failed(format!("wireless-adb task failed: {e}")))?;
        attempt.complete = result.is_ok();
        result
    }

    pub async fn disconnect(&self, request_id: u64) -> Result<String, String> {
        self.disconnect_serial_for_request(None, Some(request_id))
            .await
    }

    async fn disconnect_serial(&self, requested_serial: Option<&str>) -> Result<String, String> {
        self.disconnect_serial_for_request(requested_serial, None)
            .await
    }

    async fn disconnect_serial_for_request(
        &self,
        requested_serial: Option<&str>,
        request_id: Option<u64>,
    ) -> Result<String, String> {
        let _lifecycle = self.lifecycle.lock().await;
        // Dropping the device closes its socket; do it on a blocking thread.
        let conn = self.conn.clone();
        let identity = self.identity.clone();
        let requests = self.requests.clone();
        let requested_serial = requested_serial.map(str::to_string);
        let operation = self.work()?;
        tokio::task::spawn_blocking(move || -> Result<(), String> {
            let _operation = operation;
            let mut guard = lock_recovering(&conn);
            let requests = lock_recovering(&requests);
            if let Some(request_id) = request_id {
                requests.check(request_id)?;
            }
            if let (Some(expected), Some(active)) = (requested_serial.as_deref(), guard.as_ref()) {
                validate_requested_serial(&active.serial, expected)?;
            }
            *guard = None;
            *lock_recovering(&identity) = None;
            Ok(())
        })
        .await
        .map_err(|e| format!("wireless-adb task failed: {e}"))??;
        Ok("Disconnected.".to_string())
    }

    /// Cheap liveness probe for the frontend's connection watchdog. Returns the
    /// cached device only if a fast `echo` round-trips; otherwise clears the
    /// cached connection and reports disconnected.
    pub async fn status(&self) -> WirelessStatus {
        let Some((serial, host, _port)) = self.info() else {
            return WirelessStatus::disconnected();
        };
        // The vendored transport applies a finite timeout to every response in
        // this probe. Unlike cancelling spawn_blocking from the async side,
        // that releases the connection mutex when a socket stops responding.
        let probe = self
            .on_device_waiting(&serial, Some(STATUS_BUSY_WAIT), |dev| {
                let mut stdout = Vec::new();
                dev.shell_command_with_timeout(
                    &"echo ok",
                    Some(&mut stdout),
                    None,
                    LIVENESS_PROBE_TIMEOUT,
                )?;
                Ok(String::from_utf8_lossy(&stdout).into_owned())
            })
            .await;
        let alive = match probe {
            Ok(Some(stdout)) => stdout.contains("ok"),
            // A command still owns the socket. It runs under its own
            // deadlines and evicts the connection if the socket dies, so a
            // TV that is still connected afterwards is connected now.
            Ok(None) => self.info().is_some_and(|(current, _, _)| current == serial),
            Err(e) => {
                tracing::warn!(serial = %serial, error = %e, "wireless_status probe failed");
                false
            }
        };
        // A dead socket was already evicted inside `on_device`. Do not issue a
        // second disconnect here: it would race a reconnect to the same
        // host:port and drop the fresh connection the frontend just got.
        if !alive {
            return WirelessStatus::disconnected();
        }
        WirelessStatus {
            connected: true,
            serial: Some(serial),
            host: Some(host),
        }
    }

    /// Upload `local` to `remote` over a dedicated connection that must
    /// report the selected TV's verified hardware id before any byte is
    /// sent, like downloads and installs. The selected TV's own connection is
    /// free meanwhile, so the device list and other commands keep working,
    /// and a failed upload never evicts it.
    async fn push_impl(&self, serial: &str, local: &Path, remote: &str) -> AdbResult<()> {
        let deadline = tokio::time::Instant::now() + PUSH_DEADLINE;
        // A local storage error must not reach a healthy TV connection, so
        // the file is opened before any TV work.
        let mut file = std::fs::File::open(local)
            .map_err(|e| AdbError::Transport(format!("open {}: {e}", local.display())))?;
        let Some((active, host, port)) = self.info() else {
            return Err(AdbError::Transport("Not connected to a device.".into()));
        };
        validate_requested_serial(&active, serial).map_err(AdbError::Transport)?;
        let address: SocketAddr = format!("{host}:{port}")
            .parse()
            .map_err(|error| AdbError::Transport(format!("invalid upload address: {error}")))?;
        let expected = self
            .verified_owned_hardware_id(serial, Duration::from_secs(5))
            .await?;
        let operation = self.work().map_err(AdbError::Transport)?;
        let control = adb_client::tcp::BoundedShellControl::new(
            deadline.saturating_duration_since(tokio::time::Instant::now()),
        )?;
        let _cancel = crate::bounded_shell::CancelOnDrop(control.clone());
        let remote = remote.to_owned();
        let key = self.key_path.clone();
        let preferred = self.dedicated_keys();
        let sending = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let started = sending.clone();
        let upload = crate::bounded_shell::run_worker(operation, control, move |control| {
            let mut device = ADBTcpDevice::new_with_required_keys_and_control(
                address, &key, &preferred, &control,
            )
            .map_err(|error| {
                AdbError::Transport(format!(
                    "Could not start the upload: {}",
                    dedicated_error(&error)
                ))
            })?;
            let probe = adb_client::tcp::BoundedShellControl::new(
                control.remaining()?.min(Duration::from_secs(5)),
            )?;
            device
                .verify_hardware_binding_bounded(&expected, probe)
                .map_err(|error| {
                    AdbError::Transport(format!(
                        "Upload identity check failed before any byte was sent: {error}"
                    ))
                })?;
            started.store(true, std::sync::atomic::Ordering::Release);
            device
                .push_with_control(&mut file, &remote, control)
                .map_err(upload_error)
        });
        match tokio::time::timeout_at(deadline, upload).await {
            Ok(result) => result,
            Err(_) if sending.load(std::sync::atomic::Ordering::Acquire) => {
                Err(AdbError::Uncertain {
                    operation: "file upload",
                    detail: "the total deadline was reached; the file on the TV may be incomplete"
                        .into(),
                })
            }
            Err(_) => Err(AdbError::Transport(
                "Upload deadline reached before any byte was sent.".into(),
            )),
        }
    }

    fn devices_output(&self) -> AdbOutput {
        let stdout = match self.info() {
            Some((serial, _, _)) => format!("List of devices attached\n{serial}\tdevice\n"),
            None => "List of devices attached\n".to_string(),
        };
        AdbOutput {
            stdout,
            stderr: String::new(),
            exit_code: Some(0),
        }
    }
}

#[async_trait]
impl AdbDriver for WirelessAdb {
    fn operation_leases(&self) -> Option<Arc<Leases>> {
        Some(self.operations.clone())
    }

    fn uses_host_server(&self) -> bool {
        false
    }

    async fn pull_limited(
        &self,
        serial: &str,
        remote: &str,
        local: &Path,
        max_bytes: u64,
    ) -> AdbResult<()> {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(15 * 60);
        let control = adb_client::tcp::BoundedShellControl::new(Duration::from_secs(15 * 60))?;
        let _cancel = crate::bounded_shell::CancelOnDrop(control.clone());
        let operation = self.work().map_err(AdbError::Transport)?;
        let Some((active, host, port)) = self.info() else {
            return Err(AdbError::Transport("Not connected to a device.".into()));
        };
        validate_requested_serial(&active, serial).map_err(AdbError::Transport)?;
        let address: SocketAddr = format!("{host}:{port}")
            .parse()
            .map_err(|error| AdbError::Transport(format!("invalid download address: {error}")))?;
        let expected = self
            .verified_owned_hardware_id(serial, Duration::from_secs(5))
            .await?;
        let remote = remote.to_owned();
        let local = local.to_path_buf();
        let key = self.key_path.clone();
        let preferred = self.dedicated_keys();
        let download = crate::bounded_shell::run_worker(operation, control, move |control| {
            control.remaining()?;
            let mut device = ADBTcpDevice::new_with_required_keys_and_control(
                address, &key, &preferred, &control,
            )
            .map_err(|error| AdbError::Transport(dedicated_error(&error)))?;
            let probe = adb_client::tcp::BoundedShellControl::new(
                control.remaining()?.min(Duration::from_secs(5)),
            )?;
            device
                .verify_hardware_binding_bounded(&expected, probe)
                .map_err(|error| {
                    AdbError::Transport(format!(
                        "Download identity check failed before creating a file: {error}"
                    ))
                })?;
            control.remaining()?;
            // Publish only a complete download; rejection or cancellation must
            // neither leave a partial file nor truncate an existing destination.
            let parent = local.parent().filter(|path| !path.as_os_str().is_empty());
            let mut staged = tempfile::NamedTempFile::new_in(parent.unwrap_or(Path::new(".")))?;
            // The size check and the transfer share one sync session; the
            // writer still stops a file that grows past the limit meanwhile.
            let mut writer = crate::limited_writer::LimitedWriter::new(&mut staged, max_bytes);
            device
                .pull_with_limit(&remote, &mut writer, max_bytes)
                .map_err(|error| AdbError::Transport(download_error_message(&error)))?;
            control.remaining()?;
            std::io::Write::flush(&mut writer)?;
            control.remaining()?;
            staged.persist(&local).map_err(|error| error.error)?;
            Ok(())
        });
        tokio::time::timeout_at(deadline, download)
            .await
            .map_err(|_| {
                AdbError::Transport(
                    "Download total deadline reached; no incomplete file is published.".into(),
                )
            })?
    }

    async fn raw(&self, args: &[&str]) -> AdbResult<AdbOutput> {
        match args {
            ["devices"] => Ok(self.devices_output()),
            ["disconnect", serial] => {
                self.disconnect_serial(Some(serial))
                    .await
                    .map_err(AdbError::Transport)?;
                Ok(AdbOutput {
                    stdout: "Disconnected.".to_string(),
                    stderr: String::new(),
                    exit_code: Some(0),
                })
            }
            ["forward", ..] => Err(AdbError::Unsupported {
                operation: "forward",
            }),
            ["push", ..] => Err(AdbError::Unsupported { operation: "push" }),
            ["pull", ..] => Err(AdbError::Unsupported { operation: "pull" }),
            ["connect", ..] => Err(AdbError::Unsupported {
                operation: "connect",
            }),
            ["pair", ..] => Err(AdbError::Unsupported { operation: "pair" }),
            // The vendored reboot call returns success once adbd acknowledges
            // the reboot service. Pre-ack transport failures remain failures.
            ["-s", serial, "reboot", rest @ ..] => {
                let reboot_type = match rest {
                    [] => RebootType::System,
                    ["recovery"] => RebootType::Recovery,
                    ["bootloader"] => RebootType::Bootloader,
                    _ => RebootType::System,
                };
                self.on_device_controlled(serial, Duration::from_secs(30), move |dev, control| {
                    dev.reboot_with_control(reboot_type, control)
                })
                .await?;
                let _ = self.disconnect_serial(Some(serial)).await;
                Ok(AdbOutput {
                    stdout: "Reboot command sent.".to_string(),
                    stderr: String::new(),
                    exit_code: Some(0),
                })
            }
            _ => Err(AdbError::Unsupported { operation: "raw" }),
        }
    }

    /// Installs, downloads and uploads use a dedicated, identity-checked
    /// connection, so a long transfer leaves the selected TV's session free
    /// and a failed one cannot evict it.
    async fn raw_transfer(&self, args: &[&str]) -> AdbResult<AdbOutput> {
        match args {
            ["-s", serial, verb @ ("install" | "install-multiple"), rest @ ..] => {
                self.install_impl(serial, verb, rest).await
            }
            ["-s", serial, "pull", remote, local] => {
                self.pull_limited(serial, remote, Path::new(local), u64::MAX)
                    .await?;
                Ok(AdbOutput {
                    stdout: String::new(),
                    stderr: String::new(),
                    exit_code: Some(0),
                })
            }
            ["-s", serial, "push", local, remote] => {
                self.push_impl(serial, Path::new(local), remote).await?;
                Ok(AdbOutput {
                    stdout: String::new(),
                    stderr: String::new(),
                    exit_code: Some(0),
                })
            }
            _ => Err(AdbError::Unsupported {
                operation: "raw_transfer",
            }),
        }
    }

    async fn shell_bounded(
        &self,
        serial: &str,
        command: &str,
    ) -> AdbResult<atv_optimizer_core::adb::driver::BoundedShellOutput> {
        self.bounded_shell_impl(serial, command).await
    }

    async fn shell(&self, serial: &str, command: &str) -> AdbResult<AdbOutput> {
        let command = command.to_string();
        let timeout = shell_timeout_for(&command);
        let (stdout, stderr, code) = self
            .on_device_controlled(serial, timeout, move |dev, control| {
                let mut out: Vec<u8> = Vec::new();
                let mut err: Vec<u8> = Vec::new();
                let mut bounded_out =
                    crate::limited_writer::LimitedWriter::new(&mut out, 512 * 1024);
                let mut bounded_err =
                    crate::limited_writer::LimitedWriter::new(&mut err, 512 * 1024);
                // A v2 stream that ends without its exit status is an error
                // here (`ShellExitUnknown`), never a success; `None` is only
                // the legacy shell, which has no status to report.
                let exit = dev
                    .shell_command_with_control(
                        &command,
                        Some(&mut bounded_out),
                        Some(&mut bounded_err),
                        control,
                    )?
                    .map_or(ShellExit::Unreported, ShellExit::Code);
                // Over-limit text is a local refusal; the socket is fine.
                let text = |bytes: &[u8]| {
                    crate::limited_writer::bounded_text(bytes, 512 * 1024)
                        .map_err(RustADBError::LocalIo)
                };
                Ok((text(&out)?, text(&err)?, exit))
            })
            .await?;
        Ok(AdbOutput {
            stdout,
            stderr,
            exit_code: Some(exit_code_for(code)),
        })
    }

    async fn raw_bytes(&self, args: &[&str]) -> AdbResult<Vec<u8>> {
        let serial = match args {
            ["-s", serial, "exec-out", "screencap", "-p"] => *serial,
            _ => {
                return Err(AdbError::Unsupported {
                    operation: "raw_bytes",
                });
            }
        };
        self.on_device_controlled(serial, SHELL_INACTIVITY_TIMEOUT, |dev, control| {
            // Binary-safe either way: shell v2 when negotiated (stderr kept
            // apart, exit status), else `exec:`, which never runs under the
            // pty that would rewrite the PNG's line endings. The unbounded
            // framebuffer fallback stays refused.
            let mut out: Vec<u8> = Vec::new();
            let mut err: Vec<u8> = Vec::new();
            let mut bounded_out =
                crate::limited_writer::LimitedWriter::new(&mut out, 64 * 1024 * 1024);
            let mut bounded_err = crate::limited_writer::LimitedWriter::new(&mut err, 512 * 1024);
            let exit = dev.exec_out_with_control(
                "screencap -p",
                &mut bounded_out,
                Some(&mut bounded_err),
                control,
            )?;
            let completed = match exit {
                ShellExit::Code(code) => code == 0,
                // `exec:` reports no status; only the PNG signature below can
                // show it worked.
                ShellExit::Unreported => true,
            };
            if completed && out.starts_with(b"\x89PNG") {
                Ok(out)
            } else {
                Err(RustADBError::ADBRequestFailed(
                    "Screenshot did not return PNG; unbounded framebuffer fallback is unavailable."
                        .into(),
                ))
            }
        })
        .await
    }

    async fn device_binding(
        &self,
        serial: &str,
    ) -> AdbResult<atv_optimizer_core::adb::AdbDeviceBinding> {
        self.device_binding_impl(serial).await
    }

    async fn open_bound_device_service(
        &self,
        binding: &atv_optimizer_core::adb::AdbDeviceBinding,
        service: &str,
        lifetime: Duration,
    ) -> AdbResult<atv_optimizer_core::adb::AdbOwnedService> {
        self.open_bound_device_service_impl(binding, service, lifetime)
            .await
    }

    async fn open_device_service(
        &self,
        serial: &str,
        service: &str,
    ) -> AdbResult<Box<dyn AdbByteStream>> {
        let Some((active_serial, _, _)) = self.info() else {
            return Err(AdbError::Transport("Not connected to a device.".into()));
        };
        validate_requested_serial(&active_serial, serial).map_err(AdbError::Transport)?;
        // Generic consumers inherit the same identity checks and finite owned-stream lifetime.
        let binding = self.device_binding_impl(serial).await?;
        let owned = self
            .open_bound_device_service_with_read_policy(
                &binding,
                service,
                Duration::from_secs(15 * 60),
                Some(Duration::from_secs(2)),
            )
            .await?;
        Ok(owned.stream)
    }
}

struct CountedStream {
    stream: Box<dyn AdbByteStream>,
    _operation: Arc<Lease>,
}

impl std::io::Read for CountedStream {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        self.stream.read(out)
    }
}
impl std::io::Write for CountedStream {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        self.stream.write(data)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.stream.flush()
    }
}

pub type SharedWirelessAdb = Arc<WirelessAdb>;

/// Why a dedicated connection failed when the TV refused every key and no
/// public key may be offered on it.
pub const NO_LONGER_ALLOWED: &str = "This TV no longer allows this computer. In Devices, use Request approval on its row, then tick \u{201c}Always allow from this computer\u{201d} on the TV and choose Allow.";

/// The message for a dedicated connection's failure: [`NO_LONGER_ALLOWED`]
/// when the TV refused every key, otherwise the error itself.
pub(crate) fn dedicated_error(error: &RustADBError) -> String {
    if matches!(error, RustADBError::KeyNotAllowed) {
        NO_LONGER_ALLOWED.to_string()
    } else {
        error.to_string()
    }
}

/// Errors that mean the socket or its framing is unusable, as opposed to a
/// request adbd answered with a failure. `PayloadTooLarge` is refused before
/// anything reaches the socket, so it leaves the connection intact, as do a
/// sync `FAIL` (`SyncFailed`), a package-manager refusal (`InstallFailed`), a
/// local file error (`LocalIo`), a file over the caller's limit
/// (`FileTooLarge`) and a shell v2 stream that closed without its exit status
/// (`ShellExitUnknown`, still double-checked by the liveness probe). A
/// connection that lost framing mid-message reports `ProtocolError` on every
/// later use, so it is dropped and the next request reconnects.
fn is_fatal_transport_error(error: &RustADBError) -> bool {
    matches!(
        error,
        RustADBError::IOError(_)
            | RustADBError::PoisonError
            | RustADBError::TLSError(_)
            | RustADBError::TlsAuthRejected(_)
            | RustADBError::TlsConnectionClosed(_)
            | RustADBError::UpgradeError(_)
            | RustADBError::WrongResponseReceived(..)
            | RustADBError::ProtocolError(_)
            | RustADBError::InvalidIntegrity(..)
            | RustADBError::ADBShellNotSupported
    )
}

fn probe_alive(dev: &mut ADBTcpDevice) -> bool {
    let mut stdout = Vec::new();
    dev.shell_command_with_timeout(
        &"echo ok",
        Some(&mut stdout),
        None,
        POST_ERROR_PROBE_TIMEOUT,
    )
    .is_ok()
        && String::from_utf8_lossy(&stdout).contains("ok")
}

/// What a request that failed on a still-live connection tells the user.
fn request_error_message(error: &RustADBError) -> String {
    match error {
        RustADBError::ShellExitUnknown => {
            "The TV ended the command before reporting a result, so whether it worked is unknown."
                .to_string()
        }
        other => other.to_string(),
    }
}

/// A refusal from the TV or a local read error is the upload's answer. Any
/// other error came after bytes may have reached the TV, so the file there
/// may be incomplete.
fn upload_error(error: RustADBError) -> AdbError {
    match error {
        RustADBError::SyncFailed(_) => AdbError::Transport(error.to_string()),
        RustADBError::LocalIo(_) => AdbError::Transport(format!(
            "{error}; the file on the TV may be missing or incomplete"
        )),
        other => AdbError::Uncertain {
            operation: "file upload",
            detail: format!("{other}; the file on the TV may be incomplete"),
        },
    }
}

fn download_error_message(error: &RustADBError) -> String {
    match error {
        RustADBError::FileTooLarge(..) => "Download exceeds the file size limit.".to_string(),
        other => other.to_string(),
    }
}

/// Shell v2 reports the command's own exit status. The legacy shell, used
/// only by devices without shell v2, reports none: it maps to 0 so
/// `success()` is not permanently false there, and `shell_reported_failure()`
/// judges its text as on every path. A v2 stream that ended without a status
/// never reaches this; the transport returns it as `ShellExitUnknown`.
fn exit_code_for(exit: ShellExit) -> i32 {
    match exit {
        ShellExit::Code(code) => i32::from(code),
        ShellExit::Unreported => 0,
    }
}

fn shell_timeout_for(command: &str) -> Duration {
    let trimmed = command.trim_start();
    let slow = [
        "pm install",
        "pm uninstall",
        "pm clear",
        "cmd package install",
        "bmgr ",
    ];
    if slow.iter().any(|prefix| trimmed.starts_with(prefix)) {
        SLOW_SHELL_INACTIVITY_TIMEOUT
    } else {
        SHELL_INACTIVITY_TIMEOUT
    }
}

/// Connect guidance shared by the desktop and phone apps. Each arm names only
/// what the error shows: the approval prompt is mentioned only when the TV
/// was reached, and a hang-up without a TLS alert never claims "unpaired".
fn connect_error_message(error: &RustADBError) -> String {
    if matches!(error, RustADBError::TlsAuthRejected(_)) || error.is_client_identity_rejected() {
        return "This TV hasn't paired with this app's wireless debugging identity. On the TV, open Wireless debugging › Pair device with pairing code, pair using that code and its pairing port, then connect using the separate port on the main Wireless debugging screen.".into();
    }
    match error {
        // No TLS alert: a network drop looks the same as a silent refusal,
        // so this must not claim the key is unpaired.
        RustADBError::TlsConnectionClosed(_) => {
            "The TV closed the secure connection. Check that this device and the TV are on the same network and try again. If this keeps happening, pair again from Wireless debugging › Pair device with pairing code.".to_string()
        }
        RustADBError::IOError(io) if is_timeout(io) && is_handshake_timeout(io) => {
            "The TV connection timed out during setup. If the TV shows an Allow debugging prompt, approve it, then retry. If no prompt appears, restart the TV before trying again.".to_string()
        }
        // A bare timeout is the TCP connect itself: nothing answered, so no
        // approval prompt can be showing. The wording keeps "timed out" so the
        // shared error codes read it as a timeout.
        RustADBError::IOError(io) if is_timeout(io) => {
            "Connection timed out: nothing answered at that address. Check the TV's current IP address, that it's awake, and that it's on the same network as this device.".to_string()
        }
        RustADBError::IOError(io)
            if matches!(
                io.kind(),
                std::io::ErrorKind::NetworkUnreachable | std::io::ErrorKind::HostUnreachable
            ) =>
        {
            "Network is unreachable: this device has no route to that address. Check that it's on the TV's network, then try again.".to_string()
        }
        RustADBError::IOError(io)
            if matches!(
                io.kind(),
                std::io::ErrorKind::ConnectionAborted
                    | std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::UnexpectedEof
            ) =>
        {
            "The TV closed the connection before setup finished. Check the TV's current IP and debugging settings, then try again. Approve an Allow debugging prompt on the TV if one appears.".to_string()
        }
        RustADBError::IOError(io)
            if matches!(io.kind(), std::io::ErrorKind::ConnectionRefused) =>
        {
            "Connection refused: nothing at that address accepts ADB connections on this port. Check the port, and that Network debugging is enabled on the TV.".to_string()
        }
        _ => "Couldn't finish the ADB connection. Check the TV's current IP and debugging settings. Approve an Allow debugging prompt on the TV if one appears, then try again.".to_string(),
    }
}

/// [`connect_error_message`] for a connection that offered no public key:
/// the TV cannot be showing a prompt, so the wording never mentions one.
fn quiet_connect_error_message(error: &RustADBError) -> String {
    match error {
        RustADBError::IOError(io) if is_timeout(io) && is_handshake_timeout(io) => {
            "The TV stopped answering during setup. Check that it's awake and on the same network, then try again. If this keeps happening, restart the TV.".to_string()
        }
        RustADBError::IOError(io)
            if matches!(
                io.kind(),
                std::io::ErrorKind::ConnectionAborted
                    | std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::UnexpectedEof
            ) =>
        {
            "The TV closed the connection before setup finished. Check the TV's current IP and debugging settings, then try again.".to_string()
        }
        RustADBError::TlsConnectionClosed(_) | RustADBError::TlsAuthRejected(_) => {
            connect_error_message(error)
        }
        _ if error.is_client_identity_rejected() => connect_error_message(error),
        RustADBError::IOError(io)
            if is_timeout(io)
                || matches!(
                    io.kind(),
                    std::io::ErrorKind::NetworkUnreachable
                        | std::io::ErrorKind::HostUnreachable
                        | std::io::ErrorKind::ConnectionRefused
                ) =>
        {
            connect_error_message(error)
        }
        _ => "Couldn't finish the ADB connection. Check the TV's current IP and debugging settings, then try again.".to_string(),
    }
}

fn is_timeout(io: &std::io::Error) -> bool {
    matches!(
        io.kind(),
        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
    )
}

/// The vendored transport reports a CNXN/TLS/AUTH exchange that ran out its
/// deadline with its own "… handshake timed out" error. The TCP connect's
/// timeout is the operating system's, so only this one means the TV was
/// reached. It does not show that an approval prompt appeared: a TV can go
/// quiet before asking for one. `a_peer_silent_before_auth_does_not_claim_the_user_missed_approval`
/// pins the vendored wording.
fn is_handshake_timeout(io: &std::io::Error) -> bool {
    io.get_ref()
        .is_some_and(|inner| inner.to_string().contains("handshake timed out"))
}

/// Strip separators the TV's on-screen code is sometimes read back with and
/// require exactly six digits, so a malformed code fails here rather than
/// burning the one guess the pairing service allows per displayed code.
fn normalize_pairing_code(code: &str) -> Result<String, String> {
    let digits: String = code
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .collect();
    if digits.len() == 6 && digits.chars().all(|c| c.is_ascii_digit()) {
        Ok(digits)
    } else {
        Err("Enter the 6-digit code shown on the TV.".to_string())
    }
}

/// A finished pairing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paired {
    pub message: String,
    /// The mDNS service instance the device named in its pairing `PeerInfo`
    /// (`adb-<serial>-<suffix>`), when it sent one that can be repeated.
    pub instance: Option<String>,
}

/// `PeerInfoType::ADB_DEVICE_GUID`.
const PEER_INFO_DEVICE_GUID: u8 = 1;

/// The instance name in a device's pairing `PeerInfo`. Any other record kind,
/// or a name that is not printable ASCII of DNS-label length, is `None`:
/// the pairing still succeeded, but nothing advertised can be tied to it.
fn peer_instance_name(peer: &PeerInfo) -> Option<String> {
    if peer.kind != PEER_INFO_DEVICE_GUID {
        return None;
    }
    let name = std::str::from_utf8(&peer.data).ok()?;
    is_reportable_instance_name(name).then(|| name.to_string())
}

/// Map a pairing failure to something the user can act on. Every arm is a
/// different next step, so they stay distinct rather than collapsing into one
/// "pairing failed".
fn pair_error_message(error: &PairingError) -> String {
    match error {
        PairingError::CodeMismatch => {
            "The code didn't match. Check the 6 digits on the TV and try again.".to_string()
        }
        PairingError::Connect(_) => {
            "Pairing isn't open on the TV. Open Wireless debugging › Pair device with pairing code and try again.".to_string()
        }
        PairingError::Timeout => {
            "The TV stopped responding while pairing. Reopen Pair device with pairing code on the TV and try again.".to_string()
        }
        PairingError::Key(e) => format!("Couldn't use this app's ADB key: {e}"),
        PairingError::Tls(_) | PairingError::Protocol(_) | PairingError::Io(_) => {
            "Couldn't finish pairing with the TV. Reopen Pair device with pairing code on the TV — the code changes every time — and try again.".to_string()
        }
    }
}

/// Generate a persistent 2048-bit RSA ADB identity (PKCS#8 PEM) at `path` if it
/// does not already exist. adb_client reads this key for both the RSA AUTH
/// handshake (legacy `:5555`) and the TLS client cert (Android-11), and its TLS
/// upgrade path requires the file to be present on disk — so we create it up
/// front rather than letting adb_client generate an ephemeral in-memory key.
fn ensure_adb_key(path: &Path, allow_creation: bool) -> Result<(), String> {
    ensure_adb_key_with(path, allow_creation, generate_identity_pem)
}

fn ensure_adb_key_with(
    path: &Path,
    allow_creation: bool,
    generate: impl FnOnce() -> Result<Vec<u8>, String>,
) -> Result<(), String> {
    static KEY_INITIALIZATION: Mutex<()> = Mutex::new(());
    let _initialization = lock_recovering(&KEY_INITIALIZATION);
    // Only a definite "not there" may create a key: a new key de-authorizes
    // every TV that trusted the old one, so a transient error must not.
    match path.try_exists() {
        Ok(true) => return check_existing_key(path),
        Ok(false) => {}
        Err(e) => {
            return Err(format!(
                "Could not check this device's ADB identity ({e}); it was left as it was."
            ))
        }
    }
    if !allow_creation {
        return Err(
            "Direct ADB requires an existing PKCS#8 identity; no key was created or replaced."
                .into(),
        );
    }
    let parent = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    std::fs::create_dir_all(parent).map_err(|e| format!("create key dir: {e}"))?;
    let pem = generate().map_err(|e| format!("generate ADB key: {e}"))?;
    // tempfile creates the file owner-only, like the key it becomes.
    let mut staged = tempfile::Builder::new()
        .prefix(".adb_key.")
        .tempfile_in(parent)
        .map_err(|e| format!("write ADB key: {e}"))?;
    std::io::Write::write_all(&mut staged, &pem).map_err(|e| format!("write ADB key: {e}"))?;
    staged
        .as_file()
        .sync_all()
        .map_err(|e| format!("write ADB key: {e}"))?;
    match staged.persist_noclobber(path) {
        Ok(_) => {}
        // Another caller published one first; use it.
        Err(e) if e.error.kind() == std::io::ErrorKind::AlreadyExists => {
            return check_existing_key(path)
        }
        Err(e) => return Err(format!("write ADB key: {}", e.error)),
    }
    #[cfg(unix)]
    if let Ok(dir) = std::fs::File::open(parent) {
        let _ = dir.sync_all();
    }
    tracing::info!(path = %path.display(), "generated persistent ADB RSA key");
    Ok(())
}

/// Starts every error about a key file that exists but cannot be used. The
/// key is never replaced automatically; the app has to offer a deliberate
/// reset, since every TV must then approve this device again.
pub const DAMAGED_ADB_IDENTITY: &str = "This device's ADB identity is damaged";

/// A structural check only (PEM, PKCS#8, RSA key sequence): it catches a
/// truncated or overwritten file without the RSA arithmetic that full
/// validation costs on every connect.
fn key_file_is_well_formed(bytes: &[u8]) -> Result<(), &'static str> {
    use rsa::pkcs8::der::Decode;
    if bytes.len() > 32768 {
        return Err("larger than any RSA key");
    }
    let text = std::str::from_utf8(bytes).map_err(|_| "not PEM text")?;
    let (label, document) =
        rsa::pkcs8::SecretDocument::from_pem(text).map_err(|_| "not a complete PEM key")?;
    if label != "PRIVATE KEY" {
        return Err("not a PKCS#8 private key");
    }
    let info = rsa::pkcs8::PrivateKeyInfo::from_der(document.as_bytes())
        .map_err(|_| "not a PKCS#8 private key")?;
    rsa::pkcs1::RsaPrivateKey::from_der(info.private_key).map_err(|_| "not an RSA key")?;
    Ok(())
}

fn check_existing_key(path: &Path) -> Result<(), String> {
    let bytes = std::fs::read(path).map_err(|e| {
        format!("Could not read this device's ADB identity ({e}); it was left as it was.")
    })?;
    key_file_is_well_formed(&bytes).map_err(|reason| {
        format!(
            "{DAMAGED_ADB_IDENTITY} ({reason}). It was not replaced, because a new identity \
             means every TV has to approve this device again."
        )
    })
}

/// Live connection state for the frontend watchdog (`wireless_status`).
#[derive(Serialize)]
pub struct WirelessStatus {
    pub connected: bool,
    pub serial: Option<String>,
    pub host: Option<String>,
}

impl WirelessStatus {
    fn disconnected() -> Self {
        Self {
            connected: false,
            serial: None,
            host: None,
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn authorization_observation_tracks_only_the_current_live_handshake() {
        use super::{AuthorizationObservation, Leases, WirelessAdb};
        let driver = WirelessAdb::with_existing_key("never-created".into(), Leases::new());
        driver.begin_request(1).unwrap();
        let first = AuthorizationObservation {
            requests: driver.requests.clone(),
            request_id: 1,
            offered: Default::default(),
        };
        assert!(!driver.awaiting_authorization());
        first.public_key_sent();
        assert!(driver.awaiting_authorization());
        driver.begin_request(2).unwrap();
        assert!(!driver.awaiting_authorization());
        first.public_key_sent();
        assert!(!driver.awaiting_authorization());
        let second = AuthorizationObservation {
            requests: driver.requests.clone(),
            request_id: 2,
            offered: Default::default(),
        };
        second.public_key_sent();
        assert!(driver.awaiting_authorization());
        drop(first);
        assert!(driver.awaiting_authorization());
        drop(second);
        assert!(!driver.awaiting_authorization());
    }

    #[test]
    fn canceled_handshakes_cannot_publish_or_supersede_a_retry() {
        let mut requests = super::ConnectionRequests::default();
        requests.begin(100).unwrap();
        requests.begin(101).unwrap();
        assert!(requests.check(100).is_err());
        requests.begin(102).unwrap();
        assert!(requests.begin(101).is_err());
        assert!(requests.check(100).is_err());
        assert!(requests.check(102).is_ok());
    }

    use super::{
        connect_error_message, download_error_message, exit_code_for, is_fatal_transport_error,
        normalize_pairing_code, pair_error_message, peer_instance_name, request_error_message,
        shell_timeout_for, validate_requested_serial, CONNECTION_LOST_PREFIX,
        SHELL_INACTIVITY_TIMEOUT, SLOW_SHELL_INACTIVITY_TIMEOUT,
    };
    use adb_client::tcp::{PairingError, PeerInfo};
    use adb_client::{RustADBError, ShellExit};
    use std::io::{Error, ErrorKind};

    #[test]
    fn answers_from_the_device_keep_the_connection_but_lost_framing_does_not() {
        for answered in [
            RustADBError::SyncFailed("push: No space left on device".to_string()),
            RustADBError::InstallFailed("Failure [INSTALL_FAILED_OLDER_SDK]".to_string()),
            RustADBError::LocalIo(Error::new(ErrorKind::StorageFull, "phone full")),
            RustADBError::ShellExitUnknown,
            RustADBError::FileTooLarge(10, 5),
        ] {
            assert!(!is_fatal_transport_error(&answered), "{answered:?}");
        }
        assert!(is_fatal_transport_error(&RustADBError::ProtocolError(
            "the connection lost ADB message framing after an interrupted read or write; reconnect"
                .to_string(),
        )));
    }

    #[test]
    fn only_shell_v2_reports_a_real_exit_status() {
        assert_eq!(exit_code_for(ShellExit::Code(0)), 0);
        assert_eq!(exit_code_for(ShellExit::Code(255)), 255);
        // The legacy shell has no status; its text is judged instead.
        assert_eq!(exit_code_for(ShellExit::Unreported), 0);
    }

    #[test]
    fn a_handshake_timeout_does_not_claim_an_approval_prompt_was_shown() {
        let message = connect_error_message(&RustADBError::IOError(Error::new(
            ErrorKind::TimedOut,
            "ADB connection handshake timed out",
        )));
        assert!(message.contains("Allow"));
        assert!(message.contains("timed out"));
        assert!(message.contains("If no prompt appears"));
        assert!(!message.contains("didn't approve"));
        assert!(!message.contains("handshake"));
    }

    #[test]
    fn a_quiet_connection_failure_never_mentions_a_prompt() {
        for error in [
            RustADBError::IOError(Error::new(
                ErrorKind::TimedOut,
                "ADB connection handshake timed out",
            )),
            RustADBError::IOError(Error::from(ErrorKind::TimedOut)),
            RustADBError::IOError(Error::from(ErrorKind::ConnectionReset)),
            RustADBError::IOError(Error::from(ErrorKind::UnexpectedEof)),
            RustADBError::IOError(Error::from(ErrorKind::ConnectionAborted)),
            RustADBError::IOError(Error::from(ErrorKind::ConnectionRefused)),
            RustADBError::IOError(Error::from(ErrorKind::HostUnreachable)),
            RustADBError::ProtocolError("bad".into()),
            RustADBError::TlsConnectionClosed(Error::from(ErrorKind::ConnectionReset)),
        ] {
            let message = super::quiet_connect_error_message(&error);
            assert!(!message.contains("prompt"), "{error:?}: {message}");
            assert!(!message.contains("Allow"), "{error:?}: {message}");
            assert!(!message.contains("approve"), "{error:?}: {message}");
        }
        assert!(
            super::quiet_connect_error_message(&RustADBError::IOError(Error::new(
                ErrorKind::TimedOut,
                "ADB connection handshake timed out",
            )))
            .contains("stopped answering")
        );
    }

    #[test]
    fn a_dedicated_connection_the_tv_no_longer_allows_says_how_to_fix_it() {
        let message = super::dedicated_error(&RustADBError::KeyNotAllowed);
        assert!(message.contains("no longer allows this computer"));
        assert!(message.contains("Request approval"));
        assert!(message.contains("Always allow from this computer"));
        assert_eq!(
            super::dedicated_error(&RustADBError::ShellExitUnknown),
            RustADBError::ShellExitUnknown.to_string()
        );
    }

    #[test]
    fn a_tcp_connect_timeout_never_mentions_an_approval_prompt() {
        // `TcpStream::connect_timeout` fails with a bare OS timeout.
        for kind in [ErrorKind::TimedOut, ErrorKind::WouldBlock] {
            let message = connect_error_message(&RustADBError::IOError(Error::from(kind)));
            assert!(message.starts_with("Connection timed out"), "{message}");
            assert!(message.contains("IP address"), "{message}");
            assert!(!message.contains("Allow"), "{message}");
            assert!(!message.contains("30 seconds"), "{message}");
        }
    }

    #[test]
    fn an_unreachable_network_says_so() {
        for kind in [ErrorKind::NetworkUnreachable, ErrorKind::HostUnreachable] {
            let message = connect_error_message(&RustADBError::IOError(Error::from(kind)));
            assert!(message.starts_with("Network is unreachable"), "{message}");
            assert!(!message.contains("Allow"), "{message}");
        }
    }

    #[test]
    fn an_unpaired_key_explains_how_to_pair() {
        let message = connect_error_message(&RustADBError::TlsAuthRejected(
            "TLS alert CertificateUnknown".to_string(),
        ));
        assert!(message.contains("hasn't paired"));
        assert!(message.contains("Pair device with pairing code"));
        assert!(message.contains("pairing port"));
        assert!(!message.contains("CertificateUnknown"));
        assert!(!message.contains("Allow debugging"));
    }

    #[test]
    fn a_closed_secure_connection_hedges_instead_of_claiming_unpaired() {
        let message = connect_error_message(&RustADBError::TlsConnectionClosed(Error::new(
            ErrorKind::ConnectionReset,
            "raw reset",
        )));
        assert!(message.contains("closed the secure connection"));
        assert!(message.contains("If this keeps happening, pair again"));
        assert!(!message.contains("hasn't paired"));
        assert!(!message.contains("raw reset"));
        assert!(is_fatal_transport_error(
            &RustADBError::TlsConnectionClosed(Error::new(ErrorKind::UnexpectedEof, "eof"),)
        ));
    }

    #[test]
    fn a_command_without_a_result_reads_as_unknown_not_as_a_parse_or_lost_connection() {
        assert!(!is_fatal_transport_error(&RustADBError::ShellExitUnknown));
        let message = request_error_message(&RustADBError::ShellExitUnknown);
        assert!(message.contains("unknown"), "{message}");
        assert!(!message.contains("pars"), "{message}");
        assert!(!message.contains(CONNECTION_LOST_PREFIX), "{message}");
        assert_eq!(
            request_error_message(&RustADBError::ADBRequestFailed("x".to_string())),
            RustADBError::ADBRequestFailed("x".to_string()).to_string()
        );
    }

    #[test]
    fn an_oversized_download_keeps_the_connection_and_says_why() {
        let error = RustADBError::FileTooLarge(3 << 30, 2 << 30);
        assert!(!is_fatal_transport_error(&error));
        assert_eq!(
            download_error_message(&error),
            "Download exceeds the file size limit."
        );
    }

    #[test]
    fn existing_connect_guidance_is_unchanged_by_new_errors() {
        let timeout = connect_error_message(&RustADBError::IOError(Error::new(
            ErrorKind::WouldBlock,
            "TLS handshake timed out",
        )));
        assert!(timeout.contains("timed out"));
        assert!(!timeout.contains("didn't approve"));
        let closed = connect_error_message(&RustADBError::IOError(Error::new(
            ErrorKind::UnexpectedEof,
            "raw",
        )));
        assert!(closed.contains("closed the connection"));
        let protocol =
            connect_error_message(&RustADBError::ProtocolError("bad header magic".to_string()));
        assert!(protocol.contains("Allow debugging"));
        assert!(!protocol.contains("magic"));
    }

    #[test]
    fn framing_and_trust_failures_evict_but_a_refused_oversized_write_does_not() {
        assert!(is_fatal_transport_error(&RustADBError::ProtocolError(
            "bad header magic".to_string(),
        )));
        assert!(is_fatal_transport_error(&RustADBError::InvalidIntegrity(
            1, 2
        )));
        assert!(is_fatal_transport_error(&RustADBError::TlsAuthRejected(
            "TLS alert UnknownCA".to_string(),
        )));
        assert!(!is_fatal_transport_error(&RustADBError::PayloadTooLarge(
            70_000, 4_096,
        )));
    }

    #[test]
    fn only_socket_level_errors_are_fatal() {
        assert!(is_fatal_transport_error(&RustADBError::IOError(
            Error::new(ErrorKind::TimedOut, "read timed out",)
        )));
        assert!(!is_fatal_transport_error(
            &RustADBError::UnknownResponseType("mode is 0: source file does not exist".to_string(),)
        ));
        assert!(!is_fatal_transport_error(&RustADBError::ADBRequestFailed(
            "unknown service".to_string(),
        )));
    }

    #[test]
    fn installs_get_the_slow_shell_timeout() {
        assert_eq!(
            shell_timeout_for("pm install-multiple -r /data/local/tmp/a.apk"),
            SLOW_SHELL_INACTIVITY_TIMEOUT
        );
        assert_eq!(
            shell_timeout_for("pm list packages -e"),
            SHELL_INACTIVITY_TIMEOUT
        );
        assert_eq!(
            shell_timeout_for("  pm clear com.example"),
            SLOW_SHELL_INACTIVITY_TIMEOUT
        );
    }

    #[test]
    fn requested_serial_must_match_active_connection() {
        assert!(validate_requested_serial("tv-a:5555", "tv-a:5555").is_ok());
        assert_eq!(
            validate_requested_serial("tv-b:5555", "tv-a:5555").unwrap_err(),
            "Connected device is tv-b:5555, not tv-a:5555."
        );
    }

    #[test]
    fn connect_timeout_does_not_claim_authorization_was_reached() {
        let message = connect_error_message(&RustADBError::IOError(Error::new(
            ErrorKind::TimedOut,
            "raw OS timeout",
        )));
        assert!(message.contains("current IP"));
        assert!(!message.contains("didn't approve"));
        assert!(!message.contains("raw OS timeout"));
    }

    #[test]
    fn refused_connect_explains_network_debugging() {
        let message = connect_error_message(&RustADBError::IOError(Error::new(
            ErrorKind::ConnectionRefused,
            "raw OS refusal",
        )));
        assert!(message.contains("Network debugging"));
        assert!(!message.contains("raw OS refusal"));
    }

    #[test]
    fn the_device_guid_from_pairing_is_its_instance_name_when_it_is_usable() {
        let peer = |kind: u8, data: &[u8]| PeerInfo {
            kind,
            data: data.to_vec(),
        };
        assert_eq!(
            peer_instance_name(&peer(1, b"adb-EMULATOR37X1X11X0-LgzOi6")).as_deref(),
            Some("adb-EMULATOR37X1X11X0-LgzOi6")
        );
        assert_eq!(
            peer_instance_name(&peer(0, b"adb-EMULATOR37X1X11X0-LgzOi6")),
            None,
            "an RSA key record is not a name"
        );
        let unusable: [&[u8]; 6] = [
            b"",
            b"adb-SERIAL x-LgzOi6",
            b"adb-SERIAL-Lgz\nOi6",
            b"adb-SERIAL]-x [guid=adb-OTHER-y",
            b"adb-\xff-LgzOi6",
            b"adb-\xc3\xa9-LgzOi6",
        ];
        for name in unusable {
            assert_eq!(peer_instance_name(&peer(1, name)), None, "{name:?}");
        }
        let longest = format!("adb-{}-x", "S".repeat(57));
        assert_eq!(longest.len(), 63);
        assert_eq!(
            peer_instance_name(&peer(1, longest.as_bytes())),
            Some(longest.clone())
        );
        let too_long = format!("adb-{}-x", "S".repeat(58));
        assert_eq!(peer_instance_name(&peer(1, too_long.as_bytes())), None);
    }

    #[test]
    fn pairing_codes_are_normalized_before_the_one_allowed_guess() {
        assert_eq!(normalize_pairing_code("642091").unwrap(), "642091");
        assert_eq!(normalize_pairing_code(" 642 091 ").unwrap(), "642091");
        assert_eq!(normalize_pairing_code("642-091").unwrap(), "642091");
        for bad in ["64209", "6420911", "", "abcdef", "64209a"] {
            assert_eq!(
                normalize_pairing_code(bad).unwrap_err(),
                "Enter the 6-digit code shown on the TV."
            );
        }
    }

    #[test]
    fn pairing_errors_name_the_next_step() {
        let message = pair_error_message(&PairingError::CodeMismatch);
        assert!(message.contains("6 digits"));

        let message = pair_error_message(&PairingError::Connect(Error::new(
            ErrorKind::ConnectionRefused,
            "raw OS refusal",
        )));
        assert!(message.contains("Pair device with pairing code"));
        assert!(!message.contains("raw OS refusal"));

        let message = pair_error_message(&PairingError::Timeout);
        assert!(message.contains("stopped responding"));

        let message = pair_error_message(&PairingError::Protocol("raw detail".into()));
        assert!(!message.contains("raw detail"));
    }
}

#[cfg(test)]
mod lifecycle_tests {
    use super::*;

    #[tokio::test]
    async fn canceled_blocking_worker_remains_counted_until_it_releases_the_socket_lock() {
        let operations = Leases::new();
        let driver = Arc::new(WirelessAdb::with_operations(
            PathBuf::from("never-created"),
            operations.clone(),
        ));
        let conn = driver.conn.clone();
        let (held, held_rx) = tokio::sync::oneshot::channel();
        let (release, release_rx) = std::sync::mpsc::channel();
        let holder = std::thread::spawn(move || {
            let _guard = lock_recovering(&conn);
            held.send(()).unwrap();
            release_rx.recv().unwrap();
        });
        held_rx.await.unwrap();
        let worker_driver = driver.clone();
        let worker =
            tokio::spawn(
                async move { worker_driver.on_device("192.0.2.1:5555", |_| Ok(())).await },
            );
        while operations.active() == 0 {
            tokio::task::yield_now().await;
        }
        worker.abort();
        assert!(worker.await.unwrap_err().is_cancelled());
        assert_eq!(operations.active(), 1);
        operations.close();
        assert!(driver.work().is_err());
        release.send(()).unwrap();
        assert!(
            operations
                .wait_idle(tokio::time::Instant::now() + Duration::from_secs(2))
                .await
        );
        holder.join().unwrap();
    }

    #[test]
    fn service_stream_keeps_its_operation_until_drop() {
        let operations = Leases::new();
        let stream = CountedStream {
            stream: Box::new(std::io::Cursor::new(Vec::<u8>::new())),
            _operation: Arc::new(operations.try_acquire().unwrap()),
        };
        assert_eq!(operations.active(), 1);
        drop(stream);
        assert_eq!(operations.active(), 0);
    }
    #[test]
    fn required_identity_policy_does_not_create_or_replace_keys() {
        let temp = tempfile::tempdir().unwrap();
        let missing = temp.path().join("missing-key");
        assert!(ensure_adb_key(&missing, false).is_err());
        assert!(!missing.exists());
        let existing = temp.path().join("existing-key");
        let pem = std::fs::read(crate::connection_tests::fixture_key().path()).unwrap();
        std::fs::write(&existing, &pem).unwrap();
        ensure_adb_key(&existing, false).unwrap();
        assert_eq!(std::fs::read(existing).unwrap(), pem);
    }

    #[test]
    fn a_damaged_identity_is_reported_and_never_replaced() {
        let temp = tempfile::tempdir().unwrap();
        for body in [
            &b""[..],
            b"public test marker, not a credential",
            b"-----BEGIN PRIVATE KEY-----\nAAAA",
        ] {
            let damaged = temp.path().join("damaged-key");
            std::fs::write(&damaged, body).unwrap();
            for allow in [true, false] {
                let error = ensure_adb_key(&damaged, allow).unwrap_err();
                assert!(error.starts_with(super::DAMAGED_ADB_IDENTITY), "{error}");
                assert_eq!(std::fs::read(&damaged).unwrap(), body);
            }
        }
    }

    #[test]
    fn a_new_identity_is_complete_private_and_leaves_no_temp() {
        let temp = tempfile::tempdir().unwrap();
        let key = temp.path().join("keys").join("adb_key");
        // The test key, so the suite does no slow debug-build RSA generation.
        let fixture = std::fs::read(crate::connection_tests::fixture_key().path()).unwrap();
        ensure_adb_key_with(&key, true, || Ok(fixture.clone())).unwrap();
        assert_eq!(std::fs::read(&key).unwrap(), fixture);
        let names: Vec<_> = std::fs::read_dir(key.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, vec![std::ffi::OsString::from("adb_key")]);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&key).unwrap().permissions().mode();
            assert_eq!(mode & 0o077, 0, "{mode:o}");
        }
        ensure_adb_key_with(&key, true, || {
            panic!("an existing key is never regenerated")
        })
        .unwrap();
        assert_eq!(std::fs::read(&key).unwrap(), fixture);
    }

    #[cfg(unix)]
    #[test]
    fn an_identity_that_cannot_be_checked_is_not_regenerated() {
        let temp = tempfile::tempdir().unwrap();
        let not_a_dir = temp.path().join("file");
        std::fs::write(&not_a_dir, b"x").unwrap();
        let error = ensure_adb_key(&not_a_dir.join("adb_key"), true).unwrap_err();
        assert!(error.contains("left as it was"), "{error}");
        assert_eq!(std::fs::read(&not_a_dir).unwrap(), b"x");
    }

    #[tokio::test]
    async fn controlled_command_deadline_includes_queue_and_retains_lease_until_worker_returns() {
        let operations = Leases::new();
        let driver = Arc::new(WirelessAdb::with_existing_key(
            "never-created".into(),
            operations.clone(),
        ));
        *lock_recovering(&driver.identity) = Some(Identity {
            serial: "fixture".into(),
            host: "fixture".into(),
            port: 0,
        });
        let conn = driver.conn.clone();
        let (held, held_rx) = tokio::sync::oneshot::channel();
        let (release, release_rx) = std::sync::mpsc::channel();
        let holder = std::thread::spawn(move || {
            let _guard = lock_recovering(&conn);
            held.send(()).unwrap();
            release_rx.recv().unwrap();
        });
        held_rx.await.unwrap();
        assert!(driver
            .on_device_controlled(
                "fixture",
                Duration::from_millis(20),
                |_, _| -> Result<(), RustADBError> {
                    panic!("expired queued command must not execute")
                }
            )
            .await
            .is_err());
        assert_eq!(operations.active(), 1);
        assert_eq!(driver.info().unwrap().0, "fixture");
        release.send(()).unwrap();
        holder.join().unwrap();
        assert!(
            operations
                .wait_idle(tokio::time::Instant::now() + Duration::from_secs(2))
                .await
        );
        assert_eq!(driver.info().unwrap().0, "fixture");
    }

    #[tokio::test]
    async fn canceled_queued_command_keeps_worker_lease_and_does_not_clear_identity() {
        let operations = Leases::new();
        let driver = Arc::new(WirelessAdb::with_existing_key(
            "never-created".into(),
            operations.clone(),
        ));
        *lock_recovering(&driver.identity) = Some(Identity {
            serial: "fixture".into(),
            host: "fixture".into(),
            port: 0,
        });
        let conn = driver.conn.clone();
        let (held, held_rx) = tokio::sync::oneshot::channel();
        let (release, release_rx) = std::sync::mpsc::channel();
        let holder = std::thread::spawn(move || {
            let _guard = lock_recovering(&conn);
            held.send(()).unwrap();
            release_rx.recv().unwrap();
        });
        held_rx.await.unwrap();
        let command = tokio::spawn({
            let driver = driver.clone();
            async move {
                driver
                    .on_device_controlled(
                        "fixture",
                        Duration::from_secs(2),
                        |_, _| -> Result<(), RustADBError> {
                            panic!("canceled queued command executed")
                        },
                    )
                    .await
            }
        });
        while operations.active() == 0 {
            tokio::task::yield_now().await;
        }
        command.abort();
        assert!(command.await.unwrap_err().is_cancelled());
        assert_eq!(operations.active(), 1);
        release.send(()).unwrap();
        holder.join().unwrap();
        assert!(
            operations
                .wait_idle(tokio::time::Instant::now() + Duration::from_secs(2))
                .await
        );
        assert_eq!(driver.info().unwrap().0, "fixture");
    }

    #[tokio::test]
    async fn generic_service_refuses_missing_original_identity_before_dedicated_connection() {
        let temp = tempfile::tempdir().unwrap();
        let missing = temp.path().join("never-created-service-key");
        let operations = Leases::new();
        let driver = WirelessAdb::with_existing_key(missing.clone(), operations.clone());
        assert!(driver
            .open_device_service("127.0.0.1:0", "shell:fixture")
            .await
            .is_err());
        *lock_recovering(&driver.identity) = Some(Identity {
            serial: "127.0.0.1:0".into(),
            host: "127.0.0.1".into(),
            port: 0,
        });
        let error = driver
            .open_device_service("127.0.0.1:0", "shell:fixture")
            .await
            .err()
            .unwrap();
        assert!(error.to_string().contains("Not connected"));
        assert_eq!(operations.active(), 0);
        assert!(!missing.exists());
        let error = driver
            .open_device_service("127.0.0.1:1", "shell:fixture")
            .await
            .err()
            .unwrap();
        assert!(error.to_string().contains("not 127.0.0.1:1"));
        operations.close();
        assert!(driver
            .open_device_service("127.0.0.1:0", "shell:fixture")
            .await
            .err()
            .unwrap()
            .to_string()
            .contains("quitting"));
        assert!(!missing.exists());
    }

    #[test]
    fn required_identity_constructors_reject_missing_key_before_network_or_generation() {
        let temp = tempfile::tempdir().unwrap();
        let key = temp.path().join("missing-key");
        let endpoint: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let device = ADBTcpDevice::new_with_required_private_key_and_auth_timeout(
            endpoint,
            &key,
            Duration::from_secs(1),
        );
        assert!(
            matches!(device, Err(RustADBError::IOError(error)) if error.kind() == std::io::ErrorKind::NotFound)
        );
        let service = adb_client::tcp::ADBTcpService::new_with_required_key_and_timeouts(
            endpoint,
            "shell:id",
            &key,
            Duration::from_secs(1),
            Duration::from_secs(1),
        );
        assert!(
            matches!(service, Err(RustADBError::IOError(error)) if error.kind() == std::io::ErrorKind::NotFound)
        );
        assert!(!key.exists());
    }
}

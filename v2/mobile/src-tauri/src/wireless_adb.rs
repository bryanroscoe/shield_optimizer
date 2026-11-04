use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use adb_client::tcp::{ADBTcpDevice, ADBTcpService, PairingError};
use adb_client::{ADBDeviceExt, RebootType, RustADBError, ShellExit};
use async_trait::async_trait;
use serde::Serialize;
use shield_optimizer_core::adb::{AdbByteStream, AdbDriver, AdbError, AdbOutput, AdbResult};
use shield_optimizer_core::commands::devices::normalize_connect_address;

use tauri_plugin_atv_adb::AdbExt;

/// How long Android NSD browses for wireless-debugging services.
pub const DISCOVER_TIMEOUT_MS: u64 = 3_000;

// Re-exported so wireless_commands.rs and the mobile handler keep one import path.
pub use tauri_plugin_atv_adb::DiscoveredAdbDevice;

const AUTHORIZATION_TIMEOUT: Duration = Duration::from_secs(30);
/// Inactivity bound for one shell response, matching the desktop driver's
/// 30-second command timeout. A silently dropped socket surfaces as an error
/// here instead of pinning the connection mutex forever.
const SHELL_INACTIVITY_TIMEOUT: Duration = Duration::from_secs(30);
/// Package installs and clears can legitimately produce nothing for a while.
const SLOW_SHELL_INACTIVITY_TIMEOUT: Duration = Duration::from_secs(180);
const LIVENESS_PROBE_TIMEOUT: Duration = Duration::from_secs(5);
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
}

impl ConnectionRequests {
    fn begin(&mut self, request_id: u64) -> Result<(), String> {
        if request_id <= self.latest {
            return Err("Connection attempt superseded.".into());
        }
        self.latest = request_id;
        Ok(())
    }

    fn check(&self, request_id: u64) -> Result<(), String> {
        if request_id != self.latest {
            return Err("Connection attempt canceled.".into());
        }
        Ok(())
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
/// server, and no GPL Kotlin transport. The only thing still delegated to the
/// Kotlin plugin is **mDNS discovery** (Android's `NsdManager`), which is pure
/// framework code.
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
    app: tauri::AppHandle,
    /// PKCS#8 PEM ADB private key. Persisted so re-auth is silent across launches.
    key_path: PathBuf,
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
    pub fn new(app: tauri::AppHandle, key_path: PathBuf) -> Self {
        Self {
            app,
            key_path,
            conn: Arc::new(Mutex::new(None)),
            identity: Arc::new(Mutex::new(None)),
            lifecycle: tokio::sync::Mutex::new(()),
            requests: Arc::new(Mutex::new(ConnectionRequests::default())),
        }
    }

    /// Run a blocking plugin call (mDNS discovery) off the async runtime.
    async fn on_blocking<T, F>(&self, f: F) -> Result<T, String>
    where
        T: Send + 'static,
        F: FnOnce(&tauri::AppHandle) -> T + Send + 'static,
    {
        let app = self.app.clone();
        tokio::task::spawn_blocking(move || f(&app))
            .await
            .map_err(|e| format!("wireless-adb task failed: {e}"))
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
        let conn = self.conn.clone();
        let identity = self.identity.clone();
        let requested_serial = serial.to_string();
        tokio::task::spawn_blocking(move || {
            let mut guard = lock_recovering(&conn);
            let Some(active) = guard.as_mut() else {
                return Err("Not connected to a device.".to_string());
            };
            validate_requested_serial(&active.serial, &requested_serial)?;
            let error = match f(&mut active.device) {
                Ok(value) => return Ok(value),
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

    /// Snapshot of the connected device's identity. Reads the mirror, never
    /// the device mutex, so it returns immediately even while a command is
    /// mid-flight on the socket.
    fn info(&self) -> Option<(String, String, u16)> {
        lock_recovering(&self.identity)
            .as_ref()
            .map(|c| (c.serial.clone(), c.host.clone(), c.port))
    }

    pub async fn discover(&self) -> Result<Vec<DiscoveredAdbDevice>, String> {
        self.on_blocking(|app| app.adb().discover(DISCOVER_TIMEOUT_MS))
            .await?
            .map_err(|e| e.to_string())
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
        let addr: SocketAddr = format!("{host}:{port}")
            .parse()
            .map_err(|e| format!("invalid pairing address {host}:{port}: {e}"))?;
        let code = normalize_pairing_code(code)?;
        let key_path = self.key_path.clone();

        // `adb_client::tcp::pair` is blocking and applies its own 30-second
        // deadline through socket timeouts, so it cannot pin this thread.
        tokio::task::spawn_blocking(move || -> Result<String, String> {
            ensure_adb_key(&key_path)?;
            match adb_client::tcp::pair(addr, &code, &key_path) {
                Ok(outcome) => {
                    tracing::info!(
                        peer_kind = outcome.peer_info.kind,
                        peer = %outcome.peer_info.data_lossy(),
                        "wireless pairing succeeded"
                    );
                    Ok("Paired with the TV.".to_string())
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

    pub async fn cancel_connect(
        &self,
        request_id: u64,
        canceled_request_id: u64,
    ) -> Result<(), String> {
        self.begin_request(request_id)?;
        let conn = self.conn.clone();
        let identity = self.identity.clone();
        let requests = self.requests.clone();
        tokio::task::spawn_blocking(move || {
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

    pub async fn connect(&self, host: &str, port: u16, request_id: u64) -> Result<String, String> {
        let _lifecycle = self.lifecycle.lock().await;
        lock_recovering(&self.requests).check(request_id)?;
        normalize_connect_address(&format!("{host}:{port}"))?;
        let addr: SocketAddr = format!("{host}:{port}")
            .parse()
            .map_err(|e| format!("invalid device address {host}:{port}: {e}"))?;
        let key_path = self.key_path.clone();
        let conn = self.conn.clone();
        let identity = self.identity.clone();
        let requests = self.requests.clone();
        let host_owned = host.to_string();
        let serial = format!("{host}:{port}");

        tokio::task::spawn_blocking(move || -> Result<String, String> {
            ensure_adb_key(&key_path)?;
            // First connect to a not-yet-authorized device triggers the TV's
            // "Allow debugging?" prompt; the persisted key is remembered after.
            let device = ADBTcpDevice::new_with_custom_private_key_and_auth_timeout(
                addr,
                &key_path,
                AUTHORIZATION_TIMEOUT,
            )
            .map_err(|e| {
                tracing::warn!(serial = %serial, error = %e, "wireless authorization failed");
                connect_error_message(&e)
            })?;
            let mut guard = lock_recovering(&conn);
            // Cancellation and publication share this short lock. No socket
            // I/O holds it, so a pending handshake can be invalidated promptly.
            let requests = lock_recovering(&requests);
            requests.check(request_id)?;
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
        .map_err(|e| format!("wireless-adb task failed: {e}"))?
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
        tokio::task::spawn_blocking(move || -> Result<(), String> {
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
            .on_device(&serial, |dev| {
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
            Ok(stdout) => stdout.contains("ok"),
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
    async fn pull_limited(
        &self,
        serial: &str,
        remote: &str,
        local: &Path,
        max_bytes: u64,
    ) -> AdbResult<()> {
        let Some((active, host, port)) = self.info() else {
            return Err(AdbError::Transport("Not connected to a device.".into()));
        };
        validate_requested_serial(&active, serial).map_err(AdbError::Transport)?;
        let addr: SocketAddr = format!("{host}:{port}")
            .parse()
            .map_err(|e| AdbError::Transport(format!("invalid download address: {e}")))?;
        let remote = remote.to_string();
        let local = local.to_path_buf();
        let key = self.key_path.clone();
        // A bounded download owns its socket. Aborting on a full disk or a
        // growing file closes that stream without poisoning the control socket
        // or blocking the session's heartbeat behind a multi-minute transfer.
        tokio::task::spawn_blocking(move || -> AdbResult<()> {
            let file = std::fs::File::create(local)?;
            let mut device = ADBTcpDevice::new_with_custom_private_key_and_auth_timeout(
                addr,
                &key,
                AUTHORIZATION_TIMEOUT,
            )
            .map_err(|e| AdbError::Transport(e.to_string()))?;
            // The size check and the transfer share one sync session; the
            // writer still stops a file that grows past the limit meanwhile.
            let mut writer = crate::limited_writer::LimitedWriter::new(file, max_bytes);
            device
                .pull_with_limit(&remote, &mut writer, max_bytes)
                .map_err(|e| AdbError::Transport(download_error_message(&e)))?;
            std::io::Write::flush(&mut writer)?;
            Ok(())
        })
        .await
        .map_err(|e| AdbError::Transport(format!("download task: {e}")))?
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
                self.on_device(serial, move |dev| dev.reboot(reboot_type))
                    .await
                    .map_err(AdbError::Transport)?;
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

    /// File transfer over the sync service. Only the two forms the mobile file
    /// commands emit are supported: `pull` (device → this phone) and `push`
    /// (this phone → device). Local files are opened before touching the live
    /// connection so a phone-storage error cannot evict a healthy TV socket.
    async fn raw_transfer(&self, args: &[&str]) -> AdbResult<AdbOutput> {
        match args {
            ["-s", serial, "pull", remote, local] => {
                let remote = remote.to_string();
                let local = PathBuf::from(local);
                let mut file = std::fs::File::create(&local)
                    .map_err(|e| AdbError::Transport(format!("create {}: {e}", local.display())))?;
                self.on_device(serial, move |dev| {
                    dev.pull(&remote, &mut file)?;
                    Ok(())
                })
                .await
                .map_err(AdbError::Transport)?;
                Ok(AdbOutput {
                    stdout: String::new(),
                    stderr: String::new(),
                    exit_code: Some(0),
                })
            }
            ["-s", serial, "push", local, remote] => {
                let remote = remote.to_string();
                let local = PathBuf::from(local);
                let mut file = std::fs::File::open(&local)
                    .map_err(|e| AdbError::Transport(format!("open {}: {e}", local.display())))?;
                self.on_device(serial, move |dev| {
                    dev.push(&mut file, &remote)?;
                    Ok(())
                })
                .await
                .map_err(AdbError::Transport)?;
                Ok(AdbOutput {
                    stdout: String::new(),
                    stderr: String::new(),
                    exit_code: Some(0),
                })
            }
            ["-s", serial, verb @ ("install" | "install-multiple"), rest @ ..] => {
                let (options, apks) =
                    split_install_args(verb, rest).map_err(AdbError::Transport)?;
                // Like push: a phone-storage error must not reach (and evict)
                // a healthy TV connection.
                for apk in &apks {
                    std::fs::File::open(apk)
                        .map_err(|e| AdbError::Transport(format!("open {}: {e}", apk.display())))?;
                }
                let outcome = self
                    .on_device(serial, move |dev| {
                        let options: Vec<&str> = options.iter().map(String::as_str).collect();
                        install_outcome(dev.install_apks(
                            &apks,
                            &options,
                            SLOW_SHELL_INACTIVITY_TIMEOUT,
                        ))
                    })
                    .await
                    .map_err(AdbError::Transport)?;
                // The same contract as `adb install`: exit 0 with `Success`, or
                // a nonzero exit carrying the package manager's reason.
                match outcome {
                    Ok(stdout) => Ok(AdbOutput {
                        stdout,
                        stderr: String::new(),
                        exit_code: Some(0),
                    }),
                    Err(reason) => Err(AdbError::NonZeroExit {
                        code: Some(1),
                        stderr: reason,
                    }),
                }
            }
            _ => Err(AdbError::Unsupported {
                operation: "raw_transfer",
            }),
        }
    }

    async fn shell(&self, serial: &str, command: &str) -> AdbResult<AdbOutput> {
        let command = command.to_string();
        let timeout = shell_timeout_for(&command);
        let (stdout, stderr, code) = self
            .on_device(serial, move |dev| {
                let mut out: Vec<u8> = Vec::new();
                let mut err: Vec<u8> = Vec::new();
                let code = dev.shell_command_with_timeout(
                    &command,
                    Some(&mut out),
                    Some(&mut err),
                    timeout,
                )?;
                Ok((
                    String::from_utf8_lossy(&out).into_owned(),
                    String::from_utf8_lossy(&err).into_owned(),
                    code,
                ))
            })
            .await
            .map_err(AdbError::Transport)?;
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
        self.on_device(serial, |dev| {
            // `screencap -p` encodes on the TV (~1 MB). The framebuffer service
            // ships raw RGBA (~33 MB at 4K) and is only a fallback. stderr gets
            // its own sink so a warning can never be spliced into the PNG.
            // Without shell v2 this runs over `exec:`, never the legacy
            // shell, whose pty would rewrite the PNG's line endings.
            let mut out: Vec<u8> = Vec::new();
            let mut err: Vec<u8> = Vec::new();
            let shot = dev.exec_out_with_timeout(
                "screencap -p",
                &mut out,
                Some(&mut err),
                SHELL_INACTIVITY_TIMEOUT,
            );
            match shot {
                Ok(exit) if exit.code().unwrap_or(0) == 0 && out.starts_with(b"\x89PNG") => Ok(out),
                _ => dev.framebuffer_bytes(),
            }
        })
        .await
        .map_err(AdbError::Transport)
    }

    async fn open_device_service(
        &self,
        serial: &str,
        service: &str,
    ) -> AdbResult<Box<dyn AdbByteStream>> {
        let Some((active_serial, host, port)) = self.info() else {
            return Err(AdbError::Transport(
                "Not connected to a device.".to_string(),
            ));
        };
        if active_serial != serial {
            return Err(AdbError::Transport(format!(
                "Connected device is {active_serial}, not {serial}."
            )));
        }
        let addr: SocketAddr = format!("{host}:{port}")
            .parse()
            .map_err(|e| AdbError::Transport(format!("invalid device address: {e}")))?;
        let key_path = self.key_path.clone();
        let service = service.to_string();
        tokio::task::spawn_blocking(move || {
            ensure_adb_key(&key_path).map_err(AdbError::Transport)?;
            // The stream's own connection repeats the CNXN/TLS/AUTH handshake,
            // which may need the TV's approval again, so it gets the main
            // connection's bound.
            ADBTcpService::new_with_timeouts_and_auth_timeout(
                addr,
                &service,
                key_path,
                Duration::from_secs(2),
                Duration::from_secs(2),
                AUTHORIZATION_TIMEOUT,
            )
            .map(|stream| Box::new(stream) as Box<dyn AdbByteStream>)
            .map_err(|e| AdbError::Transport(e.to_string()))
        })
        .await
        .map_err(|e| AdbError::Transport(format!("wireless-adb task failed: {e}")))?
    }
}

pub type SharedWirelessAdb = Arc<WirelessAdb>;

/// Errors that mean the socket or its framing is unusable, as opposed to a
/// request adbd answered with a failure. `PayloadTooLarge` is refused before
/// anything reaches the socket, so it leaves the connection intact, as do a
/// sync `FAIL` (`SyncFailed`), a package-manager refusal (`InstallFailed`), a
/// phone-side file error (`LocalIo`), a file over the caller's limit
/// (`FileTooLarge`) and a shell v2 stream that closed without its exit status
/// (`ShellExitUnknown`, still double-checked by the liveness probe). A
/// connection that lost framing mid-message reports `ProtocolError` on every
/// later use.
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
/// never reaches this; the transport returns it as an error.
fn exit_code_for(exit: ShellExit) -> i32 {
    match exit {
        ShellExit::Code(code) => i32::from(code),
        ShellExit::Unreported => 0,
    }
}

/// Split `install`/`install-multiple` arguments into flag options (such as
/// `-r`) and local APK paths. `install` takes exactly one APK.
fn split_install_args(verb: &str, rest: &[&str]) -> Result<(Vec<String>, Vec<PathBuf>), String> {
    let flags = rest.iter().take_while(|arg| arg.starts_with('-')).count();
    let (options, paths) = rest.split_at(flags);
    if let Some(bad) = options.iter().find(|option| {
        option.len() < 2
            || !option
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-')
    }) {
        return Err(format!("Unsupported install option {bad:?}."));
    }
    match (verb, paths.len()) {
        (_, 0) => Err("No APK to install.".to_string()),
        ("install", n) if n > 1 => Err("install takes one APK; use install-multiple.".to_string()),
        _ => Ok((
            options.iter().map(|option| (*option).to_string()).collect(),
            paths.iter().map(PathBuf::from).collect(),
        )),
    }
}

/// A package-manager refusal, or the TV refusing the staged copy on devices
/// without `cmd`, is the install's answer, not a broken connection, so it is
/// kept out of the error path that evicts it.
fn install_outcome(
    result: Result<String, RustADBError>,
) -> Result<Result<String, String>, RustADBError> {
    match result {
        Ok(output) => Ok(Ok(output)),
        Err(RustADBError::InstallFailed(reason)) => Ok(Err(reason)),
        Err(error @ RustADBError::SyncFailed(_)) => Ok(Err(error.to_string())),
        Err(error) => Err(error),
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

fn connect_error_message(error: &RustADBError) -> String {
    match error {
        RustADBError::TlsAuthRejected(_) => {
            "This TV hasn't paired with this phone. Pair it first with the code from Wireless debugging › Pair device with pairing code, then connect again.".to_string()
        }
        // No TLS alert: a network drop looks the same as a silent refusal,
        // so this must not claim the phone is unpaired.
        RustADBError::TlsConnectionClosed(_) => {
            "The TV closed the secure connection. Check that the phone and TV are on the same network and try again. If this keeps happening, pair again from Wireless debugging › Pair device with pairing code.".to_string()
        }
        RustADBError::IOError(io)
            if matches!(
                io.kind(),
                std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
            ) =>
        {
            "The TV didn't approve debugging in time. Try again, then choose Allow on the TV within 30 seconds (and select Always allow when offered).".to_string()
        }
        RustADBError::IOError(io)
            if matches!(
                io.kind(),
                std::io::ErrorKind::ConnectionAborted
                    | std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::UnexpectedEof
            ) =>
        {
            "The TV closed the connection before authorization finished. Try again and choose Allow on the TV prompt within 30 seconds.".to_string()
        }
        RustADBError::IOError(io)
            if matches!(io.kind(), std::io::ErrorKind::ConnectionRefused) =>
        {
            "Couldn't reach the TV. Confirm Network debugging is enabled and the phone and TV are on the same network, then try again.".to_string()
        }
        _ => "Couldn't finish the secure ADB connection. Look for an Allow debugging prompt on the TV, approve it, and try again.".to_string(),
    }
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
        PairingError::Key(e) => format!("Couldn't use this phone's ADB key: {e}"),
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
fn ensure_adb_key(path: &Path) -> Result<(), String> {
    if path.exists() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create key dir: {e}"))?;
    }
    use rsa::pkcs8::{EncodePrivateKey, LineEnding};
    let key = rsa::RsaPrivateKey::new(&mut rsa::rand_core::OsRng, 2048)
        .map_err(|e| format!("generate ADB key: {e}"))?;
    let pem = key
        .to_pkcs8_pem(LineEnding::LF)
        .map_err(|e| format!("encode ADB key: {e}"))?;
    std::fs::write(path, pem.as_bytes()).map_err(|e| format!("write ADB key: {e}"))?;
    tracing::info!(path = %path.display(), "generated persistent ADB RSA key");
    Ok(())
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
        connect_error_message, download_error_message, exit_code_for, install_outcome,
        is_fatal_transport_error, normalize_pairing_code, pair_error_message,
        request_error_message, shell_timeout_for, split_install_args, validate_requested_serial,
        CONNECTION_LOST_PREFIX, SHELL_INACTIVITY_TIMEOUT, SLOW_SHELL_INACTIVITY_TIMEOUT,
    };
    use adb_client::tcp::PairingError;
    use adb_client::{RustADBError, ShellExit};
    use std::io::{Error, ErrorKind};
    use std::path::PathBuf;

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
    fn install_arguments_split_into_flags_and_apks() {
        let (options, apks) = split_install_args("install-multiple", &["-r", "/a.apk", "/b.apk"])
            .expect("valid install-multiple");
        assert_eq!(options, ["-r"]);
        assert_eq!(apks, [PathBuf::from("/a.apk"), PathBuf::from("/b.apk")]);
        let (options, apks) = split_install_args("install", &["/a.apk"]).expect("valid install");
        assert!(options.is_empty());
        assert_eq!(apks.len(), 1);
        assert!(split_install_args("install", &["-r", "/a.apk", "/b.apk"]).is_err());
        assert!(split_install_args("install-multiple", &["-r"]).is_err());
        assert!(split_install_args("install", &["-r;reboot", "/a.apk"]).is_err());
    }

    #[test]
    fn a_package_manager_refusal_is_an_answer_not_a_transport_error() {
        assert_eq!(
            install_outcome(Ok("Success\n".to_string())).unwrap(),
            Ok("Success\n".to_string())
        );
        assert_eq!(
            install_outcome(Err(RustADBError::InstallFailed(
                "Failure [INSTALL_FAILED_VERSION_DOWNGRADE]".to_string()
            )))
            .unwrap(),
            Err("Failure [INSTALL_FAILED_VERSION_DOWNGRADE]".to_string())
        );
        assert_eq!(
            install_outcome(Err(RustADBError::SyncFailed(
                "push: No space left on device".to_string()
            )))
            .unwrap(),
            Err("the device refused the file operation: push: No space left on device".to_string())
        );
        assert!(install_outcome(Err(RustADBError::ProtocolError("bad".to_string()))).is_err());
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
    fn connect_timeout_explains_the_tv_approval_prompt() {
        let message = connect_error_message(&RustADBError::IOError(Error::new(
            ErrorKind::TimedOut,
            "raw OS timeout",
        )));
        assert!(message.contains("Allow"));
        assert!(message.contains("30 seconds"));
        assert!(!message.contains("raw OS timeout"));
    }

    #[test]
    fn an_unpaired_key_explains_how_to_pair() {
        let message = connect_error_message(&RustADBError::TlsAuthRejected(
            "TLS alert CertificateUnknown".to_string(),
        ));
        assert!(message.contains("hasn't paired"));
        assert!(message.contains("Pair device with pairing code"));
        assert!(!message.contains("CertificateUnknown"));
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
            "raw",
        )));
        assert!(timeout.contains("30 seconds"));
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
    fn refused_connect_explains_network_debugging() {
        let message = connect_error_message(&RustADBError::IOError(Error::new(
            ErrorKind::ConnectionRefused,
            "raw OS refusal",
        )));
        assert!(message.contains("Network debugging"));
        assert!(!message.contains("raw OS refusal"));
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

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use adb_client::tcp::{ADBTcpDevice, ADBTcpService};
use adb_client::{ADBDeviceExt, RebootType, RustADBError};
use async_trait::async_trait;
use serde::Serialize;
use shield_optimizer_core::adb::{AdbByteStream, AdbDriver, AdbError, AdbOutput, AdbResult};
use shield_optimizer_core::commands::devices::normalize_connect_address;

use tauri_plugin_atv_adb::AdbExt;
// Re-exported so wireless_commands.rs and the mobile handler keep one import path.
pub use tauri_plugin_atv_adb::DiscoveredAdbDevice;

const AUTHORIZATION_TIMEOUT: Duration = Duration::from_secs(30);

/// A live wireless-ADB connection: the owned `adb_client` device plus the
/// identity used to synthesize `adb devices`. One connection at a time.
struct Connection {
    device: ADBTcpDevice,
    serial: String,
    host: String,
    port: u16,
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
/// `push`/`pull` through the sync service via `raw_transfer`. `pair` is not yet
/// supported (Android-11 SPAKE2 is a documented follow-up); `forward` is
/// structurally impossible on a phone and reports `Unsupported`.
pub struct WirelessAdb {
    app: tauri::AppHandle,
    /// PKCS#8 PEM ADB private key. Persisted so re-auth is silent across launches.
    key_path: PathBuf,
    /// `adb_client` is blocking and `ADBTcpDevice` is not shareable across
    /// threads, so the owned device lives behind a std mutex and every call
    /// runs on `spawn_blocking`. `None` == disconnected.
    conn: Arc<Mutex<Option<Connection>>>,
    /// Serializes connect/disconnect transitions so a slow connect cannot
    /// resurrect a connection after the user has explicitly disconnected.
    lifecycle: tokio::sync::Mutex<()>,
}

impl WirelessAdb {
    pub fn new(app: tauri::AppHandle, key_path: PathBuf) -> Self {
        Self {
            app,
            key_path,
            conn: Arc::new(Mutex::new(None)),
            lifecycle: tokio::sync::Mutex::new(()),
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
        let requested_serial = serial.to_string();
        tokio::task::spawn_blocking(move || {
            let mut guard = conn
                .lock()
                .map_err(|_| "wireless connection lock poisoned".to_string())?;
            let Some(active) = guard.as_mut() else {
                return Err("Not connected to a device.".to_string());
            };
            validate_requested_serial(&active.serial, &requested_serial)?;
            match f(&mut active.device) {
                Ok(value) => Ok(value),
                Err(e) => {
                    // A transport error means the socket is dead; forget it so a
                    // follow-up `list_devices` shows nothing and status reports
                    // disconnected.
                    *guard = None;
                    tracing::warn!(error = %e, "wireless transport error — cleared connection");
                    Err(e.to_string())
                }
            }
        })
        .await
        .map_err(|e| format!("wireless-adb task failed: {e}"))?
    }

    /// Snapshot of the connected device's identity, taken under a short lock.
    fn info(&self) -> Option<(String, String, u16)> {
        let guard = self.conn.lock().ok()?;
        guard
            .as_ref()
            .map(|c| (c.serial.clone(), c.host.clone(), c.port))
    }

    pub async fn discover(&self) -> Result<Vec<DiscoveredAdbDevice>, String> {
        self.on_blocking(|app| app.adb().discover(3_000))
            .await?
            .map_err(|e| e.to_string())
    }

    /// Android-11 wireless-debugging pairing (SPAKE2) is not implemented by
    /// `adb_client` and is a documented follow-up. Legacy TVs (Nvidia Shield,
    /// older Google TV) use Network debugging on `:5555` and need no pairing —
    /// they connect directly.
    pub async fn pair(&self, _host: &str, _port: u16, _code: &str) -> Result<String, String> {
        Err(
            "Pairing new Google TV devices isn't supported yet — use Network debugging \
             (no code needed) on the TV and connect by IP for now."
                .to_string(),
        )
    }

    pub async fn connect(&self, host: &str, port: u16) -> Result<String, String> {
        let _lifecycle = self.lifecycle.lock().await;
        normalize_connect_address(&format!("{host}:{port}"))?;
        let addr: SocketAddr = format!("{host}:{port}")
            .parse()
            .map_err(|e| format!("invalid device address {host}:{port}: {e}"))?;
        let key_path = self.key_path.clone();
        let conn = self.conn.clone();
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
            let mut guard = conn
                .lock()
                .map_err(|_| "wireless connection lock poisoned".to_string())?;
            *guard = Some(Connection {
                device,
                serial: serial.clone(),
                host: host_owned,
                port,
            });
            Ok(format!("Connected to {serial}."))
        })
        .await
        .map_err(|e| format!("wireless-adb task failed: {e}"))?
    }

    pub async fn disconnect(&self) -> Result<String, String> {
        self.disconnect_serial(None).await
    }

    async fn disconnect_serial(&self, requested_serial: Option<&str>) -> Result<String, String> {
        let _lifecycle = self.lifecycle.lock().await;
        // Dropping the device closes its socket; do it on a blocking thread.
        let conn = self.conn.clone();
        let requested_serial = requested_serial.map(str::to_string);
        tokio::task::spawn_blocking(move || -> Result<(), String> {
            let mut guard = conn
                .lock()
                .map_err(|_| "wireless connection lock poisoned".to_string())?;
            if let (Some(expected), Some(active)) = (requested_serial.as_deref(), guard.as_ref()) {
                validate_requested_serial(&active.serial, expected)?;
            }
            *guard = None;
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
                    Duration::from_secs(1),
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
        if !alive {
            let _ = self.disconnect_serial(Some(&serial)).await;
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
            _ => Err(AdbError::Unsupported {
                operation: "raw_transfer",
            }),
        }
    }

    async fn shell(&self, serial: &str, command: &str) -> AdbResult<AdbOutput> {
        let command = command.to_string();
        let (stdout, stderr, code) = self
            .on_device(serial, move |dev| {
                let mut out: Vec<u8> = Vec::new();
                let mut err: Vec<u8> = Vec::new();
                let code = dev.shell_command(&command, Some(&mut out), Some(&mut err))?;
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
            // adb_client reports the on-device exit status (`exec:`), so command
            // failures surface here instead of always reading as success.
            exit_code: code.map(|c| c as i32),
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
            // Purpose-built framebuffer capture (returns PNG bytes). Falls back
            // to `screencap -p` over the shell if the framebuffer path errors.
            match dev.framebuffer_bytes() {
                Ok(bytes) => Ok(bytes),
                Err(_) => {
                    let mut out: Vec<u8> = Vec::new();
                    dev.shell_command(&"screencap -p", Some(&mut out), None)?;
                    Ok(out)
                }
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
            ADBTcpService::new_with_timeouts(
                addr,
                &service,
                key_path,
                Duration::from_secs(2),
                Duration::from_secs(2),
            )
            .map(|stream| Box::new(stream) as Box<dyn AdbByteStream>)
            .map_err(|e| AdbError::Transport(e.to_string()))
        })
        .await
        .map_err(|e| AdbError::Transport(format!("wireless-adb task failed: {e}")))?
    }
}

pub type SharedWirelessAdb = Arc<WirelessAdb>;

fn connect_error_message(error: &RustADBError) -> String {
    match error {
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
    use super::{connect_error_message, validate_requested_serial};
    use adb_client::RustADBError;
    use std::io::{Error, ErrorKind};

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
    fn refused_connect_explains_network_debugging() {
        let message = connect_error_message(&RustADBError::IOError(Error::new(
            ErrorKind::ConnectionRefused,
            "raw OS refusal",
        )));
        assert!(message.contains("Network debugging"));
        assert!(!message.contains("raw OS refusal"));
    }
}

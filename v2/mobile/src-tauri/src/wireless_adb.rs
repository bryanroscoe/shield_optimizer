use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use base64::Engine as _;
use serde::Serialize;
use shield_optimizer_core::adb::{AdbDriver, AdbError, AdbOutput, AdbResult};
use shield_optimizer_core::commands::devices::{normalize_connect_address, validate_pairing_pin};
use tokio::sync::RwLock;

use tauri_plugin_atv_adb::{AdbExt, ConnectResponse};
// Re-exported so wireless_commands.rs and the mobile handler keep one import path.
pub use tauri_plugin_atv_adb::DiscoveredAdbDevice;

/// `AdbDriver` over the libadb-android transport (the `tauri-plugin-atv-adb`
/// Kotlin plugin). The connection lifecycle (`pair`/`connect`/`disconnect`) has
/// no adb server on a phone, so those are explicit methods the mobile-only
/// `wireless_*` commands call; the trait surface synthesizes `raw(["devices"])`
/// and routes `shell`/screencap through the bridge. `spawn`/forward/push/pull
/// are structurally impossible on a phone and report `Unsupported`.
pub struct WirelessAdb {
    app: tauri::AppHandle,
    connected: RwLock<Option<ConnectResponse>>,
}

impl WirelessAdb {
    pub fn new(app: tauri::AppHandle) -> Self {
        Self {
            app,
            connected: RwLock::new(None),
        }
    }

    /// Run a blocking plugin call off the async runtime. Each `Adb` method
    /// blocks on a JNI hop, so calling one directly on a Tokio worker would
    /// stall the runtime — core commands fan several out with `join!`.
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

    pub async fn discover(&self) -> Result<Vec<DiscoveredAdbDevice>, String> {
        self.on_blocking(|app| app.adb().discover(3_000))
            .await?
            .map_err(|e| e.to_string())
    }

    pub async fn pair(&self, host: &str, port: u16, code: &str) -> Result<String, String> {
        validate_pairing_pin(code)?;
        normalize_connect_address(&format!("{host}:{port}"))?;
        let host = host.to_string();
        let code = code.to_string();
        self.on_blocking(move |app| app.adb().pair(&host, port, &code))
            .await?
            .map(|r| r.message)
            .map_err(|e| e.to_string())
    }

    pub async fn connect(&self, host: &str, port: u16) -> Result<String, String> {
        normalize_connect_address(&format!("{host}:{port}"))?;
        let host_owned = host.to_string();
        let response = self
            .on_blocking(move |app| app.adb().connect(&host_owned, port))
            .await?
            .map_err(|e| e.to_string())?;
        let message = response.message.clone();
        *self.connected.write().await = Some(response);
        Ok(message)
    }

    pub async fn disconnect(&self) -> Result<String, String> {
        let serial = self
            .connected
            .read()
            .await
            .as_ref()
            .map(|d| d.serial.clone());
        if let Some(serial) = serial {
            self.on_blocking(move |app| app.adb().disconnect(&serial))
                .await?
                .map_err(|e| e.to_string())?;
        }
        *self.connected.write().await = None;
        Ok("Disconnected.".to_string())
    }

    /// Forget the cached connection after a transport failure. A dead socket
    /// must stop reporting as a live device, so any `shell`/`raw_bytes`
    /// transport error clears it and a follow-up `list_devices` shows nothing.
    async fn mark_transport_dead(&self) {
        if self.connected.write().await.take().is_some() {
            tracing::warn!("wireless transport error — cleared cached connection (socket dead)");
        }
    }

    /// Cheap liveness probe for the frontend's connection watchdog. Returns the
    /// cached device only if a fast `echo` round-trips; otherwise clears the
    /// cached connection and reports disconnected.
    pub async fn status(&self) -> WirelessStatus {
        let info = self.connected.read().await.clone();
        let Some(device) = info else {
            return WirelessStatus::disconnected();
        };
        let serial = device.serial.clone();
        // `shell` already clears `connected` on a transport error; the timeout
        // arm handles a hung-but-not-errored socket.
        let alive = match tokio::time::timeout(
            Duration::from_secs(3),
            self.shell(&serial, "echo ok"),
        )
        .await
        {
            Ok(Ok(out)) => out.stdout.contains("ok"),
            Ok(Err(e)) => {
                tracing::warn!(serial = %serial, error = %e, "wireless_status probe failed");
                false
            }
            Err(_) => {
                tracing::warn!(serial = %serial, "wireless_status probe timed out");
                false
            }
        };
        if !alive {
            *self.connected.write().await = None;
            return WirelessStatus::disconnected();
        }
        WirelessStatus {
            connected: true,
            serial: Some(device.serial),
            host: Some(device.host),
        }
    }

    async fn devices_output(&self) -> AdbOutput {
        let stdout = if let Some(device) = self.connected.read().await.as_ref() {
            format!("List of devices attached\n{}\tdevice\n", device.serial)
        } else {
            "List of devices attached\n".to_string()
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
            ["devices"] => Ok(self.devices_output().await),
            ["disconnect", _serial] => {
                self.disconnect().await.map_err(AdbError::Transport)?;
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
            // No adb server on the phone to run `reboot` as a host command, so
            // route it through the device shell — `reboot [recovery|bootloader]`
            // is the same on-device command. The reboot tears down the socket
            // before the stream closes cleanly, so a transport error here IS the
            // success signal — report ok and forget the now-dead connection.
            ["-s", serial, "reboot", rest @ ..] => {
                let command = format!("reboot {}", rest.join(" "));
                let result = self.shell(serial, command.trim()).await;
                *self.connected.write().await = None;
                match result {
                    Ok(out) => Ok(out),
                    Err(_) => Ok(AdbOutput {
                        stdout: "Reboot command sent.".to_string(),
                        stderr: String::new(),
                        exit_code: Some(0),
                    }),
                }
            }
            _ => Err(AdbError::Unsupported { operation: "raw" }),
        }
    }

    async fn raw_transfer(&self, _args: &[&str]) -> AdbResult<AdbOutput> {
        Err(AdbError::Unsupported {
            operation: "raw_transfer",
        })
    }

    async fn shell(&self, serial: &str, command: &str) -> AdbResult<AdbOutput> {
        let serial = serial.to_string();
        let command = command.to_string();
        let out = match self
            .on_blocking(move |app| app.adb().shell(&serial, &command))
            .await
        {
            Ok(Ok(out)) => out,
            Ok(Err(e)) => {
                self.mark_transport_dead().await;
                return Err(AdbError::Transport(e.to_string()));
            }
            Err(e) => {
                self.mark_transport_dead().await;
                return Err(AdbError::Transport(e));
            }
        };
        Ok(AdbOutput {
            stdout: out.stdout,
            stderr: out.stderr,
            exit_code: Some(out.exit_code),
        })
    }

    async fn raw_bytes(&self, args: &[&str]) -> AdbResult<Vec<u8>> {
        if !matches!(args, ["-s", _, "exec-out", "screencap", "-p"]) {
            return Err(AdbError::Unsupported {
                operation: "raw_bytes",
            });
        }
        let serial = args[1].to_string();
        let png_base64 = match self
            .on_blocking(move |app| app.adb().screencap(&serial))
            .await
        {
            Ok(Ok(png)) => png,
            Ok(Err(e)) => {
                self.mark_transport_dead().await;
                return Err(AdbError::Transport(e.to_string()));
            }
            Err(e) => {
                self.mark_transport_dead().await;
                return Err(AdbError::Transport(e));
            }
        };
        base64::engine::general_purpose::STANDARD
            .decode(png_base64)
            .map_err(|e| AdbError::Transport(format!("invalid screencap base64: {e}")))
    }
}

pub type SharedWirelessAdb = Arc<WirelessAdb>;

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

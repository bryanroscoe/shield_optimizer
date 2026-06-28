use std::sync::Arc;

use async_trait::async_trait;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use shield_optimizer_core::adb::{AdbDriver, AdbError, AdbOutput, AdbResult};
use tokio::sync::RwLock;

use tauri_plugin_atv_adb::AdbExt;
// Re-exported so wireless_commands.rs and the mobile handler keep one import path.
pub use tauri_plugin_atv_adb::DiscoveredAdbDevice;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WirelessDevice {
    pub serial: String,
    pub host: String,
    pub port: u16,
}

/// `AdbDriver` over the libadb-android transport (the `tauri-plugin-atv-adb`
/// Kotlin plugin). The connection lifecycle (`pair`/`connect`/`disconnect`) has
/// no adb server on a phone, so those are explicit methods the mobile-only
/// `wireless_*` commands call; the trait surface synthesizes `raw(["devices"])`
/// and routes `shell`/screencap through the bridge. `spawn`/forward/push/pull
/// are structurally impossible on a phone and report `Unsupported`.
pub struct WirelessAdb {
    app: tauri::AppHandle,
    connected: RwLock<Option<WirelessDevice>>,
}

impl WirelessAdb {
    pub fn new(app: tauri::AppHandle) -> Self {
        Self {
            app,
            connected: RwLock::new(None),
        }
    }

    pub async fn discover(&self) -> Result<Vec<DiscoveredAdbDevice>, String> {
        self.app.adb().discover(3_000).map_err(|e| e.to_string())
    }

    pub async fn pair(&self, host: &str, port: u16, code: &str) -> Result<String, String> {
        if code.len() != 6 || !code.chars().all(|c| c.is_ascii_digit()) {
            return Err("PIN must be exactly 6 digits.".to_string());
        }
        self.app
            .adb()
            .pair(host, port, code)
            .map(|r| r.message)
            .map_err(|e| e.to_string())
    }

    pub async fn connect(&self, host: &str, port: u16) -> Result<String, String> {
        let response = self
            .app
            .adb()
            .connect(host, port)
            .map_err(|e| e.to_string())?;
        *self.connected.write().await = Some(WirelessDevice {
            serial: response.serial.clone(),
            host: response.host.clone(),
            port: response.port,
        });
        Ok(response.message)
    }

    pub async fn disconnect(&self) -> Result<String, String> {
        let serial = self
            .connected
            .read()
            .await
            .as_ref()
            .map(|d| d.serial.clone());
        if let Some(serial) = serial.as_deref() {
            self.app
                .adb()
                .disconnect(serial)
                .map_err(|e| e.to_string())?;
        }
        *self.connected.write().await = None;
        Ok("Disconnected.".to_string())
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
            _ => Err(AdbError::Unsupported { operation: "raw" }),
        }
    }

    async fn raw_transfer(&self, _args: &[&str]) -> AdbResult<AdbOutput> {
        Err(AdbError::Unsupported {
            operation: "raw_transfer",
        })
    }

    async fn shell(&self, serial: &str, command: &str) -> AdbResult<AdbOutput> {
        let out = self
            .app
            .adb()
            .shell(serial, command)
            .map_err(|e| AdbError::Transport(e.to_string()))?;
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
        let serial = args[1];
        let png_base64 = self
            .app
            .adb()
            .screencap(serial)
            .map_err(|e| AdbError::Transport(e.to_string()))?;
        base64::engine::general_purpose::STANDARD
            .decode(png_base64)
            .map_err(|e| AdbError::Transport(format!("invalid screencap base64: {e}")))
    }
}

pub type SharedWirelessAdb = Arc<WirelessAdb>;

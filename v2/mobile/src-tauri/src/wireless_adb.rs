use std::sync::Arc;

use async_trait::async_trait;
#[cfg(target_os = "android")]
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use shield_optimizer_core::adb::{AdbDriver, AdbError, AdbOutput, AdbResult};
#[cfg(target_os = "android")]
use tauri::Manager;
use tokio::sync::RwLock;

#[cfg(target_os = "android")]
use crate::adb_plugin::AdbPlugin;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WirelessDevice {
    pub serial: String,
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredAdbDevice {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub service: String,
}

#[cfg(target_os = "android")]
#[derive(Debug, Deserialize)]
struct DiscoveryResponse {
    devices: Vec<DiscoveredAdbDevice>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg(target_os = "android")]
struct PairPayload<'a> {
    host: &'a str,
    port: u16,
    code: &'a str,
}

#[derive(Debug, Serialize)]
#[cfg(target_os = "android")]
struct ConnectPayload<'a> {
    host: &'a str,
    port: u16,
}

#[derive(Debug, Serialize)]
#[cfg(target_os = "android")]
struct SerialPayload<'a> {
    serial: &'a str,
}

#[derive(Debug, Serialize)]
#[cfg(target_os = "android")]
struct ShellPayload<'a> {
    serial: &'a str,
    command: &'a str,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg(target_os = "android")]
struct ConnectResponse {
    serial: String,
    host: String,
    port: u16,
    message: Option<String>,
}

#[derive(Debug, Deserialize)]
#[cfg(target_os = "android")]
struct ScreenshotResponse {
    base64: String,
}

pub struct WirelessAdb {
    #[cfg(target_os = "android")]
    app: tauri::AppHandle,
    connected: RwLock<Option<WirelessDevice>>,
}

impl WirelessAdb {
    pub fn new(app: tauri::AppHandle) -> Self {
        #[cfg(not(target_os = "android"))]
        let _ = app;
        Self {
            #[cfg(target_os = "android")]
            app,
            connected: RwLock::new(None),
        }
    }

    pub async fn discover(&self) -> Result<Vec<DiscoveredAdbDevice>, String> {
        #[cfg(target_os = "android")]
        {
            let response: DiscoveryResponse = self
                .bridge()
                .run("discover", ())
                .await
                .map_err(|e| e.to_string())?;
            return Ok(response.devices);
        }
        #[cfg(not(target_os = "android"))]
        {
            Ok(Vec::new())
        }
    }

    pub async fn pair(&self, host: &str, port: u16, code: &str) -> Result<String, String> {
        if code.len() != 6 || !code.chars().all(|c| c.is_ascii_digit()) {
            return Err("PIN must be exactly 6 digits.".to_string());
        }
        #[cfg(target_os = "android")]
        {
            let response: ConnectResponse = self
                .bridge()
                .run("pair", PairPayload { host, port, code })
                .await
                .map_err(|e| e.to_string())?;
            return Ok(response.message.unwrap_or_else(|| "Paired.".to_string()));
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = (host, port);
            Err("Wireless ADB pairing is only available on Android builds.".to_string())
        }
    }

    pub async fn connect(&self, host: &str, port: u16) -> Result<String, String> {
        #[cfg(target_os = "android")]
        {
            let response: ConnectResponse = self
                .bridge()
                .run("connect", ConnectPayload { host, port })
                .await
                .map_err(|e| e.to_string())?;
            self.set_connected(&response).await;
            return Ok(response
                .message
                .unwrap_or_else(|| format!("Connected to {}:{}.", response.host, response.port)));
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = (host, port);
            Err("Wireless ADB connect is only available on Android builds.".to_string())
        }
    }

    pub async fn disconnect(&self) -> Result<String, String> {
        let serial = self
            .connected
            .read()
            .await
            .as_ref()
            .map(|d| d.serial.clone());
        #[cfg(target_os = "android")]
        if let Some(serial) = serial.as_deref() {
            self.bridge()
                .run::<serde_json::Value>("disconnect", SerialPayload { serial })
                .await
                .map_err(|e| e.to_string())?;
        }
        #[cfg(not(target_os = "android"))]
        let _ = serial;
        *self.connected.write().await = None;
        Ok("Disconnected.".to_string())
    }

    #[cfg(target_os = "android")]
    fn bridge(&self) -> tauri::State<'_, AdbPlugin<tauri::Wry>> {
        self.app.state::<AdbPlugin<tauri::Wry>>()
    }

    #[cfg(target_os = "android")]
    async fn set_connected(&self, response: &ConnectResponse) {
        *self.connected.write().await = Some(WirelessDevice {
            serial: response.serial.clone(),
            host: response.host.clone(),
            port: response.port,
        });
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
            [_op, ..] => Err(AdbError::Unsupported { operation: "raw" }),
            [] => Err(AdbError::Unsupported { operation: "raw" }),
        }
    }

    async fn raw_transfer(&self, args: &[&str]) -> AdbResult<AdbOutput> {
        let _ = args;
        Err(AdbError::Unsupported {
            operation: "raw_transfer",
        })
    }

    async fn shell(&self, serial: &str, command: &str) -> AdbResult<AdbOutput> {
        #[cfg(target_os = "android")]
        {
            self.bridge()
                .run("shell", ShellPayload { serial, command })
                .await
                .map_err(|e| AdbError::Transport(e.to_string()))
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = (serial, command);
            Err(AdbError::Unsupported { operation: "shell" })
        }
    }

    async fn raw_bytes(&self, args: &[&str]) -> AdbResult<Vec<u8>> {
        #[cfg(target_os = "android")]
        {
            if !matches!(args, ["-s", _, "exec-out", "screencap", "-p"]) {
                return Err(AdbError::Unsupported {
                    operation: "raw_bytes",
                });
            }
            let serial = args[1];
            let response: ScreenshotResponse = self
                .bridge()
                .run("screencap", SerialPayload { serial })
                .await
                .map_err(|e| AdbError::Transport(e.to_string()))?;
            base64::engine::general_purpose::STANDARD
                .decode(response.base64)
                .map_err(|e| AdbError::Transport(format!("invalid screencap base64: {e}")))
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = args;
            Err(AdbError::Unsupported {
                operation: "raw_bytes",
            })
        }
    }
}

pub type SharedWirelessAdb = Arc<WirelessAdb>;

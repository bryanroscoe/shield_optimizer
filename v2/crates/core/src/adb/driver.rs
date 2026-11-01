//! `AdbDriver` trait + subprocess implementation.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::lifetimes::TrackedChild;

/// Blocking byte stream returned by a device service such as a localabstract
/// socket. Direct transports own dedicated connections; the desktop subprocess
/// backend can continue to use its existing forwarded sockets.
pub trait AdbByteStream: std::io::Read + std::io::Write + Send + 'static {}

impl<T> AdbByteStream for T where T: std::io::Read + std::io::Write + Send + 'static {}

/// Hardware identity captured on the currently selected owned control connection.
/// Keep one binding for all dedicated streams in a logical session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdbDeviceBinding {
    pub serial: String,
    pub hardware_id: String,
}

/// Interrupts only the owned service connection, not a host server or another session.
pub trait AdbServiceCancellation: Send + Sync {
    fn cancel(&self);
}

/// The stream keeps its worker lease until actual socket cleanup finishes.
/// Cancellation does not prove that the remote process has exited.
pub struct AdbOwnedService {
    pub stream: Box<dyn AdbByteStream>,
    pub cancellation: std::sync::Arc<dyn AdbServiceCancellation>,
}

impl std::fmt::Debug for AdbOwnedService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AdbOwnedService").finish_non_exhaustive()
    }
}

/// Errors a driver can return.
#[derive(Debug, Error)]
pub enum AdbError {
    #[error(
        "could not locate an adb binary. Tried PATH, ANDROID_HOME / ANDROID_SDK_ROOT, and \
        common install locations. Install platform-tools (e.g. `brew install android-platform-tools` \
        on macOS, your distro's `adb` package on Linux, or download from \
        https://developer.android.com/studio/releases/platform-tools), then relaunch. \
        You can also set SHIELD_OPTIMIZER_ADB to point at a specific binary."
    )]
    BinaryNotFound,
    #[error("adb binary at {path} is not a regular file")]
    BinaryMissing { path: String },
    #[error("adb command timed out after {seconds}s")]
    Timeout { seconds: u64 },
    #[error("adb process failed (exit code {code:?}): {stderr}")]
    NonZeroExit { code: Option<i32>, stderr: String },
    #[error("device {serial} is in state '{state}', not 'device'")]
    DeviceNotReady { serial: String, state: String },
    #[error("transport error: {0}")]
    Transport(String),
    #[error("{0}")]
    Initialization(String),
    #[error("ADB operation unsupported by this driver: {operation}")]
    Unsupported { operation: &'static str },
    #[error("{operation} was explicitly rejected by the device")]
    ServiceRejected { operation: &'static str },
    #[error("{operation} outcome is unknown: {detail}. Do not replay automatically.")]
    Uncertain {
        operation: &'static str,
        detail: String,
    },
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("the app is quitting, so no new adb work is started")]
    ShuttingDown,
}

pub type AdbResult<T> = Result<T, AdbError>;

/// Output of an ADB invocation. Both streams are captured; the caller decides
/// what to do with each.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdbOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShellTermination {
    Completed,
    OutputLimit,
    Timeout,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoundedShellOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub termination: ShellTermination,
}

impl AdbOutput {
    /// Returns `true` if exit code was 0.
    pub fn success(&self) -> bool {
        self.exit_code == Some(0)
    }

    /// stdout and stderr joined. `adb shell` exits 0 even when the on-device
    /// command fails, and tools like `pm` / `settings` write their failure text
    /// to *either* stream depending on the Android build — so any success
    /// heuristic must look at both, not just stdout.
    pub fn combined(&self) -> String {
        format!("{}\n{}", self.stdout, self.stderr)
    }

    /// Heuristic for "did the on-device command report a failure?" Scans both
    /// streams for the markers `pm` / `cmd` / `settings` emit on error. Use
    /// this instead of checking `stdout` alone or trusting the exit code.
    pub fn shell_reported_failure(&self) -> bool {
        let combined = self.combined();
        combined.contains("Failure") || combined.contains("Error") || combined.contains("Exception")
    }
}

/// How an adb process's exit status becomes a driver result.
///
/// `adb shell` exits with the device-side status of the *last* command it ran,
/// and any nonzero exit is an error before stdout is looked at. Shared by the
/// desktop subprocess driver and the test-only simulator so a simulated device
/// fails exactly where a real one would.
pub fn process_output(
    stdout: String,
    stderr: String,
    exit_code: Option<i32>,
) -> AdbResult<AdbOutput> {
    if exit_code != Some(0) {
        return Err(AdbError::NonZeroExit {
            code: exit_code,
            stderr: if stderr.is_empty() { stdout } else { stderr },
        });
    }
    Ok(AdbOutput {
        stdout,
        stderr,
        exit_code,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeServiceKind {
    LegacyAdb,
    TlsPairing,
    TlsConnect,
}

/// A service advertisement, never evidence of physical device identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeAdvertisement {
    pub service_kind: NativeServiceKind,
    pub instance: String,
    pub address: String,
    pub port: u16,
    pub interface_name: Option<String>,
    /// Observed interface metadata; this does not force an IPv4 route.
    pub interface_index: Option<u32>,
    pub scope_id: Option<u32>,
    pub connect_address: Option<String>,
    pub observed_age_ms: u64,
    /// Not resolved again in the current browse window; selection must be disabled.
    pub cached: bool,
    pub warning: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeDiscoveryReport {
    pub advertisements: Vec<NativeAdvertisement>,
    pub warnings: Vec<String>,
    /// Local observation limit; this is not the remaining DNS TTL.
    pub observation_max_age_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ScanConnectOutcome {
    Ready,
    Pending,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ScanConnectionFailure {
    pub address: String,
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ScanConnectionProgress {
    pub pending: Vec<String>,
    pub failed: Vec<ScanConnectionFailure>,
}

/// The driver abstraction. Lets tests inject a mock; production uses
/// `SubprocessAdb`.
#[async_trait]
pub trait AdbDriver: Send + Sync {
    /// Direct transports retain this counter inside blocking workers and streams.
    fn operation_leases(&self) -> Option<std::sync::Arc<super::Leases>> {
        None
    }

    /// Prevent host-server commands and installation from changing a direct backend.
    fn uses_host_server(&self) -> bool {
        true
    }

    /// Explicit observation-only native discovery; never connects or authorizes a device.
    async fn discover_network(&self) -> AdbResult<NativeDiscoveryReport> {
        Err(AdbError::Unsupported {
            operation: "native_discovery",
        })
    }

    /// Starts or joins an owned scan attempt. `valid_for` bounds queue time,
    /// not the time allowed for the user to approve the TV's prompt.
    async fn scan_connect(
        &self,
        _address: &str,
        _valid_for: std::time::Duration,
    ) -> AdbResult<ScanConnectOutcome> {
        Err(AdbError::Unsupported {
            operation: "scan_connect",
        })
    }

    async fn scan_connection_progress(&self) -> ScanConnectionProgress {
        ScanConnectionProgress::default()
    }

    /// Run `adb <args...>` (no `-s` prefix).
    async fn raw(&self, args: &[&str]) -> AdbResult<AdbOutput>;

    /// Run `adb <args...>` with a transfer-sized timeout — for `pull` / `push`
    /// / `install`, which stream whole files and legitimately take minutes on
    /// big payloads (the standard timeout would kill them mid-transfer).
    /// Default delegates to `raw` so mocks need no extra wiring.
    async fn raw_transfer(&self, args: &[&str]) -> AdbResult<AdbOutput> {
        self.raw(args).await
    }

    /// Download with a limit enforced while streaming, not after allocation.
    async fn pull_limited(
        &self,
        _serial: &str,
        _remote: &str,
        _local: &std::path::Path,
        _max_bytes: u64,
    ) -> AdbResult<()> {
        Err(AdbError::Unsupported {
            operation: "pull_limited",
        })
    }

    /// Run `adb -s <serial> shell <command>`.
    async fn shell(&self, serial: &str, command: &str) -> AdbResult<AdbOutput>;

    /// Expert shell execution must bound output and runtime at the transport.
    async fn shell_bounded(&self, _serial: &str, _command: &str) -> AdbResult<BoundedShellOutput> {
        Err(AdbError::Unsupported {
            operation: "shell_bounded",
        })
    }

    /// Run `adb <args...>` and return raw stdout bytes — for binary output
    /// like `exec-out screencap -p`, where UTF-8 conversion would corrupt the
    /// data. Default reports unsupported so mocks without binary needs don't
    /// have to implement it.
    async fn raw_bytes(&self, _args: &[&str]) -> AdbResult<Vec<u8>> {
        Err(AdbError::Unsupported {
            operation: "raw_bytes",
        })
    }

    /// Captures verified hardware identity from the currently owned control connection.
    async fn device_binding(&self, _serial: &str) -> AdbResult<AdbDeviceBinding> {
        Err(AdbError::Unsupported {
            operation: "device_binding",
        })
    }

    /// Opens a dedicated identity-bound, independently cancellable service.
    /// The finite lifetime includes setup and prevents an abandoned stream living forever.
    async fn open_bound_device_service(
        &self,
        _binding: &AdbDeviceBinding,
        _service: &str,
        _lifetime: std::time::Duration,
    ) -> AdbResult<AdbOwnedService> {
        Err(AdbError::Unsupported {
            operation: "bound_device_service",
        })
    }

    /// Open an arbitrary device-side ADB service as a blocking byte stream.
    /// Default reports unsupported so desktop and test drivers need no extra
    /// implementation.
    async fn open_device_service(
        &self,
        _serial: &str,
        _service: &str,
    ) -> AdbResult<Box<dyn AdbByteStream>> {
        Err(AdbError::Unsupported {
            operation: "open_device_service",
        })
    }

    /// Spawn `adb <args...>` as a long-lived child WITHOUT awaiting completion,
    /// handing the caller the child to own (configured `kill_on_drop(true)`).
    /// Unlike `raw`/`shell`, the process is expected to keep running — for
    /// resident helpers like the scrcpy control server. It stays counted until
    /// reaped, so quit cannot stop the server under it. Default reports
    /// unsupported so mocks need no extra wiring.
    async fn spawn(&self, _args: &[&str]) -> AdbResult<TrackedChild> {
        Err(AdbError::Unsupported { operation: "spawn" })
    }

    /// Stop the adb server this driver's commands talk to, giving up after
    /// `timeout`. Only the opt-in quit stop calls this, once, after every
    /// operation and adb process has drained. Default: nothing to stop.
    async fn stop_server(&self, _timeout: std::time::Duration) -> ServerStop {
        ServerStop::Skipped("this driver has no adb server to stop".to_string())
    }
}

/// What the quit-time `adb kill-server` came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerStop {
    /// `kill-server` ran and exited zero.
    Stopped,
    /// `kill-server` could not be started or exited nonzero.
    Failed(String),
    /// `kill-server` did not finish in time. The driver tries to end it so it
    /// cannot stop a server later, but may not be able to confirm that, and
    /// whether the server had already stopped is unknown.
    Unknown,
    /// Not attempted, for the reason given.
    Skipped(String),
}

#[cfg(test)]
mod bounded_shell_tests {
    use super::*;

    struct UnboundedDriver;

    #[async_trait]
    impl AdbDriver for UnboundedDriver {
        async fn raw(&self, _: &[&str]) -> AdbResult<AdbOutput> {
            panic!("bounded shell must not delegate to raw")
        }

        async fn shell(&self, _: &str, _: &str) -> AdbResult<AdbOutput> {
            panic!("bounded shell must not delegate to shell")
        }
    }

    #[tokio::test]
    async fn bound_service_capabilities_default_to_unsupported_without_raw_calls() {
        assert!(matches!(
            UnboundedDriver.device_binding("fixture").await,
            Err(AdbError::Unsupported {
                operation: "device_binding"
            })
        ));
        let binding = AdbDeviceBinding {
            serial: "fixture".into(),
            hardware_id: "known".into(),
        };
        assert!(matches!(
            UnboundedDriver
                .open_bound_device_service(
                    &binding,
                    "localabstract:fixture",
                    std::time::Duration::from_secs(1)
                )
                .await,
            Err(AdbError::Unsupported {
                operation: "bound_device_service"
            })
        ));
    }

    #[tokio::test]
    async fn native_discovery_defaults_to_unsupported_without_raw_calls() {
        assert!(matches!(
            UnboundedDriver.discover_network().await,
            Err(AdbError::Unsupported {
                operation: "native_discovery"
            })
        ));
    }

    #[tokio::test]
    async fn drivers_without_bounded_execution_are_unsupported() {
        assert!(matches!(
            UnboundedDriver.shell_bounded("serial", "id").await,
            Err(AdbError::Unsupported {
                operation: "shell_bounded"
            })
        ));
    }
}

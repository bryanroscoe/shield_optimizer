//! `AdbDriver` trait + subprocess implementation.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Blocking byte stream returned by a device service such as a localabstract
/// socket. Mobile implements this with a dedicated pure-Rust ADB connection;
/// desktop does not need it because its adb process owns port forwarding.
pub trait AdbByteStream: std::io::Read + std::io::Write + Send + 'static {}

impl<T> AdbByteStream for T where T: std::io::Read + std::io::Write + Send + 'static {}

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
    #[error("ADB operation unsupported by this driver: {operation}")]
    Unsupported { operation: &'static str },
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
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

/// The driver abstraction. Lets tests inject a mock; production uses
/// `SubprocessAdb`.
#[async_trait]
pub trait AdbDriver: Send + Sync {
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

    /// Run `adb <args...>` and return raw stdout bytes — for binary output
    /// like `exec-out screencap -p`, where UTF-8 conversion would corrupt the
    /// data. Default reports unsupported so mocks without binary needs don't
    /// have to implement it.
    async fn raw_bytes(&self, _args: &[&str]) -> AdbResult<Vec<u8>> {
        Err(AdbError::Unsupported {
            operation: "raw_bytes",
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
    /// handing the caller the `Child` to own (configured `kill_on_drop(true)`).
    /// Unlike `raw`/`shell`, the process is expected to keep running — for
    /// resident helpers like the scrcpy control server. Default reports
    /// unsupported so mocks need no extra wiring.
    async fn spawn(&self, _args: &[&str]) -> AdbResult<tokio::process::Child> {
        Err(AdbError::Unsupported { operation: "spawn" })
    }
}

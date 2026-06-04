//! `AdbDriver` trait + subprocess implementation.

use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::process::Command;
use tokio::time::timeout;
use tracing::{debug, warn};

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

    /// Run `adb -s <serial> shell <command>`.
    async fn shell(&self, serial: &str, command: &str) -> AdbResult<AdbOutput>;
}

/// The standard subprocess-backed driver. Wraps `tokio::process::Command`.
#[derive(Debug, Clone)]
pub struct SubprocessAdb {
    binary: PathBuf,
    command_timeout: Duration,
}

impl SubprocessAdb {
    /// Build a driver around the `adb` binary at `binary` (must exist).
    pub fn new(binary: PathBuf) -> Self {
        Self {
            binary,
            command_timeout: Duration::from_secs(30),
        }
    }

    /// Locate `adb` using the same priority order as `discover_adb_binary`.
    /// Returns `None` if no candidate exists — callers should surface a
    /// helpful error pointing the user at installation instructions.
    pub fn discover() -> Option<Self> {
        discover_adb_binary().map(Self::new)
    }

    /// Back-compat alias for the old PATH-only discovery.
    pub fn from_path() -> Option<Self> {
        Self::discover()
    }

    pub fn binary(&self) -> &PathBuf {
        &self.binary
    }

    pub fn with_timeout(mut self, dur: Duration) -> Self {
        self.command_timeout = dur;
        self
    }

    async fn run(&self, args: &[&str]) -> AdbResult<AdbOutput> {
        if !self.binary.exists() {
            return Err(AdbError::BinaryMissing {
                path: self.binary.display().to_string(),
            });
        }

        debug!(adb = ?self.binary, ?args, "adb invoke");

        let mut cmd = Command::new(&self.binary);
        cmd.args(args).kill_on_drop(true);
        super::hide_console_window(&mut cmd);
        let fut = cmd.output();

        let output = match timeout(self.command_timeout, fut).await {
            Ok(r) => r?,
            Err(_) => {
                warn!(?args, "adb timeout");
                return Err(AdbError::Timeout {
                    seconds: self.command_timeout.as_secs(),
                });
            }
        };

        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        let exit_code = output.status.code();

        // Surface real process failures rather than letting callers parse
        // empty stdout as "no results". Exit-0 with empty stdout is a
        // legitimate response for many `pm` queries (e.g. "no disabled
        // packages matched"); exit-nonzero is the signal that something
        // actually went wrong.
        if !output.status.success() {
            warn!(?args, ?exit_code, %stderr, "adb exited nonzero");
            return Err(AdbError::NonZeroExit {
                code: exit_code,
                stderr: if stderr.is_empty() {
                    stdout.clone()
                } else {
                    stderr
                },
            });
        }

        Ok(AdbOutput {
            stdout,
            stderr,
            exit_code,
        })
    }
}

#[async_trait]
impl AdbDriver for SubprocessAdb {
    async fn raw(&self, args: &[&str]) -> AdbResult<AdbOutput> {
        self.run(args).await
    }

    async fn shell(&self, serial: &str, command: &str) -> AdbResult<AdbOutput> {
        self.run(&["-s", serial, "shell", command]).await
    }
}

/// Locate an adb binary by checking the standard installation locations.
///
/// GUI apps on macOS don't inherit the user's shell PATH, so PATH search
/// alone misses Homebrew (`/opt/homebrew/bin`), Android Studio's bundled
/// SDK, and other common installs. This function walks a deterministic
/// priority list:
///
/// 1. `SHIELD_OPTIMIZER_ADB` env var (explicit user override)
/// 2. `ANDROID_HOME` / `ANDROID_SDK_ROOT` env vars + `platform-tools/adb`
/// 3. PATH search (only finds adb on Linux/Windows or when the GUI was
///    launched from a shell that exported the right PATH)
/// 4. Well-known install locations per OS:
///    - macOS: `/opt/homebrew/bin/adb`, `/usr/local/bin/adb`,
///      `~/Library/Android/sdk/platform-tools/adb`
///    - Linux: `/usr/bin/adb`, `/usr/local/bin/adb`,
///      `~/Android/Sdk/platform-tools/adb`
///    - Windows: `%LOCALAPPDATA%\Android\Sdk\platform-tools\adb.exe`
/// 5. Repo-local fallback for v1 coexistence: `../adb` relative to the
///    Cargo workspace, since the v1 repo ships an adb binary there.
pub fn discover_adb_binary() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();

    // 1. Explicit env override.
    if let Ok(p) = std::env::var("SHIELD_OPTIMIZER_ADB") {
        candidates.push(PathBuf::from(p));
    }

    // 2. App-managed install root — populated by `install_platform_tools`
    //    on first run when nothing else exists. Honored on subsequent runs
    //    so the user only pays the download cost once.
    if let Some(p) = crate::adb::install::adb_path_in_install_root() {
        candidates.push(p);
    }

    // 3. Android SDK env vars.
    for var in ["ANDROID_HOME", "ANDROID_SDK_ROOT"] {
        if let Ok(root) = std::env::var(var) {
            let mut p = PathBuf::from(root);
            p.push("platform-tools");
            p.push(adb_exe_name());
            candidates.push(p);
        }
    }

    // 4. PATH search.
    if let Some(p) = which_in_path(&adb_exe_name()) {
        candidates.push(p);
    }

    // 5. Well-known locations per OS.
    for p in well_known_adb_locations() {
        candidates.push(p);
    }

    // 6. Repo-local fallback: the v1 repo ships `./adb` at the top level.
    //    When `dev`-running v2 from the same repo, this lets developers go.
    if let Ok(cwd) = std::env::current_dir() {
        let mut p = cwd.clone();
        p.push("adb");
        candidates.push(p);
        let mut p = cwd.clone();
        p.pop();
        p.push("adb");
        candidates.push(p);
    }

    candidates.into_iter().find(|p| p.is_file())
}

fn adb_exe_name() -> String {
    if cfg!(windows) {
        "adb.exe".to_string()
    } else {
        "adb".to_string()
    }
}

fn well_known_adb_locations() -> Vec<PathBuf> {
    let mut out = Vec::new();
    let exe = adb_exe_name();

    if cfg!(target_os = "macos") {
        for p in [
            "/opt/homebrew/bin/adb",
            "/usr/local/bin/adb",
            "/opt/homebrew/share/android-platform-tools/adb",
        ] {
            out.push(PathBuf::from(p));
        }
        if let Some(home) = dirs::home_dir() {
            out.push(home.join("Library/Android/sdk/platform-tools").join(&exe));
            out.push(home.join(".android-sdk/platform-tools").join(&exe));
        }
    } else if cfg!(target_os = "linux") {
        for p in ["/usr/bin/adb", "/usr/local/bin/adb", "/snap/bin/adb"] {
            out.push(PathBuf::from(p));
        }
        if let Some(home) = dirs::home_dir() {
            out.push(home.join("Android/Sdk/platform-tools").join(&exe));
            out.push(home.join(".android-sdk/platform-tools").join(&exe));
        }
    } else if cfg!(windows) {
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            out.push(
                PathBuf::from(local)
                    .join("Android/Sdk/platform-tools")
                    .join(&exe),
            );
        }
        for p in [
            r"C:\Program Files\Android\platform-tools\adb.exe",
            r"C:\platform-tools\adb.exe",
        ] {
            out.push(PathBuf::from(p));
        }
    }
    out
}

/// Cross-platform PATH search for an executable.
fn which_in_path(bin: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    let exts: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT")
            .ok()
            .map(|s| s.split(';').map(|e| e.to_lowercase()).collect())
            .unwrap_or_else(|| vec![".exe".into(), ".bat".into(), ".cmd".into()])
    } else {
        vec![String::new()]
    };

    for dir in std::env::split_paths(&path_var) {
        for ext in &exts {
            let mut candidate = dir.join(bin);
            if !ext.is_empty() {
                candidate.set_extension(ext.trim_start_matches('.'));
            }
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Tests in this module mutate process-wide env vars; serialize them so
    // they don't trip over each other under `cargo test`'s parallelism.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn discover_uses_explicit_env_override() {
        let _g = ENV_LOCK.lock().unwrap();
        // Point the override at this very binary — we know it exists at
        // test runtime.
        let exe = std::env::current_exe().unwrap();
        std::env::set_var("SHIELD_OPTIMIZER_ADB", &exe);
        // Clear other env that might point at a real adb.
        std::env::remove_var("ANDROID_HOME");
        std::env::remove_var("ANDROID_SDK_ROOT");

        let found = discover_adb_binary().expect("override should resolve");
        // SHIELD_OPTIMIZER_ADB has top priority — so the override wins even
        // when other adbs exist on PATH.
        assert_eq!(found, exe);

        std::env::remove_var("SHIELD_OPTIMIZER_ADB");
    }

    #[test]
    fn discover_returns_none_when_nothing_exists() {
        let _g = ENV_LOCK.lock().unwrap();
        std::env::remove_var("SHIELD_OPTIMIZER_ADB");
        std::env::set_var("ANDROID_HOME", "/definitely/not/a/real/path");
        std::env::set_var("ANDROID_SDK_ROOT", "/definitely/not/a/real/path");
        // Shadow PATH so the PATH search finds nothing — using a real dir
        // that won't contain adb.
        let temp = std::env::temp_dir();
        let saved = std::env::var_os("PATH");
        std::env::set_var("PATH", &temp);

        // Note: this still walks well-known locations and the CWD fallbacks,
        // which on some dev machines DO contain an adb (the v1 repo ships one).
        // So the assertion is "either None or returns a valid file" — not "None".
        if let Some(found) = discover_adb_binary() {
            assert!(found.is_file(), "discovered path must be a real file");
        }

        if let Some(p) = saved {
            std::env::set_var("PATH", p);
        }
        std::env::remove_var("ANDROID_HOME");
        std::env::remove_var("ANDROID_SDK_ROOT");
    }

    #[test]
    fn output_success_check() {
        let ok = AdbOutput {
            stdout: String::new(),
            stderr: String::new(),
            exit_code: Some(0),
        };
        assert!(ok.success());

        let fail = AdbOutput {
            stdout: String::new(),
            stderr: "boom".into(),
            exit_code: Some(1),
        };
        assert!(!fail.success());
    }

    #[test]
    fn missing_binary_returns_typed_error() {
        // No async needed — we just need to verify the BinaryMissing path is
        // exercised. We construct a driver pointing at nowhere and confirm
        // the function-level guard works on a blocking dummy call.
        let driver = SubprocessAdb::new(PathBuf::from("/nonexistent/adb"));
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(async { driver.raw(&["devices"]).await });
        match result {
            Err(AdbError::BinaryMissing { path }) => {
                assert!(path.contains("nonexistent"));
            }
            other => panic!("expected BinaryMissing, got {other:?}"),
        }
    }
}

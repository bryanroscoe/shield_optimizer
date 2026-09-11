//! Desktop subprocess-backed ADB driver.

use std::path::{Path, PathBuf};
use std::time::Duration;

use async_trait::async_trait;
use tokio::process::Command;
use tokio::time::timeout;
use tracing::{debug, warn};

use shield_optimizer_core::adb::{AdbDriver, AdbError, AdbOutput, AdbResult};

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
        self.run_with_timeout(args, self.command_timeout).await
    }

    async fn run_with_timeout(&self, args: &[&str], dur: Duration) -> AdbResult<AdbOutput> {
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

        let output = match timeout(dur, fut).await {
            Ok(r) => r?,
            Err(_) => {
                warn!(?args, "adb timeout");
                return Err(AdbError::Timeout {
                    seconds: dur.as_secs(),
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

/// Ceiling for file transfers: long enough for multi-GB pulls over slow Wi-Fi,
/// short enough that a genuinely hung adb still surfaces as an error.
const TRANSFER_TIMEOUT: Duration = Duration::from_secs(15 * 60);

#[async_trait]
impl AdbDriver for SubprocessAdb {
    async fn raw(&self, args: &[&str]) -> AdbResult<AdbOutput> {
        self.run(args).await
    }

    async fn raw_transfer(&self, args: &[&str]) -> AdbResult<AdbOutput> {
        self.run_with_timeout(args, TRANSFER_TIMEOUT).await
    }

    async fn shell(&self, serial: &str, command: &str) -> AdbResult<AdbOutput> {
        self.run(&["-s", serial, "shell", command]).await
    }

    async fn raw_bytes(&self, args: &[&str]) -> AdbResult<Vec<u8>> {
        if !self.binary.exists() {
            return Err(AdbError::BinaryMissing {
                path: self.binary.display().to_string(),
            });
        }

        debug!(adb = ?self.binary, ?args, "adb invoke (binary)");

        let mut cmd = Command::new(&self.binary);
        cmd.args(args).kill_on_drop(true);
        super::hide_console_window(&mut cmd);

        let output = match timeout(self.command_timeout, cmd.output()).await {
            Ok(r) => r?,
            Err(_) => {
                warn!(?args, "adb timeout");
                return Err(AdbError::Timeout {
                    seconds: self.command_timeout.as_secs(),
                });
            }
        };

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
            warn!(?args, code = ?output.status.code(), %stderr, "adb exited nonzero");
            return Err(AdbError::NonZeroExit {
                code: output.status.code(),
                stderr,
            });
        }

        Ok(output.stdout)
    }

    async fn spawn(&self, args: &[&str]) -> AdbResult<tokio::process::Child> {
        if !self.binary.exists() {
            return Err(AdbError::BinaryMissing {
                path: self.binary.display().to_string(),
            });
        }

        debug!(adb = ?self.binary, ?args, "adb spawn (long-lived)");

        let mut cmd = Command::new(&self.binary);
        cmd.args(args).kill_on_drop(true);
        // Verified on device: the device-side `app_process` aborts at startup
        // (exit 134, no Java output) when the adb client's stdin is fully
        // *closed*. It needs an open fd — /dev/null works. A GUI app's inherited
        // stdin is unreliable, so pin it to null explicitly. stdout/stderr are
        // nulled too so the resident child can never block on a full pipe.
        cmd.stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        super::hide_console_window(&mut cmd);

        cmd.spawn().map_err(AdbError::Io)
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
/// 2. App-managed platform-tools install
/// 3. `ANDROID_HOME` / `ANDROID_SDK_ROOT` env vars + `platform-tools/adb`
/// 4. PATH search (only finds adb on Linux/Windows or when the GUI was
///    launched from a shell that exported the right PATH)
/// 5. Well-known install locations per OS:
///    - macOS: `/opt/homebrew/bin/adb`, `/usr/local/bin/adb`,
///      `~/Library/Android/sdk/platform-tools/adb`
///    - Linux: `/usr/bin/adb`, `/usr/local/bin/adb`,
///      `~/Android/Sdk/platform-tools/adb`
///    - Windows: `%LOCALAPPDATA%\Android\Sdk\platform-tools\adb.exe`
/// 6. Repo-local fallback for v1 coexistence: `../adb` relative to the
///    Cargo workspace, since the v1 repo ships an adb binary there.
pub fn discover_adb_binary() -> Option<PathBuf> {
    let sources = AdbDiscoverySources {
        explicit_override: std::env::var_os("SHIELD_OPTIMIZER_ADB").map(PathBuf::from),
        managed_install: crate::adb::install::adb_path_in_install_root(),
        sdk_roots: ["ANDROID_HOME", "ANDROID_SDK_ROOT"]
            .into_iter()
            .filter_map(std::env::var_os)
            .map(PathBuf::from)
            .collect(),
        well_known: well_known_adb_locations(),
        cwd: std::env::current_dir().ok(),
    };

    discover_adb_binary_from(sources, || which_in_path(&adb_exe_name()), Path::is_file)
}

struct AdbDiscoverySources {
    explicit_override: Option<PathBuf>,
    managed_install: Option<PathBuf>,
    sdk_roots: Vec<PathBuf>,
    well_known: Vec<PathBuf>,
    cwd: Option<PathBuf>,
}

fn discover_adb_binary_from<F, P>(
    sources: AdbDiscoverySources,
    path_search: F,
    mut is_file: P,
) -> Option<PathBuf>
where
    F: FnOnce() -> Option<PathBuf>,
    P: FnMut(&Path) -> bool,
{
    let exe = adb_exe_name();

    for candidate in sources
        .explicit_override
        .into_iter()
        .chain(sources.managed_install)
        .chain(
            sources
                .sdk_roots
                .into_iter()
                .map(|root| root.join("platform-tools").join(&exe)),
        )
    {
        if is_file(&candidate) {
            return Some(candidate);
        }
    }

    // PATH traversal can touch every directory in PATH. Defer it until all
    // higher-priority candidates have failed instead of doing that work
    // eagerly on every discovery call.
    if let Some(candidate) = path_search() {
        return Some(candidate);
    }

    for candidate in sources.well_known {
        if is_file(&candidate) {
            return Some(candidate);
        }
    }

    // Repo-local fallback: the v1 repo ships `./adb` at the top level. Only
    // inspect the launch CWD after every installed location has failed.
    if let Some(cwd) = sources.cwd {
        for candidate in [Some(cwd.join("adb")), cwd.parent().map(|p| p.join("adb"))]
            .into_iter()
            .flatten()
        {
            if is_file(&candidate) {
                return Some(candidate);
            }
        }
    }

    None
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

    #[test]
    fn discovery_does_not_search_path_after_valid_override() {
        let temp = tempfile::tempdir().unwrap();
        let explicit = temp.path().join("explicit-adb");
        std::fs::write(&explicit, b"fake adb").unwrap();
        let sources = AdbDiscoverySources {
            explicit_override: Some(explicit.clone()),
            managed_install: Some(temp.path().join("managed-adb")),
            sdk_roots: vec![temp.path().join("sdk")],
            well_known: vec![temp.path().join("well-known-adb")],
            cwd: Some(temp.path().join("launch-volume")),
        };

        let found = discover_adb_binary_from(
            sources,
            || panic!("PATH must not be searched after a valid explicit override"),
            Path::is_file,
        );

        assert_eq!(found.as_deref(), Some(explicit.as_path()));
    }

    #[test]
    fn discovery_uses_path_after_higher_priority_candidates_fail() {
        let temp = tempfile::tempdir().unwrap();
        let path_adb = temp.path().join("path-adb");
        std::fs::write(&path_adb, b"fake adb").unwrap();
        let sources = AdbDiscoverySources {
            explicit_override: Some(temp.path().join("missing-explicit")),
            managed_install: Some(temp.path().join("missing-managed")),
            sdk_roots: vec![temp.path().join("missing-sdk")],
            well_known: Vec::new(),
            cwd: None,
        };

        let found = discover_adb_binary_from(sources, || Some(path_adb.clone()), Path::is_file);

        assert_eq!(found.as_deref(), Some(path_adb.as_path()));
    }

    #[test]
    fn discovery_does_not_search_path_after_valid_managed_install() {
        let temp = tempfile::tempdir().unwrap();
        let managed = temp.path().join("managed-adb");
        std::fs::write(&managed, b"fake adb").unwrap();
        let sources = AdbDiscoverySources {
            explicit_override: None,
            managed_install: Some(managed.clone()),
            sdk_roots: vec![temp.path().join("sdk")],
            well_known: vec![temp.path().join("well-known-adb")],
            cwd: Some(temp.path().join("launch-volume")),
        };

        let found = discover_adb_binary_from(
            sources,
            || panic!("PATH must not be searched after a valid managed install"),
            Path::is_file,
        );

        assert_eq!(found.as_deref(), Some(managed.as_path()));
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

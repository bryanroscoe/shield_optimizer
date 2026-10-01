//! Desktop subprocess-backed ADB driver.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::time::timeout;
use tracing::{debug, warn};

use shield_optimizer_core::adb::driver::process_output;
use shield_optimizer_core::adb::driver::{BoundedShellOutput, ShellTermination};
use shield_optimizer_core::adb::{AdbDriver, AdbError, AdbOutput, AdbResult};

use crate::session;

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
        let started = Instant::now();
        if !self.binary.exists() {
            let e = AdbError::BinaryMissing {
                path: self.binary.display().to_string(),
            };
            record_failure(args, started, &e, "text");
            return Err(e);
        }

        let safe_args = redact_args(args);
        debug!(adb = ?self.binary, args = ?safe_args, "adb invoke");

        let mut cmd = Command::new(&self.binary);
        cmd.args(args).kill_on_drop(true);
        super::hide_console_window(&mut cmd);
        super::pin_working_directory(&mut cmd);
        let fut = cmd.output();

        let output = match timeout(dur, fut).await {
            Ok(Ok(output)) => output,
            Ok(Err(io)) => {
                let e = AdbError::from(io);
                record_failure(args, started, &e, "text");
                return Err(e);
            }
            Err(_) => {
                warn!(args = ?safe_args, ms = started.elapsed().as_millis(), "adb timeout");
                let e = AdbError::Timeout {
                    seconds: dur.as_secs(),
                };
                record_failure(args, started, &e, "text");
                return Err(e);
            }
        };

        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        let exit_code = output.status.code();
        if session::is_recording() {
            session::record(adb_session_line(
                args,
                started.elapsed().as_millis(),
                SessionResult::Output {
                    exit_code,
                    stdout: &stdout,
                    stderr: &stderr,
                },
                "text",
            ));
        }

        // The whole invocation, at debug: what a bug report needs and what an
        // ordinary run must not carry. Args are redacted (pairing PINs), and
        // stdout is capped so one `pm list packages` cannot flood the file —
        // and redacted outright when it's a package inventory, since debug
        // logs are embedded verbatim in the Report-a-bug bundle.
        debug!(
            args = ?safe_args,
            ms = started.elapsed().as_millis(),
            ?exit_code,
            stderr = %stderr.trim(),
            stdout = %redact_stdout_for_log(&args.join(" "), &stdout),
            "adb done"
        );

        // Surface real process failures rather than letting callers parse
        // empty stdout as "no results". Exit-0 with empty stdout is a
        // legitimate response for many `pm` queries (e.g. "no disabled
        // packages matched"); exit-nonzero is the signal that something
        // actually went wrong.
        if !output.status.success() {
            warn!(args = ?safe_args, ?exit_code, %stderr, "adb exited nonzero");
        }
        process_output(stdout, stderr, exit_code)
    }
}

/// Cap on logged stdout. Debug logging exists to be pasted into a bug report;
/// a full `pm list packages` in every entry makes that unreadable and the file
/// enormous.
const LOG_STDOUT_LIMIT: usize = 2 * 1024;

fn truncate_for_log(text: &str) -> String {
    let text = text.trim();
    if text.len() <= LOG_STDOUT_LIMIT {
        return text.to_string();
    }
    let mut end = LOG_STDOUT_LIMIT;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}… [{} bytes truncated]", &text[..end], text.len() - end)
}

/// True when the invoked command (or, for a batched/expert-shell string, any
/// part of it) asks the device to enumerate installed packages —
/// `pm list packages` and its `-d`/`-e`/`-u`/`-3` variants. Truncation alone
/// still leaves dozens of package IDs in a 2 KiB window, and that inventory
/// must never reach the debug log: `collect_diagnostics` embeds the newest
/// log lines verbatim in the Report-a-bug bundle.
fn mentions_package_listing(command_text: &str) -> bool {
    command_text.to_ascii_lowercase().contains("list packages")
}

/// Redact stdout before it is logged: a full package inventory is replaced
/// with just its entry count, everything else goes through the ordinary
/// truncation.
fn redact_stdout_for_log(command_text: &str, stdout: &str) -> String {
    if mentions_package_listing(command_text) {
        let count = stdout
            .lines()
            .filter(|line| line.trim_start().starts_with("package:"))
            .count();
        format!("<redacted: package inventory, {count} entries>")
    } else {
        truncate_for_log(stdout)
    }
}

/// Strip the pairing PIN out of an argument list before it is logged.
///
/// `adb pair <host:port> <pin>` is the one call this app makes that carries a
/// secret the user read off their own screen. Debug logging is meant to be
/// pasted into a public issue, so the PIN must never reach the file in the
/// first place — redacting at the read end would be too late.
/// Typed text rides as `input text '<payload>'`, and that payload is whatever
/// the user typed or pasted into Remote — passwords and URLs included. The
/// debug log feeds the bug-report bundle, so the payload never reaches it.
fn redact_typed_text(arg: &str) -> String {
    match arg.find("input text") {
        Some(at) => format!("{}input text <redacted text>", &arg[..at]),
        None => arg.to_string(),
    }
}

fn redact_args(args: &[&str]) -> Vec<String> {
    let Some(pair_at) = args.iter().position(|a| *a == "pair") else {
        return args.iter().map(|a| redact_typed_text(a)).collect();
    };
    args.iter()
        .enumerate()
        .map(|(i, a)| {
            // Keep the subcommand and the address it pairs with; everything
            // after that is the code.
            if i <= pair_at + 1 {
                (*a).to_string()
            } else {
                "<redacted pin>".to_string()
            }
        })
        .collect()
}

/// What an adb call produced, as the session recorder sees it.
enum SessionResult<'a> {
    Output {
        exit_code: Option<i32>,
        stdout: &'a str,
        stderr: &'a str,
    },
    Bytes {
        exit_code: Option<i32>,
        len: usize,
        stderr: &'a str,
    },
    /// The call failed before producing any output (timeout, missing binary).
    Failed(String),
}

fn record_failure(args: &[&str], started: Instant, error: &AdbError, stream: &'static str) {
    if session::is_recording() {
        session::record(adb_session_line(
            args,
            started.elapsed().as_millis(),
            SessionResult::Failed(error.to_string()),
            stream,
        ));
    }
}

/// One `kind:"adb"` session line, redacted the same way as the debug log:
/// no pairing PIN and no typed Remote text, in the args or echoed back.
fn adb_session_line(
    args: &[&str],
    ms: u128,
    result: SessionResult<'_>,
    stream: &'static str,
) -> serde_json::Value {
    let serial = args
        .iter()
        .position(|a| *a == "-s")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.to_string());
    let pin_args: Vec<&str> = match args.iter().position(|a| *a == "pair") {
        Some(at) => args.iter().skip(at + 2).copied().collect(),
        None => Vec::new(),
    };
    let types_text = args.iter().any(|a| a.contains("input text"));
    let scrub = |text: &str| -> String {
        if types_text && !text.is_empty() {
            return "<redacted text>".to_string();
        }
        pin_args
            .iter()
            .filter(|pin| !pin.is_empty())
            .fold(text.to_string(), |acc, pin| {
                acc.replace(pin, "<redacted pin>")
            })
    };
    let (exit_code, stdout, stderr, error) = match result {
        SessionResult::Output {
            exit_code,
            stdout,
            stderr,
        } => (exit_code, scrub(stdout), scrub(stderr), None),
        SessionResult::Bytes {
            exit_code,
            len,
            stderr,
        } => (exit_code, format!("<{len} bytes>"), scrub(stderr), None),
        SessionResult::Failed(error) => (None, String::new(), String::new(), Some(scrub(&error))),
    };
    serde_json::json!({
        "v": 1,
        "kind": "adb",
        "ts": session::now_ts(),
        "serial": serial,
        "args": redact_args(args),
        "ms": ms,
        "exit_code": exit_code,
        "stdout": stdout,
        "stderr": stderr,
        "error": error,
        "stream": stream,
    })
}

/// The expert shell runs whatever the user typed, so neither the command nor
/// its output is recorded — only that it ran and how it ended.
fn bounded_session_line(
    serial: &str,
    ms: u128,
    result: &AdbResult<BoundedShellOutput>,
) -> serde_json::Value {
    let (exit_code, termination, error) = match result {
        Ok(out) => (out.exit_code, Some(out.termination), None),
        Err(e) => (None, None, Some(e.to_string())),
    };
    serde_json::json!({
        "v": 1,
        "kind": "adb",
        "ts": session::now_ts(),
        "serial": serial,
        "args": ["-s", serial, "shell", "<expert shell command redacted>"],
        "ms": ms,
        "exit_code": exit_code,
        "stdout": "",
        "stderr": "",
        "error": error,
        "stream": "bounded",
        "termination": termination,
    })
}

/// Ceiling for file transfers: long enough for multi-GB pulls over slow Wi-Fi,
/// short enough that a genuinely hung adb still surfaces as an error.
const TRANSFER_TIMEOUT: Duration = Duration::from_secs(15 * 60);

const SHELL_OUTPUT_LIMIT: usize = 256 * 1024;
const SHELL_TIMEOUT: Duration = Duration::from_secs(30);

async fn collect_bounded_shell(
    mut cmd: Command,
    duration: Duration,
) -> AdbResult<BoundedShellOutput> {
    use std::process::Stdio;
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = cmd.spawn()?;
    let mut stdout = child.stdout.take().expect("stdout configured as pipe");
    let mut stderr = child.stderr.take().expect("stderr configured as pipe");
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut out_buf = [0u8; 8192];
    let mut err_buf = [0u8; 8192];
    let mut out_eof = false;
    let mut err_eof = false;
    let mut status = None;
    let deadline = tokio::time::sleep(duration);
    tokio::pin!(deadline);
    let termination: std::io::Result<ShellTermination> = async {
        loop {
            if out_eof && err_eof && status.is_some() {
                return Ok(ShellTermination::Completed);
            }
            tokio::select! {
                _ = &mut deadline => return Ok(ShellTermination::Timeout),
                result = child.wait(), if status.is_none() => status = Some(result?),
                read = stdout.read(&mut out_buf), if !out_eof => {
                    let count = read?;
                    out_eof = count == 0;
                    out.extend_from_slice(&out_buf[..count.min(SHELL_OUTPUT_LIMIT - out.len())]);
                    if out.len() == SHELL_OUTPUT_LIMIT { return Ok(ShellTermination::OutputLimit); }
                },
                read = stderr.read(&mut err_buf), if !err_eof => {
                    let count = read?;
                    err_eof = count == 0;
                    err.extend_from_slice(&err_buf[..count.min(SHELL_OUTPUT_LIMIT - err.len())]);
                    if err.len() == SHELL_OUTPUT_LIMIT { return Ok(ShellTermination::OutputLimit); }
                },
            }
        }
    }
    .await;
    if status.is_none() {
        child.start_kill()?;
        status = Some(child.wait().await?);
    }
    Ok(BoundedShellOutput {
        stdout: bounded_utf8(&out),
        stderr: bounded_utf8(&err),
        exit_code: status.and_then(|status| status.code()),
        termination: termination?,
    })
}

fn bounded_utf8(bytes: &[u8]) -> String {
    let mut text = String::from_utf8_lossy(bytes).into_owned();
    let mut end = text.len().min(SHELL_OUTPUT_LIMIT);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
    text
}

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

    async fn shell_bounded(&self, serial: &str, command: &str) -> AdbResult<BoundedShellOutput> {
        let mut cmd = Command::new(&self.binary);
        cmd.args(["-s", serial, "shell", command]);
        super::hide_console_window(&mut cmd);
        super::pin_working_directory(&mut cmd);
        let started = Instant::now();
        let result = collect_bounded_shell(cmd, SHELL_TIMEOUT).await;
        if session::is_recording() {
            session::record(bounded_session_line(
                serial,
                started.elapsed().as_millis(),
                &result,
            ));
        }
        match &result {
            // Only the expert shell uses this path. Its command and output
            // are whatever the user chose to run, so neither is logged: the
            // debug log feeds the bug-report bundle.
            Ok(out) => debug!(
                serial,
                command = "<expert shell command redacted>",
                ms = started.elapsed().as_millis(),
                exit_code = ?out.exit_code,
                termination = ?out.termination,
                stdout_bytes = out.stdout.len(),
                stderr_bytes = out.stderr.len(),
                "adb shell (bounded) done"
            ),
            Err(e) => debug!(
                serial,
                command = "<expert shell command redacted>",
                ms = started.elapsed().as_millis(),
                error = %e,
                "adb shell (bounded) failed"
            ),
        }
        result
    }

    async fn raw_bytes(&self, args: &[&str]) -> AdbResult<Vec<u8>> {
        let started = Instant::now();
        if !self.binary.exists() {
            let e = AdbError::BinaryMissing {
                path: self.binary.display().to_string(),
            };
            record_failure(args, started, &e, "bytes");
            return Err(e);
        }

        let safe_args = redact_args(args);
        debug!(adb = ?self.binary, args = ?safe_args, "adb invoke (binary)");

        let mut cmd = Command::new(&self.binary);
        cmd.args(args).kill_on_drop(true);
        super::hide_console_window(&mut cmd);
        super::pin_working_directory(&mut cmd);

        let output = match timeout(self.command_timeout, cmd.output()).await {
            Ok(Ok(output)) => output,
            Ok(Err(io)) => {
                let e = AdbError::from(io);
                record_failure(args, started, &e, "bytes");
                return Err(e);
            }
            Err(_) => {
                warn!(args = ?safe_args, ms = started.elapsed().as_millis(), "adb timeout");
                let e = AdbError::Timeout {
                    seconds: self.command_timeout.as_secs(),
                };
                record_failure(args, started, &e, "bytes");
                return Err(e);
            }
        };
        if session::is_recording() {
            session::record(adb_session_line(
                args,
                started.elapsed().as_millis(),
                SessionResult::Bytes {
                    exit_code: output.status.code(),
                    len: output.stdout.len(),
                    stderr: &String::from_utf8_lossy(&output.stderr),
                },
                "bytes",
            ));
        }

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
            warn!(args = ?safe_args, code = ?output.status.code(), %stderr, "adb exited nonzero");
            return Err(AdbError::NonZeroExit {
                code: output.status.code(),
                stderr,
            });
        }

        // Binary stdout: its size is the useful fact, not its contents.
        debug!(
            args = ?safe_args,
            ms = started.elapsed().as_millis(),
            exit_code = ?output.status.code(),
            stdout_bytes = output.stdout.len(),
            "adb done (binary)"
        );

        Ok(output.stdout)
    }

    async fn spawn(&self, args: &[&str]) -> AdbResult<tokio::process::Child> {
        if !self.binary.exists() {
            return Err(AdbError::BinaryMissing {
                path: self.binary.display().to_string(),
            });
        }

        debug!(adb = ?self.binary, args = ?redact_args(args), "adb spawn (long-lived)");

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
        super::pin_working_directory(&mut cmd);

        cmd.spawn().map_err(AdbError::Io)
    }
}

/// Memoized result of [`discover_adb_binary`].
///
/// `None` = never resolved; `Some(result)` = resolved, including a cached
/// "not found". Discovery is not free and, on a machine with no adb installed,
/// its last resort is probing the launch directory — which is a removable
/// volume when the app was opened straight from its DMG. `adb_status` runs on
/// every Devices refresh, so an uncached lookup re-probed that volume every
/// few seconds and kept macOS asking for permission (GitHub #89).
static CACHED_ADB_PATH: std::sync::RwLock<Option<Option<PathBuf>>> = std::sync::RwLock::new(None);

/// [`discover_adb_binary`], resolved at most once per process until
/// [`forget_cached_adb_binary`] is called. Use this for anything that runs on
/// a refresh or a timer; the filesystem layout it inspects only changes when
/// the user installs something, and those paths invalidate explicitly.
pub fn cached_adb_binary() -> Option<PathBuf> {
    if let Ok(guard) = CACHED_ADB_PATH.read() {
        if let Some(cached) = guard.as_ref() {
            return cached.clone();
        }
    }
    let resolved = discover_adb_binary();
    if let Ok(mut guard) = CACHED_ADB_PATH.write() {
        *guard = Some(resolved.clone());
    }
    resolved
}

/// Drop the memoized path so the next [`cached_adb_binary`] re-resolves.
/// Called after anything that can change which binary is correct: a
/// platform-tools install, or the user explicitly asking to restart adb.
pub fn forget_cached_adb_binary() {
    if let Ok(mut guard) = CACHED_ADB_PATH.write() {
        *guard = None;
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

    fn output<'a>(exit_code: i32, stdout: &'a str, stderr: &'a str) -> SessionResult<'a> {
        SessionResult::Output {
            exit_code: Some(exit_code),
            stdout,
            stderr,
        }
    }

    #[test]
    fn a_session_line_carries_the_schema_the_replay_parser_reads() {
        let line = adb_session_line(
            &["-s", "tv:5555", "shell", "pm list packages"],
            12,
            output(0, "package:com.a\npackage:com.b\n", ""),
            "text",
        );
        assert_eq!(line["v"], 1);
        assert_eq!(line["kind"], "adb");
        assert_eq!(line["serial"], "tv:5555");
        assert_eq!(
            line["args"],
            serde_json::json!(["-s", "tv:5555", "shell", "pm list packages"])
        );
        assert_eq!(line["ms"], 12);
        assert_eq!(line["exit_code"], 0);
        // Session files are local and never bundled, so inventories stay whole.
        assert_eq!(line["stdout"], "package:com.a\npackage:com.b\n");
        assert_eq!(line["error"], serde_json::Value::Null);
        assert_eq!(line["stream"], "text");

        let line = adb_session_line(&["devices"], 1, output(0, "", ""), "text");
        assert_eq!(line["serial"], serde_json::Value::Null);
    }

    #[test]
    fn a_session_line_never_carries_the_pin_or_typed_text() {
        let line = adb_session_line(
            &["pair", "192.168.1.9:37421", "314159"],
            5,
            output(1, "", "failed: wrong code 314159"),
            "text",
        );
        assert!(!line.to_string().contains("314159"));
        assert_eq!(line["args"][2], "<redacted pin>");

        let line = adb_session_line(
            &["-s", "tv:5555", "shell", "input text 'hunter2'"],
            5,
            output(1, "hunter2", "Error typing hunter2"),
            "text",
        );
        assert!(!line.to_string().contains("hunter2"));
        assert_eq!(line["args"][3], "input text <redacted text>");
    }

    #[test]
    fn a_timeout_is_recorded_as_an_error_without_an_exit_code() {
        let line = adb_session_line(
            &["-s", "tv:5555", "shell", "getprop"],
            30000,
            SessionResult::Failed(AdbError::Timeout { seconds: 30 }.to_string()),
            "text",
        );
        assert_eq!(line["exit_code"], serde_json::Value::Null);
        assert!(line["error"].as_str().unwrap().contains("timed out"));
    }

    #[test]
    fn binary_output_is_recorded_as_its_size() {
        let line = adb_session_line(
            &["-s", "tv:5555", "exec-out", "screencap", "-p"],
            40,
            SessionResult::Bytes {
                exit_code: Some(0),
                len: 2048,
                stderr: "",
            },
            "bytes",
        );
        assert_eq!(line["stdout"], "<2048 bytes>");
        assert_eq!(line["stream"], "bytes");
    }

    #[test]
    fn the_expert_shell_is_recorded_without_its_command_or_output() {
        let result = Ok(BoundedShellOutput {
            stdout: "secret output".into(),
            stderr: "secret error".into(),
            exit_code: Some(0),
            termination: ShellTermination::Completed,
        });
        let line = bounded_session_line("tv:5555", 9, &result);
        let text = line.to_string();
        assert!(!text.contains("secret"));
        assert_eq!(line["args"][3], "<expert shell command redacted>");
        assert_eq!(line["exit_code"], 0);
        assert_eq!(line["termination"], "completed");
        assert_eq!(line["stream"], "bounded");
    }

    /// Debug logging is written to be pasted into a public issue. The pairing
    /// PIN the user read off their TV must not be in it.
    #[test]
    fn typed_remote_text_never_reaches_the_log() {
        assert_eq!(
            redact_args(&["-s", "tv:5555", "shell", "input text 'hunter2'"]),
            vec!["-s", "tv:5555", "shell", "input text <redacted text>"]
        );
    }

    #[test]
    fn a_pairing_pin_never_reaches_the_log() {
        assert_eq!(
            redact_args(&["pair", "192.168.1.9:37421", "314159"]),
            vec!["pair", "192.168.1.9:37421", "<redacted pin>"]
        );
        // Everything else is logged as-is — the address is what makes a report
        // legible, and there is no secret in it.
        assert_eq!(
            redact_args(&["-s", "192.168.1.9:5555", "shell", "getprop"]),
            vec!["-s", "192.168.1.9:5555", "shell", "getprop"]
        );
    }

    /// Debug logs are embedded verbatim in the Report-a-bug bundle. A package
    /// inventory must never reach it, even truncated — only its entry count.
    #[test]
    fn package_listing_stdout_is_redacted_not_truncated() {
        let stdout = "package:com.google.android.tv.launcher\npackage:com.netflix.ninja\n";
        assert_eq!(
            redact_stdout_for_log("-s 192.168.1.9:5555 shell pm list packages", stdout),
            "<redacted: package inventory, 2 entries>"
        );
        // The `-d`/`-3` variants and a batched command string are covered too.
        assert_eq!(
            redact_stdout_for_log("pm list packages -d", "package:com.disabled.app\n"),
            "<redacted: package inventory, 1 entries>"
        );
        assert_eq!(
            redact_stdout_for_log(
                "pm list packages 2>/dev/null; echo __SEP__; dumpsys meminfo",
                "package:com.a\npackage:com.b\n"
            ),
            "<redacted: package inventory, 2 entries>"
        );
        // An unrelated command still goes through ordinary truncation.
        assert_eq!(
            redact_stdout_for_log("getprop ro.serialno", "  ABC123  "),
            "ABC123"
        );
    }

    #[test]
    fn logged_stdout_is_capped_without_splitting_a_character() {
        let long = "é".repeat(4096);
        let logged = truncate_for_log(&long);
        assert!(logged.len() < long.len());
        assert!(logged.contains("bytes truncated"));
        assert_eq!(truncate_for_log("  short  "), "short");
    }

    #[cfg(unix)]
    async fn shell_fixture(script: &str, duration: Duration) -> BoundedShellOutput {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", script]);
        collect_bounded_shell(command, duration).await.unwrap()
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn bounded_shell_preserves_nonzero_and_both_streams() {
        let result = shell_fixture("printf out; printf err >&2; exit 7", SHELL_TIMEOUT).await;
        assert_eq!(result.stdout, "out");
        assert_eq!(result.stderr, "err");
        assert_eq!(result.exit_code, Some(7));
        assert_eq!(result.termination, ShellTermination::Completed);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn ordinary_subprocess_nonzero_contract_is_unchanged() {
        let driver = SubprocessAdb::new(PathBuf::from("/bin/sh"));
        let result = driver.raw(&["-c", "printf failure >&2; exit 7"]).await;
        assert!(
            matches!(result, Err(AdbError::NonZeroExit { code: Some(7), stderr }) if stderr == "failure")
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn bounded_shell_stops_continuous_output() {
        let result = shell_fixture(
            "while :; do printf 0123456789; printf abcdefghij >&2; done",
            SHELL_TIMEOUT,
        )
        .await;
        assert_eq!(result.termination, ShellTermination::OutputLimit);
        assert!(result.stdout.len() <= SHELL_OUTPUT_LIMIT);
        assert!(result.stderr.len() <= SHELL_OUTPUT_LIMIT);
        assert!(!result.stdout.is_empty());
        assert!(!result.stderr.is_empty());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn bounded_shell_deadline_keeps_partial_output() {
        let result = shell_fixture(
            "printf started; while :; do :; done",
            Duration::from_secs(2),
        )
        .await;
        assert_eq!(result.termination, ShellTermination::Timeout);
        assert_eq!(result.stdout, "started");
    }

    #[test]
    fn bounded_utf8_handles_incomplete_and_invalid_characters() {
        let text = format!("a{}", "€".repeat(SHELL_OUTPUT_LIMIT));
        let output = bounded_utf8(&text.as_bytes()[..SHELL_OUTPUT_LIMIT - 1]);
        assert!(output.len() <= SHELL_OUTPUT_LIMIT);
        assert!(output.starts_with("a€"));
        assert!(bounded_utf8(&vec![0xff; SHELL_OUTPUT_LIMIT]).len() <= SHELL_OUTPUT_LIMIT);
    }

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

    /// The launch directory is the last thing discovery inspects, and it is a
    /// removable volume whenever the app was opened straight from its DMG.
    /// Nothing must reach it while an installed adb exists.
    #[test]
    fn discovery_never_touches_the_launch_directory_when_adb_is_installed() {
        let temp = tempfile::tempdir().unwrap();
        let well_known = temp.path().join("well-known-adb");
        std::fs::write(&well_known, b"fake adb").unwrap();
        let volume = temp.path().join("Volumes").join("SHIELD OPTIMIZER");
        let sources = AdbDiscoverySources {
            explicit_override: None,
            managed_install: None,
            sdk_roots: Vec::new(),
            well_known: vec![well_known.clone()],
            cwd: Some(volume.clone()),
        };

        let mut inspected = Vec::new();
        let found = discover_adb_binary_from(
            sources,
            || None,
            |candidate| {
                inspected.push(candidate.to_path_buf());
                candidate.is_file()
            },
        );

        assert_eq!(found.as_deref(), Some(well_known.as_path()));
        assert!(
            !inspected.iter().any(|path| path.starts_with(&volume)),
            "launch directory was probed anyway: {inspected:?}"
        );
    }

    /// `adb_status` runs on every Devices refresh. Before memoization that
    /// meant a full re-walk — PATH included, launch directory included — every
    /// few seconds on a machine with no adb installed, which is what kept
    /// macOS re-asking for removable-volume access (GitHub #89).
    #[test]
    fn repeated_discovery_resolves_once_until_explicitly_forgotten() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        // Serialize against any other test that touches the process-global
        // cache, and start from a known-empty one.
        static GUARD: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = GUARD.lock().unwrap_or_else(|e| e.into_inner());
        forget_cached_adb_binary();

        static RESOLVES: AtomicUsize = AtomicUsize::new(0);
        fn resolve_once() -> Option<PathBuf> {
            RESOLVES.fetch_add(1, Ordering::SeqCst);
            None
        }

        // Model `cached_adb_binary`'s contract over an injectable resolver:
        // the real one calls `discover_adb_binary`, which touches the live
        // filesystem and cannot be asserted on here.
        fn cached_with(resolve: fn() -> Option<PathBuf>) -> Option<PathBuf> {
            if let Ok(guard) = CACHED_ADB_PATH.read() {
                if let Some(cached) = guard.as_ref() {
                    return cached.clone();
                }
            }
            let resolved = resolve();
            if let Ok(mut guard) = CACHED_ADB_PATH.write() {
                *guard = Some(resolved.clone());
            }
            resolved
        }

        for _ in 0..10 {
            assert_eq!(cached_with(resolve_once), None);
        }
        assert_eq!(
            RESOLVES.load(Ordering::SeqCst),
            1,
            "a cached miss must not re-probe the filesystem"
        );

        // Installing adb or hitting Restart ADB is the only thing that may
        // make us look again.
        forget_cached_adb_binary();
        assert_eq!(cached_with(resolve_once), None);
        assert_eq!(RESOLVES.load(Ordering::SeqCst), 2);

        forget_cached_adb_binary();
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

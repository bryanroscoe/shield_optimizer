//! Adapter contracts with scripted sessions and a fake executable, not live adbd acceptance.

use super::*;
use crate::adb::driver::SubprocessAdb;
use shield_optimizer_core::adb::{BoundedShellOutput, ShellTermination};
use std::sync::Mutex as StdMutex;
use std::time::Duration;

const TV_A: &str = "192.0.2.10:5555";
const TV_B: &str = "192.0.2.11:5555";

#[derive(Default)]
struct ScriptedSession {
    serial: StdMutex<String>,
    ready: AtomicBool,
    calls: Arc<StdMutex<Vec<Vec<String>>>>,
}

impl ScriptedSession {
    fn record(&self, args: &[&str]) {
        self.calls
            .lock()
            .unwrap()
            .push(args.iter().map(|arg| (*arg).into()).collect());
    }

    fn response(&self, serial: &str, command: &str) -> AdbResult<AdbOutput> {
        assert_eq!(serial, *self.serial.lock().unwrap());
        match command {
            "both-streams" => Ok(AdbOutput {
                stdout: format!("{serial}: output\n"),
                stderr: "warning\n".into(),
                exit_code: Some(0),
            }),
            "stderr-failure" => Err(AdbError::NonZeroExit {
                code: Some(7),
                stderr: "rejected\n".into(),
            }),
            "stdout-failure" => Err(AdbError::NonZeroExit {
                code: Some(9),
                stderr: "Failure [blocked]\n".into(),
            }),
            "deadline" => Err(AdbError::Timeout { seconds: 1 }),
            "offline" => Err(AdbError::Transport("device offline".into())),
            _ => panic!("unregistered comparison scenario: {command}"),
        }
    }
}

#[async_trait]
impl AdbDriver for ScriptedSession {
    async fn raw(&self, _args: &[&str]) -> AdbResult<AdbOutput> {
        Err(AdbError::Unsupported {
            operation: "comparison raw",
        })
    }

    async fn shell(&self, serial: &str, command: &str) -> AdbResult<AdbOutput> {
        self.record(&["-s", serial, "shell", command]);
        self.response(serial, command)
    }

    async fn shell_bounded(&self, serial: &str, command: &str) -> AdbResult<BoundedShellOutput> {
        self.record(&["-s", serial, "shell", command]);
        assert_eq!(serial, *self.serial.lock().unwrap());
        assert_eq!(command, "bounded-failure");
        Ok(BoundedShellOutput {
            stdout: "partial output\n".into(),
            stderr: "rejected\n".into(),
            exit_code: Some(7),
            termination: ShellTermination::Completed,
        })
    }

    async fn raw_bytes(&self, args: &[&str]) -> AdbResult<Vec<u8>> {
        self.record(args);
        assert_eq!(
            args,
            ["-s", &*self.serial.lock().unwrap(), "exec-out", "binary"]
        );
        Ok(vec![0, 255, 128, 1])
    }
}

#[async_trait]
impl NetworkSession for ScriptedSession {
    async fn connect(&self, host: &str, port: u16, _request: u64) -> Result<String, String> {
        *self.serial.lock().unwrap() = format!("{host}:{port}");
        self.ready.store(true, Ordering::Release);
        Ok("connected".into())
    }

    async fn pair(&self, _host: &str, _port: u16, _code: &str) -> Result<String, String> {
        panic!("comparison must not pair or create an identity")
    }

    async fn ready(&self) -> bool {
        self.ready.load(Ordering::Acquire)
    }

    fn invalidate(&self, _request: u64) {
        self.ready.store(false, Ordering::Release);
    }

    fn close_idle(&self) -> Result<(), String> {
        self.ready.store(false, Ordering::Release);
        Ok(())
    }
}

struct Comparison {
    native: DirectNetworkAdb,
    subprocess: SubprocessAdb,
    calls: Arc<StdMutex<Vec<Vec<String>>>>,
    directory: tempfile::TempDir,
}

impl Comparison {
    async fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let binary = directory.path().join("fake-adb");
        let call_log = directory.path().join("calls");
        crate::adb::write_script(
            &binary,
            &format!(
                r#"printf '%s\n' "$*" >> '{}'
if [ "$1" = '-s' ] && [ "$3" = 'exec-out' ] && [ "$4" = 'binary' ]; then
    printf '\000\377\200\001'
    exit 0
fi
[ "$1" = '-s' ] && [ "$3" = 'shell' ] || exit 80
case "$4" in
    both-streams) printf '%s: output\n' "$2"; printf 'warning\n' >&2 ;;
    stderr-failure) printf 'rejected\n' >&2; exit 7 ;;
    stdout-failure) printf 'Failure [blocked]\n'; exit 9 ;;
    bounded-failure) printf 'partial output\n'; printf 'rejected\n' >&2; exit 7 ;;
    deadline) exec sleep 30 ;;
    offline) printf 'error: device offline\n' >&2; exit 1 ;;
    *) exit 81 ;;
esac"#,
                call_log.display()
            ),
        );
        let calls = Arc::new(StdMutex::new(Vec::new()));
        let captured = Arc::clone(&calls);
        let native = DirectNetworkAdb::with_factory(
            directory.path().join("identity-must-not-be-created"),
            Arc::new(move |_, _| {
                Arc::new(ScriptedSession {
                    calls: Arc::clone(&captured),
                    ..ScriptedSession::default()
                })
            }),
        );
        for serial in [TV_A, TV_B] {
            native.raw(&["connect", serial]).await.unwrap();
        }
        Self {
            native,
            subprocess: SubprocessAdb::new(binary)
                .with_timeout(Duration::from_secs(1))
                .with_child_leases(Leases::new()),
            calls,
            directory,
        }
    }

    fn assert_commands(&self, expected: &[String]) {
        let subprocess = std::fs::read_to_string(self.directory.path().join("calls")).unwrap();
        let subprocess: Vec<_> = subprocess.lines().map(str::to_owned).collect();
        assert_eq!(subprocess.as_slice(), expected);
        let native: Vec<_> = self
            .calls
            .lock()
            .unwrap()
            .iter()
            .map(|a| a.join(" "))
            .collect();
        assert_eq!(native.as_slice(), expected);
        assert!(!self
            .directory
            .path()
            .join("identity-must-not-be-created")
            .exists());
    }

    fn assert_calls(&self, expected: &[(&str, &str)]) {
        self.assert_commands(
            &expected
                .iter()
                .map(|(serial, command)| format!("-s {serial} shell {command}"))
                .collect::<Vec<_>>(),
        );
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Outcome {
    Output(String, String, Option<i32>),
    NonZero(Option<i32>, String),
    Timeout,
}

fn outcome(result: AdbResult<AdbOutput>) -> Outcome {
    match result {
        Ok(output) => Outcome::Output(output.stdout, output.stderr, output.exit_code),
        Err(AdbError::NonZeroExit { code, stderr }) => Outcome::NonZero(code, stderr),
        Err(AdbError::Timeout { .. }) => Outcome::Timeout,
        other => panic!("unexpected comparison result: {other:?}"),
    }
}

#[tokio::test]
async fn controlled_shell_matrix_preserves_targets_streams_and_failures_without_replay() {
    let comparison = Comparison::new().await;
    let cases = [
        (TV_A, "both-streams"),
        (TV_B, "both-streams"),
        (TV_A, "stderr-failure"),
        (TV_B, "stdout-failure"),
        (TV_A, "deadline"),
    ];
    for (serial, command) in cases {
        let native = outcome(comparison.native.shell(serial, command).await);
        let subprocess = outcome(comparison.subprocess.shell(serial, command).await);
        assert_eq!(native, subprocess, "{serial} {command}");
    }
    comparison.assert_calls(&cases);
}

#[tokio::test]
async fn controlled_bounded_nonzero_and_binary_results_match() {
    let comparison = Comparison::new().await;
    let native = comparison
        .native
        .shell_bounded(TV_B, "bounded-failure")
        .await
        .unwrap();
    let subprocess = comparison
        .subprocess
        .shell_bounded(TV_B, "bounded-failure")
        .await
        .unwrap();
    assert_eq!(native.stdout, subprocess.stdout);
    assert_eq!(native.stderr, subprocess.stderr);
    assert_eq!(native.exit_code, subprocess.exit_code);
    assert_eq!(native.termination, subprocess.termination);
    let args = ["-s", TV_A, "exec-out", "binary"];
    assert_eq!(
        comparison.native.raw_bytes(&args).await.unwrap(),
        vec![0, 255, 128, 1]
    );
    assert_eq!(
        comparison.subprocess.raw_bytes(&args).await.unwrap(),
        vec![0, 255, 128, 1]
    );
    comparison.assert_commands(&[
        format!("-s {TV_B} shell bounded-failure"),
        format!("-s {TV_A} exec-out binary"),
    ]);
}

#[tokio::test]
async fn controlled_offline_failure_remains_visible_without_implicit_retry() {
    let comparison = Comparison::new().await;
    assert!(matches!(comparison.native.shell(TV_A, "offline").await,
        Err(AdbError::Transport(message)) if message == "device offline"));
    assert!(matches!(comparison.subprocess.shell(TV_A, "offline").await,
        Err(AdbError::NonZeroExit { code: Some(1), stderr }) if stderr == "error: device offline\n"));
    comparison.assert_calls(&[(TV_A, "offline")]);
}

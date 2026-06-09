//! Tauri command handlers — the thin bridge between the frontend and the
//! engine + ADB driver. Per architectural commitment #1, no business logic
//! lives here; commands fetch facts via the ADB driver, hand them to the
//! engine for decision-making, and return the result.

pub mod apps;
pub mod backup;
pub mod devices;
pub mod files;
pub mod health;
pub mod home_tracking;
pub mod input;
pub mod install;
pub mod launcher;
pub mod loader;
pub mod optimize;
pub mod reboot;
pub mod recovery;
pub mod scan;
pub mod screenshot;
pub mod sideload;
pub mod snapshot;
pub mod state;
pub mod tuning;
pub mod update;

pub use state::AppState;

/// Setting keys must match `[A-Za-z0-9._-]+` — all real Android setting keys
/// do. Anything else (spaces, semicolons, `$`, backticks, …) would be
/// interpolated verbatim into a `settings` command and re-parsed by the
/// device shell. Shared by the tweaks writer and snapshot apply — one
/// implementation so the security check can't drift.
pub(crate) fn is_valid_setting_key(key: &str) -> bool {
    !key.is_empty()
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// Single-quote a value for the device-side shell — the standard `'\''`
/// idiom (same as `quote_path` in files.rs). Spaces and all shell
/// metacharacters inside the value are inert once wrapped.
pub(crate) fn quote_shell_arg(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// Test-only ADB driver + state builder, shared across command unit tests.
/// Lets a test stub `adb`/`shell` output by substring and run a command's
/// reusable `_impl` against it without a real device.
#[cfg(test)]
pub mod test_support {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;

    use crate::adb::{AdbDriver, AdbError, AdbOutput, AdbResult};
    use crate::engine::AppListBundle;

    use super::AppState;

    /// One canned response. `Ok` returns it as stdout (exit 0); `Err` makes the
    /// call return a typed `AdbError` so command error branches get exercised.
    enum Reply {
        Ok(String),
        Err(String),
    }

    /// A substring rule plus its (possibly sequenced) responses. `cursor`
    /// advances per matching call and sticks on the last reply, so a single
    /// needle can hand back different outputs across calls — needed to model a
    /// resolver whose answer changes after a `pm disable-user`.
    struct Rule {
        needle: String,
        replies: Vec<Reply>,
        cursor: AtomicUsize,
    }

    impl Rule {
        fn single(needle: &str, reply: Reply) -> Self {
            Self {
                needle: needle.into(),
                replies: vec![reply],
                cursor: AtomicUsize::new(0),
            }
        }
    }

    #[derive(Default)]
    pub struct MockAdb {
        shell_rules: Vec<Rule>,
        raw_rules: Vec<Rule>,
        shell_log: Arc<Mutex<Vec<String>>>,
    }

    impl MockAdb {
        /// Return `stdout` for any `shell` whose command contains `needle`.
        pub fn on_shell(mut self, needle: &str, stdout: &str) -> Self {
            self.shell_rules
                .push(Rule::single(needle, Reply::Ok(stdout.into())));
            self
        }
        /// Make matching `shell` calls fail with a typed `AdbError` carrying
        /// `err_message` — the subprocess-failure branch (nonzero exit).
        pub fn on_shell_err(mut self, needle: &str, err_message: &str) -> Self {
            self.shell_rules
                .push(Rule::single(needle, Reply::Err(err_message.into())));
            self
        }
        /// Matching `shell` calls return Ok but with output that trips
        /// `shell_reported_failure()` — how `pm`/`cmd` report errors without a
        /// nonzero exit. Pass a `stdout` containing "Failure"/"Error". Distinct
        /// name from `on_shell` keeps the intent legible at the call site.
        pub fn on_shell_failure(self, needle: &str, stdout: &str) -> Self {
            self.on_shell(needle, stdout)
        }
        /// Hand back `responses` in order for successive matching `shell` calls,
        /// sticking on the last once exhausted.
        pub fn on_shell_seq(mut self, needle: &str, responses: &[&str]) -> Self {
            self.shell_rules.push(Rule {
                needle: needle.into(),
                replies: responses.iter().map(|s| Reply::Ok((*s).into())).collect(),
                cursor: AtomicUsize::new(0),
            });
            self
        }
        /// Return `stdout` for any `raw` whose joined args contain `needle`.
        pub fn on_raw(mut self, needle: &str, stdout: &str) -> Self {
            self.raw_rules
                .push(Rule::single(needle, Reply::Ok(stdout.into())));
            self
        }
        /// Make matching `raw` calls fail with a typed `AdbError`.
        pub fn on_raw_err(mut self, needle: &str, err_message: &str) -> Self {
            self.raw_rules
                .push(Rule::single(needle, Reply::Err(err_message.into())));
            self
        }
        /// Shared handle to the recorded `shell` command log. Grab it before
        /// moving the mock into `state_with`, then assert on side-effect
        /// commands (e.g. that a revert `pm enable` was — or wasn't — issued).
        pub fn shell_log(&self) -> Arc<Mutex<Vec<String>>> {
            Arc::clone(&self.shell_log)
        }
    }

    fn ok(stdout: String) -> AdbResult<AdbOutput> {
        Ok(AdbOutput {
            stdout,
            stderr: String::new(),
            exit_code: Some(0),
        })
    }

    /// First-match-wins across rules in registration order, advancing the
    /// matched rule's cursor. Unmatched calls succeed with empty stdout, so
    /// existing tests that register no rules keep their default behavior.
    fn reply_for(rules: &[Rule], haystack: &str) -> AdbResult<AdbOutput> {
        for rule in rules {
            if haystack.contains(rule.needle.as_str()) {
                let idx = rule
                    .cursor
                    .fetch_add(1, Ordering::Relaxed)
                    .min(rule.replies.len() - 1);
                return match &rule.replies[idx] {
                    Reply::Ok(out) => ok(out.clone()),
                    Reply::Err(msg) => Err(AdbError::NonZeroExit {
                        code: Some(1),
                        stderr: msg.clone(),
                    }),
                };
            }
        }
        ok(String::new())
    }

    #[async_trait]
    impl AdbDriver for MockAdb {
        async fn raw(&self, args: &[&str]) -> AdbResult<AdbOutput> {
            reply_for(&self.raw_rules, &args.join(" "))
        }
        async fn shell(&self, _serial: &str, command: &str) -> AdbResult<AdbOutput> {
            self.shell_log.lock().unwrap().push(command.to_string());
            reply_for(&self.shell_rules, command)
        }
    }

    pub fn state_with(mock: MockAdb) -> AppState {
        AppState::new(
            Arc::new(mock),
            AppListBundle::default(),
            std::env::temp_dir(),
        )
    }
}

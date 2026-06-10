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
    use std::sync::Arc;

    use async_trait::async_trait;

    use crate::adb::{AdbDriver, AdbOutput, AdbResult};
    use crate::engine::AppListBundle;

    use super::AppState;

    #[derive(Default)]
    pub struct MockAdb {
        shell_rules: Vec<(String, String)>,
        raw_rules: Vec<(String, String)>,
    }

    impl MockAdb {
        /// Return `stdout` for any `shell` whose command contains `needle`.
        pub fn on_shell(mut self, needle: &str, stdout: &str) -> Self {
            self.shell_rules.push((needle.into(), stdout.into()));
            self
        }
        /// Return `stdout` for any `raw` whose joined args contain `needle`.
        pub fn on_raw(mut self, needle: &str, stdout: &str) -> Self {
            self.raw_rules.push((needle.into(), stdout.into()));
            self
        }
    }

    fn ok(stdout: String) -> AdbResult<AdbOutput> {
        Ok(AdbOutput {
            stdout,
            stderr: String::new(),
            exit_code: Some(0),
        })
    }

    #[async_trait]
    impl AdbDriver for MockAdb {
        async fn raw(&self, args: &[&str]) -> AdbResult<AdbOutput> {
            let joined = args.join(" ");
            for (needle, out) in &self.raw_rules {
                if joined.contains(needle.as_str()) {
                    return ok(out.clone());
                }
            }
            ok(String::new())
        }
        async fn shell(&self, _serial: &str, command: &str) -> AdbResult<AdbOutput> {
            for (needle, out) in &self.shell_rules {
                if command.contains(needle.as_str()) {
                    return ok(out.clone());
                }
            }
            ok(String::new())
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

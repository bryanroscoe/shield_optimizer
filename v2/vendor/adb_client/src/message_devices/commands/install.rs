// Derived in part from Kadb (Apache-2.0): Kadb.kt (install, installMultiple,
// uninstall and their helpers).

use std::{
    fs::File,
    io::{ErrorKind, Read},
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crate::{
    Result, RustADBError, ShellExit,
    message_devices::{
        adb_message_device::ADBMessageDevice,
        adb_message_transport::{ADBMessageTransport, DEFAULT_READ_TIMEOUT},
        adb_service::{ADBService, adb_error},
        adb_sync::{PUSH_FILE_MODE, now_mtime},
    },
    utils::check_extension_is_apk,
};

/// AOSP's client reads a status line of at most `BUFSIZ`; keep a little more.
const INSTALL_OUTPUT_LIMIT: usize = 64 * 1024;

/// How install commands reach the package manager, best first, as AOSP's
/// `adb install` chooses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InstallChannel {
    /// `abb_exec:` talks to the package service directly; arguments are
    /// NUL-separated and never shell-parsed.
    Abb,
    /// `exec:cmd package …`, streaming the APK through stdin.
    Cmd,
    /// Devices without `cmd` (before Android 7): push the APK to
    /// `/data/local/tmp` and run `pm`.
    Pm,
}

/// AOSP's `escape_arg`: single-quote, with `'` written as `'\''`.
fn escape_arg(arg: &str) -> String {
    format!("'{}'", arg.replace('\'', r"'\''"))
}

fn shell_words(args: &[&str]) -> String {
    args.iter()
        .map(|arg| escape_arg(arg))
        .collect::<Vec<_>>()
        .join(" ")
}

/// AOSP judges every streamed install step by its reply starting with
/// `Success`.
fn succeeded(output: &str) -> bool {
    output.starts_with("Success")
}

fn failure(output: &str) -> RustADBError {
    let output = output.trim();
    RustADBError::InstallFailed(if output.is_empty() {
        "the device closed the install stream without a result".to_string()
    } else {
        output.to_string()
    })
}

/// `pm` run through a shell. Old `pm` prints a `pkg:` line before
/// `Success`, so any line may carry it; a reported nonzero exit or a
/// `Failure` line still fails.
fn pm_succeeded(output: &str, exit: ShellExit) -> bool {
    if matches!(exit, ShellExit::Code(code) if code != 0) {
        return false;
    }
    let mut lines = output.lines().map(str::trim);
    let success = lines.clone().any(|line| line.starts_with("Success"));
    success && !lines.any(|line| line.starts_with("Failure"))
}

/// AOSP takes the session id from the last `[…]` of a `Success` reply.
fn session_id(output: &str) -> Option<String> {
    if !succeeded(output) {
        return None;
    }
    let start = output.rfind('[')?;
    let end = output.rfind(']')?;
    let id = output.get(start + 1..end)?;
    (!id.is_empty() && id.bytes().all(|byte| byte.is_ascii_digit())).then(|| id.to_string())
}

/// A split's name inside the install session. Only its uniqueness matters;
/// the package manager reads split names from the APKs themselves.
fn split_name(index: usize, file_name: &str) -> String {
    let safe: String = file_name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("{index}_{safe}")
}

fn temp_path(index: usize) -> String {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    format!("/data/local/tmp/adb-install-{stamp}-{index}.apk")
}

#[derive(Debug)]
struct Apk {
    file: File,
    size: u64,
    name: String,
}

impl Apk {
    fn open(path: &Path) -> Result<Self> {
        check_extension_is_apk(path)?;
        let file = File::open(path).map_err(RustADBError::LocalIo)?;
        let size = file.metadata().map_err(RustADBError::LocalIo)?.len();
        let name = path.file_name().map_or_else(
            || "base.apk".to_string(),
            |name| name.to_string_lossy().into_owned(),
        );
        Ok(Self { file, size, name })
    }
}

/// Send exactly `size` bytes of `file`; a file that shrank meanwhile would
/// leave the package manager waiting for bytes that never come.
fn copy_apk<T: ADBMessageTransport>(apk: &mut Apk, stream: &mut ADBService<T>) -> Result<()> {
    let mut chunk = vec![0_u8; stream.max_write().min(1024 * 1024)];
    let mut remaining = apk.size;
    while remaining > 0 {
        let want = usize::try_from(remaining).map_or(chunk.len(), |left| left.min(chunk.len()));
        let read = match apk.file.read(&mut chunk[..want]) {
            Ok(0) => {
                return Err(RustADBError::LocalIo(std::io::Error::new(
                    ErrorKind::UnexpectedEof,
                    "the APK became shorter while it was being installed",
                )));
            }
            Ok(read) => read,
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(error) => return Err(RustADBError::LocalIo(error)),
        };
        std::io::Write::write_all(stream, &chunk[..read]).map_err(adb_error)?;
        remaining -= read as u64;
    }
    Ok(())
}

impl<T: ADBMessageTransport> ADBMessageDevice<T> {
    pub(crate) fn install(&mut self, apk_path: &dyn AsRef<Path>, user: Option<&str>) -> Result<()> {
        let mut options = Vec::new();
        if let Some(user) = user {
            options.extend(["--user", user]);
        }
        let output = self.install_packages(&[apk_path.as_ref()], &options, DEFAULT_READ_TIMEOUT)?;
        log::info!(
            "APK file {} installed: {}",
            apk_path.as_ref().display(),
            output.trim()
        );
        Ok(())
    }

    /// Install one APK, or several splits of one app together, the way
    /// AOSP's `adb install` / `install-multiple` and Kadb do: streamed over
    /// `exec:cmd` (one APK) or an `abb_exec`/`exec:cmd` install session
    /// (several), falling back to pushing to `/data/local/tmp` and `pm` on
    /// devices without `cmd`. Returns the package manager's `Success` reply;
    /// any other reply is [`RustADBError::InstallFailed`] carrying it, and a
    /// failed session is abandoned. `timeout` bounds each wait for the device.
    pub(crate) fn install_packages(
        &mut self,
        apks: &[&Path],
        options: &[&str],
        timeout: Duration,
    ) -> Result<String> {
        if apks.is_empty() {
            return Err(RustADBError::ADBRequestFailed(
                "no APK to install".to_string(),
            ));
        }
        if options
            .iter()
            .any(|option| option.is_empty() || option.contains('\0'))
        {
            return Err(RustADBError::ADBRequestFailed(
                "install options must be non-empty and contain no NUL".to_string(),
            ));
        }
        let mut apks = apks
            .iter()
            .map(|path| Apk::open(path))
            .collect::<Result<Vec<_>>>()?;
        if apks.len() == 1 {
            let apk = apks.pop().expect("one APK");
            return self.install_single(apk, options, timeout);
        }
        self.install_multiple(apks, options, timeout)
    }

    fn install_channel(&self, streamed_session: bool) -> InstallChannel {
        let info = self.connection_info();
        if streamed_session && info.has_feature("abb_exec") {
            InstallChannel::Abb
        } else if info.has_feature("cmd") {
            InstallChannel::Cmd
        } else {
            InstallChannel::Pm
        }
    }

    fn install_single(
        &mut self,
        mut apk: Apk,
        options: &[&str],
        timeout: Duration,
    ) -> Result<String> {
        if self.install_channel(false) == InstallChannel::Cmd {
            let size = apk.size.to_string();
            let mut args = vec!["package", "install"];
            args.extend_from_slice(options);
            args.extend(["-S", size.as_str()]);
            let output =
                self.package_command(InstallChannel::Cmd, &args, Some(&mut apk), timeout)?;
            return if succeeded(&output) {
                Ok(output)
            } else {
                Err(failure(&output))
            };
        }

        let remote = temp_path(0);
        let mut args = vec!["install"];
        args.extend_from_slice(options);
        args.push(&remote);
        // The temporary copy goes whatever happens, even after a failed push.
        let result = self
            .push_apk(&mut apk, &remote, timeout)
            .and_then(|()| self.pm(&args, timeout));
        self.remove_remote(&remote, timeout);
        let (output, exit) = result?;
        if pm_succeeded(&output, exit) {
            Ok(output)
        } else {
            Err(failure(&output))
        }
    }

    fn install_multiple(
        &mut self,
        apks: Vec<Apk>,
        options: &[&str],
        timeout: Duration,
    ) -> Result<String> {
        let channel = self.install_channel(true);
        let total = apks.iter().map(|apk| apk.size).sum::<u64>().to_string();
        let mut args = vec!["install-create"];
        args.extend_from_slice(options);
        args.extend(["-S", total.as_str()]);
        let created = self.session_step(channel, &args, None, timeout)?;
        let session = session_id(&created).ok_or_else(|| failure(&created))?;

        match self.write_splits(channel, &session, apks, timeout) {
            Ok(()) => {
                let committed =
                    self.session_step(channel, &["install-commit", &session], None, timeout)?;
                if succeeded(&committed) {
                    Ok(committed)
                } else {
                    Err(failure(&committed))
                }
            }
            Err(error) => {
                // Leave no half-written session behind on the device.
                match self.session_step(channel, &["install-abandon", &session], None, timeout) {
                    Ok(output) if succeeded(&output) => {}
                    Ok(output) => log::warn!("install-abandon answered: {}", output.trim()),
                    Err(abandon) => log::warn!("install-abandon failed: {abandon}"),
                }
                Err(error)
            }
        }
    }

    fn write_splits(
        &mut self,
        channel: InstallChannel,
        session: &str,
        apks: Vec<Apk>,
        timeout: Duration,
    ) -> Result<()> {
        for (index, mut apk) in apks.into_iter().enumerate() {
            let size = apk.size.to_string();
            let name = split_name(index, &apk.name);
            let output = if channel == InstallChannel::Pm {
                let remote = temp_path(index);
                let result = self.push_apk(&mut apk, &remote, timeout).and_then(|()| {
                    self.pm(
                        &["install-write", "-S", &size, session, &name, &remote],
                        timeout,
                    )
                });
                self.remove_remote(&remote, timeout);
                result?.0
            } else {
                self.session_step(
                    channel,
                    &["install-write", "-S", &size, session, &name, "-"],
                    Some(&mut apk),
                    timeout,
                )?
            };
            if !succeeded(&output) {
                return Err(failure(&output));
            }
        }
        Ok(())
    }

    /// One `package` subcommand of an install session on `channel`.
    fn session_step(
        &mut self,
        channel: InstallChannel,
        args: &[&str],
        apk: Option<&mut Apk>,
        timeout: Duration,
    ) -> Result<String> {
        if channel == InstallChannel::Pm {
            return Ok(self.pm(args, timeout)?.0);
        }
        let mut full = vec!["package"];
        full.extend_from_slice(args);
        self.package_command(channel, &full, apk, timeout)
    }

    /// Open `args` as a streamed package-manager command, send `apk` as its
    /// stdin if given, and return what it printed.
    fn package_command(
        &mut self,
        channel: InstallChannel,
        args: &[&str],
        apk: Option<&mut Apk>,
        timeout: Duration,
    ) -> Result<String> {
        let destination = match channel {
            InstallChannel::Abb => format!("abb_exec:{}", args.join("\0")),
            InstallChannel::Cmd => format!("exec:cmd {}", shell_words(args)),
            InstallChannel::Pm => unreachable!("pm is not a streamed channel"),
        };
        let mut stream = self.open_stream_service(destination.as_bytes(), timeout)?;
        if let Some(apk) = apk
            && let Err(error) = copy_apk(apk, &mut stream)
        {
            // The package manager gave up and closed the stream. What it
            // printed (perhaps nothing) is the result; the connection is fine.
            if stream.closed_by_peer_during_write(&error) {
                return stream.read_text_to_close(INSTALL_OUTPUT_LIMIT);
            }
            return Err(error);
        }
        stream.read_text_to_close(INSTALL_OUTPUT_LIMIT)
    }

    /// Run `pm <args>` and return its combined output and exit report.
    fn pm(&mut self, args: &[&str], timeout: Duration) -> Result<(String, ShellExit)> {
        let command = format!("pm {}", shell_words(args));
        let mut output = Vec::new();
        let exit = self.shell_command_with_timeout(&command, Some(&mut output), None, timeout)?;
        Ok((String::from_utf8_lossy(&output).into_owned(), exit))
    }

    fn push_apk(&mut self, apk: &mut Apk, remote: &str, timeout: Duration) -> Result<()> {
        let mut sync = self.open_sync(timeout)?;
        let mut source = (&mut apk.file).take(apk.size);
        sync.send(&mut source, remote, PUSH_FILE_MODE, now_mtime())?;
        sync.finish();
        Ok(())
    }

    /// Best effort, as AOSP's `delete_device_file`.
    fn remove_remote(&mut self, remote: &str, timeout: Duration) {
        let command = format!("rm {} </dev/null", escape_arg(remote));
        if let Err(error) = self.shell_command_with_timeout(&command, None, None, timeout) {
            log::warn!("could not remove the temporary APK: {error}");
        }
    }

    pub(crate) fn uninstall(
        &mut self,
        package_name: &dyn AsRef<str>,
        user: Option<&str>,
    ) -> Result<()> {
        let mut args = vec!["uninstall"];
        if let Some(user) = user {
            args.extend(["--user", user]);
        }
        args.push(package_name.as_ref());
        let tool = if self.connection_info().has_feature("cmd") {
            "cmd package"
        } else {
            "pm"
        };
        let command = format!("{tool} {}", shell_words(&args));
        let mut output = Vec::new();
        let exit = self.shell_command_with_timeout(
            &command,
            Some(&mut output),
            None,
            DEFAULT_READ_TIMEOUT,
        )?;
        let output = String::from_utf8_lossy(&output);
        // Kadb trusts the exit status; without one (legacy shell), AOSP's
        // `Success` reply is the only evidence.
        let removed = match exit {
            ShellExit::Code(code) => code == 0,
            ShellExit::Unreported => succeeded(output.trim_start()),
        };
        if removed {
            log::info!("Package {} successfully uninstalled", package_name.as_ref());
            Ok(())
        } else {
            Err(RustADBError::ADBRequestFailed(format!(
                "uninstall failed: {}",
                output.trim()
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{escape_arg, pm_succeeded, session_id, split_name, succeeded};
    use crate::ShellExit;

    #[test]
    fn arguments_are_quoted_like_aosp() {
        assert_eq!(escape_arg("a b"), "'a b'");
        assert_eq!(escape_arg("it's"), r"'it'\''s'");
    }

    #[test]
    fn streamed_steps_need_a_leading_success() {
        assert!(succeeded("Success\n"));
        assert!(succeeded("Success: streamed 10 bytes\n"));
        assert!(!succeeded("Failure [INSTALL_FAILED_INVALID_APK]\n"));
        assert!(!succeeded(""));
        assert!(!succeeded("Performing Streamed Install\nSuccess\n"));
    }

    #[test]
    fn session_ids_come_from_the_last_brackets_of_a_success() {
        assert_eq!(
            session_id("Success: created install session [1234]\n").as_deref(),
            Some("1234")
        );
        assert_eq!(session_id("Failure [1234]"), None);
        assert_eq!(session_id("Success: created install session [abc]"), None);
        assert_eq!(session_id("Success"), None);
    }

    #[test]
    fn pm_output_is_judged_by_its_lines_and_exit() {
        assert!(pm_succeeded(
            "\tpkg: /data/local/tmp/a.apk\nSuccess\n",
            ShellExit::Unreported
        ));
        assert!(pm_succeeded("Success\n", ShellExit::Code(0)));
        assert!(!pm_succeeded("Success\n", ShellExit::Code(1)));
        assert!(!pm_succeeded(
            "Failure [INSTALL_FAILED_OLDER_SDK]\n",
            ShellExit::Unreported
        ));
        assert!(!pm_succeeded("", ShellExit::Code(0)));
    }

    #[test]
    fn split_names_are_unique_and_shell_safe() {
        assert_eq!(split_name(0, "base.apk"), "0_base.apk");
        assert_eq!(split_name(1, "my split's.apk"), "1_my_split_s.apk");
    }
}

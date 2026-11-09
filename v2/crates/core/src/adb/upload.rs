//! A fail-closed upload transaction. The transport must keep this entire transaction on
//! one verified connection. No step may reconnect by address after a failure.
use super::{AdbError, AdbOutput, AdbResult};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UploadResult {
    Uploaded { verified: bool },
    Exists,
}

/// Synchronous seam so a dedicated transport worker owns the socket for every phase.
pub trait UploadIo {
    fn shell(&mut self, command: &str) -> AdbResult<AdbOutput>;
    fn push(&mut self, local: &Path, remote: &str) -> AdbResult<()>;
}

fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}
fn acknowledged(out: &AdbOutput, marker: &str) -> bool {
    matches!(out.exit_code, None | Some(0)) && out.stdout.lines().any(|line| line == marker)
}
fn cleanup(io: &mut impl UploadIo, stage: &str) -> String {
    // Never recursive, never a target path, and only after mkdir was acknowledged.
    let command = format!(
        "rm -f {}/payload && rmdir {} && echo atvopt-clean",
        quote(stage),
        quote(stage)
    );
    if io
        .shell(&command)
        .is_ok_and(|out| acknowledged(&out, "atvopt-clean"))
    {
        "The temporary copy was removed.".into()
    } else {
        format!("A temporary copy may remain at {stage}.")
    }
}

// Android mksh R59 removed the unsafe stat/nonregular -> O_TRUNC race in older
// releases. Do not infer this guarantee from `set -C` support or Android API level.
// Audited: platform/external/mksh 89154bf75d0361cf87c0586c96b67242091b1e37,
// src/exec.c iosetup. Unknown shells fail closed until separately reviewed.
const SAFE_SHELL: &str = r#"case "${KSH_VERSION-}" in *"MIRBSD KSH R59 "*|*"MIRBSD KSH R59b "*|*"MIRBSD KSH R59c "*) echo atvopt-safe-shell;; *) exit 1;; esac"#;

/// No-clobber uses modern mksh's exclusive-create redirection, not `mv -n` (which
/// can check then rename). A failed copy can leave a NEW partial destination; we
/// disclose that and never delete it blindly. Refuse special files before opening
/// and check the opened descriptor before writing (a raced FIFO may block until
/// cancellation, but receives no bytes). Approved replacement uses `mv -T` so a
/// directory appearing at commit cannot receive the staged file.
pub fn commit_command(partial: &str, target: &str, overwrite: bool) -> String {
    if overwrite {
        format!(
            "mv -fT {} {} && echo atvopt-committed",
            quote(partial),
            quote(target)
        )
    } else {
        format!("{SAFE_SHELL}; {}", exclusive_copy_command(partial, target))
    }
}

fn exclusive_copy_command(partial: &str, target: &str) -> String {
    let target = quote(target);
    format!("if [ -e {target} ] || [ -L {target} ]; then exit 1; fi; ( set -C; {{ [ -f /proc/self/fd/1 ] && cat {}; }} > {target} ) && echo atvopt-committed", quote(partial))
}

pub fn upload(
    io: &mut impl UploadIo,
    local: &Path,
    target: &str,
    expected_size: u64,
    overwrite: bool,
) -> AdbResult<UploadResult> {
    let target =
        crate::commands::files::validate_sdcard_path(target).map_err(AdbError::Transport)?;
    let target = target.as_str();
    let (parent, name) = target
        .rsplit_once('/')
        .ok_or_else(|| AdbError::Transport("Invalid upload target.".into()))?;
    if parent.is_empty() || name.is_empty() || target == "/sdcard" {
        return Err(AdbError::Transport("Pick a file under /sdcard.".into()));
    }
    let target_q = quote(target);
    let check = io.shell(&format!("if [ -d {target_q} ]; then echo atvopt-directory; elif [ -e {target_q} ] || [ -L {target_q} ]; then echo atvopt-exists; else echo atvopt-absent; fi"))?;
    if acknowledged(&check, "atvopt-directory") {
        return Err(AdbError::Transport(format!(
            "A folder named {name} is already in {parent}. Nothing was uploaded for it."
        )));
    }
    if acknowledged(&check, "atvopt-exists") {
        if !overwrite {
            return Ok(UploadResult::Exists);
        }
    } else if !acknowledged(&check, "atvopt-absent") {
        return Err(AdbError::Transport(
            "The TV folder could not be checked. Nothing was uploaded.".into(),
        ));
    }
    if !overwrite {
        let safe = io.shell(SAFE_SHELL)?;
        if !acknowledged(&safe, "atvopt-safe-shell") {
            return Err(AdbError::Transport(
                "Safe upload support couldn't be verified on this TV. Nothing was uploaded.".into(),
            ));
        }
    }
    let stage = format!("{parent}/.atvopt-upload-{}", uuid::Uuid::new_v4());
    let created = io.shell(&format!("mkdir {} && echo atvopt-created", quote(&stage)));
    if !created
        .as_ref()
        .is_ok_and(|out| acknowledged(out, "atvopt-created"))
    {
        // mkdir might have run, but ownership was not acknowledged. Never remove it.
        return Err(AdbError::Transport(format!("Could not confirm a private upload folder. Nothing was uploaded; an empty folder may remain at {stage}.")));
    }
    let partial = format!("{stage}/payload");
    if let Err(error) = io.push(local, &partial) {
        let left = cleanup(io, &stage);
        return Err(AdbError::Transport(format!(
            "The file didn't finish uploading: {error}. Nothing in the folder was replaced. {left}"
        )));
    }
    let verified = match io.shell(&format!("stat -c %s {}", quote(&partial))) {
        Ok(out) if matches!(out.exit_code, None | Some(0)) => match out.stdout.trim().parse::<u64>() {
            Ok(size) if size == expected_size => true,
            Ok(size) => {
                let left = cleanup(io, &stage);
                return Err(AdbError::Transport(format!("The file reached the TV as {size} bytes instead of {expected_size}. Nothing in the folder was replaced. {left}")));
            }
            _ => false,
        },
        // A lost connection is never followed by a commit or a reconnect.
        Err(error) => return Err(AdbError::Transport(format!("Could not check the uploaded copy: {error}. Nothing in the folder was replaced. A temporary copy may remain at {stage}."))),
        _ => false,
    };
    let committed = io.shell(&commit_command(&partial, target, overwrite));
    if !committed
        .as_ref()
        .is_ok_and(|out| acknowledged(out, "atvopt-committed"))
    {
        let left = cleanup(io, &stage);
        return Err(AdbError::Uncertain {
            operation: "upload commit",
            detail: format!("The TV did not confirm putting the file in place. Check {target} before retrying: it may contain the uploaded file{}; replacement status is unknown. {left}", if overwrite { "" } else { " or a new incomplete copy" }),
        });
    }
    // Completed publication is not undone if cleanup fails. Disclose retained staging.
    let left = cleanup(io, &stage);
    if left.starts_with("A temporary") {
        return Err(AdbError::Transport(format!(
            "The file was uploaded to {target}. {left}"
        )));
    }
    Ok(UploadResult::Uploaded { verified })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    fn out(text: &str) -> AdbResult<AdbOutput> {
        Ok(AdbOutput {
            stdout: text.into(),
            stderr: String::new(),
            exit_code: Some(0),
        })
    }
    struct Io {
        replies: VecDeque<AdbResult<AdbOutput>>,
        calls: Vec<String>,
        push_error: bool,
    }
    impl UploadIo for Io {
        fn shell(&mut self, command: &str) -> AdbResult<AdbOutput> {
            if command == SAFE_SHELL {
                return out("atvopt-safe-shell");
            }
            self.calls.push(command.into());
            self.replies.pop_front().expect("unexpected shell call")
        }
        fn push(&mut self, _: &Path, remote: &str) -> AdbResult<()> {
            self.calls.push(format!("push {remote}"));
            if self.push_error {
                Err(AdbError::Transport("lost push ACK".into()))
            } else {
                Ok(())
            }
        }
    }
    fn io(replies: &[&str]) -> Io {
        Io {
            replies: replies.iter().map(|s| out(s)).collect(),
            calls: Vec::new(),
            push_error: false,
        }
    }
    fn run(io: &mut Io, overwrite: bool) -> AdbResult<UploadResult> {
        upload(io, Path::new("local"), "/sdcard/movie", 3, overwrite)
    }
    #[test]
    fn unacknowledged_reservation_never_pushes_or_cleans_an_unowned_path() {
        let mut io = io(&["atvopt-absent", ""]);
        assert!(run(&mut io, false).is_err());
        assert_eq!(io.calls.len(), 2);
        assert!(io.calls[1].starts_with("mkdir '/sdcard/.atvopt-upload-"));
    }
    #[test]
    fn short_upload_cleanup_failure_is_not_claimed_removed() {
        let mut io = io(&["atvopt-absent", "atvopt-created", "2", ""]);
        let error = run(&mut io, false).unwrap_err().to_string();
        assert!(error.contains("temporary copy may remain"));
        assert!(!error.contains("was removed"));
        assert!(!io.calls.iter().any(|c| c.contains("atvopt-committed")));
    }
    #[test]
    fn failed_push_only_cleans_reserved_stage() {
        let mut io = io(&["atvopt-absent", "atvopt-created", "atvopt-clean"]);
        io.push_error = true;
        assert!(run(&mut io, false).is_err());
        let cleanup = io.calls.last().unwrap();
        assert!(cleanup.starts_with("rm -f '/sdcard/.atvopt-upload-"));
        assert!(!cleanup.contains("rm -f '/sdcard/movie'"));
    }
    #[test]
    fn lost_commit_ack_never_claims_no_replacement_or_retries() {
        let mut io = io(&["atvopt-exists", "atvopt-created", "3", "", "atvopt-clean"]);
        let error = run(&mut io, true).unwrap_err();
        assert!(matches!(error, AdbError::Uncertain { .. }));
        assert!(!error
            .to_string()
            .contains("Nothing in the folder was replaced"));
        assert_eq!(io.calls.iter().filter(|c| c.starts_with("mv ")).count(), 1);
    }
    #[test]
    fn stat_connection_loss_never_commits_or_reconnects() {
        let mut io = io(&["atvopt-absent", "atvopt-created"]);
        io.replies
            .push_back(Err(AdbError::Transport("connection lost".into())));
        assert!(run(&mut io, false).is_err());
        assert_eq!(io.calls.len(), 4);
    }
    #[cfg(unix)]
    #[test]
    fn commit_refuses_raced_file_directory_and_symlinks() {
        // Execute the actual production command AFTER the target appears, not a mock
        // that simply repeats a pre-transfer existence check. Android mksh is a separate
        // device gate; this checks POSIX-shell behavior on the host, not TV validation.
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir().unwrap();
        let partial = root.path().join("payload");
        std::fs::write(&partial, b"new").unwrap();
        for kind in ["file", "directory", "symlink", "dangling", "fifo"] {
            let target = root.path().join(kind);
            let victim = root.path().join(format!("victim-{kind}"));
            match kind {
                "file" => std::fs::write(&target, b"existing").unwrap(),
                "directory" => std::fs::create_dir(&target).unwrap(),
                "fifo" => {
                    assert!(std::process::Command::new("mkfifo")
                        .arg(&target)
                        .status()
                        .unwrap()
                        .success());
                }
                "symlink" => {
                    std::fs::write(&victim, b"existing").unwrap();
                    symlink(&victim, &target).unwrap();
                }
                _ => symlink(&victim, &target).unwrap(),
            }
            let result = std::process::Command::new("sh")
                .arg("-c")
                .arg(
                    exclusive_copy_command(partial.to_str().unwrap(), target.to_str().unwrap())
                        .replace("/proc/self/fd/1", "/dev/fd/1"),
                )
                .output()
                .unwrap();
            assert!(!result.status.success(), "{kind}");
            assert!(!String::from_utf8_lossy(&result.stdout).contains("atvopt-committed"));
            if kind == "file" {
                assert_eq!(std::fs::read(&target).unwrap(), b"existing");
            }
            if kind == "directory" {
                assert_eq!(std::fs::read_dir(&target).unwrap().count(), 0);
            }
            if kind == "symlink" {
                assert_eq!(std::fs::read(&victim).unwrap(), b"existing");
            }
            if kind == "dangling" {
                assert!(!victim.exists());
            }
        }
        let target = root.path().join("new");
        let result = std::process::Command::new("sh")
            .arg("-c")
            .arg(
                exclusive_copy_command(partial.to_str().unwrap(), target.to_str().unwrap())
                    .replace("/proc/self/fd/1", "/dev/fd/1"),
            )
            .output()
            .unwrap();
        assert!(result.status.success());
        assert_eq!(std::fs::read(&target).unwrap(), b"new");
    }
    #[cfg(unix)]
    #[test]
    fn unknown_or_old_shell_is_rejected_before_copy() {
        for version in [
            "",
            "bash 5.2",
            "@(#)MIRBSD KSH R58 2020/03/28",
            "MIRBSD KSH R590 2021/01/01",
        ] {
            let result = std::process::Command::new("sh")
                .arg("-c")
                .arg(format!("KSH_VERSION={}; {SAFE_SHELL}", quote(version)))
                .output()
                .unwrap();
            assert!(!result.status.success(), "{version}");
        }
    }
    #[cfg(unix)]
    #[test]
    fn no_clobber_survives_target_created_after_commit_precheck() {
        let root = tempfile::tempdir().unwrap();
        let partial = root.path().join("payload");
        let target = root.path().join("new");
        std::fs::write(&partial, b"new").unwrap();
        let command = exclusive_copy_command(partial.to_str().unwrap(), target.to_str().unwrap())
            .replace(
                "fi; ( set",
                &format!(
                    "fi; printf existing > {}; ( set",
                    quote(target.to_str().unwrap())
                ),
            )
            .replace("/proc/self/fd/1", "/dev/fd/1");
        let result = std::process::Command::new("sh")
            .args(["-c", &command])
            .output()
            .unwrap();
        assert!(!result.status.success());
        assert_eq!(std::fs::read(target).unwrap(), b"existing");
    }
    #[cfg(unix)]
    #[test]
    fn approved_replacement_never_moves_payload_inside_raced_directory() {
        let root = tempfile::tempdir().unwrap();
        let partial = root.path().join("payload");
        let target = root.path().join("new-directory");
        std::fs::write(&partial, b"new").unwrap();
        std::fs::create_dir(&target).unwrap();
        let result = std::process::Command::new("sh")
            .arg("-c")
            .arg(commit_command(
                partial.to_str().unwrap(),
                target.to_str().unwrap(),
                true,
            ))
            .output()
            .unwrap();
        assert!(!result.status.success());
        assert_eq!(std::fs::read_dir(target).unwrap().count(), 0);
        assert_eq!(std::fs::read(partial).unwrap(), b"new");
    }
}

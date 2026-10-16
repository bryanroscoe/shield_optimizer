//! Local client lifetime, not ownership of the shared ADB daemon.

use std::io;
use std::process::{ExitStatus, Output};

use tokio::io::AsyncReadExt;
use tokio::process::{Child, ChildStderr, ChildStdout};

pub trait ChildCompletion: Send + 'static {
    fn reaped(self: Box<Self>);
}

pub struct AdbChild {
    child: Option<Child>,
    completion: Option<Box<dyn ChildCompletion>>,
}

fn notify_successful_wait(
    result: &io::Result<ExitStatus>,
    completion: &mut Option<Box<dyn ChildCompletion>>,
) {
    if result.is_ok() {
        if let Some(completion) = completion.take() {
            completion.reaped();
        }
    }
}

impl AdbChild {
    pub fn new(child: Child, completion: Option<Box<dyn ChildCompletion>>) -> Self {
        Self {
            child: Some(child),
            completion,
        }
    }

    pub fn take_stdout(&mut self) -> Option<ChildStdout> {
        self.child.as_mut().and_then(|child| child.stdout.take())
    }

    pub fn take_stderr(&mut self) -> Option<ChildStderr> {
        self.child.as_mut().and_then(|child| child.stderr.take())
    }

    pub fn start_kill(&mut self) -> io::Result<()> {
        self.child.as_mut().expect("child retained").start_kill()
    }

    pub async fn wait(&mut self) -> io::Result<ExitStatus> {
        let result = self.child.as_mut().expect("child retained").wait().await;
        notify_successful_wait(&result, &mut self.completion);
        result
    }

    pub async fn output(mut self) -> io::Result<Output> {
        let mut stdout = self.take_stdout();
        let mut stderr = self.take_stderr();
        let read_out = async {
            let mut bytes = Vec::new();
            if let Some(pipe) = stdout.as_mut() {
                pipe.read_to_end(&mut bytes).await?;
            }
            Ok::<_, io::Error>(bytes)
        };
        let read_err = async {
            let mut bytes = Vec::new();
            if let Some(pipe) = stderr.as_mut() {
                pipe.read_to_end(&mut bytes).await?;
            }
            Ok::<_, io::Error>(bytes)
        };
        let (status, stdout, stderr) = tokio::try_join!(self.wait(), read_out, read_err)?;
        Ok(Output {
            status,
            stdout,
            stderr,
        })
    }
}

impl Drop for AdbChild {
    fn drop(&mut self) {
        let Some(mut child) = self.child.take() else {
            return;
        };
        let mut completion = self.completion.take();
        if child.try_wait().ok().flatten().is_some() {
            if let Some(completion) = completion {
                completion.reaped();
            }
            return;
        }
        // Caller cancellation transfers ownership to a reaper. It must not
        // release the completion lease merely because kill was requested.
        let _ = child.start_kill();
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                let result = child.wait().await;
                notify_successful_wait(&result, &mut completion);
            });
        }
        // Without a runtime, dropping an unreaped lease deliberately records
        // unknown completion. kill_on_drop is not proof of successful reaping.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    struct ObservedLease(Arc<AtomicUsize>, Arc<AtomicUsize>);
    impl ChildCompletion for ObservedLease {
        fn reaped(self: Box<Self>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    impl Drop for ObservedLease {
        fn drop(&mut self) {
            self.1.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn failed_wait_keeps_lease_and_never_reports_reaped() {
        let reaped = Arc::new(AtomicUsize::new(0));
        let dropped = Arc::new(AtomicUsize::new(0));
        let mut lease: Option<Box<dyn ChildCompletion>> =
            Some(Box::new(ObservedLease(reaped.clone(), dropped.clone())));
        let error = Err(io::Error::other("injected wait failure"));
        notify_successful_wait(&error, &mut lease);
        assert!(lease.is_some());
        assert_eq!(reaped.load(Ordering::SeqCst), 0);
        assert_eq!(dropped.load(Ordering::SeqCst), 0);
        drop(lease);
        assert_eq!(reaped.load(Ordering::SeqCst), 0);
        assert_eq!(dropped.load(Ordering::SeqCst), 1);
    }
}

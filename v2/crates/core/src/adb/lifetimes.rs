//! Admission and leases for ADB work, so a quit-time `adb kill-server` cannot
//! race anything that may still start the server again.
//!
//! Two counters use this. Operations are admitted when a command takes a
//! driver snapshot and stay counted for as long as any clone of that snapshot
//! lives, which covers a whole command, a resident Remote session built from
//! it and that session's teardown. adb client processes are counted from
//! spawn until they have been reaped. Closing a counter refuses new leases
//! without touching the ones already held.

use std::mem::ManuallyDrop;
use std::path::Path;
use std::process::ExitStatus;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use async_trait::async_trait;
use tokio::process::Child;
use tokio::sync::Notify;
use tokio::time::Instant;

use super::driver::{AdbByteStream, AdbDriver, AdbError, AdbOutput, AdbResult};
use super::driver::{BoundedShellOutput, ServerStop};

#[derive(Debug, Default)]
pub struct Leases {
    inner: Mutex<Counts>,
    changed: Notify,
}

#[derive(Debug, Default)]
struct Counts {
    closed: bool,
    active: usize,
}

/// One unit of counted work. Dropping it is the release.
#[derive(Debug)]
pub struct Lease {
    leases: Arc<Leases>,
}

impl Leases {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// A new lease, or `None` once the counter is closed.
    pub fn try_acquire(self: &Arc<Self>) -> Option<Lease> {
        let mut counts = self.counts();
        if counts.closed {
            return None;
        }
        counts.active += 1;
        Some(Lease {
            leases: Arc::clone(self),
        })
    }

    /// Refuse every later lease. Leases already held stay valid.
    pub fn close(&self) {
        self.counts().closed = true;
    }

    pub fn active(&self) -> usize {
        self.counts().active
    }

    /// Wait for every lease to be released, up to `deadline`. `false` means
    /// something still held one when time ran out.
    pub async fn wait_idle(&self, deadline: Instant) -> bool {
        loop {
            let released = self.changed.notified();
            tokio::pin!(released);
            released.as_mut().enable();
            if self.active() == 0 {
                return true;
            }
            if tokio::time::timeout_at(deadline, released).await.is_err() {
                return self.active() == 0;
            }
        }
    }

    // A panic while holding the lock leaves nothing half-updated, so a
    // poisoned lock is still safe to read.
    fn counts(&self) -> MutexGuard<'_, Counts> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        let mut counts = self.leases.counts();
        counts.active -= 1;
        let idle = counts.active == 0;
        drop(counts);
        if idle {
            self.leases.changed.notify_waiters();
        }
    }
}

/// An adb client process, counted until it has been reaped. Dropped before
/// that, it is killed and reaped on the runtime, and the lease goes only once
/// that wait returns. When nothing can confirm the process is gone (no
/// runtime, or the wait failed) the lease is never released, so quit keeps
/// treating the process as alive.
#[derive(Debug)]
pub struct TrackedChild {
    child: Option<Child>,
    lease: Option<Lease>,
}

impl TrackedChild {
    pub fn new(child: Child, lease: Lease) -> Self {
        Self {
            child: Some(child),
            lease: Some(lease),
        }
    }

    /// The process itself, for taking its pipes. Waiting on it directly
    /// leaves the lease to `Drop`; prefer [`TrackedChild::wait`].
    pub fn child_mut(&mut self) -> &mut Child {
        self.child
            .as_mut()
            .expect("the child is only taken by kill_and_reap and drop")
    }

    pub fn start_kill(&mut self) -> std::io::Result<()> {
        self.child_mut().start_kill()
    }

    /// Wait for the process to exit; once it has, it is no longer counted.
    pub async fn wait(&mut self) -> std::io::Result<ExitStatus> {
        let status = self.child_mut().wait().await?;
        self.lease = None;
        Ok(status)
    }

    /// Kill the process and wait for it. Returns whether the wait confirmed
    /// it is gone; if not, it stays counted.
    pub async fn kill_and_reap(mut self) -> bool {
        let Some(mut child) = self.child.take() else {
            return self.lease.is_none();
        };
        let _ = child.start_kill();
        let reaped = child.wait().await.is_ok();
        if reaped {
            self.lease = None;
        }
        reaped
    }
}

impl Drop for TrackedChild {
    fn drop(&mut self) {
        let Some(lease) = self.lease.take() else {
            return;
        };
        let Some(mut child) = self.child.take() else {
            // Cancelled while `kill_and_reap` was waiting: nothing confirmed
            // the process is gone.
            std::mem::forget(lease);
            return;
        };
        let _ = child.start_kill();
        // ManuallyDrop keeps the lease held if the reaping task never finishes.
        let lease = ManuallyDrop::new(lease);
        match tokio::runtime::Handle::try_current() {
            Ok(runtime) => {
                runtime.spawn(async move {
                    if child.wait().await.is_ok() {
                        drop(ManuallyDrop::into_inner(lease));
                    }
                });
            }
            Err(_) => drop(child),
        }
    }
}

/// A driver snapshot that holds an operation lease for as long as any clone
/// of it lives. It forwards every `AdbDriver` method unchanged: a method left
/// to its default here would silently replace the real driver's behavior.
pub(crate) struct LeasedAdb {
    inner: Arc<dyn AdbDriver>,
    _lease: Lease,
}

impl LeasedAdb {
    pub(crate) fn new(inner: Arc<dyn AdbDriver>, lease: Lease) -> Self {
        Self {
            inner,
            _lease: lease,
        }
    }
}

#[async_trait]
impl AdbDriver for LeasedAdb {
    fn operation_leases(&self) -> Option<Arc<Leases>> {
        self.inner.operation_leases()
    }

    fn uses_host_server(&self) -> bool {
        self.inner.uses_host_server()
    }

    async fn discover_network(
        &self,
    ) -> super::driver::AdbResult<super::driver::NativeDiscoveryReport> {
        self.inner.discover_network().await
    }

    async fn scan_connect(
        &self,
        address: &str,
        valid_for: Duration,
    ) -> AdbResult<super::ScanConnectOutcome> {
        self.inner.scan_connect(address, valid_for).await
    }

    async fn scan_connection_progress(&self) -> super::ScanConnectionProgress {
        self.inner.scan_connection_progress().await
    }

    async fn raw(&self, args: &[&str]) -> AdbResult<AdbOutput> {
        self.inner.raw(args).await
    }

    async fn raw_transfer(&self, args: &[&str]) -> AdbResult<AdbOutput> {
        self.inner.raw_transfer(args).await
    }

    async fn pull_limited(
        &self,
        serial: &str,
        remote: &str,
        local: &Path,
        max_bytes: u64,
    ) -> AdbResult<()> {
        self.inner
            .pull_limited(serial, remote, local, max_bytes)
            .await
    }

    async fn shell(&self, serial: &str, command: &str) -> AdbResult<AdbOutput> {
        self.inner.shell(serial, command).await
    }

    async fn shell_bounded(&self, serial: &str, command: &str) -> AdbResult<BoundedShellOutput> {
        self.inner.shell_bounded(serial, command).await
    }

    async fn raw_bytes(&self, args: &[&str]) -> AdbResult<Vec<u8>> {
        self.inner.raw_bytes(args).await
    }

    async fn device_binding(
        &self,
        serial: &str,
    ) -> super::driver::AdbResult<super::driver::AdbDeviceBinding> {
        self.inner.device_binding(serial).await
    }

    async fn open_bound_device_service(
        &self,
        binding: &super::driver::AdbDeviceBinding,
        service: &str,
        lifetime: Duration,
    ) -> super::driver::AdbResult<super::driver::AdbOwnedService> {
        self.inner
            .open_bound_device_service(binding, service, lifetime)
            .await
    }

    async fn open_device_service(
        &self,
        serial: &str,
        service: &str,
    ) -> AdbResult<Box<dyn AdbByteStream>> {
        self.inner.open_device_service(serial, service).await
    }

    async fn spawn(&self, args: &[&str]) -> AdbResult<TrackedChild> {
        self.inner.spawn(args).await
    }

    async fn stop_server(&self, timeout: Duration) -> ServerStop {
        self.inner.stop_server(timeout).await
    }
}

/// What a command gets once quit has closed admission: every call fails
/// before anything is spawned.
pub(crate) struct ClosedAdb;

#[async_trait]
impl AdbDriver for ClosedAdb {
    async fn discover_network(
        &self,
    ) -> super::driver::AdbResult<super::driver::NativeDiscoveryReport> {
        Err(AdbError::ShuttingDown)
    }

    async fn scan_connect(&self, _: &str, _: Duration) -> AdbResult<super::ScanConnectOutcome> {
        Err(AdbError::ShuttingDown)
    }

    async fn raw(&self, _: &[&str]) -> AdbResult<AdbOutput> {
        Err(AdbError::ShuttingDown)
    }

    async fn raw_transfer(&self, _: &[&str]) -> AdbResult<AdbOutput> {
        Err(AdbError::ShuttingDown)
    }

    async fn pull_limited(&self, _: &str, _: &str, _: &Path, _: u64) -> AdbResult<()> {
        Err(AdbError::ShuttingDown)
    }

    async fn shell(&self, _: &str, _: &str) -> AdbResult<AdbOutput> {
        Err(AdbError::ShuttingDown)
    }

    async fn shell_bounded(&self, _: &str, _: &str) -> AdbResult<BoundedShellOutput> {
        Err(AdbError::ShuttingDown)
    }

    async fn raw_bytes(&self, _: &[&str]) -> AdbResult<Vec<u8>> {
        Err(AdbError::ShuttingDown)
    }

    async fn device_binding(
        &self,
        _: &str,
    ) -> super::driver::AdbResult<super::driver::AdbDeviceBinding> {
        Err(AdbError::ShuttingDown)
    }

    async fn open_bound_device_service(
        &self,
        _: &super::driver::AdbDeviceBinding,
        _: &str,
        _: Duration,
    ) -> super::driver::AdbResult<super::driver::AdbOwnedService> {
        Err(AdbError::ShuttingDown)
    }

    async fn open_device_service(&self, _: &str, _: &str) -> AdbResult<Box<dyn AdbByteStream>> {
        Err(AdbError::ShuttingDown)
    }

    async fn spawn(&self, _: &[&str]) -> AdbResult<TrackedChild> {
        Err(AdbError::ShuttingDown)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[test]
    fn a_closed_counter_refuses_new_leases_but_keeps_counting_old_ones() {
        let leases = Leases::new();
        let held = leases.try_acquire().unwrap();
        leases.close();
        assert!(leases.try_acquire().is_none());
        assert_eq!(leases.active(), 1);
        drop(held);
        assert_eq!(leases.active(), 0);
    }

    #[tokio::test]
    async fn waiting_ends_when_the_last_lease_goes() {
        let leases = Leases::new();
        let held = leases.try_acquire().unwrap();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            drop(held);
        });
        assert!(
            leases
                .wait_idle(Instant::now() + Duration::from_secs(10))
                .await
        );
        assert!(leases.wait_idle(Instant::now()).await);
    }

    #[tokio::test]
    async fn waiting_gives_up_at_the_deadline() {
        let leases = Leases::new();
        let _held = leases.try_acquire().unwrap();
        assert!(
            !leases
                .wait_idle(Instant::now() + Duration::from_millis(50))
                .await
        );
    }

    #[cfg(unix)]
    mod processes {
        use super::*;

        fn sleeper() -> Child {
            tokio::process::Command::new("sleep")
                .arg("30")
                .kill_on_drop(true)
                .spawn()
                .unwrap()
        }

        fn alive(pid: u32) -> bool {
            std::process::Command::new("kill")
                .args(["-0", &pid.to_string()])
                .stderr(std::process::Stdio::null())
                .status()
                .unwrap()
                .success()
        }

        #[tokio::test]
        async fn a_dropped_child_is_reaped_before_its_lease_goes() {
            let leases = Leases::new();
            let child = sleeper();
            let pid = child.id().unwrap();
            drop(TrackedChild::new(child, leases.try_acquire().unwrap()));
            let deadline = Instant::now() + Duration::from_secs(5);
            assert!(leases.wait_idle(deadline).await);
            assert!(!alive(pid));
        }

        #[tokio::test]
        async fn kill_and_reap_releases_once_the_process_is_gone() {
            let leases = Leases::new();
            let child = sleeper();
            let pid = child.id().unwrap();
            let tracked = TrackedChild::new(child, leases.try_acquire().unwrap());
            assert!(tracked.kill_and_reap().await);
            assert_eq!(leases.active(), 0);
            assert!(!alive(pid));
        }

        #[tokio::test]
        async fn waiting_out_the_process_releases_it() {
            let leases = Leases::new();
            let child = tokio::process::Command::new("true").spawn().unwrap();
            let mut tracked = TrackedChild::new(child, leases.try_acquire().unwrap());
            assert!(tracked.wait().await.unwrap().success());
            assert_eq!(leases.active(), 0);
        }

        #[test]
        fn a_child_dropped_with_no_runtime_to_reap_it_stays_counted() {
            let leases = Leases::new();
            let child = tokio::runtime::Runtime::new()
                .unwrap()
                .block_on(async { sleeper() });
            drop(TrackedChild::new(child, leases.try_acquire().unwrap()));
            assert_eq!(leases.active(), 1);
        }
    }

    /// Records which method each call arrived through.
    #[derive(Default)]
    struct Recorder(Mutex<Vec<&'static str>>);

    impl Recorder {
        fn note(&self, method: &'static str) {
            self.0.lock().unwrap().push(method);
        }
    }

    fn empty() -> AdbOutput {
        AdbOutput {
            stdout: String::new(),
            stderr: String::new(),
            exit_code: Some(0),
        }
    }

    #[async_trait]
    impl AdbDriver for Recorder {
        async fn discover_network(&self) -> AdbResult<super::super::NativeDiscoveryReport> {
            self.note("discover_network");
            Ok(super::super::NativeDiscoveryReport {
                advertisements: Vec::new(),
                warnings: Vec::new(),
                observation_max_age_ms: 30_000,
            })
        }

        async fn scan_connect(
            &self,
            _: &str,
            _: Duration,
        ) -> AdbResult<super::super::ScanConnectOutcome> {
            self.note("scan_connect");
            Ok(super::super::ScanConnectOutcome::Pending)
        }
        async fn scan_connection_progress(&self) -> super::super::ScanConnectionProgress {
            self.note("scan_connection_progress");
            super::super::ScanConnectionProgress::default()
        }

        async fn raw(&self, _: &[&str]) -> AdbResult<AdbOutput> {
            self.note("raw");
            Ok(empty())
        }
        async fn raw_transfer(&self, _: &[&str]) -> AdbResult<AdbOutput> {
            self.note("raw_transfer");
            Ok(empty())
        }
        async fn pull_limited(&self, _: &str, _: &str, _: &Path, _: u64) -> AdbResult<()> {
            self.note("pull_limited");
            Ok(())
        }
        async fn shell(&self, _: &str, _: &str) -> AdbResult<AdbOutput> {
            self.note("shell");
            Ok(empty())
        }
        async fn shell_bounded(&self, _: &str, _: &str) -> AdbResult<BoundedShellOutput> {
            self.note("shell_bounded");
            Err(AdbError::Transport("recorded".into()))
        }
        async fn raw_bytes(&self, _: &[&str]) -> AdbResult<Vec<u8>> {
            self.note("raw_bytes");
            Ok(Vec::new())
        }
        async fn device_binding(&self, serial: &str) -> AdbResult<super::super::AdbDeviceBinding> {
            self.note("device_binding");
            Ok(super::super::AdbDeviceBinding {
                serial: serial.into(),
                hardware_id: "fixture".into(),
            })
        }
        async fn open_bound_device_service(
            &self,
            _: &super::super::AdbDeviceBinding,
            _: &str,
            _: Duration,
        ) -> AdbResult<super::super::AdbOwnedService> {
            self.note("open_bound_device_service");
            Err(AdbError::Transport("recorded".into()))
        }
        async fn open_device_service(&self, _: &str, _: &str) -> AdbResult<Box<dyn AdbByteStream>> {
            self.note("open_device_service");
            Err(AdbError::Transport("recorded".into()))
        }
        async fn spawn(&self, _: &[&str]) -> AdbResult<TrackedChild> {
            self.note("spawn");
            Err(AdbError::Transport("recorded".into()))
        }
        async fn stop_server(&self, _: Duration) -> ServerStop {
            self.note("stop_server");
            ServerStop::Stopped
        }
    }

    async fn call_every_method(driver: &dyn AdbDriver) {
        let path = Path::new("x");
        let _ = driver.discover_network().await;
        let _ = driver.scan_connect("s", Duration::from_secs(1)).await;
        let _ = driver.scan_connection_progress().await;
        let _ = driver.raw(&[]).await;
        let _ = driver.raw_transfer(&[]).await;
        let _ = driver.pull_limited("s", "r", path, 1).await;
        let _ = driver.shell("s", "c").await;
        let _ = driver.shell_bounded("s", "c").await;
        let _ = driver.raw_bytes(&[]).await;
        let binding = driver.device_binding("s").await.unwrap();
        let _ = driver
            .open_bound_device_service(&binding, "svc", Duration::from_secs(1))
            .await;
        let _ = driver.open_device_service("s", "svc").await;
        let _ = driver.spawn(&[]).await;
        let _ = driver.stop_server(Duration::from_secs(1)).await;
    }

    #[tokio::test]
    async fn a_leased_snapshot_forwards_every_method_to_its_own_counterpart() {
        let recorder = Arc::new(Recorder::default());
        let leased = LeasedAdb::new(recorder.clone(), Leases::new().try_acquire().unwrap());
        call_every_method(&leased).await;
        assert_eq!(
            *recorder.0.lock().unwrap(),
            [
                "discover_network",
                "scan_connect",
                "scan_connection_progress",
                "raw",
                "raw_transfer",
                "pull_limited",
                "shell",
                "shell_bounded",
                "raw_bytes",
                "device_binding",
                "open_bound_device_service",
                "open_device_service",
                "spawn",
                "stop_server",
            ]
        );
    }

    #[tokio::test]
    async fn a_closed_snapshot_refuses_every_spawning_method() {
        let closed = ClosedAdb;
        assert!(matches!(
            closed.scan_connect("s", Duration::from_secs(1)).await,
            Err(AdbError::ShuttingDown)
        ));
        assert_eq!(
            closed.scan_connection_progress().await,
            super::super::ScanConnectionProgress::default()
        );
        let binding = super::super::AdbDeviceBinding {
            serial: "s".into(),
            hardware_id: "fixture".into(),
        };
        assert!(matches!(
            closed.device_binding("s").await,
            Err(AdbError::ShuttingDown)
        ));
        assert!(matches!(
            closed
                .open_bound_device_service(&binding, "svc", Duration::from_secs(1))
                .await,
            Err(AdbError::ShuttingDown)
        ));
        assert!(matches!(
            closed.discover_network().await,
            Err(AdbError::ShuttingDown)
        ));
        let path = Path::new("x");
        assert!(matches!(closed.raw(&[]).await, Err(AdbError::ShuttingDown)));
        assert!(matches!(
            closed.raw_transfer(&[]).await,
            Err(AdbError::ShuttingDown)
        ));
        assert!(matches!(
            closed.pull_limited("s", "r", path, 1).await,
            Err(AdbError::ShuttingDown)
        ));
        assert!(matches!(
            closed.shell("s", "c").await,
            Err(AdbError::ShuttingDown)
        ));
        assert!(matches!(
            closed.shell_bounded("s", "c").await,
            Err(AdbError::ShuttingDown)
        ));
        assert!(matches!(
            closed.raw_bytes(&[]).await,
            Err(AdbError::ShuttingDown)
        ));
        assert!(matches!(
            closed.open_device_service("s", "svc").await,
            Err(AdbError::ShuttingDown)
        ));
        assert!(matches!(
            closed.spawn(&[]).await,
            Err(AdbError::ShuttingDown)
        ));
    }
}

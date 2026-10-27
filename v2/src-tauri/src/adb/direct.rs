//! Opt-in network transport using the same sessions as the Android app.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use shield_direct_adb::WirelessAdb;
use shield_optimizer_core::adb::{
    AdbByteStream, AdbDriver, AdbError, AdbOutput, AdbResult, Leases, ServerStop,
};
use shield_optimizer_core::commands::devices::normalize_connect_address;
use tokio::sync::Mutex;

const MAX_SESSIONS: usize = 32;

#[async_trait]
trait NetworkSession: AdbDriver {
    async fn connect(&self, host: &str, port: u16, request: u64) -> Result<String, String>;
    async fn pair(&self, host: &str, port: u16, code: &str) -> Result<String, String>;
    async fn ready(&self) -> bool;
    fn invalidate(&self, request: u64);
    fn close_idle(&self) -> Result<(), String>;
}

#[async_trait]
impl NetworkSession for WirelessAdb {
    async fn connect(&self, host: &str, port: u16, request: u64) -> Result<String, String> {
        self.begin_request(request)?;
        WirelessAdb::connect(self, host, port, request).await
    }
    async fn pair(&self, host: &str, port: u16, code: &str) -> Result<String, String> {
        WirelessAdb::pair(self, host, port, code).await
    }
    async fn ready(&self) -> bool {
        self.status().await.connected
    }
    fn invalidate(&self, request: u64) {
        let _ = self.begin_request(request);
    }
    fn close_idle(&self) -> Result<(), String> {
        self.close_idle_session()
    }
}

type Session = Arc<dyn NetworkSession>;
struct Slot {
    session: Session,
    published: Arc<AtomicBool>,
}

type Factory = dyn Fn(PathBuf, Arc<Leases>) -> Session + Send + Sync;

pub struct DirectNetworkAdb {
    key: PathBuf,
    sessions: Mutex<BTreeMap<String, Slot>>,
    requests: Arc<AtomicU64>,
    factory: Arc<Factory>,
    operations: Arc<Leases>,
    lifecycle: Mutex<()>,
}

impl DirectNetworkAdb {
    pub fn new(key: PathBuf) -> Self {
        Self::with_factory(
            key,
            Arc::new(|key, operations| Arc::new(WirelessAdb::with_existing_key(key, operations))),
        )
    }

    fn with_factory(key: PathBuf, factory: Arc<Factory>) -> Self {
        Self {
            key,
            sessions: Mutex::new(BTreeMap::new()),
            requests: Arc::new(AtomicU64::new(0)),
            factory,
            operations: Leases::new(),
            lifecycle: Mutex::new(()),
        }
    }

    async fn session(&self, serial: &str) -> AdbResult<Session> {
        self.sessions
            .lock()
            .await
            .get(serial)
            .filter(|slot| slot.published.load(Ordering::Acquire))
            .map(|slot| slot.session.clone())
            .ok_or_else(|| {
                AdbError::Transport(format!(
                    "Direct ADB is not connected to {serial}. Use Add by IP."
                ))
            })
    }

    async fn connect(&self, address: &str) -> AdbResult<AdbOutput> {
        let _lifecycle = self.lifecycle.lock().await;
        let serial = normalize_connect_address(address).map_err(AdbError::Transport)?;
        let endpoint: std::net::SocketAddr = serial.parse().map_err(|_| {
            AdbError::Transport("Direct ADB requires a numeric IP address and port.".into())
        })?;
        let (session, published) = {
            let mut sessions = self.sessions.lock().await;
            sessions.retain(|_, slot| slot.published.load(Ordering::Acquire));
            if !sessions.contains_key(&serial) && sessions.len() >= MAX_SESSIONS {
                return Err(AdbError::Transport(
                    "Direct ADB session limit reached. Use Forget on a TV row to disconnect it first.".into(),
                ));
            }
            let slot = sessions.entry(serial.clone()).or_insert_with(|| Slot {
                session: (self.factory)(self.key.clone(), self.operations.clone()),
                published: Arc::new(AtomicBool::new(false)),
            });
            slot.published.store(false, Ordering::Release);
            (slot.session.clone(), slot.published.clone())
        };
        let request = self.requests.fetch_add(1, Ordering::Relaxed) + 1;
        let mut pending = PendingConnect {
            session: session.clone(),
            requests: self.requests.clone(),
            published: published.clone(),
            complete: false,
        };
        let result = session
            .connect(serial.rsplit_once(':').unwrap().0, endpoint.port(), request)
            .await;
        if let Err(message) = result {
            if !session.ready().await {
                self.sessions.lock().await.remove(&serial);
            }
            return Err(AdbError::Transport(message));
        }
        if !session.ready().await {
            self.sessions.lock().await.remove(&serial);
            return Err(AdbError::Transport(format!(
                "Direct ADB did not confirm {serial} is ready."
            )));
        }
        published.store(true, Ordering::Release);
        pending.complete = true;
        Ok(output(format!("connected to {serial}")))
    }

    async fn devices(&self) -> AdbResult<AdbOutput> {
        let sessions: Vec<_> = self
            .sessions
            .lock()
            .await
            .iter()
            .filter(|(_, slot)| slot.published.load(Ordering::Acquire))
            .map(|(key, slot)| (key.clone(), slot.session.clone()))
            .collect();
        let mut pending = sessions.into_iter();
        let mut probes = tokio::task::JoinSet::new();
        for _ in 0..4 {
            if let Some(entry) = pending.next() {
                probes.spawn(ready_entry(entry));
            }
        }
        let mut ready = Vec::new();
        while let Some(probed) = probes.join_next().await {
            if let Ok(line) = probed {
                ready.push(line);
            }
            if let Some(entry) = pending.next() {
                probes.spawn(ready_entry(entry));
            }
        }
        ready.sort();
        let mut stdout = "List of devices attached\n".to_string();
        for line in ready {
            stdout.push_str(&line);
        }
        Ok(output(stdout))
    }
}

async fn ready_entry((serial, session): (String, Session)) -> String {
    let status = if session.ready().await {
        "device"
    } else {
        "offline"
    };
    format!("{serial}\t{status}\n")
}

struct PendingConnect {
    session: Session,
    requests: Arc<AtomicU64>,
    complete: bool,
    published: Arc<AtomicBool>,
}

impl Drop for PendingConnect {
    fn drop(&mut self) {
        if !self.complete {
            self.published.store(false, Ordering::Release);
            self.session
                .invalidate(self.requests.fetch_add(1, Ordering::Relaxed) + 1);
        }
    }
}

fn output(stdout: String) -> AdbOutput {
    AdbOutput {
        stdout,
        stderr: String::new(),
        exit_code: Some(0),
    }
}

fn unsupported() -> AdbError {
    AdbError::Transport("This operation is unavailable in experimental direct ADB mode. Relaunch with SHIELD_OPTIMIZER_TRANSPORT=subprocess to use platform-tools; no automatic fallback was performed.".into())
}

#[async_trait]
impl AdbDriver for DirectNetworkAdb {
    fn operation_leases(&self) -> Option<Arc<Leases>> {
        Some(self.operations.clone())
    }

    fn uses_host_server(&self) -> bool {
        false
    }

    async fn stop_server(&self, _timeout: std::time::Duration) -> ServerStop {
        self.operations.close();
        if self.operations.active() != 0 {
            return ServerStop::Skipped(
                "Direct ADB work is still running; owned sockets were left alone.".into(),
            );
        }
        let mut sessions = self.sessions.lock().await;
        for slot in sessions.values() {
            if let Err(message) = slot.session.close_idle() {
                return ServerStop::Failed(message);
            }
        }
        sessions.clear();
        ServerStop::Skipped("Closed direct ADB sockets; there is no host server to stop.".into())
    }

    async fn raw(&self, args: &[&str]) -> AdbResult<AdbOutput> {
        match args {
            ["devices"] | ["devices", "-l"] => self.devices().await,
            ["version"] => Ok(output(
                "Experimental direct ADB (shared Android transport); no host adb server".into(),
            )),
            ["connect", address] => self.connect(address).await,
            ["pair", address, code] => {
                let endpoint: std::net::SocketAddr = address.parse().map_err(|_| {
                    AdbError::Transport("Enter the numeric IP and pairing port from the TV.".into())
                })?;
                let session = (self.factory)(self.key.clone(), self.operations.clone());
                session
                    .pair(address.rsplit_once(':').unwrap().0, endpoint.port(), code)
                    .await
                    .map_err(AdbError::Transport)?;
                Ok(output(format!("Successfully paired to {address}")))
            }
            ["disconnect", serial] => {
                let _lifecycle = self.lifecycle.lock().await;
                let session = self.session(serial).await?;
                session.raw(&["disconnect", serial]).await?;
                self.sessions.lock().await.remove(*serial);
                Ok(output(format!("disconnected {serial}")))
            }
            ["-s", serial, "get-state"] => {
                if self.session(serial).await?.ready().await {
                    Ok(output("device\n".into()))
                } else {
                    Err(AdbError::DeviceNotReady {
                        serial: (*serial).into(),
                        state: "offline".into(),
                    })
                }
            }
            ["-s", serial, "reboot", ..] => self.session(serial).await?.raw(args).await,
            _ => Err(unsupported()),
        }
    }

    async fn shell_bounded(
        &self,
        serial: &str,
        command: &str,
    ) -> AdbResult<shield_optimizer_core::adb::driver::BoundedShellOutput> {
        self.session(serial)
            .await?
            .shell_bounded(serial, command)
            .await
    }

    async fn shell(&self, serial: &str, command: &str) -> AdbResult<AdbOutput> {
        self.session(serial).await?.shell(serial, command).await
    }

    async fn raw_transfer(&self, args: &[&str]) -> AdbResult<AdbOutput> {
        match args {
            ["-s", serial, "install", path] | ["-s", serial, "install", "-r", path]
                if !path.is_empty() && !path.starts_with('-') =>
            {
                self.session(serial).await?.raw_transfer(args).await
            }
            ["-s", serial, "pull" | "push", ..] => {
                self.session(serial).await?.raw_transfer(args).await
            }
            _ => Err(unsupported()),
        }
    }

    async fn pull_limited(
        &self,
        serial: &str,
        remote: &str,
        local: &Path,
        max_bytes: u64,
    ) -> AdbResult<()> {
        self.session(serial)
            .await?
            .pull_limited(serial, remote, local, max_bytes)
            .await
    }

    async fn raw_bytes(&self, args: &[&str]) -> AdbResult<Vec<u8>> {
        match args {
            ["-s", serial, ..] => self.session(serial).await?.raw_bytes(args).await,
            _ => Err(unsupported()),
        }
    }

    async fn open_device_service(
        &self,
        serial: &str,
        service: &str,
    ) -> AdbResult<Box<dyn AdbByteStream>> {
        self.session(serial)
            .await?
            .open_device_service(serial, service)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;
    use std::sync::Mutex as StdMutex;

    #[derive(Default)]
    struct FakeSession {
        serial: Arc<StdMutex<String>>,
        ready: Arc<AtomicBool>,
        generation: Arc<AtomicU64>,
        calls: StdMutex<Vec<String>>,
        hold: AtomicBool,
        probe_failed: AtomicBool,
        started: Arc<tokio::sync::Notify>,
        release: Arc<tokio::sync::Notify>,
        completed: Arc<tokio::sync::Notify>,
    }
    #[async_trait]
    impl AdbDriver for FakeSession {
        async fn raw(&self, args: &[&str]) -> AdbResult<AdbOutput> {
            self.calls.lock().unwrap().push(args.join(" "));
            if args.first() == Some(&"-s") {
                assert_eq!(args[1], *self.serial.lock().unwrap());
            }
            if args.first() == Some(&"disconnect") {
                self.ready.store(false, Ordering::Release);
            }
            Ok(output("ok".into()))
        }
        async fn shell_bounded(
            &self,
            serial: &str,
            command: &str,
        ) -> AdbResult<shield_optimizer_core::adb::BoundedShellOutput> {
            assert_eq!(*self.serial.lock().unwrap(), serial);
            self.calls
                .lock()
                .unwrap()
                .push(format!("bounded {serial} {command}"));
            Ok(shield_optimizer_core::adb::BoundedShellOutput {
                stdout: "bounded stdout".into(),
                stderr: "bounded stderr".into(),
                exit_code: Some(7),
                termination: shield_optimizer_core::adb::ShellTermination::Completed,
            })
        }
        async fn shell(&self, serial: &str, command: &str) -> AdbResult<AdbOutput> {
            assert_eq!(*self.serial.lock().unwrap(), serial);
            self.calls
                .lock()
                .unwrap()
                .push(format!("{serial} {command}"));
            if command == "hold" {
                self.started.notify_one();
                self.release.notified().await;
            }
            Ok(output(format!("{serial}: {command}")))
        }
        async fn raw_bytes(&self, args: &[&str]) -> AdbResult<Vec<u8>> {
            assert_eq!(args[1], *self.serial.lock().unwrap());
            Ok(vec![0, 255, 128, 1])
        }
    }
    #[async_trait]
    impl NetworkSession for FakeSession {
        async fn connect(&self, host: &str, port: u16, request: u64) -> Result<String, String> {
            self.generation.store(request, Ordering::Release);
            if self.hold.load(Ordering::Acquire) {
                let (generation, started, release, completed, ready, serial) = (
                    self.generation.clone(),
                    self.started.clone(),
                    self.release.clone(),
                    self.completed.clone(),
                    self.ready.clone(),
                    self.serial.clone(),
                );
                let target = format!("{host}:{port}");
                return tokio::spawn(async move {
                    started.notify_one();
                    release.notified().await;
                    let result = if generation.load(Ordering::Acquire) != request {
                        Err("canceled".into())
                    } else {
                        *serial.lock().unwrap() = target;
                        ready.store(true, Ordering::Release);
                        Ok("connected".into())
                    };
                    completed.notify_one();
                    result
                })
                .await
                .map_err(|e| e.to_string())?;
            }
            if self.generation.load(Ordering::Acquire) != request {
                return Err("canceled".into());
            }
            *self.serial.lock().unwrap() = format!("{host}:{port}");
            self.ready.store(true, Ordering::Release);
            Ok("connected".into())
        }
        async fn pair(&self, host: &str, port: u16, _code: &str) -> Result<String, String> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("pair {host}:{port}"));
            Ok("Paired with the TV.".into())
        }
        async fn ready(&self) -> bool {
            self.ready.load(Ordering::Acquire) && !self.probe_failed.load(Ordering::Acquire)
        }
        fn invalidate(&self, request: u64) {
            self.generation.store(request, Ordering::Release);
        }
        fn close_idle(&self) -> Result<(), String> {
            self.ready.store(false, Ordering::Release);
            self.calls.lock().unwrap().push("close owned socket".into());
            Ok(())
        }
    }

    fn fake_driver() -> (DirectNetworkAdb, Arc<StdMutex<Vec<Arc<FakeSession>>>>) {
        let sessions = Arc::new(StdMutex::new(Vec::new()));
        let captured = sessions.clone();
        let driver = DirectNetworkAdb::with_factory(
            PathBuf::from("not-created-test-key"),
            Arc::new(move |_, _| {
                let session = Arc::new(FakeSession::default());
                captured.lock().unwrap().push(session.clone());
                session
            }),
        );
        (driver, sessions)
    }

    #[tokio::test]
    async fn two_tv_operations_keep_exact_targets_and_binary_bytes() {
        let (driver, sessions) = fake_driver();
        driver.raw(&["connect", "192.0.2.1:5555"]).await.unwrap();
        driver.raw(&["connect", "192.0.2.2:5555"]).await.unwrap();
        let (a, b) = tokio::join!(
            driver.shell("192.0.2.1:5555", "echo a"),
            driver.shell("192.0.2.2:5555", "echo b")
        );
        assert_eq!(a.unwrap().stdout, "192.0.2.1:5555: echo a");
        assert_eq!(b.unwrap().stdout, "192.0.2.2:5555: echo b");
        assert!(driver.shell("192.0.2.3:5555", "id").await.is_err());
        assert_eq!(
            driver
                .raw_bytes(&["-s", "192.0.2.2:5555", "exec-out", "screencap", "-p"])
                .await
                .unwrap(),
            vec![0, 255, 128, 1]
        );
        assert_eq!(sessions.lock().unwrap().len(), 2);
        let devices = driver.raw(&["devices"]).await.unwrap().stdout;
        assert!(devices.contains("192.0.2.1:5555\tdevice"));
        assert!(devices.contains("192.0.2.2:5555\tdevice"));
        driver.raw(&["disconnect", "192.0.2.1:5555"]).await.unwrap();
        assert!(!driver
            .raw(&["devices"])
            .await
            .unwrap()
            .stdout
            .contains("192.0.2.1"));
        assert!(driver.shell("192.0.2.2:5555", "still here").await.is_ok());
    }

    #[tokio::test]
    async fn pairing_never_publishes_an_unconnected_tv() {
        let (driver, _) = fake_driver();
        let paired = driver
            .raw(&["pair", "192.0.2.1:40000", "123456"])
            .await
            .unwrap();
        assert!(paired.stdout.to_lowercase().contains("successfully paired"));
        assert_eq!(
            driver.raw(&["devices"]).await.unwrap().stdout,
            "List of devices attached\n"
        );
        assert!(!driver.uses_host_server());
        for args in [
            vec!["kill-server"],
            vec!["start-server"],
            vec!["mdns", "services"],
            vec!["-s", "192.0.2.1:5555", "forward", "tcp:1", "tcp:2"],
        ] {
            assert!(driver
                .raw(&args)
                .await
                .unwrap_err()
                .to_string()
                .contains("no automatic fallback"));
        }
    }

    #[tokio::test]
    async fn native_install_delegates_only_supported_forms_to_the_selected_session() {
        let (driver, sessions) = fake_driver();
        driver.raw(&["connect", "192.0.2.1:5555"]).await.unwrap();
        driver.raw(&["connect", "192.0.2.2:5555"]).await.unwrap();
        for args in [
            vec!["-s", "192.0.2.2:5555", "install", "simulated input.apk"],
            vec![
                "-s",
                "192.0.2.2:5555",
                "install",
                "-r",
                "simulated input.apk",
            ],
        ] {
            driver.raw_transfer(&args).await.unwrap();
        }
        assert!(sessions.lock().unwrap()[0].calls.lock().unwrap().is_empty());
        assert_eq!(
            *sessions.lock().unwrap()[1].calls.lock().unwrap(),
            vec![
                "-s 192.0.2.2:5555 install simulated input.apk",
                "-s 192.0.2.2:5555 install -r simulated input.apk",
            ]
        );
    }

    #[tokio::test]
    async fn native_install_rejects_unsupported_forms_before_session_io() {
        let (driver, sessions) = fake_driver();
        driver.raw(&["connect", "192.0.2.1:5555"]).await.unwrap();
        for suffix in [
            vec!["install", "-d", "fixture.apk"],
            vec!["install", "-g", "fixture.apk"],
            vec!["install", "-r", "-d", "fixture.apk"],
            vec!["install", "-r"],
            vec!["install", "-r", "-g"],
            vec!["install", ""],
            vec!["install-multiple", "fixture.apk"],
        ] {
            let mut args = vec!["-s", "192.0.2.1:5555"];
            args.extend(suffix);
            assert!(driver
                .raw_transfer(&args)
                .await
                .unwrap_err()
                .to_string()
                .contains("no automatic fallback"));
        }
        assert!(sessions.lock().unwrap()[0].calls.lock().unwrap().is_empty());
        assert!(driver
            .raw_transfer(&["-s", "192.0.2.2:5555", "install", "fixture.apk"])
            .await
            .is_err());
    }

    #[tokio::test]
    async fn bounded_shell_delegates_to_the_selected_session() {
        let (driver, sessions) = fake_driver();
        driver.raw(&["connect", "192.0.2.1:5555"]).await.unwrap();
        let output = driver
            .shell_bounded("192.0.2.1:5555", "fixture")
            .await
            .unwrap();
        assert_eq!(output.stdout, "bounded stdout");
        assert_eq!(output.stderr, "bounded stderr");
        assert_eq!(output.exit_code, Some(7));
        assert!(sessions.lock().unwrap()[0]
            .calls
            .lock()
            .unwrap()
            .iter()
            .any(|call| call == "bounded 192.0.2.1:5555 fixture"));
        assert!(driver
            .shell_bounded("192.0.2.2:5555", "fixture")
            .await
            .is_err());
    }

    #[tokio::test]
    async fn canceled_connect_cannot_publish_late_handshake() {
        let session = Arc::new(FakeSession::default());
        session.hold.store(true, Ordering::Release);
        let captured = session.clone();
        let driver = Arc::new(DirectNetworkAdb::with_factory(
            PathBuf::from("not-created"),
            Arc::new(move |_, _| captured.clone()),
        ));
        let pending_driver = driver.clone();
        let pending =
            tokio::spawn(async move { pending_driver.raw(&["connect", "192.0.2.1:5555"]).await });
        session.started.notified().await;
        pending.abort();
        assert!(pending.await.unwrap_err().is_cancelled());
        session.release.notify_one();
        session.completed.notified().await;
        assert!(!session.ready.load(Ordering::Acquire));
        assert_eq!(
            driver.raw(&["devices"]).await.unwrap().stdout,
            "List of devices attached\n"
        );
        assert!(driver.shell("192.0.2.1:5555", "id").await.is_err());
        session.hold.store(false, Ordering::Release);
        driver.raw(&["connect", "192.0.2.1:5555"]).await.unwrap();
        assert_eq!(
            driver
                .raw(&["-s", "192.0.2.1:5555", "get-state"])
                .await
                .unwrap()
                .stdout,
            "device\n"
        );
    }
    #[tokio::test]
    async fn abandoned_attempts_cannot_exhaust_the_session_limit() {
        let created = Arc::new(StdMutex::new(Vec::<Arc<FakeSession>>::new()));
        let captured = created.clone();
        let hold = Arc::new(AtomicBool::new(true));
        let captured_hold = hold.clone();
        let driver = Arc::new(DirectNetworkAdb::with_factory(
            PathBuf::from("not-created"),
            Arc::new(move |_, _| {
                let session = Arc::new(FakeSession::default());
                session
                    .hold
                    .store(captured_hold.load(Ordering::Acquire), Ordering::Release);
                captured.lock().unwrap().push(session.clone());
                session
            }),
        ));
        for port in 1..=(MAX_SESSIONS as u16 + 2) {
            let pending_driver = driver.clone();
            let target = format!("192.0.2.1:{port}");
            let pending =
                tokio::spawn(async move { pending_driver.raw(&["connect", &target]).await });
            let session = loop {
                if let Some(session) = created.lock().unwrap().get(usize::from(port - 1)).cloned() {
                    break session;
                }
                tokio::task::yield_now().await;
            };
            session.started.notified().await;
            pending.abort();
            assert!(pending.await.unwrap_err().is_cancelled());
            session.release.notify_one();
            session.completed.notified().await;
        }
        hold.store(false, Ordering::Release);
        driver.raw(&["connect", "192.0.2.2:5555"]).await.unwrap();
        assert_eq!(driver.sessions.lock().await.len(), 1);
    }

    #[tokio::test]
    async fn unconfirmed_handshakes_release_their_slot() {
        let fail_probe = Arc::new(AtomicBool::new(true));
        let captured = fail_probe.clone();
        let driver = DirectNetworkAdb::with_factory(
            PathBuf::from("not-created"),
            Arc::new(move |_, _| {
                let session = Arc::new(FakeSession::default());
                session
                    .probe_failed
                    .store(captured.load(Ordering::Acquire), Ordering::Release);
                session
            }),
        );
        for port in 1..=(MAX_SESSIONS as u16 + 2) {
            assert!(driver
                .raw(&["connect", &format!("192.0.2.1:{port}")])
                .await
                .is_err());
        }
        fail_probe.store(false, Ordering::Release);
        driver.raw(&["connect", "192.0.2.2:5555"]).await.unwrap();
        assert_eq!(driver.sessions.lock().await.len(), 1);
    }
    #[tokio::test]
    async fn quit_closes_only_owned_sessions_after_work_drains() {
        let (driver, sessions) = fake_driver();
        driver.raw(&["connect", "192.0.2.1:5555"]).await.unwrap();
        let lease = driver.operations.try_acquire().unwrap();
        assert!(
            matches!(driver.stop_server(std::time::Duration::ZERO).await, ServerStop::Skipped(message) if message.contains("still running"))
        );
        assert!(sessions.lock().unwrap()[0].ready.load(Ordering::Acquire));
        drop(lease);
        assert!(
            matches!(driver.stop_server(std::time::Duration::ZERO).await, ServerStop::Skipped(message) if message.contains("no host server"))
        );
        assert!(driver.sessions.lock().await.is_empty());
        assert_eq!(
            *sessions.lock().unwrap()[0].calls.lock().unwrap(),
            vec!["close owned socket"]
        );
        assert!(driver.operations.try_acquire().is_none());
    }
    #[tokio::test]
    async fn ipv6_pool_and_session_keep_the_same_exact_listed_serial() {
        let (driver, _) = fake_driver();
        for serial in ["[2001:0DB8:0:0:0:0:0:1]:5555", "[2001:db8::1]:5555"] {
            driver.raw(&["connect", serial]).await.unwrap();
            assert!(driver
                .raw(&["devices"])
                .await
                .unwrap()
                .stdout
                .contains(&format!("{serial}\tdevice")));
            assert_eq!(
                driver
                    .raw(&["-s", serial, "get-state"])
                    .await
                    .unwrap()
                    .stdout,
                "device\n"
            );
            assert_eq!(
                driver.shell(serial, "echo exact").await.unwrap().stdout,
                format!("{serial}: echo exact")
            );
        }
        assert_eq!(driver.sessions.lock().await.len(), 2);
    }
    #[tokio::test]
    async fn a_slow_tv_does_not_block_another_tv_command() {
        let (driver, sessions) = fake_driver();
        let driver = Arc::new(driver);
        driver.raw(&["connect", "192.0.2.1:5555"]).await.unwrap();
        driver.raw(&["connect", "192.0.2.2:5555"]).await.unwrap();
        let first = sessions.lock().unwrap()[0].clone();
        let pending_driver = driver.clone();
        let pending =
            tokio::spawn(async move { pending_driver.shell("192.0.2.1:5555", "hold").await });
        first.started.notified().await;
        assert!(tokio::time::timeout(
            std::time::Duration::from_secs(1),
            driver.shell("192.0.2.2:5555", "id")
        )
        .await
        .unwrap()
        .is_ok());
        driver
            .raw_transfer(&["-s", "192.0.2.2:5555", "push", "mock-local", "mock-remote"])
            .await
            .unwrap();
        first.release.notify_one();
        pending.await.unwrap().unwrap();
    }
    #[tokio::test]
    async fn offline_rows_keep_disconnect_available_to_release_capacity() {
        let (driver, sessions) = fake_driver();
        let driver = Arc::new(driver);
        let state = shield_optimizer_core::commands::AppState::new(
            driver.clone(),
            shield_optimizer_core::engine::AppListBundle::default(),
            PathBuf::from("not-created"),
        );
        for port in 1..=(MAX_SESSIONS as u16) {
            driver
                .raw(&["connect", &format!("192.0.2.1:{port}")])
                .await
                .unwrap();
        }
        for session in sessions.lock().unwrap().iter().skip(1) {
            session.ready.store(false, Ordering::Release);
        }
        let listed = driver.raw(&["devices"]).await.unwrap().stdout;
        assert!(listed.contains("192.0.2.1:1\tdevice"));
        assert!(
            listed.contains("192.0.2.1:2\toffline"),
            "offline TV must retain a visible Forget action"
        );
        assert!(driver
            .raw(&["connect", "192.0.2.2:5555"])
            .await
            .unwrap_err()
            .to_string()
            .contains("Use Forget on a TV row"));
        let forgotten =
            shield_optimizer_core::commands::devices::forget_device_impl(&state, "192.0.2.1:2")
                .await
                .unwrap();
        assert!(forgotten.ok);
        assert_eq!(forgotten.disconnected, vec!["192.0.2.1:2"]);
        driver.raw(&["connect", "192.0.2.2:5555"]).await.unwrap();
        assert!(driver.shell("192.0.2.1:1", "healthy").await.is_ok());
        let listed = driver.raw(&["devices"]).await.unwrap().stdout;
        assert!(!listed.contains("192.0.2.1:2\t"));
        assert!(listed.contains("192.0.2.1:3\toffline"));
        assert!(listed.contains("192.0.2.2:5555\tdevice"));
    }
}

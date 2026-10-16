//! Unwired native adapters for the existing coordinator traits; test builds only.
use super::*;
use std::future::Future;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::pin::Pin;

#[derive(Default)]
struct NativeStop(std::sync::atomic::AtomicUsize);
#[async_trait]
impl Stop for NativeStop {
    async fn execute(&self) -> StopResult {
        self.0.fetch_add(1, Ordering::SeqCst);
        StopResult::Success
    }
}

struct NativePreferences {
    root: PathBuf,
}
impl NativePreferences {
    fn new(data_dir: &Path) -> std::io::Result<Self> {
        Ok(Self {
            root: data_dir.canonicalize()?,
        })
    }
    fn path(&self) -> PathBuf {
        self.root.join("exit-settings-v1.json")
    }
    fn reject_link(&self) -> Result<(), Error> {
        match std::fs::symlink_metadata(self.path()) {
            Ok(meta) if !meta.file_type().is_file() => Err(Error::Store),
            Ok(_) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(Error::Store),
        }
    }
    fn load(&self) -> (Preference, bool) {
        let read = || -> Result<Preference, Error> {
            self.reject_link()?;
            let file = match std::fs::File::open(self.path()) {
                Ok(file) => file,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    return Ok(Preference::default())
                }
                Err(_) => return Err(Error::Store),
            };
            let mut bytes = Vec::new();
            file.take(4097)
                .read_to_end(&mut bytes)
                .map_err(|_| Error::Store)?;
            if bytes.len() > 4096 {
                return Err(Error::Store);
            }
            let value: Preference = serde_json::from_slice(&bytes).map_err(|_| Error::Store)?;
            if value.version != 1 {
                return Err(Error::Store);
            }
            Ok(value)
        };
        match read() {
            Ok(value) => (value, false),
            Err(_) => (Preference::default(), true),
        }
    }
}
impl Store for NativePreferences {
    fn persist(&self, preference: Preference) -> Result<(), Error> {
        self.reject_link()?;
        let write = || -> Result<(), Box<dyn std::error::Error>> {
            let mut file = tempfile::NamedTempFile::new_in(&self.root)?;
            let bytes = preference.encode()?;
            file.write_all(bytes.as_bytes())?;
            file.as_file().sync_all()?;
            file.persist(self.path())?;
            Ok(())
        };
        write().map_err(|_| Error::Store)
    }
}

type LocalClose = Pin<Box<dyn Future<Output = ()> + Send>>;
#[derive(Default)]
struct LocalSet {
    sealed: bool,
    closes: Vec<LocalClose>,
}
#[derive(Default)]
struct NativeResources(Mutex<LocalSet>);
impl NativeResources {
    fn register_remote(
        &self,
        session: shield_optimizer_core::adb::remote_input::RemoteInputSession,
    ) -> Result<(), Error> {
        self.register(Box::pin(session.close_local()))
    }
    fn register(&self, close: LocalClose) -> Result<(), Error> {
        let mut set = self.0.lock().unwrap();
        if set.sealed || set.closes.len() >= 64 {
            return Err(Error::Busy);
        }
        set.closes.push(close);
        Ok(())
    }
}
#[async_trait]
impl Resources for NativeResources {
    async fn close_local_and_reap(&self) -> Result<(), CleanupFailure> {
        let closes = {
            let mut set = self.0.lock().unwrap();
            set.sealed = true;
            std::mem::take(&mut set.closes)
        };
        // Own each close independently: one blocked session cannot prevent the others starting.
        let tasks: Vec<_> = closes.into_iter().map(tokio::spawn).collect();
        let mut failed = false;
        for task in tasks {
            failed |= task.await.is_err();
        }
        if failed {
            Err(CleanupFailure::TaskFailed)
        } else {
            Ok(())
        }
    }
}

#[tokio::test]
async fn native_preferences_round_trip_through_coordinator_without_touching_preview() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(NativePreferences::new(dir.path()).unwrap());
    assert_eq!(store.load(), (Preference::default(), false));
    let resources = Arc::new(NativeResources::default());
    let stop = Arc::new(NativeStop::default());
    let coordinator = Coordinator::new(
        Controller::new(store.load().0),
        store.clone(),
        resources,
        stop,
    );
    let ticket = coordinator.prompt().unwrap();
    coordinator
        .clone()
        .confirm(ticket, ExitPolicy::Auto, true)
        .await
        .unwrap();
    assert_eq!(
        store.load(),
        (Preference::remembered(ExitPolicy::Auto), false)
    );
    assert!(!dir.path().join("development-previews").exists());
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn native_preferences_bound_reads_reject_invalid_versions_and_preserve_bad_input() {
    let dir = tempfile::tempdir().unwrap();
    let store = NativePreferences::new(dir.path()).unwrap();
    for bytes in [
        b"not json".to_vec(),
        vec![b'x'; 5000],
        br#"{"version":2,"policy":"auto","remembered":true}"#.to_vec(),
        br#"{"version":1,"policy":"other","remembered":true}"#.to_vec(),
    ] {
        std::fs::write(store.path(), &bytes).unwrap();
        assert_eq!(store.load(), (Preference::default(), true));
        assert_eq!(std::fs::read(store.path()).unwrap(), bytes);
    }
}

#[tokio::test]
async fn native_store_failure_leaves_prompt_and_admission_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(NativePreferences::new(dir.path()).unwrap());
    std::fs::create_dir(store.path()).unwrap();
    let resources = Arc::new(NativeResources::default());
    let stop = Arc::new(NativeStop::default());
    let coordinator = Coordinator::new(
        Controller::new(Preference::default()),
        store,
        resources,
        stop,
    );
    let ticket = coordinator.prompt().unwrap();
    assert_eq!(
        coordinator
            .clone()
            .confirm(ticket, ExitPolicy::Auto, true)
            .await,
        Err(Error::Store)
    );
    assert_eq!(coordinator.controller.phase(), Phase::Prompting);
    assert!(coordinator.controller.admit_operation().is_ok());
}

#[cfg(unix)]
#[test]
fn native_store_rejects_symlink_without_reading_or_overwriting_target() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("fixture");
    std::fs::write(&target, b"unchanged fixture").unwrap();
    let store = NativePreferences::new(dir.path()).unwrap();
    std::os::unix::fs::symlink(&target, store.path()).unwrap();
    assert_eq!(store.load(), (Preference::default(), true));
    assert_eq!(
        store.persist(Preference::remembered(ExitPolicy::Auto)),
        Err(Error::Store)
    );
    assert_eq!(std::fs::read(target).unwrap(), b"unchanged fixture");
}

#[tokio::test]
async fn native_resources_close_local_socket_and_reap_lease_before_stop() {
    use tokio::io::AsyncReadExt;
    let resources = Arc::new(NativeResources::default());
    // Verify the actual core session API is the adapter boundary; fixture below uses a local socket.
    let _remote_adapter = NativeResources::register_remote;
    let controller = Controller::new(Preference::default());
    let operation = controller.admit_operation().unwrap();
    let lease = operation.register_child().unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let connection = tokio::net::TcpStream::connect(listener.local_addr().unwrap());
    let (client, peer) = tokio::join!(connection, listener.accept());
    let client = client.unwrap();
    let mut peer = peer.unwrap().0;
    resources
        .register(Box::pin(async move {
            drop(client);
            lease.reaped();
        }))
        .unwrap();
    drop(operation);
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(NativePreferences::new(dir.path()).unwrap());
    let stop = Arc::new(NativeStop::default());
    let coordinator = Coordinator::new(controller, store, resources.clone(), stop.clone());
    let ticket = coordinator.prompt().unwrap();
    coordinator
        .clone()
        .confirm(ticket, ExitPolicy::AlwaysStop, false)
        .await
        .unwrap();
    coordinator.clone().run(ticket, true).await.unwrap();
    assert_eq!(peer.read(&mut [0]).await.unwrap(), 0);
    assert_eq!(stop.0.load(Ordering::SeqCst), 1);
    assert_eq!(resources.register(Box::pin(async {})), Err(Error::Busy));
}

#[test]
fn native_resource_registry_has_a_fixed_capacity() {
    let resources = NativeResources::default();
    for _ in 0..64 {
        resources.register(Box::pin(async {})).unwrap();
    }
    assert_eq!(resources.register(Box::pin(async {})), Err(Error::Busy));
}

#[cfg(feature = "e2e")]
#[test]
fn pinned_tauri_cleanup_clears_managed_resources_without_requesting_exit() {
    use tauri::Manager;
    struct Probe(Arc<std::sync::atomic::AtomicBool>);
    impl tauri::Resource for Probe {}
    impl Drop for Probe {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }
    let dropped = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let app = tauri::test::mock_app();
    app.resources_table().add(Probe(dropped.clone()));
    assert!(!dropped.load(Ordering::SeqCst));
    app.handle().cleanup_before_exit();
    // No Tauri API is used after cleanup; the real final executor must exit immediately.
    assert!(dropped.load(Ordering::SeqCst));
}

#[tokio::test]
async fn panicking_native_close_without_child_leases_blocks_stop_and_requires_explicit_exit() {
    let resources = Arc::new(NativeResources::default());
    let entered = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let finished = Arc::new(std::sync::atomic::AtomicBool::new(false));
    resources
        .register(Box::pin(async {
            panic!("injected local cleanup failure");
        }))
        .unwrap();
    let (e, r, f) = (entered.clone(), release.clone(), finished.clone());
    resources
        .register(Box::pin(async move {
            e.notify_one();
            r.notified().await;
            f.store(true, Ordering::SeqCst);
        }))
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(NativePreferences::new(dir.path()).unwrap());
    let stop = Arc::new(NativeStop::default());
    let controller = Controller::new(Preference::default());
    assert!(controller.0.lock().unwrap().children.is_empty());
    let coordinator = Coordinator::new(controller, store, resources, stop.clone());
    let ticket = coordinator.prompt().unwrap();
    coordinator
        .clone()
        .confirm(ticket, ExitPolicy::AlwaysStop, false)
        .await
        .unwrap();
    let run = tokio::spawn(coordinator.clone().run(ticket, true));
    entered.notified().await;
    assert!(!run.is_finished());
    assert!(coordinator.controller.admit_operation().is_err());
    assert_eq!(coordinator.recover_exit(ticket).unwrap(), None);
    release.notify_one();
    assert_eq!(run.await.unwrap(), Err(Error::CleanupFailed));
    assert!(finished.load(Ordering::SeqCst));
    assert_eq!(
        coordinator.controller.phase(),
        Phase::Failed(Failure::LocalCleanupFailed)
    );
    assert_eq!(stop.0.load(Ordering::SeqCst), 0);
    assert_eq!(coordinator.recover_exit(ticket).unwrap(), None);
    let token = coordinator.exit_without_stop(ticket).unwrap();
    assert!(coordinator.allow_exit(Event::Programmatic(token)));
    assert!(coordinator.controller.admit_operation().is_err());
    assert_eq!(stop.0.load(Ordering::SeqCst), 0);
}

//! Identity-bound native Remote orchestration. Production admission remains gated.
use super::remote_input::{encode_inject_keycode, encode_inject_text};
use super::remote_lifecycle::{RemoteSessionIo, RemoteStartCancellation};
use super::{
    AdbByteStream, AdbDeviceBinding, AdbDriver, AdbError, AdbOwnedService, AdbResult,
    AdbServiceCancellation,
};
use async_trait::async_trait;
use std::{
    collections::BTreeSet,
    io::Read,
    path::Path,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio::io::AsyncReadExt;

const LIFETIME: Duration = Duration::from_secs(300);
const IO_DEADLINE: Duration = Duration::from_secs(2);
static NEXT_ID: AtomicU32 = AtomicU32::new(1);

struct Connections(Mutex<Vec<Arc<dyn AdbServiceCancellation>>>);
impl AdbServiceCancellation for Connections {
    fn cancel(&self) {
        for connection in self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
        {
            connection.cancel();
        }
    }
}
impl Drop for Connections {
    fn drop(&mut self) {
        self.cancel();
    }
}

/// All streams in this session share one verified hardware binding.
pub struct NativeRemoteSession {
    control: Arc<Mutex<Box<dyn AdbByteStream>>>,
    connections: Arc<Connections>,
    drain: Option<tokio::task::JoinHandle<()>>,
    held: BTreeSet<u32>,
    poisoned: bool,
    cleanup: Option<(Arc<dyn AdbDriver>, AdbDeviceBinding, String)>,
}

async fn open(
    adb: &Arc<dyn AdbDriver>,
    binding: &AdbDeviceBinding,
    service: &str,
    cancel: &RemoteStartCancellation,
    connections: &Connections,
) -> AdbResult<AdbOwnedService> {
    if cancel.is_cancelled() {
        return Err(AdbError::Transport("Remote startup cancelled.".into()));
    }
    let result = tokio::select! {
        biased;
        _ = cancel.cancelled() => return Err(AdbError::Transport("Remote startup cancelled.".into())),
        result = adb.open_bound_device_service(binding, service, LIFETIME) => result?,
    };
    connections
        .0
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push(result.cancellation.clone());
    if cancel.is_cancelled() {
        result.cancellation.cancel();
        return Err(AdbError::Transport("Remote startup cancelled.".into()));
    }
    Ok(result)
}

struct CancelOnDrop(Option<Arc<dyn AdbServiceCancellation>>);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        if let Some(cancel) = self.0.take() {
            cancel.cancel();
        }
    }
}

async fn io<T: Send + 'static>(
    stream: Arc<Mutex<Box<dyn AdbByteStream>>>,
    cancellation: Arc<dyn AdbServiceCancellation>,
    work: impl FnOnce(&mut dyn AdbByteStream) -> std::io::Result<T> + Send + 'static,
) -> Result<T, String> {
    let mut guard = CancelOnDrop(Some(cancellation.clone()));
    let mut worker = tokio::task::spawn_blocking(move || {
        work(
            &mut **stream
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        )
    });
    match tokio::time::timeout(IO_DEADLINE, &mut worker).await {
        Ok(result) => {
            guard.0 = None;
            result
                .map_err(|_| "Remote worker failed.".to_string())?
                .map_err(|_| "Remote stream I/O failed; outcome unknown.".to_string())
        }
        Err(_) => {
            cancellation.cancel();
            // Owned cancellation unblocks the worker; retain its lease until cleanup.
            let _ = worker.await;
            Err("Remote stream deadline expired; outcome unknown.".into())
        }
    }
}

async fn remove_helper(
    adb: &Arc<dyn AdbDriver>,
    binding: &AdbDeviceBinding,
    jar: &str,
) -> Result<(), String> {
    let service = adb
        .open_bound_device_service(
            binding,
            &format!("shell:rm -f -- '{jar}' && printf REMOVED"),
            IO_DEADLINE,
        )
        .await
        .map_err(|e| e.to_string())?;
    let cancellation = service.cancellation.clone();
    let result = io(
        Arc::new(Mutex::new(service.stream)),
        cancellation.clone(),
        |stream| {
            let mut bytes = [0; 8];
            let mut count = 0;
            while count < bytes.len() {
                let read = stream.read(&mut bytes[count..])?;
                if read == 0 {
                    break;
                }
                count += read;
            }
            if &bytes[..count] != b"REMOVED" {
                return Err(std::io::Error::other("cleanup not acknowledged"));
            }
            Ok(())
        },
    )
    .await;
    cancellation.cancel();
    result
}

impl NativeRemoteSession {
    /// Start one isolated, finite-lived helper and control connection.
    pub async fn start(
        adb: Arc<dyn AdbDriver>,
        jar_path: &Path,
        serial: &str,
        cancel: Arc<RemoteStartCancellation>,
    ) -> Result<Self, String> {
        Self::start_with_deadline(adb, jar_path, serial, cancel, Duration::from_secs(15)).await
    }

    async fn start_with_deadline(
        adb: Arc<dyn AdbDriver>,
        jar_path: &Path,
        serial: &str,
        cancel: Arc<RemoteStartCancellation>,
        deadline: Duration,
    ) -> Result<Self, String> {
        tokio::time::timeout(deadline,Self::start_inner(adb,jar_path,serial,cancel)).await.map_err(|_|"Remote startup deadline expired. Owned connections were cancelled; helper file cleanup could not be confirmed.".to_string())?
    }

    async fn start_inner(
        adb: Arc<dyn AdbDriver>,
        jar_path: &Path,
        serial: &str,
        cancel: Arc<RemoteStartCancellation>,
    ) -> Result<Self, String> {
        let connections = Arc::new(Connections(Mutex::new(Vec::new())));
        if cancel.is_cancelled() {
            return Err("Remote startup cancelled.".into());
        }
        let binding = tokio::select! {biased;
            _=cancel.cancelled()=>return Err("Remote startup cancelled.".into()),
            binding=adb.device_binding(serial)=>binding.map_err(|e|e.to_string())?,
        };
        if cancel.is_cancelled() {
            return Err("Remote startup cancelled.".into());
        }
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed) & 0x7fff_ffff;
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| "Remote clock is unavailable.".to_string())?
            .as_nanos();
        let scid = format!(
            "{:08x}",
            (id ^ nonce as u32 ^ std::process::id().rotate_left(11)) & 0x7fff_ffff
        );
        let jar = format!(
            "/data/local/tmp/shieldopt-remote-{}-{nonce:x}-{scid}.jar",
            std::process::id()
        );
        let file = tokio::fs::File::open(jar_path)
            .await
            .map_err(|e| format!("Remote helper could not be read: {e}"))?;
        let mut bytes = Vec::new();
        file.take(16 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .await
            .map_err(|e| format!("Remote helper could not be read: {e}"))?;
        if bytes.len() > 16 * 1024 * 1024 {
            return Err("Remote helper exceeds upload limit.".into());
        }
        let result = async {
        let upload = open(&adb, &binding, "sync:", &cancel, &connections).await.map_err(|e|e.to_string())?;
        let upload_cancel = upload.cancellation.clone();
        let upload_stream = Arc::new(Mutex::new(upload.stream));
        let upload_path = jar.clone();
        io(upload_stream, upload_cancel.clone(), move |stream| {
            let target = format!("{upload_path},33261");
            stream.write_all(b"SEND")?;
            stream.write_all(&(target.len() as u32).to_le_bytes())?;
            stream.write_all(target.as_bytes())?;
            for chunk in bytes.chunks(64 * 1024) {
                stream.write_all(b"DATA")?;
                stream.write_all(&(chunk.len() as u32).to_le_bytes())?;
                stream.write_all(chunk)?;
            }
            stream.write_all(b"DONE")?;
            stream.write_all(&0u32.to_le_bytes())?;
            stream.flush()?;
            let mut result = [0; 8];
            stream.read_exact(&mut result)?;
            if result != *b"OKAY\0\0\0\0" {
                return Err(std::io::Error::other("helper upload rejected"));
            }
            Ok(())
        })
        .await?;
        upload_cancel.cancel();
        if cancel.is_cancelled() {
            return Err("Remote startup cancelled.".into());
        }
        let command = format!("shell:trap 'rm -f -- \"{jar}\"' EXIT; CLASSPATH={jar} app_process / com.genymobile.scrcpy.Server 3.1 scid={scid} log_level=warn video=false audio=false control=true tunnel_forward=true send_device_meta=false send_dummy_byte=true </dev/null");
        let resident = open(&adb, &binding, &command, &cancel, &connections).await.map_err(|e|e.to_string())?;
        let mut resident_stream = resident.stream;
        let drain = tokio::task::spawn_blocking(move || {
            let mut buffer = [0; 4096];
            loop {
                match resident_stream.read(&mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
            }
        });
        let mut attempt=0;
        let control_result=loop {
            let result=open(&adb,&binding,&format!("localabstract:scrcpy_{scid}"),&cancel,&connections).await;
            match result {
                Err(AdbError::ServiceRejected{..}) if attempt<9 => {
                    attempt+=1;
                    tokio::select!{biased; _=cancel.cancelled()=>break Err(AdbError::Transport("Remote startup cancelled.".into())), _=tokio::time::sleep(Duration::from_millis(150))=>{}}
                }
                result=>break result,
            }
        };
        let control = match control_result {
            Ok(control) => control,
            Err(error) => {
                connections.cancel();
                let _ = drain.await;
                return Err(error.to_string());
            }
        };
        let mut session = Self {
            control: Arc::new(Mutex::new(control.stream)),
            connections: connections.clone(),
            cleanup: Some((adb.clone(),binding.clone(),jar.clone())),
            drain: Some(drain),
            held: BTreeSet::new(),
            poisoned: false,
        };
        let handshake = io(
            session.control.clone(),
            session.connections.clone(),
            |stream| {
                let mut byte = [0];
                stream.read_exact(&mut byte)?;
                if byte[0] != 0 {
                    return Err(std::io::Error::other("invalid Remote handshake"));
                }
                Ok(())
            },
        )
        .await;
        if handshake.is_err() || cancel.is_cancelled() {
            let _ = Box::new(session).close().await;
            return Err(handshake
                .err()
                .unwrap_or_else(|| "Remote startup cancelled.".into()));
        }
        session.poisoned = false;
        Ok(session)
        }.await;
        if let Err(error) = result {
            connections.cancel();
            let cleanup = remove_helper(&adb, &binding, &jar).await;
            return Err(match cleanup {
                Ok(()) => error,
                Err(cleanup) => format!("{error} Helper cleanup not confirmed: {cleanup}"),
            });
        }
        result
    }

    async fn write(&mut self, message: Vec<u8>) -> Result<(), String> {
        if self.poisoned {
            return Err("Remote stream is unusable; input was not replayed.".into());
        }
        let result = io(
            self.control.clone(),
            self.connections.clone(),
            move |stream| {
                stream.write_all(&message)?;
                stream.flush()
            },
        )
        .await;
        if result.is_err() {
            self.poisoned = true;
            self.connections.cancel();
        }
        result
    }
}

#[async_trait]
impl RemoteSessionIo for NativeRemoteSession {
    fn cancellation(&self) -> Arc<dyn AdbServiceCancellation> {
        self.connections.clone()
    }
    async fn send_key(&mut self, keycode: u32, down: bool) -> Result<(), String> {
        if down {
            self.held.insert(keycode);
        }
        self.write(encode_inject_keycode(
            if down { 0 } else { 1 },
            keycode,
            0,
            0,
        ))
        .await?;
        if !down {
            self.held.remove(&keycode);
        }
        Ok(())
    }
    async fn send_key_press(&mut self, keycode: u32) -> Result<(), String> {
        self.send_key(keycode, true).await?;
        self.send_key(keycode, false).await
    }
    async fn send_text(&mut self, text: &str) -> Result<(), String> {
        if text.len() > 64 * 1024 {
            return Err("Remote text exceeds 64 KiB; nothing was sent.".into());
        }
        let mut remaining = text;
        while !remaining.is_empty() {
            let mut end = remaining.len().min(300);
            while !remaining.is_char_boundary(end) {
                end -= 1;
            }
            self.write(encode_inject_text(&remaining[..end])).await?;
            remaining = &remaining[end..];
        }
        Ok(())
    }
    async fn close(mut self: Box<Self>) -> Result<(), String> {
        let mut uncertain = self.poisoned && !self.held.is_empty();
        if !self.poisoned {
            for key in self.held.clone() {
                if self.send_key(key, false).await.is_err() {
                    uncertain = true;
                    break;
                }
            }
        }
        self.connections.cancel();
        if let Some(drain) = self.drain.take() {
            let _ = drain.await;
        }
        let cleanup = if let Some((adb, binding, jar)) = self.cleanup.take() {
            remove_helper(&adb, &binding, &jar).await
        } else {
            Ok(())
        };
        if uncertain {
            let message =
                "Remote held-key release could not be confirmed. Check the TV before retrying.";
            Err(match cleanup {
                Ok(()) => message.into(),
                Err(error) => format!("{message} Helper cleanup not confirmed: {error}"),
            })
        } else {
            cleanup
        }
    }
}
impl Drop for NativeRemoteSession {
    fn drop(&mut self) {
        self.connections.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adb::{AdbOutput, AdbResult};
    use std::io::Write;
    use std::sync::{atomic::AtomicBool, Condvar};
    #[derive(Default)]
    struct Stop {
        stopped: Mutex<bool>,
        wake: Condvar,
    }
    impl AdbServiceCancellation for Stop {
        fn cancel(&self) {
            *self.stopped.lock().unwrap() = true;
            self.wake.notify_all();
        }
    }
    struct Stream {
        response: std::io::Cursor<Vec<u8>>,
        writes: Arc<Mutex<Vec<u8>>>,
        stop: Arc<Stop>,
        resident: bool,
        fail: Arc<AtomicBool>,
    }
    impl Read for Stream {
        fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
            if self.resident {
                let mut stopped = self.stop.stopped.lock().unwrap();
                while !*stopped {
                    stopped = self.stop.wake.wait(stopped).unwrap();
                }
                return Ok(0);
            }
            std::io::Read::read(&mut self.response, out)
        }
    }
    impl Write for Stream {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if self.fail.load(Ordering::Acquire) {
                if let Some(byte) = bytes.first() {
                    self.writes.lock().unwrap().push(*byte);
                }
                return Err(std::io::Error::other("simulated partial write"));
            }
            self.writes.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    #[derive(Default)]
    struct Fake {
        bindings: Mutex<Vec<AdbDeviceBinding>>,
        services: Mutex<Vec<String>>,
        stops: Mutex<Vec<Arc<Stop>>>,
        control: Arc<Mutex<Vec<u8>>>,
        fail: Arc<AtomicBool>,
        bad_handshake: AtomicBool,
        pending_control: AtomicBool,
        control_refusals: AtomicU32,
        uncertain_open: AtomicBool,
        cancel_on_control: Mutex<Option<Arc<RemoteStartCancellation>>>,
    }
    #[async_trait]
    impl AdbDriver for Fake {
        fn uses_host_server(&self) -> bool {
            false
        }
        async fn raw(&self, _: &[&str]) -> AdbResult<AdbOutput> {
            panic!("unbound host command forbidden")
        }
        async fn shell(&self, _: &str, _: &str) -> AdbResult<AdbOutput> {
            panic!("unbound shell forbidden")
        }
        async fn device_binding(&self, serial: &str) -> AdbResult<AdbDeviceBinding> {
            Ok(AdbDeviceBinding {
                serial: serial.into(),
                hardware_id: "fixture-hardware".into(),
            })
        }
        async fn open_bound_device_service(
            &self,
            binding: &AdbDeviceBinding,
            service: &str,
            lifetime: Duration,
        ) -> AdbResult<AdbOwnedService> {
            assert_eq!(
                lifetime,
                if service.starts_with("shell:rm ") {
                    IO_DEADLINE
                } else {
                    LIFETIME
                }
            );
            self.bindings.lock().unwrap().push(binding.clone());
            self.services.lock().unwrap().push(service.into());
            if service.starts_with("localabstract:") {
                if self.uncertain_open.load(Ordering::Acquire) {
                    return Err(AdbError::Transport("OPEN acknowledgment lost".into()));
                }
                if self
                    .control_refusals
                    .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                        count.checked_sub(1)
                    })
                    .is_ok()
                {
                    return Err(AdbError::ServiceRejected {
                        operation: "fixture-control",
                    });
                }
            }
            if service.starts_with("localabstract:") && self.pending_control.load(Ordering::Acquire)
            {
                std::future::pending::<()>().await;
            }
            let stop = Arc::new(Stop::default());
            self.stops.lock().unwrap().push(stop.clone());
            let control = service.starts_with("localabstract:");
            if control {
                if let Some(cancel) = self.cancel_on_control.lock().unwrap().as_ref() {
                    cancel.cancel();
                }
            }
            Ok(AdbOwnedService {
                stream: Box::new(Stream {
                    response: std::io::Cursor::new(if service == "sync:" {
                        b"OKAY\0\0\0\0".to_vec()
                    } else if service.starts_with("shell:rm ") {
                        b"REMOVED".to_vec()
                    } else {
                        vec![u8::from(self.bad_handshake.load(Ordering::Acquire))]
                    }),
                    writes: if control {
                        self.control.clone()
                    } else {
                        Arc::default()
                    },
                    stop: stop.clone(),
                    resident: service.starts_with("shell:trap "),
                    fail: if control {
                        self.fail.clone()
                    } else {
                        Arc::default()
                    },
                }),
                cancellation: stop,
            })
        }
    }
    fn fixture() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml")
    }
    #[tokio::test]
    async fn native_remote_binding_unicode_held_release_and_isolation() {
        let fake = Arc::new(Fake::default());
        let mut first =
            NativeRemoteSession::start(fake.clone(), &fixture(), "fixture-a", Arc::default())
                .await
                .unwrap();
        let second =
            NativeRemoteSession::start(fake.clone(), &fixture(), "fixture-b", Arc::default())
                .await
                .unwrap();
        let services = fake.services.lock().unwrap().clone();
        assert_ne!(services[2], services[5]);
        let bindings = fake.bindings.lock().unwrap();
        assert!(bindings[..3].iter().all(|b| b.serial == "fixture-a"));
        assert!(bindings[3..].iter().all(|b| b.serial == "fixture-b"));
        drop(bindings);
        first.send_key(19, true).await.unwrap();
        let text = "é漢字😀".repeat(100);
        first.send_text(&text).await.unwrap();
        Box::new(first).close().await.unwrap();
        let first_jar = services[1]
            .split("CLASSPATH=")
            .nth(1)
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap();
        let second_jar = services[4]
            .split("CLASSPATH=")
            .nth(1)
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap();
        assert_ne!(first_jar, second_jar);
        assert_eq!(
            fake.services.lock().unwrap().last().unwrap(),
            &format!("shell:rm -f -- '{first_jar}' && printf REMOVED"),
            "cleanup targets only this generation's helper"
        );
        assert!(!fake
            .services
            .lock()
            .unwrap()
            .iter()
            .any(|service| service.contains("pkill")));
        let bytes = fake.control.lock().unwrap().clone();
        assert_eq!(&bytes[..14], encode_inject_keycode(0, 19, 0, 0));
        let mut cursor = 14;
        let mut restored = String::new();
        while bytes[cursor] == 1 {
            let len =
                u32::from_be_bytes(bytes[cursor + 1..cursor + 5].try_into().unwrap()) as usize;
            assert!(len <= 300);
            restored.push_str(std::str::from_utf8(&bytes[cursor + 5..cursor + 5 + len]).unwrap());
            cursor += 5 + len;
        }
        assert_eq!(restored, text);
        assert_eq!(&bytes[cursor..], encode_inject_keycode(1, 19, 0, 0));
        assert!(
            !*fake.stops.lock().unwrap()[5].stopped.lock().unwrap(),
            "old cleanup cannot cancel new control"
        );
        Box::new(second).close().await.unwrap();
        assert!(fake
            .stops
            .lock()
            .unwrap()
            .iter()
            .all(|s| *s.stopped.lock().unwrap()));
    }
    #[tokio::test]
    async fn native_remote_write_failure_never_replays_and_reports_release_unknown() {
        let fake = Arc::new(Fake::default());
        let mut session =
            NativeRemoteSession::start(fake.clone(), &fixture(), "fixture", Arc::default())
                .await
                .unwrap();
        fake.fail.store(true, Ordering::Release);
        assert!(session.send_key(19, true).await.is_err());
        fake.fail.store(false, Ordering::Release);
        assert!(session.send_key_press(20).await.is_err());
        assert_eq!(
            fake.control.lock().unwrap().len(),
            1,
            "unknown write side effect is never replayed"
        );
        assert!(Box::new(session)
            .close()
            .await
            .unwrap_err()
            .contains("release could not be confirmed"));
    }
    #[tokio::test]
    async fn native_remote_cancel_before_start_performs_no_service_io() {
        let fake = Arc::new(Fake::default());
        let cancel = Arc::new(RemoteStartCancellation::default());
        cancel.cancel();
        assert!(
            NativeRemoteSession::start(fake.clone(), &fixture(), "fixture", cancel)
                .await
                .is_err()
        );
        assert!(fake.services.lock().unwrap().is_empty());
    }
    #[tokio::test]
    async fn native_remote_late_open_cancel_and_bad_handshake_clean_all_owned_streams() {
        for bad_handshake in [false, true] {
            let fake = Arc::new(Fake::default());
            let cancel = Arc::new(RemoteStartCancellation::default());
            if bad_handshake {
                fake.bad_handshake.store(true, Ordering::Release);
            } else {
                *fake.cancel_on_control.lock().unwrap() = Some(cancel.clone());
            }
            assert!(
                NativeRemoteSession::start(fake.clone(), &fixture(), "fixture", cancel)
                    .await
                    .is_err()
            );
            assert!(fake.stops.lock().unwrap().len() >= 3);
            assert!(fake
                .stops
                .lock()
                .unwrap()
                .iter()
                .all(|stop| *stop.stopped.lock().unwrap()));
            assert!(fake.control.lock().unwrap().is_empty());
        }
    }
    #[tokio::test]
    async fn native_remote_cancel_pending_open_joins_owned_resident() {
        let fake = Arc::new(Fake::default());
        fake.pending_control.store(true, Ordering::Release);
        let cancel = Arc::new(RemoteStartCancellation::default());
        let task = tokio::spawn({
            let fake = fake.clone();
            let cancel = cancel.clone();
            async move { NativeRemoteSession::start(fake, &fixture(), "fixture", cancel).await }
        });
        while fake.services.lock().unwrap().len() < 3 {
            tokio::task::yield_now().await;
        }
        cancel.cancel();
        assert!(tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .unwrap()
            .unwrap()
            .is_err());
        assert!(fake
            .stops
            .lock()
            .unwrap()
            .iter()
            .all(|stop| *stop.stopped.lock().unwrap()));
    }

    #[tokio::test]
    async fn native_remote_abandoned_io_cancels_worker_and_waits_for_cleanup() {
        let stop = Arc::new(Stop::default());
        let started = Arc::new(AtomicBool::new(false));
        let finished = Arc::new(AtomicBool::new(false));
        let stream: Arc<Mutex<Box<dyn AdbByteStream>>> = Arc::new(Mutex::new(Box::new(Stream {
            response: std::io::Cursor::new(vec![]),
            writes: Arc::default(),
            stop: stop.clone(),
            resident: true,
            fail: Arc::default(),
        })));
        let task = tokio::spawn(io(stream, stop.clone(), {
            let started = started.clone();
            let finished = finished.clone();
            move |stream| {
                started.store(true, Ordering::Release);
                let result = stream.read(&mut [0]);
                finished.store(true, Ordering::Release);
                result
            }
        }));
        while !started.load(Ordering::Acquire) {
            tokio::task::yield_now().await;
        }
        task.abort();
        let _ = task.await;
        tokio::time::timeout(Duration::from_secs(1), async {
            while !finished.load(Ordering::Acquire) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert!(*stop.stopped.lock().unwrap());
    }
    #[tokio::test]
    async fn native_remote_retries_only_explicit_startup_refusal_never_unknown_open() {
        let fake = Arc::new(Fake::default());
        fake.control_refusals.store(2, Ordering::Release);
        let session =
            NativeRemoteSession::start(fake.clone(), &fixture(), "fixture", Arc::default())
                .await
                .unwrap();
        assert_eq!(
            fake.services
                .lock()
                .unwrap()
                .iter()
                .filter(|s| s.starts_with("localabstract:"))
                .count(),
            3
        );
        Box::new(session).close().await.unwrap();
        let fake = Arc::new(Fake::default());
        fake.uncertain_open.store(true, Ordering::Release);
        assert!(
            NativeRemoteSession::start(fake.clone(), &fixture(), "fixture", Arc::default())
                .await
                .is_err()
        );
        assert_eq!(
            fake.services
                .lock()
                .unwrap()
                .iter()
                .filter(|s| s.starts_with("localabstract:"))
                .count(),
            1
        );
        assert!(fake
            .stops
            .lock()
            .unwrap()
            .iter()
            .all(|stop| *stop.stopped.lock().unwrap()));
    }
    #[tokio::test]
    async fn native_remote_total_start_deadline_cancels_owned_streams_reports_residue_gate() {
        let fake = Arc::new(Fake::default());
        fake.pending_control.store(true, Ordering::Release);
        let error = NativeRemoteSession::start_with_deadline(
            fake.clone(),
            &fixture(),
            "fixture",
            Arc::default(),
            Duration::from_millis(50),
        )
        .await
        .err()
        .unwrap();
        assert!(error.contains("startup deadline expired"));
        assert!(error.contains("file cleanup could not be confirmed"));
        assert!(fake
            .stops
            .lock()
            .unwrap()
            .iter()
            .all(|stop| *stop.stopped.lock().unwrap()));
    }
}

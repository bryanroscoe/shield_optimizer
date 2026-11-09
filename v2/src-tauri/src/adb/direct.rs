//! Opt-in network transport using the same sessions as the Android app.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Weak};

use async_trait::async_trait;
use atv_direct_adb::{ConnectAuth, ConnectFailure, Paired, PreferredKeys, WirelessAdb};
use atv_optimizer_core::adb::{
    native_mdns_services, render_mdns_services, AdbByteStream, AdbDriver, AdbError, AdbOutput,
    AdbResult, ApprovalRequestOutcome, Leases, NativeDiscoveryReport, ScanConnectOutcome,
    ScanConnectionFailure, ScanConnectionProgress, ServerStop,
};
use atv_optimizer_core::commands::devices::normalize_connect_address;
use std::time::Duration;
use tokio::sync::{watch, Mutex, Semaphore};
use tokio::time::Instant;

const MAX_SESSIONS: usize = 32;

/// How long Request approval keeps the connection open for the person to
/// choose Allow on the TV. Cancel, Forget, an engine switch or quit end it
/// sooner.
pub const APPROVAL_WAIT: Duration = Duration::from_secs(10 * 60);

/// Bound on a connection that offers no public key. It never waits for a
/// person, only for the TV to answer.
const QUIET_CONNECT_TIMEOUT: Duration = Duration::from_secs(20);

/// How long Request approval waits for the TV to be asked (its public key
/// written). A TV that has not been asked by then counts as not answering:
/// the request stops and reports a connection failure.
const APPROVAL_START_WAIT: Duration = QUIET_CONNECT_TIMEOUT;

/// How long a quiet refusal counts as current evidence: the TV at that
/// address may be named, and a scan skips that service's other routes.
/// After this the endpoint is still listed, by its address only.
const NOT_ALLOWED_RECENT: Duration = Duration::from_secs(5 * 60);

/// How long Add by IP waits for another connection to the same address.
const FOREGROUND_LOCK_WAIT: Duration = QUIET_CONNECT_TIMEOUT;

/// What `adb connect` prints for a TV that refused every key, the way
/// platform-tools words it, so the shared connect command reads it as
/// unauthorized.
fn not_allowed_output(serial: &str) -> AdbOutput {
    output(format!(
        "failed to authenticate to {serial}: this TV has not allowed this computer yet"
    ))
}

#[cfg(all(test, unix))]
#[path = "direct_comparison.rs"]
mod comparison;

#[async_trait]
trait NetworkSession: AdbDriver {
    /// Connect under `auth`; `budget` bounds the whole handshake, including
    /// any wait for the person to choose Allow on the TV.
    async fn connect(
        &self,
        host: &str,
        port: u16,
        request: u64,
        auth: ConnectAuth,
        budget: Duration,
    ) -> Result<String, ConnectFailure>;
    async fn pair(&self, host: &str, port: u16, code: &str) -> Result<Paired, String>;
    async fn ready(&self) -> bool;
    fn awaiting_authorization(&self) -> bool {
        false
    }
    fn invalidate(&self, request: u64);
    fn close_idle(&self) -> Result<(), String>;
}

#[async_trait]
impl NetworkSession for WirelessAdb {
    async fn connect(
        &self,
        host: &str,
        port: u16,
        request: u64,
        auth: ConnectAuth,
        budget: Duration,
    ) -> Result<String, ConnectFailure> {
        self.begin_request(request)
            .map_err(ConnectFailure::Failed)?;
        WirelessAdb::connect_with(self, host, port, request, auth, budget).await
    }
    async fn pair(&self, host: &str, port: u16, code: &str) -> Result<Paired, String> {
        WirelessAdb::pair_reporting_instance(self, host, port, code).await
    }
    async fn ready(&self) -> bool {
        self.status().await.connected
    }
    fn awaiting_authorization(&self) -> bool {
        WirelessAdb::awaiting_authorization(self)
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
    pending: Arc<AtomicBool>,
}

type Factory = dyn Fn(PathBuf, Arc<Leases>) -> Session + Send + Sync;

struct ConnectionPool {
    key: PathBuf,
    sessions: Mutex<BTreeMap<String, Slot>>,
    requests: Arc<AtomicU64>,
    factory: Arc<Factory>,
    operations: Arc<Leases>,
    lifecycle: Mutex<BTreeMap<String, Weak<Mutex<()>>>>,
    /// Endpoints whose TV answered but trusts none of the keys, with when it
    /// last refused. Listed as `unauthorized` until a later connection there
    /// succeeds or gets no answer, or Forget drops it. Nothing was shown on
    /// those TVs.
    not_allowed: std::sync::Mutex<BTreeMap<String, Instant>>,
    /// Request approval waits, at most one per endpoint.
    approvals: std::sync::Mutex<BTreeMap<String, ApprovalJob>>,
    approval_ids: AtomicU64,
    approval_wait: Duration,
}

/// What a connection that finished without a transport error found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reached {
    Ready,
    /// The TV answered but has not allowed this computer.
    NotAllowed,
}

struct ScanJob {
    canceling: bool,
    task: tokio::task::JoinHandle<()>,
    result: watch::Receiver<Option<Result<Reached, String>>>,
}

struct ApprovalJob {
    id: u64,
    task: tokio::task::JoinHandle<()>,
    done: watch::Receiver<Option<Result<Reached, String>>>,
    started: Instant,
}

pub struct DirectNetworkAdb {
    pool: Arc<ConnectionPool>,
    scan_jobs: std::sync::Mutex<BTreeMap<String, ScanJob>>,
    scan_slots: Arc<Semaphore>,
    closing: AtomicBool,
    discovery: Arc<super::discovery::DiscoveryManager>,
    /// False when `pool.operations` is the app's admission counter, which
    /// outlives this driver when an engine switch retires it.
    owns_operations: bool,
}

impl DirectNetworkAdb {
    pub fn new(key: PathBuf) -> Self {
        Self::with_factory(
            key,
            Arc::new(|key, operations| {
                Arc::new(
                    WirelessAdb::with_existing_key(key, operations)
                        .with_dedicated_connections(ConnectAuth::SignaturesOnly),
                )
            }),
        )
    }

    /// Counts this driver's work on the app's admission counter, so quit
    /// drains it, and leaves that counter open when the driver is dropped.
    pub fn with_shared_operations(key: PathBuf, operations: Arc<Leases>) -> Self {
        Self::with_shared_operations_and_keys(key, PreferredKeys::none(), operations)
    }

    /// [`Self::with_shared_operations`], with every connection offering
    /// `preferred` (the platform-tools key) before the app's own `key`.
    pub fn with_shared_operations_and_keys(
        key: PathBuf,
        preferred: PreferredKeys,
        operations: Arc<Leases>,
    ) -> Self {
        Self::with_factory_and_operations(
            key,
            Arc::new(move |key, operations| {
                Arc::new(
                    WirelessAdb::with_existing_key(key, operations)
                        .with_preferred_keys(preferred.clone())
                        .with_dedicated_connections(ConnectAuth::SignaturesOnly),
                )
            }),
            operations,
            false,
        )
    }

    fn with_factory(key: PathBuf, factory: Arc<Factory>) -> Self {
        Self::with_factory_and_operations(key, factory, Leases::new(), true)
    }

    fn with_factory_and_operations(
        key: PathBuf,
        factory: Arc<Factory>,
        operations: Arc<Leases>,
        owns_operations: bool,
    ) -> Self {
        Self {
            pool: Arc::new(ConnectionPool {
                key,
                sessions: Mutex::new(BTreeMap::new()),
                requests: Arc::new(AtomicU64::new(0)),
                factory,
                operations,
                lifecycle: Mutex::new(BTreeMap::new()),
                not_allowed: std::sync::Mutex::new(BTreeMap::new()),
                approvals: std::sync::Mutex::new(BTreeMap::new()),
                approval_ids: AtomicU64::new(0),
                approval_wait: APPROVAL_WAIT,
            }),
            scan_jobs: std::sync::Mutex::new(BTreeMap::new()),
            scan_slots: Arc::new(Semaphore::new(4)),
            closing: AtomicBool::new(false),
            discovery: super::discovery::DiscoveryManager::new(Arc::new(
                super::discovery::MdnsProvider::default(),
            )),
            owns_operations,
        }
    }

    fn jobs(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, ScanJob>> {
        self.scan_jobs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    async fn connect_foreground(&self, address: &str) -> AdbResult<AdbOutput> {
        let serial = normalize_connect_address(address).map_err(AdbError::Transport)?;
        // A request already holds this TV's connection open for its answer.
        if self.pool.approval_active(&serial) {
            return Ok(not_allowed_output(&serial));
        }
        let pending = {
            let jobs = self.jobs();
            if jobs.get(&serial).is_some_and(|job| job.canceling) {
                return Err(AdbError::Transport(
                    "This endpoint is being disconnected.".into(),
                ));
            }
            jobs.get(&serial)
                .filter(|job| job.result.borrow().is_none())
                .map(|job| job.result.clone())
        };
        if let Some(mut result) = pending {
            return match wait_scan_result(&mut result).await {
                Ok(Reached::Ready) => Ok(output(format!("connected to {serial}"))),
                Ok(Reached::NotAllowed) => Ok(not_allowed_output(&serial)),
                // A request replaced the scan's attempt; it is waiting on the TV.
                Err(_) if self.pool.approval_active(&serial) => Ok(not_allowed_output(&serial)),
                Err(message) => Err(AdbError::Transport(message)),
            };
        }
        match self
            .pool
            .connect(
                &serial,
                None,
                Some(Instant::now() + FOREGROUND_LOCK_WAIT),
                ConnectAuth::SignaturesOnly,
            )
            .await?
        {
            Reached::Ready => Ok(output(format!("connected to {serial}"))),
            Reached::NotAllowed => Ok(not_allowed_output(&serial)),
        }
    }

    async fn start_scan_connect(
        &self,
        address: &str,
        valid_for: Duration,
    ) -> AdbResult<ScanConnectOutcome> {
        let operation = self
            .pool
            .operations
            .try_acquire()
            .ok_or(AdbError::ShuttingDown)?;
        let serial = normalize_connect_address(address).map_err(AdbError::Transport)?;
        serial.parse::<std::net::SocketAddr>().map_err(|_| {
            AdbError::Transport("Direct ADB requires a numeric IP address and port.".into())
        })?;
        let deadline = tokio::time::Instant::now()
            .checked_add(valid_for)
            .ok_or_else(|| AdbError::Transport("Invalid scan connection deadline.".into()))?;
        // Waiting on the person already; a quiet attempt could only queue
        // behind that wait.
        if self.pool.approval_active(&serial) {
            return Ok(ScanConnectOutcome::NotAllowed);
        }
        let mut result = {
            let mut jobs = self.jobs();
            if self.closing.load(Ordering::Acquire) {
                return Err(AdbError::ShuttingDown);
            }
            if jobs.get(&serial).is_some_and(|job| job.canceling) {
                return Err(AdbError::Transport(
                    "This endpoint is being disconnected.".into(),
                ));
            }
            if let Some(job) = jobs
                .get(&serial)
                .filter(|job| job.result.borrow().is_none() && !job.task.is_finished())
            {
                job.result.clone()
            } else {
                jobs.remove(&serial);
                if jobs.len() >= MAX_SESSIONS {
                    jobs.retain(|_, job| {
                        job.canceling || (job.result.borrow().is_none() && !job.task.is_finished())
                    });
                }
                if jobs.len() >= MAX_SESSIONS {
                    return Err(AdbError::Transport("Native scan connection limit reached; wait for pending connections to finish.".into()));
                }
                let (sender, receiver) = watch::channel(None);
                let pool = self.pool.clone();
                let slots = self.scan_slots.clone();
                let target = serial.clone();
                let task = tokio::spawn(async move {
                    let _operation = operation;
                    let result = async {
                        let _slot = tokio::time::timeout_at(deadline, slots.acquire_owned())
                            .await
                            .map_err(|_| {
                                "Advertisement expired while queued; scan again.".to_string()
                            })?
                            .map_err(|_| "Native scan is shutting down.".to_string())?;
                        match tokio::time::timeout(
                            Duration::from_secs(45),
                            pool.connect(
                                &target,
                                Some(deadline),
                                Some(deadline),
                                ConnectAuth::SignaturesOnly,
                            ),
                        )
                        .await
                        {
                            Ok(result) => result.map_err(|error| error.to_string()),
                            Err(_) => {
                                // The scan reached for it and got no answer.
                                pool.clear_not_allowed(&target);
                                Err("Connection timed out; check the TV and scan again."
                                    .to_string())
                            }
                        }
                    }
                    .await;
                    sender.send_replace(Some(result));
                });
                jobs.insert(
                    serial,
                    ScanJob {
                        canceling: false,
                        task,
                        result: receiver.clone(),
                    },
                );
                receiver
            }
        };
        match tokio::time::timeout(Duration::from_millis(250), wait_scan_result(&mut result)).await
        {
            Ok(Ok(Reached::Ready)) => Ok(ScanConnectOutcome::Ready),
            Ok(Ok(Reached::NotAllowed)) => Ok(ScanConnectOutcome::NotAllowed),
            Ok(Err(message)) => Err(AdbError::Transport(message)),
            Err(_) => Ok(ScanConnectOutcome::Pending),
        }
    }

    /// Start the one connection allowed to offer the public key, and keep
    /// it open for up to the pool's approval wait. Returns once the TV was
    /// asked, or once it is clear it could not be.
    async fn request_approval_impl(&self, address: &str) -> AdbResult<ApprovalRequestOutcome> {
        let operation = self
            .pool
            .operations
            .try_acquire()
            .ok_or(AdbError::ShuttingDown)?;
        let serial = normalize_connect_address(address).map_err(AdbError::Transport)?;
        serial.parse::<std::net::SocketAddr>().map_err(|_| {
            AdbError::Transport("Direct ADB requires a numeric IP address and port.".into())
        })?;
        if self.closing.load(Ordering::Acquire) {
            return Err(AdbError::ShuttingDown);
        }
        if let Ok(session) = self.pool.session(&serial).await {
            if session.ready().await {
                return Ok(ApprovalRequestOutcome::Ready);
            }
        }
        // A quiet scan attempt holds this endpoint's connection lock; the
        // request replaces it rather than queueing behind it.
        let replaced = {
            let mut jobs = self.jobs();
            if jobs.get(&serial).is_some_and(|job| job.canceling) {
                return Err(AdbError::Transport(
                    "This endpoint is being disconnected.".into(),
                ));
            }
            jobs.remove(&serial)
        };
        let (mut done, id, started) = {
            let mut approvals = self.pool.lock_approvals();
            if self.closing.load(Ordering::Acquire) {
                return Err(AdbError::ShuttingDown);
            }
            if let Some(job) = approvals.get(&serial).filter(|job| !job.task.is_finished()) {
                // Asking again joins the request already running.
                (job.done.clone(), job.id, job.started)
            } else {
                let id = self.pool.approval_ids.fetch_add(1, Ordering::Relaxed) + 1;
                let (sender, receiver) = watch::channel(None);
                let pool = self.pool.clone();
                let target = serial.clone();
                let task = tokio::spawn(async move {
                    let _operation = operation;
                    let result = pool
                        .connect(&target, None, None, ConnectAuth::OfferPublicKey)
                        .await
                        .map_err(|error| error.to_string());
                    pool.finish_approval(&target, id);
                    sender.send_replace(Some(result));
                });
                let started = Instant::now();
                approvals.insert(
                    serial.clone(),
                    ApprovalJob {
                        id,
                        task,
                        done: receiver.clone(),
                        started,
                    },
                );
                (receiver, id, started)
            }
        };
        // Only now, so anything waiting on that attempt sees the request.
        if let Some(job) = replaced {
            job.task.abort();
            let _ = job.task.await;
        }
        tracing::info!(serial = %serial, "approval request started");
        loop {
            if let Some(result) = done.borrow().clone() {
                return match result {
                    Ok(Reached::Ready) => Ok(ApprovalRequestOutcome::Ready),
                    Ok(Reached::NotAllowed) => Ok(ApprovalRequestOutcome::NotApproved),
                    // It never got as far as asking the TV.
                    Err(message) => Err(AdbError::Transport(message)),
                };
            }
            if self.pool.awaiting_authorization(&serial).await {
                return Ok(ApprovalRequestOutcome::Waiting);
            }
            if started.elapsed() >= APPROVAL_START_WAIT {
                // Not asked yet: a silent adbd, not a refusal. Stop rather
                // than hold a connection the TV isn't answering.
                if let Some(job) = self.pool.take_approval(&serial, id) {
                    job.task.abort();
                    let _ = job.task.await;
                    if self.pool.settle_aborted(&serial).await {
                        return Ok(ApprovalRequestOutcome::Ready);
                    }
                }
                tracing::info!(serial = %serial, "approval request: the TV did not answer");
                return Err(AdbError::Transport(format!(
                    "The TV at {serial} didn't answer within {} seconds, so it wasn't asked. Check that it's awake and on the same network, then try again.",
                    APPROVAL_START_WAIT.as_secs()
                )));
            }
            tokio::select! {
                changed = done.changed() => {
                    if changed.is_err() {
                        // Canceled or forgotten before the TV was asked.
                        return Ok(ApprovalRequestOutcome::Canceled);
                    }
                }
                () = tokio::time::sleep(Duration::from_millis(50)) => {}
            }
        }
    }

    /// End a request's wait. The TV goes back to not allowed yet, unless it
    /// allowed this computer just before the cancel, which publishes it.
    async fn cancel_approval_impl(&self, address: &str) -> AdbResult<()> {
        let serial = normalize_connect_address(address).map_err(AdbError::Transport)?;
        let job = self.pool.lock_approvals().remove(&serial);
        let Some(job) = job else {
            return Ok(());
        };
        let asked = self.pool.awaiting_authorization(&serial).await;
        job.task.abort();
        let _ = job.task.await;
        let finished = job.done.borrow().clone();
        match finished {
            // It finished first; `connect` already recorded the outcome.
            Some(_) => {}
            None => {
                // An authenticated connection stored just before the abort
                // is published; anything else is closed.
                if !self.pool.settle_aborted(&serial).await && asked {
                    self.pool.mark_not_allowed(&serial);
                }
            }
        }
        tracing::info!(serial = %serial, "approval request canceled");
        Ok(())
    }

    fn abort_approvals(&self) -> Vec<ApprovalJob> {
        let jobs = std::mem::take(&mut *self.pool.lock_approvals());
        for job in jobs.values() {
            job.task.abort();
        }
        jobs.into_values().collect()
    }

    async fn cancel_scan_jobs(&self) {
        let jobs = {
            let mut jobs = self.jobs();
            self.closing.store(true, Ordering::Release);
            std::mem::take(&mut *jobs)
        };
        for job in jobs.values() {
            job.task.abort();
        }
        let approvals = self.abort_approvals();
        for (_, job) in jobs {
            let _ = job.task.await;
        }
        for job in approvals {
            let _ = job.task.await;
        }
    }

    async fn devices(&self) -> AdbResult<AdbOutput> {
        let sessions: Vec<_> = self
            .pool
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
        // A TV that answered but allows none of the keys is listed the way
        // adb lists one: unauthorized, under its exact endpoint. It claims no
        // identity; a ready transport for the same endpoint takes its place.
        let mut listed: BTreeMap<String, &'static str> = ready.into_iter().collect();
        for serial in self.pool.unauthorized() {
            if listed.get(&serial) != Some(&"device") {
                listed.insert(serial, "unauthorized");
            }
        }
        let mut stdout = "List of devices attached\n".to_string();
        for (serial, status) in listed {
            stdout.push_str(&format!("{serial}\t{status}\n"));
        }
        Ok(output(stdout))
    }
}

impl ConnectionPool {
    fn lock_approvals(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, ApprovalJob>> {
        self.approvals
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn lock_not_allowed(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, Instant>> {
        self.not_allowed
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn approval_active(&self, serial: &str) -> bool {
        self.lock_approvals()
            .get(serial)
            .is_some_and(|job| !job.task.is_finished())
    }

    /// Endpoints with an approval request still running, asked or not.
    fn approvals_active(&self) -> Vec<String> {
        self.lock_approvals()
            .iter()
            .filter(|(_, job)| !job.task.is_finished())
            .map(|(serial, _)| serial.clone())
            .collect()
    }

    fn take_approval(&self, serial: &str, id: u64) -> Option<ApprovalJob> {
        let mut approvals = self.lock_approvals();
        if approvals.get(serial).is_some_and(|job| job.id == id) {
            approvals.remove(serial)
        } else {
            None
        }
    }

    /// Every endpoint listed as unauthorized: not allowed yet, or with a
    /// request running.
    fn unauthorized(&self) -> BTreeSet<String> {
        let mut serials: BTreeSet<String> = self.lock_not_allowed().keys().cloned().collect();
        serials.extend(self.approvals_active());
        serials
    }

    /// Endpoints that refused quietly within [`NOT_ALLOWED_RECENT`].
    fn recently_not_allowed(&self) -> Vec<String> {
        let now = Instant::now();
        self.lock_not_allowed()
            .iter()
            .filter(|(_, refused)| now.saturating_duration_since(**refused) <= NOT_ALLOWED_RECENT)
            .map(|(serial, _)| serial.clone())
            .collect()
    }

    fn mark_not_allowed(&self, serial: &str) {
        self.lock_not_allowed()
            .insert(serial.to_string(), Instant::now());
    }

    fn clear_not_allowed(&self, serial: &str) -> bool {
        self.lock_not_allowed().remove(serial).is_some()
    }

    /// After an approval task was aborted: publish an authenticated
    /// connection it stored just before the abort, or close what it left.
    /// Returns whether the TV is now connected.
    async fn settle_aborted(&self, serial: &str) -> bool {
        let Ok(_lifecycle) =
            tokio::time::timeout(QUIET_CONNECT_TIMEOUT, self.lock_target(serial)).await
        else {
            return false;
        };
        let (session, published) = {
            let sessions = self.sessions.lock().await;
            match sessions.get(serial) {
                Some(slot)
                    if !slot.published.load(Ordering::Acquire)
                        && !slot.pending.load(Ordering::Acquire) =>
                {
                    (slot.session.clone(), slot.published.clone())
                }
                _ => return false,
            }
        };
        if session.ready().await {
            published.store(true, Ordering::Release);
            self.clear_not_allowed(serial);
            tracing::info!(serial = %serial, "approval arrived as the request was canceled");
            return true;
        }
        let mut sessions = self.sessions.lock().await;
        if sessions.get(serial).is_some_and(|slot| {
            !slot.published.load(Ordering::Acquire) && !slot.pending.load(Ordering::Acquire)
        }) {
            sessions.remove(serial);
        }
        false
    }

    fn finish_approval(&self, serial: &str, id: u64) {
        let mut approvals = self.lock_approvals();
        if approvals.get(serial).is_some_and(|job| job.id == id) {
            approvals.remove(serial);
        }
    }

    /// Whether the request for `serial` has offered its key, so the TV may be
    /// showing its prompt now.
    async fn awaiting_authorization(&self, serial: &str) -> bool {
        self.sessions.lock().await.get(serial).is_some_and(|slot| {
            slot.pending.load(Ordering::Acquire) && slot.session.awaiting_authorization()
        })
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

    async fn lock_target(&self, serial: &str) -> tokio::sync::OwnedMutexGuard<()> {
        let lock = {
            let mut locks = self.lifecycle.lock().await;
            locks.retain(|_, lock| lock.strong_count() > 0);
            match locks.get(serial).and_then(Weak::upgrade) {
                Some(lock) => lock,
                None => {
                    let lock = Arc::new(Mutex::new(()));
                    locks.insert(serial.to_string(), Arc::downgrade(&lock));
                    lock
                }
            }
        };
        lock.lock_owned().await
    }

    /// Connect to `address`. Only [`ConnectAuth::OfferPublicKey`] may show the
    /// TV's prompt, and only it waits for the person, for up to the approval
    /// wait. A TV that allows none of the keys is [`Reached::NotAllowed`] and
    /// is remembered as such; a transport failure forgets that it was.
    async fn connect(
        &self,
        address: &str,
        valid_until: Option<Instant>,
        lock_until: Option<Instant>,
        auth: ConnectAuth,
    ) -> AdbResult<Reached> {
        let _operation = self
            .operations
            .try_acquire()
            .ok_or(AdbError::ShuttingDown)?;
        let serial = normalize_connect_address(address).map_err(AdbError::Transport)?;
        let endpoint: std::net::SocketAddr = serial.parse().map_err(|_| {
            AdbError::Transport("Direct ADB requires a numeric IP address and port.".into())
        })?;
        let _lifecycle = if let Some(deadline) = lock_until {
            match tokio::time::timeout_at(deadline, self.lock_target(&serial)).await {
                Ok(lifecycle) => lifecycle,
                // A request holds this TV's connection open for its answer.
                Err(_) if auth == ConnectAuth::SignaturesOnly && self.approval_active(&serial) => {
                    return Ok(Reached::NotAllowed);
                }
                Err(_) if valid_until.is_some() => {
                    return Err(AdbError::Transport(
                        "Advertisement expired while queued; scan again.".into(),
                    ));
                }
                Err(_) => {
                    return Err(AdbError::Transport(format!(
                        "Another connection to {serial} is still in progress. Try again in a moment."
                    )));
                }
            }
        } else {
            self.lock_target(&serial).await
        };
        // A request started meanwhile: let it have the TV.
        if auth == ConnectAuth::SignaturesOnly && self.approval_active(&serial) {
            return Ok(Reached::NotAllowed);
        }
        if valid_until.is_some() {
            if let Ok(session) = self.session(&serial).await {
                if session.ready().await {
                    return Ok(Reached::Ready);
                }
            }
        }
        if valid_until.is_some_and(|deadline| Instant::now() >= deadline) {
            return Err(AdbError::Transport(
                "Advertisement expired while queued; scan again.".into(),
            ));
        }
        let (session, published, active_attempt) = {
            let mut sessions = self.sessions.lock().await;
            sessions.retain(|_, slot| {
                slot.published.load(Ordering::Acquire) || slot.pending.load(Ordering::Acquire)
            });
            if !sessions.contains_key(&serial) && sessions.len() >= MAX_SESSIONS {
                return Err(AdbError::Transport(
                    "Direct ADB session limit reached. Use Forget on a TV row to disconnect it first.".into(),
                ));
            }
            let slot = sessions.entry(serial.clone()).or_insert_with(|| Slot {
                session: (self.factory)(self.key.clone(), self.operations.clone()),
                published: Arc::new(AtomicBool::new(false)),
                pending: Arc::new(AtomicBool::new(false)),
            });
            slot.published.store(false, Ordering::Release);
            slot.pending.store(true, Ordering::Release);
            (
                slot.session.clone(),
                slot.published.clone(),
                slot.pending.clone(),
            )
        };
        let request = self.requests.fetch_add(1, Ordering::Relaxed) + 1;
        let mut pending = PendingConnect {
            session: session.clone(),
            requests: self.requests.clone(),
            published: published.clone(),
            active_attempt,
            complete: false,
        };
        let budget = match auth {
            ConnectAuth::SignaturesOnly => QUIET_CONNECT_TIMEOUT,
            ConnectAuth::OfferPublicKey => self.approval_wait,
        };
        let result = session
            .connect(
                serial.rsplit_once(':').unwrap().0,
                endpoint.port(),
                request,
                auth,
                budget,
            )
            .await;
        if let Err(failure) = result {
            if !session.ready().await {
                self.sessions.lock().await.remove(&serial);
            }
            return match failure {
                ConnectFailure::NotAllowed | ConnectFailure::NotApproved(_) => {
                    self.mark_not_allowed(&serial);
                    Ok(Reached::NotAllowed)
                }
                ConnectFailure::Failed(message) => {
                    if auth == ConnectAuth::SignaturesOnly {
                        self.clear_not_allowed(&serial);
                    }
                    Err(AdbError::Transport(message))
                }
            };
        }
        if !session.ready().await {
            self.sessions.lock().await.remove(&serial);
            return Err(AdbError::Transport(format!(
                "Direct ADB did not confirm {serial} is ready."
            )));
        }
        self.clear_not_allowed(&serial);
        published.store(true, Ordering::Release);
        pending.complete = true;
        Ok(Reached::Ready)
    }
}

async fn wait_scan_result(
    result: &mut watch::Receiver<Option<Result<Reached, String>>>,
) -> Result<Reached, String> {
    loop {
        if let Some(done) = result.borrow().clone() {
            return done;
        }
        result
            .changed()
            .await
            .map_err(|_| "Native scan connection was canceled.".to_string())?;
    }
}

async fn ready_entry((serial, session): (String, Session)) -> (String, &'static str) {
    let status = if session.ready().await {
        "device"
    } else {
        "offline"
    };
    (serial, status)
}

struct PendingConnect {
    session: Session,
    requests: Arc<AtomicU64>,
    complete: bool,
    published: Arc<AtomicBool>,
    active_attempt: Arc<AtomicBool>,
}

impl Drop for PendingConnect {
    fn drop(&mut self) {
        if !self.complete {
            self.published.store(false, Ordering::Release);
            self.session
                .invalidate(self.requests.fetch_add(1, Ordering::Relaxed) + 1);
        }
        self.active_attempt.store(false, Ordering::Release);
    }
}

fn output(stdout: String) -> AdbOutput {
    AdbOutput {
        stdout,
        stderr: String::new(),
        exit_code: Some(0),
    }
}

/// `install-multiple [-r] <apk>...`: the only flag the shared driver accepts
/// is `-r`, and at least one non-empty APK path must follow it.
fn split_install_paths(rest: &[&str]) -> bool {
    let paths = rest.strip_prefix(&["-r"]).unwrap_or(rest);
    !paths.is_empty()
        && paths
            .iter()
            .all(|path| !path.is_empty() && !path.starts_with('-'))
}

/// What `adb pair` prints, including the device's own name when it gave a
/// usable one, so pairing identifies the device the same way under both
/// engines.
fn pair_output(address: &str, instance: Option<&str>) -> String {
    match instance {
        Some(name) => format!("Successfully paired to {address} [guid={name}]"),
        None => format!("Successfully paired to {address}"),
    }
}

/// `adb mdns services` from one native browse: only what was observed in that
/// window with a validated endpoint. A browse that saw nothing and reported
/// problems is a failure, not an empty network.
fn mdns_services_output(report: &NativeDiscoveryReport) -> AdbResult<AdbOutput> {
    let services = native_mdns_services(report, Duration::ZERO);
    if services.is_empty() && !report.warnings.is_empty() {
        return Err(AdbError::Transport(format!(
            "Native mDNS discovery failed: {}",
            report.warnings.join(" ")
        )));
    }
    Ok(output(render_mdns_services(&services)))
}

fn unsupported() -> AdbError {
    AdbError::Transport("This operation is unavailable in the built-in ADB engine. To use it, choose Google platform-tools under Settings > ADB engine; no automatic fallback was performed.".into())
}

impl Drop for DirectNetworkAdb {
    fn drop(&mut self) {
        self.discovery.cancel();
        self.closing.store(true, Ordering::Release);
        if self.owns_operations {
            self.pool.operations.close();
        }
        for job in self.jobs().values() {
            job.task.abort();
        }
        self.abort_approvals();
    }
}

#[async_trait]
impl AdbDriver for DirectNetworkAdb {
    fn operation_leases(&self) -> Option<Arc<Leases>> {
        Some(self.pool.operations.clone())
    }

    fn uses_host_server(&self) -> bool {
        false
    }

    async fn discover_network(&self) -> AdbResult<atv_optimizer_core::adb::NativeDiscoveryReport> {
        let lease = Arc::new(
            self.pool
                .operations
                .try_acquire()
                .ok_or(AdbError::ShuttingDown)?,
        );
        self.discovery.snapshot(lease).await
    }

    async fn scan_connect(
        &self,
        address: &str,
        valid_for: Duration,
    ) -> AdbResult<ScanConnectOutcome> {
        self.start_scan_connect(address, valid_for).await
    }

    async fn scan_connection_progress(&self) -> ScanConnectionProgress {
        let mut progress = ScanConnectionProgress::default();
        {
            for (address, job) in self.jobs().iter() {
                match job.result.borrow().clone() {
                    Some(Ok(Reached::Ready)) => progress.ready.push(address.clone()),
                    // Listed below with every other TV that has not allowed this computer.
                    Some(Ok(Reached::NotAllowed)) => {}
                    Some(Err(message)) => progress.failed.push(ScanConnectionFailure {
                        address: address.clone(),
                        message,
                    }),
                    None if job.task.is_finished() => progress.failed.push(ScanConnectionFailure {
                        address: address.clone(),
                        message: "Native scan connection was canceled.".into(),
                    }),
                    None => progress.pending.push(address.clone()),
                }
            }
        }
        // Scans never offer a key, so only an explicit request waits on the
        // person, and only once its key was offered. An endpoint is in one
        // list at a time; a request in progress is not a failure.
        let active = self.pool.approvals_active();
        progress
            .failed
            .retain(|failure| !active.contains(&failure.address));
        progress.requesting = active.clone();
        for serial in active {
            if self.pool.awaiting_authorization(&serial).await {
                progress.awaiting_authorization.push(serial);
            }
        }
        progress.not_allowed = self
            .pool
            .recently_not_allowed()
            .into_iter()
            .filter(|serial| !progress.awaiting_authorization.contains(serial))
            .collect();
        progress
    }

    async fn request_approval(&self, address: &str) -> AdbResult<ApprovalRequestOutcome> {
        self.request_approval_impl(address).await
    }

    async fn cancel_approval(&self, address: &str) -> AdbResult<()> {
        self.cancel_approval_impl(address).await
    }

    async fn stop_server(&self, timeout: std::time::Duration) -> ServerStop {
        self.pool.operations.close();
        // Ends request waits too.
        self.cancel_scan_jobs().await;
        self.discovery.cancel();
        if self.discovery.cleanup_pending() && !timeout.is_zero() {
            let _ = tokio::time::timeout(timeout, self.discovery.retry_cleanup()).await;
        }
        if self.pool.operations.active() != 0 {
            return ServerStop::Skipped(
                "Direct ADB work is still running; owned sockets were left alone.".into(),
            );
        }
        let mut sessions = self.pool.sessions.lock().await;
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
            ["connect", address] => self.connect_foreground(address).await,
            ["pair", address, code] => {
                let endpoint: std::net::SocketAddr = address.parse().map_err(|_| {
                    AdbError::Transport("Enter the numeric IP and pairing port from the TV.".into())
                })?;
                let session =
                    (self.pool.factory)(self.pool.key.clone(), self.pool.operations.clone());
                let paired = session
                    .pair(address.rsplit_once(':').unwrap().0, endpoint.port(), code)
                    .await
                    .map_err(AdbError::Transport)?;
                Ok(output(pair_output(address, paired.instance.as_deref())))
            }
            ["mdns", "services"] => mdns_services_output(&self.discover_network().await?),
            ["disconnect", serial] => {
                let _operation = self
                    .pool
                    .operations
                    .try_acquire()
                    .ok_or(AdbError::ShuttingDown)?;
                let canceled_scan = {
                    let mut jobs = self.jobs();
                    if let Some(job) = jobs.get_mut(*serial) {
                        job.canceling = true;
                        job.task.abort();
                        true
                    } else {
                        false
                    }
                };
                // Forget ends a request's wait and what the scan learned.
                let approval = self.pool.lock_approvals().remove(*serial);
                let canceled_approval = approval.is_some();
                if let Some(job) = approval {
                    job.task.abort();
                    let _ = job.task.await;
                }
                let was_not_allowed = self.pool.clear_not_allowed(serial);
                let _lifecycle = self.pool.lock_target(serial).await;
                let result = match self.pool.session(serial).await {
                    Ok(session) => session.raw(&["disconnect", serial]).await.map(|_| ()),
                    Err(_) if canceled_scan || canceled_approval || was_not_allowed => Ok(()),
                    Err(error) => Err(error),
                };
                self.jobs().remove(*serial);
                result?;
                self.pool.sessions.lock().await.remove(*serial);
                Ok(output(format!("disconnected {serial}")))
            }
            ["-s", serial, "get-state"] => {
                if self.pool.session(serial).await?.ready().await {
                    Ok(output("device\n".into()))
                } else {
                    Err(AdbError::DeviceNotReady {
                        serial: (*serial).into(),
                        state: "offline".into(),
                    })
                }
            }
            ["-s", serial, "reboot", ..] => self.pool.session(serial).await?.raw(args).await,
            _ => Err(unsupported()),
        }
    }

    async fn shell_bounded(
        &self,
        serial: &str,
        command: &str,
    ) -> AdbResult<atv_optimizer_core::adb::driver::BoundedShellOutput> {
        self.pool
            .session(serial)
            .await?
            .shell_bounded(serial, command)
            .await
    }

    async fn shell(&self, serial: &str, command: &str) -> AdbResult<AdbOutput> {
        let out = self
            .pool
            .session(serial)
            .await?
            .shell(serial, command)
            .await?;
        if !out.success() {
            let detail = out.combined().trim().to_string();
            return Err(AdbError::NonZeroExit {
                code: out.exit_code,
                stderr: if detail.is_empty() {
                    format!(
                        "Remote shell returned no output (exit status {:?}).",
                        out.exit_code
                    )
                } else {
                    detail
                },
            });
        }
        Ok(out)
    }

    async fn raw_transfer(&self, args: &[&str]) -> AdbResult<AdbOutput> {
        match args {
            ["-s", serial, "install", path] | ["-s", serial, "install", "-r", path]
                if !path.is_empty() && !path.starts_with('-') =>
            {
                self.pool.session(serial).await?.raw_transfer(args).await
            }
            ["-s", serial, "install-multiple", rest @ ..] if split_install_paths(rest) => {
                self.pool.session(serial).await?.raw_transfer(args).await
            }
            ["-s", serial, "pull" | "push", ..] => {
                self.pool.session(serial).await?.raw_transfer(args).await
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
        self.pool
            .session(serial)
            .await?
            .pull_limited(serial, remote, local, max_bytes)
            .await
    }

    async fn raw_bytes(&self, args: &[&str]) -> AdbResult<Vec<u8>> {
        match args {
            ["-s", serial, ..] => self.pool.session(serial).await?.raw_bytes(args).await,
            _ => Err(unsupported()),
        }
    }

    async fn device_binding(
        &self,
        serial: &str,
    ) -> AdbResult<atv_optimizer_core::adb::AdbDeviceBinding> {
        self.pool
            .session(serial)
            .await?
            .device_binding(serial)
            .await
    }

    async fn open_bound_device_service(
        &self,
        binding: &atv_optimizer_core::adb::AdbDeviceBinding,
        service: &str,
        lifetime: std::time::Duration,
    ) -> AdbResult<atv_optimizer_core::adb::AdbOwnedService> {
        self.pool
            .session(&binding.serial)
            .await?
            .open_bound_device_service(binding, service, lifetime)
            .await
    }

    async fn open_device_service(
        &self,
        serial: &str,
        service: &str,
    ) -> AdbResult<Box<dyn AdbByteStream>> {
        self.pool
            .session(serial)
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
        probe_held: AtomicBool,
        authorization_pending: AtomicBool,
        /// The TV allows none of the keys until a request is approved.
        untrusted: AtomicBool,
        /// The person chooses Allow (with "Always allow") on the TV.
        approve: tokio::sync::Notify,
        /// The person chooses Deny, which ends the connection.
        deny: tokio::sync::Notify,
        /// After Allow, the prompt is gone but the connection finishes only
        /// on `release`.
        hold_after_allow: AtomicBool,
        auths: StdMutex<Vec<(ConnectAuth, Duration)>>,
        /// Every connect fails like this before reaching the TV.
        unreachable: StdMutex<Option<String>>,
        /// A request's connection opens but adbd never answers it.
        silent: AtomicBool,
        /// A request authenticates just as it is canceled: the connection is
        /// stored, but the attempt never returns.
        stored_on_cancel: AtomicBool,
        started: Arc<tokio::sync::Notify>,
        release: Arc<tokio::sync::Notify>,
        completed: Arc<tokio::sync::Notify>,
        peer_instance: StdMutex<Option<String>>,
        hardware_id: StdMutex<Option<String>>,
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
        ) -> AdbResult<atv_optimizer_core::adb::BoundedShellOutput> {
            assert_eq!(*self.serial.lock().unwrap(), serial);
            self.calls
                .lock()
                .unwrap()
                .push(format!("bounded {serial} {command}"));
            Ok(atv_optimizer_core::adb::BoundedShellOutput {
                stdout: "bounded stdout".into(),
                stderr: "bounded stderr".into(),
                exit_code: Some(7),
                termination: atv_optimizer_core::adb::ShellTermination::Completed,
            })
        }
        async fn shell(&self, serial: &str, command: &str) -> AdbResult<AdbOutput> {
            assert_eq!(*self.serial.lock().unwrap(), serial);
            self.calls
                .lock()
                .unwrap()
                .push(format!("{serial} {command}"));
            if command == "silent-failure" {
                return Ok(AdbOutput {
                    stdout: String::new(),
                    stderr: String::new(),
                    exit_code: Some(7),
                });
            }
            if command == "hold" {
                self.started.notify_one();
                self.release.notified().await;
            }
            if command == "getprop ro.serialno" {
                if let Some(id) = self.hardware_id.lock().unwrap().clone() {
                    return Ok(output(format!("{id}\n")));
                }
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
        async fn connect(
            &self,
            host: &str,
            port: u16,
            request: u64,
            auth: ConnectAuth,
            budget: Duration,
        ) -> Result<String, ConnectFailure> {
            self.generation.store(request, Ordering::Release);
            self.auths.lock().unwrap().push((auth, budget));
            if let Some(message) = self.unreachable.lock().unwrap().clone() {
                return Err(ConnectFailure::Failed(message));
            }
            if auth == ConnectAuth::OfferPublicKey && self.silent.load(Ordering::Acquire) {
                tokio::time::sleep(budget).await;
                return Err(ConnectFailure::Failed(
                    "SIMULATED handshake timed out".into(),
                ));
            }
            if auth == ConnectAuth::OfferPublicKey && self.stored_on_cancel.load(Ordering::Acquire)
            {
                self.authorization_pending.store(true, Ordering::Release);
                *self.serial.lock().unwrap() = format!("{host}:{port}");
                self.ready.store(true, Ordering::Release);
                std::future::pending::<()>().await;
            }
            if self.untrusted.load(Ordering::Acquire) {
                if auth == ConnectAuth::SignaturesOnly {
                    return Err(ConnectFailure::NotAllowed);
                }
                self.authorization_pending.store(true, Ordering::Release);
                let answer = tokio::time::timeout(budget, async {
                    tokio::select! {
                        () = self.approve.notified() => true,
                        () = self.deny.notified() => false,
                    }
                })
                .await;
                self.authorization_pending.store(false, Ordering::Release);
                match answer {
                    Ok(true) => {
                        self.untrusted.store(false, Ordering::Release);
                        if self.hold_after_allow.load(Ordering::Acquire) {
                            self.release.notified().await;
                        }
                    }
                    Ok(false) => return Err(ConnectFailure::NotApproved("denied".into())),
                    Err(_) => return Err(ConnectFailure::NotApproved("timed out".into())),
                }
            }
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
                .map_err(|e| ConnectFailure::Failed(e.to_string()))?
                .map_err(ConnectFailure::Failed);
            }
            if self.generation.load(Ordering::Acquire) != request {
                return Err(ConnectFailure::Failed("canceled".into()));
            }
            *self.serial.lock().unwrap() = format!("{host}:{port}");
            self.ready.store(true, Ordering::Release);
            Ok("connected".into())
        }
        async fn pair(&self, host: &str, port: u16, _code: &str) -> Result<Paired, String> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("pair {host}:{port}"));
            Ok(Paired {
                message: "Paired with the TV.".into(),
                instance: self.peer_instance.lock().unwrap().clone(),
            })
        }
        async fn ready(&self) -> bool {
            if self.probe_held.load(Ordering::Acquire) {
                self.started.notify_one();
                self.release.notified().await;
            }
            self.ready.load(Ordering::Acquire) && !self.probe_failed.load(Ordering::Acquire)
        }
        fn awaiting_authorization(&self) -> bool {
            self.authorization_pending.load(Ordering::Acquire)
        }
        fn invalidate(&self, request: u64) {
            self.authorization_pending.store(false, Ordering::Release);
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

    #[tokio::test(start_paused = true)]
    async fn a_pending_quiet_scan_is_never_reported_as_waiting_for_approval() {
        let session = Arc::new(FakeSession::default());
        session.hold.store(true, Ordering::Release);
        let captured = session.clone();
        let driver = DirectNetworkAdb::with_factory(
            PathBuf::from("unused"),
            Arc::new(move |_, _| captured.clone()),
        );
        let target = "192.0.2.1:5555";
        driver
            .scan_connect(target, Duration::from_secs(30))
            .await
            .unwrap();
        // Even a session that says so: a scan never offers the key.
        session.authorization_pending.store(true, Ordering::Release);
        let progress = driver.scan_connection_progress().await;
        assert_eq!(progress.pending, [target]);
        assert!(progress.awaiting_authorization.is_empty());
        assert!(progress.not_allowed.is_empty());
        assert_eq!(
            *session.auths.lock().unwrap(),
            [(ConnectAuth::SignaturesOnly, QUIET_CONNECT_TIMEOUT)]
        );
        tokio::time::advance(Duration::from_secs(45)).await;
        tokio::task::yield_now().await;
        let progress = driver.scan_connection_progress().await;
        assert!(progress.pending.is_empty());
        assert_eq!(progress.failed[0].address, target);
        session.release.notify_one();
        session.completed.notified().await;
        assert!(!session.ready.load(Ordering::Acquire));
    }

    fn untrusted_driver() -> (DirectNetworkAdb, Arc<FakeSession>) {
        let session = Arc::new(FakeSession::default());
        session.untrusted.store(true, Ordering::Release);
        let captured = session.clone();
        let driver = DirectNetworkAdb::with_factory(
            PathBuf::from("unused"),
            Arc::new(move |_, _| captured.clone()),
        );
        (driver, session)
    }

    async fn request_settles(driver: &DirectNetworkAdb, target: &str) {
        while driver.pool.approval_active(target) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    fn offered_keys(session: &FakeSession) -> usize {
        session
            .auths
            .lock()
            .unwrap()
            .iter()
            .filter(|(auth, _)| *auth == ConnectAuth::OfferPublicKey)
            .count()
    }

    #[tokio::test(start_paused = true)]
    async fn scans_and_add_by_ip_list_an_unapproved_tv_as_unauthorized_without_offering_its_key() {
        let (driver, session) = untrusted_driver();
        let target = "192.0.2.1:5555";
        for _ in 0..2 {
            assert_eq!(
                driver
                    .scan_connect(target, Duration::from_secs(30))
                    .await
                    .unwrap(),
                ScanConnectOutcome::NotAllowed
            );
        }
        let added = driver.raw(&["connect", target]).await.unwrap();
        assert!(added.stdout.starts_with("failed to authenticate"));
        assert_eq!(offered_keys(&session), 0, "nothing may show on the TV");
        assert!(session
            .auths
            .lock()
            .unwrap()
            .iter()
            .all(|(_, budget)| *budget == QUIET_CONNECT_TIMEOUT));
        let devices = driver.raw(&["devices"]).await.unwrap().stdout;
        assert!(
            devices.contains("192.0.2.1:5555\tunauthorized"),
            "{devices}"
        );
        let progress = driver.scan_connection_progress().await;
        assert_eq!(progress.not_allowed, [target]);
        assert!(progress.awaiting_authorization.is_empty());
        assert!(progress.failed.is_empty() && progress.pending.is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn a_request_waits_well_past_a_minute_for_allow_then_later_scans_connect_quietly() {
        let (driver, session) = untrusted_driver();
        let target = "192.0.2.1:5555";
        driver
            .scan_connect(target, Duration::from_secs(30))
            .await
            .unwrap();
        assert_eq!(
            driver.request_approval(target).await.unwrap(),
            ApprovalRequestOutcome::Waiting
        );
        assert_eq!(
            session.auths.lock().unwrap().last(),
            Some(&(ConnectAuth::OfferPublicKey, APPROVAL_WAIT))
        );
        assert!(APPROVAL_WAIT >= Duration::from_secs(600));
        // Asking again while it waits sends nothing new.
        assert_eq!(
            driver.request_approval(target).await.unwrap(),
            ApprovalRequestOutcome::Waiting
        );
        // A scan meanwhile neither dials it again nor drops its row.
        assert_eq!(
            driver
                .scan_connect(target, Duration::from_secs(30))
                .await
                .unwrap(),
            ScanConnectOutcome::NotAllowed
        );
        assert_eq!(session.auths.lock().unwrap().len(), 2);
        for wait in [31, 30, 240] {
            tokio::time::advance(Duration::from_secs(wait)).await;
            tokio::task::yield_now().await;
            let progress = driver.scan_connection_progress().await;
            assert_eq!(progress.awaiting_authorization, [target]);
            assert!(progress.not_allowed.is_empty() && progress.failed.is_empty());
            assert!(driver
                .raw(&["devices"])
                .await
                .unwrap()
                .stdout
                .contains("192.0.2.1:5555\tunauthorized"));
        }
        session.approve.notify_one();
        request_settles(&driver, target).await;
        let progress = driver.scan_connection_progress().await;
        assert!(progress.awaiting_authorization.is_empty() && progress.not_allowed.is_empty());
        assert!(driver
            .raw(&["devices"])
            .await
            .unwrap()
            .stdout
            .contains("192.0.2.1:5555\tdevice"));
        // After Forget, a scan connects with a signature: no second request.
        driver.raw(&["disconnect", target]).await.unwrap();
        assert_eq!(
            driver
                .scan_connect(target, Duration::from_secs(30))
                .await
                .unwrap(),
            ScanConnectOutcome::Ready
        );
        assert_eq!(offered_keys(&session), 1);
        assert_eq!(
            session.auths.lock().unwrap().last().unwrap().0,
            ConnectAuth::SignaturesOnly
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_request_stays_open_between_allow_and_the_tv_being_listed() {
        let (driver, session) = untrusted_driver();
        session.hold_after_allow.store(true, Ordering::Release);
        let target = "192.0.2.1:5555";
        assert_eq!(
            driver.request_approval(target).await.unwrap(),
            ApprovalRequestOutcome::Waiting
        );
        let progress = driver.scan_connection_progress().await;
        assert_eq!(progress.awaiting_authorization, [target]);
        assert_eq!(progress.requesting, [target]);
        session.approve.notify_one();
        while session.authorization_pending.load(Ordering::Acquire) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        // The prompt is gone but nothing is listed yet: still open, so the
        // window must not read it as a refusal.
        let progress = driver.scan_connection_progress().await;
        assert!(progress.awaiting_authorization.is_empty());
        assert_eq!(progress.requesting, [target]);
        assert!(progress.not_allowed.is_empty());
        assert!(!driver
            .raw(&["devices"])
            .await
            .unwrap()
            .stdout
            .contains("192.0.2.1:5555\tdevice"));
        session.release.notify_one();
        request_settles(&driver, target).await;
        // It leaves `requesting` only once the outcome is listed.
        let progress = driver.scan_connection_progress().await;
        assert!(progress.requesting.is_empty() && progress.not_allowed.is_empty());
        assert!(driver
            .raw(&["devices"])
            .await
            .unwrap()
            .stdout
            .contains("192.0.2.1:5555\tdevice"));
    }

    #[tokio::test(start_paused = true)]
    async fn cancel_deny_or_the_wait_limit_return_the_tv_quietly_to_not_allowed() {
        for ending in ["cancel", "deny", "limit"] {
            let (driver, session) = untrusted_driver();
            let target = "192.0.2.1:5555";
            assert_eq!(
                driver.request_approval(target).await.unwrap(),
                ApprovalRequestOutcome::Waiting
            );
            match ending {
                "cancel" => driver.cancel_approval(target).await.unwrap(),
                "deny" => session.deny.notify_one(),
                _ => {
                    tokio::time::advance(APPROVAL_WAIT - Duration::from_secs(1)).await;
                    tokio::task::yield_now().await;
                    assert!(driver.pool.approval_active(target), "ended early");
                    tokio::time::advance(Duration::from_secs(2)).await;
                }
            }
            request_settles(&driver, target).await;
            let progress = driver.scan_connection_progress().await;
            assert_eq!(progress.not_allowed, [target], "{ending}");
            assert!(progress.awaiting_authorization.is_empty(), "{ending}");
            assert!(progress.requesting.is_empty(), "{ending}");
            assert!(progress.failed.is_empty(), "{ending}: no transport error");
            assert!(!session.authorization_pending.load(Ordering::Acquire));
            assert!(driver
                .raw(&["devices"])
                .await
                .unwrap()
                .stdout
                .contains("192.0.2.1:5555\tunauthorized"));
            // The button works again.
            assert_eq!(
                driver.request_approval(target).await.unwrap(),
                ApprovalRequestOutcome::Waiting,
                "{ending}"
            );
            assert_eq!(offered_keys(&session), 2);
            // Forget ends that wait and drops the row.
            driver.raw(&["disconnect", target]).await.unwrap();
            let progress = driver.scan_connection_progress().await;
            assert!(progress.awaiting_authorization.is_empty() && progress.not_allowed.is_empty());
            assert!(!driver
                .raw(&["devices"])
                .await
                .unwrap()
                .stdout
                .contains("192.0.2.1"));
            assert!(!session.authorization_pending.load(Ordering::Acquire));
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_request_that_cannot_reach_the_tv_says_why_and_keeps_its_button() {
        let (driver, session) = untrusted_driver();
        let target = "192.0.2.1:5555";
        driver
            .scan_connect(target, Duration::from_secs(30))
            .await
            .unwrap();
        *session.unreachable.lock().unwrap() = Some("SIMULATED no route".into());
        let error = driver.request_approval(target).await.unwrap_err();
        assert!(error.to_string().contains("SIMULATED no route"));
        assert_eq!(
            driver.scan_connection_progress().await.not_allowed,
            [target]
        );
    }

    #[tokio::test(start_paused = true)]
    async fn an_already_allowed_tv_connects_without_a_request() {
        let (driver, session) = fake_driver();
        let target = "192.0.2.1:5555";
        assert_eq!(
            driver.request_approval(target).await.unwrap(),
            ApprovalRequestOutcome::Ready
        );
        assert!(driver
            .raw(&["devices"])
            .await
            .unwrap()
            .stdout
            .contains("192.0.2.1:5555\tdevice"));
        // Its signature was accepted; no prompt was needed.
        let session = session.lock().unwrap()[0].clone();
        assert_eq!(
            driver.request_approval(target).await.unwrap(),
            ApprovalRequestOutcome::Ready
        );
        assert_eq!(session.auths.lock().unwrap().len(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn an_engine_switch_ends_a_request_wait() {
        let (driver, session) = untrusted_driver();
        let target = "192.0.2.1:5555";
        assert_eq!(
            driver.request_approval(target).await.unwrap(),
            ApprovalRequestOutcome::Waiting
        );
        assert!(session.authorization_pending.load(Ordering::Acquire));
        let pool = Arc::downgrade(&driver.pool);
        drop(driver);
        while pool.upgrade().is_some() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(!session.authorization_pending.load(Ordering::Acquire));
    }

    async fn settle_quiet_scan(driver: &DirectNetworkAdb, address: &str) -> ScanConnectionProgress {
        loop {
            let progress = driver.scan_connection_progress().await;
            if !progress.pending.iter().any(|pending| pending == address) {
                return progress;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    // Live, against a TV or emulator that has not allowed `KEY` (a throwaway
    // PKCS#8 key: never the platform-tools adbkey). Ignored in CI; run by hand:
    //   ATV_OPTIMIZER_LIVE_APPROVAL_ADDR=127.0.0.1:5555 \
    //   ATV_OPTIMIZER_LIVE_APPROVAL_KEY=/path/throwaway.pem \
    //   ATV_OPTIMIZER_LIVE_APPROVAL_MODE=allow|cancel|watch \
    //     cargo test -p atv-optimizer live_quiet_scan_then_request_approval -- --ignored --nocapture
    // Scans twice (the TV must show nothing), then Request approval. `allow`
    // waits for Allow, forgets the TV and scans again, which must connect with
    // no request. `cancel` cancels 10 seconds after asking. `watch` reports
    // the state each second for 90 seconds (for Deny).
    #[tokio::test(flavor = "multi_thread")]
    #[ignore]
    async fn live_quiet_scan_then_request_approval() {
        let (Ok(address), Ok(key)) = (
            std::env::var("ATV_OPTIMIZER_LIVE_APPROVAL_ADDR"),
            std::env::var("ATV_OPTIMIZER_LIVE_APPROVAL_KEY"),
        ) else {
            panic!("set ATV_OPTIMIZER_LIVE_APPROVAL_ADDR and ATV_OPTIMIZER_LIVE_APPROVAL_KEY");
        };
        let mode = std::env::var("ATV_OPTIMIZER_LIVE_APPROVAL_MODE").unwrap_or_default();
        let driver = DirectNetworkAdb::new(PathBuf::from(key));
        let started = std::time::Instant::now();
        let at = || format!("{:.1}s", started.elapsed().as_secs_f64());
        for scan in 1..=2 {
            let outcome = driver
                .scan_connect(&address, Duration::from_secs(45))
                .await
                .unwrap();
            let progress = settle_quiet_scan(&driver, &address).await;
            eprintln!("{} scan {scan}: {outcome:?}, {progress:?}", at());
            assert!(progress.not_allowed.contains(&address), "{progress:?}");
        }
        let devices = driver.raw(&["devices"]).await.unwrap().stdout;
        assert!(
            devices.contains(&format!("{address}\tunauthorized")),
            "{devices}"
        );
        let asked = std::time::Instant::now();
        let outcome = driver.request_approval(&address).await.unwrap();
        eprintln!(
            "{} request_approval returned {outcome:?} after {:?}",
            at(),
            asked.elapsed()
        );
        assert_eq!(outcome, ApprovalRequestOutcome::Waiting);
        match mode.as_str() {
            "cancel" => {
                tokio::time::sleep(Duration::from_secs(10)).await;
                driver.cancel_approval(&address).await.unwrap();
                let progress = driver.scan_connection_progress().await;
                eprintln!("{} canceled: {progress:?}", at());
                assert!(progress.not_allowed.contains(&address));
                assert!(progress.awaiting_authorization.is_empty());
            }
            "watch" => {
                for _ in 0..90 {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    let progress = driver.scan_connection_progress().await;
                    eprintln!("{} {progress:?}", at());
                    if progress.awaiting_authorization.is_empty() {
                        break;
                    }
                }
            }
            _ => {
                eprintln!("{} choose Allow (with Always allow) on the TV", at());
                loop {
                    let progress = driver.scan_connection_progress().await;
                    if !progress.awaiting_authorization.contains(&address) {
                        eprintln!(
                            "{} request ended after {:.1}s: {progress:?}",
                            at(),
                            asked.elapsed().as_secs_f64()
                        );
                        break;
                    }
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
                let devices = driver.raw(&["devices"]).await.unwrap().stdout;
                assert!(devices.contains(&format!("{address}\tdevice")), "{devices}");
                driver.raw(&["disconnect", &address]).await.unwrap();
                let outcome = driver
                    .scan_connect(&address, Duration::from_secs(45))
                    .await
                    .unwrap();
                let progress = settle_quiet_scan(&driver, &address).await;
                eprintln!("{} scan after Allow: {outcome:?}, {progress:?}", at());
                assert!(progress.ready.contains(&address), "{progress:?}");
                driver.raw(&["disconnect", &address]).await.unwrap();
            }
        }
        driver.stop_server(Duration::from_secs(2)).await;
    }

    #[tokio::test(start_paused = true)]
    async fn an_old_refusal_loses_its_evidence_and_a_scan_without_an_answer_drops_it() {
        let (driver, session) = untrusted_driver();
        let target = "192.0.2.1:5555";
        driver
            .scan_connect(target, Duration::from_secs(30))
            .await
            .unwrap();
        assert_eq!(
            driver.scan_connection_progress().await.not_allowed,
            [target]
        );
        tokio::time::advance(NOT_ALLOWED_RECENT + Duration::from_secs(1)).await;
        assert!(
            driver
                .scan_connection_progress()
                .await
                .not_allowed
                .is_empty(),
            "an old refusal is no evidence about who is at that address now"
        );
        assert!(driver
            .raw(&["devices"])
            .await
            .unwrap()
            .stdout
            .contains("192.0.2.1:5555\tunauthorized"));
        // A scan that reaches for it and gets no answer drops it.
        session.untrusted.store(false, Ordering::Release);
        session.hold.store(true, Ordering::Release);
        assert_eq!(
            driver
                .scan_connect(target, Duration::from_secs(30))
                .await
                .unwrap(),
            ScanConnectOutcome::Pending
        );
        tokio::time::advance(Duration::from_secs(46)).await;
        while driver.scan_connection_progress().await.failed.is_empty() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(!driver
            .raw(&["devices"])
            .await
            .unwrap()
            .stdout
            .contains("192.0.2.1"));
        session.release.notify_one();
        // So does one whose TV is gone.
        session.hold.store(false, Ordering::Release);
        session.untrusted.store(true, Ordering::Release);
        driver
            .scan_connect(target, Duration::from_secs(30))
            .await
            .unwrap();
        assert!(driver
            .raw(&["devices"])
            .await
            .unwrap()
            .stdout
            .contains("unauthorized"));
        *session.unreachable.lock().unwrap() = Some("SIMULATED no route".into());
        assert!(driver
            .scan_connect(target, Duration::from_secs(30))
            .await
            .is_err());
        assert!(!driver
            .raw(&["devices"])
            .await
            .unwrap()
            .stdout
            .contains("192.0.2.1"));
    }

    #[tokio::test(start_paused = true)]
    async fn a_request_whose_tv_never_answers_is_a_failure_and_never_shown_as_waiting() {
        let (driver, session) = untrusted_driver();
        session.silent.store(true, Ordering::Release);
        let driver = Arc::new(driver);
        let target = "192.0.2.1:5555";
        let requesting = driver.clone();
        let request = tokio::spawn(async move { requesting.request_approval(target).await });
        tokio::time::sleep(APPROVAL_START_WAIT / 2).await;
        let progress = driver.scan_connection_progress().await;
        assert!(
            progress.awaiting_authorization.is_empty(),
            "the TV was never asked"
        );
        assert!(progress.not_allowed.is_empty(), "nothing refused");
        let error = request.await.unwrap().unwrap_err().to_string();
        assert!(error.contains("didn't answer"), "{error}");
        assert!(!error.contains("Allow"), "{error}");
        assert!(!driver.pool.approval_active(target));
        let progress = driver.scan_connection_progress().await;
        assert!(progress.not_allowed.is_empty() && progress.awaiting_authorization.is_empty());
        assert!(!driver
            .raw(&["devices"])
            .await
            .unwrap()
            .stdout
            .contains("192.0.2.1"));
    }

    #[tokio::test(start_paused = true)]
    async fn forgetting_a_request_before_the_tv_was_asked_is_a_cancel_not_a_refusal() {
        let (driver, session) = untrusted_driver();
        session.silent.store(true, Ordering::Release);
        let driver = Arc::new(driver);
        let target = "192.0.2.1:5555";
        let requesting = driver.clone();
        let request = tokio::spawn(async move { requesting.request_approval(target).await });
        tokio::time::sleep(Duration::from_secs(1)).await;
        driver.raw(&["disconnect", target]).await.unwrap();
        assert_eq!(
            request.await.unwrap().unwrap(),
            ApprovalRequestOutcome::Canceled
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_cancel_that_loses_the_race_to_allow_publishes_the_tv() {
        let (driver, session) = untrusted_driver();
        session.stored_on_cancel.store(true, Ordering::Release);
        let target = "192.0.2.1:5555";
        driver
            .scan_connect(target, Duration::from_secs(30))
            .await
            .unwrap();
        assert_eq!(
            driver.request_approval(target).await.unwrap(),
            ApprovalRequestOutcome::Waiting
        );
        driver.cancel_approval(target).await.unwrap();
        let progress = driver.scan_connection_progress().await;
        assert!(progress.not_allowed.is_empty(), "{progress:?}");
        assert!(driver
            .raw(&["devices"])
            .await
            .unwrap()
            .stdout
            .contains("192.0.2.1:5555\tdevice"));
    }

    #[tokio::test(start_paused = true)]
    async fn a_canceled_request_leaves_no_connection_behind() {
        let (driver, session) = untrusted_driver();
        let target = "192.0.2.1:5555";
        assert_eq!(
            driver.request_approval(target).await.unwrap(),
            ApprovalRequestOutcome::Waiting
        );
        driver.cancel_approval(target).await.unwrap();
        assert!(!driver.pool.sessions.lock().await.contains_key(target));
        assert!(!session.ready.load(Ordering::Acquire));
        // It was asked, so the TV is there: not allowed yet, by evidence.
        assert_eq!(
            driver.scan_connection_progress().await.not_allowed,
            [target]
        );
    }

    #[tokio::test(start_paused = true)]
    async fn add_by_ip_never_waits_long_behind_another_connection_or_a_request() {
        let session = Arc::new(FakeSession::default());
        session.hold.store(true, Ordering::Release);
        let captured = session.clone();
        let driver = Arc::new(DirectNetworkAdb::with_factory(
            PathBuf::from("unused"),
            Arc::new(move |_, _| captured.clone()),
        ));
        let target = "192.0.2.1:5555";
        let first = driver.clone();
        let held = tokio::spawn(async move { first.raw(&["connect", target]).await });
        session.started.notified().await;
        let started = Instant::now();
        let error = driver
            .raw(&["connect", target])
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("still in progress"), "{error}");
        assert_eq!(started.elapsed(), FOREGROUND_LOCK_WAIT);
        session.release.notify_one();
        held.await.unwrap().unwrap();

        // A request holding the TV: a quiet attempt yields to it at once.
        let (driver, _session) = untrusted_driver();
        assert_eq!(
            driver.request_approval(target).await.unwrap(),
            ApprovalRequestOutcome::Waiting
        );
        let started = Instant::now();
        assert_eq!(
            driver
                .pool
                .connect(
                    target,
                    Some(Instant::now() + Duration::from_secs(30)),
                    Some(Instant::now() + Duration::from_secs(30)),
                    ConnectAuth::SignaturesOnly,
                )
                .await
                .unwrap(),
            Reached::NotAllowed
        );
        assert!(started.elapsed() <= Duration::from_secs(30));
        assert!(driver.scan_connection_progress().await.failed.is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn add_by_ip_joined_to_a_scan_a_request_replaces_reports_not_allowed() {
        let session = Arc::new(FakeSession::default());
        session.hold.store(true, Ordering::Release);
        let captured = session.clone();
        let driver = Arc::new(DirectNetworkAdb::with_factory(
            PathBuf::from("unused"),
            Arc::new(move |_, _| captured.clone()),
        ));
        let target = "192.0.2.1:5555";
        assert_eq!(
            driver
                .scan_connect(target, Duration::from_secs(30))
                .await
                .unwrap(),
            ScanConnectOutcome::Pending
        );
        let adding = driver.clone();
        let added = tokio::spawn(async move { adding.raw(&["connect", target]).await });
        for _ in 0..5 {
            tokio::task::yield_now().await;
        }
        let requesting = driver.clone();
        let request = tokio::spawn(async move { requesting.request_approval(target).await });
        let output = added.await.unwrap().unwrap();
        assert!(
            output.stdout.starts_with("failed to authenticate"),
            "{}",
            output.stdout
        );
        request.abort();
    }

    #[tokio::test(start_paused = true)]
    async fn scan_returns_promptly_joins_and_publishes_later_approval() {
        let held = Arc::new(FakeSession::default());
        held.hold.store(true, Ordering::Release);
        let count = Arc::new(AtomicU64::new(0));
        let captured = held.clone();
        let created = count.clone();
        let driver = DirectNetworkAdb::with_factory(
            PathBuf::from("unused"),
            Arc::new(move |_, _| {
                if created.fetch_add(1, Ordering::Relaxed) == 0 {
                    captured.clone()
                } else {
                    Arc::new(FakeSession::default())
                }
            }),
        );
        let start = tokio::time::Instant::now();
        assert_eq!(
            driver
                .scan_connect("192.0.2.1:5555", Duration::from_secs(30))
                .await
                .unwrap(),
            ScanConnectOutcome::Pending
        );
        assert_eq!(start.elapsed(), Duration::from_millis(250));
        assert_eq!(
            driver
                .scan_connect("192.0.2.1:5555", Duration::from_secs(30))
                .await
                .unwrap(),
            ScanConnectOutcome::Pending
        );
        assert_eq!(count.load(Ordering::Relaxed), 1);
        assert_eq!(
            driver
                .scan_connect("192.0.2.2:5555", Duration::from_secs(30))
                .await
                .unwrap(),
            ScanConnectOutcome::Ready
        );
        assert_eq!(
            driver.scan_connection_progress().await.pending,
            ["192.0.2.1:5555"]
        );
        assert!(!driver
            .raw(&["devices"])
            .await
            .unwrap()
            .stdout
            .contains("192.0.2.1"));
        held.release.notify_one();
        driver.raw(&["connect", "192.0.2.1:5555"]).await.unwrap();
        let progress = driver.scan_connection_progress().await;
        assert!(progress.pending.is_empty());
        assert!(progress.awaiting_authorization.is_empty());
        assert_eq!(progress.ready, ["192.0.2.1:5555", "192.0.2.2:5555"]);
        assert_eq!(count.load(Ordering::Relaxed), 2);
        assert!(driver
            .raw(&["devices"])
            .await
            .unwrap()
            .stdout
            .contains("192.0.2.1:5555\tdevice"));
    }

    #[tokio::test(start_paused = true)]
    async fn scanning_an_existing_busy_session_is_bounded_and_preserves_its_row() {
        let session = Arc::new(FakeSession::default());
        let captured = session.clone();
        let driver = DirectNetworkAdb::with_factory(
            PathBuf::from("unused"),
            Arc::new(move |_, _| captured.clone()),
        );
        driver.raw(&["connect", "192.0.2.1:5555"]).await.unwrap();
        session.probe_held.store(true, Ordering::Release);
        let start = tokio::time::Instant::now();
        assert_eq!(
            driver
                .scan_connect("192.0.2.1:5555", Duration::from_secs(30))
                .await
                .unwrap(),
            ScanConnectOutcome::Pending
        );
        assert_eq!(start.elapsed(), Duration::from_millis(250));
        assert!(driver.pool.sessions.lock().await["192.0.2.1:5555"]
            .published
            .load(Ordering::Acquire));
        session.probe_held.store(false, Ordering::Release);
        session.release.notify_one();
        driver.raw(&["connect", "192.0.2.1:5555"]).await.unwrap();
        assert_eq!(driver.pool.requests.load(Ordering::Relaxed), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn scan_jobs_and_active_handshakes_have_separate_bounds() {
        let created = Arc::new(StdMutex::new(Vec::<Arc<FakeSession>>::new()));
        let captured = created.clone();
        let driver = DirectNetworkAdb::with_factory(
            PathBuf::from("unused"),
            Arc::new(move |_, _| {
                let session = Arc::new(FakeSession::default());
                session.hold.store(true, Ordering::Release);
                captured.lock().unwrap().push(session.clone());
                session
            }),
        );
        for port in 1..=MAX_SESSIONS {
            assert_eq!(
                driver
                    .scan_connect(&format!("192.0.2.1:{port}"), Duration::from_secs(60))
                    .await
                    .unwrap(),
                ScanConnectOutcome::Pending
            );
        }
        assert_eq!(
            driver.scan_connection_progress().await.pending.len(),
            MAX_SESSIONS
        );
        assert_eq!(created.lock().unwrap().len(), 4);
        assert!(driver
            .scan_connect("192.0.2.2:5555", Duration::from_secs(60))
            .await
            .unwrap_err()
            .to_string()
            .contains("limit reached"));
        driver.stop_server(Duration::ZERO).await;
        assert!(matches!(
            driver
                .scan_connect("192.0.2.2:5555", Duration::from_secs(60))
                .await,
            Err(AdbError::ShuttingDown)
        ));
        let sessions = created.lock().unwrap().clone();
        for session in sessions {
            session.release.notify_one();
            session.completed.notified().await;
            assert!(!session.ready.load(Ordering::Acquire));
        }
    }

    #[tokio::test(start_paused = true)]
    async fn scan_queue_expiry_never_starts_a_handshake() {
        let created = Arc::new(StdMutex::new(Vec::<Arc<FakeSession>>::new()));
        let captured = created.clone();
        let driver = DirectNetworkAdb::with_factory(
            PathBuf::from("unused"),
            Arc::new(move |_, _| {
                let session = Arc::new(FakeSession::default());
                session.hold.store(true, Ordering::Release);
                captured.lock().unwrap().push(session.clone());
                session
            }),
        );
        for port in 1..=4 {
            assert_eq!(
                driver
                    .scan_connect(&format!("192.0.2.1:{port}"), Duration::from_secs(30))
                    .await
                    .unwrap(),
                ScanConnectOutcome::Pending
            );
        }
        let error = driver
            .scan_connect("192.0.2.1:5", Duration::from_millis(50))
            .await
            .unwrap_err();
        assert!(error.to_string().contains("expired while queued"));
        assert_eq!(created.lock().unwrap().len(), 4);
        assert_eq!(driver.scan_connection_progress().await.failed.len(), 1);
        driver.stop_server(Duration::ZERO).await;
        let sessions = created.lock().unwrap().clone();
        for session in sessions {
            session.release.notify_one();
            session.completed.notified().await;
            assert!(!session.ready.load(Ordering::Acquire));
        }
    }

    #[tokio::test(start_paused = true)]
    async fn scan_timeout_is_reported_without_retry_and_late_worker_cannot_publish() {
        let session = Arc::new(FakeSession::default());
        session.hold.store(true, Ordering::Release);
        let captured = session.clone();
        let driver = DirectNetworkAdb::with_factory(
            PathBuf::from("unused"),
            Arc::new(move |_, _| captured.clone()),
        );
        driver
            .scan_connect("192.0.2.1:5555", Duration::from_secs(30))
            .await
            .unwrap();
        tokio::time::advance(Duration::from_secs(45)).await;
        tokio::task::yield_now().await;
        let progress = driver.scan_connection_progress().await;
        assert!(progress.pending.is_empty());
        assert!(progress.failed[0].message.contains("timed out"));
        session.release.notify_one();
        session.completed.notified().await;
        assert!(!session.ready.load(Ordering::Acquire));
        assert_eq!(driver.scan_connection_progress().await, progress);
        session.hold.store(false, Ordering::Release);
        assert_eq!(
            driver
                .scan_connect("192.0.2.1:5555", Duration::from_secs(30))
                .await
                .unwrap(),
            ScanConnectOutcome::Ready
        );
        assert!(driver.scan_connection_progress().await.failed.is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn forgetting_a_pending_scan_prevents_late_publication() {
        let session = Arc::new(FakeSession::default());
        session.hold.store(true, Ordering::Release);
        let captured = session.clone();
        let driver = DirectNetworkAdb::with_factory(
            PathBuf::from("unused"),
            Arc::new(move |_, _| captured.clone()),
        );
        driver
            .scan_connect("192.0.2.1:5555", Duration::from_secs(30))
            .await
            .unwrap();
        driver.raw(&["disconnect", "192.0.2.1:5555"]).await.unwrap();
        session.release.notify_one();
        session.completed.notified().await;
        assert!(!session.ready.load(Ordering::Acquire));
        assert!(driver.pool.sessions.lock().await.is_empty());
        assert!(driver.scan_connection_progress().await.pending.is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn dropping_driver_cancels_owned_scan_without_a_reference_cycle() {
        let session = Arc::new(FakeSession::default());
        session.hold.store(true, Ordering::Release);
        let captured = session.clone();
        let driver = DirectNetworkAdb::with_factory(
            PathBuf::from("unused"),
            Arc::new(move |_, _| captured.clone()),
        );
        driver
            .scan_connect("192.0.2.1:5555", Duration::from_secs(30))
            .await
            .unwrap();
        let pool = Arc::downgrade(&driver.pool);
        drop(driver);
        tokio::task::yield_now().await;
        session.release.notify_one();
        session.completed.notified().await;
        assert!(!session.ready.load(Ordering::Acquire));
        assert!(pool.upgrade().is_none());
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
    async fn silent_nonzero_shell_result_is_an_error_without_replaying_the_command() {
        let (driver, sessions) = fake_driver();
        driver.raw(&["connect", "192.0.2.1:5555"]).await.unwrap();
        let error = driver
            .shell("192.0.2.1:5555", "silent-failure")
            .await
            .unwrap_err();
        assert!(
            matches!(error, AdbError::NonZeroExit { code: Some(7), ref stderr } if stderr.contains("no output") && stderr.contains('7'))
        );
        assert_eq!(
            *sessions.lock().unwrap()[0].calls.lock().unwrap(),
            vec!["192.0.2.1:5555 silent-failure"]
        );
        assert!(driver
            .raw(&["-s", "192.0.2.1:5555", "get-state"])
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn one_tv_authorization_wait_does_not_block_another_tv_or_lose_its_slot() {
        let held = Arc::new(FakeSession::default());
        held.hold.store(true, Ordering::Release);
        let fast = Arc::new(FakeSession::default());
        let next = Arc::new(AtomicU64::new(0));
        let held_factory = held.clone();
        let fast_factory = fast.clone();
        let driver = Arc::new(DirectNetworkAdb::with_factory(
            PathBuf::from("not-created"),
            Arc::new(move |_, _| {
                if next.fetch_add(1, Ordering::Relaxed) == 0 {
                    held_factory.clone()
                } else {
                    fast_factory.clone()
                }
            }),
        ));
        let pending_driver = driver.clone();
        let pending =
            tokio::spawn(async move { pending_driver.raw(&["connect", "192.0.2.1:5555"]).await });
        held.started.notified().await;
        tokio::time::timeout(
            std::time::Duration::from_secs(1),
            driver.raw(&["connect", "192.0.2.2:5555"]),
        )
        .await
        .expect("another TV must not wait for the held approval")
        .unwrap();
        assert_eq!(driver.pool.sessions.lock().await.len(), 2);
        let during = driver.raw(&["devices"]).await.unwrap().stdout;
        assert!(!during.contains("192.0.2.1"));
        assert!(during.contains("192.0.2.2:5555\tdevice"));
        held.release.notify_one();
        pending.await.unwrap().unwrap();
        let after = driver.raw(&["devices"]).await.unwrap().stdout;
        assert!(after.contains("192.0.2.1:5555\tdevice"));
        assert!(after.contains("192.0.2.2:5555\tdevice"));
    }

    #[tokio::test]
    async fn same_target_disconnect_waits_for_its_connect_then_removes_it() {
        let session = Arc::new(FakeSession::default());
        session.hold.store(true, Ordering::Release);
        let captured = session.clone();
        let driver = Arc::new(DirectNetworkAdb::with_factory(
            PathBuf::from("not-created"),
            Arc::new(move |_, _| captured.clone()),
        ));
        let connecting = driver.clone();
        let connect =
            tokio::spawn(async move { connecting.raw(&["connect", "192.0.2.1:5555"]).await });
        session.started.notified().await;
        let disconnecting = driver.clone();
        let disconnect =
            tokio::spawn(async move { disconnecting.raw(&["disconnect", "192.0.2.1:5555"]).await });
        tokio::task::yield_now().await;
        assert!(!disconnect.is_finished());
        session.release.notify_one();
        connect.await.unwrap().unwrap();
        disconnect.await.unwrap().unwrap();
        assert!(driver.pool.sessions.lock().await.is_empty());
        assert_eq!(driver.pool.operations.active(), 0);
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
            vec![
                "-s",
                "192.0.2.2:5555",
                "install-multiple",
                "-r",
                "base.apk",
                "split_config.xhdpi.apk",
            ],
            vec!["-s", "192.0.2.2:5555", "install-multiple", "base.apk"],
        ] {
            driver.raw_transfer(&args).await.unwrap();
        }
        assert!(sessions.lock().unwrap()[0].calls.lock().unwrap().is_empty());
        assert_eq!(
            *sessions.lock().unwrap()[1].calls.lock().unwrap(),
            vec![
                "-s 192.0.2.2:5555 install simulated input.apk",
                "-s 192.0.2.2:5555 install -r simulated input.apk",
                "-s 192.0.2.2:5555 install-multiple -r base.apk split_config.xhdpi.apk",
                "-s 192.0.2.2:5555 install-multiple base.apk",
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
            vec!["install-multiple"],
            vec!["install-multiple", "-r"],
            vec!["install-multiple", "-d", "fixture.apk"],
            vec!["install-multiple", "-r", "base.apk", "-g"],
            vec!["install-multiple", "base.apk", ""],
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
        assert_eq!(driver.pool.sessions.lock().await.len(), 1);
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
        assert_eq!(driver.pool.sessions.lock().await.len(), 1);
    }
    #[tokio::test]
    async fn retiring_a_driver_closes_only_a_counter_it_owns() {
        let fake = || -> Arc<Factory> { Arc::new(|_, _| Arc::new(FakeSession::default())) };
        let owned = DirectNetworkAdb::with_factory(PathBuf::from("unused"), fake());
        let counter = owned.pool.operations.clone();
        drop(owned);
        assert!(counter.try_acquire().is_none());

        let shared = Leases::new();
        let state = atv_optimizer_core::commands::AppState::new(
            Arc::new(DirectNetworkAdb::with_factory_and_operations(
                PathBuf::from("unused"),
                fake(),
                shared.clone(),
                false,
            )),
            atv_optimizer_core::engine::AppListBundle::default(),
            PathBuf::from("not-created"),
        );
        state
            .adb_snapshot()
            .await
            .raw(&["connect", "192.0.2.1:5555"])
            .await
            .unwrap();
        // An engine switch drops the built-in driver; the app keeps working.
        state
            .replace_adb(Arc::new(DirectNetworkAdb::with_factory(
                PathBuf::from("unused"),
                fake(),
            )))
            .await;
        assert!(shared.try_acquire().is_some());
        assert!(state.adb_snapshot().await.raw(&["devices"]).await.is_ok());
        // Quit still closes it.
        state.close_adb_admission();
        assert!(shared.try_acquire().is_none());
    }
    #[tokio::test]
    async fn quit_closes_only_owned_sessions_after_work_drains() {
        let (driver, sessions) = fake_driver();
        driver.raw(&["connect", "192.0.2.1:5555"]).await.unwrap();
        let lease = driver.pool.operations.try_acquire().unwrap();
        assert!(
            matches!(driver.stop_server(std::time::Duration::ZERO).await, ServerStop::Skipped(message) if message.contains("still running"))
        );
        assert!(sessions.lock().unwrap()[0].ready.load(Ordering::Acquire));
        drop(lease);
        assert!(
            matches!(driver.stop_server(std::time::Duration::ZERO).await, ServerStop::Skipped(message) if message.contains("no host server"))
        );
        assert!(driver.pool.sessions.lock().await.is_empty());
        assert_eq!(
            *sessions.lock().unwrap()[0].calls.lock().unwrap(),
            vec!["close owned socket"]
        );
        assert!(driver.pool.operations.try_acquire().is_none());
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
        assert_eq!(driver.pool.sessions.lock().await.len(), 2);
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
        let state = atv_optimizer_core::commands::AppState::new(
            driver.clone(),
            atv_optimizer_core::engine::AppListBundle::default(),
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
            atv_optimizer_core::commands::devices::forget_device_impl(&state, "192.0.2.1:2")
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
    #[tokio::test]
    async fn native_discovery_is_explicit_and_never_dials_or_uses_adb_commands() {
        use super::super::discovery::*;
        struct Provider(AtomicU64);
        impl DiscoveryProvider for Provider {
            fn collect(&self, _: &AtomicBool) -> DiscoveryBatch {
                self.0.fetch_add(1, Ordering::Relaxed);
                DiscoveryBatch {
                    events: Vec::new(),
                    warnings: Vec::new(),
                    cleanup_confirmed: true,
                }
            }
            fn retry_cleanup(&self) -> bool {
                true
            }
        }
        let provider = Arc::new(Provider(AtomicU64::new(0)));
        let mut driver = DirectNetworkAdb::with_factory(
            PathBuf::from("not-created"),
            Arc::new(|_, _| panic!("discovery must never dial or authorize")),
        );
        driver.discovery = DiscoveryManager::new(provider.clone());
        assert_eq!(provider.0.load(Ordering::Relaxed), 0);
        assert!(driver
            .discover_network()
            .await
            .unwrap()
            .advertisements
            .is_empty());
        assert_eq!(provider.0.load(Ordering::Relaxed), 1);
        assert!(driver.pool.sessions.lock().await.is_empty());
        driver.pool.operations.close();
        assert!(matches!(
            driver.discover_network().await,
            Err(AdbError::ShuttingDown)
        ));
        assert_eq!(provider.0.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn native_discovery_shutdown_budget_keeps_unknown_cleanup_and_other_sockets_counted() {
        use super::super::discovery::*;
        struct Slow {
            calls: AtomicU64,
            started: tokio::sync::Notify,
            release: StdMutex<std::sync::mpsc::Receiver<()>>,
        }
        impl DiscoveryProvider for Slow {
            fn collect(&self, _: &AtomicBool) -> DiscoveryBatch {
                DiscoveryBatch {
                    events: Vec::new(),
                    warnings: Vec::new(),
                    cleanup_confirmed: false,
                }
            }
            fn retry_cleanup(&self) -> bool {
                self.calls.fetch_add(1, Ordering::Relaxed);
                self.started.notify_one();
                let _ = self.release.lock().unwrap().recv();
                true
            }
        }
        let (release, release_rx) = std::sync::mpsc::channel();
        let provider = Arc::new(Slow {
            calls: AtomicU64::new(0),
            started: tokio::sync::Notify::new(),
            release: StdMutex::new(release_rx),
        });
        let (mut driver, sessions) = fake_driver();
        driver.discovery = DiscoveryManager::new(provider.clone());
        driver.raw(&["connect", "192.0.2.1:5555"]).await.unwrap();
        driver.discover_network().await.unwrap();
        assert!(matches!(
            driver.stop_server(std::time::Duration::ZERO).await,
            ServerStop::Skipped(_)
        ));
        assert_eq!(provider.calls.load(Ordering::Relaxed), 0);
        let start = std::time::Instant::now();
        assert!(matches!(
            driver
                .stop_server(std::time::Duration::from_millis(10))
                .await,
            ServerStop::Skipped(_)
        ));
        assert!(start.elapsed() < std::time::Duration::from_millis(300));
        tokio::time::timeout(
            std::time::Duration::from_secs(1),
            provider.started.notified(),
        )
        .await
        .unwrap();
        assert_eq!(driver.pool.operations.active(), 1);
        assert!(sessions.lock().unwrap()[0].ready.load(Ordering::Acquire));
        release.send(()).unwrap();
        assert!(
            driver
                .pool
                .operations
                .wait_idle(tokio::time::Instant::now() + std::time::Duration::from_secs(1))
                .await
        );
        driver.stop_server(std::time::Duration::from_secs(1)).await;
        assert!(!sessions.lock().unwrap()[0].ready.load(Ordering::Acquire));
    }

    type Script = fn(std::time::Instant) -> super::super::discovery::DiscoveryBatch;

    /// Hands back one scripted browse per call, sticking on the last.
    struct ScriptedDiscovery {
        calls: AtomicU64,
        script: Vec<Script>,
    }
    impl super::super::discovery::DiscoveryProvider for ScriptedDiscovery {
        fn collect(&self, _: &AtomicBool) -> super::super::discovery::DiscoveryBatch {
            let n = self.calls.fetch_add(1, Ordering::Relaxed) as usize;
            self.script[n.min(self.script.len() - 1)](std::time::Instant::now())
        }
        fn retry_cleanup(&self) -> bool {
            true
        }
    }

    fn scripted(driver: &mut DirectNetworkAdb, script: Vec<Script>) -> Arc<ScriptedDiscovery> {
        let provider = Arc::new(ScriptedDiscovery {
            calls: AtomicU64::new(0),
            script,
        });
        driver.discovery = super::super::discovery::DiscoveryManager::new(provider.clone());
        provider
    }

    fn seen(
        kind: atv_optimizer_core::adb::NativeServiceKind,
        fullname: &str,
        ip: &str,
        port: u16,
        scope: Option<u32>,
        observed: std::time::Instant,
    ) -> super::super::discovery::DiscoveryEvent {
        super::super::discovery::DiscoveryEvent::Resolved {
            kind,
            instance: fullname.into(),
            rows: vec![super::super::discovery::row(
                kind,
                fullname,
                ip.parse().unwrap(),
                port,
                scope.map(|_| "en0".to_string()),
                scope,
                false,
            )],
            observed,
        }
    }

    fn batch(
        events: Vec<super::super::discovery::DiscoveryEvent>,
        warnings: &[&str],
    ) -> super::super::discovery::DiscoveryBatch {
        super::super::discovery::DiscoveryBatch {
            events,
            warnings: warnings.iter().map(|w| w.to_string()).collect(),
            cleanup_confirmed: true,
        }
    }

    /// A TV that paired as `adb-FAKE0001-a1B2c3` and answers `ro.serialno`.
    fn identified_driver(
        peer_instance: Option<&str>,
    ) -> (DirectNetworkAdb, Arc<StdMutex<Vec<Arc<FakeSession>>>>) {
        let sessions = Arc::new(StdMutex::new(Vec::new()));
        let captured = sessions.clone();
        let peer_instance = peer_instance.map(str::to_string);
        let driver = DirectNetworkAdb::with_factory(
            PathBuf::from("not-created-test-key"),
            Arc::new(move |_, _| {
                let session = Arc::new(FakeSession {
                    peer_instance: StdMutex::new(peer_instance.clone()),
                    hardware_id: StdMutex::new(Some("FAKE0001".into())),
                    ..Default::default()
                });
                captured.lock().unwrap().push(session.clone());
                session
            }),
        );
        (driver, sessions)
    }

    #[test]
    fn native_pair_output_names_the_device_only_when_it_gave_a_name() {
        use atv_optimizer_core::adb::pair_output_guid;
        let named = pair_output("192.0.2.5:40000", Some("adb-FAKE0001-a1B2c3"));
        assert_eq!(
            named,
            "Successfully paired to 192.0.2.5:40000 [guid=adb-FAKE0001-a1B2c3]"
        );
        assert_eq!(pair_output_guid(&named), Some("adb-FAKE0001-a1B2c3"));
        let unnamed = pair_output("192.0.2.5:40000", None);
        assert_eq!(unnamed, "Successfully paired to 192.0.2.5:40000");
        assert_eq!(pair_output_guid(&unnamed), None);
    }

    #[tokio::test]
    async fn native_pair_reports_the_peer_name_without_reading_mdns() {
        let (mut driver, sessions) = identified_driver(Some("adb-FAKE0001-a1B2c3"));
        let provider = scripted(&mut driver, vec![|_| batch(Vec::new(), &[])]);
        let driver = Arc::new(driver);
        let state = atv_optimizer_core::commands::AppState::new(
            driver.clone(),
            atv_optimizer_core::engine::AppListBundle::default(),
            PathBuf::from("not-created"),
        );
        let paired = atv_optimizer_core::commands::devices::pair_device_impl(
            &state,
            "192.0.2.5:40000",
            "123456",
        )
        .await
        .unwrap();
        assert!(paired.ok);
        assert_eq!(paired.instance.as_deref(), Some("adb-FAKE0001-a1B2c3"));
        assert_eq!(
            provider.calls.load(Ordering::Relaxed),
            0,
            "pairing must not wait on a multicast browse"
        );
        assert_eq!(
            sessions.lock().unwrap()[0].calls.lock().unwrap().as_slice(),
            ["pair 192.0.2.5:40000"]
        );
        assert_eq!(
            driver.raw(&["devices"]).await.unwrap().stdout,
            "List of devices attached\n",
            "pairing alone never connects"
        );

        let (driver, _) = identified_driver(None);
        let state = atv_optimizer_core::commands::AppState::new(
            Arc::new(driver),
            atv_optimizer_core::engine::AppListBundle::default(),
            PathBuf::from("not-created"),
        );
        let paired = atv_optimizer_core::commands::devices::pair_device_impl(
            &state,
            "192.0.2.5:40000",
            "123456",
        )
        .await
        .unwrap();
        assert!(paired.ok);
        assert_eq!(
            paired.instance, None,
            "no name, so nothing can be tied to it"
        );
    }

    #[tokio::test]
    async fn native_mdns_services_lists_only_fresh_usable_observations() {
        use atv_optimizer_core::adb::{
            parse_mdns_services, NativeServiceKind as Kind, MDNS_SERVICE_CONNECT,
            MDNS_SERVICE_PAIRING,
        };
        let (mut driver, _) = fake_driver();
        let provider = scripted(
            &mut driver,
            vec![
                |now| {
                    let stale = now.checked_sub(Duration::from_secs(31)).unwrap();
                    batch(
                        vec![
                            seen(
                                Kind::TlsConnect,
                                "adb-FAKE0001-xYz123._adb-tls-connect._tcp.local.",
                                "192.0.2.5",
                                41000,
                                None,
                                now,
                            ),
                            seen(
                                Kind::TlsPairing,
                                "adb-FAKE0001-a1B2c3._adb-tls-pairing._tcp.local.",
                                "192.0.2.5",
                                40000,
                                None,
                                now,
                            ),
                            seen(
                                Kind::TlsConnect,
                                "adb-V6SER-c2._adb-tls-connect._tcp.local.",
                                "fe80::1",
                                41541,
                                Some(7),
                                now,
                            ),
                            seen(
                                Kind::TlsConnect,
                                "adb-NOSCOPE-c3._adb-tls-connect._tcp.local.",
                                "fe80::2",
                                41542,
                                None,
                                now,
                            ),
                            seen(
                                Kind::LegacyAdb,
                                "adb-LOOP._adb._tcp.local.",
                                "127.0.0.1",
                                5555,
                                None,
                                now,
                            ),
                            seen(
                                Kind::LegacyAdb,
                                "adb-LINK._adb._tcp.local.",
                                "169.254.1.1",
                                5555,
                                None,
                                now,
                            ),
                            seen(
                                Kind::LegacyAdb,
                                "Living Room TV._adb._tcp.local.",
                                "192.0.2.9",
                                5555,
                                None,
                                now,
                            ),
                            seen(
                                Kind::TlsConnect,
                                "adb-STALE-q._adb-tls-connect._tcp.local.",
                                "192.0.2.6",
                                41000,
                                None,
                                stale,
                            ),
                        ],
                        &[],
                    )
                },
                |_| batch(Vec::new(), &[]),
                |_| batch(Vec::new(), &["Native mDNS discovery could not start: test"]),
            ],
        );

        let out = driver.raw(&["mdns", "services"]).await.unwrap();
        assert!(out.success());
        let services = parse_mdns_services(&out.stdout);
        let mut rows: Vec<(&str, &str, String)> = services
            .iter()
            .map(|s| (s.instance.as_str(), s.service.as_str(), s.endpoint()))
            .collect();
        rows.sort();
        assert_eq!(
            rows,
            vec![
                (
                    "adb-FAKE0001-a1B2c3",
                    MDNS_SERVICE_PAIRING,
                    "192.0.2.5:40000".to_string()
                ),
                (
                    "adb-FAKE0001-xYz123",
                    MDNS_SERVICE_CONNECT,
                    "192.0.2.5:41000".to_string()
                ),
                (
                    "adb-V6SER-c2",
                    MDNS_SERVICE_CONNECT,
                    "[fe80::1%7]:41541".to_string()
                ),
            ],
            "stale, unscoped, loopback, link-local and unprintable rows are left out:\n{}",
            out.stdout
        );
        let v6 = &services
            .iter()
            .find(|s| s.instance == "adb-V6SER-c2")
            .unwrap()
            .endpoint();
        assert_eq!(normalize_connect_address(v6).unwrap(), *v6);
        assert!(
            v6.parse::<std::net::SocketAddr>().is_ok(),
            "the native engine can dial it"
        );
        assert_eq!(provider.calls.load(Ordering::Relaxed), 1);

        let out = driver.raw(&["mdns", "services"]).await.unwrap();
        assert_eq!(
            out.stdout, "List of discovered mdns services\n",
            "a record not re-observed in this browse is not offered"
        );

        let error = driver.raw(&["mdns", "services"]).await.unwrap_err();
        assert!(error.to_string().contains("could not start"), "{error}");
        assert!(
            driver.pool.sessions.lock().await.is_empty(),
            "discovery never dials"
        );

        driver.pool.operations.close();
        assert!(matches!(
            driver.raw(&["mdns", "services"]).await,
            Err(AdbError::ShuttingDown)
        ));
        assert_eq!(provider.calls.load(Ordering::Relaxed), 3);
    }

    #[tokio::test]
    async fn paired_connect_probe_finds_and_verifies_the_tv_through_native_discovery() {
        use atv_optimizer_core::adb::NativeServiceKind as Kind;
        use atv_optimizer_core::commands::devices::{
            connect_paired_impl, probe_paired_connect_impl, PairedConnectProbe,
        };
        const INSTANCE: &str = "adb-FAKE0001-a1B2c3";
        let (mut driver, _) = identified_driver(Some(INSTANCE));
        let provider = scripted(
            &mut driver,
            vec![|_| batch(Vec::new(), &[]), |now| {
                batch(
                    vec![
                        seen(
                            Kind::TlsPairing,
                            "adb-FAKE0001-a1B2c3._adb-tls-pairing._tcp.local.",
                            "192.0.2.5",
                            40000,
                            None,
                            now,
                        ),
                        seen(
                            Kind::TlsConnect,
                            "adb-OTHER9-zZ9._adb-tls-connect._tcp.local.",
                            "192.0.2.5",
                            41001,
                            None,
                            now,
                        ),
                        seen(
                            Kind::TlsConnect,
                            "adb-FAKE0001-xYz123._adb-tls-connect._tcp.local.",
                            "192.0.2.5",
                            41000,
                            None,
                            now,
                        ),
                    ],
                    &[],
                )
            }],
        );
        let driver = Arc::new(driver);
        let state = atv_optimizer_core::commands::AppState::new(
            driver.clone(),
            atv_optimizer_core::engine::AppListBundle::default(),
            PathBuf::from("not-created"),
        );

        assert_eq!(
            probe_paired_connect_impl(&state, "192.0.2.5:40000", None)
                .await
                .unwrap(),
            PairedConnectProbe::Unidentified
        );
        assert_eq!(provider.calls.load(Ordering::Relaxed), 0);

        assert_eq!(
            probe_paired_connect_impl(&state, "192.0.2.5:40000", Some(INSTANCE))
                .await
                .unwrap(),
            PairedConnectProbe::Waiting,
            "nothing advertised yet"
        );
        let probe = probe_paired_connect_impl(&state, "192.0.2.5:40000", Some(INSTANCE))
            .await
            .unwrap();
        assert_eq!(
            probe,
            PairedConnectProbe::Endpoint {
                address: "192.0.2.5:41000".into()
            },
            "the paired TV's connect port, not the pairing port or another TV on that host"
        );

        let connected = connect_paired_impl(&state, "192.0.2.5:41000", INSTANCE)
            .await
            .unwrap();
        assert!(connected.ok, "{}", connected.message);
        assert_eq!(
            probe_paired_connect_impl(&state, "192.0.2.5:40000", Some(INSTANCE))
                .await
                .unwrap(),
            PairedConnectProbe::Attached {
                serial: "192.0.2.5:41000".into()
            }
        );

        let stranger = connect_paired_impl(&state, "192.0.2.5:41000", "adb-OTHER9-zZ9")
            .await
            .unwrap();
        assert!(!stranger.ok);
        assert!(stranger.not_the_paired_device, "{}", stranger.message);
    }
}

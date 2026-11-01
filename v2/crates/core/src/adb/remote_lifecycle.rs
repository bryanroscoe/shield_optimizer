//! Per-device Remote generations and owned session lifecycle.
use super::AdbServiceCancellation;
use async_trait::async_trait;
use std::{
    collections::HashMap,
    future::Future,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex as StdMutex,
    },
};
use tokio::sync::{Mutex, Notify, OwnedMutexGuard};

#[derive(Default)]
pub struct RemoteStartCancellation {
    cancelled: AtomicBool,
    changed: Notify,
}
impl RemoteStartCancellation {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        self.changed.notify_waiters();
    }
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
    pub async fn cancelled(&self) {
        loop {
            let notified = self.changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.is_cancelled() {
                return;
            }
            notified.await;
        }
    }
}

#[async_trait]
pub trait RemoteSessionIo: Send {
    fn cancellation(&self) -> Arc<dyn AdbServiceCancellation>;
    async fn send_key(&mut self, keycode: u32, down: bool) -> Result<(), String>;
    async fn send_key_press(&mut self, keycode: u32) -> Result<(), String>;
    async fn send_text(&mut self, text: &str) -> Result<(), String>;
    async fn close(self: Box<Self>) -> Result<(), String>;
}

struct TaggedSession {
    generation: u64,
    session: Box<dyn RemoteSessionIo>,
}
#[derive(Default)]
struct Cell {
    generation: AtomicU64,
    revision: AtomicU64,
    retired: AtomicBool,
    publication: StdMutex<()>,
    operation: Arc<Mutex<()>>,
    session: Mutex<Option<TaggedSession>>,
    live_cancel: StdMutex<Option<(u64, Arc<dyn AdbServiceCancellation>)>>,
    start_cancel: StdMutex<Option<(u64, Arc<RemoteStartCancellation>)>>,
}
#[derive(Default)]
pub struct RemoteRegistry {
    cells: StdMutex<HashMap<String, Arc<Cell>>>,
    next: AtomicU64,
    closed: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
}

fn lock<T>(value: &StdMutex<T>) -> std::sync::MutexGuard<'_, T> {
    value
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
// Keep checked increments compatible with Rust toolchains before and after
// AtomicU64::fetch_update was deprecated, without weakening overflow checks.
fn checked_increment(value: &AtomicU64) -> Result<u64, u64> {
    let mut current = value.load(Ordering::Acquire);
    loop {
        let next = current.checked_add(1).ok_or(current)?;
        match value.compare_exchange_weak(current, next, Ordering::AcqRel, Ordering::Acquire) {
            Ok(previous) => return Ok(previous),
            Err(actual) => current = actual,
        }
    }
}
fn invalidate(cell: &Cell) {
    if checked_increment(&cell.revision).is_err() {
        cell.retired.store(true, Ordering::Release);
    }
}
const MAX_REMOTE_TVS: usize = 64;
impl RemoteRegistry {
    fn cell(&self, serial: &str) -> Result<Arc<Cell>, String> {
        if serial.is_empty() || serial.len() > 1024 {
            return Err("Invalid Remote device identifier.".into());
        }
        let mut cells = lock(&self.cells);
        cells.retain(|_, cell| {
            !(cell.generation.load(Ordering::Acquire) == 0
                && Arc::strong_count(cell) == 1
                && cell.operation.try_lock().is_ok()
                && cell
                    .session
                    .try_lock()
                    .is_ok_and(|session| session.is_none())
                && lock(&cell.start_cancel).is_none())
        });
        if !cells.contains_key(serial) && cells.len() >= MAX_REMOTE_TVS {
            return Err("Remote device limit reached; close an existing Remote first.".into());
        }
        Ok(cells.entry(serial.into()).or_default().clone())
    }
    pub fn begin(&self, serial: &str) -> Result<u64, String> {
        if self.closed.load(Ordering::Acquire) || self.paused.load(Ordering::Acquire) {
            return Err("Remote admission is closed.".into());
        }
        let cell = self.cell(serial)?;
        let generation =
            checked_increment(&self.next).map_err(|_| "Remote generation limit reached.")? + 1;
        // Publication and logical invalidation share this short lock.
        let _publication = lock(&cell.publication);
        if self.closed.load(Ordering::Acquire) || self.paused.load(Ordering::Acquire) {
            return Err("Remote admission is closed.".into());
        }
        if cell.retired.load(Ordering::Acquire) {
            return Err("Remote cell revision limit reached.".into());
        }
        invalidate(&cell);
        cell.generation.store(generation, Ordering::Release);
        if let Some((_, cancel)) = lock(&cell.start_cancel).as_ref() {
            cancel.cancel();
        }
        Ok(generation)
    }
    pub fn capture(&self, serial: &str) -> Result<u64, String> {
        let cell = self.cell(serial)?;
        let _publication = lock(&cell.publication);
        self.validate_cell(&cell, None)?;
        let current = cell.generation.load(Ordering::Acquire);
        if current != 0 {
            return Ok(current);
        }
        let generation =
            checked_increment(&self.next).map_err(|_| "Remote generation limit reached.")? + 1;
        cell.generation.store(generation, Ordering::Release);
        Ok(generation)
    }
    pub fn validate(&self, serial: &str, generation: u64) -> Result<(), String> {
        let cell = self.cell(serial)?;
        self.validate_cell(&cell, Some(generation))
    }
    fn validate_cell(&self, cell: &Cell, generation: Option<u64>) -> Result<(), String> {
        if self.closed.load(Ordering::Acquire)
            || self.paused.load(Ordering::Acquire)
            || cell.retired.load(Ordering::Acquire)
        {
            return Err("Remote admission is closed.".into());
        }
        if generation.is_some_and(|generation| {
            generation == 0 || cell.generation.load(Ordering::Acquire) != generation
        }) {
            return Err("Remote generation is no longer active.".into());
        }
        Ok(())
    }
    pub async fn operation(
        &self,
        serial: &str,
        generation: Option<u64>,
    ) -> Result<OwnedMutexGuard<()>, String> {
        let cell = self.cell(serial)?;
        let revision = cell.revision.load(Ordering::Acquire);
        let guard = cell.operation.clone().lock_owned().await;
        self.validate_cell(&cell, generation)?;
        if cell.revision.load(Ordering::Acquire) != revision {
            return Err("Remote operation became stale while queued.".into());
        }
        Ok(guard)
    }
    pub fn pause(&self) {
        self.paused.store(true, Ordering::Release);
    }
    pub fn resume(&self) {
        self.paused.store(false, Ordering::Release);
    }
    pub fn refuse_new(&self) {
        self.closed.store(true, Ordering::Release);
    }

    pub async fn ensure<F, Fut>(
        &self,
        serial: &str,
        generation: Option<u64>,
        factory: F,
    ) -> Result<(), String>
    where
        F: FnOnce(Arc<RemoteStartCancellation>) -> Fut + Send + 'static,
        Fut: Future<Output = Result<Box<dyn RemoteSessionIo>, String>> + Send + 'static,
    {
        let cell = self.cell(serial)?;
        self.validate_cell(cell.as_ref(), generation)?;
        let revision = cell.revision.load(Ordering::Acquire);
        let cancel = Arc::new(RemoteStartCancellation::default());
        struct Attempt {
            cell: Arc<Cell>,
            generation: Option<u64>,
            cancel: Arc<RemoteStartCancellation>,
            complete: bool,
            started: Arc<AtomicBool>,
        }
        impl Drop for Attempt {
            fn drop(&mut self) {
                if !self.complete {
                    self.cancel.cancel();
                    if let Some(generation) = self
                        .generation
                        .filter(|_| self.started.load(Ordering::Acquire))
                    {
                        let _publication = lock(&self.cell.publication);
                        if self
                            .cell
                            .generation
                            .compare_exchange(generation, 0, Ordering::AcqRel, Ordering::Acquire)
                            .is_ok()
                        {
                            invalidate(&self.cell);
                        }
                        if let Some((tag, cancel)) = lock(&self.cell.live_cancel).as_ref() {
                            if *tag == generation {
                                cancel.cancel();
                            }
                        }
                    }
                }
            }
        }
        let started = Arc::new(AtomicBool::new(false));
        let mut attempt = Attempt {
            cell: cell.clone(),
            generation,
            cancel: cancel.clone(),
            complete: false,
            started: started.clone(),
        };
        let closed = self.closed.clone();
        let paused = self.paused.clone();
        let worker = tokio::spawn(async move {
            let _operation = cell.operation.clone().lock_owned().await;
            if closed.load(Ordering::Acquire)
                || paused.load(Ordering::Acquire)
                || cell.retired.load(Ordering::Acquire)
                || cell.revision.load(Ordering::Acquire) != revision
                || cancel.is_cancelled()
                || generation.is_some_and(|g| cell.generation.load(Ordering::Acquire) != g)
            {
                return Err("Remote startup was cancelled.".into());
            }
            let tag = generation.unwrap_or_else(|| cell.generation.load(Ordering::Acquire));
            {
                let slot = cell.session.lock().await;
                if slot
                    .as_ref()
                    .is_some_and(|session| session.generation == tag)
                {
                    return Ok(());
                }
            }
            let previous = cell.session.lock().await.take();
            if let Some(previous) = previous {
                lock(&cell.live_cancel).take();
                if let Err(error) = previous.session.close().await {
                    let _publication = lock(&cell.publication);
                    let _ = cell.generation.compare_exchange(
                        tag,
                        0,
                        Ordering::AcqRel,
                        Ordering::Acquire,
                    );
                    return Err(error);
                }
            }
            if cell.retired.load(Ordering::Acquire)
                || cell.revision.load(Ordering::Acquire) != revision
                || cancel.is_cancelled()
                || closed.load(Ordering::Acquire)
                || paused.load(Ordering::Acquire)
                || generation.is_some_and(|g| cell.generation.load(Ordering::Acquire) != g)
            {
                return Err("Remote startup was cancelled.".into());
            }
            started.store(true, Ordering::Release);
            *lock(&cell.start_cancel) = Some((tag, cancel.clone()));
            let result = tokio::select! { _=cancel.cancelled()=>Err("Remote startup was cancelled.".into()),result=factory(cancel.clone())=>result };
            {
                let mut current = lock(&cell.start_cancel);
                if current
                    .as_ref()
                    .is_some_and(|(_, active)| Arc::ptr_eq(active, &cancel))
                {
                    current.take();
                }
            }
            let mut session = Some(result?);
            let published = {
                let mut slot = cell.session.lock().await;
                let _publication = lock(&cell.publication);
                if closed.load(Ordering::Acquire)
                    || paused.load(Ordering::Acquire)
                    || cell.retired.load(Ordering::Acquire)
                    || cell.revision.load(Ordering::Acquire) != revision
                    || cancel.is_cancelled()
                    || generation.is_some_and(|g| cell.generation.load(Ordering::Acquire) != g)
                {
                    false
                } else {
                    let ready = session.take().unwrap();
                    *lock(&cell.live_cancel) = Some((tag, ready.cancellation()));
                    *slot = Some(TaggedSession {
                        generation: tag,
                        session: ready,
                    });
                    true
                }
            };
            if !published {
                let _ = session.take().unwrap().close().await;
                return Err("Remote startup was cancelled before publication.".into());
            }
            Ok(())
        });
        let result = worker
            .await
            .map_err(|error| format!("Remote startup worker failed: {error}"));
        if result.is_ok() {
            attempt.complete = true;
        }
        result?
    }

    pub async fn send(
        &self,
        serial: &str,
        generation: Option<u64>,
        action: RemoteAction,
    ) -> Result<(), String> {
        let cell = self.cell(serial)?;
        let revision = cell.revision.load(Ordering::Acquire);
        let _operation = cell.operation.clone().lock_owned().await;
        self.validate_cell(&cell, generation)?;
        if cell.revision.load(Ordering::Acquire) != revision {
            return Err("Remote input became stale while queued.".into());
        }
        let mut slot = cell.session.lock().await;
        let tagged = slot.as_mut().ok_or("No active Remote session.")?;
        if generation.is_some_and(|generation| generation != tagged.generation) {
            return Err("Remote generation is no longer active.".into());
        }
        match action {
            RemoteAction::Key(code, down) => tagged.session.send_key(code, down).await,
            RemoteAction::Press(code) => tagged.session.send_key_press(code).await,
            RemoteAction::Text(text) => tagged.session.send_text(&text).await,
        }
    }

    fn prepare_close(cell: Arc<Cell>, generation: Option<u64>) -> ClosePlan {
        let _publication = lock(&cell.publication);
        let stale = generation
            .is_some_and(|generation| cell.generation.load(Ordering::Acquire) != generation);
        if !stale {
            invalidate(&cell);
            cell.generation.store(0, Ordering::Release);
        }
        if let Some((tag, cancel)) = lock(&cell.start_cancel).as_ref() {
            if !stale || generation == Some(*tag) {
                cancel.cancel();
            }
        }
        let target = lock(&cell.live_cancel)
            .as_ref()
            .and_then(|(tag, _)| (!stale || generation == Some(*tag)).then_some(*tag));
        drop(_publication);
        let guard = cell.operation.clone().try_lock_owned().ok();
        if guard.is_none() {
            if let Some((tag, cancel)) = lock(&cell.live_cancel).as_ref() {
                if target == Some(*tag) {
                    cancel.cancel();
                }
            }
        }
        ClosePlan {
            cell,
            target,
            guard,
        }
    }
    pub async fn close(&self, serial: &str, generation: Option<u64>) -> Result<(), String> {
        let cell = { lock(&self.cells).get(serial).cloned() };
        let Some(cell) = cell else {
            return Ok(());
        };
        let plan = Self::prepare_close(cell, generation);
        tokio::spawn(plan.run())
            .await
            .map_err(|error| format!("Remote cleanup worker failed: {error}"))?
    }
    pub async fn close_all(&self) {
        let cells: Vec<_> = lock(&self.cells).values().cloned().collect();
        let tasks: Vec<_> = cells
            .into_iter()
            .map(|cell| tokio::spawn(Self::prepare_close(cell, None).run()))
            .collect();
        for task in tasks {
            if let Ok(Err(error)) = task.await {
                tracing::warn!(%error,"Remote cleanup incomplete");
            }
        }
    }
}
struct ClosePlan {
    cell: Arc<Cell>,
    target: Option<u64>,
    guard: Option<OwnedMutexGuard<()>>,
}
impl ClosePlan {
    async fn run(self) -> Result<(), String> {
        let _operation = match self.guard {
            Some(guard) => guard,
            None => self.cell.operation.clone().lock_owned().await,
        };
        let mut slot = self.cell.session.lock().await;
        if !slot
            .as_ref()
            .is_some_and(|session| Some(session.generation) == self.target)
        {
            return Ok(());
        }
        let session = slot.take().unwrap();
        drop(slot);
        lock(&self.cell.live_cancel).take();
        session.session.close().await
    }
}
pub enum RemoteAction {
    Key(u32, bool),
    Press(u32),
    Text(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    #[test]
    fn checked_increment_preserves_overflow_boundary() {
        let value = AtomicU64::new(u64::MAX - 1);
        assert_eq!(checked_increment(&value), Ok(u64::MAX - 1));
        assert_eq!(checked_increment(&value), Err(u64::MAX));
        assert_eq!(value.load(Ordering::Acquire), u64::MAX);
    }

    #[test]
    fn checked_increment_does_not_lose_concurrent_updates() {
        let value = AtomicU64::new(0);
        std::thread::scope(|scope| {
            for _ in 0..4 {
                let value = &value;
                scope.spawn(move || {
                    for _ in 0..1000 {
                        checked_increment(value).unwrap();
                    }
                });
            }
        });
        assert_eq!(value.load(Ordering::Acquire), 4000);
    }

    struct Cancellation(Arc<RemoteStartCancellation>);
    impl AdbServiceCancellation for Cancellation {
        fn cancel(&self) {
            self.0.cancel();
        }
    }
    struct Fixture {
        name: &'static str,
        _lease: Option<crate::adb::Lease>,
        events: Arc<StdMutex<Vec<String>>>,
        cancel: Arc<RemoteStartCancellation>,
        block_send: Option<Arc<Notify>>,
        send_started: Arc<Notify>,
        block_close: Option<Arc<Notify>>,
        close_started: Arc<Notify>,
    }
    impl Fixture {
        fn simple(name: &'static str, events: Arc<StdMutex<Vec<String>>>) -> Self {
            Self {
                name,
                _lease: None,
                events,
                cancel: Arc::new(RemoteStartCancellation::default()),
                block_send: None,
                send_started: Arc::new(Notify::new()),
                block_close: None,
                close_started: Arc::new(Notify::new()),
            }
        }
    }
    #[async_trait]
    impl RemoteSessionIo for Fixture {
        fn cancellation(&self) -> Arc<dyn AdbServiceCancellation> {
            Arc::new(Cancellation(self.cancel.clone()))
        }
        async fn send_key(&mut self, key: u32, down: bool) -> Result<(), String> {
            lock(&self.events).push(format!("{}:key:{key}:{down}", self.name));
            Ok(())
        }
        async fn send_key_press(&mut self, key: u32) -> Result<(), String> {
            lock(&self.events).push(format!("{}:press:{key}", self.name));
            self.send_started.notify_one();
            if let Some(release) = &self.block_send {
                tokio::select! {_=release.notified()=>{},_=self.cancel.cancelled()=>return Err("delivery uncertain".into())}
            }
            Ok(())
        }
        async fn send_text(&mut self, text: &str) -> Result<(), String> {
            lock(&self.events).push(format!("{}:text:{text}", self.name));
            Ok(())
        }
        async fn close(self: Box<Self>) -> Result<(), String> {
            self.close_started.notify_one();
            if let Some(release) = &self.block_close {
                release.notified().await;
            }
            lock(&self.events).push(format!("{}:close", self.name));
            Ok(())
        }
    }
    async fn ready(registry: &RemoteRegistry, tv: &str, generation: u64, session: Fixture) {
        registry
            .ensure(tv, Some(generation), move |_| async move {
                Ok(Box::new(session) as Box<dyn RemoteSessionIo>)
            })
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn logical_generations_are_unique_bounded_and_refuse_closed_admission() {
        let registry = RemoteRegistry::default();
        let a = registry.begin("A").unwrap();
        let b = registry.begin("B").unwrap();
        assert_ne!(a, b);
        assert!(registry.validate("B", a).is_err());
        for n in 2..MAX_REMOTE_TVS {
            registry.begin(&format!("tv-{n}")).unwrap();
        }
        assert!(registry.begin("over-limit").is_err());
        registry.close_all().await;
        assert!(registry.begin("after-close").is_ok());
        registry.refuse_new();
        assert!(registry.begin("closed").is_err());
    }
    #[tokio::test]
    async fn stale_close_and_stale_send_never_touch_a_replacement_generation() {
        let registry = RemoteRegistry::default();
        let events = Arc::new(StdMutex::new(Vec::new()));
        let old = registry.begin("A").unwrap();
        ready(&registry, "A", old, Fixture::simple("old", events.clone())).await;
        let fresh = registry.begin("A").unwrap();
        ready(
            &registry,
            "A",
            fresh,
            Fixture::simple("fresh", events.clone()),
        )
        .await;
        registry.close("A", Some(old)).await.unwrap();
        assert!(registry
            .send("A", Some(old), RemoteAction::Press(1))
            .await
            .is_err());
        registry
            .send("A", Some(fresh), RemoteAction::Press(2))
            .await
            .unwrap();
        registry.close("A", Some(fresh)).await.unwrap();
        registry.close("A", Some(fresh)).await.unwrap();
        assert_eq!(
            *lock(&events),
            vec!["old:close", "fresh:press:2", "fresh:close"]
        );
    }
    #[tokio::test]
    async fn queued_compatible_operation_rechecks_generation_after_lock_acquisition() {
        let registry = Arc::new(RemoteRegistry::default());
        let old = registry.begin("A").unwrap();
        let held = registry.operation("A", Some(old)).await.unwrap();
        let queued = tokio::spawn({
            let registry = registry.clone();
            async move { registry.operation("A", Some(old)).await }
        });
        tokio::task::yield_now().await;
        let fresh = registry.begin("A").unwrap();
        drop(held);
        assert!(queued.await.unwrap().is_err());
        assert!(registry.operation("A", Some(fresh)).await.is_ok());
    }
    #[tokio::test]
    async fn cancelling_a_queued_duplicate_start_does_not_cancel_the_winner() {
        let registry = Arc::new(RemoteRegistry::default());
        let generation = registry.begin("A").unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let events = Arc::new(StdMutex::new(Vec::new()));
        let started = Arc::new(Notify::new());
        let release = Arc::new(Notify::new());
        let winner = tokio::spawn({
            let registry = registry.clone();
            let calls = calls.clone();
            let started = started.clone();
            let release = release.clone();
            let events = events.clone();
            async move {
                registry
                    .ensure("A", Some(generation), move |_| async move {
                        calls.fetch_add(1, Ordering::Relaxed);
                        started.notify_one();
                        release.notified().await;
                        Ok(Box::new(Fixture::simple("winner", events)) as Box<dyn RemoteSessionIo>)
                    })
                    .await
            }
        });
        started.notified().await;
        let loser = tokio::spawn({
            let registry = registry.clone();
            async move {
                registry
                    .ensure("A", Some(generation), move |_| async move {
                        panic!("duplicate must not start");
                        #[allow(unreachable_code)]
                        Ok(Box::new(Fixture::simple(
                            "unused",
                            Arc::new(StdMutex::new(Vec::new())),
                        )) as Box<dyn RemoteSessionIo>)
                    })
                    .await
            }
        });
        tokio::task::yield_now().await;
        loser.abort();
        let _ = loser.await;
        release.notify_one();
        winner.await.unwrap().unwrap();
        registry.validate("A", generation).unwrap();
        assert_eq!(calls.load(Ordering::Relaxed), 1);
        registry
            .send("A", Some(generation), RemoteAction::Press(1))
            .await
            .unwrap();
        registry.close_all().await;
    }
    #[tokio::test]
    async fn close_cancels_startup_and_no_late_session_can_publish() {
        struct Resource(Arc<AtomicBool>);
        impl Drop for Resource {
            fn drop(&mut self) {
                self.0.store(true, Ordering::Release);
            }
        }
        let registry = Arc::new(RemoteRegistry::default());
        let generation = registry.begin("A").unwrap();
        let started = Arc::new(Notify::new());
        let dropped = Arc::new(AtomicBool::new(false));
        let startup = tokio::spawn({
            let registry = registry.clone();
            let started = started.clone();
            let dropped = dropped.clone();
            async move {
                registry
                    .ensure("A", Some(generation), move |_| async move {
                        let _resource = Resource(dropped);
                        started.notify_one();
                        std::future::pending::<Result<Box<dyn RemoteSessionIo>, String>>().await
                    })
                    .await
            }
        });
        started.notified().await;
        registry.close("A", Some(generation)).await.unwrap();
        assert!(startup.await.unwrap().is_err());
        assert!(dropped.load(Ordering::Acquire));
        assert!(registry.validate("A", generation).is_err());
        assert!(registry
            .send("A", Some(generation), RemoteAction::Press(1))
            .await
            .is_err());
    }
    #[tokio::test]
    async fn blocked_tv_does_not_block_another_tv_and_close_interrupts_only_its_session() {
        let registry = Arc::new(RemoteRegistry::default());
        let events = Arc::new(StdMutex::new(Vec::new()));
        let a = registry.begin("A").unwrap();
        let b = registry.begin("B").unwrap();
        let mut blocked = Fixture::simple("A", events.clone());
        blocked.block_send = Some(Arc::new(Notify::new()));
        let started = blocked.send_started.clone();
        ready(&registry, "A", a, blocked).await;
        ready(&registry, "B", b, Fixture::simple("B", events.clone())).await;
        let send = tokio::spawn({
            let registry = registry.clone();
            async move { registry.send("A", Some(a), RemoteAction::Press(1)).await }
        });
        started.notified().await;
        tokio::time::timeout(
            std::time::Duration::from_secs(1),
            registry.send("B", Some(b), RemoteAction::Press(2)),
        )
        .await
        .unwrap()
        .unwrap();
        registry.close("A", Some(a)).await.unwrap();
        assert!(send.await.unwrap().is_err());
        registry
            .send("B", Some(b), RemoteAction::Press(3))
            .await
            .unwrap();
        registry.close_all().await;
    }
    #[tokio::test]
    async fn cancelled_close_caller_does_not_release_transition_before_cleanup_finishes() {
        let registry = Arc::new(RemoteRegistry::default());
        let events = Arc::new(StdMutex::new(Vec::new()));
        let old = registry.begin("A").unwrap();
        let mut session = Fixture::simple("old", events.clone());
        let release = Arc::new(Notify::new());
        session.block_close = Some(release.clone());
        let closing = session.close_started.clone();
        ready(&registry, "A", old, session).await;
        let close = tokio::spawn({
            let registry = registry.clone();
            async move { registry.close("A", Some(old)).await }
        });
        closing.notified().await;
        close.abort();
        let _ = close.await;
        let fresh = registry.begin("A").unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let replacement = tokio::spawn({
            let registry = registry.clone();
            let calls = calls.clone();
            let events = events.clone();
            async move {
                registry
                    .ensure("A", Some(fresh), move |_| async move {
                        calls.fetch_add(1, Ordering::Relaxed);
                        Ok(Box::new(Fixture::simple("fresh", events)) as Box<dyn RemoteSessionIo>)
                    })
                    .await
            }
        });
        tokio::task::yield_now().await;
        assert_eq!(calls.load(Ordering::Relaxed), 0);
        release.notify_one();
        replacement.await.unwrap().unwrap();
        registry.close("A", Some(old)).await.unwrap();
        registry
            .send("A", Some(fresh), RemoteAction::Press(1))
            .await
            .unwrap();
        registry.close_all().await;
    }
    #[tokio::test]
    async fn paused_driver_swap_invalidates_and_drains_existing_sessions() {
        let registry = RemoteRegistry::default();
        let events = Arc::new(StdMutex::new(Vec::new()));
        let old = registry.begin("A").unwrap();
        ready(&registry, "A", old, Fixture::simple("old", events.clone())).await;
        registry.pause();
        assert!(registry.begin("B").is_err());
        registry.close_all().await;
        assert!(registry.validate("A", old).is_err());
        registry.resume();
        let fresh = registry.begin("A").unwrap();
        assert_ne!(old, fresh);
        assert_eq!(*lock(&events), vec!["old:close"]);
    }
    #[tokio::test]
    async fn legacy_capture_reuses_current_token_until_close() {
        let registry = Arc::new(RemoteRegistry::default());
        let first = registry.capture("A").unwrap();
        let other = tokio::spawn({
            let registry = registry.clone();
            async move { registry.capture("A").unwrap() }
        });
        assert_eq!(first, other.await.unwrap());
        registry.close("A", None).await.unwrap();
        assert!(registry.validate("A", first).is_err());
        assert_ne!(first, registry.capture("A").unwrap());
    }
    #[tokio::test]
    async fn close_invalidates_queued_legacy_start_before_factory() {
        let registry = Arc::new(RemoteRegistry::default());
        let cell = registry.cell("A").unwrap();
        let held = cell.operation.clone().lock_owned().await;
        let calls = Arc::new(AtomicUsize::new(0));
        let start = tokio::spawn({
            let registry = registry.clone();
            let calls = calls.clone();
            async move {
                registry
                    .ensure("A", None, move |_| async move {
                        calls.fetch_add(1, Ordering::Relaxed);
                        Err("unexpected factory".into())
                    })
                    .await
            }
        });
        tokio::task::yield_now().await;
        let close = tokio::spawn({
            let registry = registry.clone();
            async move { registry.close("A", None).await }
        });
        tokio::task::yield_now().await;
        drop(held);
        assert!(start.await.unwrap().is_err());
        close.await.unwrap().unwrap();
        assert_eq!(calls.load(Ordering::Relaxed), 0);
    }
    #[tokio::test]
    async fn close_invalidates_legacy_start_waiting_for_previous_cleanup() {
        let registry = Arc::new(RemoteRegistry::default());
        let events = Arc::new(StdMutex::new(Vec::new()));
        let old = registry.begin("A").unwrap();
        let mut session = Fixture::simple("old", events);
        let release = Arc::new(Notify::new());
        session.block_close = Some(release.clone());
        let closing = session.close_started.clone();
        ready(&registry, "A", old, session).await;
        registry.begin("A").unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let start = tokio::spawn({
            let registry = registry.clone();
            let calls = calls.clone();
            async move {
                registry
                    .ensure("A", None, move |_| async move {
                        calls.fetch_add(1, Ordering::Relaxed);
                        Err("unexpected factory".into())
                    })
                    .await
            }
        });
        closing.notified().await;
        let close = tokio::spawn({
            let registry = registry.clone();
            async move { registry.close("A", None).await }
        });
        tokio::task::yield_now().await;
        release.notify_one();
        assert!(start.await.unwrap().is_err());
        close.await.unwrap().unwrap();
        assert_eq!(calls.load(Ordering::Relaxed), 0);
    }
    #[tokio::test]
    async fn closed_admission_cleanup_cancels_startup_and_drains_owned_leases() {
        let registry = Arc::new(RemoteRegistry::default());
        let leases = crate::adb::Leases::new();
        let events = Arc::new(StdMutex::new(Vec::new()));
        let a = registry.begin("A").unwrap();
        let mut session = Fixture::simple("ready", events.clone());
        session._lease = leases.try_acquire();
        ready(&registry, "A", a, session).await;
        let b = registry.begin("B").unwrap();
        let started = Arc::new(Notify::new());
        let pending_lease = leases.try_acquire().unwrap();
        let startup = tokio::spawn({
            let registry = registry.clone();
            let started = started.clone();
            async move {
                registry
                    .ensure("B", Some(b), move |cancel| async move {
                        let _lease = pending_lease;
                        started.notify_one();
                        cancel.cancelled().await;
                        Err("cancelled".into())
                    })
                    .await
            }
        });
        started.notified().await;
        assert_eq!(leases.active(), 2);
        registry.refuse_new();
        leases.close();
        registry.close_all().await;
        assert!(startup.await.unwrap().is_err());
        assert_eq!(leases.active(), 0);
        assert_eq!(*lock(&events), vec!["ready:close"]);
        assert!(registry.capture("C").is_err());
    }
    #[tokio::test]
    async fn stale_close_retires_only_its_live_session_before_new_startup() {
        let registry = RemoteRegistry::default();
        let events = Arc::new(StdMutex::new(Vec::new()));
        let a = registry.begin("A").unwrap();
        ready(&registry, "A", a, Fixture::simple("old", events.clone())).await;
        let b = registry.begin("A").unwrap();
        registry.close("A", Some(a)).await.unwrap();
        registry.validate("A", b).unwrap();
        assert_eq!(*lock(&events), vec!["old:close"]);
        registry.close("A", Some(b)).await.unwrap();
        assert_eq!(*lock(&events), vec!["old:close"]);
    }
    #[tokio::test]
    async fn closing_current_generation_also_retires_inherited_previous_session() {
        let registry = RemoteRegistry::default();
        let events = Arc::new(StdMutex::new(Vec::new()));
        let a = registry.begin("A").unwrap();
        ready(&registry, "A", a, Fixture::simple("old", events.clone())).await;
        let b = registry.begin("A").unwrap();
        registry.close("A", Some(b)).await.unwrap();
        assert_eq!(*lock(&events), vec!["old:close"]);
        assert!(registry.validate("A", b).is_err());
    }
}

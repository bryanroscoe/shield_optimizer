use super::*;
use std::sync::{
    atomic::{AtomicBool, AtomicUsize},
    Condvar,
};
use tokio::sync::Notify;

#[derive(Default)]
struct MemoryStore {
    saved: Mutex<Vec<Preference>>,
    entered: Notify,
    release: (Mutex<bool>, Condvar),
    block: AtomicBool,
    fail: AtomicBool,
}
impl Store for MemoryStore {
    fn persist(&self, preference: Preference) -> Result<(), Error> {
        self.entered.notify_one();
        if self.block.load(Ordering::SeqCst) {
            let (lock, condition) = &self.release;
            let mut released = lock.lock().unwrap();
            while !*released {
                released = condition.wait(released).unwrap();
            }
        }
        if self.fail.load(Ordering::SeqCst) {
            return Err(Error::Store);
        }
        self.saved.lock().unwrap().push(preference);
        Ok(())
    }
}
#[derive(Default)]
struct LocalResources {
    children: Mutex<Vec<ChildLease>>,
    calls: AtomicUsize,
    hold: AtomicBool,
    entered: Notify,
    release: Notify,
}
#[async_trait]
impl Resources for LocalResources {
    async fn close_local_and_reap(&self) {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.entered.notify_one();
        if self.hold.load(Ordering::SeqCst) {
            self.release.notified().await;
        }
        for child in self.children.lock().unwrap().drain(..) {
            child.reaped();
        }
    }
}
struct FakeStop {
    calls: AtomicUsize,
    result: StopResult,
    hold: bool,
    entered: Notify,
}
#[async_trait]
impl Stop for FakeStop {
    async fn execute(&self) -> StopResult {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.entered.notify_one();
        if self.hold {
            std::future::pending::<()>().await;
        }
        self.result
    }
}
fn setup(
    result: StopResult,
) -> (
    Arc<Coordinator>,
    Arc<MemoryStore>,
    Arc<LocalResources>,
    Arc<FakeStop>,
) {
    let store = Arc::new(MemoryStore::default());
    let resources = Arc::new(LocalResources::default());
    let stop = Arc::new(FakeStop {
        calls: AtomicUsize::new(0),
        result,
        hold: false,
        entered: Notify::new(),
    });
    (
        Coordinator::new(
            Controller::new(Preference::default()),
            store.clone(),
            resources.clone(),
            stop.clone(),
        ),
        store,
        resources,
        stop,
    )
}
async fn confirmed(coordinator: &Arc<Coordinator>, policy: ExitPolicy) -> Ticket {
    let ticket = coordinator.prompt().unwrap();
    coordinator
        .clone()
        .confirm(ticket, policy, false)
        .await
        .unwrap();
    ticket
}
async fn wait_phase(controller: &Controller, phase: Phase) {
    tokio::time::timeout(Duration::from_secs(2), async {
        while controller.phase() != phase {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn canceled_ipc_waiter_does_not_abandon_validate_persist_commit() {
    let (coordinator, store, _, _) = setup(StopResult::Success);
    store.block.store(true, Ordering::SeqCst);
    let ticket = coordinator.prompt().unwrap();
    let copy = coordinator.clone();
    let waiter = tokio::spawn(async move { copy.confirm(ticket, ExitPolicy::Auto, true).await });
    store.entered.notified().await;
    waiter.abort();
    let _ = waiter.await;
    *store.release.0.lock().unwrap() = true;
    store.release.1.notify_all();
    wait_phase(&coordinator.controller, Phase::Draining).await;
    assert_eq!(
        *store.saved.lock().unwrap(),
        vec![Preference::remembered(ExitPolicy::Auto)]
    );
    assert_eq!(
        coordinator.controller.preference(),
        Preference::remembered(ExitPolicy::Auto)
    );
}

#[tokio::test]
async fn store_failure_preserves_prompt_and_admission_and_stale_actions_do_not_write() {
    let (coordinator, store, _, _) = setup(StopResult::Success);
    let old = coordinator.prompt().unwrap();
    assert_eq!(coordinator.clone().run(old, true).await, Err(Error::Stale));
    coordinator.cancel(old).unwrap();
    let current = coordinator.prompt().unwrap();
    assert_ne!(old, current);
    assert_eq!(
        coordinator
            .clone()
            .confirm(old, ExitPolicy::AlwaysStop, true)
            .await,
        Err(Error::Stale)
    );
    assert_eq!(coordinator.cancel(old), Err(Error::Stale));
    assert!(store.saved.lock().unwrap().is_empty());
    store.fail.store(true, Ordering::SeqCst);
    assert_eq!(
        coordinator
            .clone()
            .confirm(current, ExitPolicy::AlwaysStop, true)
            .await,
        Err(Error::Store)
    );
    assert_eq!(coordinator.controller.phase(), Phase::Prompting);
    assert_eq!(coordinator.controller.preference(), Preference::default());
    assert!(coordinator.controller.admit_operation().is_ok());
    coordinator.cancel(current).unwrap();
}

#[tokio::test]
async fn admitted_operation_registers_later_child_then_resident_is_closed_before_reap_gate() {
    let (coordinator, _, resources, stop) = setup(StopResult::Success);
    let operation = coordinator.controller.admit_operation().unwrap();
    let ticket = confirmed(&coordinator, ExitPolicy::AlwaysStop).await;
    assert_eq!(
        coordinator.controller.admit_operation().unwrap_err(),
        Rejected::AdmissionClosed
    );
    let run = tokio::spawn(coordinator.clone().run(ticket, true));
    tokio::task::yield_now().await;
    let resident = operation.register_child().unwrap();
    resources.children.lock().unwrap().push(resident);
    assert_eq!(resources.calls.load(Ordering::SeqCst), 0);
    assert_eq!(stop.calls.load(Ordering::SeqCst), 0);
    drop(operation);
    let token = run.await.unwrap().unwrap();
    assert_eq!(resources.calls.load(Ordering::SeqCst), 1);
    assert_eq!(stop.calls.load(Ordering::SeqCst), 1);
    assert!(coordinator.allow_exit(Event::Programmatic(token)));
}

#[tokio::test]
async fn update_intent_preserves_accepted_work_and_preempts_stop_during_drain() {
    let (coordinator, _, _, stop) = setup(StopResult::Success);
    let operation = coordinator.controller.admit_operation().unwrap();
    let ticket = confirmed(&coordinator, ExitPolicy::AlwaysStop).await;
    let run = tokio::spawn(coordinator.clone().run(ticket, true));
    tokio::task::yield_now().await;
    coordinator.update_intent();
    let child = operation.register_child().unwrap();
    child.reaped();
    drop(operation);
    run.await.unwrap().unwrap();
    assert_eq!(stop.calls.load(Ordering::SeqCst), 0);
    assert_eq!(coordinator.controller.outcome(), Some(Outcome::LeftRunning));
    assert!(!coordinator.update_failed().contains("Continue"));
    assert!(coordinator.controller.admit_operation().is_err());
}

#[tokio::test]
async fn slow_update_intent_is_not_restart_commitment() {
    let (coordinator, _, _, stop) = setup(StopResult::Success);
    let operation = coordinator.controller.admit_operation().unwrap();
    coordinator.update_intent();
    assert_eq!(coordinator.controller.phase(), Phase::Running);
    operation.register_child().unwrap().reaped();
    assert!(coordinator.controller.admit_operation().is_ok());
    assert_eq!(coordinator.prompt(), Err(Error::Paused));
    let restart = coordinator.commit_restart().unwrap();
    assert!(coordinator.controller.admit_operation().is_err());
    operation.register_child().unwrap().reaped();
    drop(operation);
    coordinator.clone().run(restart, true).await.unwrap();
    assert_eq!(stop.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn updater_preempts_during_local_await_without_coordinator_lock_held() {
    let (coordinator, _, resources, stop) = setup(StopResult::Success);
    resources.hold.store(true, Ordering::SeqCst);
    let ticket = confirmed(&coordinator, ExitPolicy::AlwaysStop).await;
    let run = tokio::spawn(coordinator.clone().run(ticket, true));
    resources.entered.notified().await;
    coordinator.update_intent();
    resources.release.notify_one();
    run.await.unwrap().unwrap();
    assert_eq!(stop.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn final_exit_authorization_is_correlated_not_consumed_by_next_unrelated_event() {
    let (coordinator, _, _, _) = setup(StopResult::Success);
    let ticket = confirmed(&coordinator, ExitPolicy::AlwaysLeaveRunning).await;
    let token = coordinator.clone().run(ticket, true).await.unwrap();
    assert!(!coordinator.allow_exit(Event::WindowClose));
    assert!(!coordinator.allow_exit(Event::MenuQuit));
    assert!(!coordinator.allow_exit(Event::Programmatic(ExitToken {
        session: token.session + 1,
        ..token
    })));
    assert!(coordinator.allow_exit(Event::Programmatic(token)));
    assert!(!coordinator.allow_exit(Event::Programmatic(token)));
}

#[tokio::test]
async fn command_failure_is_observed_not_untouched_and_unknown_stays_unknown() {
    for (result, expected) in [
        (StopResult::Success, Outcome::StopReportedSuccess),
        (StopResult::CommandFailure, Outcome::StopReportedFailure),
        (StopResult::Unknown, Outcome::ServerStateUnknown),
    ] {
        let (coordinator, _, _, stop) = setup(result);
        let ticket = confirmed(&coordinator, ExitPolicy::AlwaysStop).await;
        let completion = coordinator.clone().run(ticket, true).await;
        match result {
            StopResult::Success => assert!(completion.is_ok()),
            StopResult::CommandFailure => assert_eq!(completion, Err(Error::StopFailed)),
            StopResult::Unknown => assert_eq!(completion, Err(Error::StopUnknown)),
        }
        if result != StopResult::Success {
            assert_eq!(
                coordinator.controller.phase(),
                Phase::Failed(if result == StopResult::CommandFailure {
                    Failure::StopCommandFailed
                } else {
                    Failure::StopAbandoned
                })
            );
            assert_eq!(coordinator.recover_exit(ticket).unwrap(), None);
            coordinator.update_intent();
            assert_eq!(coordinator.commit_restart(), Err(Error::Busy));
            let token = coordinator.exit_without_stop(ticket).unwrap();
            assert_eq!(coordinator.exit_without_stop(ticket), Ok(token));
            assert!(!coordinator.allow_exit(Event::MenuQuit));
            assert!(coordinator.allow_exit(Event::Programmatic(token)));
            assert!(!coordinator.allow_exit(Event::Programmatic(token)));
        }
        assert_eq!(coordinator.controller.outcome(), Some(expected));
        assert_eq!(stop.calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            coordinator.clone().run(ticket, true).await,
            Err(Error::Busy)
        );
    }
}

#[tokio::test(start_paused = true)]
async fn stop_deadline_remains_unknown_even_if_local_children_were_reaped() {
    let (coordinator, store, resources, _) = setup(StopResult::Success);
    let stop = Arc::new(FakeStop {
        calls: AtomicUsize::new(0),
        result: StopResult::Success,
        hold: true,
        entered: Notify::new(),
    });
    let coordinator = Coordinator::new(
        coordinator.controller.clone(),
        store,
        resources,
        stop.clone(),
    );
    let operation = coordinator.controller.admit_operation().unwrap();
    operation.register_child().unwrap().reaped();
    drop(operation);
    let ticket = confirmed(&coordinator, ExitPolicy::AlwaysStop).await;
    assert_eq!(
        coordinator.clone().run(ticket, true).await,
        Err(Error::StopUnknown)
    );
    assert!(coordinator.state.lock().unwrap().exit.is_none());
    let token = coordinator.exit_without_stop(ticket).unwrap();
    assert!(coordinator.allow_exit(Event::Programmatic(token)));
    assert_eq!(
        coordinator.controller.outcome(),
        Some(Outcome::ServerStateUnknown)
    );
    assert_eq!(stop.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test(start_paused = true)]
async fn drain_deadline_never_delegates_stop_or_reopens_admission() {
    let (coordinator, _, resources, stop) = setup(StopResult::Success);
    let operation = coordinator.controller.admit_operation().unwrap();
    let ticket = confirmed(&coordinator, ExitPolicy::AlwaysStop).await;
    assert_eq!(
        coordinator.clone().run(ticket, true).await,
        Err(Error::Deadline)
    );
    drop(operation);
    assert_eq!(resources.calls.load(Ordering::SeqCst), 0);
    assert_eq!(stop.calls.load(Ordering::SeqCst), 0);
    assert!(coordinator.controller.admit_operation().is_err());
    let token = coordinator.exit_without_stop(ticket).unwrap();
    assert!(coordinator.allow_exit(Event::Programmatic(token)));
}

#[tokio::test]
async fn auto_and_unsupported_target_never_delegate_stop() {
    for (policy, supported) in [(ExitPolicy::Auto, true), (ExitPolicy::AlwaysStop, false)] {
        let (coordinator, _, _, stop) = setup(StopResult::Success);
        let ticket = confirmed(&coordinator, policy).await;
        let result = coordinator.clone().run(ticket, supported).await;
        if supported {
            assert!(result.is_ok());
            assert_eq!(
                coordinator.controller.outcome(),
                Some(Outcome::AutoLeftRunningOwnershipUnknown)
            );
        } else {
            assert_eq!(result, Err(Error::TargetUnknown));
        }
        assert_eq!(stop.calls.load(Ordering::SeqCst), 0);
    }
}

#[tokio::test(start_paused = true)]
async fn teardown_deadline_retains_local_cleanup_but_never_reaches_stop() {
    let (coordinator, _, resources, stop) = setup(StopResult::Success);
    let operation = coordinator.controller.admit_operation().unwrap();
    resources
        .children
        .lock()
        .unwrap()
        .push(operation.register_child().unwrap());
    drop(operation);
    resources.hold.store(true, Ordering::SeqCst);
    let ticket = confirmed(&coordinator, ExitPolicy::AlwaysStop).await;
    assert_eq!(
        coordinator.clone().run(ticket, true).await,
        Err(Error::Deadline)
    );
    assert_eq!(stop.calls.load(Ordering::SeqCst), 0);
    resources.release.notify_one();
    tokio::task::yield_now().await;
    assert!(resources.children.lock().unwrap().is_empty());
    assert!(coordinator.controller.admit_operation().is_err());
}

#[tokio::test(start_paused = true)]
async fn updater_after_stop_delegation_does_not_claim_cancellation_or_delegate_twice() {
    let (_, store, resources, _) = setup(StopResult::Success);
    let stop = Arc::new(FakeStop {
        calls: AtomicUsize::new(0),
        result: StopResult::Success,
        hold: true,
        entered: Notify::new(),
    });
    let coordinator = Coordinator::new(
        Controller::new(Preference::default()),
        store,
        resources,
        stop.clone(),
    );
    let ticket = confirmed(&coordinator, ExitPolicy::AlwaysStop).await;
    let run = tokio::spawn(coordinator.clone().run(ticket, true));
    stop.entered.notified().await;
    coordinator.update_intent();
    assert_eq!(run.await.unwrap(), Err(Error::StopUnknown));
    assert!(coordinator.state.lock().unwrap().exit.is_none());
    assert_eq!(coordinator.commit_restart(), Err(Error::Busy));
    let token = coordinator.exit_without_stop(ticket).unwrap();
    assert_eq!(coordinator.exit_without_stop(ticket), Ok(token));
    assert!(coordinator.allow_exit(Event::Programmatic(token)));
    assert_eq!(
        coordinator.controller.outcome(),
        Some(Outcome::ServerStateUnknown)
    );
    assert_eq!(stop.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        coordinator.clone().run(ticket, true).await,
        Err(Error::Busy)
    );
}

#[tokio::test]
async fn canceled_coordinator_waiter_retains_owned_cleanup_and_finishes() {
    let (coordinator, _, resources, stop) = setup(StopResult::Success);
    resources.hold.store(true, Ordering::SeqCst);
    let ticket = confirmed(&coordinator, ExitPolicy::AlwaysStop).await;
    let waiter = tokio::spawn(coordinator.clone().run(ticket, true));
    resources.entered.notified().await;
    waiter.abort();
    let _ = waiter.await;
    resources.release.notify_one();
    wait_phase(&coordinator.controller, Phase::Finished).await;
    assert_eq!(stop.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        coordinator.controller.outcome(),
        Some(Outcome::StopReportedSuccess)
    );
    let token = coordinator.recover_exit(ticket).unwrap().unwrap();
    assert_eq!(coordinator.recover_exit(ticket).unwrap(), Some(token));
    assert!(!coordinator.allow_exit(Event::MenuQuit));
    assert!(coordinator.allow_exit(Event::Programmatic(token)));
    assert!(!coordinator.allow_exit(Event::Programmatic(token)));
    assert_eq!(coordinator.recover_exit(ticket).unwrap(), None);
    assert_eq!(
        coordinator.clone().run(ticket, true).await,
        Err(Error::Busy)
    );
    assert_eq!(stop.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn unknown_child_completion_blocks_stop_and_allows_explicit_exit_only() {
    let (coordinator, _, _, stop) = setup(StopResult::Success);
    let operation = coordinator.controller.admit_operation().unwrap();
    drop(operation.register_child().unwrap());
    drop(operation);
    let ticket = confirmed(&coordinator, ExitPolicy::AlwaysStop).await;
    assert_eq!(
        coordinator.clone().run(ticket, true).await,
        Err(Error::ChildUnknown)
    );
    assert_eq!(stop.calls.load(Ordering::SeqCst), 0);
    assert!(coordinator.controller.admit_operation().is_err());
    let token = coordinator.exit_without_stop(ticket).unwrap();
    assert!(coordinator.allow_exit(Event::Programmatic(token)));
}

#[tokio::test]
async fn factory_and_coordinator_share_the_same_admission_controller() {
    let factory = crate::adb::driver::DriverFactory::default();
    let (_, store, resources, stop) = setup(StopResult::Success);
    let coordinator = Coordinator::new(factory.controller(), store, resources, stop.clone());
    let operation = factory.operation().unwrap();
    let ticket = confirmed(&coordinator, ExitPolicy::Auto).await;
    assert!(factory.operation().is_err());
    operation.register_child().unwrap().reaped();
    drop(operation);
    coordinator.clone().run(ticket, true).await.unwrap();
    assert_eq!(stop.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn failure_exit_cannot_race_ready_to_stop_and_mint_an_extra_token() {
    let (coordinator, _, _, stop) = setup(StopResult::Success);
    let gate = Arc::new(ClaimGate::default());
    *coordinator.claim_gate.lock().unwrap() = Some(gate.clone());
    let ticket = confirmed(&coordinator, ExitPolicy::AlwaysStop).await;
    let run = tokio::spawn(coordinator.clone().run(ticket, true));
    gate.entered.notified().await;
    assert_eq!(coordinator.controller.phase(), Phase::ReadyToStop);
    assert_eq!(coordinator.exit_without_stop(ticket), Err(Error::Busy));
    assert_eq!(coordinator.recover_exit(ticket).unwrap(), None);
    gate.release.notify_one();
    let token = run.await.unwrap().unwrap();
    assert_eq!(coordinator.recover_exit(ticket).unwrap(), Some(token));
    assert_eq!(coordinator.exit_without_stop(ticket), Ok(token));
    assert_eq!(stop.calls.load(Ordering::SeqCst), 1);
    assert!(coordinator.allow_exit(Event::Programmatic(token)));
    assert_eq!(coordinator.recover_exit(ticket).unwrap(), None);
    assert_eq!(coordinator.exit_without_stop(ticket), Err(Error::Busy));
    assert!(!coordinator.allow_exit(Event::Programmatic(token)));
}

#[tokio::test]
async fn child_dropped_during_closing_local_reports_completion_unknown_not_stale() {
    let (coordinator, _, resources, stop) = setup(StopResult::Success);
    let operation = coordinator.controller.admit_operation().unwrap();
    let child = operation.register_child().unwrap();
    drop(operation);
    resources.hold.store(true, Ordering::SeqCst);
    let ticket = confirmed(&coordinator, ExitPolicy::AlwaysStop).await;
    let run = tokio::spawn(coordinator.clone().run(ticket, true));
    resources.entered.notified().await;
    assert_eq!(coordinator.controller.phase(), Phase::ClosingLocal);
    drop(child);
    resources.release.notify_one();
    assert_eq!(run.await.unwrap(), Err(Error::ChildUnknown));
    assert_eq!(
        coordinator.controller.phase(),
        Phase::Failed(Failure::UnreapedChild)
    );
    assert_eq!(coordinator.recover_exit(ticket).unwrap(), None);
    assert_eq!(stop.calls.load(Ordering::SeqCst), 0);
}

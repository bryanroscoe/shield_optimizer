//! Test-only integration contract. No production constructor, IPC, event hook or executor.
use super::*;
use async_trait::async_trait;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

static SESSIONS: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Ticket {
    session: u64,
    attempt: Attempt,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ExitToken {
    session: u64,
    sequence: u64,
}
#[derive(Clone, Copy, Debug)]
enum Event {
    WindowClose,
    MenuQuit,
    Programmatic(ExitToken),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StopResult {
    Success,
    CommandFailure,
    Unknown,
}
#[derive(Debug, PartialEq, Eq)]
enum Error {
    Stale,
    Store,
    Paused,
    Busy,
    Deadline,
    TargetUnknown,
}

trait Store: Send + Sync {
    fn persist(&self, preference: Preference) -> Result<(), Error>;
}
#[async_trait]
trait Resources: Send + Sync {
    async fn close_local_and_reap(&self);
}
#[async_trait]
trait Stop: Send + Sync {
    async fn execute(&self) -> StopResult;
}

struct State {
    pending: Option<Ticket>,
    update_intent: bool,
    running: bool,
    exit: Option<ExitToken>,
    sequence: u64,
}
struct Coordinator {
    session: u64,
    controller: Controller,
    state: Mutex<State>,
    store: Arc<dyn Store>,
    resources: Arc<dyn Resources>,
    stop: Arc<dyn Stop>,
    budget: Duration,
}
impl Coordinator {
    fn new(
        controller: Controller,
        store: Arc<dyn Store>,
        resources: Arc<dyn Resources>,
        stop: Arc<dyn Stop>,
    ) -> Arc<Self> {
        Arc::new(Self {
            session: SESSIONS.fetch_add(1, Ordering::Relaxed),
            controller,
            state: Mutex::new(State {
                pending: None,
                update_intent: false,
                running: false,
                exit: None,
                sequence: 0,
            }),
            store,
            resources,
            stop,
            budget: Duration::from_secs(1),
        })
    }
    fn prompt(&self) -> Result<Ticket, Error> {
        let mut state = self.state.lock().unwrap();
        if state.update_intent {
            return Err(Error::Paused);
        }
        if let Some(ticket) = state.pending {
            return Ok(ticket);
        }
        match self.controller.request_quit(false) {
            Request::Prompt(attempt) | Request::Draining(attempt) => {
                let ticket = Ticket {
                    session: self.session,
                    attempt,
                };
                state.pending = Some(ticket);
                Ok(ticket)
            }
            _ => Err(Error::Busy),
        }
    }
    fn validate(&self, state: &State, ticket: Ticket) -> Result<(), Error> {
        if ticket.session != self.session || state.pending != Some(ticket) {
            return Err(Error::Stale);
        }
        if state.update_intent {
            return Err(Error::Paused);
        }
        Ok(())
    }
    // The blocking worker owns the complete transaction even if its IPC waiter is canceled.
    async fn confirm(
        self: Arc<Self>,
        ticket: Ticket,
        policy: ExitPolicy,
        remember: bool,
    ) -> Result<(), Error> {
        tokio::task::spawn_blocking(move || {
            let state = self.state.lock().unwrap();
            self.validate(&state, ticket)?;
            if self.controller.phase() != Phase::Prompting {
                return Err(Error::Stale);
            }
            if remember {
                self.store.persist(Preference::remembered(policy))?;
            }
            self.controller
                .confirm(ticket.attempt, policy, remember)
                .map_err(|_| Error::Stale)?;
            Ok(())
        })
        .await
        .expect("owned preference worker")
    }
    fn cancel(&self, ticket: Ticket) -> Result<(), Error> {
        let mut state = self.state.lock().unwrap();
        self.validate(&state, ticket)?;
        self.controller
            .cancel_prompt(ticket.attempt)
            .map_err(|_| Error::Stale)?;
        state.pending = None;
        Ok(())
    }
    fn update_intent(&self) {
        self.state.lock().unwrap().update_intent = true;
    }
    fn update_failed(&self) -> &'static str {
        "Update paused. Exit without a server-stop request or restart the app."
    }
    // Proposed controller extension, deliberately confined to this test-only contract.
    // Unlike request_quit(true), Draining permits accepted operations' later children.
    fn commit_restart(&self) -> Result<Ticket, Error> {
        let mut state = self.state.lock().unwrap();
        state.update_intent = true;
        if state.running {
            return Err(Error::Busy);
        }
        let mut controller = self.controller.0.lock().unwrap();
        controller.attempt.0 += 1;
        controller.selected = ExitPolicy::AlwaysLeaveRunning;
        controller.phase = Phase::Draining;
        let ticket = Ticket {
            session: self.session,
            attempt: controller.attempt,
        };
        state.pending = Some(ticket);
        Ok(ticket)
    }
    async fn run(
        self: Arc<Self>,
        ticket: Ticket,
        supported_target: bool,
    ) -> Result<ExitToken, Error> {
        {
            let mut state = self.state.lock().unwrap();
            if state.pending != Some(ticket) || ticket.session != self.session {
                return Err(Error::Stale);
            }
            if state.running {
                return Err(Error::Busy);
            }
            if self.controller.phase() != Phase::Draining {
                return Err(Error::Stale);
            }
            state.running = true;
        }
        // This task, not its caller, owns continuations across await cancellation.
        tokio::spawn(async move { self.run_owned(ticket, supported_target).await })
            .await
            .expect("owned coordinator")
    }
    async fn run_owned(&self, ticket: Ticket, supported_target: bool) -> Result<ExitToken, Error> {
        let drain = async {
            loop {
                match self.controller.finish_drain(ticket.attempt) {
                    Ok(()) => return Ok(()),
                    Err(Rejected::WorkOutstanding) => {
                        tokio::time::sleep(Duration::from_millis(1)).await
                    }
                    _ => return Err(Error::Stale),
                }
            }
        };
        match tokio::time::timeout(self.budget, drain).await {
            Ok(result) => result?,
            Err(_) => {
                let _ = self.controller.expire(ticket.attempt);
                return Err(Error::Deadline);
            }
        }
        // Resident children may still be alive: close their sessions BEFORE waiting for leases.
        let resources = self.resources.clone();
        let cleanup = tokio::spawn(async move { resources.close_local_and_reap().await });
        let teardown = async {
            cleanup.await.map_err(|_| Error::Deadline)?;
            loop {
                {
                    let state = self.state.lock().unwrap();
                    if state.pending != Some(ticket) {
                        return Err(Error::Stale);
                    }
                    // An update veto may arrive at any await.
                    if state.update_intent {
                        self.controller.0.lock().unwrap().selected = ExitPolicy::AlwaysLeaveRunning;
                    }
                }
                match self.controller.finish_local_teardown(ticket.attempt) {
                    Ok(()) => return Ok(()),
                    Err(Rejected::ChildOutstanding) => {
                        tokio::time::sleep(Duration::from_millis(1)).await
                    }
                    _ => return Err(Error::Deadline),
                }
            }
        };
        match tokio::time::timeout(self.budget, teardown).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                let _ = self.controller.expire(ticket.attempt);
                return Err(error);
            }
            Err(_) => {
                let _ = self.controller.expire(ticket.attempt);
                return Err(Error::Deadline);
            }
        }
        let capability = {
            let state = self.state.lock().unwrap();
            if state.pending != Some(ticket) {
                return Err(Error::Stale);
            }
            if state.update_intent && self.controller.phase() == Phase::ReadyToStop {
                let mut controller = self.controller.0.lock().unwrap();
                controller.selected = ExitPolicy::AlwaysLeaveRunning;
                controller.outcome = Some(Outcome::LeftRunning);
                controller.phase = Phase::Finished;
            }
            if self.controller.phase() == Phase::ReadyToStop {
                if !supported_target {
                    return Err(Error::TargetUnknown);
                }
                Some(
                    self.controller
                        .take_stop(ticket.attempt)
                        .map_err(|_| Error::Stale)?,
                )
            } else {
                None
            }
        };
        if let Some(capability) = capability {
            match tokio::time::timeout(self.budget, self.stop.execute()).await {
                Ok(StopResult::Success) => capability.report(true),
                Ok(StopResult::CommandFailure) => capability.report(false),
                Ok(StopResult::Unknown) | Err(_) => {
                    // Proposed explicit unknown-result reporting; never reinterpret reap as success.
                    let mut state = capability.controller.0.lock().unwrap();
                    state.outcome = Some(Outcome::ServerStateUnknown);
                    state.phase = Phase::Finished;
                    drop(state);
                    let mut capability = capability;
                    capability.completed = true;
                }
            }
        }
        let mut state = self.state.lock().unwrap();
        state.sequence += 1;
        let token = ExitToken {
            session: self.session,
            sequence: state.sequence,
        };
        state.exit = Some(token);
        Ok(token)
    }
    fn exit_without_stop(&self, ticket: Ticket) -> Result<ExitToken, Error> {
        let mut state = self.state.lock().unwrap();
        if state.pending != Some(ticket) {
            return Err(Error::Stale);
        }
        let mut controller = self.controller.0.lock().unwrap();
        match controller.phase {
            Phase::Failed(_) | Phase::ReadyToStop => {
                controller.outcome = Some(
                    if controller.phase == Phase::Failed(Failure::StopAbandoned) {
                        Outcome::ServerStateUnknown
                    } else {
                        Outcome::LeftRunning
                    },
                );
                controller.phase = Phase::Finished;
            }
            _ => return Err(Error::Busy),
        }
        state.sequence += 1;
        let token = ExitToken {
            session: self.session,
            sequence: state.sequence,
        };
        state.exit = Some(token);
        Ok(token)
    }

    fn allow_exit(&self, event: Event) -> bool {
        let Event::Programmatic(token) = event else {
            return false;
        };
        let mut state = self.state.lock().unwrap();
        if state.exit != Some(token) {
            return false;
        }
        state.exit = None;
        true
    }
}

#[cfg(test)]
mod tests;

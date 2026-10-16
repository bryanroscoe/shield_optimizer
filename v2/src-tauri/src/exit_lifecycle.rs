//! Unwired desktop quit protocol. No subprocess or Tauri event is executed here.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExitPolicy {
    Auto,
    AlwaysStop,
    #[default]
    AlwaysLeaveRunning,
}

impl ExitPolicy {
    pub fn description(self) -> &'static str {
        match self {
            Self::Auto => "Shared ADB ownership cannot be verified. Auto leaves the server running.",
            Self::AlwaysStop => "Requests shared ADB server shutdown after active work finishes. This disconnects other apps and devices. If shutdown cannot finish safely, the server is left running.",
            Self::AlwaysLeaveRunning => "Leaves the shared ADB server running for other apps and devices.",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preference {
    version: u8,
    pub policy: ExitPolicy,
    pub remembered: bool,
}

impl Default for Preference {
    fn default() -> Self {
        Self {
            version: 1,
            policy: ExitPolicy::default(),
            remembered: false,
        }
    }
}

impl Preference {
    pub fn decode(json: &str) -> Self {
        serde_json::from_str::<Self>(json)
            .ok()
            .filter(|value| value.version == 1)
            .unwrap_or_default()
    }

    pub fn encode(self) -> Result<String, serde_json::Error> {
        serde_json::to_string(&self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Attempt(u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Request {
    Prompt(Attempt),
    Draining(Attempt),
    AlreadyPending,
    RestartWithoutServerStop,
    RestartAfterStopDelegated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    DrainDeadline,
    LocalTeardownDeadline,
    UnreapedChild,
    StopAbandoned,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Running,
    Restarting,
    Prompting,
    Draining,
    ClosingLocal,
    ReadyToStop,
    Stopping,
    Finished,
    Failed(Failure),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    LeftRunning,
    AutoLeftRunningOwnershipUnknown,
    StopReportedSuccess,
    StopReportedFailure,
    ServerStateUnknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rejected {
    AdmissionClosed,
    StaleAttempt,
    WrongPhase,
    WorkOutstanding,
    ChildOutstanding,
    ChildCompletionUnknown,
}

#[derive(Debug)]
struct State {
    phase: Phase,
    attempt: Attempt,
    next_id: u64,
    preference: Preference,
    selected: ExitPolicy,
    operations: HashSet<u64>,
    children: HashSet<u64>,
    child_completion_unknown: bool,
    outcome: Option<Outcome>,
}

#[derive(Debug, Clone)]
pub struct Controller(Arc<Mutex<State>>);

impl Controller {
    pub fn new(preference: Preference) -> Self {
        let preference = if preference.version == 1 {
            preference
        } else {
            Preference::default()
        };
        Self(Arc::new(Mutex::new(State {
            phase: Phase::Running,
            attempt: Attempt(0),
            next_id: 0,
            preference,
            selected: ExitPolicy::AlwaysLeaveRunning,
            operations: HashSet::new(),
            children: HashSet::new(),
            child_completion_unknown: false,
            outcome: None,
        })))
    }

    pub fn phase(&self) -> Phase {
        self.0.lock().unwrap().phase
    }
    pub fn preference(&self) -> Preference {
        self.0.lock().unwrap().preference
    }
    pub fn outcome(&self) -> Option<Outcome> {
        self.0.lock().unwrap().outcome
    }

    pub fn request_quit(&self, updater_restart: bool) -> Request {
        let mut state = self.0.lock().unwrap();
        if updater_restart {
            if matches!(
                state.phase,
                Phase::Stopping | Phase::Failed(Failure::StopAbandoned)
            ) {
                // A capability already handed out cannot promise cancellation
                // of an external side effect. No second stop is authorized.
                return Request::RestartAfterStopDelegated;
            }
            if matches!(state.phase, Phase::Restarting | Phase::Finished) {
                return Request::AlreadyPending;
            }
            state.phase = Phase::Restarting;
            return Request::RestartWithoutServerStop;
        }
        if state.phase != Phase::Running {
            return Request::AlreadyPending;
        }
        state.attempt.0 += 1;
        if state.preference.remembered {
            state.selected = state.preference.policy;
            state.phase = Phase::Draining;
            Request::Draining(state.attempt)
        } else {
            state.phase = Phase::Prompting;
            Request::Prompt(state.attempt)
        }
    }

    pub fn cancel_prompt(&self, attempt: Attempt) -> Result<(), Rejected> {
        let mut state = self.0.lock().unwrap();
        check(&state, attempt, Phase::Prompting)?;
        state.phase = Phase::Running;
        Ok(())
    }

    /// Returns a preference to persist only after an explicit remembered choice.
    pub fn confirm(
        &self,
        attempt: Attempt,
        policy: ExitPolicy,
        remember: bool,
    ) -> Result<Option<Preference>, Rejected> {
        let mut state = self.0.lock().unwrap();
        check(&state, attempt, Phase::Prompting)?;
        state.selected = policy;
        let saved = if remember {
            state.preference = Preference {
                version: 1,
                policy,
                remembered: true,
            };
            Some(state.preference)
        } else {
            None
        };
        state.phase = Phase::Draining;
        Ok(saved)
    }

    pub fn admit_operation(&self) -> Result<Operation, Rejected> {
        let mut state = self.0.lock().unwrap();
        if !matches!(state.phase, Phase::Running | Phase::Prompting) {
            return Err(Rejected::AdmissionClosed);
        }
        state.next_id += 1;
        let id = state.next_id;
        state.operations.insert(id);
        Ok(Operation {
            controller: self.clone(),
            id,
        })
    }

    pub fn finish_drain(&self, attempt: Attempt) -> Result<(), Rejected> {
        let mut state = self.0.lock().unwrap();
        check(&state, attempt, Phase::Draining)?;
        if !state.operations.is_empty() {
            return Err(Rejected::WorkOutstanding);
        }
        state.phase = Phase::ClosingLocal;
        Ok(())
    }

    /// Local socket closure / child termination only. This grants no ADB lane.
    pub fn finish_local_teardown(&self, attempt: Attempt) -> Result<(), Rejected> {
        let mut state = self.0.lock().unwrap();
        check(&state, attempt, Phase::ClosingLocal)?;
        if state.child_completion_unknown {
            return Err(Rejected::ChildCompletionUnknown);
        }
        if !state.children.is_empty() {
            return Err(Rejected::ChildOutstanding);
        }
        if state.selected == ExitPolicy::AlwaysStop {
            state.phase = Phase::ReadyToStop;
        } else {
            state.outcome = Some(if state.selected == ExitPolicy::Auto {
                Outcome::AutoLeftRunningOwnershipUnknown
            } else {
                Outcome::LeftRunning
            });
            state.phase = Phase::Finished;
        }
        Ok(())
    }

    /// Capability for the future coordinator's single narrow final stop.
    /// It is not a driver and cannot authorize generic cleanup subprocesses.
    pub fn take_stop(&self, attempt: Attempt) -> Result<StopAttempt, Rejected> {
        let mut state = self.0.lock().unwrap();
        check(&state, attempt, Phase::ReadyToStop)?;
        state.phase = Phase::Stopping;
        Ok(StopAttempt {
            controller: self.clone(),
            completed: false,
        })
    }

    pub fn expire(&self, attempt: Attempt) -> Result<(), Rejected> {
        let mut state = self.0.lock().unwrap();
        if state.attempt != attempt {
            return Err(Rejected::StaleAttempt);
        }
        state.phase = match state.phase {
            Phase::Draining => Phase::Failed(Failure::DrainDeadline),
            Phase::ClosingLocal => Phase::Failed(Failure::LocalTeardownDeadline),
            _ => return Err(Rejected::WrongPhase),
        };
        Ok(())
    }

    pub fn leave_after_failure(&self, attempt: Attempt) -> Result<(), Rejected> {
        let mut state = self.0.lock().unwrap();
        if state.attempt != attempt {
            return Err(Rejected::StaleAttempt);
        }
        if !matches!(state.phase, Phase::Failed(_)) {
            return Err(Rejected::WrongPhase);
        }
        state.outcome = Some(if state.phase == Phase::Failed(Failure::StopAbandoned) {
            Outcome::ServerStateUnknown
        } else {
            Outcome::LeftRunning
        });
        state.phase = Phase::Finished;
        Ok(())
    }
}

fn check(state: &State, attempt: Attempt, phase: Phase) -> Result<(), Rejected> {
    if state.attempt != attempt {
        return Err(Rejected::StaleAttempt);
    }
    if state.phase != phase {
        return Err(Rejected::WrongPhase);
    }
    Ok(())
}

/// The future command adapter must hold this through the whole logical operation,
/// including awaits between subprocesses and a pending driver replacement.
#[derive(Debug)]
pub struct Operation {
    controller: Controller,
    id: u64,
}

impl Operation {
    /// Register before spawning under the same lock used to commit shutdown.
    /// Keep this lease after spawn returns, until the child has been reaped.
    pub fn register_child(&self) -> Result<ChildLease, Rejected> {
        let mut state = self.controller.0.lock().unwrap();
        if !state.operations.contains(&self.id)
            || !matches!(
                state.phase,
                Phase::Running | Phase::Prompting | Phase::Draining
            )
        {
            return Err(Rejected::AdmissionClosed);
        }
        state.next_id += 1;
        let id = state.next_id;
        state.children.insert(id);
        Ok(ChildLease {
            controller: self.controller.clone(),
            id,
            completed: false,
        })
    }
}

impl Drop for Operation {
    fn drop(&mut self) {
        self.controller
            .0
            .lock()
            .unwrap()
            .operations
            .remove(&self.id);
    }
}

#[derive(Debug)]
pub struct ChildLease {
    controller: Controller,
    id: u64,
    completed: bool,
}

impl ChildLease {
    pub fn reaped(mut self) {
        self.complete();
    }
    pub fn spawn_failed(mut self) {
        self.complete();
    }
    fn complete(&mut self) {
        self.controller.0.lock().unwrap().children.remove(&self.id);
        self.completed = true;
    }
}

impl Drop for ChildLease {
    fn drop(&mut self) {
        if !self.completed {
            let mut state = self.controller.0.lock().unwrap();
            state.child_completion_unknown = true;
            if matches!(state.phase, Phase::ClosingLocal | Phase::ReadyToStop) {
                state.phase = Phase::Failed(Failure::UnreapedChild);
            }
        }
    }
}

#[derive(Debug)]
pub struct StopAttempt {
    controller: Controller,
    completed: bool,
}

impl StopAttempt {
    /// Caller reports the bounded stop result; this is not independent proof
    /// that another application cannot immediately start a new daemon.
    pub fn report(mut self, success: bool) {
        let mut state = self.controller.0.lock().unwrap();
        state.outcome = Some(if success {
            Outcome::StopReportedSuccess
        } else {
            Outcome::StopReportedFailure
        });
        state.phase = Phase::Finished;
        self.completed = true;
    }
}

impl Drop for StopAttempt {
    fn drop(&mut self) {
        if !self.completed {
            self.controller.0.lock().unwrap().phase = Phase::Failed(Failure::StopAbandoned);
        }
    }
}

#[cfg(test)]
mod tests;

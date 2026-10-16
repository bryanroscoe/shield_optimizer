use super::*;

fn prompt(controller: &Controller) -> Attempt {
    match controller.request_quit(false) {
        Request::Prompt(attempt) => attempt,
        other => panic!("expected prompt, got {other:?}"),
    }
}

fn commit(controller: &Controller, policy: ExitPolicy) -> Attempt {
    let attempt = prompt(controller);
    controller.confirm(attempt, policy, false).unwrap();
    attempt
}

fn ready_to_stop(controller: &Controller) -> Attempt {
    let attempt = commit(controller, ExitPolicy::AlwaysStop);
    controller.finish_drain(attempt).unwrap();
    controller.finish_local_teardown(attempt).unwrap();
    attempt
}

#[derive(Clone)]
struct FakeDriver(Controller);

impl FakeDriver {
    fn replacement(&self) -> Self {
        self.clone()
    }
    fn operation(&self) -> Result<Operation, Rejected> {
        self.0.admit_operation()
    }
    fn step(operation: &Operation) -> Result<(), Rejected> {
        operation.register_child()?.reaped();
        Ok(())
    }
}

#[test]
fn default_is_leave_running_with_first_quit_prompt() {
    let controller = Controller::new(Preference::default());
    assert_eq!(
        controller.preference().policy,
        ExitPolicy::AlwaysLeaveRunning
    );
    prompt(&controller);
    assert_eq!(controller.phase(), Phase::Prompting);
}

#[test]
fn corrupted_future_and_unknown_settings_fall_back_without_remembering_stop() {
    for json in [
        "",
        "null",
        "[]",
        "{}",
        "not json",
        r#"{"version":2,"policy":"always_stop","remembered":true}"#,
        r#"{"version":1,"policy":"owned","remembered":true}"#,
        r#"{"version":1,"policy":"always_stop","remembered":"true"}"#,
        r#"{"version":1,"policy":"always_stop","remembered":true,"extra":1}"#,
    ] {
        assert_eq!(Preference::decode(json), Preference::default(), "{json}");
    }
}

#[test]
fn preference_round_trip_contains_only_version_policy_and_remembered() {
    let controller = Controller::new(Preference::default());
    let attempt = prompt(&controller);
    let saved = controller
        .confirm(attempt, ExitPolicy::AlwaysStop, true)
        .unwrap()
        .unwrap();
    let json = saved.encode().unwrap();
    assert_eq!(Preference::decode(&json), saved);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&json).unwrap(),
        serde_json::json!({"version":1,"policy":"always_stop","remembered":true})
    );
}

#[test]
fn cancel_has_no_preference_admission_or_child_side_effects() {
    let controller = Controller::new(Preference::default());
    let operation = controller.admit_operation().unwrap();
    let attempt = prompt(&controller);
    controller.cancel_prompt(attempt).unwrap();
    assert_eq!(controller.preference(), Preference::default());
    assert_eq!(controller.phase(), Phase::Running);
    FakeDriver::step(&operation).unwrap();
    assert!(controller.admit_operation().is_ok());
}

#[test]
fn prompt_does_not_block_work_and_unremembered_confirmation_does_not_save() {
    let controller = Controller::new(Preference::default());
    let attempt = prompt(&controller);
    assert!(controller.admit_operation().is_ok());
    assert_eq!(
        controller.confirm(attempt, ExitPolicy::AlwaysStop, false),
        Ok(None)
    );
    assert_eq!(controller.preference(), Preference::default());
    assert!(matches!(
        controller.admit_operation(),
        Err(Rejected::AdmissionClosed)
    ));
}

#[test]
fn repeated_close_and_updater_events_coalesce_while_prompting_or_draining() {
    let controller = Controller::new(Preference::default());
    let attempt = prompt(&controller);
    assert_eq!(controller.request_quit(false), Request::AlreadyPending);
    assert_eq!(controller.request_quit(true), Request::AlreadyPending);
    controller
        .confirm(attempt, ExitPolicy::AlwaysStop, false)
        .unwrap();
    assert_eq!(controller.request_quit(false), Request::AlreadyPending);
}

#[test]
fn canceled_prompt_callback_cannot_confirm_a_new_attempt() {
    let controller = Controller::new(Preference::default());
    let old = prompt(&controller);
    controller.cancel_prompt(old).unwrap();
    let new = prompt(&controller);
    assert_ne!(old, new);
    assert_eq!(
        controller.confirm(old, ExitPolicy::AlwaysStop, true),
        Err(Rejected::StaleAttempt)
    );
    assert_eq!(controller.phase(), Phase::Prompting);
    assert_eq!(controller.preference(), Preference::default());
}

#[test]
fn logical_operation_is_held_between_subprocess_steps() {
    let controller = Controller::new(Preference::default());
    let driver = FakeDriver(controller.clone());
    let operation = driver.operation().unwrap();
    FakeDriver::step(&operation).unwrap();
    let attempt = commit(&controller, ExitPolicy::AlwaysStop);
    assert_eq!(
        controller.finish_drain(attempt),
        Err(Rejected::WorkOutstanding)
    );
    FakeDriver::step(&operation).unwrap();
    drop(operation);
    controller.finish_drain(attempt).unwrap();
}

#[test]
fn old_driver_clones_and_replacements_share_closed_admission() {
    let controller = Controller::new(Preference::default());
    let driver = FakeDriver(controller.clone());
    let old = driver.clone();
    let pending_install = driver.operation().unwrap();
    let attempt = commit(&controller, ExitPolicy::AlwaysStop);
    let replacement = driver.replacement();
    for driver in [&driver, &old, &replacement] {
        assert!(matches!(driver.operation(), Err(Rejected::AdmissionClosed)));
    }
    assert_eq!(
        controller.finish_drain(attempt),
        Err(Rejected::WorkOutstanding)
    );
    drop(pending_install);
    controller.finish_drain(attempt).unwrap();
}

#[test]
fn long_lived_child_outlives_its_start_operation_and_blocks_final_stop() {
    let controller = Controller::new(Preference::default());
    let operation = controller.admit_operation().unwrap();
    let child = operation.register_child().unwrap();
    drop(operation);
    let attempt = commit(&controller, ExitPolicy::AlwaysStop);
    controller.finish_drain(attempt).unwrap();
    assert_eq!(
        controller.finish_local_teardown(attempt),
        Err(Rejected::ChildOutstanding)
    );
    assert!(matches!(
        controller.take_stop(attempt),
        Err(Rejected::WrongPhase)
    ));
    child.reaped();
    controller.finish_local_teardown(attempt).unwrap();
    controller.take_stop(attempt).unwrap().report(true);
}

#[test]
fn failed_spawn_can_release_registration_without_a_child() {
    let controller = Controller::new(Preference::default());
    let operation = controller.admit_operation().unwrap();
    operation.register_child().unwrap().spawn_failed();
    drop(operation);
    ready_to_stop(&controller);
}

#[test]
fn dropping_child_without_reaping_is_not_treated_as_completion() {
    let controller = Controller::new(Preference::default());
    let operation = controller.admit_operation().unwrap();
    let child = operation.register_child().unwrap();
    drop(child);
    drop(operation);
    let attempt = commit(&controller, ExitPolicy::AlwaysStop);
    controller.finish_drain(attempt).unwrap();
    assert_eq!(
        controller.finish_local_teardown(attempt),
        Err(Rejected::ChildCompletionUnknown)
    );
    assert!(matches!(
        controller.take_stop(attempt),
        Err(Rejected::WrongPhase)
    ));
}

#[test]
fn local_teardown_has_no_general_adb_cleanup_lane() {
    let controller = Controller::new(Preference::default());
    let attempt = commit(&controller, ExitPolicy::AlwaysStop);
    controller.finish_drain(attempt).unwrap();
    let late_drop_driver = FakeDriver(controller.clone());
    assert!(matches!(
        late_drop_driver.operation(),
        Err(Rejected::AdmissionClosed)
    ));
    controller.finish_local_teardown(attempt).unwrap();
    assert!(matches!(
        late_drop_driver.operation(),
        Err(Rejected::AdmissionClosed)
    ));
}

#[test]
fn drain_deadline_never_authorizes_stop_or_late_operation_steps() {
    let controller = Controller::new(Preference::default());
    let operation = controller.admit_operation().unwrap();
    let attempt = commit(&controller, ExitPolicy::AlwaysStop);
    controller.expire(attempt).unwrap();
    assert_eq!(controller.phase(), Phase::Failed(Failure::DrainDeadline));
    assert!(matches!(
        operation.register_child(),
        Err(Rejected::AdmissionClosed)
    ));
    assert!(matches!(
        controller.take_stop(attempt),
        Err(Rejected::WrongPhase)
    ));
    drop(operation);
    assert_eq!(controller.finish_drain(attempt), Err(Rejected::WrongPhase));
    controller.leave_after_failure(attempt).unwrap();
    assert_eq!(controller.outcome(), Some(Outcome::LeftRunning));
    assert!(matches!(
        controller.admit_operation(),
        Err(Rejected::AdmissionClosed)
    ));
}

#[test]
fn local_teardown_deadline_cannot_be_overridden_by_late_child_completion() {
    let controller = Controller::new(Preference::default());
    let operation = controller.admit_operation().unwrap();
    let child = operation.register_child().unwrap();
    drop(operation);
    let attempt = commit(&controller, ExitPolicy::AlwaysStop);
    controller.finish_drain(attempt).unwrap();
    controller.expire(attempt).unwrap();
    child.reaped();
    assert_eq!(
        controller.phase(),
        Phase::Failed(Failure::LocalTeardownDeadline)
    );
    assert_eq!(
        controller.finish_local_teardown(attempt),
        Err(Rejected::WrongPhase)
    );
}

#[test]
fn auto_does_not_authorize_stop_even_after_perfect_local_teardown() {
    let controller = Controller::new(Preference::default());
    let attempt = commit(&controller, ExitPolicy::Auto);
    controller.finish_drain(attempt).unwrap();
    controller.finish_local_teardown(attempt).unwrap();
    assert_eq!(
        controller.outcome(),
        Some(Outcome::AutoLeftRunningOwnershipUnknown)
    );
    assert!(matches!(
        controller.take_stop(attempt),
        Err(Rejected::WrongPhase)
    ));
    assert!(ExitPolicy::Auto
        .description()
        .contains("cannot be verified"));
}

#[test]
fn leave_policy_never_authorizes_stop() {
    let controller = Controller::new(Preference::default());
    let attempt = commit(&controller, ExitPolicy::AlwaysLeaveRunning);
    controller.finish_drain(attempt).unwrap();
    controller.finish_local_teardown(attempt).unwrap();
    assert_eq!(controller.outcome(), Some(Outcome::LeftRunning));
    assert!(matches!(
        controller.take_stop(attempt),
        Err(Rejected::WrongPhase)
    ));
}

#[test]
fn final_stop_is_single_use_and_never_reopens_admission() {
    let controller = Controller::new(Preference::default());
    let attempt = ready_to_stop(&controller);
    let stop = controller.take_stop(attempt).unwrap();
    assert!(matches!(
        controller.take_stop(attempt),
        Err(Rejected::WrongPhase)
    ));
    assert!(matches!(
        controller.admit_operation(),
        Err(Rejected::AdmissionClosed)
    ));
    stop.report(true);
    assert_eq!(controller.outcome(), Some(Outcome::StopReportedSuccess));
    assert_eq!(controller.request_quit(false), Request::AlreadyPending);
    assert!(matches!(
        controller.admit_operation(),
        Err(Rejected::AdmissionClosed)
    ));
}

#[test]
fn stop_failure_or_cancellation_is_not_claimed_as_success() {
    let controller = Controller::new(Preference::default());
    let attempt = ready_to_stop(&controller);
    controller.take_stop(attempt).unwrap().report(false);
    assert_eq!(controller.outcome(), Some(Outcome::StopReportedFailure));
    let controller = Controller::new(Preference::default());
    let attempt = ready_to_stop(&controller);
    drop(controller.take_stop(attempt).unwrap());
    assert_eq!(controller.phase(), Phase::Failed(Failure::StopAbandoned));
    assert_eq!(controller.outcome(), None);
    controller.leave_after_failure(attempt).unwrap();
    assert_eq!(controller.outcome(), Some(Outcome::ServerStateUnknown));
}

#[test]
fn updater_request_has_no_stop_capability_or_preference_side_effects() {
    let preference =
        Preference::decode(r#"{"version":1,"policy":"always_stop","remembered":true}"#);
    let controller = Controller::new(preference);
    assert_eq!(
        controller.request_quit(true),
        Request::RestartWithoutServerStop
    );
    assert_eq!(controller.phase(), Phase::Running);
    assert_eq!(controller.preference(), preference);
    assert!(matches!(
        controller.take_stop(Attempt(0)),
        Err(Rejected::WrongPhase)
    ));
}

#[test]
fn app_restart_uses_only_remembered_preference_not_previous_pending_state() {
    let controller = Controller::new(Preference::default());
    let attempt = prompt(&controller);
    let preference = controller
        .confirm(attempt, ExitPolicy::Auto, true)
        .unwrap()
        .unwrap();
    let restarted = Controller::new(Preference::decode(&preference.encode().unwrap()));
    assert_eq!(restarted.phase(), Phase::Running);
    let Request::Draining(attempt) = restarted.request_quit(false) else {
        panic!("remembered");
    };
    restarted.finish_drain(attempt).unwrap();
    restarted.finish_local_teardown(attempt).unwrap();
    assert_eq!(
        restarted.outcome(),
        Some(Outcome::AutoLeftRunningOwnershipUnknown)
    );
}

#[test]
fn admission_and_commit_are_atomic_under_concurrent_requests() {
    for _ in 0..100 {
        let controller = Controller::new(Preference::default());
        let attempt = prompt(&controller);
        let barrier = Arc::new(std::sync::Barrier::new(2));
        let copy = controller.clone();
        let ready = barrier.clone();
        let worker = std::thread::spawn(move || {
            ready.wait();
            copy.admit_operation()
        });
        barrier.wait();
        controller
            .confirm(attempt, ExitPolicy::AlwaysStop, false)
            .unwrap();
        match worker.join().unwrap() {
            Ok(operation) => {
                assert_eq!(
                    controller.finish_drain(attempt),
                    Err(Rejected::WorkOutstanding)
                );
                FakeDriver::step(&operation).unwrap();
                drop(operation);
            }
            Err(error) => assert_eq!(error, Rejected::AdmissionClosed),
        }
        controller.finish_drain(attempt).unwrap();
    }
}

#[test]
fn child_abandoned_during_local_teardown_fails_closed() {
    let controller = Controller::new(Preference::default());
    let operation = controller.admit_operation().unwrap();
    let child = operation.register_child().unwrap();
    drop(operation);
    let attempt = commit(&controller, ExitPolicy::AlwaysStop);
    controller.finish_drain(attempt).unwrap();
    drop(child);
    assert_eq!(controller.phase(), Phase::Failed(Failure::UnreapedChild));
    assert!(matches!(
        controller.take_stop(attempt),
        Err(Rejected::WrongPhase)
    ));
}

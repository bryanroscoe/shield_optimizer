use super::*;

fn setup() -> (tempfile::TempDir, ExitPreviewState, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("development-previews/adb-exit-v1.json");
    let state = ExitPreviewState::development(dir.path());
    (dir, state, path)
}

#[test]
fn disabled_preview_has_no_store_or_actions() {
    let state = ExitPreviewState::default();
    for action in [
        Action::Read,
        Action::RequestQuit,
        Action::Save {
            policy: ExitPolicy::Auto,
        },
        Action::UpdaterRestart,
    ] {
        assert!(state.apply(action).unwrap_err().contains("disabled"));
    }
}

#[test]
fn first_prompt_repeats_coalesce_and_cancel_changes_neither_store_nor_admission() {
    let (_dir, state, path) = setup();
    let before = state.apply(Action::Read).unwrap();
    assert_eq!(before.preference, Preference::default());
    assert!(before.admission_open);
    let first = state.apply(Action::RequestQuit).unwrap();
    let repeated = state.apply(Action::RequestQuit).unwrap();
    assert!(first.prompt_open);
    assert!(repeated.prompt_open);
    assert!(repeated.admission_open);
    assert_eq!(first.prompt_generation, repeated.prompt_generation);
    let canceled = state
        .apply(Action::Cancel {
            prompt_generation: state.apply(Action::Read).unwrap().prompt_generation,
        })
        .unwrap();
    assert_eq!(canceled.preference, before.preference);
    assert!(canceled.admission_open);
    assert!(!path.exists());
    assert!(state
        .0
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .controller
        .admit_operation()
        .is_ok());
}

#[test]
fn remembered_choice_persists_only_to_preview_store_and_survives_restart() {
    let (dir, state, path) = setup();
    state.apply(Action::RequestQuit).unwrap();
    let confirmed = state
        .apply(Action::Confirm {
            prompt_generation: state.apply(Action::Read).unwrap().prompt_generation,
            policy: ExitPolicy::Auto,
            remember: true,
        })
        .unwrap();
    assert_eq!(confirmed.phase, "finished");
    assert!(!confirmed.stop_available);
    let saved = std::fs::read(&path).unwrap();
    let restarted = ExitPreviewState::development(dir.path());
    let read = restarted.apply(Action::Read).unwrap();
    assert_eq!(read.preference.policy, ExitPolicy::Auto);
    assert!(read.preference.remembered);
    let quit = restarted.apply(Action::RequestQuit).unwrap();
    assert!(!quit.prompt_open);
    assert_eq!(quit.phase, "finished");
    assert_eq!(std::fs::read(path).unwrap(), saved);
    assert!(!dir.path().join("adb-exit-v1.json").exists());
}

#[test]
fn cancel_preserves_an_existing_saved_preference_byte_for_byte() {
    let (dir, _state, path) = setup();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let saved = br#"{"version":1,"policy":"auto","remembered":false}"#;
    std::fs::write(&path, saved).unwrap();
    let state = ExitPreviewState::development(dir.path());
    let before = state.apply(Action::Read).unwrap().preference;
    state.apply(Action::RequestQuit).unwrap();
    let canceled = state
        .apply(Action::Cancel {
            prompt_generation: state.apply(Action::Read).unwrap().prompt_generation,
        })
        .unwrap();
    assert_eq!(canceled.preference, before);
    assert!(canceled.admission_open);
    assert_eq!(std::fs::read(&path).unwrap(), saved);
}

#[test]
fn unavailable_stop_cannot_be_saved_confirmed_or_restored_as_enabled() {
    let (dir, state, path) = setup();
    assert!(state
        .apply(Action::Save {
            policy: ExitPolicy::AlwaysStop
        })
        .is_err());
    assert!(!path.exists());
    state.apply(Action::RequestQuit).unwrap();
    assert!(state
        .apply(Action::Confirm {
            prompt_generation: state.apply(Action::Read).unwrap().prompt_generation,
            policy: ExitPolicy::AlwaysStop,
            remember: true
        })
        .is_err());
    let read = state.apply(Action::Read).unwrap();
    assert!(read.prompt_open);
    assert!(read.admission_open);
    assert!(!read.stop_available);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        &path,
        r#"{"version":1,"policy":"always_stop","remembered":true}"#,
    )
    .unwrap();
    let restarted = ExitPreviewState::development(dir.path());
    let read = restarted.apply(Action::Read).unwrap();
    assert_eq!(read.preference, Preference::default());
    assert!(read.storage_warning.is_some());
}

#[test]
fn corrupt_unknown_future_and_oversized_preferences_fail_to_leave_running() {
    let (dir, _state, path) = setup();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    for text in [
        "invalid".to_owned(),
        r#"{"version":2,"policy":"auto","remembered":true}"#.to_owned(),
        r#"{"version":1,"policy":"unknown","remembered":true}"#.to_owned(),
        "x".repeat(5000),
    ] {
        std::fs::write(&path, &text).unwrap();
        let state = ExitPreviewState::development(dir.path());
        let read = state.apply(Action::Read).unwrap();
        assert_eq!(read.preference, Preference::default());
        assert!(read.storage_warning.is_some());
        state.apply(Action::RequestQuit).unwrap();
        state
            .apply(Action::Cancel {
                prompt_generation: state.apply(Action::Read).unwrap().prompt_generation,
            })
            .unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
    }
}

#[test]
fn save_failure_keeps_prompt_preference_and_admission_unchanged() {
    let (_dir, state, path) = setup();
    std::fs::create_dir_all(&path).unwrap();
    state.apply(Action::RequestQuit).unwrap();
    assert!(state
        .apply(Action::Confirm {
            prompt_generation: state.apply(Action::Read).unwrap().prompt_generation,
            policy: ExitPolicy::Auto,
            remember: true
        })
        .is_err());
    let read = state.apply(Action::Read).unwrap();
    assert_eq!(read.preference, Preference::default());
    assert!(read.prompt_open);
    assert!(read.admission_open);
    assert!(path.is_dir());
    state
        .apply(Action::Cancel {
            prompt_generation: state.apply(Action::Read).unwrap().prompt_generation,
        })
        .unwrap();
}

#[test]
fn updater_invalidates_prompt_and_later_quit_cannot_reach_stop_or_save() {
    let (_dir, state, path) = setup();
    state.apply(Action::RequestQuit).unwrap();
    let updater = state.apply(Action::UpdaterRestart).unwrap();
    assert_eq!(updater.phase, "restarting");
    assert!(!updater.prompt_open);
    assert!(state
        .apply(Action::Confirm {
            prompt_generation: state.apply(Action::Read).unwrap().prompt_generation,
            policy: ExitPolicy::Auto,
            remember: true
        })
        .is_err());
    assert!(state
        .apply(Action::Confirm {
            prompt_generation: state.apply(Action::Read).unwrap().prompt_generation,
            policy: ExitPolicy::AlwaysStop,
            remember: true
        })
        .is_err());
    let later = state.apply(Action::RequestQuit).unwrap();
    assert_eq!(later.phase, "restarting");
    assert!(!later.stop_available);
    assert!(!path.exists());
}

#[test]
fn unremembered_choice_and_simulated_restart_keep_first_quit_behavior() {
    let (_dir, state, path) = setup();
    state.apply(Action::RequestQuit).unwrap();
    state
        .apply(Action::Confirm {
            prompt_generation: state.apply(Action::Read).unwrap().prompt_generation,
            policy: ExitPolicy::AlwaysLeaveRunning,
            remember: false,
        })
        .unwrap();
    assert!(!path.exists());
    state.apply(Action::RestartSimulation).unwrap();
    assert!(state.apply(Action::RequestQuit).unwrap().prompt_open);
}

#[test]
fn delayed_prompt_actions_cannot_mutate_a_newer_prompt_after_cancel_or_restart() {
    for reset in ["cancel", "restart", "updater_restart"] {
        let (_dir, state, path) = setup();
        let old = state.apply(Action::RequestQuit).unwrap().prompt_generation;
        if reset != "cancel" {
            if reset == "updater_restart" {
                state.apply(Action::UpdaterRestart).unwrap();
            }
            state.apply(Action::RestartSimulation).unwrap();
        } else {
            state
                .apply(Action::Cancel {
                    prompt_generation: old,
                })
                .unwrap();
        }
        let current = state.apply(Action::RequestQuit).unwrap();
        assert!(current.prompt_generation > old);
        for stale in [
            Action::Cancel {
                prompt_generation: old,
            },
            Action::Confirm {
                prompt_generation: old,
                policy: ExitPolicy::Auto,
                remember: true,
            },
        ] {
            assert!(state
                .apply(stale)
                .unwrap_err()
                .contains("no longer current"));
            let after = state.apply(Action::Read).unwrap();
            assert_eq!(after.prompt_generation, current.prompt_generation);
            assert!(after.prompt_open && after.admission_open);
            assert_eq!(after.preference, current.preference);
            assert!(!path.exists());
        }
        state
            .apply(Action::Cancel {
                prompt_generation: current.prompt_generation,
            })
            .unwrap();
    }
}

#[test]
fn successful_remembered_confirmation_clears_recovered_storage_warning() {
    let (dir, _state, path) = setup();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "corrupt").unwrap();
    let state = ExitPreviewState::development(dir.path());
    let prompt = state.apply(Action::RequestQuit).unwrap();
    assert!(prompt.storage_warning.is_some());
    let result = state
        .apply(Action::Confirm {
            prompt_generation: prompt.prompt_generation,
            policy: ExitPolicy::Auto,
            remember: true,
        })
        .unwrap();
    assert!(result.storage_warning.is_none());
    assert_eq!(
        Preference::decode(&std::fs::read_to_string(path).unwrap()),
        Preference::remembered(ExitPolicy::Auto)
    );
}

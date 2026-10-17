//! "Stop ADB when quitting" — off unless the user turns it on, and
//! conservative when on. The adb server is shared by every tool on the
//! machine, so the app never decides on its own that a server is its to stop.
//! Even with the setting on, quit leaves the server running when the app is
//! restarting for an update, when any ADB operation or adb process is still
//! in flight after a short drain, or when adb is pointed at a server the app
//! cannot target.

use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use serde::Serialize;
use tauri::State;
use tokio::time::Instant;

use shield_optimizer_core::adb::{Leases, ServerStop};

use super::AppState;

/// How long quit waits for running ADB work before giving up on the stop.
const QUIT_DRAIN: Duration = Duration::from_secs(2);
/// Upper bound on `adb kill-server` itself. It normally returns in
/// milliseconds; a wedged daemon must not keep the app from exiting.
const QUIT_STOP_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Default)]
pub struct AdbQuitControl {
    inner: Mutex<QuitState>,
}

#[derive(Default)]
struct QuitState {
    enabled: bool,
    generation: u64,
    restarting: bool,
    claimed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct StopAdbOnQuit {
    pub enabled: bool,
    /// Bumped by every applied change, so a change made from an outdated view
    /// of the setting can be told apart and refused.
    pub generation: u64,
}

/// What quit decided about the adb server.
#[derive(Debug, PartialEq, Eq)]
enum QuitStop {
    SettingOff,
    Restarting,
    AlreadyHandled,
    OperationsRunning(usize),
    ProcessesRunning(usize),
    Ran(ServerStop),
}

impl AdbQuitControl {
    pub fn setting(&self) -> StopAdbOnQuit {
        let state = self.state();
        StopAdbOnQuit {
            enabled: state.enabled,
            generation: state.generation,
        }
    }

    /// Apply a change made while the setting stood at `generation`, and return
    /// the setting as it now stands. Turning the stop on is refused when
    /// another change landed first; turning it off always applies, so a lost
    /// race can never leave the stop on behind a checkbox that reads off. Once
    /// quit has begun, nothing changes.
    pub fn set(&self, enabled: bool, generation: u64) -> StopAdbOnQuit {
        let mut state = self.state();
        if !state.claimed && (!enabled || generation == state.generation) {
            state.enabled = enabled;
            state.generation += 1;
        }
        StopAdbOnQuit {
            enabled: state.enabled,
            generation: state.generation,
        }
    }

    /// The exit about to happen is a restart (after an update), not a quit.
    pub fn note_restart(&self) {
        self.state().restarting = true;
    }

    /// Decide once whether this exit may stop the server; every later call
    /// is told it was already handled.
    fn claim(&self) -> Result<(), QuitStop> {
        let mut state = self.state();
        if std::mem::replace(&mut state.claimed, true) {
            Err(QuitStop::AlreadyHandled)
        } else if state.restarting {
            Err(QuitStop::Restarting)
        } else if !state.enabled {
            Err(QuitStop::SettingOff)
        } else {
            Ok(())
        }
    }

    fn state(&self) -> MutexGuard<'_, QuitState> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// `get_stop_adb_on_quit` — the setting as the quit path will read it.
#[tauri::command]
pub fn get_stop_adb_on_quit(control: State<'_, AdbQuitControl>) -> Result<StopAdbOnQuit, String> {
    Ok(control.setting())
}

/// `set_stop_adb_on_quit` — change the setting. Returns what it is afterwards,
/// which the UI shows instead of what it asked for.
#[tauri::command]
pub fn set_stop_adb_on_quit(
    control: State<'_, AdbQuitControl>,
    enabled: bool,
    generation: u64,
) -> Result<StopAdbOnQuit, String> {
    let setting = control.set(enabled, generation);
    tracing::info!(enabled = setting.enabled, "stop adb on quit setting");
    Ok(setting)
}

/// Run from the app's exit event.
pub async fn stop_adb_server_on_quit(state: &AppState, control: &AdbQuitControl) {
    let outcome = quit_stop(
        state,
        control,
        crate::adb::child_leases(),
        QUIT_DRAIN,
        QUIT_STOP_TIMEOUT,
    )
    .await;
    match outcome {
        QuitStop::Ran(ServerStop::Stopped) => tracing::info!("adb server stopped on quit"),
        QuitStop::Ran(ServerStop::Unknown) => tracing::warn!(
            "adb kill-server did not finish in time on quit; the adb server may or may not have stopped"
        ),
        QuitStop::Ran(ServerStop::Failed(error)) => tracing::warn!(
            %error,
            "adb kill-server failed on quit; the adb server may or may not have stopped"
        ),
        QuitStop::SettingOff | QuitStop::AlreadyHandled => {}
        skipped => tracing::info!(outcome = ?skipped, "adb kill-server not run on quit; adb server left as it was"),
    }
}

async fn quit_stop(
    state: &AppState,
    control: &AdbQuitControl,
    children: &Leases,
    drain: Duration,
    stop_timeout: Duration,
) -> QuitStop {
    if let Err(skip) = control.claim() {
        return skip;
    }
    state.close_adb_admission();
    let deadline = Instant::now() + drain;
    if !state.wait_adb_operations(deadline).await {
        return QuitStop::OperationsRunning(state.adb_operations_active());
    }
    children.close();
    if !children.wait_idle(deadline).await {
        return QuitStop::ProcessesRunning(children.active());
    }
    QuitStop::Ran(state.stop_adb_server(stop_timeout).await)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    use async_trait::async_trait;
    use shield_optimizer_core::adb::{AdbDriver, AdbError, AdbOutput, AdbResult};
    use shield_optimizer_core::engine::AppListBundle;

    use super::*;

    /// Answers every call and counts the stops it is asked for.
    #[derive(Default)]
    struct StopCounter {
        calls: AtomicUsize,
        stops: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl AdbDriver for StopCounter {
        async fn raw(&self, _: &[&str]) -> AdbResult<AdbOutput> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(AdbOutput {
                stdout: String::new(),
                stderr: String::new(),
                exit_code: Some(0),
            })
        }

        async fn shell(&self, _: &str, _: &str) -> AdbResult<AdbOutput> {
            self.raw(&[]).await
        }

        async fn stop_server(&self, _: Duration) -> ServerStop {
            self.stops.fetch_add(1, Ordering::SeqCst);
            ServerStop::Stopped
        }
    }

    fn state_counting_stops() -> (AppState, Arc<AtomicUsize>) {
        let driver = StopCounter::default();
        let stops = Arc::clone(&driver.stops);
        let state = AppState::new(
            Arc::new(driver),
            AppListBundle::default(),
            std::env::temp_dir(),
        );
        (state, stops)
    }

    fn enabled() -> AdbQuitControl {
        let control = AdbQuitControl::default();
        control.set(true, 0);
        control
    }

    async fn quit(state: &AppState, control: &AdbQuitControl, children: &Leases) -> QuitStop {
        quit_stop(state, control, children, QUIT_DRAIN, QUIT_STOP_TIMEOUT).await
    }

    #[test]
    fn the_setting_starts_off() {
        assert_eq!(
            AdbQuitControl::default().setting(),
            StopAdbOnQuit {
                enabled: false,
                generation: 0
            }
        );
    }

    #[test]
    fn a_change_from_an_outdated_view_cannot_turn_the_stop_on() {
        let control = AdbQuitControl::default();
        assert!(control.set(true, 0).enabled);
        assert_eq!(control.set(false, 1).generation, 2);
        // Still answering to generation 1, which is gone.
        let stale = control.set(true, 1);
        assert!(!stale.enabled);
        assert_eq!(stale.generation, 2);
        // Off is never refused, however outdated the view it came from.
        assert!(control.set(true, 2).enabled);
        assert!(!control.set(false, 0).enabled);
    }

    #[tokio::test]
    async fn once_quit_has_begun_the_setting_no_longer_changes() {
        let (state, _) = state_counting_stops();
        let control = enabled();
        quit(&state, &control, &Leases::new()).await;
        let after = control.set(false, control.setting().generation);
        assert!(after.enabled);
    }

    #[tokio::test]
    async fn with_the_setting_off_quit_never_stops_the_server() {
        let (state, stops) = state_counting_stops();
        let control = AdbQuitControl::default();
        assert_eq!(
            quit(&state, &control, &Leases::new()).await,
            QuitStop::SettingOff
        );
        assert_eq!(stops.load(Ordering::SeqCst), 0);
        // Admission stays open: an app that is not stopping adb has no reason
        // to refuse work during its last moments.
        assert!(state.adb_snapshot().await.raw(&["devices"]).await.is_ok());
    }

    #[tokio::test]
    async fn a_restart_for_an_update_leaves_the_server_running() {
        let (state, stops) = state_counting_stops();
        let control = enabled();
        control.note_restart();
        assert_eq!(
            quit(&state, &control, &Leases::new()).await,
            QuitStop::Restarting
        );
        assert_eq!(stops.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn quitting_twice_stops_at_most_once() {
        let (state, stops) = state_counting_stops();
        let control = enabled();
        let children = Leases::new();
        assert_eq!(
            quit(&state, &control, &children).await,
            QuitStop::Ran(ServerStop::Stopped)
        );
        assert_eq!(
            quit(&state, &control, &children).await,
            QuitStop::AlreadyHandled
        );
        assert_eq!(stops.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn after_admission_closes_new_work_is_refused_before_it_reaches_adb() {
        let driver = StopCounter::default();
        let stops = Arc::clone(&driver.stops);
        let driver = Arc::new(driver);
        let state = AppState::new(
            driver.clone(),
            AppListBundle::default(),
            std::env::temp_dir(),
        );
        quit(&state, &enabled(), &Leases::new()).await;
        assert_eq!(stops.load(Ordering::SeqCst), 1);
        let late = state.adb_snapshot().await.raw(&["devices"]).await;
        assert!(matches!(late, Err(AdbError::ShuttingDown)));
        assert_eq!(driver.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn quit_waits_for_an_operation_already_running_then_stops() {
        let (state, stops) = state_counting_stops();
        let operation = state.adb_snapshot().await;
        let finished = Arc::new(AtomicUsize::new(0));
        let finished_in_task = Arc::clone(&finished);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(500)).await;
            // Admitted before quit, so its later calls still go through.
            operation.raw(&["devices"]).await.unwrap();
            finished_in_task.store(1, Ordering::SeqCst);
        });
        assert_eq!(
            quit(&state, &enabled(), &Leases::new()).await,
            QuitStop::Ran(ServerStop::Stopped)
        );
        assert_eq!(finished.load(Ordering::SeqCst), 1);
        assert_eq!(stops.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn an_operation_that_outlasts_the_drain_leaves_the_server_running() {
        let (state, stops) = state_counting_stops();
        // A live Remote session holds its snapshot exactly like this.
        let _session = state.adb_snapshot().await;
        assert_eq!(
            quit(&state, &enabled(), &Leases::new()).await,
            QuitStop::OperationsRunning(1)
        );
        assert_eq!(stops.load(Ordering::SeqCst), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn an_adb_process_still_alive_leaves_the_server_running() {
        let (state, stops) = state_counting_stops();
        let children = Leases::new();
        let _unreaped = children.try_acquire().unwrap();
        assert_eq!(
            quit(&state, &enabled(), &children).await,
            QuitStop::ProcessesRunning(1)
        );
        assert_eq!(stops.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn quit_waits_for_old_snapshot_after_replacement_then_stops_live_driver() {
        let (state, old_stops) = state_counting_stops();
        let old_snapshot = state.adb_snapshot().await;
        let replacement = StopCounter::default();
        let replacement_stops = Arc::clone(&replacement.stops);
        state.replace_adb(Arc::new(replacement)).await;
        let release_old = async {
            tokio::task::yield_now().await;
            old_snapshot.raw(&["fixture"]).await.unwrap();
            drop(old_snapshot);
        };
        let control = enabled();
        let children = Leases::new();
        let (outcome, ()) = tokio::join!(quit(&state, &control, &children), release_old);
        assert_eq!(outcome, QuitStop::Ran(ServerStop::Stopped));
        assert_eq!(old_stops.load(Ordering::SeqCst), 0);
        assert_eq!(replacement_stops.load(Ordering::SeqCst), 1);
        assert!(matches!(
            state.adb_snapshot().await.raw(&[]).await,
            Err(AdbError::ShuttingDown)
        ));
    }

    #[tokio::test]
    async fn pending_install_prevents_stop_until_driver_replacement_finishes() {
        let (state, stops) = state_counting_stops();
        let (entered_tx, entered) = tokio::sync::oneshot::channel();
        let (release, released) = tokio::sync::oneshot::channel();
        let install = super::super::install::install_with(&state, || async {
            entered_tx.send(()).unwrap();
            released.await.unwrap();
            Ok(std::path::PathBuf::from("/fixture/not-executed"))
        });
        tokio::pin!(install);
        tokio::select! {
            _ = &mut install => panic!("installer should remain pending"),
            _ = entered => {}
        }
        assert_eq!(
            quit_stop(
                &state,
                &enabled(),
                &Leases::new(),
                Duration::ZERO,
                QUIT_STOP_TIMEOUT
            )
            .await,
            QuitStop::OperationsRunning(1)
        );
        assert_eq!(stops.load(Ordering::SeqCst), 0);
        release.send(()).unwrap();
        assert!(install.await.unwrap().ok);
        assert_eq!(state.adb_operations_active(), 0);
        assert!(matches!(
            state.adb_snapshot().await.raw(&[]).await,
            Err(AdbError::ShuttingDown)
        ));
    }

    #[tokio::test]
    async fn install_after_closed_admission_has_no_installer_effects() {
        let (state, stops) = state_counting_stops();
        state.close_adb_admission();
        let effects = AtomicUsize::new(0);
        let result = super::super::install::install_with(&state, || async {
            effects.fetch_add(1, Ordering::SeqCst);
            Ok(std::path::PathBuf::from("/fixture/not-executed"))
        })
        .await;
        assert!(result.is_err());
        assert_eq!(effects.load(Ordering::SeqCst), 0);
        assert_eq!(stops.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn canceled_install_waiter_keeps_blocking_extraction_counted() {
        let (state, stops) = state_counting_stops();
        let state = Arc::new(state);
        let (entered_tx, entered) = tokio::sync::oneshot::channel();
        let (release, released) = std::sync::mpsc::channel();
        let (done_tx, done) = tokio::sync::oneshot::channel();
        let install_state = Arc::clone(&state);
        let install = tokio::spawn(async move {
            super::super::install::install_with(&install_state, || async {
                crate::adb::install::finish_extraction(move || {
                    entered_tx.send(()).unwrap();
                    // A bounded wait also makes a failed assertion safe to unwind.
                    let released = released.recv_timeout(Duration::from_secs(5)).is_ok();
                    let _ = done_tx.send(());
                    assert!(released, "fixture extraction was not released");
                    Ok(std::path::PathBuf::from("/fixture/not-executed"))
                })
                .await
            })
            .await
        });
        entered.await.unwrap();
        install.abort();
        assert!(matches!(install.await, Err(error) if error.is_cancelled()));
        let outcome = quit_stop(
            &state,
            &enabled(),
            &Leases::new(),
            Duration::ZERO,
            QUIT_STOP_TIMEOUT,
        )
        .await;
        // Always release the owned worker before asserting the regression.
        release.send(()).unwrap();
        done.await.unwrap();
        assert_eq!(outcome, QuitStop::OperationsRunning(1));
        assert_eq!(stops.load(Ordering::SeqCst), 0);
        assert!(
            state
                .wait_adb_operations(Instant::now() + Duration::from_secs(2))
                .await
        );
    }

    #[cfg(unix)]
    mod with_a_real_process {
        use std::path::{Path, PathBuf};

        use crate::adb::SubprocessAdb;

        use super::*;

        /// A stand-in adb that logs its arguments and claims, on every call,
        /// to have started the server: the text that could once be spoofed
        /// into an ownership claim.
        fn fake_adb(dir: &Path) -> (PathBuf, PathBuf) {
            let log = dir.join("calls");
            let adb = dir.join("adb");
            crate::adb::write_script(
                &adb,
                &format!(
                    "echo \"$*\" >> '{}'\necho '* daemon started successfully' >&2",
                    log.display()
                ),
            );
            (adb, log)
        }

        fn calls(log: &Path) -> Vec<String> {
            std::fs::read_to_string(log)
                .unwrap_or_default()
                .lines()
                .map(str::to_string)
                .collect()
        }

        async fn state_with_fake_adb(dir: &Path) -> (AppState, Arc<Leases>, PathBuf) {
            let (adb, log) = fake_adb(dir);
            let children = Leases::new();
            let driver = SubprocessAdb::new(adb).with_child_leases(Arc::clone(&children));
            let state = AppState::new(
                Arc::new(driver),
                AppListBundle::default(),
                dir.to_path_buf(),
            );
            // History that once read as "this app owns the server": a device
            // shell printing the start message, and a restart.
            let adb = state.adb_snapshot().await;
            adb.shell("tv", "echo '* daemon started successfully'")
                .await
                .unwrap();
            adb.raw(&["kill-server"]).await.unwrap();
            adb.raw(&["start-server"]).await.unwrap();
            drop(adb);
            (state, children, log)
        }

        #[tokio::test]
        async fn spoofed_start_output_and_restarts_do_not_stop_the_server() {
            let dir = tempfile::tempdir().unwrap();
            let (state, children, log) = state_with_fake_adb(dir.path()).await;
            let before = calls(&log).len();
            assert_eq!(
                quit(&state, &AdbQuitControl::default(), &children).await,
                QuitStop::SettingOff
            );
            assert_eq!(calls(&log).len(), before);
        }

        #[tokio::test]
        async fn with_the_setting_on_quit_runs_kill_server_once() {
            let dir = tempfile::tempdir().unwrap();
            let (state, children, log) = state_with_fake_adb(dir.path()).await;
            let before = calls(&log).len();
            let control = enabled();
            assert_eq!(
                quit(&state, &control, &children).await,
                QuitStop::Ran(ServerStop::Stopped)
            );
            quit(&state, &control, &children).await;
            assert_eq!(calls(&log)[before..], ["kill-server"]);
        }
    }
}

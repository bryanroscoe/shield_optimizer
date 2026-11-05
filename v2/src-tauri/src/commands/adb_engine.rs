//! The ADB engine setting: built-in Rust ADB or Google platform-tools.
//!
//! The engine is chosen at launch (override, then the saved choice, then the
//! build default) and can be switched from Settings without a restart. When
//! the built-in engine is chosen but cannot start, this session uses
//! platform-tools and the status says why. That decision is made only when
//! an engine is started, never per command, and is never retried behind the
//! user's back.

use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;
use tauri::State;

use atv_optimizer_core::adb::{AdbDriver, AdbError, AdbOutput, AdbResult, Leases};
use atv_optimizer_core::engine::telemetry_diag::{adb_diag, AdbDiag, AdbSource};
use atv_optimizer_core::engine::AppListBundle;

use super::AppState;
use crate::adb::selection::{self, AdbEngine, EngineSource};

/// Builds the driver for an engine. Startup and Settings use the same one.
pub trait EngineStarter: Send + Sync {
    fn platform_tools(&self) -> Arc<dyn AdbDriver>;
    /// The built-in driver, counting its work on `operations`, or why it
    /// cannot start.
    fn builtin(&self, operations: &Arc<Leases>) -> Result<Arc<dyn AdbDriver>, String>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AdbEngineStatus {
    /// `None` only when the launch override holds an invalid value.
    pub selected: Option<AdbEngine>,
    pub source: EngineSource,
    /// The engine actually running. Differs from `selected` only after a
    /// fallback, which `fallback_reason` explains.
    pub active: Option<AdbEngine>,
    pub fallback_reason: Option<String>,
    /// This build includes the built-in engine.
    pub builtin_available: bool,
}

struct EngineState {
    selected: Option<AdbEngine>,
    source: EngineSource,
    active: Option<AdbEngine>,
    fallback_reason: Option<String>,
}

pub struct AdbEngineControl {
    data_dir: PathBuf,
    /// `None` for a harness whose driver this control did not build.
    starter: Option<Arc<dyn EngineStarter>>,
    /// The app's admission counter. Every built-in driver counts its work
    /// here, so quit drains it, and leaves it open when a switch retires
    /// that driver.
    operations: Arc<Leases>,
    state: tokio::sync::Mutex<EngineState>,
}

const ENV_LOCKED: &str = "ATV_OPTIMIZER_TRANSPORT chooses the ADB engine for this launch. Remove it and reopen the app to use this setting.";
const SIMULATED: &str =
    "This test harness runs a simulated ADB, so the engine can't be changed here.";
const NOT_BUILT: &str = "This build doesn't include the built-in engine.";

struct Started {
    driver: Arc<dyn AdbDriver>,
    active: AdbEngine,
    fallback_reason: Option<String>,
}

fn start_engine(
    starter: &Arc<dyn EngineStarter>,
    engine: AdbEngine,
    operations: &Arc<Leases>,
) -> Started {
    match engine {
        AdbEngine::PlatformTools => Started {
            driver: starter.platform_tools(),
            active: AdbEngine::PlatformTools,
            fallback_reason: None,
        },
        AdbEngine::Builtin => match starter.builtin(operations) {
            Ok(driver) => Started {
                driver,
                active: AdbEngine::Builtin,
                fallback_reason: None,
            },
            Err(reason) => {
                tracing::warn!(%reason, "built-in ADB engine could not start; using Google platform-tools");
                Started {
                    driver: starter.platform_tools(),
                    active: AdbEngine::PlatformTools,
                    fallback_reason: Some(reason),
                }
            }
        },
    }
}

impl AdbEngineControl {
    /// Choose and start this launch's engine. `Err` is an invalid launch
    /// override, which the caller turns into a driver that fails visibly.
    pub fn start(
        data_dir: PathBuf,
        env: Option<&str>,
        starter: Arc<dyn EngineStarter>,
    ) -> (Self, Result<Arc<dyn AdbDriver>, String>) {
        let operations = Leases::new();
        let saved = if env.is_some() {
            None
        } else {
            selection::load_saved(&data_dir)
        };
        let (state, driver) = match selection::resolve(env, saved) {
            Ok((engine, source)) => {
                let started = start_engine(&starter, engine, &operations);
                tracing::info!(selected = engine.label(), source = ?source, active = started.active.label(), "ADB engine");
                (
                    EngineState {
                        selected: Some(engine),
                        source,
                        active: Some(started.active),
                        fallback_reason: started.fallback_reason,
                    },
                    Ok(started.driver),
                )
            }
            Err(message) => (
                EngineState {
                    selected: None,
                    source: EngineSource::Env,
                    active: None,
                    fallback_reason: None,
                },
                Err(message),
            ),
        };
        let control = Self {
            data_dir,
            starter: Some(starter),
            operations,
            state: tokio::sync::Mutex::new(state),
        };
        (control, driver)
    }

    /// For a harness that manages its own simulated driver: reports
    /// platform-tools from the build default and refuses every switch.
    pub fn simulated() -> Self {
        Self {
            data_dir: PathBuf::new(),
            starter: None,
            operations: Leases::new(),
            state: tokio::sync::Mutex::new(EngineState {
                selected: Some(AdbEngine::PlatformTools),
                source: EngineSource::Default,
                active: Some(AdbEngine::PlatformTools),
                fallback_reason: None,
            }),
        }
    }

    /// The app state, admitting work through this control's counter, with
    /// `driver` installed.
    pub async fn app_state(
        &self,
        driver: Arc<dyn AdbDriver>,
        app_lists: AppListBundle,
        data_dir: PathBuf,
    ) -> AppState {
        // `AppState` adopts the first driver's counter. Handing it this
        // control's counter first means no driver it later retires owns it.
        let state = AppState::new(
            Arc::new(Admission(Arc::clone(&self.operations))),
            app_lists,
            data_dir,
        );
        state.replace_adb(driver).await;
        state
    }

    pub async fn status(&self) -> AdbEngineStatus {
        let state = self.state.lock().await;
        status_of(&state)
    }

    /// Save `engine` and switch to it. Choosing the engine already running
    /// only saves the choice.
    pub async fn set(&self, app: &AppState, engine: AdbEngine) -> Result<AdbEngineStatus, String> {
        let Some(starter) = &self.starter else {
            return Err(SIMULATED.into());
        };
        let mut current = self.state.lock().await;
        if current.source == EngineSource::Env {
            return Err(ENV_LOCKED.into());
        }
        if engine == AdbEngine::Builtin && !selection::builtin_available() {
            return Err(NOT_BUILT.into());
        }
        // Counted like any other ADB work: quit waits for a switch already
        // under way and refuses one that starts after it.
        let _operation = app.try_adb_operation().map_err(|e| e.to_string())?;
        selection::save(&self.data_dir, engine)?;
        current.selected = Some(engine);
        current.source = EngineSource::Saved;
        if current.active == Some(engine) {
            current.fallback_reason = None;
        } else {
            let started = start_engine(starter, engine, &self.operations);
            if current.active != Some(started.active) {
                let host_server = started.driver.uses_host_server();
                app.replace_adb(started.driver).await;
                app.telemetry.set_adb_facts(adb_facts(host_server));
            }
            current.active = Some(started.active);
            current.fallback_reason = started.fallback_reason;
        }
        tracing::info!(
            selected = engine.label(),
            active = ?current.active,
            fallback = current.fallback_reason.is_some(),
            "ADB engine changed"
        );
        Ok(status_of(&current))
    }
}

fn status_of(state: &EngineState) -> AdbEngineStatus {
    AdbEngineStatus {
        selected: state.selected,
        source: state.source,
        active: state.active,
        fallback_reason: state.fallback_reason.clone(),
        builtin_available: selection::builtin_available(),
    }
}

/// What anonymous reports say about the running driver.
pub(crate) fn adb_facts(uses_host_server: bool) -> AdbDiag {
    adb_diag(
        if uses_host_server {
            super::diagnostics::adb_source()
        } else {
            AdbSource::Builtin
        },
        None,
        None,
    )
}

/// Stands in for the driver only while `app_state` builds the app state.
struct Admission(Arc<Leases>);

#[async_trait::async_trait]
impl AdbDriver for Admission {
    fn operation_leases(&self) -> Option<Arc<Leases>> {
        Some(Arc::clone(&self.0))
    }
    async fn raw(&self, _: &[&str]) -> AdbResult<AdbOutput> {
        Err(AdbError::Initialization("ADB is still starting.".into()))
    }
    async fn shell(&self, _: &str, _: &str) -> AdbResult<AdbOutput> {
        Err(AdbError::Initialization("ADB is still starting.".into()))
    }
}

/// `adb_engine` — the selected and running engine.
#[tauri::command]
pub async fn adb_engine(control: State<'_, AdbEngineControl>) -> Result<AdbEngineStatus, String> {
    Ok(control.status().await)
}

/// `set_adb_engine` — save the choice and switch to it. Returns the status
/// afterwards, which says whether the built-in engine actually started.
#[tauri::command]
pub async fn set_adb_engine(
    state: State<'_, AppState>,
    control: State<'_, AdbEngineControl>,
    engine: AdbEngine,
) -> Result<AdbEngineStatus, String> {
    control.set(state.inner(), engine).await
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    use super::*;

    /// Answers `version` with its name, so a test can tell which driver is live.
    struct Named {
        name: &'static str,
        host_server: bool,
    }

    #[async_trait::async_trait]
    impl AdbDriver for Named {
        fn uses_host_server(&self) -> bool {
            self.host_server
        }
        async fn raw(&self, _: &[&str]) -> AdbResult<AdbOutput> {
            Ok(AdbOutput {
                stdout: self.name.into(),
                stderr: String::new(),
                exit_code: Some(0),
            })
        }
        async fn shell(&self, _: &str, _: &str) -> AdbResult<AdbOutput> {
            self.raw(&[]).await
        }
    }

    #[derive(Default)]
    struct Starter {
        builtin_error: Mutex<Option<String>>,
        platform_tools: AtomicUsize,
        builtin: AtomicUsize,
    }

    impl Starter {
        fn failing(reason: &str) -> Arc<Self> {
            let starter = Self::default();
            *starter.builtin_error.lock().unwrap() = Some(reason.into());
            Arc::new(starter)
        }
        fn calls(&self) -> (usize, usize) {
            (
                self.platform_tools.load(Ordering::SeqCst),
                self.builtin.load(Ordering::SeqCst),
            )
        }
    }

    impl EngineStarter for Starter {
        fn platform_tools(&self) -> Arc<dyn AdbDriver> {
            self.platform_tools.fetch_add(1, Ordering::SeqCst);
            Arc::new(Named {
                name: "platform-tools",
                host_server: true,
            })
        }
        fn builtin(&self, _: &Arc<Leases>) -> Result<Arc<dyn AdbDriver>, String> {
            self.builtin.fetch_add(1, Ordering::SeqCst);
            match self.builtin_error.lock().unwrap().clone() {
                Some(reason) => Err(reason),
                None => Ok(Arc::new(Named {
                    name: "built-in",
                    host_server: false,
                })),
            }
        }
    }

    async fn launch(
        dir: &std::path::Path,
        env: Option<&str>,
        starter: Arc<Starter>,
    ) -> (AdbEngineControl, Result<AppState, String>) {
        let (control, driver) = AdbEngineControl::start(dir.to_path_buf(), env, starter);
        let state = match driver {
            Ok(driver) => Ok(control
                .app_state(driver, AppListBundle::default(), dir.to_path_buf())
                .await),
            Err(message) => Err(message),
        };
        (control, state)
    }

    async fn live(state: &AppState) -> String {
        state
            .adb_snapshot()
            .await
            .raw(&["version"])
            .await
            .unwrap()
            .stdout
    }

    fn running(selected: AdbEngine, source: EngineSource) -> AdbEngineStatus {
        AdbEngineStatus {
            selected: Some(selected),
            source,
            active: Some(selected),
            fallback_reason: None,
            builtin_available: selection::builtin_available(),
        }
    }

    #[tokio::test]
    async fn launch_starts_the_saved_engine_over_the_build_default() {
        let dir = tempfile::tempdir().unwrap();
        let (control, state) = launch(dir.path(), None, Arc::new(Starter::default())).await;
        let state = state.unwrap();
        let default = selection::build_default();
        assert_eq!(
            control.status().await,
            running(default, EngineSource::Default)
        );
        let expected = if default == AdbEngine::Builtin {
            "built-in"
        } else {
            "platform-tools"
        };
        assert_eq!(live(&state).await, expected);

        selection::save(dir.path(), AdbEngine::Builtin).unwrap();
        let (control, state) = launch(dir.path(), None, Arc::new(Starter::default())).await;
        let state = state.unwrap();
        assert_eq!(
            control.status().await,
            running(AdbEngine::Builtin, EngineSource::Saved)
        );
        assert_eq!(live(&state).await, "built-in");
    }

    #[tokio::test]
    async fn the_launch_override_beats_the_saved_choice_and_locks_it() {
        let dir = tempfile::tempdir().unwrap();
        selection::save(dir.path(), AdbEngine::Builtin).unwrap();
        let starter = Arc::new(Starter::default());
        let (control, state) = launch(dir.path(), Some("subprocess"), starter.clone()).await;
        let state = state.unwrap();
        assert_eq!(
            control.status().await,
            running(AdbEngine::PlatformTools, EngineSource::Env)
        );
        let refused = control.set(&state, AdbEngine::Builtin).await.unwrap_err();
        assert!(refused.contains("ATV_OPTIMIZER_TRANSPORT"), "{refused}");
        assert_eq!(live(&state).await, "platform-tools");
        assert_eq!(starter.calls(), (1, 0), "a refused switch builds nothing");
        assert_eq!(
            selection::load_saved(dir.path()),
            Some(AdbEngine::Builtin),
            "a refused switch saves nothing"
        );
    }

    #[tokio::test]
    async fn an_invalid_override_is_a_visible_error_with_no_engine() {
        let dir = tempfile::tempdir().unwrap();
        let starter = Arc::new(Starter::default());
        let (control, state) = launch(dir.path(), Some("automatic"), starter.clone()).await;
        assert_eq!(
            state.err().unwrap(),
            "ATV_OPTIMIZER_TRANSPORT must be subprocess or direct."
        );
        let status = control.status().await;
        assert_eq!(status.source, EngineSource::Env);
        assert_eq!((status.selected, status.active), (None, None));
        assert_eq!(
            starter.calls(),
            (0, 0),
            "no fallback for a configuration error"
        );
    }

    #[tokio::test]
    async fn a_builtin_that_cannot_start_falls_back_once_with_its_reason() {
        let dir = tempfile::tempdir().unwrap();
        selection::save(dir.path(), AdbEngine::Builtin).unwrap();
        let starter = Starter::failing("fixture identity could not be saved");
        let (control, state) = launch(dir.path(), None, starter.clone()).await;
        let state = state.unwrap();
        let status = control.status().await;
        assert_eq!(status.selected, Some(AdbEngine::Builtin));
        assert_eq!(status.active, Some(AdbEngine::PlatformTools));
        assert_eq!(
            status.fallback_reason.as_deref(),
            Some("fixture identity could not be saved")
        );
        assert_eq!(live(&state).await, "platform-tools");
        // Commands run on the fallback without anything trying the built-in
        // engine again.
        for _ in 0..3 {
            live(&state).await;
        }
        assert_eq!(starter.calls(), (1, 1));
        assert_eq!(selection::load_saved(dir.path()), Some(AdbEngine::Builtin));
    }

    // Builds without the engine refuse the switch; see
    // `a_build_without_builtin_refuses_it_but_still_saves_platform_tools`.
    #[cfg(feature = "direct-adb")]
    #[tokio::test]
    async fn switching_swaps_the_live_driver_both_ways_and_saves_the_choice() {
        let dir = tempfile::tempdir().unwrap();
        selection::save(dir.path(), AdbEngine::PlatformTools).unwrap();
        let starter = Arc::new(Starter::default());
        let (control, state) = launch(dir.path(), None, starter.clone()).await;
        let state = state.unwrap();
        let before = state.adb_snapshot().await;

        let status = control.set(&state, AdbEngine::Builtin).await.unwrap();
        assert_eq!(status, running(AdbEngine::Builtin, EngineSource::Saved));
        assert_eq!(live(&state).await, "built-in");
        assert!(!state.adb_snapshot().await.uses_host_server());
        assert_eq!(selection::load_saved(dir.path()), Some(AdbEngine::Builtin));
        // Work admitted before the switch finishes on the driver it began on.
        assert_eq!(
            before.raw(&["version"]).await.unwrap().stdout,
            "platform-tools"
        );
        drop(before);

        let status = control.set(&state, AdbEngine::PlatformTools).await.unwrap();
        assert_eq!(
            status,
            running(AdbEngine::PlatformTools, EngineSource::Saved)
        );
        assert_eq!(live(&state).await, "platform-tools");
        assert_eq!(
            selection::load_saved(dir.path()),
            Some(AdbEngine::PlatformTools)
        );
        assert_eq!(starter.calls(), (2, 1));
        assert_eq!(state.adb_operations_active(), 0);
    }

    #[tokio::test]
    async fn choosing_the_running_engine_only_saves_it() {
        let dir = tempfile::tempdir().unwrap();
        selection::save(dir.path(), AdbEngine::PlatformTools).unwrap();
        let starter = Arc::new(Starter::default());
        let (control, state) = launch(dir.path(), None, starter.clone()).await;
        let state = state.unwrap();
        std::fs::remove_file(dir.path().join("adb-engine.json")).unwrap();
        let status = control.set(&state, AdbEngine::PlatformTools).await.unwrap();
        assert_eq!(
            status,
            running(AdbEngine::PlatformTools, EngineSource::Saved)
        );
        assert_eq!(starter.calls(), (1, 0), "no new driver");
        assert_eq!(
            selection::load_saved(dir.path()),
            Some(AdbEngine::PlatformTools)
        );
    }

    #[tokio::test]
    async fn choosing_builtin_again_after_a_fallback_tries_once_more() {
        let dir = tempfile::tempdir().unwrap();
        selection::save(dir.path(), AdbEngine::Builtin).unwrap();
        let starter = Starter::failing("first reason");
        let (control, state) = launch(dir.path(), None, starter.clone()).await;
        let state = state.unwrap();

        *starter.builtin_error.lock().unwrap() = Some("second reason".into());
        let status = control.set(&state, AdbEngine::Builtin).await;
        if !selection::builtin_available() {
            assert_eq!(status.unwrap_err(), NOT_BUILT);
            return;
        }
        let status = status.unwrap();
        assert_eq!(status.active, Some(AdbEngine::PlatformTools));
        assert_eq!(status.fallback_reason.as_deref(), Some("second reason"));
        assert_eq!(live(&state).await, "platform-tools");

        *starter.builtin_error.lock().unwrap() = None;
        let status = control.set(&state, AdbEngine::Builtin).await.unwrap();
        assert_eq!(status, running(AdbEngine::Builtin, EngineSource::Saved));
        assert_eq!(live(&state).await, "built-in");
        assert_eq!(starter.calls().1, 3);
    }

    #[tokio::test]
    async fn a_build_without_builtin_refuses_it_but_still_saves_platform_tools() {
        if selection::builtin_available() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let (control, state) = launch(dir.path(), None, Arc::new(Starter::default())).await;
        let state = state.unwrap();
        assert!(!control.status().await.builtin_available);
        assert_eq!(
            control.set(&state, AdbEngine::Builtin).await.unwrap_err(),
            NOT_BUILT
        );
        assert_eq!(selection::load_saved(dir.path()), None);
        control.set(&state, AdbEngine::PlatformTools).await.unwrap();
        assert_eq!(
            selection::load_saved(dir.path()),
            Some(AdbEngine::PlatformTools)
        );
    }

    #[tokio::test]
    async fn a_switch_that_cannot_be_saved_changes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        selection::save(dir.path(), AdbEngine::Builtin).unwrap();
        let (control, state) = launch(dir.path(), None, Arc::new(Starter::default())).await;
        let state = state.unwrap();
        std::fs::create_dir(dir.path().join("adb-engine.json.tmp")).unwrap();
        let error = control
            .set(&state, AdbEngine::PlatformTools)
            .await
            .unwrap_err();
        assert!(error.starts_with("Couldn't save"), "{error}");
        assert_eq!(
            control.status().await,
            running(AdbEngine::Builtin, EngineSource::Saved)
        );
        assert_eq!(live(&state).await, "built-in");
    }

    #[tokio::test]
    async fn no_switch_once_quit_has_closed_admission() {
        let dir = tempfile::tempdir().unwrap();
        selection::save(dir.path(), AdbEngine::PlatformTools).unwrap();
        let starter = Arc::new(Starter::default());
        let (control, state) = launch(dir.path(), None, starter.clone()).await;
        let state = state.unwrap();
        state.close_adb_admission();
        assert!(control.set(&state, AdbEngine::Builtin).await.is_err());
        assert_eq!(starter.calls(), (1, 0));
        assert_eq!(
            selection::load_saved(dir.path()),
            Some(AdbEngine::PlatformTools)
        );
    }

    #[tokio::test]
    async fn the_app_counts_work_on_the_counter_builtin_drivers_share() {
        let dir = tempfile::tempdir().unwrap();
        let (control, state) = launch(dir.path(), None, Arc::new(Starter::default())).await;
        let state = state.unwrap();
        let held = control.operations.try_acquire().unwrap();
        assert_eq!(state.adb_operations_active(), 1);
        drop(held);
        let snapshot = state.adb_snapshot().await;
        assert_eq!(control.operations.active(), 1);
        drop(snapshot);
        state.close_adb_admission();
        assert!(control.operations.try_acquire().is_none());
    }

    #[tokio::test]
    async fn the_simulated_harness_reports_platform_tools_and_refuses_switching() {
        let control = AdbEngineControl::simulated();
        let status = control.status().await;
        assert_eq!(status.selected, Some(AdbEngine::PlatformTools));
        assert_eq!(status.source, EngineSource::Default);
        assert_eq!(status.active, Some(AdbEngine::PlatformTools));
        let state = AppState::new(
            Arc::new(Named {
                name: "simulator",
                host_server: true,
            }),
            AppListBundle::default(),
            PathBuf::from("not-created"),
        );
        for engine in [AdbEngine::Builtin, AdbEngine::PlatformTools] {
            assert_eq!(control.set(&state, engine).await.unwrap_err(), SIMULATED);
        }
        assert_eq!(live(&state).await, "simulator");
    }

    #[test]
    fn status_serializes_the_names_the_frontend_reads() {
        let status = AdbEngineStatus {
            selected: Some(AdbEngine::Builtin),
            source: EngineSource::Saved,
            active: Some(AdbEngine::PlatformTools),
            fallback_reason: Some("fixture".into()),
            builtin_available: true,
        };
        assert_eq!(
            serde_json::to_value(&status).unwrap(),
            serde_json::json!({
                "selected": "builtin",
                "source": "saved",
                "active": "platform_tools",
                "fallback_reason": "fixture",
                "builtin_available": true,
            })
        );
        let requested: AdbEngine = serde_json::from_str(r#""platform_tools""#).unwrap();
        assert_eq!(requested, AdbEngine::PlatformTools);
    }
}

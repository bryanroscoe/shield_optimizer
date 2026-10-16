//! Isolated debug app: real quit coordination, no ADB or update/install capability.
#[cfg(not(debug_assertions))]
compile_error!("dev-exit is restricted to debug builds");
use super::*;
use serde::{Deserialize, Serialize};
use tauri::{Manager, Runtime};

struct UnavailableStop;
#[async_trait]
impl Stop for UnavailableStop {
    async fn execute(&self) -> StopResult {
        unreachable!("Stop is rejected before commitment")
    }
}
struct AppResources<R: Runtime>(tauri::AppHandle<R>);
#[async_trait]
impl<R: Runtime> Resources for AppResources<R> {
    async fn close_local_and_reap(&self) -> Result<(), CleanupFailure> {
        let state = self.0.state::<crate::commands::AppState>();
        let closes: Vec<_> = state
            .remote_sessions
            .lock()
            .await
            .drain()
            .map(|(_, session)| tokio::spawn(session.close_local()))
            .collect();
        let mut failed = false;
        for close in closes {
            failed |= close.await.is_err();
        }
        if failed {
            Err(CleanupFailure::TaskFailed)
        } else {
            Ok(())
        }
    }
}
trait Delivery: Send + Sync {
    fn show_status(&self) -> Result<(), String> {
        Ok(())
    }
    fn deliver(&self, coordinator: Arc<Coordinator>, token: ExitToken) -> Result<(), String>;
}
struct NativeDelivery<R: Runtime>(tauri::AppHandle<R>);
impl<R: Runtime> Delivery for NativeDelivery<R> {
    fn show_status(&self) -> Result<(), String> {
        let window = self
            .0
            .get_webview_window("main")
            .ok_or("Main window unavailable.")?;
        if window.url().map_err(|error| error.to_string())?.path() != "/dev/native-exit" {
            window
                .navigate("http://localhost:1420/dev/native-exit".parse().unwrap())
                .map_err(|error| error.to_string())?;
        }
        window.show().map_err(|error| error.to_string())?;
        window.set_focus().map_err(|error| error.to_string())
    }
    fn deliver(&self, coordinator: Arc<Coordinator>, token: ExitToken) -> Result<(), String> {
        let app = self.0.clone();
        self.0
            .run_on_main_thread(move || {
                final_delivery(
                    &coordinator,
                    token,
                    || app.cleanup_before_exit(),
                    || std::process::exit(0),
                );
            })
            .map_err(|error| error.to_string())
    }
}
fn final_delivery(
    coordinator: &Coordinator,
    token: ExitToken,
    cleanup: impl FnOnce(),
    exit: impl FnOnce(),
) -> bool {
    if !coordinator.allow_exit(Event::Programmatic(token)) {
        return false;
    }
    cleanup();
    exit();
    true
}
struct DevState {
    coordinator: Arc<Coordinator>,
    delivery: Arc<dyn Delivery>,
    warning: Mutex<Option<String>>,
    error: Mutex<Option<String>>,
    started: std::sync::atomic::AtomicBool,
    queue: std::sync::OnceLock<tokio::sync::mpsc::UnboundedSender<Work>>,
}
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum Action {
    Read,
    RequestQuit,
    Save {
        policy: ExitPolicy,
    },
    Confirm {
        token: String,
        policy: ExitPolicy,
        remember: bool,
    },
    Cancel {
        token: String,
    },
    ExitAfterFailure {
        token: String,
    },
    RetryDelivery {
        token: String,
    },
}
#[derive(Serialize)]
struct Snapshot {
    preference: Preference,
    phase: String,
    token: Option<String>,
    prompt_open: bool,
    error: Option<String>,
    warning: Option<String>,
    stop_available: bool,
}
struct Work {
    action: Action,
    reply: Option<tokio::sync::oneshot::Sender<Result<Snapshot, String>>>,
}
impl DevState {
    fn token(ticket: Ticket) -> String {
        format!("{}:{}", ticket.session, ticket.attempt.0)
    }
    fn ticket(&self, token: &str) -> Result<Ticket, String> {
        self.coordinator
            .state
            .lock()
            .unwrap()
            .pending
            .filter(|ticket| Self::token(*ticket) == token)
            .ok_or_else(|| "This prompt is no longer current.".to_owned())
    }
    fn snapshot(&self) -> Snapshot {
        let state = self.coordinator.state.lock().unwrap();
        let phase = self.coordinator.controller.phase();
        Snapshot {
            preference: self.coordinator.controller.preference(),
            phase: format!("{phase:?}"),
            token: state.pending.map(Self::token),
            prompt_open: phase == Phase::Prompting,
            error: self.error.lock().unwrap().clone(),
            warning: self.warning.lock().unwrap().clone(),
            stop_available: false,
        }
    }
    fn start(self: &Arc<Self>, ticket: Ticket) {
        if self.started.swap(true, Ordering::SeqCst) {
            return;
        }
        let state = self.clone();
        // This owned task survives cancellation/navigation of the invoking webview.
        tauri::async_runtime::spawn(async move {
            match state.coordinator.clone().run(ticket, false).await {
                Ok(token) => {
                    if let Err(error) = state.delivery.deliver(state.coordinator.clone(), token) {
                        *state.error.lock().unwrap() = Some(error);
                        if let Err(reveal) = state.delivery.show_status() {
                            let mut error = state.error.lock().unwrap();
                            *error = Some(format!(
                                "{}; status window: {reveal}",
                                error.as_deref().unwrap_or("Exit delivery failed")
                            ));
                        }
                    }
                }
                Err(error) => {
                    *state.error.lock().unwrap() = Some(format!(
                        "Quit paused: {error:?}. Shared ADB was not stopped."
                    ));
                    if let Err(error) = state.delivery.show_status() {
                        *state.error.lock().unwrap() = Some(error);
                    }
                }
            }
        });
    }
    fn start_queue(self: &Arc<Self>) {
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel::<Work>();
        self.queue
            .set(sender)
            .unwrap_or_else(|_| panic!("native queue initialized twice"));
        let state = Arc::downgrade(self);
        tauri::async_runtime::spawn(async move {
            while let Some(work) = receiver.recv().await {
                let Some(state) = state.upgrade() else {
                    break;
                };
                let result = state.apply(work.action).await;
                if let Err(error) = &result {
                    *state.error.lock().unwrap() = Some(error.clone());
                }
                if let Some(reply) = work.reply {
                    let _ = reply.send(result);
                }
            }
        });
    }
    fn enqueue_native_request(self: &Arc<Self>) {
        // No coordinator/store lock on the event thread. FIFO ownership survives IPC cancellation.
        if self
            .queue
            .get()
            .expect("native queue initialized")
            .send(Work {
                action: Action::RequestQuit,
                reply: None,
            })
            .is_err()
        {
            tracing::error!("Native coordinator queue unavailable; exit remains prevented");
        }
    }
    async fn command(&self, action: Action) -> Result<Snapshot, String> {
        let (reply, response) = tokio::sync::oneshot::channel();
        self.queue
            .get()
            .ok_or("Native coordinator not initialized.")?
            .send(Work {
                action,
                reply: Some(reply),
            })
            .map_err(|_| "Native coordinator unavailable.")?;
        response
            .await
            .map_err(|_| "Native coordinator response unavailable.".to_owned())?
    }
    fn request(self: &Arc<Self>) -> Result<(), String> {
        let ticket = self
            .coordinator
            .prompt()
            .map_err(|error| format!("{error:?}"))?;
        let phase = self.coordinator.controller.phase();
        if phase == Phase::Draining {
            self.start(ticket);
        } else if phase == Phase::Prompting
            || matches!(phase, Phase::Failed(_))
            || (phase == Phase::Finished
                && self
                    .coordinator
                    .recover_exit(ticket)
                    .map_err(|error| format!("{error:?}"))?
                    .is_some())
        {
            self.delivery.show_status()?;
        }
        Ok(())
    }
    async fn apply(self: &Arc<Self>, action: Action) -> Result<Snapshot, String> {
        match action {
            Action::Read => {}
            Action::RequestQuit => self.request()?,
            Action::Save { policy } => {
                if policy == ExitPolicy::AlwaysStop {
                    return Err("Always stop is unavailable in this development app.".to_owned());
                }
                let state = self.clone();
                tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
                    let _transaction = state.coordinator.state.lock().unwrap();
                    if state.coordinator.controller.phase() != Phase::Running {
                        return Err("Quit is already pending.".to_owned());
                    }
                    let preference = Preference::remembered(policy);
                    state
                        .coordinator
                        .store
                        .persist(preference)
                        .map_err(|error| format!("{error:?}"))?;
                    state.coordinator.controller.0.lock().unwrap().preference = preference;
                    *state.warning.lock().unwrap() = None;
                    *state.error.lock().unwrap() = None;
                    Ok(())
                })
                .await
                .map_err(|error| error.to_string())??;
            }
            Action::Confirm {
                token,
                policy,
                remember,
            } => {
                if policy == ExitPolicy::AlwaysStop {
                    return Err("Always stop is unavailable in this development app.".to_owned());
                }
                let ticket = self.ticket(&token)?;
                self.coordinator
                    .clone()
                    .confirm(ticket, policy, remember)
                    .await
                    .map_err(|error| format!("{error:?}"))?;
                if remember {
                    *self.warning.lock().unwrap() = None;
                }
                *self.error.lock().unwrap() = None;
                self.start(ticket);
            }
            Action::Cancel { token } => {
                self.coordinator
                    .cancel(self.ticket(&token)?)
                    .map_err(|error| format!("{error:?}"))?;
                *self.error.lock().unwrap() = None;
            }
            Action::ExitAfterFailure { token } => {
                let exit = self
                    .coordinator
                    .exit_without_stop(self.ticket(&token)?)
                    .map_err(|error| format!("{error:?}"))?;
                self.delivery.deliver(self.coordinator.clone(), exit)?;
            }
            Action::RetryDelivery { token } => {
                let exit = self
                    .coordinator
                    .recover_exit(self.ticket(&token)?)
                    .map_err(|error| format!("{error:?}"))?
                    .ok_or("No completed exit is awaiting delivery.")?;
                self.delivery.deliver(self.coordinator.clone(), exit)?;
            }
        }
        Ok(self.snapshot())
    }
}
#[tauri::command]
async fn dev_exit_action(
    state: tauri::State<'_, Arc<DevState>>,
    action: Action,
) -> Result<Snapshot, String> {
    state.inner().command(action).await
}
fn handler<R: Runtime>() -> impl Fn(tauri::ipc::Invoke<R>) -> bool + Send + Sync + 'static {
    let native: Box<dyn Fn(tauri::ipc::Invoke<R>) -> bool + Send + Sync> =
        Box::new(tauri::generate_handler![dev_exit_action]);
    move |invoke: tauri::ipc::Invoke<R>| {
        if invoke.message.command() == "dev_exit_action" {
            native(invoke)
        } else {
            invoke.resolver.reject(
                "Isolated native-exit development app: device/install commands are disabled.",
            );
            true
        }
    }
}
fn route<R: Runtime>(app: &tauri::AppHandle<R>, event: &tauri::RunEvent) {
    match event {
        // Hold the last/main window so a first-quit prompt remains visible. No hide-to-tray behavior is added.
        tauri::RunEvent::WindowEvent {
            label,
            event: tauri::WindowEvent::CloseRequested { api, .. },
            ..
        } if label == "main" && app.webview_windows().len() == 1 => {
            api.prevent_close();
            app.state::<Arc<DevState>>().enqueue_native_request();
        }
        tauri::RunEvent::ExitRequested { api, .. } => {
            api.prevent_exit();
            app.state::<Arc<DevState>>().enqueue_native_request();
        }
        _ => {}
    }
}
pub(crate) fn run() {
    let data_dir = std::env::var_os("SHIELD_DEV_EXIT_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            crate::default_data_dir().with_file_name("ShieldOptimizerDevelopmentExit")
        });
    std::fs::create_dir_all(&data_dir).expect("create isolated development data directory");
    let store = Arc::new(NativePreferences::new(&data_dir).expect("native preference store"));
    let (mut preference, warning) = store.load();
    let warning = if preference.policy == ExitPolicy::AlwaysStop {
        preference = Preference::default();
        Some("Stored Stop is unavailable; using Leave without remembering.".to_owned())
    } else if warning {
        Some("Invalid preferences; using Leave without overwriting them.".to_owned())
    } else {
        None
    };
    let factory = crate::adb::driver::DriverFactory::with_preference(preference);
    let controller = factory.controller();
    let state = crate::commands::AppState::new(
        Arc::new(crate::NoAdbDriver),
        crate::engine::AppListBundle::default(),
        data_dir,
    );
    let mut context = tauri::generate_context!();
    context.config_mut().identifier = "com.shieldoptimizer.native-exit-development".to_owned();
    tauri::Builder::default()
        // No updater/process plugin exists in this development app; their direct exit capabilities are unavailable.
        .manage(state)
        .manage(factory)
        .setup(move |app| {
            let coordinator = Coordinator::new(
                controller,
                store,
                Arc::new(AppResources(app.handle().clone())),
                Arc::new(UnavailableStop),
            );
            let state = Arc::new(DevState {
                coordinator,
                delivery: Arc::new(NativeDelivery(app.handle().clone())),
                warning: Mutex::new(warning),
                error: Mutex::new(None),
                started: std::sync::atomic::AtomicBool::new(false),
                queue: std::sync::OnceLock::new(),
            });
            state.start_queue();
            app.manage(state.clone());
            if let Some(window) = app.get_webview_window("main") {
                window.set_title("ATV Optimizer — isolated native quit development")?;
                window.navigate("http://localhost:1420/dev/native-exit".parse()?)?;
            }
            Ok(())
        })
        .invoke_handler(handler())
        .build(context)
        .expect("build isolated development app")
        .run(|app, event| route(app, &event));
}

#[cfg(test)]
mod tests;

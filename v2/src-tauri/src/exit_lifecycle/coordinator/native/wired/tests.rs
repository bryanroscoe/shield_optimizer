use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use tauri::test::{mock_builder, mock_context, noop_assets};

#[derive(Default)]
struct FakeDelivery(AtomicUsize);
impl Delivery for FakeDelivery {
    fn deliver(&self, coordinator: Arc<Coordinator>, token: ExitToken) -> Result<(), String> {
        final_delivery(
            &coordinator,
            token,
            || {},
            || {
                self.0.fetch_add(1, Ordering::SeqCst);
            },
        );
        Ok(())
    }
}
fn fixture() -> (
    tempfile::TempDir,
    tauri::App<tauri::test::MockRuntime>,
    Arc<DevState>,
    Arc<FakeDelivery>,
    crate::adb::driver::DriverFactory,
) {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(NativePreferences::new(dir.path()).unwrap());
    let factory = crate::adb::driver::DriverFactory::default();
    let app = mock_builder()
        .manage(crate::commands::AppState::new(
            Arc::new(crate::NoAdbDriver),
            crate::engine::AppListBundle::default(),
            dir.path().to_owned(),
        ))
        .invoke_handler(handler())
        .build(mock_context(noop_assets()))
        .unwrap();
    let delivery = Arc::new(FakeDelivery::default());
    let coordinator = Coordinator::new(
        factory.controller(),
        store,
        Arc::new(AppResources(app.handle().clone())),
        Arc::new(UnavailableStop),
    );
    let state = Arc::new(DevState {
        coordinator,
        delivery: delivery.clone(),
        warning: Mutex::new(None),
        error: Mutex::new(None),
        started: std::sync::atomic::AtomicBool::new(false),
    });
    app.manage(state.clone());
    (dir, app, state, delivery, factory)
}
async fn finished(state: &DevState) {
    tokio::time::timeout(Duration::from_secs(3), async {
        while !state.coordinator.state.lock().unwrap().completed {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn native_request_confirm_uses_factory_gate_and_real_store_with_fake_exit() {
    let (dir, _app, state, delivery, factory) = fixture();
    state.request().unwrap();
    let first = state.snapshot().token.unwrap();
    state.request().unwrap();
    assert_eq!(state.snapshot().token.as_ref(), Some(&first));
    assert!(factory.operation().is_ok());
    state
        .apply(Action::Confirm {
            token: first,
            policy: ExitPolicy::Auto,
            remember: true,
        })
        .await
        .unwrap();
    assert!(factory.operation().is_err());
    finished(&state).await;
    tokio::time::timeout(Duration::from_secs(3), async {
        while delivery.0.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(delivery.0.load(Ordering::SeqCst), 1);
    assert_eq!(
        state.coordinator.controller.outcome(),
        Some(Outcome::AutoLeftRunningOwnershipUnknown)
    );
    let store = NativePreferences::new(dir.path()).unwrap();
    assert_eq!(store.load().0, Preference::remembered(ExitPolicy::Auto));
    assert!(!dir.path().join("development-previews").exists());
}

#[tokio::test]
async fn native_stale_actions_cancel_and_unavailable_stop_preserve_new_prompt() {
    let (dir, _app, state, delivery, factory) = fixture();
    state.request().unwrap();
    let old = state.snapshot().token.unwrap();
    state
        .apply(Action::Cancel { token: old.clone() })
        .await
        .unwrap();
    state.request().unwrap();
    let current = state.snapshot().token.unwrap();
    for action in [
        Action::Cancel { token: old.clone() },
        Action::Confirm {
            token: old,
            policy: ExitPolicy::Auto,
            remember: true,
        },
        Action::Confirm {
            token: current.clone(),
            policy: ExitPolicy::AlwaysStop,
            remember: true,
        },
    ] {
        assert!(state.apply(action).await.is_err());
        assert_eq!(state.snapshot().token.as_ref(), Some(&current));
        assert!(state.snapshot().prompt_open);
    }
    assert!(factory.operation().is_ok());
    assert_eq!(delivery.0.load(Ordering::SeqCst), 0);
    assert!(!dir.path().join("exit-settings-v1.json").exists());
}

#[tokio::test]
async fn real_tauri_ipc_rejects_device_install_and_update_commands() {
    use tauri::test::{get_ipc_response, INVOKE_KEY};
    use tauri::webview::InvokeRequest;
    let (_dir, app, state, delivery, _) = fixture();
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    for command in [
        "connect_device",
        "install_adb",
        "restart_adb",
        "scan_network",
        "plugin:process|restart",
        "plugin:updater|download_and_install",
    ] {
        let request = InvokeRequest {
            cmd: command.to_owned(),
            callback: tauri::ipc::CallbackFn(0),
            error: tauri::ipc::CallbackFn(1),
            url: "tauri://localhost".parse().unwrap(),
            body: tauri::ipc::InvokeBody::Json(serde_json::json!({})),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_owned(),
        };
        let result = get_ipc_response(&webview, request);
        assert!(result.is_err(), "{command} unexpectedly accepted");
    }
    let request = InvokeRequest {
        cmd: "dev_exit_action".to_owned(),
        callback: tauri::ipc::CallbackFn(0),
        error: tauri::ipc::CallbackFn(1),
        url: "tauri://localhost".parse().unwrap(),
        body: tauri::ipc::InvokeBody::Json(serde_json::json!({"action":{"type":"request_quit"}})),
        headers: Default::default(),
        invoke_key: INVOKE_KEY.to_owned(),
    };
    let body = tokio::task::spawn_blocking(move || get_ipc_response(&webview, request))
        .await
        .unwrap()
        .unwrap()
        .deserialize::<serde_json::Value>()
        .unwrap();
    assert_eq!(body["prompt_open"], true);
    assert_eq!(state.coordinator.controller.phase(), Phase::Prompting);
    assert_eq!(delivery.0.load(Ordering::SeqCst), 0);
}

#[test]
fn native_unrelated_run_events_do_not_create_exit_authorization() {
    let (_dir, app, state, delivery, _) = fixture();
    route(app.handle(), &tauri::RunEvent::Ready);
    assert_eq!(state.coordinator.controller.phase(), Phase::Running);
    assert_eq!(delivery.0.load(Ordering::SeqCst), 0);
}

#[cfg(unix)]
#[tokio::test]
async fn wired_teardown_drains_populated_appstate_map_with_local_fixture_only() {
    use shield_optimizer_core::adb::{AdbChild, AdbDriver, AdbOutput, AdbResult, ChildCompletion};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    struct Completion(ChildLease, Arc<AtomicUsize>);
    impl ChildCompletion for Completion {
        fn reaped(self: Box<Self>) {
            self.0.reaped();
            self.1.fetch_add(1, Ordering::SeqCst);
        }
    }
    struct FixtureDriver {
        controller: Controller,
        calls: AtomicUsize,
        reaped: Arc<AtomicUsize>,
        closed: Arc<AtomicUsize>,
    }
    #[async_trait]
    impl AdbDriver for FixtureDriver {
        async fn raw(&self, args: &[&str]) -> AdbResult<AdbOutput> {
            assert!(
                !args.contains(&"--remove"),
                "local teardown must not request ADB cleanup"
            );
            self.calls.fetch_add(1, Ordering::SeqCst);
            if args.contains(&"forward") {
                let port = args
                    .iter()
                    .find_map(|arg| arg.strip_prefix("tcp:"))
                    .unwrap()
                    .parse::<u16>()
                    .unwrap();
                let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
                    .await
                    .unwrap();
                let closed = self.closed.clone();
                tokio::spawn(async move {
                    let (mut peer, _) = listener.accept().await.unwrap();
                    peer.write_all(&[0]).await.unwrap();
                    assert_eq!(peer.read(&mut [0]).await.unwrap(), 0);
                    closed.fetch_add(1, Ordering::SeqCst);
                });
            }
            Ok(AdbOutput {
                stdout: String::new(),
                stderr: String::new(),
                exit_code: Some(0),
            })
        }
        async fn shell(&self, _: &str, _: &str) -> AdbResult<AdbOutput> {
            panic!("no device shell")
        }
        async fn spawn(&self, _: &[&str]) -> AdbResult<AdbChild> {
            let operation = self.controller.admit_operation().unwrap();
            let lease = operation.register_child().unwrap();
            let child = tokio::process::Command::new("/bin/sleep")
                .arg("30")
                .kill_on_drop(true)
                .spawn()
                .unwrap();
            Ok(AdbChild::new(
                child,
                Some(Box::new(Completion(lease, self.reaped.clone()))),
            ))
        }
    }
    let (_dir, app, state, delivery, factory) = fixture();
    let driver = Arc::new(FixtureDriver {
        controller: factory.controller(),
        calls: AtomicUsize::new(0),
        reaped: Arc::new(AtomicUsize::new(0)),
        closed: Arc::new(AtomicUsize::new(0)),
    });
    let session = shield_optimizer_core::adb::remote_input::RemoteInputSession::start(
        driver.clone(),
        Path::new("fixture.jar"),
        "fixture-only",
    )
    .await
    .unwrap();
    let calls = driver.calls.load(Ordering::SeqCst);
    app.state::<crate::commands::AppState>()
        .remote_sessions
        .lock()
        .await
        .insert("fixture-only".to_owned(), session);
    state.request().unwrap();
    let token = state.snapshot().token.unwrap();
    state
        .apply(Action::Confirm {
            token,
            policy: ExitPolicy::AlwaysLeaveRunning,
            remember: false,
        })
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        while delivery.0.load(Ordering::SeqCst) == 0 || driver.closed.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(app
        .state::<crate::commands::AppState>()
        .remote_sessions
        .lock()
        .await
        .is_empty());
    assert_eq!(driver.reaped.load(Ordering::SeqCst), 1);
    assert_eq!(driver.calls.load(Ordering::SeqCst), calls);
    assert_eq!(delivery.0.load(Ordering::SeqCst), 1);
    assert!(state.error.lock().unwrap().is_none());
}

#[test]
fn pinned_mock_loop_routes_native_close_and_exit_requests_through_same_controller() {
    let (_dir, app, state, delivery, _) = fixture();
    let main = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    let seen_close = Arc::new(AtomicUsize::new(0));
    let seen_exit = Arc::new(AtomicUsize::new(0));
    let (close_count, exit_count) = (seen_close.clone(), seen_exit.clone());
    let controller = state.coordinator.clone();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        app.run_return(move |app, event| {
            match &event {
                tauri::RunEvent::Ready => {
                    main.close().unwrap();
                }
                tauri::RunEvent::WindowEvent {
                    event: tauri::WindowEvent::CloseRequested { .. },
                    ..
                } => {
                    route(app, &event);
                    close_count.fetch_add(1, Ordering::SeqCst);
                    assert_eq!(controller.controller.phase(), Phase::Prompting);
                    let ticket = controller.state.lock().unwrap().pending.unwrap();
                    controller.cancel(ticket).unwrap();
                    assert_eq!(controller.controller.phase(), Phase::Running);
                    main.destroy().unwrap();
                }
                tauri::RunEvent::ExitRequested { .. } => {
                    route(app, &event);
                    exit_count.fetch_add(1, Ordering::SeqCst);
                    assert_eq!(controller.controller.phase(), Phase::Prompting);
                    // MockRuntime has no supported stop API after preventing exit.
                    // Unwind only the fixture loop; never authorize an unrelated exit event.
                    panic!("native-route-harness-complete");
                }
                _ => route(app, &event),
            }
        });
    }));
    let reason = result.expect_err("fixture loop must terminate through its sentinel");
    assert_eq!(
        reason.downcast_ref::<&str>().copied(),
        Some("native-route-harness-complete")
    );
    assert_eq!(seen_close.load(Ordering::SeqCst), 1);
    assert_eq!(seen_exit.load(Ordering::SeqCst), 1);
    assert_eq!(delivery.0.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn saving_native_preferences_keeps_existing_factory_permits_and_rejects_stop() {
    let (dir, _app, state, delivery, factory) = fixture();
    let operation = factory.operation().unwrap();
    assert!(state
        .apply(Action::Save {
            policy: ExitPolicy::AlwaysStop
        })
        .await
        .is_err());
    assert!(!dir.path().join("exit-settings-v1.json").exists());
    state
        .apply(Action::Save {
            policy: ExitPolicy::Auto,
        })
        .await
        .unwrap();
    operation.register_child().unwrap().reaped();
    assert!(factory.operation().is_ok());
    assert_eq!(
        NativePreferences::new(dir.path()).unwrap().load().0,
        Preference::remembered(ExitPolicy::Auto)
    );
    state.request().unwrap();
    assert_eq!(state.coordinator.controller.phase(), Phase::Draining);
    state.request().unwrap();
    state.request().unwrap();
    drop(operation);
    tokio::time::timeout(Duration::from_secs(3), async {
        while delivery.0.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(delivery.0.load(Ordering::SeqCst), 1);
    assert!(state.error.lock().unwrap().is_none());
}

#[tokio::test]
async fn owned_final_delivery_consumes_token_then_cleans_and_exits_once() {
    let (_dir, _app, state, _, _) = fixture();
    let ticket = state.coordinator.prompt().unwrap();
    state
        .coordinator
        .clone()
        .confirm(ticket, ExitPolicy::AlwaysLeaveRunning, false)
        .await
        .unwrap();
    let token = state.coordinator.clone().run(ticket, false).await.unwrap();
    let order = Mutex::new(Vec::new());
    assert!(final_delivery(
        &state.coordinator,
        token,
        || order.lock().unwrap().push("cleanup"),
        || order.lock().unwrap().push("exit")
    ));
    assert_eq!(*order.lock().unwrap(), vec!["cleanup", "exit"]);
    assert!(!final_delivery(
        &state.coordinator,
        token,
        || panic!("duplicate cleanup"),
        || panic!("duplicate exit")
    ));
}

#[test]
fn native_prompt_reveal_returns_to_settings_without_reloading_current_prompt() {
    let (_dir, app, _, _, _) = fixture();
    let main = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    let delivery = NativeDelivery(app.handle().clone());
    delivery.show_status().unwrap();
    assert_eq!(main.url().unwrap().path(), "/dev/native-exit");
    delivery.show_status().unwrap();
    assert_eq!(main.url().unwrap().path(), "/dev/native-exit");
}

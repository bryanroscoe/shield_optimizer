use super::*;
use crate::exit_lifecycle::{Attempt, ExitPolicy, Rejected, Request};
use shield_optimizer_core::commands::AppState;
use std::os::unix::fs::PermissionsExt;

fn fixture(script: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("inert-client");
    std::fs::write(&path, format!("#!/bin/sh\n{script}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
    (dir, path)
}

fn commit(factory: &DriverFactory) -> Attempt {
    let Request::Prompt(attempt) = factory.controller.request_quit(false) else {
        panic!("prompt");
    };
    factory
        .controller
        .confirm(attempt, ExitPolicy::AlwaysStop, false)
        .unwrap();
    attempt
}

async fn ready_after_reaping(factory: &DriverFactory, attempt: Attempt) {
    timeout(Duration::from_secs(3), async {
        loop {
            match factory.controller.finish_drain(attempt) {
                Ok(()) => break,
                Err(Rejected::WorkOutstanding) => tokio::task::yield_now().await,
                other => panic!("drain: {other:?}"),
            }
        }
        loop {
            match factory.controller.finish_local_teardown(attempt) {
                Ok(()) => break,
                Err(Rejected::ChildOutstanding) => tokio::task::yield_now().await,
                other => panic!("teardown: {other:?}"),
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(factory.controller.phase(), Phase::ReadyToStop);
    // Deliberately never take or execute a final server-stop capability.
}

async fn file_exists(path: &Path) {
    timeout(Duration::from_secs(3), async {
        while !path.exists() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn all_finite_subprocess_paths_capture_and_reap() {
    let (_dir, binary) = fixture("printf output; printf diagnostic >&2");
    let factory = DriverFactory::default();
    let driver = factory.create(binary);
    for output in [
        driver.raw(&["fixture"]).await.unwrap(),
        driver.raw_transfer(&["fixture"]).await.unwrap(),
        driver.shell("fixture", "ignored").await.unwrap(),
    ] {
        assert_eq!(output.stdout, "output");
        assert_eq!(output.stderr, "diagnostic");
    }
    assert_eq!(driver.raw_bytes(&["fixture"]).await.unwrap(), b"output");
    let bounded = driver.shell_bounded("fixture", "ignored").await.unwrap();
    assert_eq!(bounded.stdout, "output");
    assert_eq!(bounded.termination, ShellTermination::Completed);
    ready_after_reaping(&factory, commit(&factory)).await;
}

#[tokio::test]
async fn scope_spans_multiple_steps_old_clones_and_replacement() {
    let (dir, binary) = fixture("printf x >> \"$(dirname \"$0\")/calls\"");
    let factory = DriverFactory::default();
    let old = factory.create(binary.clone());
    let state = Arc::new(AppState::new(
        Arc::new(old.clone()),
        crate::engine::AppListBundle::default(),
        dir.path().to_owned(),
    ));
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let (resume_tx, resume_rx) = tokio::sync::oneshot::channel();
    let copy = state.clone();
    let job = tokio::spawn(async move {
        copy.with_adb_operation(async {
            copy.adb_operation()
                .await?
                .raw(&["first"])
                .await
                .map_err(|e| e.to_string())?;
            started_tx.send(()).unwrap();
            resume_rx.await.unwrap();
            copy.with_adb_operation(async {
                copy.adb_operation()
                    .await?
                    .raw(&["second"])
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(())
            })
            .await
        })
        .await
    });
    started_rx.await.unwrap();
    let attempt = commit(&factory);
    let replacement = factory.create(binary);
    state.replace_adb(Arc::new(replacement.clone())).await;
    assert!(old.raw(&["late-old"]).await.is_err());
    assert!(replacement.raw(&["late-new"]).await.is_err());
    assert_eq!(
        factory.controller.finish_drain(attempt),
        Err(Rejected::WorkOutstanding)
    );
    resume_tx.send(()).unwrap();
    job.await.unwrap().unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.path().join("calls")).unwrap(),
        "xx"
    );
    ready_after_reaping(&factory, attempt).await;
}

#[tokio::test]
async fn cancellation_before_poll_never_spawns() {
    let (dir, binary) = fixture("touch \"$(dirname \"$0\")/started\"");
    let factory = DriverFactory::default();
    let driver = factory.create(binary);
    drop(driver.raw(&["fixture"]));
    assert!(!dir.path().join("started").exists());
    ready_after_reaping(&factory, commit(&factory)).await;
}

#[tokio::test]
async fn spawn_failure_releases_registration_without_claiming_a_child() {
    let dir = tempfile::tempdir().unwrap();
    let factory = DriverFactory::default();
    // A directory exists but cannot be executed. No real adb path is consulted.
    assert!(factory
        .create(dir.path().to_owned())
        .raw(&[])
        .await
        .is_err());
    ready_after_reaping(&factory, commit(&factory)).await;
}

#[tokio::test]
async fn timeout_kills_and_reaps_before_completion_eligibility() {
    let (_dir, binary) = fixture("exec /bin/sleep 30");
    let factory = DriverFactory::default();
    let driver = factory
        .create(binary)
        .with_timeout(Duration::from_millis(25));
    assert!(matches!(
        driver.raw(&[]).await,
        Err(AdbError::Timeout { .. })
    ));
    ready_after_reaping(&factory, commit(&factory)).await;
}

#[tokio::test]
async fn caller_abort_transfers_child_to_reaper() {
    let (dir, binary) = fixture("touch \"$(dirname \"$0\")/started\"; exec /bin/sleep 30");
    let factory = DriverFactory::default();
    let driver = factory.create(binary);
    let job = tokio::spawn(async move { driver.raw(&[]).await });
    file_exists(&dir.path().join("started")).await;
    job.abort();
    assert!(job.await.unwrap_err().is_cancelled());
    ready_after_reaping(&factory, commit(&factory)).await;
}

#[tokio::test]
async fn long_lived_child_remains_outstanding_until_wait_reaps_it() {
    let (_dir, binary) = fixture("exec /bin/sleep 30");
    let factory = DriverFactory::default();
    let mut child = factory.create(binary).spawn(&[]).await.unwrap();
    let attempt = commit(&factory);
    factory.controller.finish_drain(attempt).unwrap();
    assert_eq!(
        factory.controller.finish_local_teardown(attempt),
        Err(Rejected::ChildOutstanding)
    );
    child.start_kill().unwrap();
    child.wait().await.unwrap();
    factory.controller.finish_local_teardown(attempt).unwrap();
    assert_eq!(factory.controller.phase(), Phase::ReadyToStop);
}

#[tokio::test]
async fn dropped_resident_child_is_reaped_without_adb_cleanup_launch() {
    let (dir, binary) = fixture("printf x >> \"$(dirname \"$0\")/calls\"; exec /bin/sleep 30");
    let factory = DriverFactory::default();
    let child = factory.create(binary).spawn(&[]).await.unwrap();
    file_exists(&dir.path().join("calls")).await;
    let attempt = commit(&factory);
    drop(child);
    ready_after_reaping(&factory, attempt).await;
    assert_eq!(
        std::fs::read_to_string(dir.path().join("calls")).unwrap(),
        "x"
    );
}

#[tokio::test]
async fn bounded_output_limit_reaps_the_actual_fixture_child() {
    let (_dir, binary) = fixture("while :; do printf 0123456789; done");
    let factory = DriverFactory::default();
    let result = factory
        .create(binary)
        .shell_bounded("fixture", "ignored")
        .await
        .unwrap();
    assert_eq!(result.termination, ShellTermination::OutputLimit);
    assert!(result.stdout.len() <= SHELL_OUTPUT_LIMIT);
    ready_after_reaping(&factory, commit(&factory)).await;
}

#[tokio::test]
async fn inherited_open_pipes_do_not_escape_the_callers_timeout() {
    let (_dir, binary) = fixture("/bin/sleep 1 &\nprintf output");
    let factory = DriverFactory::default();
    let driver = factory
        .create(binary)
        .with_timeout(Duration::from_millis(25));
    assert!(matches!(
        driver.raw(&[]).await,
        Err(AdbError::Timeout { .. })
    ));
    ready_after_reaping(&factory, commit(&factory)).await;
}

#[tokio::test]
async fn detached_resident_handle_does_not_hold_the_initiating_operation() {
    let (_dir, binary) = fixture("printf ok");
    let factory = DriverFactory::default();
    let scoped = factory.create(binary).operation_driver().unwrap().unwrap();
    let resident = scoped.background_driver().unwrap();
    drop(scoped);
    let attempt = commit(&factory);
    assert!(!resident.allow_background_cleanup());
    assert!(resident.raw(&["late-forward-remove"]).await.is_err());
    ready_after_reaping(&factory, attempt).await;
}

#[test]
fn every_desktop_dispatch_entry_has_a_guard_or_explicit_non_adb_classification() {
    let sources = [
        (
            "devices",
            include_str!("../../../../crates/core/src/commands/devices.rs"),
        ),
        (
            "health",
            include_str!("../../../../crates/core/src/commands/health.rs"),
        ),
        (
            "launcher",
            include_str!("../../../../crates/core/src/commands/launcher.rs"),
        ),
        (
            "apps",
            include_str!("../../../../crates/core/src/commands/apps.rs"),
        ),
        (
            "input",
            include_str!("../../../../crates/core/src/commands/input.rs"),
        ),
        (
            "snapshot",
            include_str!("../../../../crates/core/src/commands/snapshot.rs"),
        ),
        (
            "screenshot",
            include_str!("../../../../crates/core/src/commands/screenshot.rs"),
        ),
        (
            "recovery",
            include_str!("../../../../crates/core/src/commands/recovery.rs"),
        ),
        (
            "reboot",
            include_str!("../../../../crates/core/src/commands/reboot.rs"),
        ),
        (
            "shell",
            include_str!("../../../../crates/core/src/commands/shell.rs"),
        ),
        (
            "logcat",
            include_str!("../../../../crates/core/src/commands/logcat.rs"),
        ),
        (
            "tuning",
            include_str!("../../../../crates/core/src/commands/tuning.rs"),
        ),
        (
            "optimize",
            include_str!("../../../../crates/core/src/commands/optimize.rs"),
        ),
        ("install", include_str!("../../commands/install.rs")),
        ("scan", include_str!("../../commands/scan.rs")),
        ("sideload", include_str!("../../commands/sideload.rs")),
        ("backup", include_str!("../../commands/backup.rs")),
        ("files", include_str!("../../commands/files.rs")),
        ("update", include_str!("../../commands/update.rs")),
        ("app_report", include_str!("../../commands/app_report.rs")),
        ("diagnostics", include_str!("../../commands/diagnostics.rs")),
    ];
    let exempt: std::collections::HashSet<_> = [
        "scan::local_address_for",
        "apps::safety_info",
        "apps::process_safety_info",
        "sideload::list_apks_in_folder",
        "update::check_for_update",
        "app_report::save_app_report",
        "diagnostics::set_debug_logging",
        "diagnostics::get_debug_logging",
        "diagnostics::log_dir_path",
        "diagnostics::open_log_dir",
        "diagnostics::log_ui_event",
    ]
    .into_iter()
    .collect();
    let dispatch = include_str!("../../lib.rs")
        .split("tauri::generate_handler![")
        .nth(1)
        .unwrap()
        .split("\n    ]")
        .next()
        .unwrap();
    let mut seen_exempt = std::collections::HashSet::new();
    let mut guarded = 0;
    for entry in dispatch
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
    {
        if exempt.contains(entry) {
            seen_exempt.insert(entry);
            continue;
        }
        let (module, name) = entry.split_once("::").unwrap();
        let source = sources.iter().find(|(key, _)| *key == module).unwrap().1;
        let signature = regex::Regex::new(&format!(r"pub async fn {name}(?:<[^\n]+>)?\(")).unwrap();
        let start = signature
            .find(source)
            .unwrap_or_else(|| panic!("unclassified command {entry}"))
            .start();
        let body = source[start..].split("\n}").next().unwrap();
        assert!(
            body.contains(".with_adb_operation("),
            "missing whole-future guard: {entry}"
        );
        guarded += 1;
    }
    assert_eq!(seen_exempt, exempt);
    assert_eq!(guarded, 77);
}

#[cfg(feature = "e2e")]
#[tokio::test]
async fn real_tauri_dispatch_holds_operation_until_async_response() {
    use tauri::test::{get_ipc_response, mock_builder, mock_context, noop_assets, INVOKE_KEY};
    use tauri::webview::InvokeRequest;
    let (dir,binary)=fixture("touch \"$(dirname \"$0\")/started\"; while [ ! -f \"$(dirname \"$0\")/release\" ]; do /bin/sleep 0.01; done; printf 'connected to fixture' ");
    let factory = DriverFactory::default();
    let state = AppState::new(
        Arc::new(factory.create(binary)),
        crate::engine::AppListBundle::default(),
        dir.path().to_owned(),
    );
    let app = mock_builder()
        .manage(state)
        .manage(factory.clone())
        .invoke_handler(crate::invoke_handler())
        .build(mock_context(noop_assets()))
        .unwrap();
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    let request = InvokeRequest {
        cmd: "connect_device".to_owned(),
        callback: tauri::ipc::CallbackFn(0),
        error: tauri::ipc::CallbackFn(1),
        url: "tauri://localhost".parse().unwrap(),
        body: tauri::ipc::InvokeBody::Json(serde_json::json!({"address":"127.0.0.1:12345"})),
        headers: Default::default(),
        invoke_key: INVOKE_KEY.to_owned(),
    };
    let job = tokio::task::spawn_blocking(move || get_ipc_response(&webview, request));
    file_exists(&dir.path().join("started")).await;
    let attempt = commit(&factory);
    assert_eq!(
        factory.controller.finish_drain(attempt),
        Err(Rejected::WorkOutstanding)
    );
    std::fs::write(dir.path().join("release"), "").unwrap();
    let body = job
        .await
        .unwrap()
        .unwrap()
        .deserialize::<serde_json::Value>()
        .unwrap();
    assert_eq!(body["ok"], true);
    ready_after_reaping(&factory, attempt).await;
}

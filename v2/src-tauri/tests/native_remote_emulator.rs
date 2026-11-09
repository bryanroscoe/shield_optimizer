//! Real-adbd harness for the built-in engine's fast Remote.
//!
//! Never run by CI: every test is `#[ignore]` and also returns early unless
//! `ATVOPT_EMU_ADDR` (adbd endpoint), `ATVOPT_EMU_KEY` (an already-allowed
//! throwaway key) and `ATVOPT_EMU_ADB_SERIAL` (the same device as platform-tools
//! sees it, used only to observe the device) are set. Point it at an emulator:
//!
//! ```text
//! ATVOPT_EMU_ADDR=127.0.0.1:5555 ATVOPT_EMU_KEY=/path/allowed.pem \
//! ATVOPT_EMU_ADB_SERIAL=<platform-tools serial> ATVOPT_EMU_LOG=/path/remote.log \
//!   cargo test -p atv-optimizer --test native_remote_emulator -- --ignored --nocapture --test-threads=1
//! ```
//!
//! It drives the real `DirectNetworkAdb` and `AppState` Remote API with the
//! bundled scrcpy server, and checks the effect with platform-tools `shell`
//! observations (focus, `uiautomator`, `dumpsys input`, `/proc`). Optional:
//! `ATVOPT_EMU_TLS_ADDR` + `ATVOPT_EMU_TLS_KEY` for a paired Wireless debugging
//! endpoint, and `ATVOPT_EMU_LONG=1` for the stream-lifetime renewal case.
#![cfg(feature = "direct-adb")]

use std::fmt::Display;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use atv_optimizer_core::adb::AdbDriver;
use atv_optimizer_core::commands::AppState;
use atv_optimizer_core::engine::AppListBundle;
use atv_optimizer_lib::adb::direct::DirectNetworkAdb;

const HOME: u32 = 3;
const BACK: u32 = 4;
const UP: u32 = 19;
const DOWN: u32 = 20;
const LEFT: u32 = 21;
const RIGHT: u32 = 22;
/// KEYCODE_A and KEYCODE_B: one character each in a focused text field.
const KEY_A: u32 = 29;
const KEY_B: u32 = 30;

const SETTINGS: &str = "com.android.tv.settings";
const TEXT_FIELD: &str = "com.android.tv.settings/.connectivity.AddWifiNetworkActivity";
const HELPERS: &str = "for p in /proc/[0-9]*; do grep -qs shieldopt-remote- \"$p/environ\" && echo \"P ${p#/proc/}\"; done; for f in /data/local/tmp/shieldopt-remote-*; do [ -e \"$f\" ] && echo \"J ${f##*/}\"; done; true";

struct Config {
    addr: String,
    key: PathBuf,
    observe: String,
    platform_adb: String,
    log: Option<PathBuf>,
}

fn var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

impl Config {
    fn from_env() -> Option<Self> {
        let config = Self {
            addr: var("ATVOPT_EMU_ADDR")?,
            key: PathBuf::from(var("ATVOPT_EMU_KEY")?),
            observe: var("ATVOPT_EMU_ADB_SERIAL")?,
            platform_adb: var("ATVOPT_EMU_PLATFORM_ADB").unwrap_or_else(|| "adb".into()),
            log: var("ATVOPT_EMU_LOG").map(PathBuf::from),
        };
        Some(config)
    }
}

fn config_or_skip() -> Option<Config> {
    let config = Config::from_env();
    if config.is_none() {
        eprintln!("skipped: set ATVOPT_EMU_ADDR, ATVOPT_EMU_KEY and ATVOPT_EMU_ADB_SERIAL");
    }
    config
}

struct Report {
    log: Option<Mutex<std::fs::File>>,
    failures: Mutex<Vec<String>>,
    passes: Mutex<usize>,
}

impl Report {
    fn new(path: Option<&Path>) -> Self {
        let log = path.map(|path| {
            Mutex::new(
                std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                    .expect("open harness log"),
            )
        });
        Self {
            log,
            failures: Mutex::default(),
            passes: Mutex::default(),
        }
    }
    fn line(&self, text: String) {
        println!("{text}");
        if let Some(log) = &self.log {
            let _ = writeln!(log.lock().unwrap(), "{text}");
        }
    }
    fn info(&self, name: &str, detail: impl Display) {
        self.line(format!("INFO {name}: {detail}"));
    }
    fn check(&self, name: &str, ok: bool, detail: impl Display) {
        if ok {
            *self.passes.lock().unwrap() += 1;
            self.line(format!("PASS {name}: {detail}"));
        } else {
            self.failures.lock().unwrap().push(name.into());
            self.line(format!("FAIL {name}: {detail}"));
        }
    }
    fn finish(&self) {
        let failures = self.failures.lock().unwrap().clone();
        self.line(format!(
            "SUMMARY passed={} failed={} {:?}",
            self.passes.lock().unwrap(),
            failures.len(),
            failures
        ));
        assert!(failures.is_empty(), "failed scenarios: {failures:?}");
    }
}

/// Platform-tools observation of the same device. Every call names the device.
struct Observer {
    adb: String,
    serial: String,
}

#[derive(Debug, Default, PartialEq)]
struct Helpers {
    processes: Vec<String>,
    jars: Vec<String>,
}

impl Helpers {
    fn is_clean(&self) -> bool {
        self.processes.is_empty() && self.jars.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Node {
    class: String,
    text: String,
    bounds: String,
}

fn attribute(node: &str, name: &str) -> String {
    let key = format!(" {name}=\"");
    node.find(&key)
        .map(|start| {
            let rest = &node[start + key.len()..];
            rest[..rest.find('"').unwrap_or(rest.len())].to_string()
        })
        .unwrap_or_default()
}

fn unescape(text: &str) -> String {
    text.replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

impl Observer {
    fn shell(&self, script: &str) -> String {
        let output = std::process::Command::new(&self.adb)
            .args(["-s", &self.serial, "shell", script])
            .output()
            .expect("run platform-tools adb");
        String::from_utf8_lossy(&output.stdout).into_owned()
    }
    fn helpers(&self) -> Helpers {
        let mut helpers = Helpers::default();
        for line in self.shell(HELPERS).lines() {
            if let Some(pid) = line.strip_prefix("P ") {
                helpers.processes.push(pid.trim().into());
            } else if let Some(jar) = line.strip_prefix("J ") {
                helpers.jars.push(jar.trim().into());
            }
        }
        helpers
    }
    fn wait_clean(&self, timeout: Duration) -> Result<Duration, Helpers> {
        let started = Instant::now();
        loop {
            let helpers = self.helpers();
            if helpers.is_clean() {
                return Ok(started.elapsed());
            }
            if started.elapsed() >= timeout {
                return Err(helpers);
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }
    fn focus(&self) -> String {
        self.shell("dumpsys window | grep mCurrentFocus")
            .trim()
            .to_string()
    }
    fn wait_focus(&self, needle: &str, present: bool, timeout: Duration) -> Option<Duration> {
        let started = Instant::now();
        loop {
            if self.focus().contains(needle) == present {
                return Some(started.elapsed());
            }
            if started.elapsed() >= timeout {
                return None;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    /// Another agent may raise "Allow debugging?" on this device; wait it out.
    fn wait_no_prompt(&self, report: &Report) {
        for _ in 0..240 {
            let focus = self.focus();
            if !focus.contains("Debugging") && !focus.contains("systemui") {
                return;
            }
            report.info("prompt", format!("waiting for {focus}"));
            std::thread::sleep(Duration::from_millis(500));
        }
    }
    fn focused(&self) -> Option<Node> {
        let xml = self.shell(
            "uiautomator dump /data/local/tmp/atvopt-remote-ui.xml >/dev/null 2>&1; \
             cat /data/local/tmp/atvopt-remote-ui.xml; rm -f /data/local/tmp/atvopt-remote-ui.xml",
        );
        xml.rsplit("<node")
            .find(|node| node.contains("focused=\"true\""))
            .map(|node| Node {
                class: attribute(node, "class"),
                text: unescape(&attribute(node, "text")),
                bounds: attribute(node, "bounds"),
            })
    }
    /// Text of the focused text field, if a text field has focus.
    fn field(&self) -> Option<String> {
        self.focused()
            .filter(|node| node.class.ends_with("EditText"))
            .map(|node| node.text)
    }
    /// Opens an empty, focused text field. Each typed letter key lands as one
    /// character, so the field counts exactly how often a key reached the TV.
    fn open_field(&self) -> bool {
        self.start_activity(TEXT_FIELD);
        self.wait_focus("AddWifiNetworkActivity", true, Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(700));
        self.field().as_deref() == Some("")
    }
    fn start_activity(&self, component_or_action: &str) {
        // -S starts from a fresh task, so Back leaves it in one press.
        if component_or_action.contains('/') {
            self.shell(&format!(
                "am start -S -W -n {component_or_action} >/dev/null 2>&1"
            ));
        } else {
            self.shell(&format!(
                "am start -S -W -a {component_or_action} >/dev/null 2>&1"
            ));
        }
    }
}

fn millis(duration: Duration) -> String {
    format!("{:.1}ms", duration.as_secs_f64() * 1000.0)
}

fn stats(samples: &[Duration]) -> String {
    if samples.is_empty() {
        return "n=0".into();
    }
    let mut sorted = samples.to_vec();
    sorted.sort();
    let pick = |q: f64| sorted[((sorted.len() as f64 - 1.0) * q).round() as usize];
    format!(
        "n={} min={} p50={} p95={} max={}",
        sorted.len(),
        millis(sorted[0]),
        millis(pick(0.5)),
        millis(pick(0.95)),
        millis(sorted[sorted.len() - 1])
    )
}

fn jar() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/scrcpy-server-v3.1")
}

struct Ctx {
    report: Report,
    observer: Observer,
    driver: Arc<DirectNetworkAdb>,
    state: Arc<AppState>,
    jar: PathBuf,
    data: tempfile::TempDir,
}

async fn connect(driver: &DirectNetworkAdb, addr: &str, report: &Report, name: &str) -> bool {
    let started = Instant::now();
    let result = driver.raw(&["connect", addr]).await;
    let listed = driver
        .raw(&["devices"])
        .await
        .map(|out| out.stdout.contains(addr))
        .unwrap_or(false);
    report.check(
        name,
        result.is_ok() && listed,
        format!(
            "{} in {} listed={listed}",
            result
                .as_ref()
                .map(|out| out.stdout.trim().to_string())
                .unwrap_or_else(|e| e.to_string()),
            millis(started.elapsed())
        ),
    );
    result.is_ok() && listed
}

impl Ctx {
    async fn new(config: &Config, key: &Path, report: Report) -> Self {
        let driver = Arc::new(DirectNetworkAdb::new(key.to_path_buf()));
        let data = tempfile::tempdir().expect("temporary data folder");
        let state = Arc::new(AppState::new(
            driver.clone() as Arc<dyn AdbDriver>,
            AppListBundle::default(),
            data.path().to_path_buf(),
        ));
        Self {
            report,
            observer: Observer {
                adb: config.platform_adb.clone(),
                serial: config.observe.clone(),
            },
            driver,
            state,
            jar: jar(),
            data,
        }
    }

    async fn start(&self, serial: &str) -> Result<(u64, Duration), String> {
        let generation = self.state.begin_remote_generation(serial).await?;
        let started = Instant::now();
        self.state
            .ensure_remote_session_for_generation(
                self.state.adb_snapshot().await,
                &self.jar,
                serial,
                Some(generation),
            )
            .await?;
        Ok((generation, started.elapsed()))
    }

    /// What one `send_key` command does on the channel path: ensure, then
    /// write; a failed write closes the generation and is never resent.
    async fn command_press(
        &self,
        serial: &str,
        generation: u64,
        code: u32,
    ) -> Result<Duration, String> {
        let started = Instant::now();
        self.state
            .ensure_remote_session_for_generation(
                self.state.adb_snapshot().await,
                &self.jar,
                serial,
                Some(generation),
            )
            .await
            .map_err(|e| format!("before input: {e}"))?;
        if let Err(error) = self
            .state
            .remote_send_key_press_for_generation(serial, code, Some(generation))
            .await
        {
            let close = self.state.close_remote_generation(serial, generation).await;
            return Err(format!("after input: {error} (close: {close:?})"));
        }
        Ok(started.elapsed())
    }

    async fn press(&self, serial: &str, generation: u64, code: u32) -> Result<Duration, String> {
        let started = Instant::now();
        self.state
            .remote_send_key_press_for_generation(serial, code, Some(generation))
            .await?;
        Ok(started.elapsed())
    }

    async fn close(&self, serial: &str, generation: u64) -> (Result<(), String>, Duration) {
        let started = Instant::now();
        let result = self.state.close_remote_generation(serial, generation).await;
        (result, started.elapsed())
    }

    fn check_clean(&self, name: &str, timeout: Duration) -> bool {
        match self.observer.wait_clean(timeout) {
            Ok(after) => {
                self.report.check(
                    name,
                    true,
                    format!("no helper process or jar after {}", millis(after)),
                );
                true
            }
            Err(left) => {
                self.report
                    .check(name, false, format!("left behind {left:?}"));
                false
            }
        }
    }

    fn go_home(&self) {
        self.observer.shell("input keyevent 3");
        self.observer
            .wait_focus("tvlauncher", true, Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(300));
    }
}

async fn scenario_start_and_latency(ctx: &Ctx, serial: &str) -> Option<u64> {
    let report = &ctx.report;
    ctx.observer.wait_no_prompt(report);
    ctx.go_home();
    let (generation, startup) = match ctx.start(serial).await {
        Ok(started) => started,
        Err(error) => {
            report.check("start", false, error);
            return None;
        }
    };
    let helpers = ctx.observer.helpers();
    report.check(
        "start",
        !helpers.processes.is_empty(),
        format!(
            "cold start {} processes={} jars={}",
            millis(startup),
            helpers.processes.len(),
            helpers.jars.len()
        ),
    );

    // Every press types one letter into a focused field, so the field shows
    // whether each press landed exactly once and in order.
    for (name, through_command) in [("latency.send", false), ("latency.command", true)] {
        let opened = ctx.observer.open_field();
        let mut samples = Vec::new();
        let mut failures = Vec::new();
        let mut expected = String::new();
        for i in 0..30 {
            let (code, letter) = if i % 2 == 0 {
                (KEY_A, 'a')
            } else {
                (KEY_B, 'b')
            };
            let sent = if through_command {
                ctx.command_press(serial, generation, code).await
            } else {
                ctx.press(serial, generation, code).await
            };
            match sent {
                Ok(elapsed) => {
                    samples.push(elapsed);
                    expected.push(letter);
                }
                Err(error) => failures.push(error),
            }
            tokio::time::sleep(Duration::from_millis(60)).await;
        }
        std::thread::sleep(Duration::from_millis(500));
        let landed = ctx.observer.field();
        report.check(
            name,
            opened && failures.is_empty() && landed.as_deref() == Some(expected.as_str()),
            format!(
                "{} per press{}; failures={failures:?}; field shows {landed:?}",
                stats(&samples),
                if through_command {
                    " (ensure+send, as one send_key command)"
                } else {
                    ""
                }
            ),
        );
    }
    ctx.go_home();
    Some(generation)
}

async fn scenario_effects(ctx: &Ctx, serial: &str, generation: u64) {
    let report = &ctx.report;
    let observer = &ctx.observer;

    observer.wait_no_prompt(report);
    observer.start_activity("android.settings.SETTINGS");
    let opened = observer.wait_focus(SETTINGS, true, Duration::from_secs(5));
    let sent = ctx.command_press(serial, generation, HOME).await;
    let effect = observer.wait_focus("tvlauncher", true, Duration::from_secs(5));
    report.check(
        "home",
        opened.is_some() && sent.is_ok() && effect.is_some(),
        format!(
            "settings opened={opened:?} send={sent:?} launcher focused after {:?}",
            effect.map(millis)
        ),
    );

    observer.wait_no_prompt(report);
    observer.start_activity("android.settings.SETTINGS");
    let opened = observer.wait_focus(SETTINGS, true, Duration::from_secs(5));
    let sent = ctx.command_press(serial, generation, BACK).await;
    let effect = observer.wait_focus(SETTINGS, false, Duration::from_secs(5));
    report.check(
        "back",
        opened.is_some() && sent.is_ok() && effect.is_some(),
        format!(
            "send={sent:?} settings left after {:?}; focus now {}",
            effect.map(millis),
            observer.focus()
        ),
    );

    observer.wait_no_prompt(report);
    observer.start_activity("android.settings.SETTINGS");
    observer.wait_focus(SETTINGS, true, Duration::from_secs(5));
    std::thread::sleep(Duration::from_millis(700));
    let before = observer.focused();
    let down = ctx.command_press(serial, generation, DOWN).await;
    std::thread::sleep(Duration::from_millis(400));
    let moved = observer.focused();
    let up = ctx.command_press(serial, generation, UP).await;
    std::thread::sleep(Duration::from_millis(400));
    let back = observer.focused();
    report.check(
        "dpad",
        down.is_ok()
            && up.is_ok()
            && before.is_some()
            && moved.as_ref().map(|n| &n.bounds) != before.as_ref().map(|n| &n.bounds)
            && back.as_ref().map(|n| &n.bounds) == before.as_ref().map(|n| &n.bounds),
        format!(
            "focus {:?} -> DOWN -> {:?} -> UP -> {:?}",
            before.map(|n| n.bounds),
            moved.map(|n| n.bounds),
            back.map(|n| n.bounds)
        ),
    );

    // Held key: Back acts on release, so Settings stays while it is held and
    // leaves once the up arrives.
    observer.wait_no_prompt(report);
    observer.start_activity("android.settings.SETTINGS");
    let opened = observer.wait_focus(SETTINGS, true, Duration::from_secs(5));
    let down = ctx
        .state
        .remote_send_key_for_generation(serial, BACK, true, Some(generation))
        .await;
    std::thread::sleep(Duration::from_millis(1500));
    let held = observer.focus().contains(SETTINGS);
    let up = ctx
        .state
        .remote_send_key_for_generation(serial, BACK, false, Some(generation))
        .await;
    let released = observer.wait_focus(SETTINGS, false, Duration::from_secs(5));
    report.check(
        "held.release",
        opened.is_some() && down.is_ok() && held && up.is_ok() && released.is_some(),
        format!(
            "Back down={down:?}; Settings still open after 1.5 s held={held}; up={up:?}; Settings left {:?} after release",
            released.map(millis)
        ),
    );
    ctx.go_home();
}

async fn scenario_text(ctx: &Ctx, serial: &str, generation: u64) {
    let report = &ctx.report;
    let observer = &ctx.observer;
    for (name, text, required) in [
        ("text.ascii", "atv remote 42!", true),
        ("text.latin", "café über", true),
        ("text.cjk", "漢字", false),
        ("text.emoji", "😀", false),
    ] {
        observer.wait_no_prompt(report);
        observer.start_activity(TEXT_FIELD);
        observer.wait_focus("AddWifiNetworkActivity", true, Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(700));
        let before = observer.focused();
        let started = Instant::now();
        let sent = ctx
            .state
            .remote_send_text_for_generation(serial, text, Some(generation))
            .await;
        let elapsed = started.elapsed();
        std::thread::sleep(Duration::from_millis(700));
        let after = observer.focused();
        let landed = after.as_ref().map(|n| n.text.clone()).unwrap_or_default();
        let exact = landed == text;
        let detail = format!(
            "field {:?} before={:?} send={sent:?} in {} landed={landed:?}",
            after.as_ref().map(|n| n.class.clone()),
            before.map(|n| n.text),
            millis(elapsed)
        );
        if required {
            report.check(name, sent.is_ok() && exact, detail);
        } else {
            report.info(
                name,
                format!(
                    "{detail} ({})",
                    if exact {
                        "landed"
                    } else {
                        "not representable by the TV keymap"
                    }
                ),
            );
        }
    }
    ctx.go_home();
}

async fn scenario_concurrent(ctx: &Ctx, serial: &str, generation: u64) {
    let report = &ctx.report;
    let local = ctx.data.path().join("pulled-hosts");
    let shells = {
        let driver = ctx.driver.clone();
        let serial = serial.to_string();
        tokio::spawn(async move {
            let mut errors = Vec::new();
            for _ in 0..10 {
                match driver
                    .shell(
                        &serial,
                        "ls /sdcard >/dev/null && getprop ro.build.version.release",
                    )
                    .await
                {
                    Ok(out) if !out.stdout.trim().is_empty() => {}
                    other => errors.push(format!("{other:?}")),
                }
            }
            errors
        })
    };
    let files = {
        let driver = ctx.driver.clone();
        let serial = serial.to_string();
        let local = local.clone();
        tokio::spawn(async move {
            let mut errors = Vec::new();
            for _ in 0..5 {
                if let Err(error) = driver.shell(&serial, "ls -la /data/local/tmp").await {
                    errors.push(error.to_string());
                }
                let target = local.to_string_lossy().into_owned();
                if let Err(error) = driver
                    .raw_transfer(&["-s", &serial, "pull", "/system/etc/hosts", &target])
                    .await
                {
                    errors.push(error.to_string());
                }
            }
            errors
        })
    };
    let mut presses = Vec::new();
    let mut failures = Vec::new();
    for i in 0..20 {
        match ctx
            .command_press(serial, generation, if i % 2 == 0 { RIGHT } else { LEFT })
            .await
        {
            Ok(elapsed) => presses.push(elapsed),
            Err(error) => failures.push(error),
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let shell_errors = shells.await.unwrap();
    let file_errors = files.await.unwrap();
    let pulled = std::fs::metadata(&local).map(|m| m.len()).unwrap_or(0);
    report.check(
        "concurrent",
        failures.is_empty() && shell_errors.is_empty() && file_errors.is_empty() && pulled > 0,
        format!(
            "presses {} failures={failures:?}; shell errors={shell_errors:?}; file errors={file_errors:?}; pulled {pulled} bytes",
            stats(&presses)
        ),
    );
}

async fn scenario_close(ctx: &Ctx, serial: &str, generation: u64) {
    let (result, elapsed) = ctx.close(serial, generation).await;
    ctx.report.check(
        "close",
        result.is_ok(),
        format!("{result:?} in {}", millis(elapsed)),
    );
    ctx.check_clean("close.cleanup", Duration::from_secs(5));
    let stale = ctx.press(serial, generation, LEFT).await;
    ctx.report.check(
        "close.stale_generation",
        stale.is_err(),
        format!("press after close: {stale:?}"),
    );
}

async fn scenario_reopen(ctx: &Ctx, serial: &str, rounds: usize) {
    let report = &ctx.report;
    let mut starts = Vec::new();
    let mut presses = Vec::new();
    let mut closes = Vec::new();
    let mut errors = Vec::new();
    for round in 0..rounds {
        match ctx.start(serial).await {
            Ok((generation, startup)) => {
                starts.push(startup);
                match ctx.command_press(serial, generation, LEFT).await {
                    Ok(elapsed) => presses.push(elapsed),
                    Err(error) => errors.push(format!("round {round} press: {error}")),
                }
                let (result, elapsed) = ctx.close(serial, generation).await;
                closes.push(elapsed);
                if let Err(error) = result {
                    errors.push(format!("round {round} close: {error}"));
                }
                if let Err(left) = ctx.observer.wait_clean(Duration::from_secs(5)) {
                    errors.push(format!("round {round} left {left:?}"));
                }
            }
            Err(error) => errors.push(format!("round {round} start: {error}")),
        }
    }
    report.check(
        "reopen",
        errors.is_empty() && starts.len() == rounds,
        format!(
            "{rounds} rounds; start {}; first press {}; close {}; errors={errors:?}",
            stats(&starts),
            stats(&presses),
            stats(&closes)
        ),
    );
}

async fn scenario_back_to_back(ctx: &Ctx, serial: &str) {
    let report = &ctx.report;
    let first = ctx.start(serial).await;
    let Ok((first, _)) = first else {
        report.check("back_to_back", false, format!("first start {first:?}"));
        return;
    };
    let first_press = ctx.command_press(serial, first, LEFT).await;
    let second = ctx.start(serial).await;
    let helpers = ctx.observer.helpers();
    let stale = ctx.press(serial, first, RIGHT).await;
    let ok = match &second {
        Ok((second, _)) => {
            let press = ctx.command_press(serial, *second, RIGHT).await;
            let (close, _) = ctx.close(serial, *second).await;
            report.info(
                "back_to_back.second",
                format!("press={press:?} close={close:?}"),
            );
            press.is_ok() && close.is_ok()
        }
        Err(_) => false,
    };
    report.check(
        "back_to_back",
        first_press.is_ok() && ok && stale.is_err() && helpers.processes.len() <= 2,
        format!(
            "first press={first_press:?} second start={:?} old generation press={stale:?} helpers while second live={helpers:?}",
            second.as_ref().map(|(_, t)| millis(*t))
        ),
    );
    ctx.check_clean("back_to_back.cleanup", Duration::from_secs(5));
}

/// Closing while a key is held must release it first.
async fn scenario_held_close(ctx: &Ctx, serial: &str) {
    let report = &ctx.report;
    let observer = &ctx.observer;
    let Ok((generation, _)) = ctx.start(serial).await else {
        report.check("held.close", false, "start failed");
        return;
    };
    observer.wait_no_prompt(report);
    observer.start_activity("android.settings.SETTINGS");
    let opened = observer.wait_focus(SETTINGS, true, Duration::from_secs(5));
    let down = ctx
        .state
        .remote_send_key_for_generation(serial, BACK, true, Some(generation))
        .await;
    std::thread::sleep(Duration::from_millis(800));
    let held = observer.focus().contains(SETTINGS);
    let (closed, took) = ctx.close(serial, generation).await;
    let released = observer.wait_focus(SETTINGS, false, Duration::from_secs(5));
    report.check(
        "held.close",
        opened.is_some() && down.is_ok() && held && closed.is_ok() && released.is_some(),
        format!(
            "Back held={held}; close={closed:?} in {}; Settings left {:?} after close (close sent the release)",
            millis(took),
            released.map(millis)
        ),
    );
    ctx.check_clean("held.close.cleanup", Duration::from_secs(5));
    ctx.go_home();
}

async fn scenario_cancel_startup(ctx: &Ctx, serial: &str) {
    let report = &ctx.report;
    for (delay, abort) in [
        (0, false),
        (10, false),
        (30, false),
        (60, false),
        (100, false),
        (160, false),
        (250, false),
        (400, false),
        (700, false),
        (30, true),
        (120, true),
        (300, true),
    ] {
        let name = format!(
            "cancel_startup.{}{delay}ms",
            if abort { "drop." } else { "close." }
        );
        let generation = match ctx.state.begin_remote_generation(serial).await {
            Ok(generation) => generation,
            Err(error) => {
                report.check(&name, false, error);
                continue;
            }
        };
        let task = {
            let state = ctx.state.clone();
            let jar = ctx.jar.clone();
            let serial = serial.to_string();
            tokio::spawn(async move {
                let started = Instant::now();
                let result = state
                    .ensure_remote_session_for_generation(
                        state.adb_snapshot().await,
                        &jar,
                        &serial,
                        Some(generation),
                    )
                    .await;
                (result, started.elapsed())
            })
        };
        tokio::time::sleep(Duration::from_millis(delay)).await;
        let cancel_started = Instant::now();
        let (outcome, closed) = if abort {
            task.abort();
            let outcome = tokio::time::timeout(Duration::from_secs(20), task).await;
            // The UI closes the generation after dropping the request.
            let closed = ctx.state.close_remote_generation(serial, generation).await;
            (
                match outcome {
                    Ok(Err(error)) if error.is_cancelled() => "request dropped".to_string(),
                    other => format!("{other:?}"),
                },
                closed,
            )
        } else {
            let closed = ctx.state.close_remote_generation(serial, generation).await;
            let outcome = tokio::time::timeout(Duration::from_secs(20), task).await;
            (
                match outcome {
                    Ok(Ok((result, took))) => format!("{result:?} after {}", millis(took)),
                    other => format!("{other:?}"),
                },
                closed,
            )
        };
        let returned = cancel_started.elapsed();
        let clean = ctx.observer.wait_clean(Duration::from_secs(8));
        report.check(
            &name,
            clean.is_ok() && returned < Duration::from_secs(10),
            format!(
                "startup={outcome} close={closed:?} returned in {} cleanup={:?}",
                millis(returned),
                clean.map(millis)
            ),
        );
    }
}

async fn scenario_helper_death(ctx: &Ctx, serial: &str) {
    let report = &ctx.report;
    let Ok((generation, _)) = ctx.start(serial).await else {
        report.check("helper_death", false, "start failed");
        return;
    };
    let first = ctx.command_press(serial, generation, LEFT).await;
    let opened = ctx.observer.open_field();
    let helpers = ctx.observer.helpers();
    for pid in &helpers.processes {
        ctx.observer.shell(&format!("kill {pid}"));
    }
    std::thread::sleep(Duration::from_millis(800));
    let after_death = ctx.command_press(serial, generation, KEY_A).await;
    std::thread::sleep(Duration::from_millis(500));
    let field = ctx.observer.field();
    let landed = field.as_deref().map_or(0, |text| text.matches('a').count());
    report.info(
        "helper_death",
        format!("field ready={opened}; killed own helper {helpers:?}; first press={first:?}; press after death={after_death:?}; field shows {field:?}"),
    );
    report.check(
        "helper_death.no_replay",
        landed <= 1,
        format!("the key landed {landed}x"),
    );
    report.check(
        "helper_death.recovers",
        after_death.is_ok() && landed == 1,
        "the next press renews the session before any input and lands once".to_string(),
    );
    ctx.go_home();
    let _ = ctx.close(serial, generation).await;
    ctx.check_clean("helper_death.cleanup", Duration::from_secs(5));
}

/// A local TCP relay that can stop forwarding without closing anything, as a
/// TV that vanished from the network would look.
struct Relay {
    addr: SocketAddr,
    frozen: Arc<AtomicBool>,
    severed: Arc<AtomicBool>,
    pairs: Arc<Mutex<Vec<(TcpStream, TcpStream)>>>,
}

fn pump(mut from: TcpStream, mut to: TcpStream, frozen: Arc<AtomicBool>, severed: Arc<AtomicBool>) {
    let _ = from.set_read_timeout(Some(Duration::from_millis(30)));
    let mut buffer = [0u8; 16 * 1024];
    loop {
        if severed.load(Ordering::Acquire) {
            return;
        }
        if frozen.load(Ordering::Acquire) {
            std::thread::sleep(Duration::from_millis(20));
            continue;
        }
        match from.read(&mut buffer) {
            Ok(0) => {
                let _ = to.shutdown(std::net::Shutdown::Write);
                return;
            }
            Ok(n) => {
                // Bytes read just as the relay froze are held, not forwarded:
                // a vanished TV never gets them, a returning one gets them late.
                while frozen.load(Ordering::Acquire) && !severed.load(Ordering::Acquire) {
                    std::thread::sleep(Duration::from_millis(20));
                }
                if severed.load(Ordering::Acquire) {
                    return;
                }
                if to.write_all(&buffer[..n]).is_err() {
                    return;
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(_) => {
                let _ = to.shutdown(std::net::Shutdown::Both);
                return;
            }
        }
    }
}

impl Relay {
    fn start(target: SocketAddr) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind relay");
        let addr = listener.local_addr().unwrap();
        let frozen = Arc::new(AtomicBool::new(false));
        let severed = Arc::new(AtomicBool::new(false));
        let pairs: Arc<Mutex<Vec<(TcpStream, TcpStream)>>> = Arc::default();
        {
            let frozen = frozen.clone();
            let severed = severed.clone();
            let pairs = pairs.clone();
            std::thread::spawn(move || {
                for client in listener.incoming() {
                    let Ok(client) = client else { return };
                    let Ok(upstream) = TcpStream::connect(target) else {
                        continue;
                    };
                    pairs
                        .lock()
                        .unwrap()
                        .push((client.try_clone().unwrap(), upstream.try_clone().unwrap()));
                    let (a, b) = (client.try_clone().unwrap(), upstream.try_clone().unwrap());
                    let (f, s) = (frozen.clone(), severed.clone());
                    std::thread::spawn(move || pump(a, b, f, s));
                    let (f, s) = (frozen.clone(), severed.clone());
                    std::thread::spawn(move || pump(upstream, client, f, s));
                }
            });
        }
        Self {
            addr,
            frozen,
            severed,
            pairs,
        }
    }
    fn freeze(&self) {
        self.frozen.store(true, Ordering::Release);
    }
    fn thaw(&self) {
        self.frozen.store(false, Ordering::Release);
    }
    /// Drop every relayed connection without forwarding anything buffered.
    fn sever(&self) {
        self.severed.store(true, Ordering::Release);
        for (client, upstream) in self.pairs.lock().unwrap().drain(..) {
            let _ = upstream.shutdown(std::net::Shutdown::Both);
            let _ = client.shutdown(std::net::Shutdown::Both);
        }
        std::thread::sleep(Duration::from_millis(100));
        self.severed.store(false, Ordering::Release);
        self.frozen.store(false, Ordering::Release);
    }
}

async fn scenario_tv_gone(ctx: &Ctx, addr: &str, heal: bool) {
    let report = &ctx.report;
    let name = if heal {
        "tv_gone.heal"
    } else {
        "tv_gone.vanish"
    };
    let target: SocketAddr = addr.parse().expect("numeric emulator address");
    let relay = Relay::start(target);
    let serial = relay.addr.to_string();
    if !connect(&ctx.driver, &serial, report, &format!("{name}.connect")).await {
        return;
    }
    let Ok((generation, startup)) = ctx.start(&serial).await else {
        report.check(name, false, "start through relay failed");
        return;
    };
    let first = ctx.command_press(&serial, generation, LEFT).await;
    let opened = ctx.observer.open_field();

    relay.freeze();
    let started = Instant::now();
    let pressed = tokio::time::timeout(
        Duration::from_secs(20),
        ctx.command_press(&serial, generation, KEY_A),
    )
    .await;
    let returned = started.elapsed();
    let during = ctx.observer.field();
    let reported = match &pressed {
        Ok(Err(message)) => message.clone(),
        other => format!("{other:?}"),
    };
    report.check(
        &format!("{name}.uncertain"),
        opened && reported.starts_with("after input") && reported.contains("unknown") && returned < Duration::from_secs(10),
        format!(
            "start {} first press={first:?}; press while gone returned in {}: {reported}; field while gone {during:?}",
            millis(startup),
            millis(returned)
        ),
    );
    // A new Remote right away must fail before input, within bounds.
    let retry_started = Instant::now();
    let retry = tokio::time::timeout(Duration::from_secs(25), ctx.start(&serial)).await;
    report.info(
        &format!("{name}.restart_while_gone"),
        format!(
            "{:?} after {}",
            retry
                .as_ref()
                .map(|r| r.as_ref().map(|(g, t)| (*g, millis(*t)))),
            millis(retry_started.elapsed())
        ),
    );
    if let Ok(Ok((generation, _))) = retry {
        let _ = ctx.close(&serial, generation).await;
    }

    if heal {
        relay.thaw();
    } else {
        relay.sever();
    }
    std::thread::sleep(Duration::from_millis(1500));
    let field = ctx.observer.field();
    let landed = field.as_deref().map_or(0, |text| text.matches('a').count());
    report.check(
        &format!("{name}.no_replay"),
        landed <= 1,
        format!(
            "after the TV {} the field shows {field:?}: the key landed {landed}x (late delivery of the one uncertain write is possible, a second copy is not)",
            if heal { "came back" } else { "dropped every connection" }
        ),
    );
    match ctx.observer.wait_clean(Duration::from_secs(15)) {
        Ok(after) => report.check(
            &format!("{name}.device_cleanup"),
            true,
            format!(
                "helper gone on the device {} after the connection ended",
                millis(after)
            ),
        ),
        Err(left) => report.check(
            &format!("{name}.device_cleanup"),
            false,
            format!("left behind {left:?}"),
        ),
    }
    relay.sever();
    let _ = ctx.driver.raw(&["disconnect", &serial]).await;
    ctx.go_home();
}

async fn scenario_two_serials(ctx: &Ctx, addr: &str) {
    let report = &ctx.report;
    let target: SocketAddr = addr.parse().expect("numeric emulator address");
    let relay = Relay::start(target);
    let other = relay.addr.to_string();
    if !connect(&ctx.driver, &other, report, "two_serials.connect").await {
        return;
    }
    let a = ctx.start(addr).await;
    let b = ctx.start(&other).await;
    let (Ok((ga, _)), Ok((gb, _))) = (&a, &b) else {
        report.check("two_serials", false, format!("a={a:?} b={b:?}"));
        relay.sever();
        return;
    };
    let both = ctx.observer.helpers();
    let pa = ctx.command_press(addr, *ga, LEFT).await;
    let pb = ctx.command_press(&other, *gb, RIGHT).await;
    let (close_a, _) = ctx.close(addr, *ga).await;
    std::thread::sleep(Duration::from_millis(500));
    let after_a = ctx.observer.helpers();
    let pb2 = ctx.command_press(&other, *gb, LEFT).await;
    let (close_b, _) = ctx.close(&other, *gb).await;
    report.check(
        "two_serials",
        pa.is_ok()
            && pb.is_ok()
            && pb2.is_ok()
            && close_a.is_ok()
            && close_b.is_ok()
            && both.jars.iter().collect::<std::collections::BTreeSet<_>>().len() == both.jars.len()
            && !after_a.processes.is_empty(),
        format!(
            "both live {both:?}; presses a={pa:?} b={pb:?}; after closing a {after_a:?}; b still works={pb2:?}; closes {close_a:?}/{close_b:?}"
        ),
    );
    ctx.check_clean("two_serials.cleanup", Duration::from_secs(5));
    relay.sever();
    let _ = ctx.driver.raw(&["disconnect", &other]).await;
}

async fn scenario_shell_latency(ctx: &Ctx, serial: &str) {
    let mut samples = Vec::new();
    let mut errors = Vec::new();
    for i in 0..20 {
        let started = Instant::now();
        match ctx
            .driver
            .shell(
                serial,
                &format!("input keyevent {}", if i % 2 == 0 { RIGHT } else { LEFT }),
            )
            .await
        {
            Ok(_) => samples.push(started.elapsed()),
            Err(error) => errors.push(error.to_string()),
        }
    }
    ctx.report.check(
        "latency.shell_fallback",
        errors.is_empty(),
        format!(
            "input keyevent per press {} errors={errors:?}",
            stats(&samples)
        ),
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs a real adbd; see the module docs"]
async fn native_fast_remote_against_real_adbd() {
    let Some(config) = config_or_skip() else {
        return;
    };
    let report = Report::new(config.log.as_deref());
    report.info("run", format!("plain endpoint, jar {}", jar().display()));
    let ctx = Ctx::new(&config, &config.key, report).await;
    let serial = config.addr.as_str();
    let baseline = ctx.observer.helpers();
    ctx.report.check(
        "baseline",
        baseline.is_clean(),
        format!("helpers before the run {baseline:?}"),
    );
    if !connect(&ctx.driver, serial, &ctx.report, "connect").await {
        ctx.report.finish();
        return;
    }
    if let Some(generation) = scenario_start_and_latency(&ctx, serial).await {
        scenario_effects(&ctx, serial, generation).await;
        scenario_text(&ctx, serial, generation).await;
        scenario_concurrent(&ctx, serial, generation).await;
        scenario_close(&ctx, serial, generation).await;
    }
    scenario_shell_latency(&ctx, serial).await;
    scenario_reopen(&ctx, serial, 12).await;
    scenario_back_to_back(&ctx, serial).await;
    scenario_held_close(&ctx, serial).await;
    scenario_cancel_startup(&ctx, serial).await;
    // Cancelling startup must leave the TV's own connection usable.
    let shell = ctx.driver.shell(serial, "echo still-connected").await;
    ctx.report.check(
        "cancel_startup.connection_kept",
        shell
            .as_ref()
            .is_ok_and(|out| out.stdout.contains("still-connected")),
        format!("{:?}", shell.map(|out| out.stdout.trim().to_string())),
    );
    scenario_helper_death(&ctx, serial).await;
    scenario_two_serials(&ctx, serial).await;
    scenario_tv_gone(&ctx, serial, false).await;
    scenario_tv_gone(&ctx, serial, true).await;
    // The driver must still serve the same TV after all of that.
    let after = ctx.start(serial).await;
    if let Ok((generation, _)) = &after {
        let press = ctx.command_press(serial, *generation, LEFT).await;
        let _ = ctx.close(serial, *generation).await;
        ctx.report
            .check("final_session", press.is_ok(), format!("{press:?}"));
    } else {
        ctx.report
            .check("final_session", false, format!("{after:?}"));
    }
    ctx.check_clean("final_cleanup", Duration::from_secs(5));
    ctx.go_home();
    ctx.report.finish();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs a paired Wireless debugging endpoint; see the module docs"]
async fn native_fast_remote_over_tls() {
    let Some(config) = config_or_skip() else {
        return;
    };
    let (Some(addr), Some(key)) = (var("ATVOPT_EMU_TLS_ADDR"), var("ATVOPT_EMU_TLS_KEY")) else {
        eprintln!("skipped: set ATVOPT_EMU_TLS_ADDR and ATVOPT_EMU_TLS_KEY");
        return;
    };
    let report = Report::new(config.log.as_deref());
    report.info("run", "TLS endpoint");
    let ctx = Ctx::new(&config, Path::new(&key), report).await;
    if !connect(&ctx.driver, &addr, &ctx.report, "tls.connect").await {
        ctx.report.finish();
        return;
    }
    if let Some(generation) = scenario_start_and_latency(&ctx, &addr).await {
        scenario_text(&ctx, &addr, generation).await;
        scenario_close(&ctx, &addr, generation).await;
    }
    scenario_shell_latency(&ctx, &addr).await;
    scenario_reopen(&ctx, &addr, 10).await;
    ctx.check_clean("tls.final_cleanup", Duration::from_secs(5));
    ctx.report.finish();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "takes over five minutes against a real adbd; see the module docs"]
async fn native_fast_remote_renews_before_stream_lifetime() {
    let Some(config) = config_or_skip() else {
        return;
    };
    if var("ATVOPT_EMU_LONG").is_none() {
        eprintln!("skipped: set ATVOPT_EMU_LONG=1");
        return;
    }
    let report = Report::new(config.log.as_deref());
    report.info("run", "stream lifetime");
    let ctx = Ctx::new(&config, &config.key, report).await;
    let serial = config.addr.as_str();
    if !connect(&ctx.driver, serial, &ctx.report, "lifetime.connect").await {
        ctx.report.finish();
        return;
    }
    let Ok((generation, _)) = ctx.start(serial).await else {
        ctx.report.check("lifetime.start", false, "start failed");
        ctx.report.finish();
        return;
    };
    let first = ctx.command_press(serial, generation, LEFT).await;
    ctx.report
        .info("lifetime.first", format!("{first:?}; idling 310 s"));
    tokio::time::sleep(Duration::from_secs(310)).await;
    let opened = ctx.observer.open_field();
    let later = ctx.command_press(serial, generation, KEY_A).await;
    std::thread::sleep(Duration::from_millis(500));
    let field = ctx.observer.field();
    ctx.report.check(
        "lifetime.renewed",
        opened && later.is_ok() && field.as_deref() == Some("a"),
        format!("press after the stream lifetime: {later:?}; field shows {field:?}"),
    );
    ctx.go_home();
    let _ = ctx.close(serial, generation).await;
    ctx.check_clean("lifetime.cleanup", Duration::from_secs(5));
    ctx.report.finish();
}

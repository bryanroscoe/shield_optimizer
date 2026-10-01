//! Dev-only E2E server: the app's real command layer against a simulated
//! device, served over local HTTP for the Playwright harness in `v2/e2e/`.
//!
//! Commands are dispatched through `shield_optimizer_v2_lib::invoke_handler`
//! — the same handler table the shipped app registers — on a Tauri
//! `MockRuntime` app, so argument names, deserialization, state and every
//! command body run exactly as in the desktop app. Only the adb driver is
//! swapped for `SimulatedAdb`.
//!
//! Built only with `--features e2e`. See `v2/e2e/README.md`.
//!
//! ```text
//! e2e_server [--port N] [--fixtures DIR]
//! e2e_server profile-from-session <session.jsonl> <out-dir> [--name NAME] [--serial KEY]
//! ```

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde::Deserialize;
use serde_json::{json, Value};
use tauri::test::{get_ipc_response, mock_builder, mock_context, noop_assets, INVOKE_KEY};
use tauri::webview::InvokeRequest;
use tauri::WebviewWindowBuilder;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use shield_optimizer_core::adb::sim::{
    self, replay, Device, FaultRule, HomePolicy, SimulatedAdb, TransportState, Wireless, World,
};
use shield_optimizer_core::commands::{loader, AppState};
use shield_optimizer_core::license::Entitlement;
use shield_optimizer_v2_lib::commands::diagnostics;

/// Commands answered here rather than dispatched: they reach the internet
/// or the host desktop, which a test must not do.
fn intercepted(cmd: &str) -> Option<Value> {
    Some(match cmd {
        "check_for_update" => json!({
            "current": env!("CARGO_PKG_VERSION"),
            "latest": null,
            "update_available": false,
            "url": "https://github.com/bryanroscoe/shield_optimizer/releases",
            "current_notes": null,
        }),
        "install_adb" => {
            json!({"ok": true, "path": "/simulated/platform-tools/adb", "message": "Simulated install."})
        }
        "open_log_dir" => Value::Null,
        _ => return None,
    })
}

struct Server {
    sim: SimulatedAdb,
    webview: tauri::WebviewWindow<tauri::test::MockRuntime>,
    fixtures: PathBuf,
    data_dir: PathBuf,
    invokes: Mutex<Vec<Value>>,
}

#[derive(Deserialize)]
struct TransportSpec {
    key: String,
    #[serde(default = "device_state")]
    state: TransportState,
}

fn device_state() -> TransportState {
    TransportState::Device
}

/// One device in a scenario: a captured profile plus overrides.
#[derive(Deserialize, Default)]
struct DeviceSpec {
    profile: String,
    /// Replace `ro.serialno` (to put two copies of one profile on the LAN).
    #[serde(default)]
    serial: Option<String>,
    #[serde(default)]
    props: BTreeMap<String, String>,
    /// `true`/`false`/`null`; absent keeps the profile's.
    #[serde(default, deserialize_with = "some_option")]
    leanback: Option<Option<bool>>,
    #[serde(default)]
    authorized: Option<bool>,
    #[serde(default)]
    transports: Option<Vec<TransportSpec>>,
    #[serde(default)]
    ip: Option<String>,
    #[serde(default, deserialize_with = "some_option")]
    legacy_port: Option<Option<u16>>,
    #[serde(default)]
    wireless: Option<Wireless>,
    #[serde(default)]
    home_policy: Option<HomePolicy>,
    #[serde(default)]
    auto_attach: Option<bool>,
    #[serde(default)]
    reattach_after_polls: Option<u32>,
    #[serde(default)]
    paired: bool,
    /// Shell commands run on the device first, unlogged and fault-free.
    #[serde(default)]
    setup: Vec<String>,
}

fn some_option<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<Option<T>>, D::Error> {
    Ok(Some(Option::deserialize(d)?))
}

#[derive(Deserialize, Default)]
struct Scenario {
    #[serde(default)]
    devices: Vec<DeviceSpec>,
    #[serde(default)]
    faults: Vec<FaultRule>,
}

impl Server {
    fn build_device(&self, spec: &DeviceSpec) -> Result<(Device, Vec<String>), String> {
        let dir = self.fixtures.join(&spec.profile);
        let mut d = sim::load_profile(&dir)?;
        let mut keys = sim::captured_transports(&dir);
        if let Some(serial) = &spec.serial {
            let old = d.serial.clone();
            d.serial = serial.clone();
            d.props.insert("ro.serialno".into(), serial.clone());
            keys = keys.into_iter().map(|k| k.replace(&old, serial)).collect();
            if let Some(w) = d.network.as_mut().and_then(|n| n.wireless.as_mut()) {
                w.connect_instance = w.connect_instance.replace(&old, serial);
            }
        }
        for (k, v) in &spec.props {
            d.props.insert(k.clone(), v.clone());
        }
        if let Some(l) = spec.leanback {
            d.leanback = l;
        }
        if let Some(a) = spec.authorized {
            d.authorized = a;
        }
        if let Some(ip) = &spec.ip {
            let net = d.network.get_or_insert_with(Default::default);
            keys = keys.into_iter().map(|k| k.replace(&net.ip, ip)).collect();
            net.ip = ip.clone();
        }
        if let Some(port) = spec.legacy_port {
            let net = d.network.get_or_insert_with(Default::default);
            net.legacy_port = port;
            net.advertise_legacy = port.is_some();
        }
        if let Some(w) = &spec.wireless {
            d.network.get_or_insert_with(Default::default).wireless = Some(w.clone());
        }
        if let Some(p) = spec.home_policy {
            d.home.policy = p;
        }
        if let Some(a) = spec.auto_attach {
            d.auto_attach = a;
        }
        d.reattach_after_polls = spec.reattach_after_polls.or(d.reattach_after_polls);
        if let Some(t) = &spec.transports {
            keys = t.iter().map(|t| t.key.clone()).collect();
        }
        Ok((d, keys))
    }

    fn reset(&self, scenario: Scenario) -> Result<Value, String> {
        let mut world = World::new(sim::stock_launchers());
        for spec in &scenario.devices {
            let (d, keys) = self.build_device(spec)?;
            let serial = d.serial.clone();
            world.add_device(d);
            if spec.paired {
                world.paired.insert(serial.clone());
            }
            let states: Vec<TransportState> = match &spec.transports {
                Some(t) => t.iter().map(|t| t.state).collect(),
                None => vec![TransportState::Device; keys.len()],
            };
            for (key, state) in keys.iter().zip(states) {
                world.attach(key, &serial, state);
            }
            for cmd in &spec.setup {
                world
                    .setup_shell(&serial, cmd)
                    .map_err(|e| format!("setup `{cmd}` on {serial}: {e}"))?;
            }
        }
        world.faults = scenario.faults;
        // Snapshots, the Home-handler tracker and anything else the app keeps
        // on disk start empty too, so a scenario sees the same state alone
        // or in the full suite. Logs are kept for the run.
        for entry in std::fs::read_dir(&self.data_dir)
            .into_iter()
            .flatten()
            .flatten()
        {
            if entry.file_name() == "logs" {
                continue;
            }
            let path = entry.path();
            let _ = if path.is_dir() {
                std::fs::remove_dir_all(&path)
            } else {
                std::fs::remove_file(&path)
            };
        }
        *self.sim.world() = world;
        self.invokes.lock().unwrap().clear();
        Ok(self.state())
    }

    fn state(&self) -> Value {
        let w = self.sim.world();
        let stock = sim::stock_launchers();
        let devices: Vec<Value> = w
            .devices
            .values()
            .map(|d| {
                let mut probe = d.clone();
                probe.home.transient_polls = 0;
                let pick =
                    |f: &dyn Fn(&shield_optimizer_core::adb::sim::Package) -> bool| -> Vec<String> {
                        d.packages
                            .iter()
                            .filter(|(_, p)| f(p))
                            .map(|(n, _)| n.clone())
                            .collect()
                    };
                json!({
                    "serial": d.serial,
                    "model": d.prop("ro.product.model"),
                    "authorized": d.authorized,
                    "leanback": d.leanback,
                    "packages": {
                        "enabled": pick(&|p| p.installed && p.enabled),
                        "disabled": pick(&|p| p.installed && !p.enabled),
                        "uninstalled": pick(&|p| !p.installed),
                    },
                    "settings": d.settings,
                    "home": {
                        "resolved": probe.resolve_home(&stock),
                        "preferred": d.home.preferred,
                        "role_holder": d.home.role_holder,
                        "policy": d.home.policy,
                        "handlers": d.home_handlers().iter().map(|c| c.short()).collect::<Vec<_>>(),
                    },
                    "wm": {"size": d.wm_size, "density": d.wm_density},
                    "input_log": d.input_log,
                    "started": d.started,
                    "network": d.network,
                })
            })
            .collect();
        let replay = w.replay.as_ref().map(
            |r| json!({"divergences": r.report(), "repeats": r.repeats, "reordered": r.reordered}),
        );
        let out = json!({
            "devices": devices,
            "transports": w.transports,
            "paired": w.paired,
            "mdns": w.mdns_services(),
            "faults": w.faults,
            "gaps": w.gaps,
            "log_len": w.log.len(),
            "replay": replay,
        });
        out
    }

    async fn invoke(self: &Arc<Self>, cmd: String, args: Value) -> Value {
        if let Some(v) = intercepted(&cmd) {
            self.invokes
                .lock()
                .unwrap()
                .push(json!({"cmd": cmd, "args": args, "ok": true, "intercepted": true}));
            return json!({"ok": true, "value": v});
        }
        let webview = self.webview.clone();
        let request = InvokeRequest {
            cmd: cmd.clone(),
            callback: tauri::ipc::CallbackFn(0),
            error: tauri::ipc::CallbackFn(1),
            url: "tauri://localhost".parse().unwrap(),
            body: tauri::ipc::InvokeBody::Json(args.clone()),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        };
        let response = tokio::task::spawn_blocking(move || get_ipc_response(&webview, request))
            .await
            .map_err(|e| json!(format!("dispatch panicked: {e}")));
        let result = match response {
            Ok(Ok(body)) => {
                json!({"ok": true, "value": body.deserialize::<Value>().unwrap_or(Value::Null)})
            }
            Ok(Err(error)) | Err(error) => json!({"ok": false, "error": error}),
        };
        self.invokes.lock().unwrap().push(json!({
            "cmd": cmd,
            "args": args,
            "ok": result["ok"],
            "error": result.get("error"),
        }));
        result
    }

    async fn route(
        self: &Arc<Self>,
        method: &str,
        path: &str,
        body: Value,
    ) -> Result<Value, String> {
        let (path, query) = path.split_once('?').unwrap_or((path, ""));
        let field = |k: &str| body.get(k).and_then(Value::as_str).map(str::to_string);
        match (method, path) {
            ("GET", "/health") => Ok(json!("ok")),
            ("POST", "/invoke") => {
                let cmd = field("cmd").ok_or("missing cmd")?;
                Ok(self
                    .invoke(cmd, body.get("args").cloned().unwrap_or(json!({})))
                    .await)
            }
            ("POST", "/control/reset") => {
                let scenario: Scenario = serde_json::from_value(body).map_err(|e| e.to_string())?;
                self.reset(scenario)
            }
            ("GET", "/control/state") => Ok(self.state()),
            ("GET", "/control/profiles") => Ok(json!(sim::list_profiles(&self.fixtures))),
            ("POST", "/control/fault") => {
                let rule: FaultRule = serde_json::from_value(body).map_err(|e| e.to_string())?;
                self.sim.world().faults.push(rule);
                Ok(json!(true))
            }
            ("POST", "/control/faults/clear") => {
                self.sim.world().faults.clear();
                Ok(json!(true))
            }
            ("POST", "/control/shell") => {
                let serial = field("serial").ok_or("missing serial")?;
                let command = field("command").ok_or("missing command")?;
                self.sim
                    .world()
                    .setup_shell(&serial, &command)
                    .map(Value::from)
            }
            ("POST", "/control/attach") => {
                let key = field("key").ok_or("missing key")?;
                let serial = field("serial").ok_or("missing serial")?;
                let state: TransportState = body
                    .get("state")
                    .cloned()
                    .map(serde_json::from_value)
                    .transpose()
                    .map_err(|e| e.to_string())?
                    .unwrap_or(TransportState::Device);
                self.sim.world().attach(&key, &serial, state);
                Ok(json!(true))
            }
            ("POST", "/control/detach") => {
                let key = field("key").ok_or("missing key")?;
                Ok(json!(self.sim.world().detach(&key)))
            }
            ("POST", "/control/device") => {
                let serial = field("serial").ok_or("missing serial")?;
                let mut w = self.sim.world();
                let d = w.devices.get_mut(&serial).ok_or("no such device")?;
                if let Some(v) = body.get("authorized").and_then(Value::as_bool) {
                    d.authorized = v;
                }
                if let Some(v) = body.get("auto_attach").and_then(Value::as_bool) {
                    d.auto_attach = v;
                }
                if let Some(v) = body.get("home_policy") {
                    d.home.policy = serde_json::from_value(v.clone()).map_err(|e| e.to_string())?;
                }
                if let Some(t) = body.get("transient_home") {
                    d.home.transient_holder =
                        t.get("holder").and_then(Value::as_str).map(str::to_string);
                    d.home.transient_polls =
                        t.get("polls").and_then(Value::as_u64).unwrap_or(1) as u32;
                }
                if let Some(p) = body.get("pairing") {
                    let instance = p
                        .get("instance")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    let port = p.get("port").and_then(Value::as_u64).unwrap_or(37_123) as u16;
                    let pin = p
                        .get("pin")
                        .and_then(Value::as_str)
                        .unwrap_or("123456")
                        .to_string();
                    if let Some(w) = d.network.as_mut().and_then(|n| n.wireless.as_mut()) {
                        w.pairing = Some((instance, port, pin));
                    } else {
                        return Err("device has no Wireless debugging config".into());
                    }
                }
                Ok(json!(true))
            }
            ("GET", "/control/log") => {
                let since: u64 = query
                    .split('&')
                    .find_map(|kv| kv.strip_prefix("since="))
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0);
                let w = self.sim.world();
                Ok(json!(w
                    .log
                    .iter()
                    .filter(|i| i.seq > since)
                    .collect::<Vec<_>>()))
            }
            ("GET", "/control/invokes") => Ok(json!(*self.invokes.lock().unwrap())),
            ("POST", "/control/replay") => {
                let path = field("path").ok_or("missing path")?;
                let text = std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
                let lines = replay::parse_session(&text);
                let mut w = self.sim.world();
                if body.get("profile").and_then(Value::as_bool).unwrap_or(true) {
                    if let Some(d) = replay::device_from_session(&lines, None) {
                        let serial = d.serial.clone();
                        let keys: Vec<String> = lines
                            .iter()
                            .filter(|l| l.kind == "adb")
                            .filter_map(|l| l.serial.clone())
                            .collect::<std::collections::BTreeSet<_>>()
                            .into_iter()
                            .collect();
                        w.add_device(d);
                        for k in keys {
                            w.attach(&k, &serial, TransportState::Device);
                        }
                    }
                }
                w.replay = Some(replay::Replay::new(&lines));
                let ui: Vec<Value> = lines
                    .iter()
                    .filter(|l| l.kind == "ui")
                    .map(
                        |l| json!({"event": l.event, "label": l.label, "path": l.path, "ts": l.ts}),
                    )
                    .collect();
                Ok(json!({"adb_calls": lines.iter().filter(|l| l.kind == "adb").count(), "ui": ui}))
            }
            ("GET", "/control/replay") => {
                let w = self.sim.world();
                Ok(w.replay
                    .as_ref()
                    .map(|r| json!({"divergences": r.report(), "repeats": r.repeats, "reordered": r.reordered}))
                    .unwrap_or(Value::Null))
            }
            _ => Err(format!("no route {method} {path}")),
        }
    }
}

async fn serve_conn(server: Arc<Server>, mut stream: TcpStream) {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 16384];
    let header_end = loop {
        match stream.read(&mut chunk).await {
            Ok(0) | Err(_) => return,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
        }
        if let Some(at) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break at + 4;
        }
    };
    let head = String::from_utf8_lossy(&buf[..header_end]).to_string();
    let mut lines = head.lines();
    let mut first = lines.next().unwrap_or("").split_whitespace();
    let method = first.next().unwrap_or("").to_string();
    let path = first.next().unwrap_or("/").to_string();
    let length: usize = lines
        .filter_map(|l| l.split_once(':'))
        .find(|(k, _)| k.trim().eq_ignore_ascii_case("content-length"))
        .and_then(|(_, v)| v.trim().parse().ok())
        .unwrap_or(0);
    while buf.len() < header_end + length {
        match stream.read(&mut chunk).await {
            Ok(0) | Err(_) => break,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
        }
    }
    let (status, body) = if method == "OPTIONS" {
        ("204 No Content", String::new())
    } else {
        let body: Value = serde_json::from_slice(&buf[header_end..]).unwrap_or(Value::Null);
        match server.route(&method, &path, body).await {
            Ok(v) => ("200 OK", v.to_string()),
            Err(e) => ("400 Bad Request", json!({"error": e}).to_string()),
        }
    };
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Headers: content-type\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.shutdown().await;
}

fn arg_value(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn profile_from_session(args: &[String]) -> Result<(), String> {
    let [session, out, ..] = args else {
        return Err("usage: e2e_server profile-from-session <session.jsonl> <out-dir> [--name NAME] [--serial KEY]".into());
    };
    let text = std::fs::read_to_string(session).map_err(|e| format!("{session}: {e}"))?;
    let lines = replay::parse_session(&text);
    let key = arg_value(args, "--serial");
    let device = replay::device_from_session(&lines, key.as_deref())
        .ok_or("the session recorded no successful shell reads")?;
    let out = PathBuf::from(out);
    let name = arg_value(args, "--name").unwrap_or_else(|| {
        out.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "from-session".into())
    });
    replay::write_profile(&device, &name, &out).map_err(|e| e.to_string())?;
    println!(
        "wrote {} ({} packages, serial {}). Scrub it with `e2e/capture-device.sh --scrub-only {}` before checking it in.",
        out.display(),
        device.packages.len(),
        device.serial,
        out.display()
    );
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("profile-from-session") {
        if let Err(e) = profile_from_session(&args[1..]) {
            eprintln!("{e}");
            std::process::exit(2);
        }
        return;
    }
    let port: u16 = arg_value(&args, "--port")
        .and_then(|p| p.parse().ok())
        .unwrap_or(0);
    let fixtures = arg_value(&args, "--fixtures")
        .map(PathBuf::from)
        .unwrap_or_else(sim::fixtures_dir);

    let data_dir = std::env::temp_dir().join(format!("atvopt-e2e-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&data_dir);
    std::fs::create_dir_all(&data_dir).expect("create data dir");
    // The scan's port sweep is answered from the simulated LAN (192.0.2.x).
    std::env::set_var("SHIELD_OPTIMIZER_SUBNET", "192.0.2");
    let log_control = diagnostics::init_logging(&data_dir);

    let sim = SimulatedAdb::empty();
    {
        let sweep_sim = sim.clone();
        *shield_optimizer_v2_lib::adb::scan::SWEEP_OVERRIDE
            .write()
            .unwrap() = Some(Box::new(move |prefix: [u8; 3]| {
            let net = format!("{}.{}.{}.", prefix[0], prefix[1], prefix[2]);
            sweep_sim
                .world()
                .legacy_listeners()
                .into_iter()
                .filter(|ip| ip.starts_with(&net))
                .collect()
        }));
    }
    let app_lists = loader::load_embedded_app_lists().expect("embedded app lists");
    let state = AppState::new(Arc::new(sim.clone()), app_lists, data_dir.clone())
        .with_entitlement(Entitlement::Pro)
        .with_known_names(loader::load_known_names());

    let app = mock_builder()
        .manage(state)
        .manage(log_control)
        .invoke_handler(shield_optimizer_v2_lib::invoke_handler())
        .build(mock_context(noop_assets()))
        .expect("build mock app");
    let webview = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .expect("mock webview");

    let server = Arc::new(Server {
        sim,
        webview,
        fixtures,
        data_dir: data_dir.clone(),
        invokes: Mutex::new(Vec::new()),
    });

    tauri::async_runtime::block_on(async move {
        let listener = TcpListener::bind(("127.0.0.1", port)).await.expect("bind");
        let bound = listener.local_addr().expect("local addr").port();
        println!(
            "E2E_SERVER_LISTENING port={bound} data_dir={}",
            data_dir.display()
        );
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                continue;
            };
            tokio::spawn(serve_conn(server.clone(), stream));
        }
    });
}

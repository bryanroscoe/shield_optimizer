//! The host side of the simulation: the adb server's transports, mDNS view
//! and pairings, plus the devices behind them.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::device::{Device, DeviceShell};
use super::faults::{FaultEffect, FaultRule, FaultScope};
use super::shell::Out;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TransportState {
    Device,
    Unauthorized,
    Offline,
}

impl TransportState {
    fn as_adb(self) -> &'static str {
        match self {
            TransportState::Device => "device",
            TransportState::Unauthorized => "unauthorized",
            TransportState::Offline => "offline",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transport {
    /// adb's key: `ip:port`, an mDNS service name, or a USB serial.
    pub key: String,
    /// Hardware serial of the device behind it.
    pub serial: String,
    pub state: TransportState,
    pub id: u32,
}

/// One adb invocation, as the app made it and as the simulator answered.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Invocation {
    pub seq: u64,
    pub args: Vec<String>,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub timed_out: bool,
}

/// A settled reply, after any delay.
pub(crate) enum ReplyOut {
    Text(Out),
    Bytes(Vec<u8>),
}

/// What one adb call produced.
pub(crate) enum Reply {
    Out(Out),
    Timeout {
        delay_ms: u64,
    },
    Bytes(Vec<u8>),
    /// Answered, but `ms` of real time later.
    Delayed(Out, u64),
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct World {
    pub devices: BTreeMap<String, Device>,
    pub transports: Vec<Transport>,
    /// Hardware serials this host holds a Wireless debugging pairing for.
    pub paired: BTreeSet<String>,
    /// Extra raw `adb mdns services` lines (instance, service, endpoint).
    pub extra_mdns: Vec<(String, String, String)>,
    pub faults: Vec<FaultRule>,
    pub log: Vec<Invocation>,
    /// Commands the simulator had no handler for, in the order seen.
    pub gaps: Vec<String>,
    /// Package names of the catalog's stock launchers.
    pub stock_launchers: Vec<String>,
    /// Virtual seconds added by `sleep` (on top of real elapsed time).
    pub clock: f64,
    next_transport_id: u32,
    seq: u64,
    /// Transports a disconnect dropped that adb will bring back by itself.
    pending_reattach: Vec<(String, String, u32)>,
    /// When set, every call is answered from a recorded session first.
    #[serde(skip)]
    pub replay: Option<super::replay::Replay>,
}

const MAX_LOG: usize = 5000;

impl World {
    pub fn new(stock_launchers: Vec<String>) -> Self {
        Self {
            stock_launchers,
            next_transport_id: 1,
            ..Self::default()
        }
    }

    pub fn add_device(&mut self, device: Device) {
        self.devices.insert(device.serial.clone(), device);
    }

    pub fn attach(&mut self, key: &str, serial: &str, state: TransportState) {
        self.transports.retain(|t| t.key != key);
        self.next_transport_id += 1;
        self.transports.push(Transport {
            key: key.to_string(),
            serial: serial.to_string(),
            state,
            id: self.next_transport_id,
        });
    }

    pub fn detach(&mut self, key: &str) -> bool {
        let before = self.transports.len();
        self.transports.retain(|t| t.key != key);
        before != self.transports.len()
    }

    fn transport(&self, key: &str) -> Option<&Transport> {
        self.transports.iter().find(|t| t.key == key)
    }

    pub(crate) fn handle(&mut self, args: &[&str]) -> Reply {
        self.seq += 1;
        let joined = args.join(" ");
        let target_serial = match args {
            ["-s", key, ..] => self
                .transport(key)
                .map(|t| t.serial.clone())
                .unwrap_or_default(),
            _ => String::new(),
        };
        let reply =
            match super::faults::take(&mut self.faults, FaultScope::Adb, &target_serial, &joined) {
                Some(FaultEffect::Fail {
                    stdout,
                    stderr,
                    exit_code,
                }) => Reply::Out(Out {
                    stdout,
                    stderr,
                    code: exit_code,
                }),
                Some(FaultEffect::Output { stdout, exit_code }) => {
                    Reply::Out(Out::with_code(stdout, exit_code))
                }
                Some(FaultEffect::Ignore { stdout }) => Reply::Out(Out::ok(stdout)),
                Some(FaultEffect::Timeout { delay_ms }) => Reply::Timeout { delay_ms },
                Some(FaultEffect::Delay { ms }) => match self.answer(args) {
                    Reply::Out(o) => Reply::Delayed(o, ms),
                    Reply::Delayed(o, more) => Reply::Delayed(o, ms + more),
                    other => other,
                },
                None => self.answer(args),
            };
        let (stdout, stderr, exit_code, timed_out) = match &reply {
            Reply::Out(o) | Reply::Delayed(o, _) => {
                (o.stdout.clone(), o.stderr.clone(), Some(o.code), false)
            }
            Reply::Timeout { .. } => (String::new(), String::new(), None, true),
            Reply::Bytes(b) => (
                format!("<{} bytes>", b.len()),
                String::new(),
                Some(0),
                false,
            ),
        };
        self.log.push(Invocation {
            seq: self.seq,
            args: args.iter().map(|s| s.to_string()).collect(),
            stdout,
            stderr,
            exit_code,
            timed_out,
        });
        if self.log.len() > MAX_LOG {
            self.log.drain(..self.log.len() - MAX_LOG);
        }
        reply
    }

    fn answer(&mut self, args: &[&str]) -> Reply {
        if let Some(replay) = self.replay.as_mut() {
            if let Some((out, timed_out)) = replay.answer(args) {
                return if timed_out {
                    Reply::Timeout { delay_ms: 0 }
                } else {
                    Reply::Out(out)
                };
            }
        }
        self.dispatch(args)
    }

    fn dispatch(&mut self, args: &[&str]) -> Reply {
        if let ["-s", key, rest @ ..] = args {
            return self.on_transport(key, rest);
        }
        let out = match args {
            ["devices"] => self.devices_list(false),
            ["devices", "-l"] => self.devices_list(true),
            ["connect", target] => self.connect(target),
            ["disconnect"] => {
                self.transports.retain(|t| !is_network_key(&t.key));
                Out::ok("disconnected everything\n")
            }
            ["disconnect", key] => self.disconnect(key),
            ["pair", addr, pin] => self.pair(addr, pin),
            ["mdns", "services"] => Out::ok(self.mdns_services()),
            ["mdns", "check"] => Out::ok("mdns daemon version [Openscreen discovery 0.0.0]\n"),
            ["start-server"] => {
                self.restart_autoconnect();
                Out::ok("")
            }
            ["kill-server"] => {
                self.transports.retain(|t| !is_network_key(&t.key));
                Out::ok("")
            }
            ["version"] => Out::ok(
                "Android Debug Bridge version 1.0.41\nVersion 35.0.2-simulated\nInstalled as /simulated/platform-tools/adb\n",
            ),
            ["reconnect", ..] => Out::ok(""),
            _ => {
                self.gaps.push(format!("adb {}", args.join(" ")));
                Out::err(
                    format!("simulated adb has no handler for: adb {}\n", args.join(" ")),
                    1,
                )
            }
        };
        Reply::Out(out)
    }

    fn devices_list(&mut self, long: bool) -> Out {
        let mut ready = Vec::new();
        for entry in self.pending_reattach.iter_mut() {
            if entry.2 <= 1 {
                ready.push((entry.0.clone(), entry.1.clone()));
                entry.2 = 0;
            } else {
                entry.2 -= 1;
            }
        }
        self.pending_reattach.retain(|e| e.2 > 0);
        for (key, serial) in ready {
            self.attach(&key, &serial, TransportState::Device);
        }
        let mut out = String::from("List of devices attached\n");
        for t in &self.transports {
            out.push_str(&format!("{}\t{}", t.key, t.state.as_adb()));
            if long {
                match (t.state, self.devices.get(&t.serial)) {
                    (TransportState::Device, Some(d)) => out.push_str(&format!(
                        " product:{} model:{} device:{} transport_id:{}",
                        d.prop("ro.product.name"),
                        d.prop("ro.product.model").replace(' ', "_"),
                        d.prop("ro.product.device"),
                        t.id
                    )),
                    _ => out.push_str(&format!(" transport_id:{}", t.id)),
                }
            }
            out.push('\n');
        }
        out.push('\n');
        Out::ok(out)
    }

    /// The device listening at `host:port`, and whether that is its Wireless
    /// debugging (TLS) port rather than legacy network debugging.
    fn device_at(&self, host: &str, port: u16) -> Option<(String, bool)> {
        self.devices.values().find_map(|d| {
            let net = d.network.as_ref()?;
            if net.ip != host {
                return None;
            }
            if net.legacy_port == Some(port) {
                return Some((d.serial.clone(), false));
            }
            match &net.wireless {
                Some(w) if w.connect_port == port => Some((d.serial.clone(), true)),
                _ => None,
            }
        })
    }

    fn connect(&mut self, target: &str) -> Out {
        if let Some(t) = self.transport(target) {
            if t.state == TransportState::Device {
                return Out::ok(format!("already connected to {target}\n"));
            }
        }
        // An mDNS service name resolves to the endpoint it advertises.
        let endpoint = if target.contains("._adb-tls-connect._tcp") {
            let instance = target.split("._").next().unwrap_or("");
            self.devices.values().find_map(|d| {
                let net = d.network.as_ref()?;
                let w = net.wireless.as_ref()?;
                (w.connect_instance == instance).then(|| format!("{}:{}", net.ip, w.connect_port))
            })
        } else {
            Some(target.to_string())
        };
        let refused = Out::with_code(
            format!("failed to connect to '{target}': Connection refused\n"),
            1,
        );
        let Some(endpoint) = endpoint else {
            return refused;
        };
        let Some((host, port)) = endpoint
            .rsplit_once(':')
            .and_then(|(h, p)| Some((h.to_string(), p.parse::<u16>().ok()?)))
        else {
            return refused;
        };
        let Some((serial, tls)) = self.device_at(&host, port) else {
            return refused;
        };
        let authorized = self.devices[&serial].authorized;
        if tls && !self.paired.contains(&serial) {
            return Out::with_code(
                format!("failed to connect to {target}: unable to complete TLS handshake (not paired)\n"),
                1,
            );
        }
        if !authorized {
            self.attach(target, &serial, TransportState::Unauthorized);
            return Out::ok(format!("failed to authenticate to {target}\n"));
        }
        self.attach(target, &serial, TransportState::Device);
        Out::ok(format!("connected to {target}\n"))
    }

    fn disconnect(&mut self, key: &str) -> Out {
        let Some(t) = self.transport(key).cloned() else {
            return Out::with_code(format!("error: no such device '{key}'\n"), 1);
        };
        self.detach(key);
        if let Some(d) = self.devices.get(&t.serial) {
            if d.auto_attach && self.paired.contains(&d.serial) {
                if let Some(mdns_key) = mdns_key(d) {
                    let polls = d.reattach_after_polls.unwrap_or(1);
                    self.pending_reattach.retain(|(k, _, _)| k != &mdns_key);
                    self.pending_reattach
                        .push((mdns_key, d.serial.clone(), polls.max(1)));
                }
            }
        }
        Out::ok(format!("disconnected {key}\n"))
    }

    fn pair(&mut self, addr: &str, pin: &str) -> Out {
        let found = self.devices.values_mut().find_map(|d| {
            let net = d.network.as_ref()?;
            let w = net.wireless.as_ref()?;
            let (instance, port, code) = w.pairing.clone()?;
            (format!("{}:{port}", net.ip) == addr).then_some((d.serial.clone(), instance, code))
        });
        let Some((serial, instance, code)) = found else {
            return Out::with_code("Failed: Unable to start pairing client.\n", 1);
        };
        if code != pin {
            return Out::with_code("Failed: Wrong password or connection was dropped.\n", 1);
        }
        self.paired.insert(serial.clone());
        let dev = self.devices.get_mut(&serial).expect("device exists");
        if let Some(w) = dev.network.as_mut().and_then(|n| n.wireless.as_mut()) {
            w.pairing = None;
        }
        if dev.auto_attach {
            if let Some(key) = mdns_key(dev) {
                self.attach(&key, &serial, TransportState::Device);
            }
        }
        Out::ok(format!("Successfully paired to {addr} [guid={instance}]\n"))
    }

    fn restart_autoconnect(&mut self) {
        let keys: Vec<(String, String)> = self
            .devices
            .values()
            .filter(|d| d.auto_attach && self.paired.contains(&d.serial))
            .filter_map(|d| Some((mdns_key(d)?, d.serial.clone())))
            .collect();
        for (key, serial) in keys {
            if self.transport(&key).is_none() {
                self.attach(&key, &serial, TransportState::Device);
            }
        }
    }

    pub fn mdns_services(&self) -> String {
        let mut out = String::from("List of discovered mdns services\n");
        for d in self.devices.values() {
            let Some(net) = &d.network else { continue };
            if let (true, Some(port)) = (net.advertise_legacy, net.legacy_port) {
                out.push_str(&format!("adb-{}\t_adb._tcp\t{}:{port}\n", d.serial, net.ip));
            }
            if let Some(w) = &net.wireless {
                out.push_str(&format!(
                    "{}\t_adb-tls-connect._tcp\t{}:{}\n",
                    w.connect_instance, net.ip, w.connect_port
                ));
                if let Some((instance, port, _)) = &w.pairing {
                    out.push_str(&format!(
                        "{instance}\t_adb-tls-pairing._tcp\t{}:{port}\n",
                        net.ip
                    ));
                }
            }
        }
        for (instance, service, endpoint) in &self.extra_mdns {
            out.push_str(&format!("{instance}\t{service}\t{endpoint}\n"));
        }
        out
    }

    /// IPs that answer on :5555, for the subnet sweep.
    pub fn legacy_listeners(&self) -> Vec<String> {
        self.devices
            .values()
            .filter_map(|d| {
                let net = d.network.as_ref()?;
                (net.legacy_port == Some(5555)).then(|| net.ip.clone())
            })
            .collect()
    }

    fn on_transport(&mut self, key: &str, rest: &[&str]) -> Reply {
        let Some(t) = self.transport(key).cloned() else {
            return Reply::Out(Out::err(format!("adb: device '{key}' not found\n"), 1));
        };
        match t.state {
            TransportState::Unauthorized => {
                return Reply::Out(Out::err(
                    "adb: device unauthorized.\nThis adb server's $ADB_VENDOR_KEYS is not set\nTry 'adb kill-server' if that seems wrong.\nOtherwise check for a confirmation dialog on your device.\n",
                    1,
                ))
            }
            TransportState::Offline => {
                return Reply::Out(Out::err("adb: device offline\n", 1))
            }
            TransportState::Device => {}
        }
        let serial = t.serial;
        match rest {
            ["shell", cmd @ ..] if !cmd.is_empty() => self.shell(&serial, &cmd.join(" ")),
            ["exec-out", "screencap", "-p"] => Reply::Bytes(TINY_PNG.to_vec()),
            ["push", local, remote] => {
                let name = local.rsplit(['/', '\\']).next().unwrap_or(local);
                let dest = if remote.ends_with('/') {
                    format!("{remote}{name}")
                } else {
                    remote.to_string()
                };
                if let Some(d) = self.devices.get_mut(&serial) {
                    d.files.insert(dest, format!("pushed from {local}\n"));
                }
                Reply::Out(Out::ok(format!("{local}: 1 file pushed, 0 skipped.\n")))
            }
            ["pull", remote, local] => {
                let body = self
                    .devices
                    .get(&serial)
                    .and_then(|d| d.files.get(*remote).cloned());
                match body {
                    Some(body) => {
                        let path = std::path::Path::new(local);
                        let path = if path.is_dir() {
                            path.join(remote.rsplit('/').next().unwrap_or("pulled"))
                        } else {
                            path.to_path_buf()
                        };
                        match std::fs::write(&path, body) {
                            Ok(()) => Reply::Out(Out::ok(format!("{remote}: 1 file pulled.\n"))),
                            Err(e) => Reply::Out(Out::err(format!("adb: error: {e}\n"), 1)),
                        }
                    }
                    None => Reply::Out(Out::err(
                        format!("adb: error: failed to stat remote object '{remote}': No such file or directory\n"),
                        1,
                    )),
                }
            }
            ["install", ..] | ["install-multiple", ..] => {
                Reply::Out(Out::ok("Performing Streamed Install\nSuccess\n"))
            }
            ["forward", ..] | ["reboot", ..] => Reply::Out(Out::ok("")),
            ["get-state"] => Reply::Out(Out::ok("device\n")),
            _ => {
                self.gaps.push(format!("adb -s {key} {}", rest.join(" ")));
                Reply::Out(Out::err(
                    format!(
                        "simulated adb has no handler for: adb -s {key} {}\n",
                        rest.join(" ")
                    ),
                    1,
                ))
            }
        }
    }

    pub(crate) fn shell(&mut self, serial: &str, command: &str) -> Reply {
        let Some(dev) = self.devices.get_mut(serial) else {
            return Reply::Out(Out::err("adb: device not found\n", 1));
        };
        let mut ctx = DeviceShell {
            dev,
            faults: &mut self.faults,
            gaps: &mut self.gaps,
            stock: &self.stock_launchers,
            clock: &mut self.clock,
            timeout: None,
            delay_ms: 0,
        };
        let out = super::shell::run(command, &mut ctx);
        if let Some(delay_ms) = ctx.timeout {
            return Reply::Timeout { delay_ms };
        }
        if ctx.delay_ms > 0 {
            return Reply::Delayed(out, ctx.delay_ms);
        }
        Reply::Out(out)
    }

    /// Run a setup command on a device directly: no faults, not logged.
    pub fn setup_shell(&mut self, serial: &str, command: &str) -> Result<String, String> {
        let saved = std::mem::take(&mut self.faults);
        let reply = self.shell(serial, command);
        self.faults = saved;
        match reply {
            Reply::Out(o) | Reply::Delayed(o, _) if o.code == 0 => Ok(o.stdout),
            Reply::Out(o) | Reply::Delayed(o, _) => Err(format!("{}{}", o.stdout, o.stderr)),
            _ => Err("unexpected reply".into()),
        }
    }
}

fn is_network_key(key: &str) -> bool {
    key.contains(':') || key.contains("._tcp")
}

pub(crate) fn mdns_key(d: &Device) -> Option<String> {
    let w = d.network.as_ref()?.wireless.as_ref()?;
    Some(format!("{}._adb-tls-connect._tcp", w.connect_instance))
}

/// 1×1 transparent PNG.
const TINY_PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
    0x42, 0x60, 0x82,
];

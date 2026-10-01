//! One simulated Android device: its packages, settings, properties and Home
//! state, and the device-side commands that read and change them.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::faults::{FaultEffect, FaultRule, FaultScope};
use super::shell::{glob_match, printf, Exec, Out};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Package {
    pub system: bool,
    /// `false` once `pm disable-user` / `pm disable` ran.
    pub enabled: bool,
    /// `false` after `pm uninstall --user 0` on a system app: still on the
    /// image (`pm list packages -u` shows it), gone for the user.
    pub installed: bool,
}

/// A component that declares `MAIN`/`HOME`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HomeComponent {
    pub package: String,
    /// Fully qualified activity class.
    pub class: String,
    /// Intent-filter priority. Settings' FallbackHome sits at -1000.
    pub priority: i32,
}

impl HomeComponent {
    /// `pkg/.Short` when the class lives under the package, as Android prints it.
    pub fn short(&self) -> String {
        match self.class.strip_prefix(&format!("{}.", self.package)) {
            Some(rest) => format!("{}/.{rest}", self.package),
            None => format!("{}/{}", self.package, self.class),
        }
    }

    fn matches(&self, component: &str) -> bool {
        let Some((pkg, class)) = component.split_once('/') else {
            return false;
        };
        let class = if let Some(rest) = class.strip_prefix('.') {
            format!("{pkg}.{rest}")
        } else {
            class.to_string()
        };
        pkg == self.package && class == self.class
    }
}

/// How the device decides which Home handler wins.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum HomePolicy {
    /// A preferred activity or role holder wins; with none and several
    /// handlers, the chooser (`ResolverActivity`) answers.
    #[default]
    Preference,
    /// An enabled stock launcher overrides every preference (Shield / Android
    /// 11 answers "Success" to set-home-activity and stays on stock). Only
    /// disabling stock hands Home over.
    StockOverrides,
    /// `resolve-activity` ranks HOME filters by priority and ignores the
    /// preferred activity and the role, while the Home key follows the HOME
    /// role. Google TV with Setup Wraith enabled (#122): the resolver names
    /// Setup Wraith even after the role, and Home, moved to Monet.
    PriorityResolver,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HomeState {
    pub components: Vec<HomeComponent>,
    /// `pkg/.Activity` set by `set-home-activity` (or the initial resolve).
    pub preferred: Option<String>,
    pub role_holder: Option<String>,
    pub policy: HomePolicy,
    /// A transient holder (Setup Wraith) the resolver reports for the next
    /// `transient_polls` reads before settling.
    pub transient_holder: Option<String>,
    pub transient_polls: u32,
}

/// Wireless debugging (Android 11+): a TLS connect service on a random port,
/// and while the pairing dialog is open, a pairing service with its own
/// random instance suffix and port.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Wireless {
    pub connect_port: u16,
    /// `adb-<serial>-<suffix>` for `_adb-tls-connect._tcp`.
    pub connect_instance: String,
    /// Open pairing dialog: `(instance, port, pin)`.
    pub pairing: Option<(String, u16, String)>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Network {
    pub ip: String,
    /// Legacy "Network debugging" on :5555 (what the subnet sweep finds).
    pub legacy_port: Option<u16>,
    pub wireless: Option<Wireless>,
    /// Advertise `_adb._tcp` for the legacy port.
    #[serde(default)]
    pub advertise_legacy: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    pub serial: String,
    pub props: BTreeMap<String, String>,
    /// `pm has-feature android.software.leanback`: `None` = the build has no
    /// answer (prints usage, exits 1).
    pub leanback: Option<bool>,
    /// Insertion-ordered package table.
    pub packages: Vec<(String, Package)>,
    pub settings: BTreeMap<String, BTreeMap<String, String>>,
    pub home: HomeState,
    /// Canned outputs for read-only commands, keyed by the exact command.
    pub texts: BTreeMap<String, String>,
    /// Device files `cat` can read (globs are matched against these).
    pub files: BTreeMap<String, String>,
    pub wm_size: (String, Option<String>),
    pub wm_density: (String, Option<String>),
    pub appops: BTreeMap<String, String>,
    pub permissions: BTreeMap<String, bool>,
    pub network: Option<Network>,
    /// This host's adb key is trusted by the device.
    pub authorized: bool,
    /// adb re-attaches the device by itself (paired + advertised).
    pub auto_attach: bool,
    /// After a disconnect, the transport comes back on the N-th `adb devices`.
    pub reattach_after_polls: Option<u32>,
    /// Keys and text sent with `input`, newest last.
    pub input_log: Vec<String>,
    /// Last `am start` intents.
    pub started: Vec<String>,
    pub uptime_base: f64,
}

pub(crate) const NS: [&str; 3] = ["global", "secure", "system"];

impl Device {
    pub fn new(serial: &str) -> Self {
        Self {
            serial: serial.to_string(),
            props: BTreeMap::new(),
            leanback: None,
            packages: Vec::new(),
            settings: NS
                .iter()
                .map(|n| (n.to_string(), BTreeMap::new()))
                .collect(),
            home: HomeState::default(),
            texts: BTreeMap::new(),
            files: BTreeMap::new(),
            wm_size: ("1920x1080".into(), None),
            wm_density: ("320".into(), None),
            appops: BTreeMap::new(),
            permissions: BTreeMap::new(),
            network: None,
            authorized: true,
            auto_attach: false,
            reattach_after_polls: None,
            input_log: Vec::new(),
            started: Vec::new(),
            uptime_base: 86_400.0,
        }
    }

    pub fn package(&self, name: &str) -> Option<&Package> {
        self.packages
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, p)| p)
    }

    pub fn package_mut(&mut self, name: &str) -> Option<&mut Package> {
        self.packages
            .iter_mut()
            .find(|(n, _)| n == name)
            .map(|(_, p)| p)
    }

    /// Install a user app that declares HOME with `class` at `priority`.
    pub fn add_home_app(&mut self, package: &str, class: &str, priority: i32) {
        self.add_package(
            package,
            Package {
                system: false,
                enabled: true,
                installed: true,
            },
        );
        self.home.components.retain(|c| c.package != package);
        self.home.components.push(HomeComponent {
            package: package.to_string(),
            class: class.to_string(),
            priority,
        });
    }

    pub fn add_package(&mut self, name: &str, pkg: Package) {
        match self.package_mut(name) {
            Some(existing) => *existing = pkg,
            None => self.packages.push((name.to_string(), pkg)),
        }
    }

    fn usable(&self, name: &str) -> bool {
        self.package(name).is_some_and(|p| p.installed && p.enabled)
    }

    pub fn prop(&self, key: &str) -> String {
        self.props.get(key).cloned().unwrap_or_default()
    }

    fn sdk(&self) -> u32 {
        self.prop("ro.build.version.sdk").parse().unwrap_or(30)
    }

    /// Enabled HOME handlers, highest priority first.
    pub fn home_handlers(&self) -> Vec<&HomeComponent> {
        let mut out: Vec<&HomeComponent> = self
            .home
            .components
            .iter()
            .filter(|c| self.usable(&c.package))
            .collect();
        out.sort_by_key(|c| std::cmp::Reverse(c.priority));
        out
    }

    /// What `resolve-activity --brief … HOME` answers right now. `stock` is
    /// the catalog's stock launcher list.
    pub fn resolve_home(&mut self, stock: &[String]) -> Option<String> {
        if self.home.transient_polls > 0 {
            if let Some(holder) = self.home.transient_holder.clone() {
                self.home.transient_polls -= 1;
                return Some(holder);
            }
        }
        let handlers: Vec<HomeComponent> = self.home_handlers().into_iter().cloned().collect();
        let real: Vec<&HomeComponent> = handlers.iter().filter(|c| c.priority > -1000).collect();
        if self.home.policy == HomePolicy::PriorityResolver {
            if let Some(top) = real.first() {
                return Some(top.short());
            }
        }
        if self.home.policy == HomePolicy::StockOverrides {
            if let Some(s) = real.iter().find(|c| stock.contains(&c.package)) {
                return Some(s.short());
            }
        }
        if let Some(pref) = &self.home.preferred {
            if let Some(c) = real.iter().find(|c| c.matches(pref)) {
                return Some(c.short());
            }
        }
        if let Some(holder) = &self.home.role_holder {
            if let Some(c) = real.iter().find(|c| &c.package == holder) {
                return Some(c.short());
            }
        }
        match real.as_slice() {
            [one] => Some(one.short()),
            [] => handlers.first().map(|c| c.short()),
            _ => Some("android/com.android.internal.app.ResolverActivity".to_string()),
        }
    }

    fn query_home(&self, components: bool) -> String {
        let handlers = self.home_handlers();
        if components {
            return handlers
                .iter()
                .map(|c| format!("{}\n", c.short()))
                .collect();
        }
        if handlers.is_empty() {
            return "No activities found\n".into();
        }
        let mut out = format!("{} activities found:\n", handlers.len());
        for (i, c) in handlers.iter().enumerate() {
            out.push_str(&format!(
                "  Activity #{i}:\n    priority={} preferredOrder=0 match=0x108000 specificIndex=-1 isDefault=true\n    ActivityInfo:\n      name={}\n      packageName={}\n      enabled=true exported=true directBootAware=false\n",
                c.priority, c.class, c.package
            ));
        }
        out
    }

    fn disable(&mut self, pkg: &str, how: &str) -> Out {
        match self.package_mut(pkg) {
            Some(p) if p.installed => {
                p.enabled = false;
                if self.home.role_holder.as_deref() == Some(pkg) {
                    self.home.role_holder = None;
                }
                Out::ok(format!("Package {pkg} new state: {how}\n"))
            }
            _ => unknown_package("disable-user", pkg),
        }
    }

    fn enable(&mut self, pkg: &str) -> Out {
        match self.package_mut(pkg) {
            Some(p) if p.installed => {
                p.enabled = true;
                Out::ok(format!("Package {pkg} new state: enabled\n"))
            }
            _ => unknown_package("enable", pkg),
        }
    }

    fn list_packages(&self, args: &[String]) -> Out {
        let flags: Vec<&str> = args
            .iter()
            .map(String::as_str)
            .filter(|a| a.starts_with('-'))
            .collect();
        let filter = args
            .iter()
            .map(String::as_str)
            .rfind(|a| !a.starts_with('-') && *a != "0");
        let has = |f: &str| flags.contains(&f);
        let mut out = String::new();
        for (name, p) in &self.packages {
            if !p.installed && !has("-u") {
                continue;
            }
            if has("-d") && p.enabled {
                continue;
            }
            if has("-e") && !p.enabled {
                continue;
            }
            if has("-3") && p.system {
                continue;
            }
            if has("-s") && !p.system {
                continue;
            }
            if let Some(f) = filter {
                if !name.contains(f) {
                    continue;
                }
            }
            if has("-f") {
                out.push_str(&format!("package:{}={name}\n", apk_path(name, p)));
            } else {
                out.push_str(&format!("package:{name}\n"));
            }
        }
        Out::ok(out)
    }

    /// `pm …` / `cmd package …`.
    fn pm(&mut self, args: &[String], stock: &[String]) -> Option<Out> {
        let sub = args.first()?.as_str();
        let rest = &args[1..];
        let target = |rest: &[String]| -> Option<String> {
            rest.iter()
                .rfind(|a| !a.starts_with('-') && *a != "0")
                .cloned()
        };
        Some(match sub {
            "list" if rest.first().map(String::as_str) == Some("packages") => {
                self.list_packages(&rest[1..])
            }
            "disable-user" => match target(rest) {
                Some(p) => self.disable(&p, "disabled-user"),
                None => Out::err("Error: no package specified\n", 1),
            },
            "disable" => match target(rest) {
                Some(p) => self.disable(&p, "disabled"),
                None => Out::err("Error: no package specified\n", 1),
            },
            "enable" => match target(rest) {
                Some(p) => self.enable(&p),
                None => Out::err("Error: no package specified\n", 1),
            },
            "uninstall" => match target(rest) {
                Some(p) => match self.package_mut(&p) {
                    Some(pkg) if pkg.installed => {
                        if pkg.system {
                            pkg.installed = false;
                        } else {
                            self.packages.retain(|(n, _)| n != &p);
                        }
                        Out::ok("Success\n")
                    }
                    _ => Out::with_code("Failure [DELETE_FAILED_INTERNAL_ERROR]\n".to_string(), 1),
                },
                None => Out::err("Error: no package specified\n", 1),
            },
            "install-existing" => match target(rest) {
                Some(p) => match self.package_mut(&p) {
                    Some(pkg) => {
                        pkg.installed = true;
                        Out::ok(format!("Package {p} installed for user: 0\n"))
                    }
                    None => Out::with_code(format!("Package {p} doesn't exist\n"), 1),
                },
                None => Out::err("Error: no package specified\n", 1),
            },
            "trim-caches" => Out::ok(""),
            "path" => match target(rest) {
                Some(p) => match self.package(&p) {
                    Some(pkg) if pkg.installed => {
                        Out::ok(format!("package:{}\n", apk_path(&p, pkg)))
                    }
                    _ => Out::with_code("", 1),
                },
                None => Out::err("Error: no package specified\n", 1),
            },
            "has-feature" => match rest.first().map(String::as_str) {
                Some("android.software.leanback") => match self.leanback {
                    Some(true) => Out::ok("true\n"),
                    Some(false) => Out::with_code("false\n", 1),
                    None => Out::err("Unknown command: has-feature\n", 1),
                },
                _ => Out::with_code("false\n", 1),
            },
            "grant" | "revoke" => {
                let (Some(pkg), Some(perm)) = (rest.first(), rest.get(1)) else {
                    return Some(Out::err("Error: no package specified\n", 1));
                };
                if self.package(pkg).is_none() {
                    return Some(unknown_package(sub, pkg));
                }
                self.permissions
                    .insert(format!("{pkg}|{perm}"), sub == "grant");
                Out::ok("")
            }
            "set-home-activity" => match target(rest) {
                Some(comp) => self.set_home_activity(&comp, stock),
                None => Out::err("Error: no component specified\n", 1),
            },
            "query-activities" => {
                let components = rest.iter().any(|a| a == "--components");
                if rest.iter().any(|a| a == "android.intent.category.HOME") {
                    Out::ok(self.query_home(components))
                } else {
                    return None;
                }
            }
            "resolve-activity" => {
                if !rest.iter().any(|a| a == "android.intent.category.HOME") {
                    return None;
                }
                match self.resolve_home(stock) {
                    Some(c) => Out::ok(format!(
                        "priority=0 preferredOrder=0 match=0x108000 specificIndex=-1 isDefault=true\n{c}\n"
                    )),
                    None => Out::ok("No activity found\n"),
                }
            }
            _ => return None,
        })
    }

    fn set_home_activity(&mut self, comp: &str, _stock: &[String]) -> Out {
        let found = self
            .home
            .components
            .iter()
            .find(|c| c.matches(comp))
            .cloned();
        match found {
            Some(c) if self.usable(&c.package) => {
                self.home.preferred = Some(c.short());
                if self.sdk() >= 29 {
                    self.home.role_holder = Some(c.package.clone());
                }
                Out::ok("Success\n")
            }
            _ => Out::with_code(
                format!(
                    "Error: java.lang.IllegalArgumentException: Component {comp} is not a Home activity\n"
                ),
                255,
            ),
        }
    }

    fn role(&mut self, args: &[String]) -> Option<Out> {
        let sub = args.first()?.as_str();
        let sdk = self.sdk();
        Some(match sub {
            "get-role-holders" if sdk >= 31 => {
                if args.get(1).map(String::as_str) != Some("android.app.role.HOME") {
                    return Some(Out::ok("\n"));
                }
                Out::ok(format!(
                    "{}\n",
                    self.home.role_holder.clone().unwrap_or_default()
                ))
            }
            "add-role-holder" if sdk >= 29 => {
                let Some(pkg) = args.iter().skip(2).find(|a| !a.starts_with('-')) else {
                    return Some(Out::err("Error: no package\n", 1));
                };
                let declares = self.home.components.iter().find(|c| &c.package == pkg);
                if let Some(c) = declares.cloned() {
                    if self.usable(pkg) {
                        self.home.role_holder = Some(pkg.clone());
                        self.home.preferred = Some(c.short());
                    }
                }
                Out::ok("")
            }
            other => Out::ok(format!("Unknown command: {other}\n")),
        })
    }

    fn settings(&mut self, args: &[String]) -> Out {
        let args: Vec<&String> = {
            let mut v = Vec::new();
            let mut skip = false;
            for a in args {
                if skip {
                    skip = false;
                    continue;
                }
                if a == "--user" {
                    skip = true;
                    continue;
                }
                v.push(a);
            }
            v
        };
        let usage = || Out::err("usage: settings [--user <USER_ID>] get namespace key\n", 1);
        let Some(verb) = args.first() else {
            return usage();
        };
        let Some(ns) = args.get(1).map(|s| s.as_str()) else {
            return usage();
        };
        if !NS.contains(&ns) {
            return Out::err(format!("Invalid namespace '{ns}'\n"), 1);
        }
        let table = self.settings.entry(ns.to_string()).or_default();
        match verb.as_str() {
            "get" => match args.get(2) {
                Some(k) => Out::ok(format!(
                    "{}\n",
                    table
                        .get(k.as_str())
                        .cloned()
                        .unwrap_or_else(|| "null".into())
                )),
                None => usage(),
            },
            "put" => match (args.get(2), args.get(3)) {
                (Some(k), Some(v)) => {
                    table.insert(k.to_string(), v.to_string());
                    Out::ok("")
                }
                _ => usage(),
            },
            "delete" => match args.get(2) {
                Some(k) => {
                    let n = usize::from(table.remove(k.as_str()).is_some());
                    Out::ok(format!("Deleted {n} rows\n"))
                }
                None => usage(),
            },
            "list" => Out::ok(
                table
                    .iter()
                    .map(|(k, v)| format!("{k}={v}\n"))
                    .collect::<String>(),
            ),
            _ => usage(),
        }
    }

    fn getprop(&self, args: &[String]) -> Out {
        match args.first() {
            Some(k) => Out::ok(format!("{}\n", self.prop(k))),
            None => Out::ok(
                self.props
                    .iter()
                    .map(|(k, v)| format!("[{k}]: [{v}]\n"))
                    .collect::<String>(),
            ),
        }
    }

    fn wm(&mut self, args: &[String]) -> Option<Out> {
        let (field, label) = match args.first()?.as_str() {
            "size" => (&mut self.wm_size, "size"),
            "density" => (&mut self.wm_density, "density"),
            _ => return None,
        };
        Some(match args.get(1).map(String::as_str) {
            None => {
                let mut out = format!("Physical {label}: {}\n", field.0);
                if let Some(o) = &field.1 {
                    out.push_str(&format!("Override {label}: {o}\n"));
                }
                Out::ok(out)
            }
            Some("reset") => {
                field.1 = None;
                Out::ok("")
            }
            Some(v) => {
                field.1 = Some(v.to_string());
                Out::ok("")
            }
        })
    }

    fn appops(&mut self, args: &[String]) -> Option<Out> {
        let sub = args.first()?.as_str();
        let pkg = args.get(1)?;
        let op = args.get(2)?;
        let key = format!("{pkg}|{op}");
        Some(match sub {
            "get" => {
                let mode = self
                    .appops
                    .get(&key)
                    .cloned()
                    .unwrap_or_else(|| "allow".into());
                Out::ok(format!("{op}: {mode}\n"))
            }
            "set" => {
                self.appops
                    .insert(key, args.get(3).cloned().unwrap_or_else(|| "allow".into()));
                Out::ok("")
            }
            _ => return None,
        })
    }

    pub(super) fn dumpsys(&mut self, args: &[String], clock: f64) -> Option<Out> {
        let key = format!("dumpsys {}", args.join(" "));
        if let Some(text) = self.texts.get(&key) {
            let text = text.clone();
            return Some(Out::ok(match args.first().map(String::as_str) {
                Some("meminfo") if args.len() == 1 => self.without_stopped_processes(&text),
                _ => text,
            }));
        }
        let first = args.first()?.as_str();
        Some(Out::ok(match first {
            "package" if args.len() == 2 => self.dumpsys_package(&args[1]),
            "usagestats" => self.usagestats(clock),
            // AOSP's default, served only for a profile with no capture, so a
            // generated profile never writes it out as if the device said it.
            "activity" if args.len() == 2 && args[1] == "settings" => {
                "ACTIVITY MANAGER SETTINGS (dumpsys activity settings) activity_manager_constants:\n  max_cached_processes=32\n\n  CUR_MAX_CACHED_PROCESSES=32\n  CUR_MAX_EMPTY_PROCESSES=16\n".into()
            }
            _ => return None,
        }))
    }

    /// A disabled or uninstalled app has no process; drop its rows from the
    /// captured memory report so the Health tab sees what a real device would.
    fn without_stopped_processes(&self, text: &str) -> String {
        let stopped: Vec<&str> = self
            .packages
            .iter()
            .filter(|(_, p)| !p.enabled || !p.installed)
            .map(|(n, _)| n.as_str())
            .collect();
        text.lines()
            .filter(|line| {
                let Some((_, rest)) = line.split_once("K: ") else {
                    return true;
                };
                let process = rest.split([' ', ':']).next().unwrap_or("");
                !stopped.contains(&process)
            })
            .map(|l| format!("{l}\n"))
            .collect()
    }

    fn dumpsys_package(&self, pkg: &str) -> String {
        let Some(p) = self.package(pkg) else {
            return format!("Unable to find package: {pkg}\n");
        };
        let mut out = format!(
            "Packages:\n  Package [{pkg}] (5f1e2d3):\n    userId=10123\n    pkg=Package{{5f1e2d3 {pkg}}}\n    codePath={}\n    versionName=1.0\n    enabled={}\n    User 0: ceDataInode=0 installed={} hidden=false suspended=false stopped=false notLaunched=false enabled={}\n",
            apk_path(pkg, p).rsplit_once('/').map(|(d, _)| d).unwrap_or("/data/app"),
            if p.enabled { 0 } else { 3 },
            p.installed,
            if p.enabled { 0 } else { 3 },
        );
        let perms: Vec<(&str, bool)> = self
            .permissions
            .iter()
            .filter_map(|(k, v)| {
                let (kp, perm) = k.split_once('|')?;
                (kp == pkg).then_some((perm, *v))
            })
            .collect();
        if !perms.is_empty() {
            out.push_str("      runtime permissions:\n");
            for (perm, granted) in perms {
                out.push_str(&format!(
                    "        {perm}: granted={granted}, flags=[ USER_SET ]\n"
                ));
            }
        }
        out
    }

    fn usagestats(&self, _clock: f64) -> String {
        let mut out = String::from("user=0\n In-memory daily stats\n  timeRange=\"2026-09-29, 00:00 - 2026-09-30, 00:00\"\n  packages\n");
        for (i, (name, p)) in self.packages.iter().enumerate() {
            if !p.installed || p.system || i % 3 == 0 {
                continue;
            }
            out.push_str(&format!(
                "    package={name} totalTimeUsed=\"0{}:1{}\" lastTimeUsed=\"2026-09-2{} 20:1{}:00\" totalTimeVisible=\"00:10\" lastTimeVisible=\"2026-09-29 20:10:00\" lastTimeComponentUsed=\"2026-09-29 20:10:00\" totalTimeFS=\"00:00\" lastTimeFS=\"1970-01-01 00:00:00\" appLaunchCount={}\n",
                i % 9,
                i % 6,
                i % 9,
                i % 6,
                i % 7 + 1
            ));
        }
        out
    }

    fn proc_file(&self, path: &str, clock: f64) -> Option<String> {
        let uptime = self.uptime_base + clock;
        let ticks = (uptime * 100.0) as u64;
        Some(match path {
            "/proc/uptime" => format!("{uptime:.2} {:.2}\n", uptime * 3.1),
            "/proc/stat" => {
                let busy = ticks / 4;
                format!(
                    "cpu  {} 0 {} {} 0 0 0 0 0 0\ncpu0 {} 0 {} {} 0 0 0 0 0 0\n",
                    busy,
                    busy / 3,
                    ticks * 4 - busy,
                    busy / 4,
                    busy / 12,
                    ticks - busy / 4
                )
            }
            "/proc/net/dev" => {
                let rx = (uptime * 1_500_000.0) as u64;
                let tx = (uptime * 40_000.0) as u64;
                format!(
                    "Inter-|   Receive                                                |  Transmit\n face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed\n    lo:    1000      10    0    0    0     0          0         0     1000      10    0    0    0     0       0          0\n  eth0: {rx} {} 0 0 0 0 0 0 {tx} {} 0 0 0 0 0 0\n",
                    rx / 1400,
                    tx / 1400
                )
            }
            _ => return None,
        })
    }

    fn cat(&self, args: &[String], globbed: &[bool], clock: f64) -> Out {
        let mut out = Out::default();
        for (i, path) in args.iter().enumerate() {
            if globbed.get(i + 1).copied().unwrap_or(false) {
                let mut any = false;
                for (p, body) in &self.files {
                    if glob_match(path, p) {
                        out.stdout.push_str(body);
                        any = true;
                    }
                }
                if !any {
                    out.stderr
                        .push_str(&format!("cat: {path}: No such file or directory\n"));
                    out.code = 1;
                }
                continue;
            }
            if let Some(body) = self
                .proc_file(path, clock)
                .or_else(|| self.files.get(path).cloned())
            {
                out.stdout.push_str(&body);
            } else {
                out.stderr
                    .push_str(&format!("cat: {path}: No such file or directory\n"));
                out.code = 1;
            }
        }
        out
    }

    fn ls(&self, args: &[String]) -> Out {
        let path = args
            .iter()
            .rfind(|a| !a.starts_with('-'))
            .cloned()
            .unwrap_or_else(|| "/".into());
        let path = path.trim_end_matches('/');
        let mut out = String::from("total 24\n");
        let mut found = false;
        for p in self.files.keys() {
            let Some(rest) = p.strip_prefix(&format!("{path}/")) else {
                continue;
            };
            if rest.contains('/') {
                continue;
            }
            found = true;
            let size = self.files[p].len();
            out.push_str(&format!(
                "-rw-rw---- 1 u0_a10 media_rw {size} 2026-09-20 12:00 {rest}\n"
            ));
        }
        let mut dirs: Vec<&str> = self
            .files
            .keys()
            .filter_map(|p| p.strip_prefix(&format!("{path}/")))
            .filter_map(|rest| rest.split_once('/').map(|(d, _)| d))
            .collect();
        dirs.dedup();
        for d in dirs {
            found = true;
            out.push_str(&format!(
                "drwxrwx--x 2 u0_a10 media_rw 3452 2026-09-20 12:00 {d}\n"
            ));
        }
        if !found && !matches!(path, "/sdcard" | "/storage/emulated/0" | "") {
            return Out::err(format!("ls: {path}: No such file or directory\n"), 1);
        }
        Out::ok(out)
    }

    fn am(&mut self, args: &[String]) -> Option<Out> {
        let sub = args.first()?.as_str();
        Some(match sub {
            "start" => {
                let intent = args[1..].join(" ");
                self.started.push(intent.clone());
                let wait = args.iter().any(|a| a == "-W");
                let mut out = format!("Starting: Intent {{ {intent} }}\n");
                if wait {
                    out.push_str(
                        "Status: ok\nLaunchState: WARM\nTotalTime: 120\nWaitTime: 130\nComplete\n",
                    );
                }
                Out::ok(out)
            }
            "force-stop" | "kill" | "kill-all" => Out::ok(""),
            _ => return None,
        })
    }
}

fn apk_path(name: &str, p: &Package) -> String {
    if p.system {
        format!("/system/priv-app/{name}/{name}.apk")
    } else {
        format!("/data/app/~~sim/{name}-1/base.apk")
    }
}

fn unknown_package(verb: &str, pkg: &str) -> Out {
    Out {
        stdout: String::new(),
        stderr: format!(
            "Exception occurred while executing '{verb}':\njava.lang.IllegalArgumentException: Unknown package: {pkg}\n"
        ),
        code: 255,
    }
}

/// Shell execution context: a device plus the run's faults and gap log.
pub(crate) struct DeviceShell<'a> {
    pub dev: &'a mut Device,
    pub faults: &'a mut Vec<FaultRule>,
    pub gaps: &'a mut Vec<String>,
    pub stock: &'a [String],
    pub clock: &'a mut f64,
    pub timeout: Option<u64>,
    pub delay_ms: u64,
}

impl Exec for DeviceShell<'_> {
    fn aborted(&self) -> bool {
        self.timeout.is_some()
    }

    fn exec(&mut self, argv: &[String], globbed: &[bool], stdin: Option<&str>) -> Out {
        let text = argv.join(" ");
        let serial = self.dev.serial.clone();
        if let Some(effect) = super::faults::take(self.faults, FaultScope::Shell, &serial, &text) {
            match effect {
                FaultEffect::Fail {
                    stdout,
                    stderr,
                    exit_code,
                } => {
                    return Out {
                        stdout,
                        stderr,
                        code: exit_code,
                    }
                }
                FaultEffect::Ignore { stdout } => return Out::ok(stdout),
                FaultEffect::Output { stdout, exit_code } => {
                    return Out::with_code(stdout, exit_code)
                }
                FaultEffect::Timeout { delay_ms } => {
                    self.timeout = Some(delay_ms);
                    return Out::default();
                }
                FaultEffect::Delay { ms } => self.delay_ms += ms,
            }
        }
        let out = self.exec_inner(argv, globbed, stdin);
        match out {
            Some(o) => o,
            None => {
                self.gaps.push(text.clone());
                Out::err(
                    format!("/system/bin/sh: simulated device has no handler for: {text}\n"),
                    127,
                )
            }
        }
    }
}

impl DeviceShell<'_> {
    fn exec_inner(
        &mut self,
        argv: &[String],
        globbed: &[bool],
        stdin: Option<&str>,
    ) -> Option<Out> {
        let args = &argv[1..];
        let dev = &mut *self.dev;
        Some(match argv[0].as_str() {
            "true" | ":" => Out::ok(""),
            "false" => Out::with_code("", 1),
            "echo" => Out::ok(format!("{}\n", args.join(" "))),
            "printf" => Out::ok(printf(args.first().map(String::as_str).unwrap_or(""), &args[1.min(args.len())..])),
            "sleep" => {
                *self.clock += args.first().and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
                Out::ok("")
            }
            "getprop" => dev.getprop(args),
            "settings" => dev.settings(args),
            "pm" => return dev.pm(args, self.stock),
            "cmd" => match args.first().map(String::as_str) {
                Some("package") => return dev.pm(&args[1..], self.stock),
                Some("role") => return dev.role(&args[1..]),
                Some("appops") => return dev.appops(&args[1..]),
                _ => return None,
            },
            "appops" => return dev.appops(args),
            "dumpsys" => return dev.dumpsys(args, *self.clock),
            "wm" => return dev.wm(args),
            "am" => return dev.am(args),
            "monkey" => Out::ok("Events injected: 1\n"),
            "input" => {
                dev.input_log.push(args.join(" "));
                Out::ok("")
            }
            "top" => Out::ok(dev.texts.get("top -b -n 1")?.clone()),
            "df" => Out::ok(
                dev.texts
                    .get("df -h /data")
                    .cloned()
                    .unwrap_or_else(|| "Filesystem       Size Used Avail Use% Mounted on\n/dev/block/dm-5   11G 8.4G  2.4G  78% /data\n".into()),
            ),
            "cat" if !args.is_empty() => dev.cat(args, globbed, *self.clock),
            "cat" => Out::ok(stdin.unwrap_or("").to_string()),
            "ls" => dev.ls(args),
            "rm" => {
                for a in args.iter().filter(|a| !a.starts_with('-')) {
                    let prefix = format!("{a}/");
                    dev.files.retain(|p, _| p != a && !p.starts_with(&prefix));
                }
                Out::ok("")
            }
            "find" => {
                let pattern = args
                    .iter()
                    .position(|a| a == "-name")
                    .and_then(|i| args.get(i + 1))
                    .cloned()
                    .unwrap_or_else(|| "*".into());
                let roots: Vec<&String> = args.iter().take_while(|a| !a.starts_with('-')).collect();
                let hits: String = dev
                    .files
                    .keys()
                    .filter(|p| roots.iter().any(|r| p.starts_with(r.as_str())))
                    .filter(|p| glob_match(&pattern, p.rsplit('/').next().unwrap_or(p)))
                    .map(|p| format!("{p}\n"))
                    .collect();
                Out::ok(hits)
            }
            "head" | "tail" => {
                let n: usize = args
                    .iter()
                    .find_map(|a| a.trim_start_matches('-').trim_start_matches('n').parse().ok())
                    .unwrap_or(10);
                let lines: Vec<&str> = stdin.unwrap_or("").lines().collect();
                let pick: Vec<&str> = if argv[0] == "head" {
                    lines.into_iter().take(n).collect()
                } else {
                    let skip = lines.len().saturating_sub(n);
                    lines.into_iter().skip(skip).collect()
                };
                Out::ok(pick.iter().map(|l| format!("{l}\n")).collect::<String>())
            }
            "grep" => {
                let needle = args.iter().rfind(|a| !a.starts_with('-'))?.clone();
                let hits: String = stdin
                    .unwrap_or("")
                    .lines()
                    .filter(|l| l.contains(&needle))
                    .map(|l| format!("{l}\n"))
                    .collect();
                let code = i32::from(hits.is_empty());
                Out::with_code(hits, code)
            }
            "wc" => Out::ok(format!("{}\n", stdin.unwrap_or("").lines().count())),
            "stat" if args.first().map(String::as_str) == Some("-c") => Out::ok(
                args.iter()
                    .skip(2)
                    .map(|p| format!("{}\n", 4_000_000 + p.len() as u64 * 131_072))
                    .collect::<String>(),
            ),
            "id" => Out::ok("uid=2000(shell) gid=2000(shell)\n"),
            "reboot" => Out::ok(""),
            "screencap" => Out::ok(""),
            _ => return None,
        })
    }
}

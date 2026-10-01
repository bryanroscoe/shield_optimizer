//! Recorded sessions: the JSONL file the desktop app writes under
//! `<data_dir>/logs/sessions/` while Debug logging is on.
//!
//! Each line is one JSON object with a `kind`:
//! - `session` — header (`app_version`, `os`, `started`).
//! - `adb` — one adb invocation: `args`, `serial`, `ms`, `exit_code`,
//!   `stdout`, `stderr`, `error` (a driver error such as a timeout) and
//!   `stream` (`text`, `bytes` or `bounded` for the expert shell).
//! - `ui` — a breadcrumb from the frontend: `event` (`route`, `tab`,
//!   `click`) and `label`.
//! - `event` — an info-level summary line (pair, connect, forget, …).
//!
//! A session can be replayed (every command answered with what the device
//! said at the time, with divergences reported) or turned into a device
//! profile (the recorded reads folded back into a stateful [`Device`]).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::device::{Device, HomeComponent, Package};
use super::shell::Out;
use crate::adb::{BATCH_SEPARATOR, BATCH_STATUS};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SessionLine {
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub ts: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub serial: Option<String>,
    #[serde(default)]
    pub ms: Option<u64>,
    #[serde(default)]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub stdout: Option<String>,
    #[serde(default)]
    pub stderr: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub stream: Option<String>,
    #[serde(default)]
    pub event: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
}

pub fn parse_session(text: &str) -> Vec<SessionLine> {
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

/// Recorded args are redacted at write time (pairing PIN, typed text); an
/// incoming call is normalised the same way before it is compared.
pub fn normalise_args(args: &[String]) -> Vec<String> {
    let pair_at = args.iter().position(|a| a == "pair");
    args.iter()
        .enumerate()
        .map(|(i, a)| {
            if pair_at.is_some_and(|p| i > p + 1) {
                return "<redacted pin>".to_string();
            }
            let a = match a.find("input text") {
                Some(at) => format!("{}input text <redacted text>", &a[..at]),
                None => a.clone(),
            };
            ephemeral(&a)
        })
        .collect()
}

/// True when every device command in the call only reads state, so its
/// position relative to other reads does not matter.
fn is_read(args: &[String]) -> bool {
    const READS: &[&str] = &[
        "getprop",
        "pm list",
        "pm path",
        "pm has-feature",
        "dumpsys",
        "settings get",
        "settings list",
        "cmd package query-activities",
        "cmd package resolve-activity",
        "cmd role get-role-holders",
        "cmd appops get",
        "wm size",
        "wm density",
        "cat ",
        "top",
        "df",
        "ls",
        "stat",
        "find",
        "sleep",
        "echo",
        "printf",
        "true",
        ":",
        "exit",
    ];
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    let command = match words.as_slice() {
        ["-s", _, "shell", cmd] => *cmd,
        ["devices", ..] | ["mdns", ..] | ["version"] => return true,
        _ => return false,
    };
    let is_write_wm = |c: &str| {
        (c.starts_with("wm size ") || c.starts_with("wm density "))
            && c.split_whitespace().count() > 2
    };
    command
        .split([';', '|', '&', '(', ')'])
        .map(|c| c.trim().trim_end_matches("2>/dev/null").trim())
        .filter(|c| !c.is_empty())
        .all(|c| READS.iter().any(|r| c.starts_with(r)) && !is_write_wm(c))
}

/// The Remote's scrcpy session picks a fresh local port and session id every
/// time, so those values are matched by shape, not by value.
fn ephemeral(arg: &str) -> String {
    static PATTERNS: std::sync::LazyLock<[(regex::Regex, &str); 3]> =
        std::sync::LazyLock::new(|| {
            [
                (regex::Regex::new(r"^tcp:\d+$").unwrap(), "tcp:<port>"),
                (
                    regex::Regex::new(r"scrcpy_[0-9a-f]+").unwrap(),
                    "scrcpy_<scid>",
                ),
                (regex::Regex::new(r"scid=[0-9a-f]+").unwrap(), "scid=<scid>"),
            ]
        });
    PATTERNS.iter().fold(arg.to_string(), |acc, (re, with)| {
        re.replace_all(&acc, *with).into_owned()
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Divergence {
    /// The app sent a command the recording never saw.
    Unrecorded { seq: usize, args: Vec<String> },
    /// The command was recorded, but later than where the replay stood.
    OutOfOrder {
        seq: usize,
        args: Vec<String>,
        expected: Vec<String>,
    },
    /// Recorded commands the app never sent.
    NotReplayed { args: Vec<String> },
}

/// One recorded answer.
pub struct Recorded {
    pub out: Out,
    pub timed_out: bool,
    /// Binary stdout (a screenshot); only its size was recorded.
    pub bytes: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Replay {
    calls: Vec<SessionLine>,
    used: Vec<bool>,
    cursor: usize,
    seq: usize,
    pub divergences: Vec<Divergence>,
    /// Calls answered by re-using the last response for identical args
    /// (polling ran more often than in the recording) — expected, not a diff.
    pub repeats: usize,
    /// Calls answered a few places early (concurrent reads), not reported.
    pub reordered: usize,
}

/// How far ahead of the replay position a call may be matched before it
/// counts as out of order.
const REORDER_WINDOW: usize = 4;

/// Calls whose number legitimately varies with timing (pollers). They are
/// matched by args alone and never reported as out of order.
fn is_poll(args: &[String]) -> bool {
    matches!(
        args.iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .as_slice(),
        ["devices"] | ["devices", "-l"] | ["mdns", "services"] | ["version"]
    )
}

impl Replay {
    pub fn new(lines: &[SessionLine]) -> Self {
        let calls: Vec<SessionLine> = lines.iter().filter(|l| l.kind == "adb").cloned().collect();
        Self {
            used: vec![false; calls.len()],
            calls,
            ..Self::default()
        }
    }

    fn recorded(line: &SessionLine) -> Recorded {
        Recorded {
            out: Self::reply(line),
            timed_out: line
                .error
                .as_deref()
                .is_some_and(|e| e.contains("timed out")),
            bytes: line.stream.as_deref() == Some("bytes"),
        }
    }

    fn reply(line: &SessionLine) -> Out {
        Out {
            stdout: line.stdout.clone().unwrap_or_default(),
            stderr: line.stderr.clone().unwrap_or_default(),
            code: line
                .exit_code
                .unwrap_or(if line.error.is_some() { 1 } else { 0 }),
        }
    }

    /// The recorded answer for `args`, or `None` when it was never recorded.
    pub fn answer(&mut self, args: &[&str]) -> Option<Recorded> {
        self.seq += 1;
        let want = normalise_args(&args.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        let matches = |l: &SessionLine| normalise_args(&l.args) == want;
        if is_poll(&want) {
            // A poll answers from the current phase only: never with a
            // response recorded after the next state-changing call.
            let boundary = (self.cursor..self.calls.len())
                .find(|&j| !self.used[j] && !is_poll(&normalise_args(&self.calls[j].args)))
                .unwrap_or(self.calls.len());
            let found = (0..boundary).find(|&i| !self.used[i] && matches(&self.calls[i]));
            if let Some(i) = found {
                self.used[i] = true;
                return Some(Self::recorded(&self.calls[i]));
            }
            if let Some(i) = (0..boundary).rev().find(|&i| matches(&self.calls[i])) {
                self.repeats += 1;
                return Some(Self::recorded(&self.calls[i]));
            }
            // Nothing earlier at all: the session's first such poll.
            let i = (boundary..self.calls.len()).find(|&i| matches(&self.calls[i]))?;
            self.repeats += 1;
            return Some(Self::recorded(&self.calls[i]));
        }
        // Skip poll entries when deciding what "next" is.
        while self.cursor < self.calls.len()
            && (self.used[self.cursor] || is_poll(&normalise_args(&self.calls[self.cursor].args)))
        {
            self.cursor += 1;
        }
        if self.cursor < self.calls.len() && matches(&self.calls[self.cursor]) {
            let i = self.cursor;
            self.used[i] = true;
            self.cursor += 1;
            return Some(Self::recorded(&self.calls[i]));
        }
        if let Some(i) = (0..self.calls.len()).find(|&i| !self.used[i] && matches(&self.calls[i])) {
            // The UI fires independent reads concurrently, so a short swap
            // against the recording is scheduling, not a behaviour change.
            // Writes keep their order: swapping two mutations, or a read
            // and a mutation, can change what the device ends up in.
            let skipped: Vec<usize> = (self.cursor..i)
                .filter(|&j| !self.used[j] && !is_poll(&normalise_args(&self.calls[j].args)))
                .collect();
            let all_reads = is_read(&self.calls[i].args)
                && skipped.iter().all(|&j| is_read(&self.calls[j].args));
            if i > self.cursor && skipped.len() <= REORDER_WINDOW && all_reads {
                self.used[i] = true;
                self.reordered += 1;
                return Some(Self::recorded(&self.calls[i]));
            }
            self.divergences.push(Divergence::OutOfOrder {
                seq: self.seq,
                args: want,
                expected: self
                    .calls
                    .get(self.cursor)
                    .map(|l| l.args.clone())
                    .unwrap_or_default(),
            });
            self.used[i] = true;
            return Some(Self::recorded(&self.calls[i]));
        }
        if let Some(last) = self.calls.iter().rev().find(|l| matches(l)) {
            // An extra copy of a read is a refresh that ran once more; an
            // extra copy of a write (a duplicated disable, say) is a
            // behaviour change.
            if is_read(&last.args) {
                self.repeats += 1;
            } else {
                self.divergences.push(Divergence::Unrecorded {
                    seq: self.seq,
                    args: want,
                });
            }
            return Some(Self::recorded(last));
        }
        self.divergences.push(Divergence::Unrecorded {
            seq: self.seq,
            args: want,
        });
        None
    }

    /// Divergences so far plus every recorded non-poll call never replayed.
    pub fn report(&self) -> Vec<Divergence> {
        let mut out = self.divergences.clone();
        for (i, l) in self.calls.iter().enumerate() {
            if !self.used[i] && !is_poll(&normalise_args(&l.args)) {
                out.push(Divergence::NotReplayed {
                    args: l.args.clone(),
                });
            }
        }
        out
    }
}

/// Split a recorded batched shell command and its stdout back into
/// `(sub-command, section)` pairs. Handles both `batch_command` and
/// `checked_batch_command` shapes; anything else is one pair.
pub fn split_recorded(command: &str, stdout: &str) -> Vec<(String, String)> {
    let sep = format!("; echo {BATCH_SEPARATOR}; ");
    let command = command.trim_end_matches("; true");
    let parts: Vec<&str> = command.split(&sep).collect();
    let sections: Vec<&str> = stdout.split(BATCH_SEPARATOR).collect();
    parts
        .iter()
        .enumerate()
        .map(|(i, part)| {
            let cmd = part
                .trim()
                .trim_start_matches('(')
                .split(") 2>/dev/null")
                .next()
                .unwrap_or(part)
                .trim_end_matches(" 2>/dev/null")
                .to_string();
            let mut section = sections.get(i).copied().unwrap_or("").to_string();
            if let Some((body, _)) = section.rsplit_once(BATCH_STATUS) {
                section = body.to_string();
            }
            (cmd, section.trim_matches(['\n', '\r']).to_string())
        })
        .collect()
}

/// Build a device from what a session recorded the device saying. Only reads
/// feed it; anything never read stays at the simulator's generic default.
pub fn device_from_session(lines: &[SessionLine], serial_hint: Option<&str>) -> Option<Device> {
    let mut reads: Vec<(String, String)> = Vec::new();
    let mut key = serial_hint.map(str::to_string);
    for l in lines.iter().filter(|l| l.kind == "adb") {
        let args: Vec<&str> = l.args.iter().map(String::as_str).collect();
        let ["-s", k, "shell", cmd] = args.as_slice() else {
            continue;
        };
        if l.exit_code.unwrap_or(1) != 0 {
            continue;
        }
        if key.is_none() {
            key = Some(k.to_string());
        }
        if key.as_deref() != Some(*k) {
            continue;
        }
        reads.extend(split_recorded(cmd, l.stdout.as_deref().unwrap_or("")));
    }
    let serial = reads
        .iter()
        .find(|(c, _)| c == "getprop ro.serialno")
        .map(|(_, v)| v.trim().to_string())
        .filter(|s| !s.is_empty())
        .or(key)?;
    let mut d = Device::new(&serial);
    let mut lists: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (cmd, out) in &reads {
        let words: Vec<&str> = cmd.split_whitespace().collect();
        match words.as_slice() {
            ["getprop", k] => {
                d.props.insert(k.to_string(), out.trim().to_string());
            }
            ["settings", "get", ns, k] if out.trim() != "null" => {
                d.settings
                    .entry(ns.to_string())
                    .or_default()
                    .insert(k.to_string(), out.trim().to_string());
            }
            ["settings", "list", ns] => {
                for line in out.lines() {
                    if let Some((k, v)) = line.split_once('=') {
                        d.settings
                            .entry(ns.to_string())
                            .or_default()
                            .insert(k.into(), v.into());
                    }
                }
            }
            ["pm", "list", "packages", flags @ ..] if flags.iter().all(|f| f.starts_with('-')) => {
                let names = out
                    .lines()
                    .filter_map(|l| l.trim().strip_prefix("package:"))
                    .map(str::to_string)
                    .collect();
                lists.insert(flags.join(" "), names);
            }
            ["pm", "has-feature", "android.software.leanback"] => {
                d.leanback = match out.trim() {
                    "true" => Some(true),
                    "false" => Some(false),
                    _ => None,
                };
            }
            ["cmd", "package", "query-activities", "-a", ..] => {
                let mut class = None;
                for line in out.lines() {
                    let t = line.trim();
                    if let Some(n) = t.strip_prefix("name=") {
                        class.get_or_insert_with(|| n.to_string());
                    } else if let Some(p) = t.strip_prefix("packageName=") {
                        if let Some(c) = class.take() {
                            if !d.home.components.iter().any(|h| h.package == p) {
                                let priority = if c.ends_with("FallbackHome") {
                                    -1000
                                } else {
                                    0
                                };
                                d.home.components.push(HomeComponent {
                                    package: p.into(),
                                    class: c,
                                    priority,
                                });
                            }
                        }
                    }
                }
            }
            ["cmd", "package", "resolve-activity", ..] => {
                if d.home.preferred.is_none() {
                    d.home.preferred = out
                        .lines()
                        .map(str::trim)
                        .find(|l| l.contains('/') && !l.contains("ResolverActivity"))
                        .map(str::to_string);
                }
            }
            // Tweaks greps this down to one line, which is all the
            // simulator ever serves from it, so the line stands in for the dump.
            ["dumpsys", "activity", "settings", "|", "grep", "CUR_MAX_CACHED_PROCESSES"] => {
                d.texts
                    .entry("dumpsys activity settings".into())
                    .or_insert_with(|| format!("{out}\n"));
            }
            ["dumpsys", ..] | ["top", ..] | ["df", ..] => {
                d.texts.entry(cmd.clone()).or_insert_with(|| out.clone());
            }
            ["wm", "size"] | ["wm", "density"] => {
                d.texts.entry(cmd.clone()).or_insert_with(|| out.clone());
            }
            _ => {}
        }
    }
    let installed = lists.get("").cloned().unwrap_or_default();
    let disabled = lists.get("-d").cloned().unwrap_or_default();
    let third = lists.get("-3").cloned().unwrap_or_default();
    // `-u` also lists system apps uninstalled for the user, which
    // `install-existing` can bring back; the plain list says which are in use.
    let all = lists
        .get("-u")
        .cloned()
        .filter(|u| !u.is_empty())
        .unwrap_or_else(|| installed.clone());
    for name in &all {
        d.packages.push((
            name.clone(),
            Package {
                system: !third.contains(name),
                enabled: !disabled.contains(name),
                installed: installed.is_empty() || installed.contains(name),
            },
        ));
    }
    super::profile::synthesize(&mut d);
    Some(d)
}

/// Write a device back out in the capture layout, so a session-built profile
/// can be scrubbed and checked in next to the real captures.
pub fn write_profile(d: &Device, name: &str, dir: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let meta = serde_json::json!({
        "name": name,
        "serial": d.serial,
        "model": d.prop("ro.product.model"),
        "captured": chrono::Utc::now().format("%Y-%m-%d").to_string(),
        "leanback": d.leanback,
        "notes": "Built from a recorded session (e2e_server profile-from-session).",
        "home_components": d.home.components.iter().map(|c| c.short()).collect::<Vec<_>>(),
    });
    std::fs::write(
        dir.join("device.json"),
        serde_json::to_string_pretty(&meta)? + "\n",
    )?;
    let props: String = d
        .props
        .iter()
        .map(|(k, v)| format!("[{k}]: [{v}]\n"))
        .collect();
    std::fs::write(dir.join("getprop.txt"), props)?;
    let list = |f: &dyn Fn(&Package) -> bool| -> String {
        d.packages
            .iter()
            .filter(|(_, p)| f(p))
            .map(|(n, _)| format!("package:{n}\n"))
            .collect()
    };
    std::fs::write(dir.join("pm-list-packages.txt"), list(&|p| p.installed))?;
    std::fs::write(dir.join("pm-list-packages-u.txt"), list(&|_| true))?;
    std::fs::write(
        dir.join("pm-list-packages-d.txt"),
        list(&|p| p.installed && !p.enabled),
    )?;
    std::fs::write(
        dir.join("pm-list-packages-e.txt"),
        list(&|p| p.installed && p.enabled),
    )?;
    std::fs::write(
        dir.join("pm-list-packages-s.txt"),
        list(&|p| p.installed && p.system),
    )?;
    std::fs::write(
        dir.join("pm-list-packages-3.txt"),
        list(&|p| p.installed && !p.system),
    )?;
    for ns in super::device::NS {
        let body: String = d
            .settings
            .get(ns)
            .map(|t| t.iter().map(|(k, v)| format!("{k}={v}\n")).collect())
            .unwrap_or_default();
        std::fs::write(dir.join(format!("settings-{ns}.txt")), body)?;
    }
    let mut query = String::new();
    for (i, c) in d.home.components.iter().enumerate() {
        query.push_str(&format!(
            "  Activity #{i}:\n    priority={} preferredOrder=0 match=0x108000 specificIndex=-1 isDefault=true\n    ActivityInfo:\n      name={}\n      packageName={}\n",
            c.priority, c.class, c.package
        ));
    }
    std::fs::write(dir.join("home-query-activities.txt"), query)?;
    if let Some(p) = &d.home.preferred {
        std::fs::write(
            dir.join("home-resolve-activity.txt"),
            format!(
                "priority=0 preferredOrder=0 match=0x108000 specificIndex=-1 isDefault=true\n{p}\n"
            ),
        )?;
    }
    for (cmd, file) in [
        ("dumpsys meminfo", "dumpsys-meminfo.txt"),
        ("dumpsys diskstats", "dumpsys-diskstats.txt"),
        ("dumpsys activity settings", "dumpsys-activity-settings.txt"),
        ("top -b -n 1", "top.txt"),
        ("wm size", "wm-size.txt"),
        ("wm density", "wm-density.txt"),
    ] {
        if let Some(t) = d.texts.get(cmd) {
            std::fs::write(dir.join(file), t)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adb::{batch_command, checked_batch_command};

    fn line(args: &[&str], stdout: &str) -> SessionLine {
        SessionLine {
            kind: "adb".into(),
            args: args.iter().map(|s| s.to_string()).collect(),
            stdout: Some(stdout.into()),
            exit_code: Some(0),
            ..SessionLine::default()
        }
    }

    #[test]
    fn replays_in_order_and_reports_divergence() {
        let lines = vec![
            line(&["devices"], "List of devices attached\nA\tdevice\n"),
            line(&["-s", "A", "shell", "getprop ro.serialno"], "SER\n"),
            line(&["-s", "A", "shell", "getprop a"], "\n"),
            line(&["-s", "A", "shell", "getprop b"], "\n"),
            line(&["-s", "A", "shell", "getprop c"], "\n"),
            line(&["-s", "A", "shell", "getprop d"], "\n"),
            line(&["-s", "A", "shell", "pm enable x"], "ok\n"),
        ];
        let mut r = Replay::new(&lines);
        assert_eq!(
            r.answer(&["devices"]).unwrap().out.stdout,
            "List of devices attached\nA\tdevice\n"
        );
        assert_eq!(r.answer(&["devices"]).unwrap().out.code, 0);
        assert_eq!(r.repeats, 1);
        assert_eq!(
            r.answer(&["-s", "A", "shell", "pm enable x"])
                .unwrap()
                .out
                .stdout,
            "ok\n"
        );
        assert!(matches!(r.divergences[0], Divergence::OutOfOrder { .. }));
        assert!(r
            .answer(&["-s", "A", "shell", "pm disable-user y"])
            .is_none());
        let report = r.report();
        assert!(report
            .iter()
            .any(|d| matches!(d, Divergence::Unrecorded { .. })));
        assert!(report
            .iter()
            .any(|d| matches!(d, Divergence::NotReplayed { .. })));
    }

    #[test]
    fn remote_ports_and_scids_match_by_shape_and_extra_calls_diverge() {
        let lines = vec![
            line(
                &[
                    "-s",
                    "A",
                    "forward",
                    "tcp:40001",
                    "localabstract:scrcpy_1a2b3c4d",
                ],
                "",
            ),
            line(&["-s", "A", "shell", "pm disable-user --user 0 x"], "ok\n"),
        ];
        let mut r = Replay::new(&lines);
        assert!(r
            .answer(&[
                "-s",
                "A",
                "forward",
                "tcp:51515",
                "localabstract:scrcpy_0badf00d"
            ])
            .is_some());
        assert!(r.divergences.is_empty());
        assert!(r
            .answer(&["-s", "A", "shell", "pm disable-user --user 0 x"])
            .is_some());
        assert!(r.divergences.is_empty());
        assert!(r
            .answer(&["-s", "A", "shell", "pm disable-user --user 0 x"])
            .is_some());
        assert!(matches!(r.divergences[0], Divergence::Unrecorded { .. }));
    }

    #[test]
    fn polls_stay_in_their_phase_and_u_packages_survive() {
        let lines = vec![
            line(&["devices"], "A\tdevice\n"),
            line(&["disconnect", "A"], "disconnected A\n"),
            line(&["devices"], "\n"),
        ];
        let mut r = Replay::new(&lines);
        assert_eq!(r.answer(&["devices"]).unwrap().out.stdout, "A\tdevice\n");
        assert_eq!(r.answer(&["devices"]).unwrap().out.stdout, "A\tdevice\n");
        assert!(r.answer(&["disconnect", "A"]).is_some());
        assert_eq!(r.answer(&["devices"]).unwrap().out.stdout, "\n");

        let out = format!(
            "package:a\n{BATCH_STATUS}0\n{BATCH_SEPARATOR}\npackage:a\npackage:gone\n{BATCH_STATUS}0\n"
        );
        let cmd = checked_batch_command(&["pm list packages", "pm list packages -u"]);
        let d = device_from_session(&[line(&["-s", "K", "shell", &cmd], &out)], None).unwrap();
        assert!(!d.package("gone").unwrap().installed);
        assert!(d.package("a").unwrap().installed);
    }

    #[test]
    fn a_recorded_cached_limit_survives_a_generated_profile() {
        let cmd = checked_batch_command(&[
            "getprop ro.serialno",
            "dumpsys activity settings | grep CUR_MAX_CACHED_PROCESSES",
        ]);
        let out = format!(
            "SER\n{BATCH_STATUS}0\n{BATCH_SEPARATOR}\n  CUR_MAX_CACHED_PROCESSES=4\n{BATCH_STATUS}0\n"
        );
        let d = device_from_session(&[line(&["-s", "K", "shell", &cmd], &out)], None).unwrap();
        let dir = tempfile::tempdir().unwrap();
        write_profile(&d, "t", dir.path()).unwrap();
        let mut loaded = super::super::profile::load_profile(dir.path()).unwrap();
        let shown = loaded
            .dumpsys(&["activity".into(), "settings".into()], 0.0)
            .unwrap()
            .stdout;
        assert!(shown.contains("CUR_MAX_CACHED_PROCESSES=4"), "{shown}");

        let none = device_from_session(
            &[line(&["-s", "K", "shell", "getprop ro.serialno"], "SER\n")],
            None,
        )
        .unwrap();
        let dir = tempfile::tempdir().unwrap();
        write_profile(&none, "t", dir.path()).unwrap();
        assert!(!dir.path().join("dumpsys-activity-settings.txt").exists());
    }

    #[test]
    fn reads_may_swap_but_writes_keep_their_order() {
        let lines = vec![
            line(&["-s", "A", "shell", "dumpsys meminfo"], "m"),
            line(&["-s", "A", "shell", "getprop ro.serialno"], "S"),
            line(&["-s", "A", "shell", "pm disable-user --user 0 a"], "ok"),
            line(&["-s", "A", "shell", "pm enable b"], "ok"),
        ];
        let mut r = Replay::new(&lines);
        r.answer(&["-s", "A", "shell", "getprop ro.serialno"]);
        r.answer(&["-s", "A", "shell", "dumpsys meminfo"]);
        assert!(r.divergences.is_empty());
        assert_eq!(r.reordered, 1);
        r.answer(&["-s", "A", "shell", "pm enable b"]);
        assert!(matches!(r.divergences[0], Divergence::OutOfOrder { .. }));
    }

    #[test]
    fn pins_compare_redacted() {
        let lines = vec![line(
            &["pair", "1.2.3.4:5", "<redacted pin>"],
            "Successfully paired",
        )];
        let mut r = Replay::new(&lines);
        assert!(r.answer(&["pair", "1.2.3.4:5", "123456"]).is_some());
        assert!(r.divergences.is_empty());
    }

    #[test]
    fn batches_split_back_into_reads() {
        let cmd = format!(
            "{}; true",
            batch_command(&[
                "getprop ro.serialno",
                "pm has-feature android.software.leanback"
            ])
        );
        let pairs = split_recorded(&cmd, &format!("SER\n{BATCH_SEPARATOR}\ntrue\n"));
        assert_eq!(
            pairs[0],
            ("getprop ro.serialno".to_string(), "SER".to_string())
        );
        assert_eq!(pairs[1].1, "true");
        let cmd = checked_batch_command(&["pm list packages", "pm list packages -d"]);
        let out = format!("package:a\npackage:b\n{BATCH_STATUS}0\n{BATCH_SEPARATOR}\npackage:b\n{BATCH_STATUS}0\n");
        let pairs = split_recorded(&cmd, &out);
        assert_eq!(pairs[0].0, "pm list packages");
        assert_eq!(
            pairs[1],
            ("pm list packages -d".to_string(), "package:b".to_string())
        );
        let lines = vec![line(&["-s", "K", "shell", &cmd], &out)];
        let d = device_from_session(&lines, None).unwrap();
        assert_eq!(d.serial, "K");
        assert!(!d.package("b").unwrap().enabled);
    }
}

//! Local session recording: one JSON line per adb call, info summary and UI
//! breadcrumb, so a real testing session can be read back and replayed.
//!
//! Recording runs only while Debug logging is on, writes to
//! `<data_dir>/logs/sessions/<startup-timestamp>.jsonl`, and is best-effort:
//! a failed write is dropped, never surfaced to the adb call that caused it.
//!
//! Session files are local only. Package inventories are recorded in full —
//! the inventory redaction is a policy for the Report-a-bug bundle, and that
//! bundle reads only files directly in `logs/`, never this subdirectory.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{json, Map, Value};
use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber};
use tracing_subscriber::filter::{LevelFilter, Targets};
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::Layer;

pub const SESSION_DIR_NAME: &str = "sessions";
const SESSION_FILE_SUFFIX: &str = ".jsonl";
const MAX_FILE_BYTES: u64 = 20 * 1024 * 1024;
const KEEP_SESSION_FILES: usize = 10;
const SCHEMA_VERSION: u32 = 1;
const CAP_REACHED_MESSAGE: &str = "session file size cap reached; recording stopped";

static RECORDING: AtomicBool = AtomicBool::new(false);
static RECORDER: OnceLock<Recorder> = OnceLock::new();

/// Install the process-wide recorder under `logs_dir/sessions`. The file is
/// named after this moment but only created on the first recorded line.
pub fn install(logs_dir: &Path, recording: bool) {
    let _ = RECORDER.set(Recorder::new(
        logs_dir.join(SESSION_DIR_NAME),
        session_file_name(Utc::now()),
        MAX_FILE_BYTES,
    ));
    set_recording(recording);
}

/// Follows the Debug logging toggle.
pub fn set_recording(on: bool) {
    RECORDING.store(on, Ordering::Relaxed);
}

pub fn is_recording() -> bool {
    RECORDING.load(Ordering::Relaxed) && RECORDER.get().is_some()
}

pub fn now_ts() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// Write one already-redacted line. No-op while recording is off.
pub fn record(line: Value) {
    if !RECORDING.load(Ordering::Relaxed) {
        return;
    }
    if let Some(recorder) = RECORDER.get() {
        recorder.write(line);
    }
}

pub fn record_ui(event: &str, label: &str, path: Option<&str>) {
    if !is_recording() {
        return;
    }
    record(json!({
        "v": SCHEMA_VERSION,
        "kind": "ui",
        "ts": now_ts(),
        "event": event,
        "label": label,
        "path": path,
    }));
}

/// `2026-09-30T14-03-22Z.jsonl` — colons are not legal in Windows file names.
fn session_file_name(at: DateTime<Utc>) -> String {
    format!("{}{SESSION_FILE_SUFFIX}", at.format("%Y-%m-%dT%H-%M-%SZ"))
}

pub struct Recorder {
    dir: PathBuf,
    file_name: String,
    max_bytes: u64,
    state: Mutex<RecorderState>,
}

#[derive(Default)]
struct RecorderState {
    file: Option<File>,
    written: u64,
    stopped: bool,
}

impl Recorder {
    pub fn new(dir: PathBuf, file_name: String, max_bytes: u64) -> Self {
        Self {
            dir,
            file_name,
            max_bytes,
            state: Mutex::new(RecorderState::default()),
        }
    }

    pub fn path(&self) -> PathBuf {
        self.dir.join(&self.file_name)
    }

    fn open(&self) -> std::io::Result<File> {
        std::fs::create_dir_all(&self.dir)?;
        // Leave room for the file about to be created.
        prune_sessions(
            &self.dir,
            KEEP_SESSION_FILES.saturating_sub(1),
            &self.file_name,
        );
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.path())
    }

    pub fn write(&self, line: Value) {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        if state.stopped {
            return;
        }
        if state.file.is_none() {
            match self.open() {
                Ok(file) => {
                    state.written = file.metadata().map(|m| m.len()).unwrap_or(0);
                    state.file = Some(file);
                    let header = json!({
                        "v": SCHEMA_VERSION,
                        "kind": "session",
                        "ts": now_ts(),
                        "app_version": env!("CARGO_PKG_VERSION"),
                        "os": std::env::consts::OS,
                    });
                    if !append(&mut state, &header) {
                        return;
                    }
                }
                Err(_) => {
                    // Don't retry on every adb call against an unwritable dir.
                    state.stopped = true;
                    return;
                }
            }
        }
        let Ok(mut bytes) = serde_json::to_vec(&line) else {
            return;
        };
        bytes.push(b'\n');
        if state.written + bytes.len() as u64 > self.max_bytes {
            let notice = json!({
                "v": SCHEMA_VERSION,
                "kind": "event",
                "ts": now_ts(),
                "message": CAP_REACHED_MESSAGE,
            });
            append(&mut state, &notice);
            state.stopped = true;
            state.file = None;
            return;
        }
        write_bytes(&mut state, &bytes);
    }
}

fn append(state: &mut RecorderState, line: &Value) -> bool {
    match serde_json::to_vec(line) {
        Ok(mut bytes) => {
            bytes.push(b'\n');
            write_bytes(state, &bytes)
        }
        Err(_) => false,
    }
}

fn write_bytes(state: &mut RecorderState, bytes: &[u8]) -> bool {
    let Some(file) = state.file.as_mut() else {
        return false;
    };
    if file.write_all(bytes).is_err() {
        state.stopped = true;
        state.file = None;
        return false;
    }
    state.written += bytes.len() as u64;
    true
}

/// Delete all but the newest `keep` session files (names are UTC timestamps,
/// so they sort chronologically). `current` is never deleted.
fn prune_sessions(dir: &Path, keep: usize, current: &str) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut names: Vec<String> = entries
        .flatten()
        .filter(|e| e.path().is_file())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.ends_with(SESSION_FILE_SUFFIX) && n != current)
        .collect();
    names.sort();
    let excess = names.len().saturating_sub(keep);
    for name in &names[..excess] {
        let _ = std::fs::remove_file(dir.join(name));
    }
}

/// Forwards the INFO-and-above summary lines from the command modules to the
/// session file as `kind:"event"` lines, so a recording reads as "what the
/// user did, what adb said" in order.
pub fn event_layer<S>() -> impl Layer<S>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    SessionEventLayer.with_filter(
        Targets::new()
            .with_target("shield_optimizer_core::commands", LevelFilter::INFO)
            .with_target("shield_optimizer_v2_lib::commands", LevelFilter::INFO),
    )
}

struct SessionEventLayer;

impl<S: Subscriber> Layer<S> for SessionEventLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        if !is_recording() {
            return;
        }
        record(event_line(event));
    }
}

fn event_line(event: &Event<'_>) -> Value {
    let mut visitor = FieldVisitor::default();
    event.record(&mut visitor);
    let mut message = visitor.message.unwrap_or_default();
    for (key, value) in &visitor.fields {
        let text = match value {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        message.push_str(&format!(" {key}={text}"));
    }
    let meta = event.metadata();
    json!({
        "v": SCHEMA_VERSION,
        "kind": "event",
        "ts": now_ts(),
        "level": meta.level().as_str(),
        "target": meta.target(),
        "message": message,
        "fields": Value::Object(visitor.fields),
    })
}

#[derive(Default)]
struct FieldVisitor {
    message: Option<String>,
    fields: Map<String, Value>,
}

impl Visit for FieldVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        let text = format!("{value:?}");
        if field.name() == "message" {
            self.message = Some(text);
        } else {
            self.fields
                .insert(field.name().to_string(), Value::String(text));
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message = Some(value.to_string());
        } else {
            self.fields
                .insert(field.name().to_string(), Value::String(value.to_string()));
        }
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.fields.insert(field.name().to_string(), json!(value));
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.fields.insert(field.name().to_string(), json!(value));
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.fields.insert(field.name().to_string(), json!(value));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_lines(path: &Path) -> Vec<Value> {
        std::fs::read_to_string(path)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    #[test]
    fn file_names_are_filesystem_safe_utc_timestamps() {
        let at = DateTime::parse_from_rfc3339("2026-09-30T14:03:22.456Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(session_file_name(at), "2026-09-30T14-03-22Z.jsonl");
    }

    #[test]
    fn nothing_is_created_until_the_first_line_then_a_header_leads() {
        let dir = tempfile::tempdir().unwrap();
        let sessions = dir.path().join(SESSION_DIR_NAME);
        let recorder = Recorder::new(sessions.clone(), "s.jsonl".into(), MAX_FILE_BYTES);
        assert!(!sessions.exists());

        recorder.write(json!({"v": 1, "kind": "adb", "args": ["devices"]}));
        recorder.write(json!({"v": 1, "kind": "ui", "event": "route", "label": "/"}));

        let lines = read_lines(&recorder.path());
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0]["kind"], "session");
        assert_eq!(lines[0]["v"], 1);
        assert_eq!(lines[0]["app_version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(lines[0]["os"], std::env::consts::OS);
        assert!(lines[0]["ts"].as_str().unwrap().ends_with('Z'));
        assert_eq!(lines[1]["kind"], "adb");
        assert_eq!(lines[2]["event"], "route");
    }

    #[test]
    fn the_size_cap_writes_one_notice_and_stops() {
        let dir = tempfile::tempdir().unwrap();
        let recorder = Recorder::new(dir.path().to_path_buf(), "s.jsonl".into(), 1024);
        let big = "x".repeat(300);
        for _ in 0..20 {
            recorder.write(json!({"v": 1, "kind": "adb", "stdout": big}));
        }
        let lines = read_lines(&recorder.path());
        let last = lines.last().unwrap();
        assert_eq!(last["kind"], "event");
        assert_eq!(last["message"], CAP_REACHED_MESSAGE);
        assert_eq!(
            lines
                .iter()
                .filter(|l| l["message"] == CAP_REACHED_MESSAGE)
                .count(),
            1
        );
        // Header + two 300-byte lines fit under 1 KiB; the third does not.
        assert_eq!(lines.iter().filter(|l| l["kind"] == "adb").count(), 2);
        let len_after = std::fs::metadata(recorder.path()).unwrap().len();
        recorder.write(json!({"v": 1, "kind": "adb"}));
        assert_eq!(std::fs::metadata(recorder.path()).unwrap().len(), len_after);
    }

    #[test]
    fn rotation_keeps_the_newest_ten_files() {
        let dir = tempfile::tempdir().unwrap();
        for day in 1..=14 {
            let name = format!("2026-09-{day:02}T10-00-00Z.jsonl");
            std::fs::write(dir.path().join(name), "{}\n").unwrap();
        }
        std::fs::write(dir.path().join("notes.txt"), "keep me").unwrap();

        let recorder = Recorder::new(
            dir.path().to_path_buf(),
            "2026-09-30T10-00-00Z.jsonl".into(),
            MAX_FILE_BYTES,
        );
        recorder.write(json!({"v": 1, "kind": "event", "message": "hi"}));

        let mut sessions: Vec<String> = std::fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .map(|e| e.file_name().into_string().unwrap())
            .filter(|n| n.ends_with(".jsonl"))
            .collect();
        sessions.sort();
        assert_eq!(sessions.len(), 10);
        assert_eq!(sessions.first().unwrap(), "2026-09-06T10-00-00Z.jsonl");
        assert_eq!(sessions.last().unwrap(), "2026-09-30T10-00-00Z.jsonl");
        assert!(dir.path().join("notes.txt").exists());
    }

    #[test]
    fn an_unwritable_dir_is_swallowed() {
        let dir = tempfile::tempdir().unwrap();
        let blocker = dir.path().join("file");
        std::fs::write(&blocker, "").unwrap();
        let recorder = Recorder::new(blocker.join("sessions"), "s.jsonl".into(), MAX_FILE_BYTES);
        recorder.write(json!({"v": 1}));
        recorder.write(json!({"v": 1}));
    }
}

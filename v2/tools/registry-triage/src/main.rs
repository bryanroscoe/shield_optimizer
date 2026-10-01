//! Read a folder of user diagnostics and print the packages they mention, as a
//! candidate list for a person reviewing the app catalog.
//!
//! Reports are untrusted evidence. This tool only reads: it never writes the
//! catalog or any other file, and what it prints is a count of sightings, not a
//! verdict. Being reported often is not a reason to call an app Safe.
//!
//! See README.md.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use regex::Regex;
use serde::Serialize;
use serde_json::Value;
use shield_optimizer_core::commands::loader::{launchers, load_embedded_app_lists};
use shield_optimizer_core::engine::{classify_with_catalog, AppListBundle, CatalogVerdict, Safety};

/// Larger than any real export: the mobile collector caps itself at 64 KiB and
/// a desktop bundle is a page of Markdown plus a log tail.
const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_DEPTH: usize = 4;
const MAX_COUNT: u64 = 9_999;

const KINDS: &[&str] = &["installed_package", "unresolved_process"];
const REASONS: &[&str] = &[
    "uncatalogued_package",
    "unknown_safety_classification",
    "safety_lookup_unavailable",
    "process_not_resolved",
];
const FAMILIES: &[&str] = &["shield", "google_tv", "android_tv", "unknown"];

static PACKAGE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z][A-Za-z0-9_]*(?:\.[A-Za-z][A-Za-z0-9_]*)+$").unwrap());
static PROCESS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z0-9_][A-Za-z0-9_.:-]*$").unwrap());
static IPV4: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\d{1,3}(?:\.\d{1,3}){3}(?::\d+)?$").unwrap());
static MAC: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:[0-9A-Fa-f]{2}:){5}[0-9A-Fa-f]{2}$").unwrap());
static HOST_PORT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Za-z0-9.-]+:\d+$").unwrap());

/// One sighting, normalised from either export shape.
#[derive(Debug, Clone, PartialEq)]
struct Sighting {
    kind: String,
    token: String,
    reason: String,
    app_version: Option<String>,
    family: String,
    count: u64,
    first_seen: Option<DateTime<Utc>>,
    last_seen: Option<DateTime<Utc>>,
}

#[derive(Debug, Default)]
struct Batch {
    mobile_files: usize,
    desktop_files: usize,
    skipped_files: Vec<String>,
    rejected_records: usize,
    sightings: Vec<(PathBuf, Sighting)>,
}

/// Same rules as the mobile collector's `isValidDiagnosticToken`, so a token
/// that could carry an address or a path never reaches the output.
fn valid_token(kind: &str, token: &str) -> bool {
    if token.is_empty() || token.len() > 255 {
        return false;
    }
    if token
        .chars()
        .any(|c| c.is_control() || c.is_whitespace() || c == '/' || c == '\\')
        || token.contains("://")
        || IPV4.is_match(token)
        || MAC.is_match(token)
        || token.matches(':').count() > 1
        || HOST_PORT.is_match(token)
    {
        return false;
    }
    if kind == "installed_package" {
        PACKAGE.is_match(token)
    } else {
        PROCESS.is_match(token)
    }
}

fn valid_version(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 32
        && value.starts_with(|c: char| c.is_ascii_alphanumeric())
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '+' | '_' | '-'))
}

fn timestamp(value: Option<&Value>) -> Option<Option<DateTime<Utc>>> {
    let raw = value?.as_str()?;
    Some(Some(
        DateTime::parse_from_rfc3339(raw).ok()?.with_timezone(&Utc),
    ))
}

/// One record of the mobile `UnknownDiagnosticReport`. `None` means the record
/// is malformed and is counted as rejected rather than guessed at.
fn mobile_record(value: &Value) -> Option<Sighting> {
    let obj = value.as_object()?;
    let kind = obj.get("kind")?.as_str().filter(|k| KINDS.contains(k))?;
    let token = obj.get("token")?.as_str()?;
    if !valid_token(kind, token) {
        return None;
    }
    let reason = obj
        .get("reason")?
        .as_str()
        .filter(|r| REASONS.contains(r))?;
    if (kind == "unresolved_process") != (reason == "process_not_resolved") {
        return None;
    }
    let app_version = obj
        .get("app_version")?
        .as_str()
        .filter(|v| valid_version(v))?;
    let family = match obj.get("device_family") {
        None | Some(Value::Null) => "unknown",
        Some(v) => v.as_str().filter(|f| FAMILIES.contains(f))?,
    };
    let count = obj
        .get("count")?
        .as_u64()
        .filter(|c| (1..=MAX_COUNT).contains(c))?;
    Some(Sighting {
        kind: kind.to_string(),
        token: token.to_string(),
        reason: reason.to_string(),
        app_version: Some(app_version.to_string()),
        family: family.to_string(),
        count,
        first_seen: timestamp(obj.get("first_seen"))?,
        last_seen: timestamp(obj.get("last_seen"))?,
    })
}

/// Parse a mobile export. Returns `None` when the file is not one.
fn parse_mobile(text: &str) -> Option<(Vec<Sighting>, usize)> {
    let root: Value = serde_json::from_str(text).ok()?;
    if root.get("schema_version")?.as_u64()? != 1 {
        return None;
    }
    let records = root.get("records")?.as_array()?;
    let parsed: Vec<Sighting> = records.iter().filter_map(mobile_record).collect();
    let rejected = records.len() - parsed.len();
    Some((parsed, rejected))
}

/// Parse a desktop bug-report bundle (`engine::diagnostics::format_diagnostics`).
/// It deliberately carries no package inventory, so the only packages in it are
/// the HOME handlers. The serial and properties are never read.
fn parse_desktop(text: &str) -> Option<(Vec<Sighting>, usize)> {
    if !text.contains("## ATV Optimizer diagnostics") {
        return None;
    }
    let mut app_version = None;
    let mut family = "unknown";
    let mut in_handlers = false;
    let mut out = Vec::new();
    let mut rejected = 0;
    for line in text.lines() {
        let line = line.trim();
        if let Some(v) = line.strip_prefix("- App version: ") {
            app_version = Some(v.trim())
                .filter(|v| valid_version(v))
                .map(str::to_string);
        } else if let Some(v) = line.strip_prefix("- Detected type: ") {
            family = match v.trim() {
                "Nvidia Shield" => "shield",
                "Google TV" => "google_tv",
                _ => "unknown",
            };
        } else if line.starts_with('#') {
            in_handlers = line == "#### HOME handlers";
        } else if in_handlers {
            let Some(component) = line
                .strip_prefix("- `")
                .and_then(|rest| rest.strip_suffix('`'))
            else {
                continue;
            };
            let package = component.split('/').next().unwrap_or_default();
            if !valid_token("installed_package", package) {
                rejected += 1;
                continue;
            }
            out.push(Sighting {
                kind: "installed_package".into(),
                token: package.to_string(),
                reason: "home_handler".into(),
                app_version: None,
                family: family.to_string(),
                count: 1,
                first_seen: None,
                last_seen: None,
            });
        }
    }
    for s in &mut out {
        s.app_version.clone_from(&app_version);
    }
    Some((out, rejected))
}

fn collect_files(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) -> Result<()> {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .collect::<Result<_, _>>()?;
    entries.sort_by_key(|e| e.path());
    for entry in entries {
        // symlink_metadata: a link out of the report folder is not followed.
        let meta = fs::symlink_metadata(entry.path())?;
        if meta.is_dir() && depth < MAX_DEPTH {
            collect_files(&entry.path(), depth + 1, out)?;
        } else if meta.is_file() {
            out.push(entry.path());
        }
    }
    Ok(())
}

fn read_batch(dirs: &[PathBuf]) -> Result<Batch> {
    let mut files = Vec::new();
    for dir in dirs {
        if !dir.is_dir() {
            bail!("{} is not a folder", dir.display());
        }
        collect_files(dir, 0, &mut files)?;
    }
    // Overlapping arguments (a folder twice, or a folder and its child) must
    // not count one report twice.
    let files: BTreeSet<PathBuf> = files
        .into_iter()
        .map(|p| fs::canonicalize(&p).unwrap_or(p))
        .collect();
    let mut batch = Batch::default();
    for path in files {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if !matches!(ext.as_str(), "json" | "md" | "txt") {
            continue;
        }
        let size = fs::metadata(&path)?.len();
        let text = (size <= MAX_FILE_BYTES)
            .then(|| fs::read_to_string(&path).ok())
            .flatten();
        let parsed = text.as_deref().and_then(|t| {
            if ext == "json" {
                parse_mobile(t).map(|r| (true, r))
            } else {
                parse_desktop(t).map(|r| (false, r))
            }
        });
        match parsed {
            Some((mobile, (sightings, rejected))) => {
                if mobile {
                    batch.mobile_files += 1;
                } else {
                    batch.desktop_files += 1;
                }
                batch.rejected_records += rejected;
                batch
                    .sightings
                    .extend(sightings.into_iter().map(|s| (path.clone(), s)));
            }
            None => batch.skipped_files.push(path.display().to_string()),
        }
    }
    Ok(batch)
}

/// How the catalog the app ships would answer today. Printed so the reviewer
/// sees what a change would be a change *from*.
fn current_verdict(bundle: &AppListBundle, kind: &str, token: &str) -> String {
    if kind != "installed_package" {
        return "process (not a package)".into();
    }
    let entry = bundle.find(token);
    let verdict = classify_with_catalog(
        token,
        entry.map(|e| CatalogVerdict {
            risk: e.risk,
            description: &e.optimize_description,
        }),
    );
    let mut label = match verdict {
        Safety::NeverDisable { .. } => "protected".to_string(),
        Safety::Caution { .. } => "caution".to_string(),
        Safety::Safe { .. } => "safe".to_string(),
        Safety::Unknown { .. } => "unknown".to_string(),
    };
    if let Some(e) = entry {
        label.push_str(&format!(
            " (catalog, reviewed {})",
            e.reviewed_at.as_deref().unwrap_or("never")
        ));
    }
    let cat = launchers();
    if cat
        .custom
        .iter()
        .chain(cat.stock.iter())
        .any(|l| l.package == token)
    {
        label.push_str(" (launcher list)");
    }
    label
}

fn scope_name(family: &str) -> &str {
    match family {
        "google_tv" => "googletv",
        other => other,
    }
}

#[derive(Debug, Serialize, PartialEq)]
struct Candidate {
    kind: String,
    token: String,
    reports: usize,
    count: u64,
    device_scope: Vec<String>,
    reasons: Vec<String>,
    app_versions: Vec<String>,
    first_seen: Option<String>,
    last_seen: Option<String>,
    today: String,
}

fn candidates(batch: &Batch, bundle: &AppListBundle) -> Vec<Candidate> {
    #[derive(Default)]
    struct Acc {
        files: BTreeSet<PathBuf>,
        count: u64,
        scope: BTreeSet<String>,
        reasons: BTreeSet<String>,
        versions: BTreeSet<String>,
        first: Option<DateTime<Utc>>,
        last: Option<DateTime<Utc>>,
    }
    let mut acc: BTreeMap<(String, String), Acc> = BTreeMap::new();
    for (path, s) in &batch.sightings {
        let a = acc.entry((s.kind.clone(), s.token.clone())).or_default();
        a.files.insert(path.clone());
        a.count = a.count.saturating_add(s.count);
        a.scope.insert(scope_name(&s.family).to_string());
        a.reasons.insert(s.reason.clone());
        a.versions.extend(s.app_version.clone());
        a.first = a.first.into_iter().chain(s.first_seen).min();
        a.last = a.last.into_iter().chain(s.last_seen).max();
    }
    let mut out: Vec<Candidate> = acc
        .into_iter()
        .map(|((kind, token), a)| Candidate {
            today: current_verdict(bundle, &kind, &token),
            kind,
            token,
            reports: a.files.len(),
            count: a.count,
            device_scope: a.scope.into_iter().collect(),
            reasons: a.reasons.into_iter().collect(),
            app_versions: a.versions.into_iter().collect(),
            first_seen: a.first.map(|d| d.format("%Y-%m-%d").to_string()),
            last_seen: a.last.map(|d| d.format("%Y-%m-%d").to_string()),
        })
        .collect();
    out.sort_by(|a, b| {
        a.kind
            .cmp(&b.kind)
            .then(b.reports.cmp(&a.reports))
            .then(b.count.cmp(&a.count))
            .then(a.token.cmp(&b.token))
    });
    out
}

fn print_table(batch: &Batch, list: &[Candidate]) {
    println!(
        "Read {} mobile export(s) and {} desktop bundle(s); skipped {} file(s); rejected {} malformed record(s).",
        batch.mobile_files,
        batch.desktop_files,
        batch.skipped_files.len(),
        batch.rejected_records
    );
    for file in &batch.skipped_files {
        println!("  skipped: {file}");
    }
    println!();
    println!("Candidates for review only. Reports are untrusted; a sighting is not a verdict,");
    println!("and nothing here is a reason to mark an app Safe. This tool wrote nothing.");
    for (kind, title) in [
        ("installed_package", "Packages"),
        ("unresolved_process", "Unresolved processes"),
    ] {
        let rows: Vec<&Candidate> = list.iter().filter(|c| c.kind == kind).collect();
        println!();
        println!("{title} ({})", rows.len());
        if rows.is_empty() {
            continue;
        }
        println!(
            "  {:<48} {:>7} {:>6}  {:<24} {:<30} today",
            "token", "reports", "count", "device scope", "reasons"
        );
        for c in rows {
            println!(
                "  {:<48} {:>7} {:>6}  {:<24} {:<30} {}",
                c.token,
                c.reports,
                c.count,
                c.device_scope.join(", "),
                c.reasons.join(", "),
                c.today
            );
        }
    }
}

fn main() -> Result<()> {
    let mut json = false;
    let mut dirs = Vec::new();
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--json" => json = true,
            "-h" | "--help" => {
                println!("usage: registry-triage [--json] <report-folder>...");
                return Ok(());
            }
            _ if arg.starts_with('-') => bail!("unknown flag {arg}"),
            _ => dirs.push(PathBuf::from(arg)),
        }
    }
    if dirs.is_empty() {
        bail!("usage: registry-triage [--json] <report-folder>...");
    }
    let bundle = load_embedded_app_lists().map_err(anyhow::Error::msg)?;
    let batch = read_batch(&dirs)?;
    let list = candidates(&batch, &bundle);
    if json {
        println!("{}", serde_json::to_string_pretty(&list)?);
    } else {
        print_table(&batch, &list);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixtures() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
    }

    #[test]
    fn tokens_follow_the_mobile_collector_rules() {
        assert!(valid_token("installed_package", "com.example.app"));
        assert!(!valid_token("installed_package", "example"));
        assert!(!valid_token("installed_package", "192.168.1.20"));
        assert!(!valid_token("installed_package", "com.example/.Main"));
        assert!(!valid_token("unresolved_process", "tv.local:5555"));
        assert!(!valid_token("unresolved_process", "aa:bb:cc:dd:ee:ff"));
        assert!(valid_token("unresolved_process", "vendor.media.omx"));
        assert!(valid_token("unresolved_process", "com.example:remote"));
    }

    #[test]
    fn mobile_records_that_break_the_contract_are_rejected() {
        let (good, rejected) = parse_mobile(
            r#"{"schema_version":1,"generated_at":"2026-09-20T00:00:00Z","truncated":false,"records":[
              {"kind":"installed_package","token":"com.a.b","reason":"uncatalogued_package","app_version":"0.1.0","registry_version":null,"device_family":"shield","device_os":"11","first_seen":"2026-09-01T00:00:00Z","last_seen":"2026-09-02T00:00:00Z","count":2},
              {"kind":"installed_package","token":"com.a.c","reason":"process_not_resolved","app_version":"0.1.0","registry_version":null,"device_family":"shield","device_os":"11","first_seen":"2026-09-01T00:00:00Z","last_seen":"2026-09-02T00:00:00Z","count":1},
              {"kind":"installed_package","token":"com.a.d","reason":"uncatalogued_package","app_version":"0.1.0","registry_version":null,"device_family":"phone","device_os":"11","first_seen":"2026-09-01T00:00:00Z","last_seen":"2026-09-02T00:00:00Z","count":1},
              {"kind":"installed_package","token":"com.a.e","reason":"uncatalogued_package","app_version":"0.1.0","registry_version":null,"device_family":null,"device_os":null,"first_seen":"yesterday","last_seen":"2026-09-02T00:00:00Z","count":1},
              {"kind":"installed_package","token":"com.a.f","reason":"uncatalogued_package","app_version":"0.1.0","registry_version":null,"device_family":null,"device_os":null,"first_seen":"2026-09-01T00:00:00Z","last_seen":"2026-09-02T00:00:00Z","count":0}
            ]}"#,
        )
        .expect("a mobile export");
        assert_eq!(rejected, 4);
        assert_eq!(good.len(), 1);
        assert_eq!(good[0].token, "com.a.b");
        assert_eq!(good[0].count, 2);
        assert!(parse_mobile(r#"{"schema_version":2,"records":[]}"#).is_none());
    }

    #[test]
    fn desktop_bundles_yield_home_handlers_and_never_the_serial() {
        let text = fs::read_to_string(fixtures().join("reports/desktop-bundle.md")).unwrap();
        let (sightings, rejected) = parse_desktop(&text).expect("a desktop bundle");
        assert_eq!(rejected, 0);
        let tokens: Vec<&str> = sightings.iter().map(|s| s.token.as_str()).collect();
        assert_eq!(
            tokens,
            [
                "com.google.android.apps.tv.launcherx",
                "com.example.homescreen"
            ]
        );
        assert!(sightings.iter().all(|s| s.family == "google_tv"));
        assert!(sightings
            .iter()
            .all(|s| s.app_version.as_deref() == Some("2.3.0")));
        assert!(!format!("{sightings:?}").contains("SERIAL123"));
    }

    #[test]
    fn a_batch_aggregates_by_package_and_reports_today_without_promoting() {
        let bundle = load_embedded_app_lists().unwrap();
        let batch = read_batch(&[fixtures().join("reports")]).unwrap();
        assert_eq!(batch.mobile_files, 2);
        assert_eq!(batch.desktop_files, 1);
        assert_eq!(batch.skipped_files.len(), 1, "{:?}", batch.skipped_files);
        let list = candidates(&batch, &bundle);
        let seen = list
            .iter()
            .find(|c| c.token == "com.example.unknown")
            .expect("aggregated");
        assert_eq!(seen.reports, 2);
        assert_eq!(seen.count, 5);
        assert_eq!(seen.device_scope, ["googletv", "shield"]);
        assert_eq!(seen.first_seen.as_deref(), Some("2026-09-01"));
        assert_eq!(seen.last_seen.as_deref(), Some("2026-09-20"));
        // Reported often, still unknown: sightings never become a verdict.
        assert_eq!(seen.today, "unknown");
        let protected = list
            .iter()
            .find(|c| c.token == "com.android.systemui")
            .expect("protected package listed");
        assert!(protected.today.starts_with("protected"));
        let catalogued = list
            .iter()
            .find(|c| c.token == "com.netflix.ninja")
            .expect("catalogued package listed");
        assert!(catalogued.today.contains("catalog, reviewed 20"));
        let process = list
            .iter()
            .find(|c| c.kind == "unresolved_process")
            .expect("process listed");
        assert_eq!(process.today, "process (not a package)");
    }

    #[test]
    fn overlapping_folders_count_each_report_once() {
        let bundle = load_embedded_app_lists().unwrap();
        let reports = fixtures().join("reports");
        let once = candidates(
            &read_batch(std::slice::from_ref(&reports)).unwrap(),
            &bundle,
        );
        let overlapping = read_batch(&[reports.clone(), reports.join("nested"), reports]).unwrap();
        assert_eq!(overlapping.mobile_files, 2);
        assert_eq!(candidates(&overlapping, &bundle), once);
    }

    #[test]
    fn triage_leaves_the_report_folder_untouched() {
        let dir = fixtures().join("reports");
        let snapshot = |d: &Path| {
            let mut files = Vec::new();
            collect_files(d, 0, &mut files).unwrap();
            files
                .into_iter()
                .map(|p| (p.clone(), fs::read(&p).unwrap()))
                .collect::<Vec<_>>()
        };
        let before = snapshot(&dir);
        let bundle = load_embedded_app_lists().unwrap();
        let _ = candidates(&read_batch(std::slice::from_ref(&dir)).unwrap(), &bundle);
        assert_eq!(before, snapshot(&dir));
    }
}

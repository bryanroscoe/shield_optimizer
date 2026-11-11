//! Redacting the app's own log so part of it can go to the developer.
//!
//! Pure: the host reads the log file and hands the text in. Two uses:
//!
//! - [`connection_excerpt`]: the last lines from the adb, connect and
//!   discovery code, attached to an automatic failure or "found nothing"
//!   report on desktop. At most [`EXCERPT_MAX_LINES`] lines and
//!   [`EXCERPT_MAX_BYTES`] bytes after redaction.
//! - [`upload_log`]: the recent log, every target, when the user presses
//!   Send logs. At most [`UPLOAD_LOG_MAX_BYTES`] after redaction.
//!
//! Every line goes through the same scrubbing as report messages
//! ([`super::telemetry::redact_message`]: addresses, MACs, mDNS names,
//! `.local` hosts, paths, emails, package ids, serial-shaped tokens), after
//! the identifiers the caller already knows (serials, hardware ids, TV names,
//! the computer's user name) are removed by exact match, and the values of
//! identity-bearing fields and system properties are dropped whole. The
//! timestamp, level and module path at the front of a line are kept as they
//! are, since they carry no user data and the generic rules would mangle them.

use std::sync::LazyLock;

use regex::Regex;

use super::telemetry::redact_unbounded;
use super::telemetry_diag::classify_error_text;

pub const EXCERPT_MAX_LINES: usize = 60;
pub const EXCERPT_MAX_BYTES: usize = 8 * 1024;
pub const UPLOAD_LOG_MAX_BYTES: usize = 256 * 1024;
const EXCERPT_LINE_CHARS: usize = 300;
const UPLOAD_LINE_CHARS: usize = 2_000;
/// Shorter "known" strings are too likely to be ordinary words.
const MIN_KNOWN_CHARS: usize = 4;

/// What the caller knows identifies this user or their TVs.
#[derive(Debug, Default, Clone, Copy)]
pub struct Scrub<'a> {
    /// Removed wherever they appear: transport serials, hardware ids, TV
    /// names, the home directory.
    pub known: &'a [&'a str],
    /// Removed as whole words, case-insensitively: the user name, which is
    /// too likely to be part of other words to remove as a substring.
    pub words: &'a [&'a str],
}

use super::telemetry::re;

/// `2026-11-02T14:03:22.123456Z  INFO atv_optimizer_core::adb::driver: `
static HEADER: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"^(\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{1,9})?Z)\s+(TRACE|DEBUG|INFO|WARN|ERROR)\s+(?:([a-z0-9_]+(?:::[a-z0-9_]+)*):\s)?(.*)$",
    )
});

/// Fields whose value names a person, a TV, an address or a file, whatever
/// it looks like: `address=…`, `serial="…"`, `name=Living Room TV`. An
/// unquoted value runs to the next `key=` or the end of the line, so a
/// name or path with spaces in it goes whole.
static SENSITIVE_FIELD: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"(?i)\b(?:serial|serials|target|address|addr|host|hostname|ip|subnet|peer|instance|name|device_name|friendly_name|ssid|bssid|user|username|account|email|path|adb|dir|file|home)=",
    )
});
static NEXT_FIELD: LazyLock<Regex> = LazyLock::new(|| re(r"\s+[A-Za-z_][A-Za-z0-9_.]*="));

/// A `getprop` dump line for a property that holds a name, an address or
/// an identifier: `[persist.sys.device_name]: [Living Room]`.
static SENSITIVE_PROP: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"(?i)(\[[a-z0-9_.]*(?:name|serial|host|mac|ssid|account|user|email|imei|meid|iccid|phone|wifi|bt|bluetooth|addr|ip|dns|location|timezone|country|locale|fingerprint)[a-z0-9_.]*\]:\s*\[)[^\]\n]*(\])",
    )
});

/// Where the log itself names a transport: an `adb devices` row
/// (`<serial>\tunauthorized`), an `-s <serial>` argument, a `serial=` field
/// and a `[ro.serialno]: [...]` dump line.
static DEVICES_ROW: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"(?m)^\s*(\S+)\s+(?:device|unauthorized|offline|authorizing|connecting|recovery|rescue|sideload|bootloader|no permissions)\b",
    )
});
static DASH_S: LazyLock<Regex> = LazyLock::new(|| re(r#""-s",\s*"([^"]+)""#));
static SERIAL_FIELD: LazyLock<Regex> =
    LazyLock::new(|| re(r#"\b(?:serial|serials|target)=(?:"([^"]+)"|([^\s"]+))"#));
static SERIALNO_PROP: LazyLock<Regex> =
    LazyLock::new(|| re(r"\[ro(?:\.boot)?\.serialno\]:\s*\[([^\]\n]+)\]"));

/// Transport serials the text itself names, from any session. The session's
/// own list only knows the TVs it saw; an older log in the same upload can
/// name one that is gone, and a serial made only of letters looks like a
/// word to the generic rules.
pub fn identifiers_in(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut add = |v: &str| {
        let v = v.trim();
        if v.chars().count() >= MIN_KNOWN_CHARS
            && !v.eq_ignore_ascii_case("list")
            && !v.starts_with('<')
            && !out.iter().any(|o| o == v)
        {
            out.push(v.to_string());
        }
    };
    for c in DEVICES_ROW.captures_iter(text) {
        add(&c[1]);
    }
    for c in DASH_S.captures_iter(text) {
        add(&c[1]);
    }
    for c in SERIAL_FIELD.captures_iter(text) {
        if let Some(m) = c.get(1).or_else(|| c.get(2)) {
            add(m.as_str());
        }
    }
    for c in SERIALNO_PROP.captures_iter(text) {
        add(&c[1]);
    }
    out
}

/// `scrub` plus the identifiers `text` names itself.
fn with_found<T>(text: &str, scrub: &Scrub, f: impl FnOnce(&Scrub) -> T) -> T {
    let found = identifiers_in(text);
    let mut known: Vec<&str> = scrub.known.to_vec();
    known.extend(found.iter().map(String::as_str));
    f(&Scrub {
        known: &known,
        words: scrub.words,
    })
}

static CONTROL: LazyLock<Regex> = LazyLock::new(|| re(r"[\x00-\x08\x0b-\x1f\x7f]"));

/// Log targets that belong to adb, connecting and discovery. Logs copied
/// over from before the rename carry the desktop crate's old name.
fn is_connection_target(target: &str) -> bool {
    target == "atv_optimizer_lib"
        || target == "shield_optimizer_v2_lib"
        || target.contains("::adb")
        || [
            "::scan",
            "::devices",
            "::install",
            "::quit",
            "::wireless",
            "wireless_adb",
        ]
        .iter()
        .any(|t| target.contains(t))
}

fn replace_known(mut s: String, scrub: &Scrub) -> String {
    let mut known: Vec<&str> = scrub
        .known
        .iter()
        .map(|k| k.trim())
        .filter(|k| k.chars().count() >= MIN_KNOWN_CHARS)
        .collect();
    // Longest first, so a serial inside a longer mDNS name goes with it.
    known.sort_by_key(|k| std::cmp::Reverse(k.len()));
    for k in known {
        s = s.replace(k, "<id>");
    }
    for word in scrub.words {
        let word = word.trim();
        if word.chars().count() < MIN_KNOWN_CHARS {
            continue;
        }
        if let Ok(pattern) = Regex::new(&format!(r"(?i)\b{}\b", regex::escape(word))) {
            s = pattern.replace_all(&s, "<user>").into_owned();
        }
    }
    s
}

fn drop_sensitive_fields(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut at = 0;
    for key in SENSITIVE_FIELD.find_iter(s) {
        if key.start() < at {
            continue;
        }
        out.push_str(&s[at..key.end()]);
        out.push_str("<redacted>");
        let rest = &s[key.end()..];
        let len = match rest.strip_prefix('"') {
            Some(quoted) => quoted.find('"').map_or(rest.len(), |end| end + 2),
            None => NEXT_FIELD
                .find(rest)
                .map_or(rest.len(), |next| next.start()),
        };
        at = key.end() + len;
    }
    out.push_str(&s[at.min(s.len())..]);
    out
}

/// The value after a setting that holds something the user named: the TV's
/// name or the Private DNS host. `settings put global device_name 'Kids
/// Room TV'`, `device_name = "Den"`, `[persist.sys.device_name]: [Den]`.
static NAMED_VALUE: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r#"(?i)\b(device_name|private_dns_specifier|bluetooth_name)\b(\s*\]?:\s*\[?|\s*=\s*|\s+)("(?:[^"\\\n]|\\.)*"|(?:[^"\\;\],\n]|\\.)+)"#,
    )
});

/// Where a log line starts carrying a command's raw output.
static OUTPUT_FIELD: LazyLock<Regex> = LazyLock::new(|| re(r"(^|\s)(stderr|stdout)="));

/// The line with everything from its first `stderr=`/`stdout=` replaced:
/// stderr by the code of what it said, stdout by `<omitted>`. `None` when
/// the line has neither.
fn without_output_fields(line: &str) -> Option<String> {
    let first = OUTPUT_FIELD.find(line)?;
    let mut out = line[..first.start()].to_string();
    let rest = &line[first.start()..];
    let stdout_at = rest.find("stdout=");
    if let Some(at) = rest.find("stderr=") {
        let value_end = stdout_at.filter(|s| *s > at).unwrap_or(rest.len());
        let value = rest[at + "stderr=".len()..value_end].trim();
        let code = if value.is_empty() {
            "none"
        } else {
            classify_error_text(value)
        };
        out.push_str(&format!(" stderr=<{code}>"));
    }
    if stdout_at.is_some() {
        out.push_str(" stdout=<omitted>");
    }
    Some(out)
}

/// Command output never leaves the computer: device reads print TV names,
/// DNS hosts and whatever else the TV holds, in shapes no rule can know. A
/// line's `stdout=`/`stderr=` value and the continuation lines it spills
/// onto are dropped (stderr is kept as an error code).
fn without_command_output(text: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut dropping = false;
    for line in text.lines() {
        let header = HEADER.is_match(line);
        if dropping && !header {
            continue;
        }
        dropping = false;
        match without_output_fields(line) {
            Some(cut) => {
                out.push(cut);
                dropping = true;
            }
            None => out.push(line.to_string()),
        }
    }
    out.join("\n")
}

fn redact_body(body: &str, scrub: &Scrub) -> String {
    let s = replace_known(body.to_string(), scrub);
    let s = NAMED_VALUE.replace_all(&s, "${1}${2}<redacted>");
    let s = SENSITIVE_PROP.replace_all(&s, "${1}<redacted>${2}");
    let s = drop_sensitive_fields(&s);
    redact_unbounded(&s)
}

/// Redact one log line, keeping a leading timestamp, level and module path,
/// and cap it at `max_chars`.
pub fn redact_log_line(line: &str, scrub: &Scrub, max_chars: usize) -> String {
    let line = CONTROL.replace_all(line, " ");
    let out = match HEADER.captures(&line) {
        Some(c) => {
            let body = redact_body(c.get(4).map_or("", |m| m.as_str()), scrub);
            match c.get(3) {
                Some(target) => format!("{} {} {}: {}", &c[1], &c[2], target.as_str(), body),
                None => format!("{} {} {}", &c[1], &c[2], body),
            }
        }
        None => redact_body(&line, scrub),
    };
    let out = out.trim_end();
    if out.chars().count() > max_chars {
        let mut cut: String = out.chars().take(max_chars.saturating_sub(1)).collect();
        cut.push('…');
        cut
    } else {
        out.to_string()
    }
}

/// One log event: a header line and the continuation lines a multi-line
/// value (an adb command's stdout) spills onto.
struct Record<'a> {
    target: Option<&'a str>,
    lines: Vec<&'a str>,
}

fn records(log: &str) -> Vec<Record<'_>> {
    let mut out: Vec<Record> = Vec::new();
    for line in log.lines() {
        match HEADER.captures(line) {
            Some(c) => out.push(Record {
                target: c.get(3).map(|m| m.as_str()),
                lines: vec![line],
            }),
            // Text before the first header is the tail of a record that
            // started before the window the caller read; it has no target.
            None => match out.last_mut() {
                Some(record) => record.lines.push(line),
                None => out.push(Record {
                    target: None,
                    lines: vec![line],
                }),
            },
        }
    }
    out
}

/// Keep the newest lines whose total size, newlines included, fits `max_bytes`.
fn newest_fitting(lines: Vec<String>, max_bytes: usize) -> Vec<String> {
    let mut total = 0;
    let mut keep = Vec::new();
    for line in lines.into_iter().rev() {
        let size = line.len() + 1;
        if total + size > max_bytes {
            break;
        }
        total += size;
        keep.push(line);
    }
    keep.reverse();
    keep
}

/// The redacted tail of the adb, connect and discovery log lines, or `None`
/// when there are none.
pub fn connection_excerpt(log: &str, scrub: &Scrub) -> Option<String> {
    with_found(log, scrub, |scrub| excerpt_of(log, scrub))
}

fn excerpt_of(log: &str, scrub: &Scrub) -> Option<String> {
    let log = without_command_output(log);
    let lines: Vec<&str> = records(&log)
        .into_iter()
        .filter(|r| r.target.is_some_and(is_connection_target))
        .flat_map(|r| r.lines)
        .filter(|l| !l.trim().is_empty())
        .collect();
    let start = lines.len().saturating_sub(EXCERPT_MAX_LINES);
    let redacted: Vec<String> = lines[start..]
        .iter()
        .map(|l| redact_log_line(l, scrub, EXCERPT_LINE_CHARS))
        .collect();
    let kept = newest_fitting(redacted, EXCERPT_MAX_BYTES);
    (!kept.is_empty()).then(|| kept.join("\n"))
}

/// Every line of `text`, redacted, newest kept when it is over `max_bytes`.
/// Used for the log in a Send logs upload and for the summary above it.
pub fn redact_text(text: &str, scrub: &Scrub, max_bytes: usize) -> String {
    with_found(text, scrub, |scrub| {
        let text = without_command_output(text);
        let redacted: Vec<String> = text
            .lines()
            .map(|l| redact_log_line(l, scrub, UPLOAD_LINE_CHARS))
            .collect();
        newest_fitting(redacted, max_bytes).join("\n")
    })
}

/// The recent log for a Send logs upload.
pub fn upload_log(log: &str, scrub: &Scrub) -> String {
    redact_text(log, scrub, UPLOAD_LOG_MAX_BYTES)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    const SERIAL_NO_DIGITS: &str = "QWERTYUIOPAS";

    fn scrub() -> Scrub<'static> {
        Scrub {
            known: &[
                SERIAL_NO_DIGITS,
                "Bryan's Living Room",
                "/Users/bryanr",
                "ab",
            ],
            words: &["bryanr"],
        }
    }

    /// A realistic slice of the desktop log, with every kind of identifier
    /// the app has been seen to write.
    const LOG: &str = "\
2026-11-02T14:00:00.000001Z  INFO atv_optimizer_lib: adb located adb=/Users/bryanr/Library/Application Support/ATVOptimizer/platform-tools/adb
2026-11-02T14:00:01.000002Z DEBUG atv_optimizer_lib::adb::driver: adb invoke adb=\"C:\\Users\\Jane Doe\\AppData\\Local\\adb.exe\" args=[\"connect\", \"192.168.42.211:5555\"]
2026-11-02T14:00:01.500000Z DEBUG atv_optimizer_lib::adb::driver: adb done args=[\"devices\"] ms=13 exit_code=Some(0) stderr= stdout=List of devices attached
192.168.42.211:5555\tunauthorized
adb-1324619053514-jBeCEe._adb-tls-connect._tcp\tdevice
QWERTYUIOPAS\toffline
[fe80::1c2:3ff:fe4d:5e6f%en0]:5555\toffline
2026-11-02T14:00:02.000000Z  INFO atv_optimizer_core::commands::devices: connect finished address=10.0.0.5:5555 ok=false error=failed to connect to 'living-room-tv.local:5555': Connection refused
2026-11-02T14:00:02.100000Z DEBUG atv_optimizer_core::adb::driver: adb done stdout=[persist.sys.device_name]: [Bryan's Living Room]
[ro.serialno]: [1324619053514]
[ro.product.model]: [SHIELD Android TV]
[ro.boot.wifimacaddr]: [3c:a6:f6:12:ab:cd]
2026-11-02T14:00:02.200000Z  WARN atv_optimizer_core::commands::launcher: launcher read failed for com.example.secretapp
2026-11-02T14:00:03.000000Z  INFO atv_optimizer_lib::commands::scan: scan finished subnet=192.168.42 found=0 connected=0
2026-11-02T14:00:03.100000Z  INFO atv_optimizer_core::commands::devices: device name=Bryan's Living Room host=Bryans-MacBook.local email bryan@example.com user=bryanr
2026-11-02T14:00:04.000000Z  INFO atv_optimizer_core::telemetry: unrelated line from bryanr
";

    const LEAKS: [&str; 19] = [
        "192.168",
        "10.0.0.5",
        "fe80",
        "1324619053514",
        "jBeCEe",
        SERIAL_NO_DIGITS,
        "Living Room",
        "living-room-tv",
        "Bryans-MacBook",
        "bryan@example.com",
        "bryanr",
        "Jane",
        "AppData",
        "Library",
        "3c:a6:f6",
        "com.example",
        "secretapp",
        "/Users",
        "C:\\",
    ];

    fn assert_no_leaks(text: &str) {
        for leak in LEAKS {
            assert!(!text.contains(leak), "{leak} leaked into:\n{text}");
        }
    }

    #[test]
    fn lines_from_before_the_rename_still_count_as_connection_lines() {
        let log = "2026-11-02T14:00:00.000001Z  INFO shield_optimizer_v2_lib: adb located\n\
2026-11-02T14:00:01.000000Z  INFO shield_optimizer_core::commands::devices: connect finished\n\
2026-11-02T14:00:02.000000Z  INFO shield_optimizer_core::telemetry: unrelated\n";
        let out = connection_excerpt(log, &scrub()).expect("excerpt");
        assert!(out.contains("adb located"), "{out}");
        assert!(out.contains("connect finished"), "{out}");
        assert!(!out.contains("unrelated"), "{out}");
    }

    #[test]
    fn the_excerpt_is_connection_lines_only_and_scrubbed() {
        let excerpt = connection_excerpt(LOG, &scrub()).expect("excerpt");
        assert_no_leaks(&excerpt);
        assert!(!excerpt.contains("telemetry: unrelated"), "{excerpt}");
        assert!(!excerpt.contains("launcher read failed"), "{excerpt}");
        // What makes it useful survives.
        for keep in [
            "2026-11-02T14:00:02.000000Z INFO atv_optimizer_core::commands::devices: connect finished",
            "Connection refused",
            "stderr=<none> stdout=<omitted>",
            "ms=13 exit_code=Some(0)",
            "scan finished subnet=<redacted>",
            "found=0 connected=0",
        ] {
            assert!(excerpt.contains(keep), "missing {keep:?} in:\n{excerpt}");
        }
    }

    #[test]
    fn the_upload_keeps_every_target_and_is_scrubbed() {
        let text = upload_log(LOG, &scrub());
        assert_no_leaks(&text);
        assert!(text.contains("launcher read failed for <pkg>"), "{text}");
        assert!(text.contains("unrelated line from <user>"), "{text}");
    }

    #[test]
    fn excerpts_are_bounded_by_lines_and_bytes() {
        let mut log = String::new();
        for i in 0..500 {
            log.push_str(&format!(
                "2026-11-02T14:00:00.{i:06}Z DEBUG atv_optimizer_lib::adb::driver: adb done ms={i} {}\n",
                "x".repeat(400)
            ));
        }
        let excerpt = connection_excerpt(&log, &Scrub::default()).unwrap();
        assert!(excerpt.len() <= EXCERPT_MAX_BYTES);
        assert!(excerpt.lines().count() <= EXCERPT_MAX_LINES);
        assert!(excerpt
            .lines()
            .all(|l| l.chars().count() <= EXCERPT_LINE_CHARS));
        assert!(excerpt.ends_with('…'));
        assert!(
            excerpt.contains("ms=499"),
            "the newest lines are the ones kept"
        );

        let short =
            "2026-11-02T14:00:00.000000Z  INFO atv_optimizer_lib::adb::driver: a\n".repeat(200);
        assert_eq!(
            connection_excerpt(&short, &Scrub::default())
                .unwrap()
                .lines()
                .count(),
            EXCERPT_MAX_LINES
        );
        assert_eq!(connection_excerpt("", &Scrub::default()), None);
        assert_eq!(
            connection_excerpt(
                "2026-11-02T14:00:00Z  INFO atv_optimizer_core::telemetry: x\n",
                &Scrub::default()
            ),
            None
        );
    }

    #[test]
    fn uploads_keep_the_newest_bytes() {
        let log: String = (0..20_000)
            .map(|i| format!("line {i} {}\n", "y".repeat(40)))
            .collect();
        let text = upload_log(&log, &Scrub::default());
        assert!(text.len() <= UPLOAD_LOG_MAX_BYTES);
        assert!(text.ends_with(&format!("line 19999 {}", "y".repeat(40))));
    }

    #[test]
    fn a_line_without_a_header_is_redacted_whole() {
        assert_eq!(
            redact_log_line(
                "adb-ABC123456-x._adb-tls-connect._tcp 192.168.1.2:5555",
                &Scrub::default(),
                200
            ),
            "<mdns> <ip>"
        );
        // The timestamp is not mistaken for a host:port or an address.
        assert_eq!(
            redact_log_line(
                "2026-11-02T14:03:22.123Z ERROR x::adb: boom",
                &Scrub::default(),
                200
            ),
            "2026-11-02T14:03:22.123Z ERROR x::adb: boom"
        );
        // Control characters can't forge a second line.
        assert!(!redact_log_line("a\u{1b}[31mb\rc", &Scrub::default(), 200).contains('\u{1b}'));
    }

    #[test]
    fn serials_from_an_earlier_session_are_found_in_the_log_itself() {
        // Nothing this session knows: the TV was connected before a restart.
        let log = "\
2026-11-01T09:00:00.000000Z DEBUG atv_optimizer_lib::adb::driver: adb done args=[\"devices\"] stdout=List of devices attached
ZXCVBNMLKJHG\tunauthorized
POIUYTREWQLK    device usb:1-1 product:x
2026-11-01T09:00:01.000000Z DEBUG atv_optimizer_lib::adb::driver: adb invoke args=[\"-s\", \"MNBVCXZLKJHG\", \"shell\", \"getprop\"]
2026-11-01T09:00:02.000000Z  INFO atv_optimizer_core::commands::devices: forget finished serial=ASDFGHJKLQWE ok=true
2026-11-01T09:00:03.000000Z DEBUG atv_optimizer_core::adb::driver: adb done stdout=[ro.serialno]: [QAZWSXEDCRFV]
2026-11-01T09:00:04.000000Z  INFO atv_optimizer_core::commands::devices: later ZXCVBNMLKJHG POIUYTREWQLK MNBVCXZLKJHG ASDFGHJKLQWE QAZWSXEDCRFV gone
";
        let found = identifiers_in(log);
        for serial in [
            "ZXCVBNMLKJHG",
            "POIUYTREWQLK",
            "MNBVCXZLKJHG",
            "ASDFGHJKLQWE",
            "QAZWSXEDCRFV",
        ] {
            assert!(
                found.iter().any(|f| f == serial),
                "{serial} not found in {found:?}"
            );
        }
        for text in [
            upload_log(log, &Scrub::default()),
            connection_excerpt(log, &Scrub::default()).unwrap(),
        ] {
            for serial in [
                "ZXCVBNMLKJHG",
                "POIUYTREWQLK",
                "MNBVCXZLKJHG",
                "ASDFGHJKLQWE",
                "QAZWSXEDCRFV",
            ] {
                assert!(!text.contains(serial), "{serial} leaked:\n{text}");
            }
            assert!(
                text.contains("later <id> <id> <id> <id> <id> gone"),
                "{text}"
            );
        }
    }

    /// The exact lines an independent review got past both redactors.
    const NAME_AND_DNS_LOG: &str = "\
2026-11-02T14:00:00.000000Z DEBUG atv_optimizer_lib::adb::driver: adb done args=[\"-s\", \"192.168.1.20:5555\", \"shell\", \"settings get global device_name\"] ms=40 exit_code=Some(0) stderr= stdout=Bedroom Shield
2026-11-02T14:00:01.000000Z  WARN atv_optimizer_lib::adb::driver: adb timeout args=[\"-s\", \"192.168.1.20:5555\", \"shell\", \"settings put global device_name 'Kids Room TV'\"] ms=30001
2026-11-02T14:00:02.000000Z  WARN atv_optimizer_lib::adb::driver: adb exited nonzero args=[\"-s\", \"192.168.1.20:5555\", \"shell\", \"settings put global private_dns_specifier '1a2b3c.dns.nextdns.io'; settings put global private_dns_mode hostname\"] exit_code=Some(1) stderr=Error: connection timed out
while talking to 1a2b3c.dns.nextdns.io
2026-11-02T14:00:03.000000Z DEBUG atv_optimizer_lib::adb::driver: adb done args=[\"-s\", \"192.168.1.20:5555\", \"shell\", \"settings get global private_dns_specifier\"] ms=12 exit_code=Some(0) stderr= stdout=9f8e7d.dns.nextdns.io
2026-11-02T14:00:04.000000Z  INFO atv_optimizer_core::commands::devices: rename failed: Device still reports device_name = \"Den TV\" after the write. dns private_dns_specifier=4a5b6c.dns.nextdns.io resolver 7x7.example-dns.net
2026-11-02T14:00:04.500000Z  INFO atv_optimizer_core::commands::devices: probe 7x7.example-dns.net and 0123abcd.dns.nextdns.io failed
2026-11-02T14:00:05.000000Z DEBUG atv_optimizer_lib::adb::driver: adb done args=[\"shell\", \"getprop\"] ms=9 exit_code=Some(0) stderr= stdout=[persist.sys.device_name]: [Living Room]
[ro.product.model]: [SHIELD Android TV]
";

    #[test]
    fn tv_names_and_dns_hosts_never_survive() {
        let leaks = [
            "Bedroom",
            "Kids Room",
            "1a2b3c",
            "9f8e7d",
            "4a5b6c",
            "nextdns",
            "Den TV",
            "Living Room",
            "7x7",
            "example-dns",
            "0123abcd",
        ];
        for text in [
            upload_log(NAME_AND_DNS_LOG, &Scrub::default()),
            connection_excerpt(NAME_AND_DNS_LOG, &Scrub::default()).unwrap(),
        ] {
            for leak in leaks {
                assert!(!text.contains(leak), "{leak} leaked:\n{text}");
            }
            assert!(text.contains("device_name <redacted>"), "{text}");
            assert!(text.contains("stderr=<timeout>"), "{text}");
            assert!(text.contains("stdout=<omitted>"), "{text}");
            assert!(text.contains("private_dns_mode hostname"), "{text}");
        }
    }

    #[test]
    fn short_known_strings_are_ignored_and_words_match_whole() {
        let s = Scrub {
            known: &["ab"],
            words: &["anna"],
        };
        assert_eq!(
            redact_log_line("about Anna's bandanna", &s, 200),
            "about <user>'s bandanna"
        );
    }
}

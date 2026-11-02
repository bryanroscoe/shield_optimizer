// Server-side redaction. The client redacts first; this is the second pass that
// makes sure a client bug cannot put identifying data into the database.
//
// Order matters: wider shapes (URLs, emails, adb mDNS names) are replaced before
// the narrower ones they contain (hosts, IPs, serial-looking tokens).

const RULES: Array<[RegExp, string]> = [
  // scheme://anything (http, file, content, adb-tls...)
  [/\b[a-z][a-z0-9+.-]{1,20}:\/\/\S+/gi, "[url]"],
  [/[a-z0-9._%+-]+@[a-z0-9.-]+\.[a-z]{2,}/gi, "[email]"],
  // adb mDNS service names embed the TV serial: adb-0323220012345-AbCdEf._adb-tls-connect._tcp
  [/\badb-[\w.-]+/gi, "[host]"],
  [/\b[\w-]+\.local\b\.?/gi, "[host]"],
  [/\b(?:[0-9a-f]{2}[:-]){5}[0-9a-f]{2}\b/gi, "[mac]"],
  [/\b(?:\d{1,3}\.){3}\d{1,3}(?::\d{1,5})?\b/g, "[ip]"],
  // IPv6, including compressed (fe80::1), bracketed ([::1]:5555) and zone ids.
  // A whole token of hex groups and colons; Rust paths like adb::driver are left
  // alone because they contain non-hex letters. Clock times (12:34:56) also
  // match, which is an acceptable loss.
  [/(?<![\w:])\[?[0-9a-f]{0,4}(?::[0-9a-f]{0,4}){2,7}(?:%\w+)?\]?(?::\d{1,5})?(?![\w:])/gi, "[ip]"],
  // host:port (localhost:5037, my-tv:5555, tv.lan:40123)
  [/\b[a-z0-9][\w.-]*:\d{2,5}\b/gi, "[host]"],
  // A quoted string with a path separator in it is a path, spaces and all
  // ("C:\Users\Jane Smith\x.txt", '/home/Jane Smith/x').
  [/"[^"\n]*[\\/][^"\n]*"|'[^'\n]*[\\/][^'\n]*'/g, "[path]"],
  // Windows drive paths, UNC paths, home-relative and absolute POSIX paths.
  [/\b[a-z]:[\\/][^\s"'<>|]*/gi, "[path]"],
  [/\\\\[^\s"'<>|]+/g, "[path]"],
  [/~[\\/][^\s"'<>|]*/g, "[path]"],
  [/(?<![\w.\]])\/[^\s"'<>|]+/g, "[path]"],
  // Relative paths: two or more separators (data/users/Jane/private.cfg), or one
  // separator ending in a file extension (users/private.cfg). and/or survives.
  [/\b[\w.-]+(?:[\\/][\w.-]+){2,}/g, "[path]"],
  [/\b[\w.-]+[\\/][\w-]+\.[a-z0-9]{1,5}\b/gi, "[path]"],
  // Unquoted paths with spaces: words after a [path] that still contain a
  // separator are the rest of it (C:\Users\Jane Smith\x.txt).
  [/\[path\](?:\s+(?:\[path\]|[^\s]*[\\/][^\s]*))+/g, "[path]"],
];

// Long alphanumeric tokens with several digits: adb serials (0323220012345,
// 3A091FDJH000AB), Android ids, hashes, long timestamps.
const SERIAL = /\b(?=[a-z0-9]*\d[a-z0-9]*\d[a-z0-9]*\d[a-z0-9]*\d)[a-z0-9]{10,}\b/gi;
// Upper-case letter+digit tokens of 8 or more: adb serials with few digits
// (R5CRABCDEFZ, HT7A1JT00123). Messages only: in the model field the same shape
// is a real model number (55UQ7590PUB) that the device report needs. A serial
// made only of letters cannot be told apart from a word; the client redacts it.
const SERIAL_UPPER = /\b(?=[A-Z0-9]*\d)(?=[A-Z0-9]*[A-Z])[A-Z0-9]{8,}\b/g;
// Same shape in lower or mixed case (r5crabcdefz), at 10+ characters so short
// words with a digit (android14) survive.
// Serials and transport ids with separators (emulator-5554, ABCDEF-1234):
// hyphen/underscore-joined tokens with a run of 4+ digits. Dates go too.
const SERIAL_SEPARATED = /\b(?=[\w-]*\d{4})[a-z0-9]+(?:[-_][a-z0-9]+)+\b/gi;
const SERIAL_ANYCASE = /\b(?=[a-z0-9]*\d)(?=[a-z0-9]*[a-z])[a-z0-9]{10,}\b/gi;

// Package / class shaped tokens: com.example.app, java.lang.SecurityException,
// and two-segment application ids such as com.foo or tv.baz. Only used to
// detect a list, so file names like a.json in a list-shaped message go too.
const PACKAGE = /\b[a-z][a-z0-9_]*(?:\.[a-z0-9_]+)+\b/gi;
// One package name in an error ("disable failed for com.foo.bar") is useful for
// debugging; three or more is a package list, which is never stored.
const PACKAGE_LIST_THRESHOLD = 3;

// Control characters other than plain spaces become spaces.
const CONTROL = /[\u0000-\u001f\u007f-\u009f\u2028\u2029]/g;

export const MESSAGE_MAX = 500;

// ATV Optimizer license keys (ATVOPT-<payload>-<signature>, any case) and any
// message about a license or licensee. The licensee is free text, so it cannot
// be found reliably; the whole message is dropped and error_code keeps the why.
const LICENSE_KEY = /\batvopt-[\w-]*/i;
const LICENSE_TOPIC = /licen[cs]/i;
export const LICENSE_REMOVED = "[license-related message removed]";
export const PATH_REMOVED = "[message removed: contained a path]";

export function redactMessage(input: string): string {
  if (LICENSE_KEY.test(input) || LICENSE_TOPIC.test(input)) return LICENSE_REMOVED;
  let text = input.replace(CONTROL, " ");
  for (const [pattern, replacement] of RULES) {
    text = text.replace(pattern, replacement);
  }
  text = text.replace(SERIAL, "[serial]");
  text = text.replace(SERIAL_UPPER, "[serial]");
  text = text.replace(SERIAL_ANYCASE, "[serial]");
  text = text.replace(SERIAL_SEPARATED, "[serial]");
  const packages = text.match(PACKAGE) ?? [];
  if (packages.length >= PACKAGE_LIST_THRESHOLD) {
    text = text.replace(PACKAGE, "[package]");
  }
  text = text.replace(/ {2,}/g, " ").trim();
  // Catch-all: the patterns above cannot see every path shape (unquoted ones
  // with spaces, odd relative forms), so any separator left over means the
  // message may still carry part of a path and it is dropped whole.
  if (/[\\/]/.test(text)) return PATH_REMOVED;
  return clamp(text, MESSAGE_MAX);
}

// Short identifying fields (manufacturer, model, os, ...). Anything that the
// message redactor would have touched means the client put the wrong thing in
// the field, so the whole value is replaced rather than partially kept.
export function redactShortField(input: string, opts: { serialShape?: boolean } = {}): string {
  const cleaned = input.replace(CONTROL, " ").replace(/ {2,}/g, " ").trim();
  if (cleaned === "") return "";
  let probe = cleaned;
  for (const [pattern, replacement] of RULES) {
    probe = probe.replace(pattern, replacement);
  }
  if (probe !== cleaned || LICENSE_KEY.test(cleaned) || /[\\/]/.test(cleaned)) return "[redacted]";
  // A whole value that looks like a serial (the client sent ro.serialno as the
  // device codename, say). Codenames like "mdarcy" or "AFTMM" stay. Not applied
  // to model, where real model numbers (55UQ7590PUB) have the same shape.
  if (
    opts.serialShape !== false &&
    ((/^[a-z0-9]{10,}$/i.test(cleaned) && (cleaned.match(/\d/g) ?? []).length >= 4) ||
      new RegExp(`^${SERIAL_UPPER.source}$`).test(cleaned) ||
      new RegExp(`^${SERIAL_ANYCASE.source}$`, "i").test(cleaned) ||
      new RegExp(`^${SERIAL_SEPARATED.source}$`, "i").test(cleaned))
  ) {
    return "[redacted]";
  }
  return cleaned;
}

// Clamp by code points so a surrogate pair is never split.
export function clamp(text: string, max: number): string {
  const chars = Array.from(text);
  return chars.length <= max ? text : chars.slice(0, max).join("");
}

// ── Log text (schema 2) ─────────────────────────────────────────────────────
//
// A desktop event may carry `log_excerpt`: the app's own adb and connection
// log lines, already redacted by the client (engine/log_redact.rs). Each line
// is redacted again here with the message rules. The leading timestamp, level
// and module path are set aside first, since the address rules would read a
// clock time as IPv6 and a date-time as host:port.

const LOG_HEADER =
  /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{1,9})?Z(?:\s+(?:TRACE|DEBUG|INFO|WARN|ERROR)\s+(?:[a-z0-9_]+(?:::[a-z0-9_]+)*:\s)?)?/;

export const EXCERPT_MAX_LINES = 60;
export const EXCERPT_MAX_BYTES = 8 * 1024;
const EXCERPT_LINE_MAX = 300;
export const LINE_REMOVED_PATH = "[line removed: contained a path]";
export const LINE_REMOVED_LICENSE = "[line removed: license-related]";

// Command output never leaves the computer: device reads print TV names, DNS
// hosts and whatever else the TV holds, in shapes no rule can know. From a
// line's first `stderr=`/`stdout=` on, the value is dropped (the client keeps
// stderr as an error code, `stderr=<timeout>`), and so are the continuation
// lines that follow it up to the next timestamped line.
const OUTPUT_FIELD = /(^|\s)(?:stderr|stdout)=/;

function cutOutput(line: string): string | null {
  const m = OUTPUT_FIELD.exec(line);
  if (!m) return null;
  const head = line.slice(0, m.index);
  const rest = line.slice(m.index);
  // A client-side code (`stderr=<timeout>`) is already safe and is kept.
  const code = /stderr=<([a-z][a-z0-9_]{0,47})>/.exec(rest)?.[1];
  const parts: string[] = [];
  if (rest.includes("stderr=")) parts.push(`stderr=<${code ?? "omitted"}>`);
  if (rest.includes("stdout=")) parts.push("stdout=<omitted>");
  return `${head.trimEnd()} ${parts.join(" ")}`.trimStart();
}

// Linear: one scan over the lines.
export function dropCommandOutput(text: string): string {
  const out: string[] = [];
  let dropping = false;
  for (const line of text.split("\n")) {
    const header = LOG_HEADER_START.test(line);
    if (dropping && !header) continue;
    dropping = false;
    const cut = cutOutput(line);
    if (cut !== null) {
      out.push(cut);
      dropping = true;
    } else {
      out.push(line);
    }
  }
  return out.join("\n");
}

const LOG_HEADER_START = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}/;

// The value after a setting that holds something the user named: the TV's
// name or the Private DNS host (`settings put global device_name 'Kids Room
// TV'`, `device_name = "Den"`, `private_dns_specifier=1a2b3c.dns.nextdns.io`).
// Bounded so it stays linear on hostile input.
const NAMED_VALUE =
  /\b(device_name|private_dns_specifier|bluetooth_name)\b(\s{0,4}\]?:\s{0,4}\[?|\s{0,4}=\s{0,4}|\s{1,4})("(?:[^"\\\n]|\\.){0,256}"|(?:[^"\\;\],\n]|\\.){1,256})/gi;

// A dotted host that can't be a package id (a label starts with a digit or
// holds a hyphen) under a letters-only top-level label: `1a2b3c.dns.nextdns.io`.
const DOTTED_HOST = /\b[a-z0-9-]{1,63}(?:\.[a-z0-9-]{1,63}){0,8}\.[a-z]{2,24}\b/gi;

function hostShaped(token: string): string {
  return token.split(".").some((l) => l.includes("-") || /^\d/.test(l)) ? "[host]" : token;
}

export function redactLogLine(line: string, max = EXCERPT_LINE_MAX): string {
  // The client caps lines at 300 characters; anything longer is cut before
  // any rule runs, so no line can make the rules below expensive.
  const text = clamp(line, max * 2).replace(CONTROL, " ");
  if (LICENSE_KEY.test(text) || LICENSE_TOPIC.test(text)) return LINE_REMOVED_LICENSE;
  const header = LOG_HEADER.exec(text)?.[0] ?? "";
  let body = text.slice(header.length);
  body = body.replace(NAMED_VALUE, "$1$2[redacted]");
  for (const [pattern, replacement] of RULES) body = body.replace(pattern, replacement);
  body = body.replace(DOTTED_HOST, hostShaped);
  body = body.replace(SERIAL, "[serial]");
  body = body.replace(SERIAL_UPPER, "[serial]");
  body = body.replace(SERIAL_ANYCASE, "[serial]");
  body = body.replace(SERIAL_SEPARATED, "[serial]");
  if ((body.match(PACKAGE) ?? []).length >= PACKAGE_LIST_THRESHOLD) body = body.replace(PACKAGE, "[package]");
  if (/[\\/]/.test(body)) body = LINE_REMOVED_PATH;
  return clamp(`${header}${body}`.replace(/ {2,}/g, " ").trimEnd(), max);
}

// Keeps the newest lines that fit both caps.
function newestFitting(lines: string[], maxLines: number, maxBytes: number): string[] {
  const out: string[] = [];
  let bytes = 0;
  for (let i = lines.length - 1; i >= 0 && out.length < maxLines; i--) {
    const line = lines[i] ?? "";
    const size = new TextEncoder().encode(line).length + 1;
    if (bytes + size > maxBytes) break;
    bytes += size;
    out.push(line);
  }
  return out.reverse();
}

export function redactLogExcerpt(input: string): string {
  // Bound the work before redacting: a client never sends more than the
  // caps. The newest lines are at the end, so the end is what's kept.
  const limit = EXCERPT_MAX_BYTES * 2;
  const tail = input.length > limit ? input.slice(-limit).replace(/^[^\n]*\n?/, "") : input;
  const lines = dropCommandOutput(tail.replace(/\r\n/g, "\n"))
    .split("\n")
    .slice(-EXCERPT_MAX_LINES)
    .map((l) => redactLogLine(l))
    .filter((l) => l.trim() !== "");
  return newestFitting(lines, EXCERPT_MAX_LINES, EXCERPT_MAX_BYTES).join("\n");
}

// Send logs uploads are up to 256 KiB, too much to run every message rule
// over inside the Worker's CPU budget. The client redacts them line by line
// with the full rules; here a few linear passes remove what matters most if
// a client bug let one through: command output, TV names and Private DNS
// hosts, IPv4 addresses (with ports), MAC addresses, emails, adb mDNS service
// names, *.local hosts, digit- or hyphen-labelled hosts and licence keys.
//
// It works token by token (whitespace-separated), which keeps it linear and
// cheap inside the 10 ms CPU budget: a token with none of `. @ : -` in it can't
// hold anything the patterns look for and is skipped, every pattern is bounded,
// and a token over LONG_TOKEN characters (no log line has one; a hostile body
// might) is dropped whole rather than scanned.
const UPLOAD_PASS =
  /\b(?:\d{1,3}\.){3}\d{1,3}(?::\d{1,5})?\b|\b(?:[0-9a-f]{2}[:-]){5}[0-9a-f]{2}\b|[a-z0-9._%+-]{1,64}@[a-z0-9.-]{1,253}\.[a-z]{2,24}|\badb-[\w.-]{1,128}|\b[\w-]{1,63}\.local\b\.?|\batvopt-[\w-]{0,128}/gi;
const LONG_TOKEN = 512;
export const LONG_TOKEN_REMOVED = "[long token removed]";
const MAYBE_SENSITIVE = /[.@:-]/;

function redactUploadToken(token: string): string {
  if (token.length > LONG_TOKEN) return LONG_TOKEN_REMOVED;
  if (!MAYBE_SENSITIVE.test(token)) return token;
  return token
    .replace(UPLOAD_PASS, "[redacted]")
    .replace(DOTTED_HOST, (t) => (hostShaped(t) === t ? t : "[redacted]"));
}

export function redactUploadText(input: string): string {
  const text = dropCommandOutput(input.replace(CONTROL_EXCEPT_NEWLINE, " ").replace(/\r\n/g, "\n")).replace(
    NAMED_VALUE,
    "$1$2[redacted]",
  );
  // Logs repeat the same tokens (module paths, timestamps' shapes, field
  // names) thousands of times; each distinct one is redacted once.
  const seen = new Map<string, string>();
  return text
    .split(/(\s+)/)
    .map((part, i) => {
      if (i % 2 === 1) return part;
      let out = seen.get(part);
      if (out === undefined) {
        out = redactUploadToken(part);
        if (seen.size < 20_000) seen.set(part, out);
      }
      return out;
    })
    .join("");
}

const CONTROL_EXCEPT_NEWLINE = /[\u0000-\u0008\u000b-\u001f\u007f-\u009f\u2028\u2029]/g;

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
  if (probe !== cleaned || LICENSE_KEY.test(cleaned)) return "[redacted]";
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

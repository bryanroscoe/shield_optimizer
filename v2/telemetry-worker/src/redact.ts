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
  // Windows drive paths, UNC paths, home-relative and absolute POSIX paths.
  [/\b[a-z]:[\\/][^\s"'<>|]*/gi, "[path]"],
  [/\\\\[^\s"'<>|]+/g, "[path]"],
  [/~[\\/][^\s"'<>|]*/g, "[path]"],
  [/(?<![\w.\]])\/[^\s"'<>|]+/g, "[path]"],
];

// Long alphanumeric tokens with several digits: adb serials (0323220012345,
// 3A091FDJH000AB), Android ids, hashes, long timestamps.
const SERIAL = /\b(?=[a-z0-9]*\d[a-z0-9]*\d[a-z0-9]*\d[a-z0-9]*\d)[a-z0-9]{10,}\b/gi;

// Java package / class shaped tokens: com.example.app, java.lang.SecurityException.
const PACKAGE = /\b[a-z][a-z0-9_]*(?:\.[a-z0-9_]+){2,}\b/gi;
// One package name in an error ("disable failed for com.foo.bar") is useful for
// debugging; three or more is a package list, which is never stored.
const PACKAGE_LIST_THRESHOLD = 3;

// Control characters other than plain spaces become spaces.
const CONTROL = /[\u0000-\u001f\u007f-\u009f\u2028\u2029]/g;

export const MESSAGE_MAX = 500;

export function redactMessage(input: string): string {
  let text = input.replace(CONTROL, " ");
  for (const [pattern, replacement] of RULES) {
    text = text.replace(pattern, replacement);
  }
  text = text.replace(SERIAL, "[serial]");
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
export function redactShortField(input: string): string {
  const cleaned = input.replace(CONTROL, " ").replace(/ {2,}/g, " ").trim();
  if (cleaned === "") return "";
  let probe = cleaned;
  for (const [pattern, replacement] of RULES) {
    probe = probe.replace(pattern, replacement);
  }
  if (probe !== cleaned) return "[redacted]";
  // A whole value that looks like a serial (the client sent ro.serialno as the
  // model, say). Codenames like "mdarcy" or "AFTMM" stay.
  if (/^[a-z0-9]{10,}$/i.test(cleaned) && (cleaned.match(/\d/g) ?? []).length >= 4) {
    return "[redacted]";
  }
  return cleaned;
}

// Clamp by code points so a surrogate pair is never split.
export function clamp(text: string, max: number): string {
  const chars = Array.from(text);
  return chars.length <= max ? text : chars.slice(0, max).join("");
}

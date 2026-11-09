const REDACTED = "[redacted]";

/// Shorter than this, a serial is only redacted where it stands as a whole
/// token: "A1" must not eat the middle of "A1B2", let alone every word that
/// happens to contain those letters.
const SERIAL_SUBSTRING_MIN = 4;

function escapeRegExp(text: string): string {
  return text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/// Anything shaped like an Android package id (two or more dot-joined
/// segments, each starting with a letter). The leading boundary is captured
/// rather than a lookbehind, which older macOS WebViews cannot parse.
const PACKAGE_SHAPED = /(^|[^A-Za-z0-9_.])([A-Za-z][A-Za-z0-9_]*(?:\.[A-Za-z][A-Za-z0-9_]*)+)(?![A-Za-z0-9_])/g;

function redactSerials(text: string, redact: string[]): string {
  let out = text;
  for (const id of redact) {
    const v = id;
    if (!v) continue;
    const body = escapeRegExp(v);
    out =
      v.length >= SERIAL_SUBSTRING_MIN
        ? out.replace(new RegExp(body, "gi"), REDACTED)
        : out.replace(new RegExp(`(^|[^A-Za-z0-9])${body}(?![A-Za-z0-9])`, "gi"), `$1${REDACTED}`);
  }
  return out;
}

function redactAddresses(text: string, redact: string[]): string {
  return redactSerials(text, redact)
    .replace(/\b\d{1,3}(?:\.\d{1,3}){3}(?::\d+)?\b/g, REDACTED)
    .replace(/\b(?:[0-9A-Fa-f]{2}[:-]){5}[0-9A-Fa-f]{2}\b/g, REDACTED)
    .replace(/\b(?:[0-9A-Fa-f]{1,4}:){7}[0-9A-Fa-f]{1,4}\b/g, REDACTED)
    .replace(/(?:\b[0-9A-Fa-f]{1,4}(?::[0-9A-Fa-f]{1,4})*)?::(?:[0-9A-Fa-f]{1,4}(?::[0-9A-Fa-f]{1,4})*\b)?/g, (m) =>
      m === "::" ? m : REDACTED,
    );
}

function redactIdentifiers(text: string, redact: string[], reportedPackage?: string): string {
  const keep = reportedPackage?.toLowerCase();
  return redactAddresses(text, redact)
    .replace(PACKAGE_SHAPED, (_m, before: string, token: string) => {
      if (token.toLowerCase() === keep) return before + token;
      if (token.split(".").every((segment) => segment.length === 1)) return before + token;
      return before + REDACTED;
    });
}

// Only an explicit HTTP(S) authority can be treated as a domain rather than
// an installed-package token. Every other URL component keeps privacy rules.
// Private-network names stay redacted, and so does a host that reads like a
// package id (`com.private.app`): real hostnames don't start with a generic TLD.
function publicHostname(host: string, reportedPackage?: string): boolean {
  if (reportedPackage && host === reportedPackage.toLowerCase()) return true;
  return /^(?:[a-z0-9](?:[a-z0-9-]*[a-z0-9])?\.)+[a-z]{2,}$/i.test(host)
    && !/(?:^|\.)(?:localhost|local|localdomain|lan|internal|intranet|private|corp|home|home\.arpa|box|invalid|test|onion|ts\.net)$/i.test(host)
    && !/^(?:com|org|net|edu|gov|mil|int|io|info|biz)\./i.test(host)
    // Wildcard-DNS names spell an address with dashes (192-168-1-5.sslip.io).
    && !/(?:^|[^0-9])\d{1,3}(?:-\d{1,3}){3}(?![0-9])/.test(host);
}

// Query keys can be tokens too. Only these carry no personal data, and only
// the reported id and a language/country code survive as values.
const PUBLIC_QUERY_KEYS = new Set(["id", "hl", "gl"]);

function decoded(value: string): string | null {
  // Inspect nested escaping as well as the ordinary single-encoded case.
  for (let depth = 0; depth < 5; depth++) {
    try {
      const next = decodeURIComponent(value);
      if (next === value) return value;
      value = next;
    } catch { return null; }
  }
  return null;
}

function redactUrl(raw: string, redact: string[], reportedPackage?: string): string {
  let url: URL;
  try { url = new URL(raw); } catch { return REDACTED; }
  const scrub = (value: string) => redactIdentifiers(value, redact, reportedPackage);
  // A host is checked for serials and addresses only: its dots and hyphens
  // would otherwise read as package ids (#323).
  const host = publicHostname(url.hostname, reportedPackage) && redactAddresses(url.hostname, redact) === url.hostname
    ? url.hostname : REDACTED;
  // Decode each path segment for inspection, retaining its original encoding
  // only when no identifying content was found. Encoded separators are inspected too.
  const path = url.pathname.split('/').map((part) => {
    const value = decoded(part);
    return value !== null && scrub(value) === value ? part : REDACTED;
  }).join('/');
  // Query values and fragments may be credentials, account ids or nested URLs.
  // The reported app's exact id is the only query value the report needs.
  const query = [...url.searchParams].map(([key, value]) => {
    const safeKey = PUBLIC_QUERY_KEYS.has(key.toLowerCase()) ? encodeURIComponent(key) : REDACTED;
    const safeValue = (key === 'id' && value === reportedPackage && scrub(value) === value)
      || (/^(?:hl|gl)$/i.test(key) && /^[a-z]{2,3}(?:[-_][a-z]{2,4})?$/i.test(value))
      ? value : REDACTED;
    return `${safeKey}=${safeValue === REDACTED ? REDACTED : encodeURIComponent(safeValue)}`;
  }).join('&');
  return `${url.protocol}//${url.username || url.password ? REDACTED + '@' : ''}${host}${url.port ? ':' + scrub(url.port) : ''}${path}${query ? '?' + query : ''}${url.hash ? '#' + REDACTED : ''}`;
}

export function redactReportNote(text: string, redact: string[], reportedPackage?: string): string {
  // Preserve whole-note serial matching, including the scheme and boundaries
  // between prose and links, before any URL parsing or normalization.
  text = redactSerials(text, redact);
  const urlPattern = /https?:\/\/[^\s<>"`]+/gi;
  let result = '', end = 0;
  for (const match of text.matchAll(urlPattern)) {
    const at = match.index;
    result += redactIdentifiers(text.slice(end, at), redact, reportedPackage);
    // Trailing prose punctuation is not part of the link.
    const raw = match[0].replace(/[.,;!?)]+$/, '');
    result += redactUrl(raw, redact, reportedPackage) + match[0].slice(raw.length);
    end = at + match[0].length;
  }
  return result + redactIdentifiers(text.slice(end), redact, reportedPackage);
}

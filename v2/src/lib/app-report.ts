// "Report this app": one package, written out as a record the catalog
// reviewer's tool (tools/registry-triage) reads, in the same shape as the
// mobile app's unknown-package export. Nothing here sends anything. The user
// sees the exact JSON, then copies it, saves it, or opens a GitHub form they
// still have to submit.
//
// A report is a claim by one user, not a verdict. It never carries a
// suggested classification, and it is never read back into the app.

import type { DeviceType, TvEvidence } from "./types";
import type { Safety } from "../../shared/safety";
import { idKey } from "./prefs";

export type AppReportReason = "not_listed" | "wrong_verdict" | "wrong_description" | "other";

/// The `reason` values a desktop report writes. registry-triage accepts these
/// beside the mobile collector's own reasons.
export const APP_REPORT_REASONS: { id: AppReportReason; label: string; record: string }[] = [
  { id: "not_listed", label: "Not in the list", record: "user_report_not_listed" },
  { id: "wrong_verdict", label: "Wrong verdict", record: "user_report_wrong_verdict" },
  { id: "wrong_description", label: "Wrong description", record: "user_report_wrong_description" },
  { id: "other", label: "Other", record: "user_report_other" },
];

export type ReportDeviceFamily = "shield" | "google_tv" | "android_tv" | "unknown";

/// What the panel needs about the device. Deliberately no serial, no address
/// and no properties beyond the Android version.
export interface AppReportDevice {
  family: ReportDeviceFamily;
  androidVersion: string | null;
  /// The device's adb serial and ro.serialno. Never written into a report:
  /// they are only used to strip themselves out of the user's note.
  redact: string[];
}

export interface AppReportState {
  status: "enabled" | "disabled" | "missing" | null;
  /// Whether meminfo showed a process for the package: null when RAM was not
  /// read or the read failed, false for a successful read with no process.
  running: boolean | null;
  /// PSS when running; null otherwise.
  ramMb: number | null;
  /// `diskstats` is the full app/data/cache row; `apk_files` is the fallback
  /// that only knows the APK size.
  storage: {
    source: "diskstats" | "apk_files";
    app_bytes: number | null;
    data_bytes: number | null;
    cache_bytes: number | null;
  } | null;
}

export interface AppReportInput {
  package: string;
  appName: string | null;
  reason: AppReportReason;
  note: string;
  appVersion: string | null;
  device: AppReportDevice;
  /// null when the safety lookup did not complete.
  verdict: Safety | null;
  includeState: boolean;
  state: AppReportState;
  now: Date;
}

export const APP_REPORT_NOTE_MAX = 1000;
const NAME_MAX = 120;

/// A device that reported nothing about being a TV is "unknown", never
/// promoted to a family it did not claim.
export function reportFamily(type: DeviceType, tv: TvEvidence): ReportDeviceFamily {
  if (type === "shield") return "shield";
  if (type === "google_tv") return "google_tv";
  return tv === "tv" ? "android_tv" : "unknown";
}

/// The registry-triage / mobile collector rules for an installed_package
/// token. A package id that fails them cannot go into a report.
export function isReportablePackage(pkg: string): boolean {
  return (
    pkg.length > 0 &&
    pkg.length <= 255 &&
    /^[A-Za-z][A-Za-z0-9_]*(?:\.[A-Za-z][A-Za-z0-9_]*)+$/.test(pkg)
  );
}

function validVersion(value: string | null): value is string {
  return !!value && value.length <= 32 && /^[0-9A-Za-z][0-9A-Za-z.+_-]*$/.test(value);
}

function validDeviceOs(value: string | null): value is string {
  return !!value && value.length <= 32 && /^[0-9A-Za-z][0-9A-Za-z._ -]*$/.test(value);
}

function clean(text: string, max: number, keepNewlines: boolean): string | null {
  const pattern = keepNewlines ? /[\u0000-\u0009\u000b-\u001f\u007f]/g : /[\u0000-\u001f\u007f]/g;
  const out = text.replace(pattern, " ").trim().slice(0, max).trim();
  return out.length > 0 ? out : null;
}

const REDACTED = "[redacted]";

/// Shorter than this, a serial is only redacted where it stands as a whole
/// token: "A1" must not eat the middle of "A1B2", let alone every word that
/// happens to contain those letters.
const SERIAL_SUBSTRING_MIN = 4;

function escapeRegExp(text: string): string {
  return text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/// Anything shaped like an Android package id (two or more dot-joined
/// segments, each starting with a letter).
const PACKAGE_SHAPED = /(?<![A-Za-z0-9_.])[A-Za-z][A-Za-z0-9_]*(?:\.[A-Za-z][A-Za-z0-9_]*)+(?![A-Za-z0-9_])/g;

/// The note is the user's own text, but a pasted log line or address must not
/// carry the identifiers the rest of the report leaves out. Addresses, MACs,
/// this device's own serials, and package ids other than the reported one are
/// replaced, visibly, in the preview.
///
/// Serials match whatever their casing. One of four characters or more is
/// redacted wherever it appears; a shorter one only as a whole token. Either
/// way, over-redacting a word costs the reviewer a word, while missing an id
/// would ship it.
///
/// Other package ids are redacted rather than warned about: the dialog
/// promises no other installed app leaves the machine, and a warning only
/// holds that promise if the user reads it. A token made only of one-letter
/// segments ("e.g", "i.e") is left alone; no real package looks like that.
export function redactNote(text: string, redact: string[], reportedPackage?: string): string {
  let out = text;
  for (const id of redact) {
    // Same placeholder rule as everywhere else a hardware id is used: a
    // ro.serialno of "unknown" is no id, and scrubbing it would eat the word.
    const v = idKey(id);
    if (!v) continue;
    const body = escapeRegExp(v);
    const pattern =
      v.length >= SERIAL_SUBSTRING_MIN ? body : `(?<![A-Za-z0-9])${body}(?![A-Za-z0-9])`;
    out = out.replace(new RegExp(pattern, "gi"), REDACTED);
  }
  const keep = reportedPackage?.toLowerCase();
  return out
    .replace(/\b\d{1,3}(?:\.\d{1,3}){3}(?::\d+)?\b/g, REDACTED)
    .replace(/\b(?:[0-9A-Fa-f]{2}[:-]){5}[0-9A-Fa-f]{2}\b/g, REDACTED)
    .replace(/\b(?:[0-9A-Fa-f]{1,4}:){7}[0-9A-Fa-f]{1,4}\b/g, REDACTED)
    .replace(/(?:\b[0-9A-Fa-f]{1,4}(?::[0-9A-Fa-f]{1,4})*)?::(?:[0-9A-Fa-f]{1,4}(?::[0-9A-Fa-f]{1,4})*\b)?/g, (m) =>
      m === "::" ? m : REDACTED,
    )
    .replace(PACKAGE_SHAPED, (m) => {
      if (m.toLowerCase() === keep) return m;
      if (m.split(".").every((segment) => segment.length === 1)) return m;
      return REDACTED;
    });
}

function finiteOrNull(n: number | null | undefined): number | null {
  return typeof n === "number" && Number.isFinite(n) && n >= 0 ? n : null;
}

/// The report file: a `schema_version: 1` export with one record. Returns
/// null when the package id cannot be reported.
export function buildAppReport(input: AppReportInput): Record<string, unknown> | null {
  if (!isReportablePackage(input.package)) return null;
  const at = input.now.toISOString();
  const reason = APP_REPORT_REASONS.find((r) => r.id === input.reason) ?? APP_REPORT_REASONS[3];
  const record: Record<string, unknown> = {
    kind: "installed_package",
    token: input.package,
    reason: reason.record,
    app_version: validVersion(input.appVersion) ? input.appVersion : "unknown",
    registry_version: null,
    device_family: input.device.family,
    device_os: validDeviceOs(input.device.androidVersion) ? input.device.androidVersion : null,
    first_seen: at,
    last_seen: at,
    count: 1,
    app_name: input.appName ? clean(input.appName, NAME_MAX, false) : null,
    current_verdict: input.verdict ? input.verdict.kind : "unavailable",
    verdict_source: input.verdict ? input.verdict.source : null,
    note: clean(redactNote(input.note, input.device.redact, input.package), APP_REPORT_NOTE_MAX, true),
  };
  if (input.includeState) {
    const s = input.state;
    record.state = {
      installed: s.status === null ? null : s.status !== "missing",
      enabled: s.status === "enabled" ? true : s.status === "disabled" ? false : null,
      running: s.running,
      ram_mb: s.running ? finiteOrNull(s.ramMb) : null,
      storage: s.storage
        ? {
            source: s.storage.source,
            app_bytes: finiteOrNull(s.storage.app_bytes),
            data_bytes: finiteOrNull(s.storage.data_bytes),
            cache_bytes: finiteOrNull(s.storage.cache_bytes),
          }
        : null,
    };
  }
  return {
    schema_version: 1,
    generated_at: at,
    truncated: false,
    source: "desktop_app_report",
    records: [record],
  };
}

export function reportText(report: Record<string, unknown>): string {
  return JSON.stringify(report, null, 2);
}

export function reportFileName(pkg: string, now: Date): string {
  return `app-report-${pkg}-${now.toISOString().slice(0, 10)}.json`;
}

const ISSUE_BASE = "https://github.com/bryanroscoe/shield_optimizer/issues/new";
/// Same ceiling the bug form uses, well under what GitHub accepts in a URL.
export const APP_REPORT_PREFILL_LIMIT = 6000;

/// The app_report.yml issue form, its fields prefilled by query parameters of
/// the same ids. When the report would not fit, the form opens with the
/// package and reason only and the caller asks the user to paste.
export function appReportIssueUrl(
  pkg: string,
  reason: AppReportReason,
  text: string,
): { url: string; prefilled: boolean } {
  const url = new URL(ISSUE_BASE);
  url.searchParams.set("template", "app_report.yml");
  url.searchParams.set("title", `App report: ${pkg}`);
  url.searchParams.set("package", pkg);
  url.searchParams.set("reason", APP_REPORT_REASONS.find((r) => r.id === reason)?.label ?? "Other");
  const short = url.toString();
  url.searchParams.set("report", text);
  const full = url.toString();
  return full.length <= APP_REPORT_PREFILL_LIMIT
    ? { url: full, prefilled: true }
    : { url: short, prefilled: false };
}

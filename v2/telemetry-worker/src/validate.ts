import { clamp, redactMessage, redactShortField } from "./redact";

export const SCHEMA = 1;
export const MAX_EVENTS = 50;
export const MAX_BODY_BYTES = 64 * 1024;

export const APPS = ["desktop", "mobile"] as const;
export const KINDS = ["device_seen", "action_result", "error"] as const;
export const TIERS = ["unknown", "not_supported", "beta", "supported", "fully_supported"] as const;
export const PROFILES = ["shield", "googletv", "androidtv", "fire", "unknown"] as const;
export const ACTIONS = [
  "connect",
  "scan",
  "disable",
  "enable",
  "uninstall",
  "optimize",
  "snapshot_apply",
  "launcher",
  "settings_write",
  "pairing",
  "other",
] as const;

export type App = (typeof APPS)[number];
export type Kind = (typeof KINDS)[number];
export type Tier = (typeof TIERS)[number];
export type Profile = (typeof PROFILES)[number];
export type Action = (typeof ACTIONS)[number];

export interface DeviceInfo {
  manufacturer: string;
  brand: string;
  model: string;
  device: string;
  product: string;
  android_release: string;
  sdk: number | null;
  profile: Profile;
  support_tier: Tier;
}

// The only shape that ever reaches the database. Built field by field, so
// unknown input fields (serials, IPs, names, package lists...) have nowhere to go.
export interface CleanEvent {
  install_id: string;
  app: App;
  app_version: string;
  os: string;
  client_ts: string;
  kind: Kind;
  device: DeviceInfo | null;
  action: Action | null;
  ok: boolean | null;
  error_code: string | null;
  message: string | null;
}

export type Result<T> = { ok: true; value: T } | { ok: false; reason: string };

const UUID_V4 = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
// X.Y.Z with only the pre-release tags release.sh produces. No free-form
// suffix or build metadata: app_version is an aggregate dimension kept forever.
const APP_VERSION = /^\d{1,4}\.\d{1,4}\.\d{1,4}(?:-(?:alpha|beta|rc|preview|pre|dev)(?:\.\d{1,3})?)?$/;
const ERROR_CODE = /^[a-z][a-z0-9_]{0,63}$/;
const ANDROID_RELEASE = /^\d{1,2}(?:\.\d{1,2}){0,2}$/;
// RFC 3339 date-time with a mandatory offset.
const RFC3339 = /^\d{4}-\d{2}-\d{2}[Tt ]\d{2}:\d{2}:\d{2}(?:\.\d{1,9})?(?:[Zz]|[+-]\d{2}:\d{2})$/;

// Error codes are constants like pm_denied or adb_exit_255. More than four
// digits means a serial, IP or timestamp was folded in (serial_0323220012345,
// ip_192_168_1_5); license words mean licence data. Such codes are rejected,
// since codes are kept forever in agg_error_daily.
function plausibleErrorCode(code: string): boolean {
  return (code.match(/\d/g) ?? []).length <= 4 && !/licen[cs]/.test(code);
}

const FIELD_MAX = 64;
const OS_MAX = 40;

function isRecord(v: unknown): v is Record<string, unknown> {
  return typeof v === "object" && v !== null && !Array.isArray(v);
}

function oneOf<T extends string>(list: readonly T[], v: unknown): T | null {
  return typeof v === "string" && (list as readonly string[]).includes(v) ? (v as T) : null;
}

function shortField(v: unknown, max = FIELD_MAX, serialShape = true): string {
  if (typeof v !== "string") return "";
  return clamp(redactShortField(clamp(v, 256), { serialShape }), max);
}

function cleanDevice(raw: unknown): Result<DeviceInfo> {
  if (!isRecord(raw)) return { ok: false, reason: "device_not_object" };
  const tier = raw.support_tier === undefined ? "unknown" : oneOf(TIERS, raw.support_tier);
  if (tier === null) return { ok: false, reason: "unknown_support_tier" };
  // A newer client may know a profile this Worker does not; it is recorded as
  // unknown rather than dropping the event.
  const profile = oneOf(PROFILES, raw.profile) ?? "unknown";
  const release = typeof raw.android_release === "string" ? raw.android_release.trim() : "";
  let sdk: number | null = null;
  if (typeof raw.sdk === "number" && Number.isInteger(raw.sdk) && raw.sdk >= 1 && raw.sdk <= 99) {
    sdk = raw.sdk;
  }
  return {
    ok: true,
    value: {
      manufacturer: shortField(raw.manufacturer),
      brand: shortField(raw.brand),
      model: shortField(raw.model, FIELD_MAX, false),
      device: shortField(raw.device),
      product: shortField(raw.product),
      android_release: ANDROID_RELEASE.test(release) ? release : "",
      sdk,
      profile,
      support_tier: tier,
    },
  };
}

export function validateEvent(raw: unknown): Result<CleanEvent> {
  if (!isRecord(raw)) return { ok: false, reason: "event_not_object" };
  if (raw.schema !== SCHEMA) return { ok: false, reason: "unsupported_schema" };

  const kind = oneOf(KINDS, raw.kind);
  if (!kind) return { ok: false, reason: "unknown_kind" };
  const app = oneOf(APPS, raw.app);
  if (!app) return { ok: false, reason: "unknown_app" };

  if (typeof raw.install_id !== "string" || !UUID_V4.test(raw.install_id)) {
    return { ok: false, reason: "bad_install_id" };
  }
  if (typeof raw.app_version !== "string" || !APP_VERSION.test(raw.app_version)) {
    return { ok: false, reason: "bad_app_version" };
  }
  if (typeof raw.ts !== "string" || !RFC3339.test(raw.ts) || Number.isNaN(Date.parse(raw.ts))) {
    return { ok: false, reason: "bad_ts" };
  }
  const os = shortField(raw.os, OS_MAX).toLowerCase();
  if (!os || os === "[redacted]") return { ok: false, reason: "bad_os" };

  let device: DeviceInfo | null = null;
  if (raw.device !== undefined && raw.device !== null) {
    const d = cleanDevice(raw.device);
    if (!d.ok) return d;
    device = d.value;
  }

  let action: Action | null = null;
  if (raw.action !== undefined && raw.action !== null) {
    // Same forward-compatibility rule as profile: unknown actions become "other".
    action = oneOf(ACTIONS, raw.action) ?? "other";
  }

  let ok: boolean | null = null;
  if (raw.ok !== undefined && raw.ok !== null) {
    if (typeof raw.ok !== "boolean") return { ok: false, reason: "bad_ok" };
    ok = raw.ok;
  }

  let errorCode: string | null = null;
  if (raw.error_code !== undefined && raw.error_code !== null) {
    if (typeof raw.error_code !== "string" || !ERROR_CODE.test(raw.error_code) || !plausibleErrorCode(raw.error_code)) {
      return { ok: false, reason: "bad_error_code" };
    }
    errorCode = raw.error_code;
  }

  let message: string | null = null;
  if (raw.message !== undefined && raw.message !== null) {
    if (typeof raw.message !== "string") return { ok: false, reason: "bad_message" };
    // Bound the work before redacting; the result is clamped to 500 again after.
    message = redactMessage(clamp(raw.message, 2000)) || null;
  }

  switch (kind) {
    case "device_seen":
      if (!device) return { ok: false, reason: "device_seen_without_device" };
      // Fields that only mean something on other kinds are dropped, so a
      // stale error_code cannot show up in the error report.
      action = null;
      ok = null;
      errorCode = null;
      break;
    case "action_result":
      if (!action) return { ok: false, reason: "action_result_without_action" };
      if (ok === null) return { ok: false, reason: "action_result_without_ok" };
      if (ok) errorCode = null;
      break;
    case "error":
      if (!errorCode) return { ok: false, reason: "error_without_error_code" };
      ok = null;
      break;
  }

  return {
    ok: true,
    value: {
      install_id: raw.install_id.toLowerCase(),
      app,
      app_version: raw.app_version,
      os,
      client_ts: new Date(raw.ts).toISOString(),
      kind,
      device,
      action,
      ok,
      error_code: errorCode,
      message,
    },
  };
}

export interface ParsedBatch {
  events: CleanEvent[];
  rejected: Record<string, number>;
}

export function validateBatch(body: unknown): Result<ParsedBatch> {
  if (!isRecord(body) || !Array.isArray(body.events)) {
    return { ok: false, reason: "body_must_be_object_with_events_array" };
  }
  if (body.events.length === 0) return { ok: false, reason: "no_events" };
  if (body.events.length > MAX_EVENTS) return { ok: false, reason: "too_many_events" };
  const events: CleanEvent[] = [];
  const rejected: Record<string, number> = {};
  for (const raw of body.events) {
    const r = validateEvent(raw);
    if (r.ok) events.push(r.value);
    else rejected[r.reason] = (rejected[r.reason] ?? 0) + 1;
  }
  return { ok: true, value: { events, rejected } };
}

export function cleanCountry(v: unknown): string | null {
  return typeof v === "string" && /^[A-Z][A-Z0-9]$/.test(v) ? v : null;
}

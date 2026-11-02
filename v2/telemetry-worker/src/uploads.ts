// POST /v1/diagnostics: what a desktop user sent with "Send logs to
// developer". A redacted summary of their computer and TVs plus up to 256 KiB
// of the app's own log, keyed by the install id the app shows as its support
// ID, so a complaint that quotes the ID can be matched to its logs.
//
// The user pressed a button that says it sends their logs, so this is
// accepted whether or not their automatic reports are on. The client
// redacts every line (engine/log_redact.rs); redactUploadText is a second,
// cheaper pass. Kept for LOG_RETENTION_DAYS (30), then deleted by the cron.

import type { Env } from "./env";
import { intVar } from "./env";
import { admit, utcDay, type QuotaRow } from "./ingest";
import { clamp, redactShortField, redactUploadText } from "./redact";

export const UPLOAD_SCHEMA = 1;
// The client sends at most 256 KiB of log and a 16 KiB summary.
export const UPLOAD_MAX_TEXT = 300 * 1024;
export const UPLOAD_MAX_BODY = UPLOAD_MAX_TEXT + 16 * 1024;
export const UPLOAD_GLOBAL_KEY = "upload:*";

const UUID_V4 = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
const APP_VERSION = /^\d{1,4}\.\d{1,4}\.\d{1,4}(?:-(?:alpha|beta|rc|preview|pre|dev)(?:\.\d{1,3})?)?$/;
const RFC3339 = /^\d{4}-\d{2}-\d{2}[Tt ]\d{2}:\d{2}:\d{2}(?:\.\d{1,9})?(?:[Zz]|[+-]\d{2}:\d{2})$/;

export interface CleanUpload {
  install_id: string;
  app: "desktop";
  app_version: string;
  os: string;
  client_ts: string;
  reports_on: boolean | null;
  body: string;
}

export type UploadResult = { ok: true; value: CleanUpload } | { ok: false; reason: string };

function isRecord(v: unknown): v is Record<string, unknown> {
  return typeof v === "object" && v !== null && !Array.isArray(v);
}

export function validateUpload(raw: unknown): UploadResult {
  if (!isRecord(raw)) return { ok: false, reason: "body_not_object" };
  if (raw.schema !== UPLOAD_SCHEMA) return { ok: false, reason: "unsupported_schema" };
  // Only the desktop app has Send logs. The phone app's declared data does
  // not include logs, so an upload claiming to be from it is refused.
  if (raw.app !== "desktop") return { ok: false, reason: "unsupported_app" };
  if (typeof raw.install_id !== "string" || !UUID_V4.test(raw.install_id)) {
    return { ok: false, reason: "bad_install_id" };
  }
  if (typeof raw.app_version !== "string" || !APP_VERSION.test(raw.app_version)) {
    return { ok: false, reason: "bad_app_version" };
  }
  if (typeof raw.ts !== "string" || !RFC3339.test(raw.ts) || Number.isNaN(Date.parse(raw.ts))) {
    return { ok: false, reason: "bad_ts" };
  }
  const os = clamp(redactShortField(clamp(typeof raw.os === "string" ? raw.os : "", 256)), 40).toLowerCase();
  if (!os || os === "[redacted]") return { ok: false, reason: "bad_os" };
  if (typeof raw.text !== "string" || raw.text.trim() === "") return { ok: false, reason: "bad_text" };
  if (raw.text.length > UPLOAD_MAX_TEXT) return { ok: false, reason: "text_too_large" };
  return {
    ok: true,
    value: {
      install_id: raw.install_id.toLowerCase(),
      app: "desktop",
      app_version: raw.app_version,
      os,
      client_ts: new Date(raw.ts).toISOString(),
      reports_on: typeof raw.reports_enabled === "boolean" ? raw.reports_enabled : null,
      body: redactUploadText(raw.text),
    },
  };
}

// Per install and globally, per UTC day, through the same rate_daily counters
// as events (keys `upload:<install id>` and `upload:*`).
export async function admitUpload(env: Env, installId: string, now: Date): Promise<boolean> {
  const rows: QuotaRow[] = [
    { key: `upload:${installId}`, n: 1, cap: intVar(env.PER_INSTALL_DAILY_UPLOADS, 5) },
    { key: UPLOAD_GLOBAL_KEY, n: 1, cap: intVar(env.GLOBAL_DAILY_UPLOADS, 100) },
  ];
  return admit(env, utcDay(now), rows);
}

export async function storeUpload(env: Env, u: CleanUpload, now: Date): Promise<number> {
  const bytes = new TextEncoder().encode(u.body).length;
  const row = await env.DB.prepare(
    `INSERT INTO diag_uploads (received_at, client_ts, install_id, app, app_version, os, reports_on, bytes, body)
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9) RETURNING id`,
  )
    .bind(
      now.toISOString(),
      u.client_ts,
      u.install_id,
      u.app,
      u.app_version,
      u.os,
      u.reports_on === null ? null : u.reports_on ? 1 : 0,
      bytes,
      u.body,
    )
    .first<{ id: number }>();
  return row?.id ?? 0;
}

// POST /v1/diagnostics: what a user sent with "Send logs to developer"
// (desktop) or "Report a bug" (desktop and, from schema 2, the phone app). A
// redacted summary of their computer or phone and TVs plus the app's own log
// (up to 256 KiB on desktop, 64 KiB from the phone) and, for a bug report,
// the description they typed. Keyed by the install id the app shows as its
// support ID, so a complaint that quotes the ID can be matched to it.
//
// The user pressed a button that says it sends these, so this is accepted
// whether or not their automatic reports are on. The client redacts every
// line (engine/log_redact.rs); redactUploadText is a second, cheaper pass,
// and the note gets redactNote. Kept for LOG_RETENTION_DAYS (30), then
// deleted by the cron.

import type { Env } from "./env";
import { intVar } from "./env";
import { admit, releaseQuota, utcDay, type QuotaRow } from "./ingest";
import { clamp, redactNote, redactShortField, redactUploadText } from "./redact";

// The desktop client sends at most 256 KiB of log and a 16 KiB summary. JSON
// escaping can double that on the wire (a log of quoted adb argument lists
// is mostly quotes), so the body limit leaves room for it, plus a note whose
// 4000 characters escape to at most 24 000 bytes.
export const UPLOAD_MAX_TEXT = 300 * 1024;
export const MOBILE_MAX_TEXT = 64 * 1024;
export const NOTE_MAX_CHARS = 4000;
export const UPLOAD_MAX_BODY = UPLOAD_MAX_TEXT * 2 + 16 * 1024 + 24 * 1024;
export const UPLOAD_GLOBAL_KEY = "upload:*";
export const UPLOAD_BYTES_KEY = "upload-bytes:*";

const UUID_V4 = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
const APP_VERSION = /^\d{1,4}\.\d{1,4}\.\d{1,4}(?:-(?:alpha|beta|rc|preview|pre|dev)(?:\.\d{1,3})?)?$/;
const RFC3339 = /^\d{4}-\d{2}-\d{2}[Tt ]\d{2}:\d{2}:\d{2}(?:\.\d{1,9})?(?:[Zz]|[+-]\d{2}:\d{2})$/;

export type UploadKind = "logs" | "bug";

export interface CleanUpload {
  install_id: string;
  kind: UploadKind;
  app: "desktop" | "mobile";
  app_version: string;
  os: string;
  client_ts: string;
  reports_on: boolean | null;
  // Redacted. A bug report may have only a note, and then body is "".
  body: string;
  note: string | null;
}

export type UploadResult = { ok: true; value: CleanUpload } | { ok: false; reason: string };

function isRecord(v: unknown): v is Record<string, unknown> {
  return typeof v === "object" && v !== null && !Array.isArray(v);
}

export function validateUpload(raw: unknown): UploadResult {
  if (!isRecord(raw)) return { ok: false, reason: "body_not_object" };
  if (raw.schema !== 1 && raw.schema !== 2) return { ok: false, reason: "unsupported_schema" };
  // Schema 1 is desktop Send logs only: the phone app's declared data did not
  // include anything it sends before schema 2's Report a bug.
  const app = raw.app;
  if (app !== "desktop" && (raw.schema === 1 || app !== "mobile")) return { ok: false, reason: "unsupported_app" };
  let kind: UploadKind = "logs";
  if (raw.schema === 2) {
    if (raw.kind !== "logs" && raw.kind !== "bug") return { ok: false, reason: "bad_kind" };
    kind = raw.kind;
  }
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

  let note: string | null = null;
  if (raw.schema === 2 && raw.note !== undefined && raw.note !== null) {
    if (typeof raw.note !== "string") return { ok: false, reason: "bad_note" };
    const trimmed = raw.note.trim();
    if (Array.from(trimmed).length > NOTE_MAX_CHARS) return { ok: false, reason: "note_too_large" };
    const redacted = trimmed === "" ? "" : redactNote(trimmed);
    if (redacted !== "") note = redacted;
  }

  const textGiven = raw.text !== undefined && raw.text !== null;
  if (textGiven && typeof raw.text !== "string") return { ok: false, reason: "bad_text" };
  const text = typeof raw.text === "string" ? raw.text : "";
  if (text.length > (app === "mobile" ? MOBILE_MAX_TEXT : UPLOAD_MAX_TEXT)) return { ok: false, reason: "text_too_large" };
  const hasText = text.trim() !== "";
  if (kind === "logs" && !hasText) return { ok: false, reason: "bad_text" };
  if (kind === "bug" && !hasText && note === null) return { ok: false, reason: "empty_report" };
  return {
    ok: true,
    value: {
      install_id: raw.install_id.toLowerCase(),
      kind,
      app,
      app_version: raw.app_version,
      os,
      client_ts: new Date(raw.ts).toISOString(),
      reports_on: typeof raw.reports_enabled === "boolean" ? raw.reports_enabled : null,
      body: hasText ? redactUploadText(text) : "",
      note,
    },
  };
}

// Per install and globally, per UTC day, through the same rate_daily counters
// as events (keys `upload:<install id>` and `upload:*`).
export type UploadAdmission = "ok" | "install" | "global";

async function installCount(env: Env, day: string, key: string): Promise<number> {
  const row = await env.DB.prepare("SELECT events FROM rate_daily WHERE day = ?1 AND key = ?2")
    .bind(day, key)
    .first<{ events: number }>();
  return row?.events ?? 0;
}

// The per-install cap is checked first, with a read: an install over its own
// five a day is refused before it can touch the shared budget, so one person
// retrying can't use up anyone else's. Then one statement admits against all
// three counters or none: this install, the global count and the global bytes
// (6 MiB a day kept 30 days is up to about 189 MB; with everything else at its
// worst that can pass the storage budget, and the cron's oldest-first trim
// then holds it there; README "Storage budget").
export async function admitUpload(env: Env, installId: string, bytes: number, now: Date): Promise<UploadAdmission> {
  const day = utcDay(now);
  const key = `upload:${installId}`;
  const installCap = intVar(env.PER_INSTALL_DAILY_UPLOADS, 5);
  if ((await installCount(env, day, key)) + 1 > installCap) return "install";
  const rows: QuotaRow[] = [
    { key, n: 1, cap: installCap },
    { key: UPLOAD_GLOBAL_KEY, n: 1, cap: intVar(env.GLOBAL_DAILY_UPLOADS, 200) },
    { key: UPLOAD_BYTES_KEY, n: bytes, cap: intVar(env.GLOBAL_DAILY_UPLOAD_BYTES, 6 * 1024 * 1024) },
  ];
  if (await admit(env, day, rows)) return "ok";
  // Refused: a concurrent upload from the same install took the last slot,
  // or the shared budget is spent.
  return (await installCount(env, day, key)) + 1 > installCap ? "install" : "global";
}

// The text and the note, as counted against GLOBAL_DAILY_UPLOAD_BYTES and
// stored in `bytes`, which the storage budget estimates uploads by.
export function uploadBytes(u: CleanUpload): number {
  const encoder = new TextEncoder();
  return encoder.encode(u.body).length + encoder.encode(u.note ?? "").length;
}

// Gives an admitted upload's per-install count and its bytes back when
// storing it failed, so storage trouble can't lock a user out of Send logs
// for the day or spend the shared byte budget on nothing. As with events, the
// global count is not refunded: it bounds retries if storage stays broken.
// Best effort.
export async function releaseUpload(env: Env, installId: string, bytes: number, now: Date): Promise<void> {
  await releaseQuota(
    env,
    [
      { key: `upload:${installId}`, n: 1, cap: 0 },
      { key: UPLOAD_BYTES_KEY, n: bytes, cap: 0 },
    ],
    now,
  );
}

export async function storeUpload(env: Env, u: CleanUpload, now: Date): Promise<number> {
  const bytes = uploadBytes(u);
  const row = await env.DB.prepare(
    `INSERT INTO diag_uploads (received_at, client_ts, install_id, app, app_version, os, reports_on, bytes, body, kind, note)
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11) RETURNING id`,
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
      u.kind,
      u.note,
    )
    .first<{ id: number }>();
  return row?.id ?? 0;
}

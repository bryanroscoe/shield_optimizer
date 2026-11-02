// Admin support lookup: everything stored for one install, found by the
// support ID a user quotes. The app shows the short form (`7D0B-0B9E`, the
// first eight hex digits); the full UUID is in the bug-report bundle and in
// Send logs. Both are accepted, with or without the dash, in any case.

import type { Env } from "./env";

const UUID_V4 = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
const SHORT = /^[0-9a-f]{4}-?[0-9a-f]{4}$/;
const MAX_INSTALLS = 5;
export const DEFAULT_EVENT_LIMIT = 50;
export const MAX_EVENT_LIMIT = 200;
const UPLOAD_LIMIT = 20;

export type SupportQuery = { kind: "full"; id: string } | { kind: "prefix"; prefix: string };

export function parseSupportQuery(raw: string): SupportQuery | null {
  const q = raw.trim().toLowerCase();
  if (UUID_V4.test(q)) return { kind: "full", id: q };
  if (SHORT.test(q)) return { kind: "prefix", prefix: q.replace("-", "") };
  return null;
}

// Ids are stored lower case and every hex digit and "-" sorts before "g",
// so the ids starting with a prefix (or equal to a full id, since they all
// have the same length) are exactly the range [start, start + "g").
function range(q: SupportQuery): [string, string] {
  const start = q.kind === "full" ? q.id : q.prefix;
  return [start, `${start}g`];
}

export interface SupportEvent {
  id: number;
  received_at: string;
  client_ts: string;
  app: string;
  app_version: string;
  os: string;
  kind: string;
  device: Record<string, unknown> | null;
  action: string | null;
  ok: boolean | null;
  error_code: string | null;
  message: string | null;
  diag: unknown;
  log_excerpt: string | null;
}

export interface SupportUpload {
  id: number;
  received_at: string;
  client_ts: string;
  app_version: string;
  os: string;
  reports_on: boolean | null;
  bytes: number;
}

export interface SupportInstall {
  install_id: string;
  short: string;
  first_seen: string | null;
  last_seen: string | null;
  events: number;
  kinds: Record<string, number>;
  apps: string[];
  app_versions: string[];
  os: string[];
  devices: Array<{ manufacturer: string; model: string; android_release: string | null; profile: string | null; events: number }>;
  error_codes: Array<{ code: string; count: number }>;
  recent: SupportEvent[];
  uploads: SupportUpload[];
}

export interface SupportReport {
  query: string;
  matches: number;
  truncated: boolean;
  installs: SupportInstall[];
}

export function shortId(id: string): string {
  const hex = id.replace(/[^0-9a-f]/gi, "").slice(0, 8).toUpperCase();
  return hex.length === 8 ? `${hex.slice(0, 4)}-${hex.slice(4)}` : "";
}

function parseJson(text: unknown): unknown {
  if (typeof text !== "string") return null;
  try {
    return JSON.parse(text);
  } catch {
    return null;
  }
}

type Row = Record<string, unknown>;

export async function supportLookup(env: Env, q: SupportQuery, eventLimit: number): Promise<SupportReport> {
  const [lo, hi] = range(q);
  const ids = await env.DB.prepare(
    `SELECT install_id, MAX(last) AS last FROM (
       SELECT install_id, MAX(received_at) AS last FROM events WHERE install_id >= ?1 AND install_id < ?2 GROUP BY install_id
       UNION ALL
       SELECT install_id, MAX(received_at) AS last FROM diag_uploads WHERE install_id >= ?1 AND install_id < ?2 GROUP BY install_id
     ) GROUP BY install_id ORDER BY last DESC LIMIT ?3`,
  )
    .bind(lo, hi, MAX_INSTALLS + 1)
    .all<{ install_id: string }>();
  const all = ids.results.map((r) => r.install_id);
  const installs: SupportInstall[] = [];
  for (const id of all.slice(0, MAX_INSTALLS)) installs.push(await installReport(env, id, eventLimit));
  return {
    query: q.kind === "full" ? q.id : shortId(q.prefix),
    matches: all.length,
    truncated: all.length > MAX_INSTALLS,
    installs,
  };
}

async function installReport(env: Env, id: string, eventLimit: number): Promise<SupportInstall> {
  const [summary, kinds, versions, devices, codes, recent, uploads] = await env.DB.batch([
    env.DB.prepare(
      "SELECT MIN(received_at) AS first_seen, MAX(received_at) AS last_seen, COUNT(*) AS events FROM events WHERE install_id = ?1",
    ).bind(id),
    env.DB.prepare("SELECT kind, COUNT(*) AS n FROM events WHERE install_id = ?1 GROUP BY kind").bind(id),
    env.DB.prepare("SELECT DISTINCT app, app_version, os FROM events WHERE install_id = ?1").bind(id),
    env.DB.prepare(
      `SELECT manufacturer, model, android_release, profile, COUNT(*) AS events FROM events
       WHERE install_id = ?1 AND manufacturer IS NOT NULL
       GROUP BY manufacturer, model, android_release, profile ORDER BY events DESC LIMIT 20`,
    ).bind(id),
    env.DB.prepare(
      `SELECT error_code AS code, COUNT(*) AS count FROM events
       WHERE install_id = ?1 AND error_code IS NOT NULL GROUP BY error_code ORDER BY count DESC LIMIT 20`,
    ).bind(id),
    env.DB.prepare(
      `SELECT id, received_at, client_ts, app, app_version, os, kind, manufacturer, brand, model, device, product,
              android_release, sdk, profile, support_tier, action, ok, error_code, message, diag, log_excerpt
       FROM events WHERE install_id = ?1 ORDER BY received_at DESC, id DESC LIMIT ?2`,
    ).bind(id, eventLimit),
    env.DB.prepare(
      `SELECT id, received_at, client_ts, app_version, os, reports_on, bytes FROM diag_uploads
       WHERE install_id = ?1 ORDER BY received_at DESC, id DESC LIMIT ?2`,
    ).bind(id, UPLOAD_LIMIT),
  ]);
  const s = (summary?.results?.[0] ?? {}) as Row;
  const v = (versions?.results ?? []) as Row[];
  const uniq = (key: string) => [...new Set(v.map((r) => String(r[key])))];
  const uploadRows = ((uploads?.results ?? []) as Row[]).map(
    (r): SupportUpload => ({
      id: Number(r.id),
      received_at: String(r.received_at),
      client_ts: String(r.client_ts),
      app_version: String(r.app_version),
      os: String(r.os),
      reports_on: r.reports_on === null || r.reports_on === undefined ? null : r.reports_on === 1,
      bytes: Number(r.bytes),
    }),
  );
  return {
    install_id: id,
    short: shortId(id),
    first_seen: (s.first_seen as string | null) ?? null,
    last_seen: (s.last_seen as string | null) ?? null,
    events: Number(s.events ?? 0),
    kinds: Object.fromEntries(((kinds?.results ?? []) as Row[]).map((r) => [String(r.kind), Number(r.n)])),
    apps: uniq("app"),
    app_versions: uniq("app_version"),
    os: uniq("os"),
    devices: ((devices?.results ?? []) as Row[]).map((r) => ({
      manufacturer: String(r.manufacturer ?? ""),
      model: String(r.model ?? ""),
      android_release: (r.android_release as string | null) ?? null,
      profile: (r.profile as string | null) ?? null,
      events: Number(r.events),
    })),
    error_codes: ((codes?.results ?? []) as Row[]).map((r) => ({ code: String(r.code), count: Number(r.count) })),
    recent: ((recent?.results ?? []) as Row[]).map(
      (r): SupportEvent => ({
        id: Number(r.id),
        received_at: String(r.received_at),
        client_ts: String(r.client_ts),
        app: String(r.app),
        app_version: String(r.app_version),
        os: String(r.os),
        kind: String(r.kind),
        device:
          r.manufacturer === null || r.manufacturer === undefined
            ? null
            : {
                manufacturer: r.manufacturer,
                brand: r.brand,
                model: r.model,
                device: r.device,
                product: r.product,
                android_release: r.android_release,
                sdk: r.sdk,
                profile: r.profile,
                support_tier: r.support_tier,
              },
        action: (r.action as string | null) ?? null,
        ok: r.ok === null || r.ok === undefined ? null : r.ok === 1,
        error_code: (r.error_code as string | null) ?? null,
        message: (r.message as string | null) ?? null,
        diag: parseJson(r.diag),
        log_excerpt: (r.log_excerpt as string | null) ?? null,
      }),
    ),
    uploads: uploadRows,
  };
}

export async function uploadText(env: Env, id: number): Promise<{ install_id: string; received_at: string; body: string } | null> {
  return env.DB.prepare("SELECT install_id, received_at, body FROM diag_uploads WHERE id = ?1")
    .bind(id)
    .first<{ install_id: string; received_at: string; body: string }>();
}

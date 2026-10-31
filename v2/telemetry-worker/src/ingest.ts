import type { Env } from "./env";
import { intVar } from "./env";
import type { CleanEvent } from "./validate";

// Every write uses json_each over one bound JSON parameter, so a 50-event batch
// costs at most six D1 queries. That matters on the free plan, which caps a
// Worker invocation at 50 D1 queries and a statement at 100 bound parameters.

export interface QuotaRow {
  key: string;
  n: number;
  cap: number;
}

export type QuotaResult = { ok: true; rows: QuotaRow[] } | { ok: false; scope: "install" | "global" };

export const GLOBAL_KEY = "*";

export function utcDay(now: Date): string {
  return now.toISOString().slice(0, 10);
}

// Daily caps as counters in D1 (`rate_daily`: one row per install id, plus '*'
// for the global total). Admission and increment are one statement: its
// uncorrelated NOT EXISTS checks every counter the batch would touch against
// the state before the statement, and D1 runs one write at a time, so either
// every counter moves or none does, and concurrent requests cannot both slip
// under a cap. A refused request writes nothing; retry loops and floods of
// made-up install ids cost row reads, not the much smaller write quota.
export async function consumeQuota(env: Env, events: CleanEvent[], now: Date): Promise<QuotaResult> {
  const day = utcDay(now);
  const installCap = intVar(env.PER_INSTALL_DAILY_EVENTS, 500);
  const globalCap = intVar(env.GLOBAL_DAILY_EVENTS, 7000);
  const perInstall = new Map<string, number>();
  for (const e of events) perInstall.set(e.install_id, (perInstall.get(e.install_id) ?? 0) + 1);
  const rows: QuotaRow[] = [...perInstall].map(([key, n]) => ({ key, n, cap: installCap }));
  rows.push({ key: GLOBAL_KEY, n: events.length, cap: globalCap });

  const admitted = await env.DB.prepare(
    `INSERT INTO rate_daily (day, key, events)
     SELECT ?1, json_extract(value, '$.key'), json_extract(value, '$.n') FROM json_each(?2)
     WHERE NOT EXISTS (
       SELECT 1 FROM json_each(?2) AS j
       LEFT JOIN rate_daily AS r ON r.day = ?1 AND r.key = json_extract(j.value, '$.key')
       WHERE COALESCE(r.events, 0) + json_extract(j.value, '$.n') > json_extract(j.value, '$.cap')
     )
     ON CONFLICT (day, key) DO UPDATE SET events = events + excluded.events
     RETURNING key`,
  )
    .bind(day, JSON.stringify(rows))
    .all();
  if (admitted.results.length === rows.length) return { ok: true, rows };

  // Refused: a read to say which cap, for the response body.
  const global = await env.DB.prepare("SELECT events FROM rate_daily WHERE day = ?1 AND key = ?2")
    .bind(day, GLOBAL_KEY)
    .first<{ events: number }>();
  return { ok: false, scope: (global?.events ?? 0) + events.length > globalCap ? "global" : "install" };
}

// Gives back an admitted batch's quota when storing it failed, so a transient
// D1 error does not use up the day's cap for events that were never recorded.
// Best effort: if D1 is down this fails too, and the request already errors.
export async function releaseQuota(env: Env, rows: QuotaRow[], now: Date): Promise<void> {
  await env.DB.prepare(
    `UPDATE rate_daily
        SET events = max(0, events - (SELECT json_extract(j.value, '$.n') FROM json_each(?2) AS j
                                       WHERE json_extract(j.value, '$.key') = rate_daily.key))
      WHERE day = ?1 AND key IN (SELECT json_extract(value, '$.key') FROM json_each(?2))`,
  )
    .bind(utcDay(now), JSON.stringify(rows))
    .run();
}

interface Dims {
  manufacturer: string;
  model: string;
  device: string;
  profile: string;
  app: string;
  app_version: string;
}

function dimsOf(e: CleanEvent): Dims {
  return {
    manufacturer: e.device?.manufacturer ?? "",
    model: e.device?.model ?? "",
    device: e.device?.device ?? "",
    profile: e.device?.profile ?? "",
    app: e.app,
    app_version: e.app_version,
  };
}

const DIM_COLS = "manufacturer, model, device, profile, app, app_version";
const DIM_SELECT = DIM_COLS.split(", ")
  .map((c) => `json_extract(value, '$.${c}')`)
  .join(", ");

export async function storeEvents(
  env: Env,
  events: CleanEvent[],
  country: string | null,
  now: Date,
): Promise<void> {
  const receivedAt = now.toISOString();
  const day = utcDay(now);

  const raw = events.map((e) => ({
    client_ts: e.client_ts,
    install_id: e.install_id,
    app: e.app,
    app_version: e.app_version,
    os: e.os,
    kind: e.kind,
    manufacturer: e.device?.manufacturer ?? null,
    brand: e.device?.brand ?? null,
    model: e.device?.model ?? null,
    device: e.device?.device ?? null,
    product: e.device?.product ?? null,
    android_release: e.device?.android_release || null,
    sdk: e.device?.sdk ?? null,
    profile: e.device?.profile ?? null,
    support_tier: e.device?.support_tier ?? null,
    action: e.action,
    ok: e.ok === null ? null : e.ok ? 1 : 0,
    error_code: e.error_code,
    message: e.message,
  }));
  const rawCols = Object.keys(raw[0] ?? {});

  const daily = new Map<string, Dims & { events: number; seen: number; action_ok: number; action_fail: number; errors: number }>();
  const installs = new Map<string, Dims & { install_id: string }>();
  const errors = new Map<string, Dims & { error_code: string; count: number }>();
  for (const e of events) {
    const dims = dimsOf(e);
    const key = JSON.stringify(dims);
    const d = daily.get(key) ?? { ...dims, events: 0, seen: 0, action_ok: 0, action_fail: 0, errors: 0 };
    d.events += 1;
    if (e.kind === "device_seen") d.seen += 1;
    if (e.kind === "action_result" && e.ok === true) d.action_ok += 1;
    if (e.kind === "action_result" && e.ok === false) d.action_fail += 1;
    if (e.kind === "error") d.errors += 1;
    daily.set(key, d);
    installs.set(`${key}|${e.install_id}`, { ...dims, install_id: e.install_id });
    if (e.error_code && (e.kind === "error" || e.ok === false)) {
      const ekey = `${key}|${e.error_code}`;
      const x = errors.get(ekey) ?? { ...dims, error_code: e.error_code, count: 0 };
      x.count += 1;
      errors.set(ekey, x);
    }
  }

  const statements = [
    env.DB.prepare(
      `INSERT INTO events (received_at, country, ${rawCols.join(", ")})
       SELECT ?1, ?2, ${rawCols.map((c) => `json_extract(value, '$.${c}')`).join(", ")} FROM json_each(?3)`,
    ).bind(receivedAt, country, JSON.stringify(raw)),
    env.DB.prepare(
      `INSERT INTO agg_device_daily (day, ${DIM_COLS}, events, seen, action_ok, action_fail, errors, last_seen)
       SELECT ?1, ${DIM_SELECT}, json_extract(value, '$.events'), json_extract(value, '$.seen'),
              json_extract(value, '$.action_ok'), json_extract(value, '$.action_fail'),
              json_extract(value, '$.errors'), ?2
       FROM json_each(?3) WHERE true
       ON CONFLICT (day, ${DIM_COLS}) DO UPDATE SET
         events = events + excluded.events,
         seen = seen + excluded.seen,
         action_ok = action_ok + excluded.action_ok,
         action_fail = action_fail + excluded.action_fail,
         errors = errors + excluded.errors,
         last_seen = max(last_seen, excluded.last_seen)`,
    ).bind(day, receivedAt, JSON.stringify([...daily.values()])),
    env.DB.prepare(
      `INSERT INTO agg_device_installs (${DIM_COLS}, install_id, first_seen, last_seen)
       SELECT ${DIM_SELECT}, json_extract(value, '$.install_id'), ?1, ?1
       FROM json_each(?2) WHERE true
       ON CONFLICT (${DIM_COLS}, install_id) DO UPDATE SET last_seen = max(last_seen, excluded.last_seen)`,
    ).bind(receivedAt, JSON.stringify([...installs.values()])),
  ];
  if (errors.size > 0) {
    statements.push(
      env.DB.prepare(
        `INSERT INTO agg_error_daily (day, ${DIM_COLS}, error_code, count)
         SELECT ?1, ${DIM_SELECT}, json_extract(value, '$.error_code'), json_extract(value, '$.count')
         FROM json_each(?2) WHERE true
         ON CONFLICT (day, ${DIM_COLS}, error_code) DO UPDATE SET count = count + excluded.count`,
      ).bind(day, JSON.stringify([...errors.values()])),
    );
  }
  await env.DB.batch(statements);
}

export async function purgeExpired(env: Env, now: Date): Promise<{ events: number; rate: number }> {
  const days = intVar(env.RAW_RETENTION_DAYS, 90);
  const cutoff = new Date(now.getTime() - days * 86_400_000).toISOString();
  const rateCutoff = utcDay(new Date(now.getTime() - 2 * 86_400_000));
  const [ev, rate] = await env.DB.batch([
    env.DB.prepare("DELETE FROM events WHERE received_at < ?1").bind(cutoff),
    env.DB.prepare("DELETE FROM rate_daily WHERE day < ?1").bind(rateCutoff),
  ]);
  return { events: ev?.meta.changes ?? 0, rate: rate?.meta.changes ?? 0 };
}

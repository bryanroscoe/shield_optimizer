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
// Stored bytes of accepted events, across everyone, per UTC day.
export const EVENT_BYTES_KEY = "event-bytes:*";

// What one events row costs in storage, as counted against
// GLOBAL_DAILY_EVENT_BYTES and estimated by the storage-budget purge: a fixed
// allowance for the row, its timestamps and its two index entries, plus every
// stored string. The SQL twin is EVENT_SIZE_SQL below; keep them in step.
export const EVENT_ROW_OVERHEAD = 256;
const encoder = new TextEncoder();

export function eventBytes(e: CleanEvent): number {
  let n = EVENT_ROW_OVERHEAD;
  const strings: Array<string | null> = [
    e.install_id, e.app, e.app_version, e.os, e.client_ts, e.kind, e.action, e.error_code, e.message, e.diag, e.log_excerpt,
  ];
  if (e.device) for (const v of Object.values(e.device)) if (typeof v === "string") strings.push(v);
  for (const s of strings) if (s) n += encoder.encode(s).length;
  return n;
}

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
  const byteCap = intVar(env.GLOBAL_DAILY_EVENT_BYTES, DEFAULT_DAILY_EVENT_BYTES);
  const bytes = events.reduce((a, e) => a + eventBytes(e), 0);
  const rows: QuotaRow[] = [...perInstall].map(([key, n]) => ({ key, n, cap: installCap }));
  rows.push({ key: GLOBAL_KEY, n: events.length, cap: globalCap });
  rows.push({ key: EVENT_BYTES_KEY, n: bytes, cap: byteCap });

  const admitted = await admit(env, day, rows);
  if (admitted) return { ok: true, rows };

  // Refused: a read to say which cap, for the response body.
  const counters = await env.DB.prepare("SELECT key, events FROM rate_daily WHERE day = ?1 AND key IN (?2, ?3)")
    .bind(day, GLOBAL_KEY, EVENT_BYTES_KEY)
    .all<{ key: string; events: number }>();
  const used = (key: string) => counters.results.find((r) => r.key === key)?.events ?? 0;
  const global = used(GLOBAL_KEY) + events.length > globalCap || used(EVENT_BYTES_KEY) + bytes > byteCap;
  return { ok: false, scope: global ? "global" : "install" };
}

// 1.5 MiB a day kept RAW_RETENTION_DAYS (90) is at most ~142 MB of events.
// See "Storage budget" in README.md before raising it.
export const DEFAULT_DAILY_EVENT_BYTES = 1536 * 1024;

// Bumps every counter in `rows` by its `n`, or none of them when any would
// pass its `cap`. One statement; see consumeQuota.
export async function admit(env: Env, day: string, rows: QuotaRow[]): Promise<boolean> {
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
  return admitted.results.length === rows.length;
}

// Gives back an admitted batch's per-install quota (and its bytes) when
// storing it failed, so a transient D1 error does not lock an install out for
// events that were never recorded. The global counter is deliberately not refunded: it is the failure
// budget that bounds admission+refund writes if storage stays broken while
// rate_daily is still writable. Best effort: if D1 is down this fails too.
export async function releaseQuota(env: Env, rows: QuotaRow[], now: Date): Promise<void> {
  await env.DB.prepare(
    `UPDATE rate_daily
        SET events = max(0, events - (SELECT json_extract(j.value, '$.n') FROM json_each(?2) AS j
                                       WHERE json_extract(j.value, '$.key') = rate_daily.key))
      WHERE day = ?1 AND key IN (SELECT json_extract(value, '$.key') FROM json_each(?2))`,
  )
    .bind(utcDay(now), JSON.stringify(rows.filter((r) => r.key !== GLOBAL_KEY)))
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
    diag: e.diag,
    log_excerpt: e.log_excerpt,
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
    if (e.error_code && (e.kind === "error" || (e.kind === "action_result" && e.ok === false))) {
      const ekey = `${key}|${e.error_code}`;
      const x = errors.get(ekey) ?? { ...dims, error_code: e.error_code, count: 0 };
      x.count += 1;
      errors.set(ekey, x);
    }
  }

  const statements = [
    env.DB.prepare(
      `INSERT INTO events (received_at, ${rawCols.join(", ")})
       SELECT ?1, ${rawCols.map((c) => `json_extract(value, '$.${c}')`).join(", ")} FROM json_each(?2)`,
    ).bind(receivedAt, JSON.stringify(raw)),
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

export interface PurgeResult {
  events: number;
  rate: number;
  excerpts: number;
  uploads: number;
  storage: StorageTrim;
}

// Raw events go after RAW_RETENTION_DAYS (90). Log text is kept for less:
// excerpts are cleared from their events, and Send logs uploads deleted,
// after LOG_RETENTION_DAYS (30). Then the storage budget is enforced.
export async function purgeExpired(env: Env, now: Date): Promise<PurgeResult> {
  const days = intVar(env.RAW_RETENTION_DAYS, 90);
  const cutoff = new Date(now.getTime() - days * 86_400_000).toISOString();
  const logDays = intVar(env.LOG_RETENTION_DAYS, 30);
  const logCutoff = new Date(now.getTime() - logDays * 86_400_000).toISOString();
  const rateCutoff = utcDay(new Date(now.getTime() - 2 * 86_400_000));
  const [ev, rate, excerpts, uploads] = await env.DB.batch([
    env.DB.prepare("DELETE FROM events WHERE received_at < ?1").bind(cutoff),
    env.DB.prepare("DELETE FROM rate_daily WHERE day < ?1").bind(rateCutoff),
    env.DB.prepare("UPDATE events SET log_excerpt = NULL WHERE log_excerpt IS NOT NULL AND received_at < ?1").bind(
      logCutoff,
    ),
    env.DB.prepare("DELETE FROM diag_uploads WHERE received_at < ?1").bind(logCutoff),
  ]);
  return {
    events: ev?.meta.changes ?? 0,
    rate: rate?.meta.changes ?? 0,
    excerpts: excerpts?.meta.changes ?? 0,
    uploads: uploads?.meta.changes ?? 0,
    storage: await enforceStorageBudget(env),
  };
}

// The storage budget. A Workers Free D1 database stops taking writes at 500 MB,
// after which every ingest, upload and tier sync answers 503. The daily caps
// bound most of what is stored (README "Storage budget"), but the aggregate
// tables are kept forever and a flood of made-up device names grows them
// without limit. So when the estimated stored bytes pass STORAGE_BUDGET_BYTES,
// the cron frees space, oldest first, towards 80% of it: Send logs uploads,
// then log excerpts, then raw events, and only then the oldest aggregate days.
// Sizes are estimates (string bytes plus a per-row allowance), not the database
// file size, which SQLite does not shrink on delete; freed pages are reused by
// later writes.
//
// The trim must not cause the outage it prevents: the Free plan also allows
// only 100k rows written and 5M rows read per day, and the event caps already
// spend most of the writes under a flood. So the estimate (a read of every
// row) runs once per cron, and each run deletes at most
// STORAGE_TRIM_MAX_ROW_WRITES row writes' worth (index entries included),
// in capped oldest-first batches. A large overshoot converges over several
// days instead of in one burst: a run that hit the cap resumes the next day
// for as long as the estimate is still over the budget.
export const DEFAULT_STORAGE_BUDGET_BYTES = 300_000_000;
export const DEFAULT_STORAGE_TRIM_MAX_ROW_WRITES = 20_000;
const TARGET_FRACTION = 0.8;

const textBytes = (cols: string[]) => `LENGTH(CAST(${cols.map((c) => `COALESCE(${c}, '')`).join(" || ")} AS BLOB))`;
const EVENT_SIZE_SQL = `${EVENT_ROW_OVERHEAD} + ${textBytes([
  "install_id", "app", "app_version", "os", "client_ts", "kind", "manufacturer", "brand", "model", "device", "product",
  "android_release", "profile", "support_tier", "action", "error_code", "message", "diag", "log_excerpt",
])}`;
const AGG_ROW_OVERHEAD = 128;
const AGG_DIMS = ["manufacturer", "model", "device", "profile", "app", "app_version"];
const UPLOAD_SIZE_SQL = `bytes + ${EVENT_ROW_OVERHEAD}`;

interface Tier {
  name: string;
  table: string;
  // Rows are freed in ascending order of this column (ties by key).
  col: string;
  // What identifies a row: rowid, or the primary key of a WITHOUT ROWID table.
  key: string;
  size: string;
  filter?: string;
  // Row writes per freed row: the row plus each index entry it changes.
  writesPerRow: number;
  // The statement that frees the rows whose key is in `keys`.
  free: (keys: string) => string;
}

const remove = (table: string, key: string) => (keys: string) => `DELETE FROM ${table} WHERE ${key} IN (${keys})`;
const AGG_DAILY_PK = `(${["day", ...AGG_DIMS].join(", ")})`;
const AGG_ERROR_PK = `(${["day", ...AGG_DIMS, "error_code"].join(", ")})`;
const AGG_INSTALLS_PK = `(${[...AGG_DIMS, "install_id"].join(", ")})`;

const TIERS: Tier[] = [
  // The row plus diag_uploads_install and diag_uploads_received_at.
  { name: "uploads", table: "diag_uploads", col: "received_at", key: "rowid", size: UPLOAD_SIZE_SQL, writesPerRow: 3, free: remove("diag_uploads", "rowid") },
  {
    name: "excerpts",
    table: "events",
    col: "received_at",
    key: "rowid",
    size: "LENGTH(CAST(log_excerpt AS BLOB))",
    filter: "log_excerpt IS NOT NULL",
    // log_excerpt is not indexed.
    writesPerRow: 1,
    free: (keys) => `UPDATE events SET log_excerpt = NULL WHERE rowid IN (${keys})`,
  },
  // The row plus events_received_at and events_install.
  { name: "events", table: "events", col: "received_at", key: "rowid", size: EVENT_SIZE_SQL, writesPerRow: 3, free: remove("events", "rowid") },
  // WITHOUT ROWID tables with no secondary index: one write per row.
  {
    name: "agg_error_daily",
    table: "agg_error_daily",
    col: "day",
    key: AGG_ERROR_PK,
    size: `${AGG_ROW_OVERHEAD} + ${textBytes(["day", ...AGG_DIMS, "error_code"])}`,
    writesPerRow: 1,
    free: remove("agg_error_daily", AGG_ERROR_PK),
  },
  {
    name: "agg_device_daily",
    table: "agg_device_daily",
    col: "day",
    key: AGG_DAILY_PK,
    size: `${AGG_ROW_OVERHEAD} + ${textBytes(["day", ...AGG_DIMS, "last_seen"])}`,
    writesPerRow: 1,
    free: remove("agg_device_daily", AGG_DAILY_PK),
  },
  {
    name: "agg_device_installs",
    table: "agg_device_installs",
    col: "last_seen",
    key: AGG_INSTALLS_PK,
    size: `${AGG_ROW_OVERHEAD} + ${textBytes([...AGG_DIMS, "install_id", "first_seen", "last_seen"])}`,
    writesPerRow: 1,
    free: remove("agg_device_installs", AGG_INSTALLS_PK),
  },
];

const sum = (size: string, table: string, filter?: string) =>
  `(SELECT COALESCE(SUM(${size}), 0) FROM ${table}${filter ? ` WHERE ${filter}` : ""})`;

// One query; reads every row once.
export async function estimateStorageBytes(env: Env): Promise<number> {
  const parts = [
    sum(UPLOAD_SIZE_SQL, "diag_uploads"),
    sum(EVENT_SIZE_SQL, "events"),
    ...TIERS.filter((t) => t.table.startsWith("agg_")).map((t) => sum(t.size, t.table)),
    sum(`64 + ${textBytes(["day", "key"])}`, "rate_daily"),
  ];
  const row = await env.DB.prepare(`SELECT ${parts.join(" + ")} AS total`).first<{ total: number }>();
  return row?.total ?? 0;
}


export interface StorageTrim {
  before: number;
  // Estimated: `before` minus the bytes of the rows freed this run.
  after: number;
  budget: number;
  trimmed: string[];
  rowWrites: number;
  // The per-run write budget ran out before reaching the target; the next
  // cron carries on.
  capped: boolean;
}

export async function enforceStorageBudget(env: Env): Promise<StorageTrim> {
  const budget = intVar(env.STORAGE_BUDGET_BYTES, DEFAULT_STORAGE_BUDGET_BYTES);
  const maxWrites = intVar(env.STORAGE_TRIM_MAX_ROW_WRITES, DEFAULT_STORAGE_TRIM_MAX_ROW_WRITES);
  const before = await estimateStorageBytes(env);
  const result: StorageTrim = { before, after: before, budget, trimmed: [], rowWrites: 0, capped: false };
  if (before <= budget) return result;
  const target = Math.floor(budget * TARGET_FRACTION);
  for (const tier of TIERS) {
    const excess = result.after - target;
    if (excess <= 0) break;
    const rowsLeft = Math.floor((maxWrites - result.rowWrites) / tier.writesPerRow);
    if (rowsLeft <= 0) {
      result.capped = true;
      break;
    }
    // The oldest rows, at most what the write budget allows, and of those
    // only as many as cover the excess. The same ordering picks them for the
    // free statement, so it frees exactly the rows counted here.
    const keyCols = tier.key === "rowid" ? "rowid" : tier.key.slice(1, -1);
    const oldest = (select: string) =>
      `SELECT ${select} FROM ${tier.table}${tier.filter ? ` WHERE ${tier.filter}` : ""} ORDER BY ${tier.col}, ${keyCols} LIMIT ?1`;
    const sizes = await env.DB.prepare(oldest(`${tier.size} AS b`)).bind(rowsLeft).all<{ b: number }>();
    let rows = 0;
    let freed = 0;
    for (const { b } of sizes.results) {
      if (freed >= excess) break;
      rows += 1;
      freed += b;
    }
    if (rows === 0) continue;
    const res = await env.DB.prepare(tier.free(oldest(keyCols))).bind(rows).run();
    result.rowWrites += (res.meta.changes ?? rows) * tier.writesPerRow;
    result.after -= freed;
    result.trimmed.push(tier.name);
    if (freed < excess && rows === rowsLeft) {
      result.capped = true;
      break;
    }
  }
  return result;
}

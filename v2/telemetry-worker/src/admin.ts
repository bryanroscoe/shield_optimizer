import type { Env } from "./env";

const MIN_TOKEN_LENGTH = 16;

async function sha256(text: string): Promise<ArrayBuffer> {
  return crypto.subtle.digest("SHA-256", new TextEncoder().encode(text));
}

export type AuthResult = "ok" | "unauthorized" | "disabled";

// Compares SHA-256 digests in constant time so neither length nor content of
// the token leaks through timing.
export async function checkAdmin(request: Request, env: Env): Promise<AuthResult> {
  const expected = env.ADMIN_TOKEN ?? "";
  if (expected.length < MIN_TOKEN_LENGTH) return "disabled";
  const header = request.headers.get("authorization") ?? "";
  const match = /^Bearer\s+(.+)$/i.exec(header);
  if (!match?.[1]) return "unauthorized";
  const [a, b] = await Promise.all([sha256(match[1].trim()), sha256(expected)]);
  return crypto.subtle.timingSafeEqual(a, b) ? "ok" : "unauthorized";
}

export const GROUP_COLUMNS = ["manufacturer", "model", "device", "profile", "app", "app_version"] as const;
type GroupColumn = (typeof GROUP_COLUMNS)[number];

export interface DeviceReportRow {
  [dim: string]: unknown;
  installs: number;
  events: number;
  seen: number;
  action_ok: number;
  action_fail: number;
  errors: number;
  top_errors: Array<{ code: string; count: number }>;
  last_seen: string | null;
}

export function parseGroup(param: string | null): GroupColumn[] | null {
  if (param === null || param.trim() === "") return [...GROUP_COLUMNS];
  const cols = param.split(",").map((s) => s.trim());
  if (cols.length === 0 || cols.some((c) => !(GROUP_COLUMNS as readonly string[]).includes(c))) return null;
  // Keep canonical order and drop duplicates; only whitelisted names reach SQL.
  return GROUP_COLUMNS.filter((c) => cols.includes(c));
}

export async function deviceReport(env: Env, group: GroupColumn[], sinceDay: string): Promise<DeviceReportRow[]> {
  const g = group.join(", ");
  const [totals, installs, errors] = await env.DB.batch([
    env.DB.prepare(
      `SELECT ${g}, SUM(events) AS events, SUM(seen) AS seen, SUM(action_ok) AS action_ok,
              SUM(action_fail) AS action_fail, SUM(errors) AS errors, MAX(last_seen) AS last_seen
       FROM agg_device_daily WHERE day >= ?1 GROUP BY ${g}`,
    ).bind(sinceDay),
    env.DB.prepare(
      `SELECT ${g}, COUNT(DISTINCT install_id) AS installs
       FROM agg_device_installs WHERE last_seen >= ?1 GROUP BY ${g}`,
    ).bind(sinceDay),
    env.DB.prepare(
      `SELECT ${g}, error_code, SUM(count) AS count
       FROM agg_error_daily WHERE day >= ?1 GROUP BY ${g}, error_code ORDER BY count DESC`,
    ).bind(sinceDay),
  ]);

  const keyOf = (r: Record<string, unknown>) => JSON.stringify(group.map((c) => r[c]));
  const rows = new Map<string, DeviceReportRow>();
  for (const r of (totals?.results ?? []) as Array<Record<string, unknown>>) {
    const row: DeviceReportRow = {
      installs: 0,
      events: Number(r.events ?? 0),
      seen: Number(r.seen ?? 0),
      action_ok: Number(r.action_ok ?? 0),
      action_fail: Number(r.action_fail ?? 0),
      errors: Number(r.errors ?? 0),
      top_errors: [],
      last_seen: (r.last_seen as string | null) ?? null,
    };
    for (const c of group) row[c] = r[c] === "" ? null : r[c];
    rows.set(keyOf(r), row);
  }
  for (const r of (installs?.results ?? []) as Array<Record<string, unknown>>) {
    const row = rows.get(keyOf(r));
    if (row) row.installs = Number(r.installs ?? 0);
  }
  for (const r of (errors?.results ?? []) as Array<Record<string, unknown>>) {
    const row = rows.get(keyOf(r));
    if (row && row.top_errors.length < 5) row.top_errors.push({ code: String(r.error_code), count: Number(r.count) });
  }
  return [...rows.values()].sort((a, b) => b.installs - a.installs || b.events - a.events);
}
